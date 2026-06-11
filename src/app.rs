use colored::Colorize;

use crate::config::{self, Config};
use crate::error::{Error, Result};
use crate::git;
use crate::github;
use crate::pr::PullRequest;
use crate::tags::Tags;
use crate::template;
use crate::ui;

pub fn run(args: crate::cli::Args) -> Result<()> {
    ui::init_render_config();

    config::ensure_config_dir_exists(std::path::Path::new(&args.config));
    let config = Config::load(&args.config)?;

    let branch_info = git::get_branch_bases_and_commits()?;

    if branch_info.commits.is_empty() {
        return Err(Error::NoCommits);
    }

    let tags_path = config::get_tags_path_with_dir(&args.config);
    let mut tags = Tags::from_file(tags_path)?;

    // In --update-only mode there are no new commits with a tag, so look up the
    // existing PR on GitHub and pull the tag from its title instead.
    let github_tag = if args.update_only {
        github::get_current_branch_pr()
            .ok()
            .and_then(|p| crate::tags::extract_from_str(&p.title))
    } else {
        None
    };

    let mut pr = build_pr_from_branch(&branch_info, &mut tags, github_tag, args.update_only)?;

    pr.base = select_base_branch(&branch_info)?;

    let mut created_pr_number: Option<u32> = None;

    if !args.update_only {
        pr = gather_pr_details(&config, &branch_info, pr)?;
        created_pr_number = publish_pr(&pr, args.dry_run)?;
    }

    update_related_prs(&config, &branch_info, &pr, created_pr_number, args.dry_run)?;

    Ok(())
}

fn build_pr_from_branch(
    branch_info: &git::BranchInfo,
    tags: &mut Tags,
    github_tag: Option<String>,
    update_only: bool,
) -> Result<PullRequest> {
    if let Some(tag) = github_tag {
        tags.add_and_save(tag.clone())?;
        println!("{} PR Tag: {}", ">".bright_green(), tag.bright_cyan());
        return Ok(PullRequest::new().with_tag(tag).with_jira(true));
    }

    if update_only {
        println!(
            "{} No PR tag found; skipping related PR tracking.",
            ">".bright_green()
        );
        return Ok(PullRequest::new());
    }

    let found_tag = crate::tags::extract_from_vec(branch_info.commits.clone());

    if let Some((tag, commit)) = found_tag {
        tags.add_and_save(tag.clone())?;

        println!("{} PR title: {}", ">".bright_green(), commit.bright_cyan());
        println!("{} PR Tag: {}", ">".bright_green(), tag.bright_cyan());

        Ok(PullRequest::new()
            .with_tag(tag)
            .with_title(commit)
            .with_jira(true))
    } else {
        let selected_tag = ui::prompt_tag(tags)?;

        let default_title = ui::default_pr_title(branch_info);
        let full_title = match &selected_tag {
            Some(tag) => {
                tags.add(tag.clone());
                tags.save()?;
                format!("[{}]: {}", tag, default_title)
            }
            None => default_title,
        };

        println!(
            "{} PR title: {}",
            ">".bright_green(),
            full_title.bright_cyan()
        );

        let pr = PullRequest::new().with_title(full_title).with_jira(false);

        Ok(if let Some(tag) = selected_tag {
            pr.with_tag(tag)
        } else {
            pr
        })
    }
}

fn select_base_branch(branch_info: &git::BranchInfo) -> Result<String> {
    match branch_info.bases.len() {
        0 => Err(Error::InvalidInput(
            "Could not determine a base branch from git history".to_string(),
        )),
        1 => {
            let base = branch_info.bases[0].clone();
            println!("{} PR base: {}", ">".bright_green(), base.bright_cyan());
            Ok(base)
        }
        _ => ui::prompt_base(branch_info.bases.clone()),
    }
}

