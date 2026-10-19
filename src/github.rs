use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::plugin::{Draft, Forge, PrRef, RemotePr};

// GraphQL query to get assignable users (potential reviewers) for a repository
const REVIEWERS_QUERY: &str = r#"query ($repo: String!, $owner: String!) {
  repository(name: $repo, owner: $owner) {
    assignableUsers(first: 100) {
      nodes {
        login
      }
      pageInfo {
        hasNextPage
        endCursor
      }
    }
  }
}"#;

// GraphQL query to get pull requests for a user
const RELATED_PR_QUERY: &str = r#"query ($login: String!) {
  user(login: $login) {
    pullRequests(last: 50, states: [OPEN]) {
      edges {
        node {
          id
          title
          resourcePath
          number
          body
          headRefName
          baseRefName
        }
      }
    }
  }
}"#;

// Response types for GraphQL queries

#[derive(Serialize, Deserialize)]
struct Login {
    login: String,
}

#[derive(Serialize, Deserialize)]
struct Nodes {
    nodes: Vec<Login>,
}

#[derive(Serialize, Deserialize)]
struct AssignableUsers {
    #[serde(alias = "assignableUsers")]
    assignable_users: Nodes,
}

#[derive(Serialize, Deserialize)]
struct Repository {
    repository: AssignableUsers,
}

/// Represents a GitHub Pull Request
#[derive(Serialize, Deserialize, Debug, Clone)]
struct PullRequest {
    /// The node ID of the PR
    id: String,
    /// The title of the PR
    title: String,
    /// The resource path (e.g., /owner/repo/pull/123)
    #[serde(alias = "resourcePath")]
    resource_path: String,
    /// The PR number
    number: u32,
    /// The body/description of the PR
    body: String,
    /// The head (source) branch name
    #[serde(alias = "headRefName", default)]
    head_ref_name: String,
    /// The base (target) branch name
    #[serde(alias = "baseRefName", default)]
    base_ref_name: String,
}

impl From<PullRequest> for RemotePr {
    fn from(pr: PullRequest) -> Self {
        RemotePr {
            pr: PrRef {
                repo: repo_from_resource_path(&pr.resource_path),
                number: pr.number,
            },
            title: pr.title,
            body: pr.body,
            head: pr.head_ref_name,
            base: pr.base_ref_name,
        }
    }
}

/// "/owner/repo/pull/123" -> "owner/repo"
fn repo_from_resource_path(resource_path: &str) -> String {
    resource_path
        .trim_start_matches('/')
        .splitn(3, '/')
        .take(2)
        .collect::<Vec<_>>()
        .join("/")
}

/// The GitHub forge, through the `gh` CLI
pub struct GhForge;

impl Forge for GhForge {
    fn create(&self, draft: &Draft, dry_run: bool) -> crate::Result<Option<PrRef>> {
        let url = publish_pr(
            draft.base.clone(),
            draft.title.clone(),
            draft.body.clone(),
            draft.reviewers.clone(),
            dry_run,
        )
        .map_err(Error::GitHubCli)?;

        if dry_run {
            return Ok(None);
        }

        println!("Published at: {}", url);
        Ok(parse_pr_url(&url).map(|(number, resource_path)| PrRef {
            repo: repo_from_resource_path(&resource_path),
            number,
        }))
    }

    fn get(&self, pr: &PrRef) -> crate::Result<RemotePr> {
        get_pr_by_number(&pr.repo, pr.number)
            .map(RemotePr::from)
            .map_err(Error::GitHubCli)
    }

    fn current_pr(&self) -> crate::Result<Option<RemotePr>> {
        get_current_branch_pr()
            .map(|pr| pr.map(RemotePr::from))
            .map_err(Error::GitHubCli)
    }

    fn open_prs_by(&self, user: Option<&str>) -> crate::Result<Vec<RemotePr>> {
        get_user_prs(user)
            .map(|prs| prs.into_iter().map(RemotePr::from).collect())
            .map_err(Error::GitHubCli)
    }

    fn update_body(&self, pr: &PrRef, body: &str, dry_run: bool) -> crate::Result<()> {
        update_pr(&pr.repo, pr.number, body, dry_run)
            .map(|_| ())
            .map_err(Error::GitHubCli)
    }

    fn templates(&self) -> Vec<PathBuf> {
        [
            ".github/PULL_REQUEST_TEMPLATE.md",
            "PULL_REQUEST_TEMPLATE.md",
        ]
        .iter()
        .map(Path::new)
        .filter(|path| path.exists())
        .map(Path::to_path_buf)
        .collect()
    }
}

#[derive(Serialize, Deserialize)]
struct PullRequestNode {
    node: PullRequest,
}

#[derive(Serialize, Deserialize)]
struct Edges {
    edges: Vec<PullRequestNode>,
}

#[derive(Serialize, Deserialize)]
struct PullRequests {
    #[serde(alias = "pullRequests")]
    pull_requests: Edges,
}

#[derive(Serialize, Deserialize)]
struct User {
    user: PullRequests,
}

#[derive(Serialize, Deserialize)]
struct Response<D> {
    data: D,
}

