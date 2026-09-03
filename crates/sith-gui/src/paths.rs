//! Where the tool keeps what it remembers.
//!
//! Every platform has its own answer and none of them is the others'. Asking
//! for `$HOME/.config` on Windows produces a directory nobody backs up and no
//! installer removes, so the location comes from the platform rather than
//! from a guess.

use std::path::PathBuf;

/// The directory holding this tool's own files.
///
/// `~/.config/sith` on Linux (honouring `XDG_CONFIG_HOME`),
/// `~/Library/Application Support/sith` on macOS, and
/// `%APPDATA%\sith` on Windows.
pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("sith"))
}

/// Custom themes, one file each.
pub fn themes_dir() -> Option<PathBuf> {
    config_dir().map(|d| d.join("themes"))
}

/// The recently opened list.
pub fn recent_file() -> Option<PathBuf> {
    config_dir().map(|d| d.join("recent.json"))
}

/// Key bindings, when they have been customised.
pub fn keys_file() -> Option<PathBuf> {
    config_dir().map(|d| d.join("keys.json"))
}

/// Create a file's directory, and say whether it can be written to.
pub fn ensure_parent(path: &std::path::Path) -> std::io::Result<()> {
    match path.parent() {
        Some(dir) => std::fs::create_dir_all(dir),
        None => Ok(()),
    }
}
