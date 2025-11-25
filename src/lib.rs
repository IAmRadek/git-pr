//! # git-pr
//!
//! A highly opinionated tool for PR creation with automatic related PR tracking.

pub mod app;
pub mod cli;
pub mod config;
pub mod error;
pub mod git;
pub mod github;
pub mod jira;
pub mod pr;
pub mod tags;
pub mod template;
pub mod ui;

// Re-export commonly used types
pub use config::Config;
pub use error::{Error, Result};
pub use pr::PullRequest;
