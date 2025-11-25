use regex::Regex;

use crate::github::PullRequest;

/// Template for PR body with placeholders for dynamic content
pub const TEMPLATE: &str = r#"Tracked by <!-- ISSUE_URL -->
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

/// Generate the PR body from the template with the given values
///
/// # Arguments
/// * `jira_ticket` - The Jira ticket identifier (e.g., "TRACK-123")
/// * `is_jira_ticket` - Whether to include the Jira tracking link
/// * `description` - Description of what the PR does
/// * `implementation` - Implementation details and considerations
///
/// # Returns
/// A formatted PR body string with all placeholders replaced
pub fn make_body(
    jira_ticket: &str,
    is_jira_ticket: &bool,
    description: &str,
    implementation: &str,
) -> String {
    let jira_url = std::env::var("JIRA_URL").unwrap_or_default();

    let mut body = TEMPLATE.to_string();

    if *is_jira_ticket && !jira_url.is_empty() {
        let tracking_link = format!("[{}]({}{})", jira_ticket, jira_url, jira_ticket);
        body = body.replace("<!-- ISSUE_URL -->", &tracking_link);
    } else {
        // Remove the tracking line entirely if not a Jira ticket
        body = body.replace("Tracked by <!-- ISSUE_URL -->\n", "");
    }

    body = body.replace("<!-- THIS PR -->", description);
    body = body.replace("<!-- IMPLEMENTATION -->", implementation);

    body
}

/// Replace the related PRs section in a PR body with updated links
///
/// This function finds the `<!-- RELATED_PR -->...<!-- /RELATED_PR -->` section
/// and replaces it with the current list of related PRs.
///
/// # Arguments
/// * `body` - The current PR body
/// * `this_pr_number` - The PR number of the current PR (marked as "this pr")
/// * `related_prs` - List of all related PRs to include
///
/// # Returns
/// The updated PR body with the new related PRs section
pub fn replace_related_prs(
    body: &str,
    this_pr_number: &u32,
    related_prs: &[PullRequest],
) -> String {
    let mut related_prs_lines: Vec<String> = vec!["<!-- RELATED_PR -->".into()];

    for pr in related_prs {
        // Remove leading slash from resource path
        let resource_path = pr.resource_path.trim_start_matches('/');

        if *this_pr_number == pr.number {
            related_prs_lines.push(format!("- {} - (this pr)", resource_path));
        } else {
            related_prs_lines.push(format!("- {}", resource_path));
        }
    }

    related_prs_lines.push("<!-- /RELATED_PR -->".into());

    let re = Regex::new(r"(?sm)^<!-- RELATED_PR -->(.*)<!-- /RELATED_PR -->")
        .expect("Invalid regex pattern");

    re.replace(body, related_prs_lines.join("\n")).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_body_with_jira() {
        std::env::set_var("JIRA_URL", "https://jira.example.com/browse/");

        let body = make_body("TRACK-123", &true, "Adds a new feature", "Used library X");

        assert!(body.contains("[TRACK-123](https://jira.example.com/browse/TRACK-123)"));
        assert!(body.contains("Adds a new feature"));
        assert!(body.contains("Used library X"));
    }

    #[test]
    fn test_make_body_without_jira() {
        let body = make_body("TAG-456", &false, "Bug fix", "Fixed the issue");

        assert!(!body.contains("Tracked by"));
        assert!(body.contains("Bug fix"));
        assert!(body.contains("Fixed the issue"));
    }

    #[test]
    fn test_replace_related_prs() {
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

        let result = replace_related_prs(body, &1, &related_prs);

        assert!(result.contains("owner/repo/pull/1 - (this pr)"));
        assert!(result.contains("- owner/repo/pull/2"));
        assert!(!result.contains("old/stuff"));
    }
}
