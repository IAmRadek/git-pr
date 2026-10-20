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

/// The top-level sections of `SAMPLE_CONFIG` as (key, text). A section's text
/// starts at the comment lines directly above its key.
pub fn sample_sections() -> Vec<(&'static str, &'static str)> {
    let mut lines: Vec<(usize, &str)> = Vec::new();
    let mut offset = 0;
    for line in SAMPLE_CONFIG.split_inclusive('\n') {
        lines.push((offset, line));
        offset += line.len();
    }

    let mut starts: Vec<(usize, &'static str)> = Vec::new();
    for (i, (_, line)) in lines.iter().enumerate() {
        if !line.starts_with(|c: char| c.is_ascii_lowercase()) {
            continue;
        }
        let Some((key, _)) = line.split_once(':') else {
            continue;
        };

        let mut first = i;
        while first > 0 && lines[first - 1].1.starts_with('#') {
            first -= 1;
        }
        starts.push((lines[first].0, key));
    }

    starts
        .iter()
        .enumerate()
        .map(|(n, (start, key))| {
            let end = starts.get(n + 1).map_or(SAMPLE_CONFIG.len(), |(s, _)| *s);
            (*key, SAMPLE_CONFIG[*start..end].trim_end())
        })
        .collect()
}

/// The sample sections whose key `existing` does not have at the top level
pub fn missing_sections(existing: &str) -> Result<Vec<(&'static str, &'static str)>> {
    let value: serde_yaml::Value =
        serde_yaml::from_str(existing).map_err(|e| Error::Config(e.to_string()))?;

    let present = |key: &str| match &value {
        serde_yaml::Value::Mapping(mapping) => mapping.contains_key(key),
        _ => false,
    };

    if !matches!(
        value,
        serde_yaml::Value::Mapping(_) | serde_yaml::Value::Null
    ) {
        return Err(Error::Config(
            "config.yaml must be a mapping of sections".to_string(),
        ));
    }

    Ok(sample_sections()
        .into_iter()
        .filter(|(key, _)| !present(key))
        .collect())
}

/// `existing` with `sections` appended. The existing text stays as it is.
pub fn append_sections(existing: &str, sections: &[(&str, &str)]) -> String {
    let mut text = existing.trim_end().to_string();
    for (_, section) in sections {
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        text.push_str(section);
    }
    text.push('\n');
    text
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
    fn test_sample_sections_cover_every_config_section() {
        let keys: Vec<&str> = sample_sections().iter().map(|(key, _)| *key).collect();

        assert_eq!(
            keys,
            vec!["jira", "github", "forges", "plugins", "template"]
        );
        for (key, text) in sample_sections() {
            assert!(text.starts_with("# "), "{} has no comment", key);
            assert!(text.contains(&format!("\n{}:", key)), "{} has no key", key);
        }
    }

    #[test]
    fn test_missing_sections_of_old_config() {
        let old = "jira:\n  url: \"u\"\ngithub:\n  user: \"me\"\n";

        let keys: Vec<&str> = missing_sections(old)
            .unwrap()
            .iter()
            .map(|(key, _)| *key)
            .collect();

        assert_eq!(keys, vec!["forges", "plugins", "template"]);
    }

    #[test]
    fn test_missing_sections_of_empty_file() {
        assert_eq!(missing_sections("").unwrap().len(), sample_sections().len());
    }

    #[test]
    fn test_append_sections_keeps_existing_text() {
        let old = "# my settings\njira:\n  url: \"u\"\n";

        let migrated = append_sections(old, &missing_sections(old).unwrap());
        let config: Config = serde_yaml::from_str(&migrated).unwrap();

        assert!(migrated.starts_with(old));
        assert_eq!(config.jira.url.as_deref(), Some("u"));
        assert!(missing_sections(&migrated).unwrap().is_empty());
    }
}
