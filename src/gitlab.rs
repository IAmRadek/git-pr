//! The GitLab forge and reviewer source, through the `glab` CLI.
//!
//! glab chooses the GitLab host from the git remote of the current directory,
//! so a self-hosted GitLab needs only a `forges` entry and `glab auth login`.

use std::path::PathBuf;
use std::process::Command;

use serde::Deserialize;

use crate::error::Error;
use crate::plugin::{Context, Draft, Forge, PrRef, RemotePr, ReviewerSource};

const MR_TEMPLATES_DIR: &str = ".gitlab/merge_request_templates";

/// A merge request as the GitLab REST API returns it, reduced to the fields we read
#[derive(Deserialize, Debug)]
struct MergeRequest {
    iid: u32,
    title: String,
    #[serde(default)]
    description: Option<String>,
    source_branch: String,
    target_branch: String,
    web_url: String,
}

impl TryFrom<MergeRequest> for RemotePr {
    type Error = String;

    fn try_from(mr: MergeRequest) -> Result<Self, String> {
        let (repo, _) = parse_mr_url(&mr.web_url)
            .ok_or_else(|| format!("Unexpected merge request URL: {}", mr.web_url))?;

        Ok(RemotePr {
            pr: PrRef {
                repo,
                number: mr.iid,
            },
            title: mr.title,
            body: mr.description.unwrap_or_default(),
            head: mr.source_branch,
            base: mr.target_branch,
        })
    }
}

#[derive(Deserialize)]
struct Member {
    username: String,
}

/// "https://host/group/sub/project/-/merge_requests/12" -> ("group/sub/project", 12)
fn parse_mr_url(url: &str) -> Option<(String, u32)> {
    let url = url.trim();
    let (project_url, rest) = url.split_once("/-/merge_requests/")?;
    let number = rest.split(['/', '#', '?']).next()?.parse().ok()?;

    let without_scheme = project_url
        .split_once("://")
        .map_or(project_url, |(_, r)| r);
    let (_host, project) = without_scheme.split_once('/')?;
    if project.is_empty() {
        return None;
    }

    Some((project.to_string(), number))
}

/// The first merge request URL in glab output
fn find_mr_url(output: &str) -> Option<&str> {
    output
        .split_whitespace()
        .find(|word| word.contains("/-/merge_requests/"))
}

/// The GitLab forge, through the `glab` CLI
#[derive(Default)]
pub struct GlForge {
    /// The user whose open MRs are related-PR candidates.
    /// `None` means the authenticated user.
    pub user: Option<String>,
}

impl Forge for GlForge {
    fn create(&self, draft: &Draft, dry_run: bool) -> crate::Result<Option<PrRef>> {
        let mut args: Vec<String> = vec![
            "mr".into(),
            "create".into(),
            "--target-branch".into(),
            draft.base.clone(),
            "--title".into(),
            draft.title.clone(),
            "--description".into(),
            draft.body.clone(),
            "--yes".into(),
        ];
        if !draft.reviewers.is_empty() {
            args.push("--reviewer".into());
            args.push(draft.reviewers.join(","));
        }
        if !draft.labels.is_empty() {
            args.push("--label".into());
            args.push(draft.labels.join(","));
        }

        if dry_run {
            println!("glab {:?}", args);
            return Ok(None);
        }

        let username = current_username().map_err(Error::GitLabCli)?;
        args.push("--assignee".into());
        args.push(username);

        let output = glab(&args).map_err(Error::GitLabCli)?;
        let url = find_mr_url(&output).ok_or_else(|| {
            Error::GitLabCli(format!("No merge request URL in glab output: {}", output))
        })?;

        println!("Published at: {}", url);
        Ok(parse_mr_url(url).map(|(repo, number)| PrRef { repo, number }))
    }

    fn get(&self, pr: &PrRef) -> crate::Result<RemotePr> {
        let output = glab(&[
            "mr",
            "view",
            &pr.number.to_string(),
            "--repo",
            &pr.repo,
            "--output",
            "json",
        ])
        .map_err(Error::GitLabCli)?;

        parse_merge_request(&output).map_err(Error::GitLabCli)
    }

    fn current_pr(&self) -> crate::Result<Option<RemotePr>> {
        // glab exits with an error when the branch has no merge request
        let Ok(output) = glab(&["mr", "view", "--output", "json"]) else {
            return Ok(None);
        };

        parse_merge_request(&output)
            .map(Some)
            .map_err(Error::GitLabCli)
    }