#[derive(Deserialize)]
struct ViewerLogin {
    login: String,
}

/// Get the list of available reviewers for the current repository
///
/// Uses the GitHub CLI to query the GraphQL API for assignable users.
/// Returns an empty list if the query fails.
pub fn get_available_reviewers() -> Result<Vec<String>, String> {
    let output = Command::new("gh")
        .args([
            "api",
            "graphql",
            "-F",
            "owner=:owner",
            "-F",
            "repo=:repo",
            "-f",
            &format!("query={}", REVIEWERS_QUERY),
        ])
        .output()
        .map_err(|e| format!("Failed to execute gh command: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("GitHub CLI error: {}", stderr));
    }

    let response: Response<Repository> =
        serde_json::from_slice(&output.stdout).unwrap_or_else(|_| Response {
            data: Repository {
                repository: AssignableUsers {
                    assignable_users: Nodes { nodes: vec![] },
                },
            },
        });

    let logins = response
        .data
        .repository
        .assignable_users
        .nodes
        .into_iter()
        .map(|node| node.login)
        .collect();

    Ok(logins)
}

/// Get the recent pull requests for the current user
///
/// # Arguments
/// * `github_user` - The GitHub username to query PRs for. Falls back to GITHUB_USER env var if None.
fn get_user_prs(github_user: Option<&str>) -> Result<Vec<PullRequest>, String> {
    let login = match github_user {
        Some(user) if !user.is_empty() => user.to_string(),
        _ => current_user_login()?,
    };

    let output = Command::new("gh")
        .args([
            "api",
            "graphql",
            "-F",
            &format!("login={}", login),
            "-f",
            &format!("query={}", RELATED_PR_QUERY),
        ])
        .output()
        .map_err(|e| format!("Failed to execute gh command: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("GitHub CLI error: {}", stderr));
    }

    let response: Response<User> = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let prs = response
        .data
        .user
        .pull_requests
        .edges
        .into_iter()
        .map(|edge| edge.node)
        .collect();

    Ok(prs)
}

fn current_user_login() -> Result<String, String> {
    if let Ok(user) = std::env::var("GITHUB_USER") {
        if !user.is_empty() {
            return Ok(user);
        }
    }

    let output = Command::new("gh")
        .args(["api", "user"])
        .output()
        .map_err(|e| format!("Failed to execute gh command: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Failed to determine authenticated GitHub user: {}",
            stderr.trim()
        ));
    }

    let user: ViewerLogin = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse authenticated GitHub user: {}", e))?;

    if user.login.is_empty() {
        return Err("Authenticated GitHub user login is empty".to_string());
    }

    Ok(user.login)
}

/// Parse a PR URL to extract the PR number and resource path
///
/// # Arguments
/// * `url` - The PR URL (e.g., "https://github.com/owner/repo/pull/123")
///
/// # Returns
/// A tuple of (pr_number, resource_path) or None if parsing fails
fn parse_pr_url(url: &str) -> Option<(u32, String)> {
    // URL format: https://github.com/owner/repo/pull/123
    let url = url.trim();

    // Remove protocol and domain
    let path = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))?;

    // Split into parts: owner/repo/pull/number
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() < 4 || parts[2] != "pull" {
        return None;
    }

    let owner = parts[0];
    let repo = parts[1];
    let number: u32 = parts[3].parse().ok()?;

    let resource_path = format!("/{}/{}/pull/{}", owner, repo, number);

    Some((number, resource_path))
}

/// Fetch the open PR for the current branch, if one exists
fn get_current_branch_pr() -> Result<Option<PullRequest>, String> {
    let output = Command::new("gh")
        .args([
            "pr",
            "view",
            "--json",
            "id,title,number,body,headRefName,baseRefName,url",
        ])
        .output()
        .map_err(|e| format!("Failed to execute gh command: {}", e))?;

    // gh exits with an error when the branch has no PR
    if !output.status.success() {
        return Ok(None);
    }

    #[derive(Deserialize)]
    struct PrView {
        id: String,
        title: String,
        number: u32,
        body: String,
        #[serde(alias = "headRefName")]
        head_ref_name: String,
        #[serde(alias = "baseRefName")]
        base_ref_name: String,
        url: String,
    }

    let pr_view: PrView = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse PR response: {}", e))?;

    let resource_path = parse_pr_url(&pr_view.url)
        .map(|(_, path)| path)
        .unwrap_or_default();

    Ok(Some(PullRequest {
        id: pr_view.id,
        title: pr_view.title,
        resource_path,
        number: pr_view.number,
        body: pr_view.body,
        head_ref_name: pr_view.head_ref_name,
        base_ref_name: pr_view.base_ref_name,
    }))
}

