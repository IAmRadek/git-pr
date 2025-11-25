//! PR body template handling
//!
//! This module generates and manipulates PR body content using configurable
//! templates with dynamic form fields and special markers.

use regex::Regex;
use std::collections::HashMap;

use crate::config::Config;
use crate::github::PullRequest;

/// Generate the PR body from the template with the given field values
///
/// # Arguments
/// * `config` - The application configuration containing template settings
/// * `tag` - The tag/ticket identifier (e.g., "TRACK-123")
/// * `is_jira` - Whether this is a Jira ticket (tag found in commit)
/// * `fields` - Map of field names to their values
///
/// # Returns
/// A formatted PR body string with all placeholders replaced
pub fn make_body(
    config: &Config,
    tag: &str,
    is_jira: bool,
    fields: &HashMap<String, String>,
) -> String {
    let mut body = config.template.body.clone();

    // Replace form field placeholders {{field_name}}
    for field in &config.template.fields {
        let placeholder = format!("{{{{{}}}}}", field.name);
        let value = fields.get(&field.name).map(|s| s.as_str()).unwrap_or("");

        if value.is_empty() {
            // Remove lines containing empty placeholders
            body = remove_placeholder_line(&body, &placeholder);
        } else {
            body = body.replace(&placeholder, value);
        }
    }

    // Also remove any remaining unknown placeholders
    body = remove_empty_placeholders(&body);

    // Prepend Jira tracking link if applicable
    if is_jira {
        if let Some(jira_url) = config.jira_url() {
            let tracking_line = format!("Tracked by [{}]({}{})\n\n", tag, jira_url, tag);
            body = format!("{}{}", tracking_line, body);
        }
    }

    body
}

/// Remove a line containing the given placeholder
fn remove_placeholder_line(body: &str, placeholder: &str) -> String {
    let escaped = regex::escape(placeholder);
    // Match the entire line containing the placeholder (including newline)
    let pattern = format!(r"(?m)^.*{}.*\n?", escaped);
    let re = Regex::new(&pattern).expect("Invalid regex pattern");
    re.replace_all(body, "").to_string()
}

/// Remove any remaining {{...}} placeholders and their lines
fn remove_empty_placeholders(body: &str) -> String {
    let re = Regex::new(r"(?m)^.*\{\{[^}]+\}\}\s*\n?").expect("Invalid regex pattern");
    re.replace_all(body, "").to_string()
}

