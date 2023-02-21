use std::collections::{HashMap, HashSet};

use git2::{BranchType, Oid, Repository, RepositoryState};
use inquire::{Autocomplete, CustomUserError};
use inquire::autocompletion::Replacement;

use crate::errors::Error;

pub(crate) fn get_repository() -> Result<Repository, Error> {
    let r = Repository::open(".").map_err(|_| Error::NotInGitRepo)?;
    if r.state() != RepositoryState::Clean {
        Err(Error::BranchNotClean)
    } else {
        Ok(r)
    }
}

#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub bases: Vec<String>,
    pub commits: Vec<String>,
}

impl Autocomplete for BranchInfo {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, CustomUserError> {
        let mut suggestions = Vec::new();
        for tag in self.commits.iter().rev() {
            if tag.to_lowercase().contains(input.to_lowercase().as_str()) {
                suggestions.push(tag.clone());
            }
        }
        Ok(suggestions)
    }

    fn get_completion(&mut self, input: &str, highlighted_suggestion: Option<String>) -> Result<Replacement, CustomUserError> {
        if let Some(..) = highlighted_suggestion {
            return Ok(Some(highlighted_suggestion.unwrap()));
        }
        for tag in self.commits.iter() {
            if tag.contains(input) {
                return Ok(Some(tag.clone()));
            }
        }
        Ok(None)
    }
}


pub(crate) fn get_branch_bases_and_commits() -> Result<BranchInfo, Error> {
    let repo = get_repository()?;

    let head = repo.head().map_err(|_| Error::BranchNotClean)?;
    let current_branch = head.shorthand().unwrap_or("HEAD");

    if is_main(current_branch) {
        return Err(Error::CannotBeInMainBranch(current_branch.to_string()));
    }

    let mut commit_branches: HashMap<Oid, HashSet<String>> = HashMap::new();
    let branches = repo.branches(None).unwrap();

    for result in branches {
        let (branch, _) = result.unwrap();

        let name = branch.get().shorthand().unwrap();
        if name == current_branch || name == format!("origin/{}", current_branch) {
            continue;
        }

        let mut revwalk = repo.revwalk().unwrap();
        revwalk.push_ref(branch.get().name().unwrap()).unwrap();

        for each in revwalk {
            let id = each.unwrap();

            commit_branches.entry(id).and_modify(|curr| {
                curr.insert(name.into());
            }).or_insert_with(|| HashSet::from([name.into()]));
        }
    }

    let branch = repo.find_branch(current_branch, BranchType::Local).unwrap();
    let mut revwalk = repo.revwalk().unwrap();
    revwalk.push_ref(branch.get().name().unwrap()).unwrap();

    let mut bases: Vec<String> = Vec::new();
    let mut commits: Vec<String> = Vec::new();

    for each in revwalk {
        let oid = each.unwrap();

        if let Some(branches) = commit_branches.get(&oid) {
            let mut branches = branches.iter().collect::<Vec<&String>>();
            branches.sort();
            branches.iter().filter(|b| !b.starts_with("origin/")).take(1).for_each(|b| {
                bases.push(b.to_string());
            });
            break;
        } else {
            let commit = repo.find_commit(oid).unwrap();
            let message = commit.message().unwrap();
            commits.push(message.trim().to_string());
        }
    }

    Ok(BranchInfo {
        bases,
        commits,
    })
}

fn is_main(name: &str) -> bool {
    let forbidden = vec!["master", "main", "development", "stage", "production"];
    forbidden.contains(&name)
}

