//! `safe-mxc`: run a command in an `mxc-sdk` sandbox.

use std::path::PathBuf;

fn usage() -> &'static str {
    "usage: safe-mxc [--configs] [--enable FEATURES] ...\n\nRun a command in a sandbox (mxc-sdk). When launched from a Git repo subdirectory, the rest of the repo and Git metadata are read-only."
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if raw.iter().any(|a| a == "-h" || a == "--help") {
        println!("{}", usage());
        std::process::exit(0);
    }
    let mut configs = false;
    let mut enable: Vec<String> = Vec::new();
    let mut command: Vec<String> = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--configs" => configs = true,
            s if s.starts_with("--enable=") => {
                enable.extend(
                    s["--enable=".len()..]
                        .split(',')
                        .filter(|v| !v.is_empty())
                        .map(str::to_string),
                );
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
    let home = std::env::var("HOME").unwrap_or_default();
    let policy =
        safe_policy::build_policy(&home, Vec::new(), Vec::new(), Vec::new(), configs, enable);
    let workdir = PathBuf::from(".");
    match safe_runtime::run(&policy, &command, &workdir) {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("safe-mxc: {err:#}");
            std::process::exit(1);
        }
    }
}
