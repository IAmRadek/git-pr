//! PR body template handling.

use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::github::PullRequest;

pub fn resolve_body_template(config: &Config) -> String {
    if let Some(path) = find_repo_template() {
        if let Ok(contents) = fs::read_to_string(path) {
            return contents;
        }
    }

    if let Some(path) = &config.template.path {
        if let Ok(contents) = fs::read_to_string(path) {
            return contents;
        }
    }

    config.template.body.clone()
}

pub fn build_editor_body(config: &Config, tag: &str, is_jira: bool) -> String {
    let template = resolve_body_template(config);
    let mut lines = vec![
        format!("Ticket: {}", tag),
    ];

    if is_jira {
        if let Some(jira_url) = config.jira_url() {
            lines.push(format!("Jira: {}{}", jira_url, tag));
        }
    }

    lines.push(String::new());
    lines.push(template);
    lines.join("\n")
}

fn find_repo_template() -> Option<PathBuf> {
    let candidates = [
        ".github/PULL_REQUEST_TEMPLATE.md",
        "PULL_REQUEST_TEMPLATE.md",
    ];

    for candidate in candidates {
        let path = Path::new(candidate);
        if path.exists() {
            return Some(path.to_path_buf());
        }
    }

    None
}

pub fn replace_related_prs(
    config: &Config,
    body: &str,
    this_pr_number: &u32,
    related_prs: &[PullRequest],
) -> String {
    let markers = &config.template.markers;

    let mut related_prs_lines: Vec<String> = vec![markers.related_pr_start.clone()];

    for pr in related_prs {
        let resource_path = pr.resource_path.trim_start_matches('/');

        if *this_pr_number == pr.number {
            related_prs_lines.push(format!("- {} - (this pr)", resource_path));
        } else {
            related_prs_lines.push(format!("- {}", resource_path));
        }
    }

    related_prs_lines.push(markers.related_pr_end.clone());

    let start_escaped = regex::escape(&markers.related_pr_start);
    let end_escaped = regex::escape(&markers.related_pr_end);
    let pattern = format!(r"(?sm)^{}(.*){}$", start_escaped, end_escaped);

    let re = Regex::new(&pattern).expect("Invalid regex pattern");

    re.replace(body, related_prs_lines.join("\n")).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config_with_jira() -> Config {
        let mut config = Config::default();
        config.jira.url = Some("https://jira.example.com/browse/".to_string());
        config
    }

    #[test]
    fn test_build_editor_body_with_jira() {
        let config = test_config_with_jira();
        let body = build_editor_body(&config, "TRACK-123", true);

        assert!(body.contains("Ticket: TRACK-123"));
        assert!(body.contains("Jira: https://jira.example.com/browse/TRACK-123"));
        assert!(body.contains("Related PRs:"));
    }

    #[test]
    fn test_build_editor_body_without_jira_url() {
        let config = Config::default();
        let body = build_editor_body(&config, "TRACK-123", true);

        assert!(body.contains("Ticket: TRACK-123"));
        assert!(!body.contains("Jira:"));
    }

    #[test]
    fn test_replace_related_prs() {
        let config = Config::default();

        let body = r#"Some text
<!-- RELATED_PR -->
- old/stuff
<!-- /RELATED_PR -->
More text"#;

        let related_prs = vec![
            PullRequest {
                id: "1".into(),
                title: "PR 1".into(),
                resource_path: "/owner/repo/pull/1".into(),
                number: 1,
                body: String::new(),
            },
            PullRequest {
                id: "2".into(),
                title: "PR 2".into(),
                resource_path: "/owner/repo/pull/2".into(),
                number: 2,
                body: String::new(),
            },
        ];

        let result = replace_related_prs(&config, body, &1, &related_prs);

        assert!(result.contains("owner/repo/pull/1 - (this pr)"));
        assert!(result.contains("- owner/repo/pull/2"));
        assert!(!result.contains("old/stuff"));
    }

    #[test]
    fn test_replace_related_prs_with_custom_markers() {
        let mut config = Config::default();
        config.template.markers.related_pr_start = "{{RELATED_START}}".to_string();
        config.template.markers.related_pr_end = "{{RELATED_END}}".to_string();

        let body = r#"Some text
{{RELATED_START}}
- old/stuff
{{RELATED_END}}
More text"#;

        let related_prs = vec![PullRequest {
            id: "1".into(),
            title: "PR 1".into(),
            resource_path: "/owner/repo/pull/1".into(),
            number: 1,
            body: String::new(),
        }];

        let result = replace_related_prs(&config, body, &1, &related_prs);

        assert!(result.contains("{{RELATED_START}}"));
        assert!(result.contains("{{RELATED_END}}"));
        assert!(result.contains("owner/repo/pull/1 - (this pr)"));
        assert!(!result.contains("old/stuff"));
    }
}
