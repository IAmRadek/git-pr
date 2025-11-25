use std::collections::{HashMap, HashSet};

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
    /// Potential base branches for the PR
    pub bases: Vec<String>,
    /// Commit messages on the current branch
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

/// Get the base branches and commits for the current branch
pub fn get_branch_bases_and_commits() -> Result<BranchInfo, Error> {
    let repo = get_repository()?;

    let head = repo.head().map_err(|_| Error::BranchNotClean)?;
    let current_branch = head.shorthand().unwrap_or("HEAD");

    if is_main(current_branch) {
        return Err(Error::CannotBeInMainBranch(current_branch.to_string()));
    }

    let mut commit_branches: HashMap<Oid, HashSet<String>> = HashMap::new();
    let branches = repo.branches(None).map_err(Error::Git)?;

    for result in branches {
        let (branch, _) = result.map_err(Error::Git)?;

        let name = branch.get().shorthand().unwrap_or("");
        if name == current_branch || name == format!("origin/{}", current_branch) {
            continue;
        }

        let mut revwalk = repo.revwalk().map_err(Error::Git)?;
        if let Some(ref_name) = branch.get().name() {
            revwalk.push_ref(ref_name).map_err(Error::Git)?;

            for each in revwalk {
                let id = each.map_err(Error::Git)?;

                commit_branches
                    .entry(id)
                    .and_modify(|curr| {
                        curr.insert(name.into());
                    })
                    .or_insert_with(|| HashSet::from([name.into()]));
            }
        }
    }

    let branch = repo
        .find_branch(current_branch, BranchType::Local)
        .map_err(Error::Git)?;
    let mut revwalk = repo.revwalk().map_err(Error::Git)?;

    if let Some(ref_name) = branch.get().name() {
        revwalk.push_ref(ref_name).map_err(Error::Git)?;
    }

    let mut bases: Vec<String> = Vec::new();
    let mut commits: Vec<String> = Vec::new();

    for each in revwalk {
        let oid = each.map_err(Error::Git)?;

        if let Some(branches) = commit_branches.get(&oid) {
            let mut branches: Vec<&String> = branches.iter().collect();
            branches.sort();
            branches
                .iter()
                .filter(|b| !b.starts_with("origin/"))
                .take(1)
                .for_each(|b| {
                    bases.push(b.to_string());
                });
            break;
        } else {
            let commit = repo.find_commit(oid).map_err(Error::Git)?;
            if let Some(message) = commit.message() {
                commits.push(message.trim().to_string());
            }
        }
    }

    Ok(BranchInfo { bases, commits })
}

/// Check if the given branch name is a protected/main branch
fn is_main(name: &str) -> bool {
    const PROTECTED_BRANCHES: &[&str] = &["master", "main", "development", "stage", "production"];
    PROTECTED_BRANCHES.contains(&name)
}
