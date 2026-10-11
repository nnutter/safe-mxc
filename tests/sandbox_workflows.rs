//! Blackbox functional tests: user workflows against the built binary.
//!
//! Each test spawns `safe-mxc` as a user runs it and asserts on exit
//! codes, stdout/stderr, and filesystem effects only. No internals.
//!
//! Tests that need a working sandbox skip when the host denies
//! sandboxing (for example Seatbelt returning `Operation not permitted`).
//! The skip probe is itself blackbox: it runs the binary and reads the
//! backend failure out of stderr.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    OnceLock,
};

fn binary() -> String {
    env!("CARGO_BIN_EXE_safe-mxc").to_string()
}

fn output(args: &[&str], cwd: Option<&Path>, envs: &[(&str, &str)]) -> std::process::Output {
    let mut command = Command::new(binary());
    command.args(args);
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    for (name, value) in envs {
        command.env(name, value);
    }
    command.output().expect("run safe-mxc")
}

fn stdout_text(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_text(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Whether this host lets `safe-mxc` sandbox a trivial command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sandbox {
    Available,
    Denied,
    Broken,
}

fn probe_sandbox() -> Sandbox {
    static PROBE: OnceLock<Sandbox> = OnceLock::new();
    *PROBE.get_or_init(|| {
        let out = output(&["true"], None, &[]);
        if out.status.success() {
            return Sandbox::Available;
        }
        let stderr = stderr_text(&out);
        let denied = [
            "Operation not permitted",
            "BackendError",
            "backend_error",
            "BackendUnavailable",
            "backend_unavailable",
            "UnsupportedContainment",
            "unsupported_containment",
        ]
        .iter()
        .any(|marker| stderr.contains(marker));
        if denied {
            Sandbox::Denied
        } else {
            Sandbox::Broken
        }
    })
}

/// Returns false when the caller should skip (no working sandbox here).
/// Panics when the probe itself fails for another reason.
fn require_sandbox(test: &str) -> bool {
    match probe_sandbox() {
        Sandbox::Available => true,
        Sandbox::Denied => {
            eprintln!("skipping {test}: host denies sandboxing");
            false
        }
        Sandbox::Broken => panic!("sandbox probe failed for {test}"),
    }
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Fresh empty directory under the system temp dir.
fn temp_dir(name: &str) -> PathBuf {
    let id = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir =
        std::env::temp_dir().join(format!("safe-mxc-test-{name}-{}-{id}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// `temp/repo` with `outer.txt` and a `sub/` directory, committed with git.
fn git_repo_fixture() -> Option<(PathBuf, PathBuf)> {
    if !git_available() {
        return None;
    }
    let root = temp_dir("repo").join("repo");
    let sub = root.join("sub");
    std::fs::create_dir_all(&sub).expect("create fixture dirs");
    std::fs::write(root.join("outer.txt"), "outer\n").expect("write outer");
    let run = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(&root)
            .output()
            .expect("run git")
    };
    if !run(&["init", "-q"]).status.success() {
        return None;
    }
    run(&["config", "user.email", "test@example.com"]);
    run(&["config", "user.name", "test"]);
    run(&["add", "."]);
    if !run(&["commit", "-qm", "fixture"]).status.success() {
        return None;
    }
    Some((root, sub))
}

#[test]
fn help_exits_zero() {
    let out = output(&["--help"], None, &[]);
    assert!(out.status.success());
    assert!(stdout_text(&out).contains("usage: safe-mxc"));
}

#[test]
fn missing_command_fails_with_usage() {
    let out = output(&[], None, &[]);
    assert!(!out.status.success());
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn runs_command_in_sandbox() {
    if !require_sandbox("runs_command_in_sandbox") {
        return;
    }
    let out = output(&["echo", "hello-from-sandbox"], None, &[]);
    assert!(out.status.success(), "stderr: {}", stderr_text(&out));
    assert!(stdout_text(&out).contains("hello-from-sandbox"));
}

#[test]
fn propagates_command_exit_code() {
    if !require_sandbox("propagates_command_exit_code") {
        return;
    }
    let out = output(&["sh", "-c", "exit 3"], None, &[]);
    assert_eq!(out.status.code(), Some(3), "stderr: {}", stderr_text(&out));
}

#[test]
fn repo_subdirectory_keeps_rest_of_repo_read_only() {
    if !require_sandbox("repo_subdirectory_keeps_rest_of_repo_read_only") {
        return;
    }
    let Some((root, sub)) = git_repo_fixture() else {
        eprintln!("skipping repo_subdirectory test: git unavailable");
        return;
    };
    let _ = root;
    let escape = output(&["sh", "-c", "echo x > ../outer.txt"], Some(&sub), &[]);
    assert!(
        !escape.status.success(),
        "writing outside cwd must fail: {}",
        stderr_text(&escape)
    );
    let inner = output(&["sh", "-c", "echo ok > inner.txt"], Some(&sub), &[]);
    assert!(inner.status.success(), "stderr: {}", stderr_text(&inner));
    assert!(sub.join("inner.txt").is_file());
}

#[test]
fn configs_mode_starts_in_home() {
    if !require_sandbox("configs_mode_starts_in_home") {
        return;
    }
    let home = temp_dir("home");
    let out = output(
        &["--configs", "pwd"],
        None,
        &[("HOME", home.to_str().expect("home is utf-8"))],
    );
    assert!(out.status.success(), "stderr: {}", stderr_text(&out));
    assert_eq!(stdout_text(&out).trim(), home.to_string_lossy().trim());
}

#[test]
fn github_credentials_are_isolated() {
    if !require_sandbox("github_credentials_are_isolated") {
        return;
    }
    let script = "echo \"token=[${GH_TOKEN-unset}]\"; echo \"ghdir=[$GH_CONFIG_DIR]\"";
    let out = output(
        &["sh", "-c", script],
        None,
        &[("GH_TOKEN", "secret-xyz"), ("GITHUB_TOKEN", "secret-xyz")],
    );
    assert!(out.status.success(), "stderr: {}", stderr_text(&out));
    let stdout = stdout_text(&out);
    assert!(stdout.contains("token=[unset]"), "tokens leaked: {stdout}");
    assert!(
        stdout.contains(".sandbox/.config/gh"),
        "sandbox gh config not routed: {stdout}"
    );
}

#[test]
fn host_gh_config_is_blocked() {
    if !require_sandbox("host_gh_config_is_blocked") {
        return;
    }
    let home = std::env::var("HOME").expect("HOME is set");
    let marker = Path::new(&home).join(".config/gh");
    if !marker.exists() {
        eprintln!("skipping host_gh_config_is_blocked: no ~/.config/gh on host");
        return;
    }
    let out = output(
        &[
            "sh",
            "-c",
            "test -r ~/.config/gh && echo readable || echo blocked",
        ],
        None,
        &[],
    );
    assert!(out.status.success(), "stderr: {}", stderr_text(&out));
    assert!(stdout_text(&out).contains("blocked"));
}

#[test]
fn enable_flag_is_accepted() {
    if !require_sandbox("enable_flag_is_accepted") {
        return;
    }
    let out = output(&["--enable=clipboard", "echo", "hi"], None, &[]);
    assert!(out.status.success(), "stderr: {}", stderr_text(&out));
}