fn gather_pr_details(
    config: &Config,
    _branch_info: &git::BranchInfo,
    pr: PullRequest,
) -> Result<PullRequest> {
    let initial_body = template::build_editor_body(config, pr.tag.as_deref(), pr.is_jira);
    let body = ui::prompt_body(&initial_body)?;

    let reviewers_list = github::get_available_reviewers()
        .unwrap_or_else(|_| config.github.default_reviewers.clone());
    let reviewers = ui::prompt_reviewers(reviewers_list)?;

    Ok(pr.with_body(body).with_reviewers(reviewers))
}

fn publish_pr(pr: &PullRequest, dry_run: bool) -> Result<Option<u32>> {
    match github::publish_pr(
        pr.base.clone(),
        pr.title.clone(),
        pr.body.clone(),
        pr.reviewers.clone(),
        dry_run,
    ) {
        Ok(url) => {
            println!("Published at: {}", url);
            let pr_number = github::parse_pr_url(&url).map(|(num, _)| num);
            Ok(pr_number)
        }
        Err(err) => Err(Error::GitHubCli(err)),
    }
}

fn update_related_prs(
    config: &Config,
    branch_info: &git::BranchInfo,
    pr: &PullRequest,
    created_pr_number: Option<u32>,
    dry_run: bool,
) -> Result<()> {
    let Some(current_tag) = pr.tag.as_deref() else {
        println!(
            "{} No PR tag; skipping related PR updates.",
            ">".bright_green()
        );
        return Ok(());
    };

    let mut related_prs = match github::get_user_prs(config.github_user().as_deref()) {
        Ok(all_prs) => {
            let tags = collect_stack_tags(&all_prs, branch_info, current_tag);
            filter_related_prs_by_tags(all_prs, &tags)
        }
        Err(err) => {
            return Err(Error::GitHubCli(err));
        }
    };

    if let Some(pr_number) = created_pr_number {
        let already_in_list = related_prs.iter().any(|p| p.number == pr_number);
        if !already_in_list {
            match github::get_pr_by_number(pr_number) {
                Ok(new_pr) => {
                    related_prs.insert(0, new_pr);
                }
                Err(err) => {
                    println!(
                        "{} Could not fetch newly created PR #{}: {}",
                        "!".yellow(),
                        pr_number,
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
            config,
            &related_pr.body,
            &related_pr.number,
            &related_prs,
        );

        match github::update_pr(
            &related_pr.number,
            &related_pr.resource_path,
            updated_body,
            dry_run,
        ) {
            Ok(msg) => {
                println!(
                    "{} Updated #{}: {}",
                    "+".bright_green(),
                    related_pr.number,
                    msg
                );
            }
            Err(err) => {
                println!(
                    "{} Update #{} failed: {}",
                    "x".red(),
                    related_pr.number,
                    err
                );
            }
        }
    }

    Ok(())
}

fn collect_stack_tags(
    all_prs: &[github::PullRequest],
    branch_info: &git::BranchInfo,
    current_tag: &str,
) -> Vec<String> {
    use std::collections::{HashMap, HashSet};

    let pr_by_head: HashMap<&str, &github::PullRequest> = all_prs
        .iter()
        .map(|pr| (pr.head_ref_name.as_str(), pr))
        .collect();

    let mut prs_by_base: HashMap<&str, Vec<&github::PullRequest>> = HashMap::new();
    for pr in all_prs {
        prs_by_base
            .entry(pr.base_ref_name.as_str())
            .or_default()
            .push(pr);
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
                    branch = pr.base_ref_name.as_str();
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
                queue.push(child_pr.head_ref_name.as_str());
            }
        }
    }

    tags.into_iter().collect()
}

fn filter_related_prs_by_tags(
    prs: Vec<github::PullRequest>,
    tags: &[String],
) -> Vec<github::PullRequest> {
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

    fn make_pr(number: u32, title: &str, head: &str, base: &str) -> github::PullRequest {
        github::PullRequest {
            id: number.to_string(),
            title: title.to_string(),
            resource_path: format!("/owner/repo/pull/{}", number),
            number,
            body: String::new(),
            head_ref_name: head.to_string(),
            base_ref_name: base.to_string(),
        }
    }

    fn make_branch_info(current: &str, bases: &[&str]) -> git::BranchInfo {
        git::BranchInfo {
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