    fn open_prs(&self) -> crate::Result<Vec<RemotePr>> {
        let endpoint = match self.user.as_deref() {
            Some(user) if !user.is_empty() => format!(
                "merge_requests?state=opened&scope=all&author_username={}&per_page=50",
                user
            ),
            _ => "merge_requests?state=opened&scope=created_by_me&per_page=50".to_string(),
        };

        let output = glab(&["api", &endpoint]).map_err(Error::GitLabCli)?;
        let mrs: Vec<MergeRequest> = serde_json::from_str(&output)
            .map_err(|e| Error::GitLabCli(format!("Failed to parse merge requests: {}", e)))?;

        mrs.into_iter()
            .map(|mr| RemotePr::try_from(mr).map_err(Error::GitLabCli))
            .collect()
    }

    fn update_body(&self, pr: &PrRef, body: &str, dry_run: bool) -> crate::Result<()> {
        let number = pr.number.to_string();
        let args = [
            "mr",
            "update",
            &number,
            "--repo",
            &pr.repo,
            "--description",
            body,
            "--yes",
        ];

        if dry_run {
            println!("glab {:?}", args);
            return Ok(());
        }

        glab(&args).map(|_| ()).map_err(Error::GitLabCli)
    }

    /// `Default.md` first, then the other templates by name
    fn templates(&self) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(MR_TEMPLATES_DIR) else {
            return vec![];
        };

        let mut templates: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
            .collect();
        templates.sort_by_key(|path| {
            let is_default = path.file_stem().is_some_and(|stem| stem == "Default");
            (!is_default, path.clone())
        });
        templates
    }

    /// The GitLab reference "group/project!12", which GitLab renders as a link
    fn link(&self, pr: &PrRef) -> String {
        format!("{}!{}", pr.repo, pr.number)
    }
}

/// Suggests the members of the current GitLab project
pub struct GlReviewers;

impl ReviewerSource for GlReviewers {
    fn name(&self) -> &'static str {
        "gitlab_reviewers"
    }

    fn reviewers(&self, _ctx: &Context, _draft: &Draft) -> crate::Result<Vec<String>> {
        let output =
            glab(&["api", "projects/:id/members/all?per_page=100"]).map_err(Error::GitLabCli)?;
        let members: Vec<Member> = serde_json::from_str(&output)
            .map_err(|e| Error::GitLabCli(format!("Failed to parse members: {}", e)))?;

        Ok(members.into_iter().map(|m| m.username).collect())
    }
}

fn parse_merge_request(json: &str) -> Result<RemotePr, String> {
    let mr: MergeRequest =
        serde_json::from_str(json).map_err(|e| format!("Failed to parse merge request: {}", e))?;
    RemotePr::try_from(mr)
}

fn current_username() -> Result<String, String> {
    let output = glab(&["api", "user"])?;
    let user: Member = serde_json::from_str(&output)
        .map_err(|e| format!("Failed to parse authenticated GitLab user: {}", e))?;
    Ok(user.username)
}

/// Runs glab and returns its stdout
fn glab<S: AsRef<std::ffi::OsStr>>(args: &[S]) -> Result<String, String> {
    let output = Command::new("glab")
        .args(args)
        .output()
        .map_err(|e| format!("Failed to execute glab command: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("glab error: {}", stderr.trim()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mr_url() {
        assert_eq!(
            parse_mr_url("https://gitlab.com/group/project/-/merge_requests/12"),
            Some(("group/project".to_string(), 12))
        );
        assert_eq!(
            parse_mr_url("https://git.example.com/group/sub/project/-/merge_requests/7/diffs\n"),
            Some(("group/sub/project".to_string(), 7))
        );
        assert_eq!(
            parse_mr_url("https://gitlab.com/group/project/-/issues/12"),
            None
        );
        assert_eq!(
            parse_mr_url("https://gitlab.com/group/project/-/merge_requests/abc"),
            None
        );
    }

    #[test]
    fn test_find_mr_url_in_create_output() {
        let output = "\nCreating merge request for feature into main in group/project\n\n!12 Title (feature)\n https://gitlab.com/group/project/-/merge_requests/12\n";

        assert_eq!(
            find_mr_url(output),
            Some("https://gitlab.com/group/project/-/merge_requests/12")
        );
    }

    #[test]
    fn test_parse_merge_request_json() {
        let json = r#"{
            "id": 99, "iid": 12, "title": "[TRACK-1]: A",
            "description": null,
            "source_branch": "feature", "target_branch": "main",
            "web_url": "https://gitlab.com/group/project/-/merge_requests/12",
            "references": {"full": "group/project!12"}
        }"#;

        let remote = parse_merge_request(json).unwrap();

        assert_eq!(
            remote.pr,
            PrRef {
                repo: "group/project".into(),
                number: 12
            }
        );
        assert_eq!(remote.body, "");
        assert_eq!(remote.head, "feature");
        assert_eq!(remote.base, "main");
    }

    #[test]
    fn test_link_is_gitlab_reference() {
        let pr = PrRef {
            repo: "group/project".into(),
            number: 12,
        };

        assert_eq!(GlForge::default().link(&pr), "group/project!12");
    }
}
