//! Sandbox runtime: runs a command inside an `mxc-sdk` sandbox.
//!
//! Translates a [`safe_policy::Policy`] into `mxc_sdk::v1::ContainerRequest`
//! and runs it. No `bwrap` or `safehouse` CLIs.
//!
//! Both the Seatbelt (macOS) and Bubblewrap (Linux) backends execute the
//! request command with `sh -c`, so argv is shell-quoted into one line.

use std::io::{IsTerminal, Read, Write};
use std::path::Path;

use mxc_sdk::v1::{self, ClipboardPolicy, Containment, FilesystemPolicy, UiPolicy, WaitResult};

/// Exit code used when the request times out.
pub const TIMEOUT_EXIT_CODE: i32 = 124;

/// Quote one argv word for `sh -c`.
pub fn shell_quote(word: &str) -> String {
    if word.is_empty() {
        return "''".to_string();
    }
    let safe = word
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':'));
    if safe {
        return word.to_string();
    }
    format!("'{}'", word.replace('\'', "'\\''"))
}

/// Join argv into one `sh -c` command line.
pub fn command_line(command: &[String]) -> String {
    command
        .iter()
        .map(|word| shell_quote(word))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Map `--enable` features onto a clipboard policy.
///
/// `clipboard` grants full access; the read/write variants grant one
/// direction. Anything else leaves clipboard denied.
pub fn clipboard_policy(enable: &[String]) -> ClipboardPolicy {
    if enable.iter().any(|name| name == "clipboard") {
        return ClipboardPolicy::All;
    }
    let read = enable.iter().any(|name| name == "clipboard-read");
    let write = enable.iter().any(|name| name == "clipboard-write");
    match (read, write) {
        (true, true) => ClipboardPolicy::All,
        (true, false) => ClipboardPolicy::Read,
        (false, true) => ClipboardPolicy::Write,
        (false, false) => ClipboardPolicy::None,
    }
}

/// Current stdout terminal size in rows and columns, if known.
pub fn terminal_size() -> Option<(u16, u16)> {
    let mut size: libc::winsize = unsafe { std::mem::zeroed() };
    let ok = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut size) } == 0;
    if ok && size.ws_col > 0 && size.ws_row > 0 {
        Some((size.ws_row, size.ws_col))
    } else {
        None
    }
}

/// Builds the container request for `command`.
///
/// The environment is explicit and does not inherit the host default:
/// non-empty passthrough variables, an overridden `GH_CONFIG_DIR`
/// pointing at the sandbox config, and no host GitHub token variables
/// (they are outside the passthrough set). The working directory stays
/// writable while repository and metadata paths stay read-only.
pub fn build_request(
    policy: &safe_policy::Policy,
    command: &[String],
    workdir: &Path,
    home: &str,
) -> v1::ContainerRequest {
    let mut request = v1::ContainerRequest::new(command_line(command));
    request.containment = Containment::Process;
    request.working_directory = Some(workdir.to_string_lossy().into_owned());
    request.container_name = Some("safe-mxc".to_string());

    let mut readwrite = policy.writable.clone();
    readwrite.push(workdir.to_string_lossy().into_owned());
    let readwrite: Vec<String> = readwrite
        .into_iter()
        .filter(|path| !path.is_empty())
        .collect();
    let readonly: Vec<String> = policy
        .read_only
        .iter()
        .chain(policy.repository_read_only.iter())
        .filter(|path| !path.is_empty())
        .cloned()
        .collect();
    // The Seatbelt family rejects grants for missing host paths, so on
    // macOS only existing paths are granted. Linux keeps every grant:
    // the Bubblewrap backend tolerates absent paths like --bind-try.
    #[cfg(target_os = "macos")]
    let (readwrite, readonly) = (
        safe_policy::existing_paths(readwrite),
        safe_policy::existing_paths(readonly),
    );
    request.filesystem = Some(FilesystemPolicy {
        readwrite_paths: readwrite,
        readonly_paths: readonly,
        denied_paths: policy.denied.clone(),
        clear_policy_on_exit: None,
    });

    let mut environment: Vec<(String, String)> = safe_policy::passthrough_environment(
        std::env::vars()
            .filter(|(name, _)| !safe_policy::GITHUB_TOKEN_ENVIRONMENT.contains(&name.as_str())),
    );
    environment.push((
        "GH_CONFIG_DIR".to_string(),
        safe_policy::home_join(home, safe_policy::SANDBOX_GH_CONFIG),
    ));
    environment.push(("HOME".to_string(), home.to_string()));
    request.environment = Some(environment);
    request.inherit_default_environment = Some(false);

    if !policy.enable.is_empty() {
        request.ui = Some(UiPolicy {
            disable: false,
            clipboard: clipboard_policy(&policy.enable),
            allow_input_injection: false,
        });
    }

    request
}

