//! Sandbox policy: what a sandboxed command may use.
//!
//! Platform-independent. The runtime translates [`Policy`] into
//! `mxc_sdk::v1::ContainerRequest`. Ported from `safe-ng`.
//!
//! This crate performs no sandboxing and spawns no subprocesses. Host
//! discovery (git, Homebrew, `PATH` scans) lives in the CLI; filesystem
//! probes here are limited to `is_dir`/`canonicalize` checks.

use std::path::Path;

/// Keeps paths that exist on the host.
///
/// Mirrors `safe-ng` passing only existing grants to safehouse on macOS:
/// the Seatbelt family rejects grants for missing host paths.
pub fn existing_paths(paths: Vec<String>) -> Vec<String> {
    paths
        .into_iter()
        .filter(|path| Path::new(path).exists())
        .collect()
}

/// Home-relative paths granted read-only.
pub const READ_ONLY_HOME_PATHS: &[&str] = &[
    "bin",
    "Library/Application Support/tuicr",
    ".agents",
    ".config/brave-search",
    ".config/git",
    ".config/nvim",
    ".config/revdiff",
    ".config/tea",
    ".gitattributes",
    ".gitconfig",
    ".gitignore",
    ".go",
    ".grok",
    ".local/share/tuicr",
    ".pi",
    ".ssh/config",
    ".ssh/known_hosts",
];

/// Host-absolute paths granted read-only when present.
pub const READ_ONLY_SYSTEM_PATHS: &[&str] = &[
    "/Applications/Ghostty.app",
    "/Applications/Xcode.app",
    "/Library/Developer/PrivateFrameworks",
    "/Library/Preferences",
];

/// Sandbox `gh` config, relative to `$HOME`.
pub const SANDBOX_GH_CONFIG: &str = ".sandbox/.config/gh";

/// Writable home directories granted inside the sandbox.
pub const WRITABLE_HOME_DIRECTORIES: &[&str] = &[
    SANDBOX_GH_CONFIG,
    ".android",
    ".cache",
    ".cargo",
    ".dotfiles.git",
    ".go/pkg/mod",
    ".go/pkg/sumdb",
    ".gradle",
    ".npm",
    ".local/share/mise",
    ".local/share/nvim",
    ".local/share/timber",
    ".local/state/nvim",
    ".rustup",
    ".skiko",
    "Library/Caches",
    "Library/Developer/CoreSimulator",
];

/// Socket-style writable home paths granted inside the sandbox.
pub const WRITABLE_HOME_BINDS: &[&str] = &[".config/herdr/herdr.sock"];

/// Extra writable home directories granted by `--configs`.
pub const CONFIG_HOME_DIRECTORIES: &[&str] = &[".config", ".dotfiles.git"];

/// Environment variables passed through into the sandbox.
pub const PASSTHROUGH_ENVIRONMENT: &[&str] = &[
    "COLORTERM",
    "DISPLAY",
    "EDITOR",
    "GOPATH",
    "GOPRIVATE",
    "GH_CONFIG_DIR",
    "HERDR_ENV",
    "HERDR_PANE_ID",
    "HERDR_SOCKET_PATH",
    "HTTPS_PROXY",
    "HTTP_PROXY",
    "LANG",
    "LC_ADDRESS",
    "LC_ALL",
    "LC_COLLATE",
    "LC_CTYPE",
    "LC_IDENTIFICATION",
    "LC_MEASUREMENT",
    "LC_MESSAGES",
    "LC_MONETARY",
    "LC_NAME",
    "LC_NUMERIC",
    "LC_PAPER",
    "LC_TELEPHONE",
    "LC_TIME",
    "NODE_EXTRA_CA_CERTS",
    "NO_BROWSER",
    "NO_PROXY",
    "PLAYWRIGHT_MCP_SANDBOX",
    "SAFEHOUSE",
    "TERM",
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "TZ",
    "VISUAL",
    "WAYLAND_DISPLAY",
    "XAUTHORITY",
    "XDG_CACHE_HOME",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_RUNTIME_DIR",
    "XDG_STATE_HOME",
    "ZED_ENVIRONMENT",
    "ZED_TERM",
    "ZED_TERMINAL_THREAD_ID",
    "http_proxy",
    "https_proxy",
    "no_proxy",
];

/// Host environment variables that must not leak into the sandbox.
pub const GITHUB_TOKEN_ENVIRONMENT: &[&str] = &[
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "GH_ENTERPRISE_TOKEN",
    "GITHUB_ENTERPRISE_TOKEN",
];

/// What the sandboxed command may use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    /// Host paths the command may read.
    pub read_only: Vec<String>,
    /// Host paths the command may write.
    pub writable: Vec<String>,
    /// Repository paths that stay read-only.
    pub repository_read_only: Vec<String>,
    /// Host paths denied even when covered by a grant.
    pub denied: Vec<String>,
    /// Whether `--configs` mode is on.
    pub configs: bool,
    /// Extra features enabled on macOS.
    pub enable: Vec<String>,
}

