//! Built-in plugin that rewrites the related-PR block in every PR of a stack.

use colored::Colorize;

use crate::error::Result;
use crate::git::BranchInfo;
use crate::plugin::{AfterCreate, Context, Draft, Forge, PrRef, RemotePr};
use crate::template;

/// Links the open PRs that share a tag with the current stack
pub struct RelatedPrs;

impl AfterCreate for RelatedPrs {
    fn name(&self) -> &'static str {
        "related_prs"
    }

    fn run(
        &self,
        ctx: &Context,
        forge: &dyn Forge,
        draft: &Draft,
        created: Option<&PrRef>,
        dry_run: bool,
    ) -> Result<()> {
        let Some(current_tag) = draft.ticket.as_ref().map(|t| t.id.as_str()) else {
            println!(
                "{} No PR tag; skipping related PR updates.",
                ">".bright_green()
            );
            return Ok(());
        };

        let all_prs = forge.open_prs()?;
        let tags = collect_stack_tags(&all_prs, &ctx.branch, current_tag);
        let mut related_prs = filter_related_prs_by_tags(all_prs, &tags);

        if let Some(created) = created {
            let already_in_list = related_prs.iter().any(|p| p.pr == *created);
            if !already_in_list {
                match forge.get(created) {
                    Ok(new_pr) => {
                        related_prs.insert(0, new_pr);
                    }
                    Err(err) => {
                        println!(
                            "{} Could not fetch newly created PR {}#{}: {}",
                            "!".yellow(),
                            created.repo,
                            created.number,
                            err
                        );
                    }
                }
            }
        }

        if related_prs.is_empty() {
            println!("{} No related PRs found.", ">".bright_green());
            return Ok(());
        }

        if related_prs.len() == 1 {
            println!(
                "{} Only one related PR found; skipping related PR updates.",
                ">".bright_green()
            );
            return Ok(());
        }

        println!(
            "{} Found {} related PRs. Updating...",
            ">".bright_green(),
            related_prs.len()
        );

        for related_pr in &related_prs {
            let updated_body = template::replace_related_prs(
                &ctx.config,
                forge,
                &related_pr.body,
                &related_pr.pr,
                &related_prs,
            );

            match forge.update_body(&related_pr.pr, &updated_body, dry_run) {
                Ok(()) => {
                    println!(
                        "{} Updated {}#{}",
                        "+".bright_green(),
                        related_pr.pr.repo,
                        related_pr.pr.number
                    );
                }
                Err(err) => {
                    println!(
                        "{} Update {}#{} failed: {}",
                        "x".red(),
                        related_pr.pr.repo,
                        related_pr.pr.number,
                        err
                    );
                }
            }
        }

        Ok(())
    }
}

fn collect_stack_tags(
    all_prs: &[RemotePr],
    branch_info: &BranchInfo,
    current_tag: &str,
) -> Vec<String> {
    use std::collections::{HashMap, HashSet};

    let pr_by_head: HashMap<&str, &RemotePr> =
        all_prs.iter().map(|pr| (pr.head.as_str(), pr)).collect();

    let mut prs_by_base: HashMap<&str, Vec<&RemotePr>> = HashMap::new();
    for pr in all_prs {
        prs_by_base.entry(pr.base.as_str()).or_default().push(pr);
    }

    let mut tags: HashSet<String> = HashSet::new();
    tags.insert(current_tag.to_string());

    // Walk up: follow base branch -> its PR -> that PR's base -> etc.
    if let Some(first_base) = branch_info.bases.first() {
        let mut branch = first_base.as_str();
        let mut visited: HashSet<&str> = HashSet::new();
        loop {
            if !visited.insert(branch) {
                break;
            }
            match pr_by_head.get(branch) {
                Some(pr) => {
                    if let Some(tag) = crate::tags::extract_from_str(&pr.title) {
                        tags.insert(tag);
                    }
                    branch = pr.base.as_str();
                }
                None => break,
            }
        }
    }

    // Walk down: find PRs whose base is the current branch, recursively.
    let mut queue = vec![branch_info.current_branch.as_str()];
    let mut visited_down: HashSet<&str> = HashSet::new();
    while let Some(base) = queue.pop() {
        if !visited_down.insert(base) {
            continue;
        }
        if let Some(children) = prs_by_base.get(base) {
            for child_pr in children {
                if let Some(tag) = crate::tags::extract_from_str(&child_pr.title) {
                    tags.insert(tag);
                }
                queue.push(child_pr.head.as_str());
            }
        }
    }

    tags.into_iter().collect()
}

