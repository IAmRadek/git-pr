use colored::Colorize;

use crate::config::{self, Config};
use crate::error::{Error, Result};
use crate::git;
use crate::github;
use crate::plugin::{Draft, Forge, PrRef, Registry, RemotePr, Ticket};
use crate::tags::Tags;
use crate::template;
use crate::ui;

pub fn run(args: crate::cli::Args) -> Result<()> {
    ui::init_render_config();

    config::ensure_config_dir_exists(std::path::Path::new(&args.config));
    let config = Config::load(&args.config)?;
    let registry = Registry::default();
    let forge = registry.forge.as_ref();

    let branch_info = git::get_branch_bases_and_commits()?;

    if branch_info.commits.is_empty() {
        return Err(Error::NoCommits);
    }

    let tags_path = config::get_tags_path_with_dir(&args.config);
    let mut tags = Tags::from_file(tags_path)?;

    // In --update-only mode there are no new commits with a tag, so look up the
    // existing PR on GitHub and pull the tag from its title instead.
    let github_tag = if args.update_only {
        forge
            .current_pr()
            .ok()
            .flatten()
            .and_then(|p| crate::tags::extract_from_str(&p.title))
    } else {
        None
    };

    let mut pr = build_pr_from_branch(
        &config,
        &branch_info,
        &mut tags,
        github_tag,
        args.update_only,
    )?;

    pr.base = select_base_branch(&branch_info)?;

    let mut created: Option<PrRef> = None;

    if !args.update_only {
        pr = gather_pr_details(&config, forge, &branch_info, pr)?;
        created = forge.create(&pr, args.dry_run)?;
    }

    update_related_prs(
        &config,
        forge,
        &branch_info,
        &pr,
        created.as_ref(),
        args.dry_run,
    )?;

    Ok(())
}

fn build_pr_from_branch(
    config: &Config,
    branch_info: &git::BranchInfo,
    tags: &mut Tags,
    github_tag: Option<String>,
    update_only: bool,
) -> Result<Draft> {
    if let Some(tag) = github_tag {
        tags.add_and_save(tag.clone())?;
        println!("{} PR Tag: {}", ">".bright_green(), tag.bright_cyan());
        return Ok(Draft {
            ticket: Some(jira_ticket(config, tag)),
            ..Draft::default()
        });
    }

    if update_only {
        println!(
            "{} No PR tag found; skipping related PR tracking.",
            ">".bright_green()
        );
        return Ok(Draft::default());
    }

    let found_tag = crate::tags::extract_from_vec(branch_info.commits.clone());

    if let Some((tag, commit)) = found_tag {
        tags.add_and_save(tag.clone())?;

        println!("{} PR title: {}", ">".bright_green(), commit.bright_cyan());
        println!("{} PR Tag: {}", ">".bright_green(), tag.bright_cyan());

        Ok(Draft {
            ticket: Some(jira_ticket(config, tag)),
            title: commit,
            ..Draft::default()
        })
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

        Ok(Draft {
            ticket: selected_tag.map(|id| Ticket {
                id,
                ..Ticket::default()
            }),
            title: full_title,
            ..Draft::default()
        })
    }
}

/// A ticket from a commit or PR title, linked to Jira when `jira.url` is set
fn jira_ticket(config: &Config, id: String) -> Ticket {
    Ticket {
        url: config.jira_url().map(|url| format!("{}{}", url, id)),
        id,
        summary: None,
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
    forge: &dyn Forge,
    _branch_info: &git::BranchInfo,
    pr: Draft,
) -> Result<Draft> {
    let initial_body = template::build_editor_body(config, &forge.templates(), pr.ticket.as_ref());
    let body = ui::prompt_body(&initial_body)?;

    let reviewers_list = github::get_available_reviewers()
        .unwrap_or_else(|_| config.github.default_reviewers.clone());
    let reviewers = ui::prompt_reviewers(reviewers_list)?;

    Ok(Draft {
        body,
        reviewers,
        ..pr
    })
}

fn update_related_prs(
    config: &Config,
    forge: &dyn Forge,
    branch_info: &git::BranchInfo,
    pr: &Draft,
    created: Option<&PrRef>,
    dry_run: bool,
) -> Result<()> {
    let Some(current_tag) = pr.ticket.as_ref().map(|t| t.id.as_str()) else {
        println!(
            "{} No PR tag; skipping related PR updates.",
            ">".bright_green()
        );
        return Ok(());
    };

    let all_prs = forge.open_prs_by(config.github_user().as_deref())?;
    let tags = collect_stack_tags(&all_prs, branch_info, current_tag);
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
        let updated_body =
            template::replace_related_prs(config, &related_pr.body, &related_pr.pr, &related_prs);

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

fn collect_stack_tags(
    all_prs: &[RemotePr],
    branch_info: &git::BranchInfo,
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

    fn make_branch_info(current: &str, bases: &[&str]) -> git::BranchInfo {
        git::BranchInfo {
            current_branch: current.to_string(),
            bases: bases.iter().map(|s| s.to_string()).collect(),
            commits: vec![],
        }
    }

    #[test]
    fn test_jira_ticket_links_when_jira_url_set() {
        let mut config = Config::default();
        config.jira.url = Some("https://jira.example.com/browse/".to_string());

        let ticket = jira_ticket(&config, "TRACK-123".to_string());

        assert_eq!(ticket.id, "TRACK-123");
        assert_eq!(
            ticket.url.as_deref(),
            Some("https://jira.example.com/browse/TRACK-123")
        );
    }

    #[test]
    fn test_jira_ticket_without_jira_url() {
        let ticket = jira_ticket(&Config::default(), "TRACK-123".to_string());

        assert_eq!(ticket.url, None);
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
