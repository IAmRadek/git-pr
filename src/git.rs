use git2::{BranchType, Oid, Repository, RepositoryState};
use inquire::autocompletion::Replacement;
use inquire::{Autocomplete, CustomUserError};

use crate::error::Error;

/// Opens the git repository in the current directory
pub fn get_repository() -> Result<Repository, Error> {
    let r = Repository::open(".").map_err(|_| Error::NotInGitRepo)?;
    if r.state() != RepositoryState::Clean {
        Err(Error::BranchNotClean)
    } else {
        Ok(r)
    }
}

/// Information about the current branch including potential base branches and commits
#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub current_branch: String,
    pub bases: Vec<String>,
    pub commits: Vec<String>,
}

impl Autocomplete for BranchInfo {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, CustomUserError> {
        let mut suggestions = Vec::new();
        for commit in self.commits.iter().rev() {
            if commit.to_lowercase().contains(&input.to_lowercase()) {
                suggestions.push(commit.clone());
            }
        }
        Ok(suggestions)
    }

    fn get_completion(
        &mut self,
        input: &str,
        highlighted_suggestion: Option<String>,
    ) -> Result<Replacement, CustomUserError> {
        if highlighted_suggestion.is_some() {
            return Ok(highlighted_suggestion);
        }
        for commit in self.commits.iter() {
            if commit.contains(input) {
                return Ok(Some(commit.clone()));
            }
        }
        Ok(None)
    }
}

pub fn get_branch_bases_and_commits() -> Result<BranchInfo, Error> {
    let repo = get_repository()?;

    let head = repo.head().map_err(|_| Error::BranchNotClean)?;
    let current_branch = head.shorthand().unwrap_or("HEAD");

    if is_main(current_branch) {
        return Err(Error::CannotBeInMainBranch(current_branch.to_string()));
    }

    let current_oid = head.peel_to_commit().map_err(Error::Git)?.id();

    let mut base_candidates: Vec<(String, Oid, usize)> = Vec::new();
    let branches = repo.branches(Some(BranchType::Local)).map_err(Error::Git)?;

    for result in branches {
        let (branch, _) = result.map_err(Error::Git)?;
        let name = branch.get().shorthand().unwrap_or("");

        if name.is_empty() || name == current_branch {
            continue;
        }

        let branch_oid = branch.get().peel_to_commit().map_err(Error::Git)?.id();

        let (ahead, behind) = repo
            .graph_ahead_behind(current_oid, branch_oid)
            .map_err(Error::Git)?;

        // Candidate bases must be ancestors of the current branch.
        if ahead > 0 && behind == 0 {
            base_candidates.push((name.to_string(), branch_oid, ahead));
        }
    }

    base_candidates.sort_by_key(|(_, _, ahead)| *ahead);

    let bases: Vec<String> = base_candidates
        .iter()
        .map(|(name, _, _)| name.clone())
        .collect();

    let commits = if let Some((_, base_oid, _)) = base_candidates.first() {
        collect_commits_since(&repo, current_oid, *base_oid)?
    } else {
        collect_all_branch_commits(&repo, current_branch)?
    };

    Ok(BranchInfo {
        current_branch: current_branch.to_string(),
        bases,
        commits,
    })
}

fn collect_commits_since(
    repo: &Repository,
    head_oid: Oid,
    base_oid: Oid,
) -> Result<Vec<String>, Error> {
    let merge_base = repo.merge_base(head_oid, base_oid).map_err(Error::Git)?;
    let mut revwalk = repo.revwalk().map_err(Error::Git)?;
    revwalk.push(head_oid).map_err(Error::Git)?;
    revwalk.hide(merge_base).map_err(Error::Git)?;

    let mut commits = Vec::new();
    for each in revwalk {
        let oid = each.map_err(Error::Git)?;
        let commit = repo.find_commit(oid).map_err(Error::Git)?;
        if let Some(message) = commit.message() {
            commits.push(message.trim().to_string());
        }
    }

    Ok(commits)
}

fn collect_all_branch_commits(
    repo: &Repository,
    current_branch: &str,
) -> Result<Vec<String>, Error> {
    let branch = repo
        .find_branch(current_branch, BranchType::Local)
        .map_err(Error::Git)?;
    let mut revwalk = repo.revwalk().map_err(Error::Git)?;

    if let Some(ref_name) = branch.get().name() {
        revwalk.push_ref(ref_name).map_err(Error::Git)?;
    }

    let mut commits = Vec::new();
    for each in revwalk {
        let oid = each.map_err(Error::Git)?;
        let commit = repo.find_commit(oid).map_err(Error::Git)?;
        if let Some(message) = commit.message() {
            commits.push(message.trim().to_string());
        }
    }

    Ok(commits)
}

fn is_main(name: &str) -> bool {
    const PROTECTED_BRANCHES: &[&str] = &["master", "main", "development", "stage", "production"];
    PROTECTED_BRANCHES.contains(&name)
}
