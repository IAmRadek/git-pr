use std::collections::{HashMap, HashSet};
use std::process;

use git2::{Branch, BranchType, Error, ObjectType, Oid, Reference, Repository};

pub(crate) fn get_branch_commits(repo: &Repository, branch_name: &str) -> Result<(String, Vec<String>), String> {
    //Iterate over commits and find each commit branch name
    let mut commit_branches: HashMap<Oid, HashSet<String>> = HashMap::new();

    let branches = repo.branches(None).unwrap();

    let _: Vec<()> = branches.map(|result| {
        let (b, _) = result.unwrap();

        let name = b.get().shorthand().unwrap();
        if name == branch_name || name == format!("origin/{}", branch_name) {
            return;
        }

        let mut revwalk = repo.revwalk().unwrap();
        revwalk.push_ref(b.get().name().unwrap()).unwrap();

        let _: Vec<()> = revwalk.map(|id| {
            let id = id.unwrap();

            commit_branches.entry(id).and_modify(|curr| {
                curr.insert(name.into());
            }).or_insert(HashSet::from([name.into()]));
        }).collect();
    }).collect();

    let branch = repo.find_branch(branch_name, BranchType::Local).unwrap();
    let mut revwalk = repo.revwalk().unwrap();
    revwalk.push_ref(branch.get().name().unwrap()).unwrap();

    let mut my_commits: Vec<String> = Vec::new();
    let mut base = String::new();

    for each in revwalk {
        let oid = each.unwrap();

        if let Some(branches) = commit_branches.get(&oid) {
            let mut branches = branches.iter().collect::<Vec<&String>>();
            branches.sort();
            branches.iter().filter(|b| !b.starts_with("origin/")).take(1).for_each(|b| {
                base = b.to_string();
            });
            println!("{} {:?}", oid, branches);
            break;
        } else {
            let commit = repo.find_commit(oid).unwrap();
            let message = commit.message().unwrap();
            my_commits.push(message.to_string());
        }
    }

    Ok((base, my_commits))
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
