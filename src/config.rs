//! Configuration management for git-pr.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

const PKG_NAME: &str = "git-pr";
const CONFIG_FILE: &str = "config.yaml";

pub const DEFAULT_TEMPLATE: &str = r#"Related PRs:
<!-- RELATED_PR -->
<!-- /RELATED_PR -->

## Summary

## Changes

## Testing
"#;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub jira: JiraConfig,
    pub template: TemplateConfig,
    pub github: GitHubConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct JiraConfig {
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TemplateConfig {
    /// Optional path to a local template file used when the repo template is absent.
    pub path: Option<String>,
    /// Built-in fallback template body used when no file-based template is found.
    pub body: String,
    pub markers: MarkerConfig,
}

impl Default for TemplateConfig {
    fn default() -> Self {
        Self {
            path: None,
            body: DEFAULT_TEMPLATE.to_string(),
            markers: MarkerConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MarkerConfig {
    pub related_pr_start: String,
    pub related_pr_end: String,
}

impl Default for MarkerConfig {
    fn default() -> Self {
        Self {
            related_pr_start: "<!-- RELATED_PR -->".to_string(),
            related_pr_end: "<!-- /RELATED_PR -->".to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GitHubConfig {
    pub user: Option<String>,
    pub default_reviewers: Vec<String>,
}

impl Config {
    pub fn load(config_dir: &str) -> Result<Self> {
        let config_path = PathBuf::from(config_dir).join(CONFIG_FILE);

        let mut config = if config_path.exists() {
            let contents = std::fs::read_to_string(&config_path).map_err(Error::Io)?;
            serde_yaml::from_str(&contents).map_err(|e| Error::Config(e.to_string()))?
        } else {
            Config::default()
        };

        config.apply_env_overrides();
        Ok(config)
    }

    pub fn save(&self, config_dir: &str) -> Result<()> {
        let config_path = PathBuf::from(config_dir).join(CONFIG_FILE);
        let contents = serde_yaml::to_string(self).map_err(|e| Error::Config(e.to_string()))?;
        std::fs::write(&config_path, contents).map_err(Error::Io)?;
        Ok(())
    }

    fn apply_env_overrides(&mut self) {
        if self.jira.url.is_none() {
            if let Ok(url) = std::env::var("JIRA_URL") {
                if !url.is_empty() {
                    self.jira.url = Some(url);
                }
            }
        }

        if self.github.user.is_none() {
            if let Ok(user) = std::env::var("GITHUB_USER") {
                if !user.is_empty() {
                    self.github.user = Some(user);
                }
            }
        }
    }

    pub fn jira_url(&self) -> Option<&str> {
        self.jira.url.as_deref()
    }

    pub fn github_user(&self) -> Option<String> {
        self.github.user.clone()
    }

    pub fn sample_yaml() -> String {
        let config = Config::default();
        serde_yaml::to_string(&config).unwrap_or_else(|_| "# Error generating sample".to_string())
    }
}

pub fn get_tags_path() -> String {
    let path = PathBuf::from(get_config_dir()).join("tags.txt");
    path.to_str()
        .expect("Failed to convert tags path to string")
        .to_string()
}

pub fn get_tags_path_with_dir(config_dir: &str) -> String {
    let path = PathBuf::from(config_dir).join("tags.txt");
    path.to_str()
        .expect("Failed to convert tags path to string")
        .to_string()
}

pub fn get_config_dir() -> String {
    let home = std::env::var("HOME").expect("HOME environment variable not set");
    let path = PathBuf::from(home).join(".config").join(PKG_NAME);

    ensure_config_dir_exists(&path);

    path.to_str()
        .expect("Failed to convert config path to string")
        .to_string()
}

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
        assert!(!config.template.body.is_empty());
        assert!(config.template.path.is_none());
    }

    #[test]
    fn test_default_markers() {
        let config = Config::default();
        assert_eq!(
            config.template.markers.related_pr_start,
            "<!-- RELATED_PR -->"
        );
        assert_eq!(
            config.template.markers.related_pr_end,
            "<!-- /RELATED_PR -->"
        );
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(yaml.contains("jira:"));
        assert!(yaml.contains("template:"));
        assert!(yaml.contains("github:"));
        assert!(yaml.contains("markers:"));
    }

    #[test]
    fn test_config_deserialization() {
        let yaml = r#"
jira:
  url: "https://jira.example.com/browse/"
template:
  path: "./templates/pr.md"
  body: "Custom template"
  markers:
    related_pr_start: "<!-- START -->"
    related_pr_end: "<!-- END -->"
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
        assert_eq!(config.template.path, Some("./templates/pr.md".to_string()));
        assert_eq!(config.template.body, "Custom template");
        assert_eq!(config.template.markers.related_pr_start, "<!-- START -->");
        assert_eq!(config.template.markers.related_pr_end, "<!-- END -->");
        assert_eq!(config.github.user, Some("testuser".to_string()));
        assert_eq!(config.github.default_reviewers.len(), 2);
    }

    #[test]
    fn test_partial_config_deserialization() {
        let yaml = r#"
jira:
  url: "https://jira.example.com/browse/"
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            config.jira.url,
            Some("https://jira.example.com/browse/".to_string())
        );
        assert!(!config.template.body.is_empty());
    }

    #[test]
    fn test_sample_yaml_generation() {
        let sample = Config::sample_yaml();
        assert!(sample.contains("jira:"));
        assert!(sample.contains("template:"));
    }
}
