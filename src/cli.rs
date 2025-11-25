//! Command-line interface argument parsing for git-pr
//!
//! This module defines the CLI arguments using clap with derive macros.

use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::config::get_config_dir;

/// A highly opinionated tool for PR creation with automatic related PR tracking
#[derive(Parser, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Only update related PRs without creating a new PR
    ///
    /// When set, skips the PR creation flow and only updates the "Related PRs"
    /// section of existing PRs that share the same tag.
    #[arg(short, long, default_value_t = false)]
    #[serde(skip)]
    pub update_only: bool,

    /// Perform a dry run without making any changes
    ///
    /// When set, prints the commands that would be executed without actually
    /// creating or updating any PRs.
    #[arg(short, long, default_value_t = false)]
    #[serde(skip)]
    pub dry_run: bool,

    /// Path to the configuration directory
    ///
    /// Defaults to ~/.config/git-pr. Can also be set via the GIT_PR_CONFIG
    /// environment variable.
    #[arg(short, long, env = "GIT_PR_CONFIG", default_value_t = get_config_dir())]
    pub config: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_args() {
        let args = Args::default();
        assert!(!args.update_only);
        assert!(!args.dry_run);
    }

    #[test]
    fn test_args_parsing() {
        let args = Args::parse_from(["git-pr", "--dry-run"]);
        assert!(args.dry_run);
        assert!(!args.update_only);
    }

    #[test]
    fn test_update_only_flag() {
        let args = Args::parse_from(["git-pr", "-u"]);
        assert!(args.update_only);
    }
}