/// Replace the related PRs section in a PR body with updated links
///
/// This function finds the `<!-- RELATED_PR -->...<!-- /RELATED_PR -->` section
/// and replaces it with the current list of related PRs.
///
/// # Arguments
/// * `config` - The application configuration containing marker settings
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
    let markers = &config.template.markers;

    let mut related_prs_lines: Vec<String> = vec![markers.related_pr_start.clone()];

    for pr in related_prs {
        // Remove leading slash from resource path
        let resource_path = pr.resource_path.trim_start_matches('/');

        if *this_pr_number == pr.number {
            related_prs_lines.push(format!("- {} - (this pr)", resource_path));
        } else {
            related_prs_lines.push(format!("- {}", resource_path));
        }
    }

    related_prs_lines.push(markers.related_pr_end.clone());

    // Build regex pattern from markers (escape special regex characters)
    let start_escaped = regex::escape(&markers.related_pr_start);
    let end_escaped = regex::escape(&markers.related_pr_end);
    let pattern = format!(r"(?sm)^{}(.*){}$", start_escaped, end_escaped);

    let re = Regex::new(&pattern).expect("Invalid regex pattern");

    re.replace(body, related_prs_lines.join("\n")).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{FieldType, FormField};

    fn test_config_with_jira() -> Config {
        let mut config = Config::default();
        config.jira.url = Some("https://jira.example.com/browse/".to_string());
        config
    }

    fn test_config_without_jira() -> Config {
        Config::default()
    }

    fn make_fields(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn test_make_body_with_jira() {
        let config = test_config_with_jira();
        let fields = make_fields(&[
            ("description", "Adds a new feature"),
            ("implementation", "Used library X"),
        ]);

        let body = make_body(&config, "TRACK-123", true, &fields);

        assert!(body.contains("Tracked by [TRACK-123](https://jira.example.com/browse/TRACK-123)"));
        assert!(body.contains("Adds a new feature"));
        assert!(body.contains("Used library X"));
    }

    #[test]
    fn test_make_body_without_jira() {
        let config = test_config_without_jira();
        let fields = make_fields(&[
            ("description", "Bug fix"),
            ("implementation", "Fixed the issue"),
        ]);

        let body = make_body(&config, "TAG-456", false, &fields);

        assert!(!body.contains("Tracked by"));
        assert!(body.contains("Bug fix"));
        assert!(body.contains("Fixed the issue"));
    }

    #[test]
    fn test_make_body_with_jira_flag_but_no_url() {
        let config = test_config_without_jira();
        let fields = make_fields(&[("description", "Some work")]);

        // Even if is_jira is true, no tracking line without URL
        let body = make_body(&config, "JIRA-123", true, &fields);

        assert!(!body.contains("Tracked by"));
    }

    #[test]
    fn test_make_body_removes_empty_fields() {
        let config = test_config_without_jira();
        let fields = make_fields(&[("description", "Has description")]);
        // implementation is empty

        let body = make_body(&config, "TAG", false, &fields);

        assert!(body.contains("Has description"));
        // The implementation line should be removed
        assert!(!body.contains("{{implementation}}"));
        // But the section header might still be there (that's in the template)
    }

    #[test]
    fn test_make_body_with_custom_template() {
        let mut config = test_config_with_jira();
        config.template.body = r#"## Summary
{{summary}}

## Details
{{details}}
"#
        .to_string();
        config.template.fields = vec![
            FormField {
                name: "summary".to_string(),
                prompt: "Summary:".to_string(),
                field_type: FieldType::Text,
                required: true,
                default: None,
            },
            FormField {
                name: "details".to_string(),
                prompt: "Details:".to_string(),
                field_type: FieldType::Editor,
                required: false,
                default: None,
            },
        ];

        let fields = make_fields(&[("summary", "Quick fix"), ("details", "Fixed a bug")]);

        let body = make_body(&config, "FIX-123", true, &fields);

        assert!(body.contains("Tracked by [FIX-123]"));
        assert!(body.contains("## Summary\nQuick fix"));
        assert!(body.contains("## Details\nFixed a bug"));
    }

    #[test]
    fn test_make_body_removes_line_with_empty_optional_field() {
        let mut config = test_config_without_jira();
        config.template.body = r#"## Required
{{required_field}}

## Optional
{{optional_field}}

## Footer
"#
        .to_string();
        config.template.fields = vec![
            FormField {
                name: "required_field".to_string(),
                prompt: "Required:".to_string(),
                field_type: FieldType::Text,
                required: true,
                default: None,
            },
            FormField {
                name: "optional_field".to_string(),
                prompt: "Optional:".to_string(),
                field_type: FieldType::Text,
                required: false,
                default: None,
            },
        ];

        let fields = make_fields(&[("required_field", "I am here")]);

        let body = make_body(&config, "TAG", false, &fields);

        assert!(body.contains("## Required\nI am here"));
        assert!(body.contains("## Optional"));
        assert!(body.contains("## Footer"));
        // The placeholder line should be gone
        assert!(!body.contains("{{optional_field}}"));
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

    #[test]
    fn test_remove_placeholder_line() {
        let body = "Line 1\nLine with {{placeholder}} here\nLine 3\n";
        let result = remove_placeholder_line(body, "{{placeholder}}");
        assert_eq!(result, "Line 1\nLine 3\n");
    }

    #[test]
    fn test_remove_empty_placeholders() {
        let body = "Keep this\n{{unknown}} should go\nAnd this stays\n";
        let result = remove_empty_placeholders(body);
        assert!(result.contains("Keep this"));
        assert!(result.contains("And this stays"));
        assert!(!result.contains("{{unknown}}"));
    }
}