/// Warns about backend policy notes on stderr.
fn report_warnings(warnings: &[String]) {
    for warning in warnings {
        eprintln!("safe-mxc: warning: {warning}");
    }
}

/// Maps the wait outcome onto a process exit code.
fn exit_code(outcome: WaitResult) -> i32 {
    match outcome {
        WaitResult::Exited(code) => {
            if code < 0 {
                eprintln!("safe-mxc: sandboxed command died by signal");
                1
            } else {
                code
            }
        }
        WaitResult::TimedOut => {
            eprintln!("safe-mxc: sandboxed command timed out");
            TIMEOUT_EXIT_CODE
        }
    }
}

/// Copies stdin into the PTY writer until EOF.
fn pump_stdin_to_pty(mut writer: Box<dyn Write + Send>) {
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut buffer = [0u8; 8192];
    loop {
        match input.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                if writer.write_all(&buffer[..n]).is_err() || writer.flush().is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}

/// Runs the request on a caller-controlled PTY.
fn run_with_pty(request: v1::ContainerRequest) -> anyhow::Result<i32> {
    let (rows, cols) = terminal_size().unwrap_or((24, 80));
    let process = v1::spawn_with_pty(
        request,
        v1::SpawnWithPtyOptions {
            size: v1::MxcPtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            },
            ..Default::default()
        },
    )?;

    if let Ok(writer) = process.take_writer() {
        std::thread::spawn(|| pump_stdin_to_pty(writer));
    }
    let mut reader = process.try_clone_reader()?;
    let pump = std::thread::spawn(move || {
        let mut output = std::io::stdout().lock();
        let mut buffer = [0u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    if output.write_all(&buffer[..n]).is_err() || output.flush().is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let outcome = process.wait()?;
    let _ = pump.join();
    report_warnings(&process.warnings());
    Ok(exit_code(outcome))
}

/// Copies a pipe fully in one direction.
fn pump_pipe<R, W>(mut reader: R, mut writer: W)
where
    R: Read + Send + 'static,
    W: Write + Send + 'static,
{
    std::thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    if writer.write_all(&buffer[..n]).is_err() || writer.flush().is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
}

/// Runs the request with piped stdio.
fn run_with_pipes(request: v1::ContainerRequest) -> anyhow::Result<i32> {
    let mut process = v1::spawn(request, Default::default())?;

    if let Some(stdin) = process.take_stdin() {
        let reader: Box<dyn Read + Send> = Box::new(std::io::stdin());
        pump_pipe(reader, stdin);
    }
    if let Some(stdout) = process.take_stdout() {
        pump_pipe(stdout, std::io::stdout());
    }
    if let Some(stderr) = process.take_stderr() {
        pump_pipe(stderr, std::io::stderr());
    }

    let outcome = process.wait()?;
    report_warnings(&process.warnings());
    Ok(exit_code(outcome))
}

/// Runs `command` in a sandbox and returns its exit code.
///
/// Uses a PTY when stdout is a terminal, else piped stdio.
pub fn run(
    policy: &safe_policy::Policy,
    command: &[String],
    workdir: &Path,
    home: &str,
) -> anyhow::Result<i32> {
    if command.is_empty() {
        anyhow::bail!("no command given");
    }
    let request = build_request(policy, command, workdir, home);
    if std::io::stdout().is_terminal() {
        run_with_pty(request)
    } else {
        run_with_pipes(request)
    }
}
