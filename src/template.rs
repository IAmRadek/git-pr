//! PR body template handling.

use regex::Regex;
use std::fs;
use std::path::PathBuf;

use crate::config::Config;
use crate::plugin::{Context, Draft, DraftHook, Forge, PrRef, RemotePr, Ticket};

/// Fills the draft body from the template before the editor opens
pub struct TemplateBody {
    /// The forge's template files, highest priority first
    pub repo_templates: Vec<PathBuf>,
}

impl DraftHook for TemplateBody {
    fn name(&self) -> &'static str {
        "template_body"
    }

    fn before_edit(&self, ctx: &Context, draft: &mut Draft) -> crate::Result<()> {
        draft.body = build_editor_body(&ctx.config, &self.repo_templates, draft.ticket.as_ref());
        Ok(())
    }
}

/// `repo_templates` are the forge's template files, highest priority first
pub fn resolve_body_template(config: &Config, repo_templates: &[PathBuf]) -> String {
    if let Some(path) = repo_templates.first() {
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

pub fn build_editor_body(
    config: &Config,
    repo_templates: &[PathBuf],
    ticket: Option<&Ticket>,
) -> String {
    let template = resolve_body_template(config, repo_templates);
    let mut lines = Vec::new();

    if let Some(ticket) = ticket {
        lines.push(format!("Ticket: {}", ticket.id));

        if let Some(url) = &ticket.url {
            lines.push(format!("Jira: {}", url));
        }
    }

    if !template_contains_related_pr_markers(&template, config) {
        lines.push(String::new());
        lines.push(config.template.markers.related_pr_start.clone());
        lines.push(config.template.markers.related_pr_end.clone());
    }

    lines.push(String::new());
    lines.push(template);
    lines.join("\n")
}

fn template_contains_related_pr_markers(template: &str, config: &Config) -> bool {
    let markers = &config.template.markers;
    template.contains(&markers.related_pr_start) && template.contains(&markers.related_pr_end)
}

pub fn replace_related_prs(
    config: &Config,
    forge: &dyn Forge,
    body: &str,
    this_pr: &PrRef,
    related_prs: &[RemotePr],
) -> String {
    let markers = &config.template.markers;

    let mut related_prs_lines: Vec<String> = vec![markers.related_pr_start.clone()];

    for related in related_prs {
        let link = forge.link(&related.pr);

        if related.pr == *this_pr {
            related_prs_lines.push(format!("- {} - (this pr)", link));
        } else {
            related_prs_lines.push(format!("- {}", link));
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
    use crate::github::GhForge;

    fn remote(repo: &str, number: u32) -> RemotePr {
        RemotePr {
            pr: PrRef {
                repo: repo.to_string(),
                number,
            },
            title: format!("PR {}", number),
            body: String::new(),
            head: String::new(),
            base: String::new(),
        }
    }

    fn ticket(id: &str, url: Option<&str>) -> Ticket {
        Ticket {
            id: id.to_string(),
            url: url.map(str::to_string),
            summary: None,
        }
    }

    #[test]
    fn test_build_editor_body_with_jira() {
        let config = Config::default();
        let ticket = ticket(
            "TRACK-123",
            Some("https://jira.example.com/browse/TRACK-123"),
        );
        let body = build_editor_body(&config, &[], Some(&ticket));

        assert!(body.contains("Ticket: TRACK-123"));
        assert!(body.contains("Jira: https://jira.example.com/browse/TRACK-123"));
        assert!(body.contains("Related PRs:"));
    }

    #[test]
    fn test_build_editor_body_without_jira_url() {
        let config = Config::default();
        let body = build_editor_body(&config, &[], Some(&ticket("TRACK-123", None)));

        assert!(body.contains("Ticket: TRACK-123"));
        assert!(!body.contains("Jira:"));
        assert_eq!(body.matches("<!-- RELATED_PR -->").count(), 1);
        assert_eq!(body.matches("<!-- /RELATED_PR -->").count(), 1);
    }

    #[test]
    fn test_build_editor_body_prepends_markers_when_template_lacks_them() {
        let mut config = Config::default();
        config.template.body = "## Summary\n".to_string();

        let body = build_editor_body(&config, &[], Some(&ticket("TRACK-123", None)));

        assert!(body.contains("Ticket: TRACK-123"));
        assert!(body.contains("<!-- RELATED_PR -->"));
        assert!(body.contains("<!-- /RELATED_PR -->"));
        assert!(body.contains("## Summary"));
    }

    #[test]
    fn test_build_editor_body_avoids_duplicate_markers() {
        let config = Config::default();

        let body = build_editor_body(&config, &[], Some(&ticket("TRACK-123", None)));

        assert_eq!(body.matches("<!-- RELATED_PR -->").count(), 1);
        assert_eq!(body.matches("<!-- /RELATED_PR -->").count(), 1);
    }

    #[test]
    fn test_build_editor_body_without_tag() {
        let config = Config::default();
        let body = build_editor_body(&config, &[], None);

        assert!(!body.contains("Ticket:"));
        assert!(!body.contains("Jira:"));
        assert!(body.contains("Related PRs:"));
    }

    #[test]
    fn test_template_body_fills_draft_body() {
        let mut config = Config::default();
        config.template.body = "## From config\n".to_string();
        let ctx = Context {
            branch: crate::git::BranchInfo {
                current_branch: "feature".into(),
                bases: vec![],
                commits: vec![],
            },
            config,
        };
        let mut draft = Draft {
            ticket: Some(ticket("TRACK-1", None)),
            ..Draft::default()
        };

        TemplateBody {
            repo_templates: vec![],
        }
        .before_edit(&ctx, &mut draft)
        .unwrap();

        assert!(draft.body.starts_with("Ticket: TRACK-1\n"));
        assert!(draft.body.ends_with("## From config\n"));
    }

    #[test]
    fn test_replace_related_prs() {
        let config = Config::default();

        let body = r#"Some text
<!-- RELATED_PR -->
- old/stuff
<!-- /RELATED_PR -->
More text"#;

        let related_prs = vec![remote("owner/repo", 1), remote("owner/repo", 2)];

        let result = replace_related_prs(
            &config,
            &GhForge::default(),
            body,
            &related_prs[0].pr,
            &related_prs,
        );

        assert!(result.contains("owner/repo/pull/1 - (this pr)"));
        assert!(result.contains("- owner/repo/pull/2"));
        assert!(!result.contains("old/stuff"));
    }

    #[test]
    fn test_replace_related_prs_marks_only_same_repo() {
        let config = Config::default();
        let body = "<!-- RELATED_PR -->\n<!-- /RELATED_PR -->";

        let related_prs = vec![remote("owner/api", 5), remote("owner/web", 5)];

        let result = replace_related_prs(
            &config,
            &GhForge::default(),
            body,
            &related_prs[0].pr,
            &related_prs,
        );

        assert!(result.contains("- owner/api/pull/5 - (this pr)"));
        assert!(result.contains("- owner/web/pull/5\n"));
    }

    #[test]
    fn test_resolve_body_template_prefers_repo_template() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut file, b"## From repo\n").unwrap();

        let template = resolve_body_template(&Config::default(), &[file.path().to_path_buf()]);

        assert_eq!(template, "## From repo\n");
    }

    #[test]
    fn test_resolve_body_template_falls_back_to_config_body() {
        let mut config = Config::default();
        config.template.body = "## From config\n".to_string();

        assert_eq!(resolve_body_template(&config, &[]), "## From config\n");
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

        let related_prs = vec![remote("owner/repo", 1)];

        let result = replace_related_prs(
            &config,
            &GhForge::default(),
            body,
            &related_prs[0].pr,
            &related_prs,
        );

        assert!(result.contains("{{RELATED_START}}"));
        assert!(result.contains("{{RELATED_END}}"));
        assert!(result.contains("owner/repo/pull/1 - (this pr)"));
        assert!(!result.contains("old/stuff"));
    }
}