/// Fetch a single PR by number
///
/// # Arguments
/// * `repo` - The repository as "owner/repo"
/// * `pr_number` - The PR number to fetch
///
/// # Returns
/// The PullRequest details or an error
fn get_pr_by_number(repo: &str, pr_number: u32) -> Result<PullRequest, String> {
    let output = Command::new("gh")
        .args([
            "pr",
            "view",
            &pr_number.to_string(),
            "--repo",
            repo,
            "--json",
            "id,title,number,body,headRefName,baseRefName,url",
        ])
        .output()
        .map_err(|e| format!("Failed to execute gh command: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to fetch PR: {}", stderr));
    }

    #[derive(Deserialize)]
    struct PrView {
        id: String,
        title: String,
        number: u32,
        body: String,
        #[serde(alias = "headRefName")]
        head_ref_name: String,
        #[serde(alias = "baseRefName")]
        base_ref_name: String,
        url: String,
    }

    let pr_view: PrView = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse PR response: {}", e))?;

    // Convert URL to resource path
    let resource_path = parse_pr_url(&pr_view.url)
        .map(|(_, path)| path)
        .unwrap_or_default();

    Ok(PullRequest {
        id: pr_view.id,
        title: pr_view.title,
        resource_path,
        number: pr_view.number,
        body: pr_view.body,
        head_ref_name: pr_view.head_ref_name,
        base_ref_name: pr_view.base_ref_name,
    })
}

/// Publish a new pull request to GitHub
///
/// # Arguments
/// * `base` - The base branch to merge into
/// * `title` - The PR title
/// * `body` - The PR body/description
/// * `reviewers` - List of GitHub usernames to request review from
/// * `dry_run` - If true, only print the command without executing
fn publish_pr(
    base: String,
    title: String,
    body: String,
    reviewers: Vec<String>,
    dry_run: bool,
) -> Result<String, String> {
    let reviewers_str = reviewers.join(",");

    if dry_run {
        let reviewer_arg = if reviewers_str.is_empty() {
            String::new()
        } else {
            format!(" -r {}", reviewers_str)
        };
        println!(
            "gh pr create -B {} -t {:?} -a @me -b {:?}{}",
            base, title, body, reviewer_arg
        );
        return Ok("Dry run - no PR created".into());
    }

    let mut command = Command::new("gh");
    command.args([
        "pr", "create", "-B", &base, "-t", &title, "-a", "@me", "-b", &body,
    ]);
    if !reviewers_str.is_empty() {
        command.args(["-r", &reviewers_str]);
    }

    let output = command
        .output()
        .map_err(|e| format!("Failed to execute gh command: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to create PR: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.trim().to_string())
}

/// Update an existing pull request
///
/// # Arguments
/// * `repo_url` - The repository as "owner/repo"
/// * `pr_number` - The PR number to update
/// * `body` - The new body/description for the PR
/// * `dry_run` - If true, only print the command without executing
fn update_pr(repo_url: &str, pr_number: u32, body: &str, dry_run: bool) -> Result<String, String> {
    let pr_number_str = pr_number.to_string();

    if dry_run {
        println!(
            "gh pr edit {} --repo {} -b {:?}",
            pr_number_str, repo_url, body
        );
        return Ok("Dry run - no PR updated".into());
    }

    let output = Command::new("gh")
        .args(["pr", "edit", &pr_number_str, "--repo", repo_url, "-b", body])
        .output()
        .map_err(|e| format!("Failed to execute gh command: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to update PR: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remote_pr_from_pull_request() {
        let pr = PullRequest {
            id: "id".into(),
            title: "[TASK-1]: A".into(),
            resource_path: "/owner/repo/pull/123".into(),
            number: 123,
            body: "body".into(),
            head_ref_name: "feature".into(),
            base_ref_name: "main".into(),
        };

        let remote = RemotePr::from(pr);

        assert_eq!(
            remote.pr,
            PrRef {
                repo: "owner/repo".into(),
                number: 123
            }
        );
        assert_eq!(remote.head, "feature");
        assert_eq!(remote.base, "main");
    }

    #[test]
    fn test_parse_pr_url_https() {
        let url = "https://github.com/owner/repo/pull/123";
        let result = parse_pr_url(url);
        assert_eq!(result, Some((123, "/owner/repo/pull/123".to_string())));
    }

    #[test]
    fn test_parse_pr_url_http() {
        let url = "http://github.com/owner/repo/pull/456";
        let result = parse_pr_url(url);
        assert_eq!(result, Some((456, "/owner/repo/pull/456".to_string())));
    }

    #[test]
    fn test_parse_pr_url_with_whitespace() {
        let url = "  https://github.com/owner/repo/pull/789  \n";
        let result = parse_pr_url(url);
        assert_eq!(result, Some((789, "/owner/repo/pull/789".to_string())));
    }

    #[test]
    fn test_parse_pr_url_invalid_not_pull() {
        let url = "https://github.com/owner/repo/issues/123";
        let result = parse_pr_url(url);
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_pr_url_invalid_no_number() {
        let url = "https://github.com/owner/repo/pull/abc";
        let result = parse_pr_url(url);
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_pr_url_invalid_too_short() {
        let url = "https://github.com/owner";
        let result = parse_pr_url(url);
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_pr_url_with_extra_path() {
        // URLs like https://github.com/owner/repo/pull/123/files should still work
        let url = "https://github.com/owner/repo/pull/123/files";
        let result = parse_pr_url(url);
        assert_eq!(result, Some((123, "/owner/repo/pull/123".to_string())));
    }
}
