//! Blackbox functional tests: user workflows against the built binary.
//!
//! Each test spawns `safe-mxc` as a user runs it. No internals.

use std::process::Command;

fn binary() -> String {
    env!("CARGO_BIN_EXE_safe-mxc").to_string()
}

#[test]
fn help_exits_zero() {
    let out = Command::new(binary())
        .arg("--help")
        .output()
        .expect("run safe-mxc --help");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("usage: safe-mxc"));
}

#[test]
fn runs_command_in_sandbox() {
    let out = Command::new(binary())
        .args(["echo", "hello-from-sandbox"])
        .output()
        .expect("run safe-mxc echo");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("hello-from-sandbox"));
}

#[test]
fn propagates_command_exit_code() {
    let out = Command::new(binary())
        .args(["sh", "-c", "exit 3"])
        .output()
        .expect("run safe-mxc sh");
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn missing_command_fails_with_usage() {
    let out = Command::new(binary())
        .output()
        .expect("run safe-mxc without command");
    assert!(!out.status.success());
}
