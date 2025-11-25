//! PR body template handling
//!
//! This module generates and manipulates PR body content using configurable
//! templates and placeholder markers.

use regex::Regex;

use crate::config::Config;
use crate::github::PullRequest;

/// Generate the PR body from the template with the given values
///
/// # Arguments
/// * `config` - The application configuration containing template settings
/// * `jira_ticket` - The Jira ticket identifier (e.g., "TRACK-123")
/// * `is_jira_ticket` - Whether to include the Jira tracking link
/// * `description` - Description of what the PR does
/// * `implementation` - Implementation details and considerations
///
/// # Returns
/// A formatted PR body string with all placeholders replaced
pub fn make_body(
    config: &Config,
    jira_ticket: &str,
    is_jira_ticket: &bool,
    description: &str,
    implementation: &str,
) -> String {
    let jira_url = config.jira_url();
    let placeholders = &config.template.placeholders;

    let mut body = config.template.body.clone();

    if *is_jira_ticket && !jira_url.is_empty() {
        let tracking_link = format!("[{}]({}{})", jira_ticket, jira_url, jira_ticket);
        body = body.replace(&placeholders.issue_url, &tracking_link);
    } else {
        // Remove the tracking line entirely if not a Jira ticket
        let tracking_line_with_newline = format!("{}\n", placeholders.tracking_line_prefix);
        body = body.replace(&tracking_line_with_newline, "");
    }

    body = body.replace(&placeholders.description, description);
    body = body.replace(&placeholders.implementation, implementation);

    body
}

/// Replace the related PRs section in a PR body with updated links
///
/// This function finds the `<!-- RELATED_PR -->...<!-- /RELATED_PR -->` section
/// and replaces it with the current list of related PRs.
///
/// # Arguments
/// * `config` - The application configuration containing placeholder markers
/// * `body` - The current PR body
/// * `this_pr_number` - The PR number of the current PR (marked as "this pr")
/// * `related_prs` - List of all related PRs to include
///
/// # Returns
/// The updated PR body with the new related PRs section
pub fn replace_related_prs(
    config: &Config,
    body: &str,
    this_pr_number: &u32,
    related_prs: &[PullRequest],
) -> String {
    let placeholders = &config.template.placeholders;

    let mut related_prs_lines: Vec<String> = vec![placeholders.related_pr_start.clone()];

    for pr in related_prs {
        // Remove leading slash from resource path
        let resource_path = pr.resource_path.trim_start_matches('/');

        if *this_pr_number == pr.number {
            related_prs_lines.push(format!("- {} - (this pr)", resource_path));
        } else {
            related_prs_lines.push(format!("- {}", resource_path));
        }
    }

    related_prs_lines.push(placeholders.related_pr_end.clone());

    // Build regex pattern from placeholders (escape special regex characters)
    let start_escaped = regex::escape(&placeholders.related_pr_start);
    let end_escaped = regex::escape(&placeholders.related_pr_end);
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

    fn test_config_without_jira() -> Config {
        Config::default()
    }

    #[test]
    fn test_make_body_with_jira() {
        let config = test_config_with_jira();

        let body = make_body(
            &config,
            "TRACK-123",
            &true,
            "Adds a new feature",
            "Used library X",
        );

        assert!(body.contains("[TRACK-123](https://jira.example.com/browse/TRACK-123)"));
        assert!(body.contains("Adds a new feature"));
        assert!(body.contains("Used library X"));
    }

    #[test]
    fn test_make_body_without_jira() {
        let config = test_config_without_jira();

        let body = make_body(&config, "TAG-456", &false, "Bug fix", "Fixed the issue");

        assert!(!body.contains("Tracked by"));
        assert!(body.contains("Bug fix"));
        assert!(body.contains("Fixed the issue"));
    }

    #[test]
    fn test_make_body_with_custom_template() {
        let mut config = test_config_with_jira();
        config.template.body = r#"# PR: <!-- ISSUE_URL -->

## Description
<!-- THIS PR -->

## Notes
<!-- IMPLEMENTATION -->
"#
        .to_string();

        let body = make_body(
            &config,
            "FEAT-999",
            &true,
            "New awesome feature",
            "Some implementation notes",
        );

        assert!(body.contains("# PR: [FEAT-999](https://jira.example.com/browse/FEAT-999)"));
        assert!(body.contains("## Description\nNew awesome feature"));
        assert!(body.contains("## Notes\nSome implementation notes"));
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
        config.template.placeholders.related_pr_start = "{{RELATED_START}}".to_string();
        config.template.placeholders.related_pr_end = "{{RELATED_END}}".to_string();

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