/// Joins a home-relative path onto an absolute home directory.
pub fn home_join(home: &str, relative: &str) -> String {
    format!("{home}/{relative}")
}

/// Returns true when `path` is `parent` or lives under it.
pub fn path_is_within(path: &str, parent: &str) -> bool {
    let path = path.trim_end_matches('/');
    let parent = parent.trim_end_matches('/');
    path == parent || path.starts_with(&format!("{parent}/"))
}

/// Deduplicates paths, preserving order and dropping empties.
pub fn unique_paths(paths: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for path in paths {
        if path.is_empty() || !seen.insert(path.clone()) {
            continue;
        }
        result.push(path);
    }
    result
}

/// Drops paths covered by one of `parents`.
pub fn drop_paths_covered_by(paths: &[String], parents: &[String]) -> Vec<String> {
    let parents: Vec<&str> = parents
        .iter()
        .map(|parent| parent.trim_end_matches('/'))
        .filter(|parent| !parent.is_empty())
        .collect();
    paths
        .iter()
        .filter(|path| !parents.iter().any(|parent| path_is_within(path, parent)))
        .cloned()
        .collect()
}

/// Drops paths nested under another path in the same set.
pub fn drop_redundant_nested(paths: Vec<String>) -> Vec<String> {
    let items = unique_paths(paths);
    items
        .iter()
        .filter(|path| {
            !items
                .iter()
                .any(|parent| *path != parent && path_is_within(path, parent))
        })
        .cloned()
        .collect()
}

/// Writable home-relative directories for the given mode.
pub fn writable_home_directories(configs: bool) -> Vec<String> {
    let mut dirs: Vec<String> = WRITABLE_HOME_DIRECTORIES
        .iter()
        .map(|path| (*path).to_string())
        .collect();
    if configs {
        dirs.extend(
            CONFIG_HOME_DIRECTORIES
                .iter()
                .map(|path| (*path).to_string()),
        );
    }
    unique_paths(dirs)
}

/// Host `gh` config paths to deny: `~/.config/gh` plus its symlink target.
pub fn host_gh_config_paths(home: &str) -> Vec<String> {
    let path = home_join(home, ".config/gh");
    let resolved = std::fs::canonicalize(&path)
        .map(|resolved| resolved.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.clone());
    unique_paths(vec![path, resolved])
}

/// Filters environment variables down to the non-empty passthrough set.
pub fn passthrough_environment(
    vars: impl IntoIterator<Item = (String, String)>,
) -> Vec<(String, String)> {
    vars.into_iter()
        .filter(|(name, value)| {
            !name.is_empty()
                && !value.is_empty()
                && PASSTHROUGH_ENVIRONMENT.contains(&name.as_str())
        })
        .collect()
}

/// Builds a [`Policy`] from already-discovered host paths.
///
/// `path_dirs` are the `PATH` entries under `$HOME`, `homebrew_root` is the
/// `brew --prefix` output (empty when absent), and `git_common_dir` is the
/// repo metadata directory (empty outside repos). `repository_read_only`
/// holds the repo root plus metadata for subdirectory launches.
pub fn build_policy(
    home: &str,
    path_dirs: Vec<String>,
    homebrew_root: &str,
    git_common_dir: &str,
    configs: bool,
    enable: Vec<String>,
    repository_read_only: Vec<String>,
) -> Policy {
    let mut writable: Vec<String> = writable_home_directories(configs)
        .into_iter()
        .map(|relative| home_join(home, &relative))
        .collect();
    writable.extend(
        WRITABLE_HOME_BINDS
            .iter()
            .map(|relative| home_join(home, relative)),
    );
    if !git_common_dir.is_empty() {
        writable.push(git_common_dir.to_string());
    }
    let writable = drop_redundant_nested(writable);

    let mut read_only = unique_paths(
        path_dirs
            .into_iter()
            .chain(
                READ_ONLY_HOME_PATHS
                    .iter()
                    .map(|relative| home_join(home, relative)),
            )
            .chain(
                READ_ONLY_SYSTEM_PATHS
                    .iter()
                    .map(|path| (*path).to_string()),
            )
            .chain(homebrew_read_only_paths(homebrew_root).into_iter())
            .chain(repository_read_only.iter().cloned()),
    );
    read_only = drop_paths_covered_by(&read_only, &writable);
    let writable = drop_paths_covered_by(&writable, &repository_read_only);

    Policy {
        read_only,
        writable,
        repository_read_only,
        denied: host_gh_config_paths(home),
        configs,
        enable,
    }
}

/// Read-only Homebrew paths: `<prefix>/bin` and `<prefix>`.
pub fn homebrew_read_only_paths(homebrew_root: &str) -> Vec<String> {
    if homebrew_root.is_empty() {
        return Vec::new();
    }
    vec![format!("{homebrew_root}/bin"), homebrew_root.to_string()]
}
