//! Configuration management for git-pr
//!
//! This module handles loading configuration from YAML files and provides
//! default values for all settings including the PR body template.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// The name of the package, used for config directory naming
const PKG_NAME: &str = "git-pr";

/// The configuration file name
const CONFIG_FILE: &str = "config.yaml";

/// Default PR body template with placeholders for dynamic content
pub const DEFAULT_TEMPLATE: &str = r#"Tracked by <!-- ISSUE_URL -->
Related PRs:
<!-- RELATED_PR -->
- [ABCD-XXXX](https://example.com/ABCD-XXXX)
- [ABCD-XXXX](https://example.com/ABCD-XXXX)
<!-- /RELATED_PR -->

## This PR...

<!-- THIS PR -->

## Considerations and implementation

<!-- IMPLEMENTATION -->
"#;

/// Main configuration structure for git-pr
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Jira integration settings
    pub jira: JiraConfig,

    /// PR template settings
    pub template: TemplateConfig,

    /// GitHub settings
    pub github: GitHubConfig,
}

/// Jira integration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct JiraConfig {
    /// Base URL for Jira ticket links (e.g., "https://company.atlassian.net/browse/")
    /// Falls back to JIRA_URL environment variable if not set
    pub url: Option<String>,

    /// Whether to automatically detect Jira tickets from branch/commit names
    pub auto_detect: bool,
}

impl Default for JiraConfig {
    fn default() -> Self {
        Self {
            url: None,
            auto_detect: true,
        }
    }
}

/// PR template configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TemplateConfig {
    /// The PR body template with placeholders:
    /// - `<!-- ISSUE_URL -->` - Replaced with Jira ticket link
    /// - `<!-- RELATED_PR -->...<!-- /RELATED_PR -->` - Related PRs section
    /// - `<!-- THIS PR -->` - PR description
    /// - `<!-- IMPLEMENTATION -->` - Implementation details
    pub body: String,

    /// Placeholder markers (advanced customization)
    pub placeholders: PlaceholderConfig,
}

impl Default for TemplateConfig {
    fn default() -> Self {
        Self {
            body: DEFAULT_TEMPLATE.to_string(),
            placeholders: PlaceholderConfig::default(),
        }
    }
}

/// Placeholder markers used in the template
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PlaceholderConfig {
    /// Marker for the issue/ticket URL
    pub issue_url: String,

    /// Start marker for related PRs section
    pub related_pr_start: String,

    /// End marker for related PRs section
    pub related_pr_end: String,

    /// Marker for the PR description
    pub description: String,

    /// Marker for implementation details
    pub implementation: String,

    /// Line to remove when no Jira ticket is present
    pub tracking_line_prefix: String,
}

impl Default for PlaceholderConfig {
    fn default() -> Self {
        Self {
            issue_url: "<!-- ISSUE_URL -->".to_string(),
            related_pr_start: "<!-- RELATED_PR -->".to_string(),
            related_pr_end: "<!-- /RELATED_PR -->".to_string(),
            description: "<!-- THIS PR -->".to_string(),
            implementation: "<!-- IMPLEMENTATION -->".to_string(),
            tracking_line_prefix: "Tracked by <!-- ISSUE_URL -->".to_string(),
        }
    }
}

/// GitHub-related configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GitHubConfig {
    /// GitHub username (falls back to GITHUB_USER env var)
    pub user: Option<String>,

    /// Default reviewers to suggest
    pub default_reviewers: Vec<String>,
}

impl Config {
    /// Load configuration from a YAML file
    ///
    /// If the file doesn't exist, returns default configuration.
    /// Environment variables can override certain settings.
    pub fn load(config_dir: &str) -> Result<Self> {
        let config_path = PathBuf::from(config_dir).join(CONFIG_FILE);

        let mut config = if config_path.exists() {
            let contents = std::fs::read_to_string(&config_path).map_err(Error::Io)?;
            serde_yaml::from_str(&contents).map_err(|e| Error::Config(e.to_string()))?
        } else {
            Config::default()
        };

        // Apply environment variable overrides
        config.apply_env_overrides();

        Ok(config)
    }

    /// Save configuration to a YAML file
    pub fn save(&self, config_dir: &str) -> Result<()> {
        let config_path = PathBuf::from(config_dir).join(CONFIG_FILE);

        let contents = serde_yaml::to_string(self).map_err(|e| Error::Config(e.to_string()))?;

        std::fs::write(&config_path, contents).map_err(Error::Io)?;

        Ok(())
    }

    /// Apply environment variable overrides to the configuration
    fn apply_env_overrides(&mut self) {
        // JIRA_URL env var overrides config if config value is not set
        if self.jira.url.is_none() {
            if let Ok(url) = std::env::var("JIRA_URL") {
                if !url.is_empty() {
                    self.jira.url = Some(url);
                }
            }
        }

        // GITHUB_USER env var overrides config if config value is not set
        if self.github.user.is_none() {
            if let Ok(user) = std::env::var("GITHUB_USER") {
                if !user.is_empty() {
                    self.github.user = Some(user);
                }
            }
        }
    }

    /// Get the effective Jira URL (from config or empty string)
    pub fn jira_url(&self) -> String {
        self.jira.url.clone().unwrap_or_default()
    }

    /// Get the effective GitHub user
    pub fn github_user(&self) -> Option<String> {
        self.github.user.clone()
    }

    /// Generate a sample configuration file content
    pub fn sample_yaml() -> String {
        let config = Config::default();
        serde_yaml::to_string(&config).unwrap_or_else(|_| "# Error generating sample".to_string())
    }
}

/// Get the path to the tags file
///
/// Returns the full path to `~/.config/git-pr/tags.txt`
pub fn get_tags_path() -> String {
    let path = PathBuf::from(get_config_dir()).join("tags.txt");
    path.to_str()
        .expect("Failed to convert tags path to string")
        .to_string()
}

/// Get the path to the tags file using a custom config directory
pub fn get_tags_path_with_dir(config_dir: &str) -> String {
    let path = PathBuf::from(config_dir).join("tags.txt");
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
pub fn ensure_config_dir_exists(path: &Path) {
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

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.jira.url.is_none());
        assert!(config.jira.auto_detect);
        assert!(!config.template.body.is_empty());
        assert_eq!(config.template.placeholders.issue_url, "<!-- ISSUE_URL -->");
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(yaml.contains("jira:"));
        assert!(yaml.contains("template:"));
        assert!(yaml.contains("github:"));
    }

    #[test]
    fn test_config_deserialization() {
        let yaml = r#"
jira:
  url: "https://jira.example.com/browse/"
  auto_detect: false
template:
  body: "Custom template"
github:
  user: "testuser"
  default_reviewers:
    - "reviewer1"
    - "reviewer2"
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            config.jira.url,
            Some("https://jira.example.com/browse/".to_string())
        );
        assert!(!config.jira.auto_detect);
        assert_eq!(config.template.body, "Custom template");
        assert_eq!(config.github.user, Some("testuser".to_string()));
        assert_eq!(config.github.default_reviewers.len(), 2);
    }

    #[test]
    fn test_partial_config_deserialization() {
        // Test that partial configs work with defaults
        let yaml = r#"
jira:
  url: "https://jira.example.com/browse/"
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            config.jira.url,
            Some("https://jira.example.com/browse/".to_string())
        );
        // Other fields should have defaults
        assert!(config.jira.auto_detect);
        assert!(!config.template.body.is_empty());
    }

    #[test]
    fn test_sample_yaml_generation() {
        let sample = Config::sample_yaml();
        assert!(sample.contains("jira:"));
        assert!(sample.contains("template:"));
    }
}