fn filter_related_prs_by_tags(prs: Vec<RemotePr>, tags: &[String]) -> Vec<RemotePr> {
    use std::collections::HashSet;
    let tag_set: HashSet<&str> = tags.iter().map(|t| t.as_str()).collect();

    prs.into_iter()
        .filter(|pr| match crate::tags::extract_from_str(&pr.title) {
            Some(extracted_tag) => tag_set.contains(extracted_tag.as_str()),
            None => {
                println!(
                    "{} {} {}",
                    "x".bright_red(),
                    pr.title.bright_cyan(),
                    "No tag found".bright_red()
                );
                false
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pr(number: u32, title: &str, head: &str, base: &str) -> RemotePr {
        RemotePr {
            pr: PrRef {
                repo: "owner/repo".to_string(),
                number,
            },
            title: title.to_string(),
            body: String::new(),
            head: head.to_string(),
            base: base.to_string(),
        }
    }

    fn make_branch_info(current: &str, bases: &[&str]) -> BranchInfo {
        BranchInfo {
            current_branch: current.to_string(),
            bases: bases.iter().map(|s| s.to_string()).collect(),
            commits: vec![],
        }
    }

    #[test]
    fn test_linear_stack_collects_all_tags() {
        // A <- B <- C <- D (current), each with a different task tag
        let prs = vec![
            make_pr(1, "[TASK-1]: feat A", "branch-a", "main"),
            make_pr(2, "[TASK-2]: feat B", "branch-b", "branch-a"),
            make_pr(3, "[TASK-3]: feat C", "branch-c", "branch-b"),
        ];
        let branch_info = make_branch_info("branch-d", &["branch-c"]);

        let mut tags = collect_stack_tags(&prs, &branch_info, "TASK-4");
        tags.sort();

        assert_eq!(tags, vec!["TASK-1", "TASK-2", "TASK-3", "TASK-4"]);
    }

    #[test]
    fn test_down_walk_finds_child_prs() {
        // current branch is B (mid-stack), C and D are children
        let prs = vec![
            make_pr(3, "[TASK-3]: feat C", "branch-c", "branch-b"),
            make_pr(4, "[TASK-4]: feat D", "branch-d", "branch-c"),
        ];
        let branch_info = make_branch_info("branch-b", &["branch-a"]);

        let mut tags = collect_stack_tags(&prs, &branch_info, "TASK-2");
        tags.sort();

        assert_eq!(tags, vec!["TASK-2", "TASK-3", "TASK-4"]);
    }

    #[test]
    fn test_no_stack_prs_returns_only_current_tag() {
        let prs = vec![make_pr(1, "[OTHER-1]: unrelated", "other-branch", "main")];
        let branch_info = make_branch_info("my-branch", &["main"]);

        let tags = collect_stack_tags(&prs, &branch_info, "TASK-1");

        assert_eq!(tags, vec!["TASK-1"]);
    }

    #[test]
    fn test_cycle_guard() {
        // pathological: A's base is B and B's base is A
        let prs = vec![
            make_pr(1, "[TASK-1]: A", "branch-a", "branch-b"),
            make_pr(2, "[TASK-2]: B", "branch-b", "branch-a"),
        ];
        let branch_info = make_branch_info("branch-c", &["branch-a"]);

        // should terminate and not hang
        let mut tags = collect_stack_tags(&prs, &branch_info, "TASK-3");
        tags.sort();

        assert_eq!(tags, vec!["TASK-1", "TASK-2", "TASK-3"]);
    }

    #[test]
    fn test_root_branch_no_up_walk() {
        // current branch bases directly on main, no PR above it
        let prs = vec![make_pr(1, "[TASK-1]: sibling", "other", "main")];
        let branch_info = make_branch_info("my-branch", &["main"]);

        let tags = collect_stack_tags(&prs, &branch_info, "TASK-2");

        assert_eq!(tags, vec!["TASK-2"]);
    }
}
