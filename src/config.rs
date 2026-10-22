//! Configuration management for git-pr.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    /// Forge for each origin host, e.g. `git.example.com: github`.
    /// Hosts in `DEFAULT_FORGES` need no entry.
    pub forges: HashMap<String, ForgeKind>,
    pub plugins: PluginsConfig,
}

/// Plugin names per kind, in run order. A missing list means the built-in
/// defaults; an empty list turns that kind off.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticket_sources: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft_hooks: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_create: Option<Vec<String>>,
}

/// The forges that git-pr can talk to
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForgeKind {
    Github,
}

const DEFAULT_FORGES: &[(&str, ForgeKind)] = &[("github.com", ForgeKind::Github)];

pub const DEFAULT_JQL: &str =
    "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct JiraConfig {
    /// The ticket link prefix, ending in `/browse/`. The `jira` plugin also
    /// derives the API address from it.
    pub url: Option<String>,
    /// Chooses the authentication and the search endpoint of the `jira` plugin
    pub kind: JiraKind,
    /// The query for the tickets that the `jira` plugin suggests
    pub jql: String,
}

impl Default for JiraConfig {
    fn default() -> Self {
        Self {
            url: None,
            kind: JiraKind::default(),
            jql: DEFAULT_JQL.to_string(),
        }
    }
}

/// Jira Cloud (atlassian.net) or a self-hosted Data Center or Server
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JiraKind {
    #[default]
    Cloud,
    Datacenter,
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

    /// The forge for `host`: the `forges` entry first, then `DEFAULT_FORGES`
    pub fn forge_kind(&self, host: &str) -> Option<ForgeKind> {
        self.forges.get(host).copied().or_else(|| {
            DEFAULT_FORGES
                .iter()
                .find(|(default_host, _)| *default_host == host)
                .map(|(_, kind)| *kind)
        })
    }
}

/// The commented sample that `--init` writes
pub const SAMPLE_CONFIG: &str = include_str!("../git-pr.yaml");

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
        let tags_path = get_tags_path_with_dir("/tmp/git-pr");
        assert_eq!(tags_path, "/tmp/git-pr/tags.txt");
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
  # Removed setting; old configs must still load
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
    fn test_forge_kind_default_host() {
        let config = Config::default();

        assert_eq!(config.forge_kind("github.com"), Some(ForgeKind::Github));
        assert_eq!(config.forge_kind("gitlab.example.com"), None);
    }

    #[test]
    fn test_forge_kind_from_config() {
        let yaml = r#"
forges:
  git.example.com: github
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(
            config.forge_kind("git.example.com"),
            Some(ForgeKind::Github)
        );
        assert_eq!(config.forge_kind("github.com"), Some(ForgeKind::Github));
    }

    #[test]
    fn test_sample_config_matches_defaults() {
        let config: Config = serde_yaml::from_str(SAMPLE_CONFIG).unwrap();
        let defaults = Config::default();

        assert_eq!(config.jira.url, defaults.jira.url);
        assert_eq!(config.jira.kind, defaults.jira.kind);
        assert_eq!(config.jira.jql, DEFAULT_JQL);
        assert_eq!(config.github.user, defaults.github.user);
        assert!(config.forges.is_empty());
        assert!(config.plugins.ticket_sources.is_none());
        assert!(config.plugins.draft_hooks.is_none());
        assert!(config.plugins.after_create.is_none());
        assert_eq!(config.template.path, defaults.template.path);
        assert_eq!(config.template.body, DEFAULT_TEMPLATE);
        assert_eq!(
            config.template.markers.related_pr_start,
            defaults.template.markers.related_pr_start
        );
        assert_eq!(
            config.template.markers.related_pr_end,
            defaults.template.markers.related_pr_end
        );
    }

    #[test]
    fn test_sample_config_has_every_section() {
        let defaults = serde_yaml::to_value(Config::default()).unwrap();
        let serde_yaml::Value::Mapping(sections) = defaults else {
            panic!("Config does not serialize to a mapping");
        };

        for key in sections.keys() {
            let key = key.as_str().unwrap();
            assert!(
                SAMPLE_CONFIG.contains(&format!("\n{}:", key)),
                "the sample has no `{}` section",
                key
            );
        }
    }
}
