use std::process;

use git2::{Branch, BranchType, Error, Reference, Repository};

pub(crate) fn get_branch_commits(repo: &Repository, branch_name: &str) -> Result<Vec<String>, String> {
    let main = get_main_branch(&repo).unwrap();
    let branch = repo.find_branch(branch_name, BranchType::Local).unwrap();

    let mut revwalk = repo.revwalk().unwrap();
    revwalk.push_range(format!("{}..{}", main.get().target().unwrap(), branch.get().target().unwrap()).as_str()).unwrap();

    Ok(revwalk.map(|id| -> String{
        let commit = repo.find_commit(id.unwrap()).unwrap();
        commit.message().unwrap().into()
    }).collect())
}

fn get_main_branch(repo: &Repository) -> Result<Branch, Error> {
    match repo.find_branch("master", BranchType::Local) {
        Ok(branch) => Ok(branch),
        Err(_) => {
            match repo.find_branch("main", BranchType::Local) {
                Ok(branch) => Ok(branch),
                Err(err) => {
                    Err(err)
                }
            }
        }
    }
}

pub(crate) fn get_head(repo: &Repository) -> Reference {
    match repo.head() {
        Ok(head) => head,
        Err(_) => {
            println!("Unable to find repo HEAD.");
            process::exit(1)
        }
    }
}
