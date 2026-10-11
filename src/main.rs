//! `safe-mxc`: run a command in an `mxc-sdk` sandbox.
//!
//! Host discovery (home, git, Homebrew) lives here. The sandbox policy
//! comes from `safe-policy`; execution comes from `safe-runtime`.

use std::path::{Path, PathBuf};

/// Parsed command line.
struct Cli {
    configs: bool,
    enable: Vec<String>,
    command: Vec<String>,
}

fn usage() -> &'static str {
    "usage: safe-mxc [--configs] [--enable FEATURES] ...\n\
     \n\
     Run a command in a sandbox (mxc-sdk). When launched from a Git repo\n\
     subdirectory, the rest of the repo and Git metadata are read-only.\n\
     \n\
     GitHub CLI uses ~/.sandbox/.config/gh; ~/.config/gh is blocked and\n\
     host GitHub token environment variables are removed."
}

fn die(message: &str) -> ! {
    eprintln!("safe-mxc: {message}");
    std::process::exit(1);
}

/// Splits `--enable` values on commas, dropping empties and duplicates.
fn parse_enable(values: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for value in values {
        for feature in value.split(',') {
            let feature = feature.trim();
            if feature.is_empty() || !seen.insert(feature.to_string()) {
                continue;
            }
            result.push(feature.to_string());
        }
    }
    result
}

fn parse_args(raw: &[String]) -> Cli {
    if raw.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("{}", usage());
        std::process::exit(0);
    }
    let mut configs = false;
    let mut enables = Vec::new();
    let mut command = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--configs" => configs = true,
            value if value.starts_with("--enable=") => {
                enables.push(value["--enable=".len()..].to_string());
            }
            "--" => {
                command.extend_from_slice(&raw[i + 1..]);
                break;
            }
            _ => {
                command.extend_from_slice(&raw[i..]);
                break;
            }
        }
        i += 1;
    }
    if command.is_empty() {
        eprintln!("{}", usage());
        std::process::exit(2);
    }
    Cli {
        configs,
        enable: parse_enable(&enables),
        command,
    }
}

/// Runs git and returns trimmed stdout, or empty on failure.
fn git_output(args: &[&str], working_dir: &Path) -> String {
    std::process::Command::new("git")
        .arg("-C")
        .arg(working_dir)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Absolute physical current directory.
fn physical_cwd() -> PathBuf {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| cwd.canonicalize().ok())
        .unwrap_or_else(|| die("unable to resolve the current directory"))
}

/// Repo root plus metadata for subdirectory launches, else empty.
fn repository_read_only_paths(working_dir: &Path) -> Vec<String> {
    let toplevel = git_output(
        &["rev-parse", "--path-format=absolute", "--show-toplevel"],
        working_dir,
    );
    if toplevel.is_empty() || Path::new(&toplevel) == working_dir {
        return Vec::new();
    }
    if !safe_policy::path_is_within(&working_dir.to_string_lossy(), &toplevel) {
        return Vec::new();
    }
    let common = git_output(
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        working_dir,
    );
    let metadata = git_output(
        &["rev-parse", "--path-format=absolute", "--git-dir"],
        working_dir,
    );
    let mut paths = vec![toplevel];
    if !common.is_empty() {
        paths.push(common);
    }
    if !metadata.is_empty() {
        paths.push(metadata);
    }
    safe_policy::unique_paths(paths)
}

/// `brew --prefix`, or empty when brew is absent.
fn brew_prefix() -> String {
    std::process::Command::new("brew")
        .arg("--prefix")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default()
}

/// `PATH` entries that are directories under `$HOME`.
fn home_path_directories(home: &str) -> Vec<String> {
    let prefix = format!("{}/", home.trim_end_matches('/'));
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths)
                .filter_map(|entry| {
                    let text = entry.to_string_lossy().into_owned();
                    if text.starts_with(&prefix) && entry.is_dir() {
                        Some(text)
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Points `EDITOR`/`VISUAL` at Homebrew or `PATH` nvim when available.
fn configure_editor(homebrew_root: &str) {
    if !homebrew_root.is_empty() {
        let nvim = format!("{homebrew_root}/bin/nvim");
        std::env::set_var("EDITOR", &nvim);
        std::env::set_var("VISUAL", &nvim);
        return;
    }
    let found = std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let candidate = dir.join("nvim");
            candidate
                .is_file()
                .then(|| candidate.to_string_lossy().into_owned())
        })
    });
    if let Some(nvim) = found {
        std::env::set_var("EDITOR", &nvim);
        std::env::set_var("VISUAL", &nvim);
    }
}

/// Creates missing directories, dying on failure.
fn ensure_directories(paths: &[String]) {
    for path in paths {
        if Path::new(path).is_dir() {
            continue;
        }
        if std::fs::create_dir_all(path).is_err() {
            die(&format!("unable to create writable directory: {path}"));
        }
    }
}

/// Creates missing directories on a best-effort basis.
///
/// Caches may live under homes where creation is restricted; the sandbox
/// backend tolerates absent grants the way `bwrap --bind-try` does.
fn ensure_directories_best_effort(paths: &[String]) {
    for path in paths {
        if Path::new(path).is_dir() {
            continue;
        }
        let _ = std::fs::create_dir_all(path);
    }
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let cli = parse_args(&raw);

    if std::env::consts::OS != "macos" && std::env::consts::OS != "linux" {
        die(&format!(
            "unsupported operating system: {}",
            std::env::consts::OS
        ));
    }

    let home = std::env::var("HOME").unwrap_or_default();
    if home.is_empty() {
        die("HOME is not set");
    }

    let sandbox_gh = Path::new(&home).join(safe_policy::SANDBOX_GH_CONFIG);
    let host_gh = Path::new(&home).join(".config/gh");
    let sandbox_gh_text = sandbox_gh.to_string_lossy().into_owned();
    for denied in safe_policy::host_gh_config_paths(&home) {
        if safe_policy::path_is_within(&sandbox_gh_text, &denied)
            || safe_policy::path_is_within(&denied, &sandbox_gh_text)
        {
            die("sandbox gh config must be separate from ~/.config/gh");
        }
    }
    let _ = host_gh;

    let brew_missing = std::process::Command::new("brew")
        .arg("--version")
        .output()
        .is_err();
    if std::env::consts::OS == "macos" && brew_missing {
        die("brew is required on macOS");
    }
    let homebrew_root = if brew_missing {
        String::new()
    } else {
        brew_prefix()
    };

    let working_dir = if cli.configs {
        PathBuf::from(&home)
    } else {
        physical_cwd()
    };
    let repository_read_only = if cli.configs {
        Vec::new()
    } else {
        repository_read_only_paths(&working_dir)
    };
    let common_dir = if cli.configs || !repository_read_only.is_empty() {
        String::new()
    } else {
        git_output(
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
            &working_dir,
        )
    };

    let policy = safe_policy::build_policy(
        &home,
        home_path_directories(&home),
        &homebrew_root,
        &common_dir,
        cli.configs,
        cli.enable,
        repository_read_only,
    );

    configure_editor(&homebrew_root);
    ensure_directories_best_effort(&policy.writable);
    ensure_directories(&[sandbox_gh_text]);
    std::env::set_var("SAFEHOUSE", "1");

    if cli.configs && std::env::set_current_dir(&home).is_err() {
        die(&format!("HOME is not an accessible directory: {home}"));
    }

    match safe_runtime::run(&policy, &cli.command, &working_dir, &home) {
        Ok(code) => std::process::exit(code),
        Err(err) => die(&format!("{err:#}")),
    }
}
