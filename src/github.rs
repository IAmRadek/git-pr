use std::process::Command;

use serde::{Deserialize, Serialize};

const REVIEWERS_QUERY: &str = "query ($repo: String!, $owner: String!) {
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
}";

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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct PullRequest {
    pub id: String,
    pub title: String,
    #[serde(alias = "resourcePath")]
    pub resource_path: String,
    pub number: u32,
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


#[derive(Serialize, Deserialize)]
struct CurrentBranch {
    title: String,
}

pub(crate) fn get_available_reviewers() -> Result<Vec<String>, String> {
    let cmd = Command::new("gh")
        .args(vec![
            "api", "graphql",
            "-F", "owner=:owner",
            "-F", "repo=:repo",
            "-f", format!("query={}", REVIEWERS_QUERY).as_str(),
        ])
        .output()
        .expect("Failed to get available reviewers");

    let v: Response<Repository> = serde_json::from_slice(cmd.stdout.as_slice())
        .expect("expected to be json");

    let nodes = v.data.repository.assignable_users.nodes;
    Ok(nodes.into_iter().map(|node| -> String {
        node.login
    }).collect())
}

const RELATED_PR_QUERY: &str = "query ($login: String!) {
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
}";

pub(crate) fn get_user_prs() -> Result<Vec<PullRequest>, String> {
    let login = env!("GITHUB_USER", "Env GITHUB_USER not found!");

    let cmd = Command::new("gh")
        .args(vec![
            "api", "graphql",
            "-F", format!("login={}", login).as_str(),
            "-f", format!("query={}", RELATED_PR_QUERY).as_str(),
        ])
        .output()
        .expect("Failed to get available reviewers");

    let v: Response<User> = serde_json::from_slice(cmd.stdout.as_slice())
        .expect("expected to be json");

    let edges = v.data.user.pull_requests.edges;
    Ok(edges.into_iter().map(|edge| -> PullRequest {
        edge.node
    }).collect())
}

pub(crate) fn publish_pr(base: String, title: String, pr_body: String, reviewers: Vec<String>, dry_run: bool) -> Result<String, String> {
    if dry_run {
        println!("gh pr create -B {} -t {} -a @me -b {} -r {}", base, title, pr_body, reviewers.join(","));

        return Ok("Dry run".into());
    }


    let cmd = Command::new("gh")
        .args(vec![
            "pr", "create",
            "-B", format!("{}", base).as_str(),
            "-t", format!("{}", title).as_str(),
            "-a", "@me",
            "-b", format!("{}", pr_body).as_str(),
            "-r", reviewers.join(",").as_str(),
        ])
        .output()
        .expect("Failed to create PR");

    Ok(String::from_utf8(cmd.stdout).unwrap_or("Failed to get stdout".into()))
}

pub(crate) fn update_pr(pr: &u32, resource_path: &String, body: String, dry_run: bool) -> Result<String, String> {
    let mut parts: Vec<&str> = resource_path.split("/").collect();
    parts.pop();            // removes pr number
    parts.pop();            // removes "pull"
    parts.remove(0); // removes ""

    let repo_url = parts.join("/");

    let pr_number = format!("{}", pr.clone());
    let pr_body = format!("{}", body.clone());
    let pr_url = format!("{}", repo_url.clone());

    if dry_run {
        println!("gh pr edit {} --repo {} -b {}", pr_number, pr_url, pr_body);

        return Ok("Dry run".into());
    }

    let cmd = Command::new("gh")
        .args(vec![
            "pr", "edit",
            pr_number.as_str(),
            "--repo", pr_url.as_str(),
            "-b", pr_body.as_str(),
        ])
        .output()
        .expect("Failed to create PR");


    let stdout = String::from_utf8(cmd.stdout).unwrap_or("Failed to get stdout".into());
    Ok(String::from(stdout.trim()))
}
