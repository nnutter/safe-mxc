//! Sandbox runtime: runs a command inside an `mxc-sdk` sandbox.
//!
//! Translates a [`safe_policy::Policy`] into `mxc_sdk::v1::ContainerRequest`
//! and runs it. No `bwrap` or `safehouse` CLIs.

use std::path::Path;

/// Runs `command` in a sandbox and returns its exit code.
///
/// Uses `spawn_with_pty` for interactive terminals, else `spawn`.
/// Placeholder: runs without sandbox until the MXC translation lands.
pub fn run(
    _policy: &safe_policy::Policy,
    command: &[String],
    _workdir: &Path,
) -> anyhow::Result<i32> {
    if command.is_empty() {
        anyhow::bail!("no command given");
    }
    // Placeholder path so the workspace builds before the MXC work lands.
    // The runtime specialist replaces this with ContainerRequest + spawn.
    let status = std::process::Command::new(&command[0])
        .args(&command[1..])
        .status()?;
    Ok(status.code().unwrap_or(1))
}
