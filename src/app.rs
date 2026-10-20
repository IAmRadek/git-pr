use colored::Colorize;

use crate::config::{self, Config};
use crate::error::{Error, Result};
use crate::git;
use crate::plugin::{Context, Draft, PrRef, Registry, Ticket};
use crate::tags::{jira_ticket, Tags};
use crate::ui;

pub fn run(args: crate::cli::Args) -> Result<()> {
    ui::init_render_config();

    config::ensure_config_dir_exists(std::path::Path::new(&args.config));
    let config = Config::load(&args.config)?;

    let branch_info = git::get_branch_bases_and_commits()?;

    if branch_info.commits.is_empty() {
        return Err(Error::NoCommits);
    }

    let registry = Registry::for_origin(&config, &git::origin_url()?)?;
    let forge = registry.forge.as_ref();

    let ctx = Context {
        branch: branch_info,
        config,
    };

    let tags_path = config::get_tags_path_with_dir(&args.config);
    let mut tags = Tags::from_file(tags_path)?;

    // In --update-only mode there are no new commits with a tag, so look up the
    // existing PR on the forge and pull the tag from its title instead.
    let remote_tag = if args.update_only {
        forge
            .current_pr()
            .ok()
            .flatten()
            .and_then(|p| crate::tags::extract_from_str(&p.title))
    } else {
        None
    };

    let mut pr = build_pr_from_branch(&ctx, &registry, &mut tags, remote_tag, args.update_only)?;

    pr.base = select_base_branch(&ctx.branch)?;

    let mut created: Option<PrRef> = None;

    if !args.update_only {
        pr = gather_pr_details(&ctx, &registry, pr)?;
        created = forge.create(&pr, args.dry_run)?;
    }

    registry.after_create(&ctx, &pr, created.as_ref(), args.dry_run);

    Ok(())
}

fn build_pr_from_branch(
    ctx: &Context,
    registry: &Registry,
    tags: &mut Tags,
    remote_tag: Option<String>,
    update_only: bool,
) -> Result<Draft> {
    if let Some(tag) = remote_tag {
        tags.add_and_save(tag.clone())?;
        println!("{} PR Tag: {}", ">".bright_green(), tag.bright_cyan());
        return Ok(Draft {
            ticket: Some(jira_ticket(&ctx.config, tag)),
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

    let (ticket, title) = match registry.detect_ticket(ctx) {
        Some(ticket) => {
            let title = pr_title(&ctx.branch, Some(&ticket));
            println!("{} PR title: {}", ">".bright_green(), title.bright_cyan());
            println!("{} PR Tag: {}", ">".bright_green(), ticket.id.bright_cyan());
            (Some(ticket), title)
        }
        None => {
            // Recent tags first, then the tickets that plugins suggest
            let mut suggestions: Vec<String> = tags.iter().cloned().collect();
            for ticket in registry.suggest_tickets(ctx) {
                if !suggestions.contains(&ticket.id) {
                    suggestions.push(ticket.id);
                }
            }

            let ticket = ui::prompt_tag(suggestions)?.map(|id| Ticket {
                id,
                ..Ticket::default()
            });
            let title = pr_title(&ctx.branch, ticket.as_ref());
            println!("{} PR title: {}", ">".bright_green(), title.bright_cyan());
            (ticket, title)
        }
    };

    if let Some(ticket) = &ticket {
        tags.add_and_save(ticket.id.clone())?;
    }

    Ok(Draft {
        ticket,
        title,
        ..Draft::default()
    })
}

/// The commit that carries `[ID]`, else `[ID]: <default title>`
fn pr_title(branch: &git::BranchInfo, ticket: Option<&Ticket>) -> String {
    let default_title = ui::default_pr_title(branch);

    let Some(ticket) = ticket else {
        return default_title;
    };

    let marker = format!("[{}]", ticket.id);
    branch
        .commits
        .iter()
        .find(|commit| commit.contains(&marker))
        .cloned()
        .unwrap_or_else(|| format!("{}: {}", marker, default_title))
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

fn gather_pr_details(ctx: &Context, registry: &Registry, mut pr: Draft) -> Result<Draft> {
    registry.before_edit(ctx, &mut pr);
    pr.body = ui::prompt_body(&pr.body)?;
    registry.after_edit(ctx, &mut pr);

    pr.reviewers = ui::prompt_reviewers(registry.reviewers(ctx, &pr))?;

    Ok(pr)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch(commits: &[&str]) -> git::BranchInfo {
        git::BranchInfo {
            current_branch: "feature".into(),
            bases: vec![],
            commits: commits.iter().map(|c| c.to_string()).collect(),
        }
    }

    fn ticket(id: &str) -> Ticket {
        Ticket {
            id: id.to_string(),
            ..Ticket::default()
        }
    }

    #[test]
    fn test_pr_title_uses_commit_with_ticket() {
        let branch = branch(&["newest", "[TRACK-1]: tagged", "oldest"]);

        assert_eq!(
            pr_title(&branch, Some(&ticket("TRACK-1"))),
            "[TRACK-1]: tagged"
        );
    }

    #[test]
    fn test_pr_title_prefixes_oldest_commit_for_typed_ticket() {
        let branch = branch(&["newest", "oldest"]);

        assert_eq!(
            pr_title(&branch, Some(&ticket("TRACK-1"))),
            "[TRACK-1]: oldest"
        );
    }

    #[test]
    fn test_pr_title_without_ticket() {
        let branch = branch(&["newest", "oldest"]);

        assert_eq!(pr_title(&branch, None), "oldest");
    }
}
