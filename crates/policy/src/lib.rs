//! Sandbox policy: what a sandboxed command may use.
//!
//! Platform-independent. The runtime translates this into `mxc-sdk` requests.
//! Ported from `safe-ng` path and environment rules.

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
    "LC_ALL",
    "NO_BROWSER",
    "NO_PROXY",
    "TERM",
    "TERM_PROGRAM",
    "TZ",
    "VISUAL",
    "XDG_CACHE_HOME",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_RUNTIME_DIR",
    "XDG_STATE_HOME",
    "http_proxy",
    "https_proxy",
    "no_proxy",
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
    /// Whether `--configs` mode is on.
    pub configs: bool,
    /// Extra features enabled on macOS.
    pub enable: Vec<String>,
}

/// Returns true when `path` is `parent` or lives under it.
pub fn path_is_within(path: &str, parent: &str) -> bool {
    let path = path.trim_end_matches('/');
    let parent = parent.trim_end_matches('/');
    path == parent || path.starts_with(&format!("{parent}/"))
}

/// Builds a [`Policy`] from already-discovered host paths.
///
/// `home` is the absolute home directory. `read_only` and `writable` are
/// absolute host paths. Pure: performs no I/O.
pub fn build_policy(
    home: &str,
    read_only: Vec<String>,
    writable: Vec<String>,
    repository_read_only: Vec<String>,
    configs: bool,
    enable: Vec<String>,
) -> Policy {
    let _ = home;
    Policy {
        read_only,
        writable,
        repository_read_only,
        configs,
        enable,
    }
}
