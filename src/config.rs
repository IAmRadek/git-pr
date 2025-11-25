//! Configuration management for git-pr
//!
//! This module handles configuration paths and ensures the config directory exists.

use std::path::{Path, PathBuf};

/// The name of the package, used for config directory naming
const PKG_NAME: &str = "git-pr";

/// Get the path to the tags file
///
/// Returns the full path to `~/.config/git-pr/tags.txt`
pub fn get_tags_path() -> String {
    let path = PathBuf::from(get_config_dir()).join("tags.txt");
    path.to_str()
        .expect("Failed to convert tags path to string")
        .to_string()
}

/// Get the configuration directory path
///
/// Returns the path to `~/.config/git-pr/`, creating it if it doesn't exist.
///
/// # Panics
///
/// Panics if the HOME environment variable is not set or if the directory
/// cannot be created.
pub fn get_config_dir() -> String {
    let home = std::env::var("HOME").expect("HOME environment variable not set");
    let path = PathBuf::from(home).join(".config").join(PKG_NAME);

    ensure_config_dir_exists(&path);

    path.to_str()
        .expect("Failed to convert config path to string")
        .to_string()
}

/// Ensure the configuration directory exists, creating it if necessary
///
/// # Arguments
///
/// * `path` - The path to the configuration directory
///
/// # Panics
///
/// Panics if the directory cannot be created.
fn ensure_config_dir_exists(path: &Path) {
    if !path.exists() {
        std::fs::create_dir_all(path).expect("Failed to create config directory");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_config_dir_contains_pkg_name() {
        let config_dir = get_config_dir();
        assert!(config_dir.contains(PKG_NAME));
    }

    #[test]
    fn test_get_tags_path_ends_with_tags_txt() {
        let tags_path = get_tags_path();
        assert!(tags_path.ends_with("tags.txt"));
    }
}
