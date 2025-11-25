use std::process::Command;

use serde::{Deserialize, Serialize};

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
    pullRequests(last: 20) {
      edges {
        node {
          id
          title
          resourcePath
          number
          body
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
pub struct PullRequest {
    /// The node ID of the PR
    pub id: String,
    /// The title of the PR
    pub title: String,
    /// The resource path (e.g., /owner/repo/pull/123)
    #[serde(alias = "resourcePath")]
    pub resource_path: String,
    /// The PR number
    pub number: u32,
    /// The body/description of the PR
    pub body: String,
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
pub fn get_user_prs(github_user: Option<&str>) -> Result<Vec<PullRequest>, String> {
    let login = match github_user {
        Some(user) if !user.is_empty() => user.to_string(),
        _ => std::env::var("GITHUB_USER").map_err(|_| {
            "GitHub user not configured. Set github.user in config or GITHUB_USER environment variable".to_string()
        })?,
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

/// Publish a new pull request to GitHub
///
/// # Arguments
/// * `base` - The base branch to merge into
/// * `title` - The PR title
/// * `body` - The PR body/description
/// * `reviewers` - List of GitHub usernames to request review from
/// * `dry_run` - If true, only print the command without executing
pub fn publish_pr(
    base: String,
    title: String,
    body: String,
    reviewers: Vec<String>,
    dry_run: bool,
) -> Result<String, String> {
    let reviewers_str = reviewers.join(",");

    if dry_run {
        println!(
            "gh pr create -B {} -t {:?} -a @me -b {:?} -r {}",
            base, title, body, reviewers_str
        );
        return Ok("Dry run - no PR created".into());
    }

    let output = Command::new("gh")
        .args([
            "pr",
            "create",
            "-B",
            &base,
            "-t",
            &title,
            "-a",
            "@me",
            "-b",
            &body,
            "-r",
            &reviewers_str,
        ])
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
/// * `pr_number` - The PR number to update
/// * `resource_path` - The resource path of the PR (used to determine the repo)
/// * `body` - The new body/description for the PR
/// * `dry_run` - If true, only print the command without executing
pub fn update_pr(
    pr_number: &u32,
    resource_path: &str,
    body: String,
    dry_run: bool,
) -> Result<String, String> {
    // Parse repo from resource path (e.g., "/owner/repo/pull/123" -> "owner/repo")
    let parts: Vec<&str> = resource_path.split('/').collect();
    if parts.len() < 4 {
        return Err(format!("Invalid resource path: {}", resource_path));
    }

    // Skip empty first element, take owner and repo
    let repo_url = format!("{}/{}", parts[1], parts[2]);
    let pr_number_str = pr_number.to_string();

    if dry_run {
        println!(
            "gh pr edit {} --repo {} -b {:?}",
            pr_number_str, repo_url, body
        );
        return Ok("Dry run - no PR updated".into());
    }

    let output = Command::new("gh")
        .args([
            "pr",
            "edit",
            &pr_number_str,
            "--repo",
            &repo_url,
            "-b",
            &body,
        ])
        .output()
        .map_err(|e| format!("Failed to execute gh command: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to update PR: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.trim().to_string())
}
