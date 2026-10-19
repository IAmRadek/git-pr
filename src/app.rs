use colored::Colorize;

use crate::config::{self, Config};
use crate::error::{Error, Result};
use crate::git;
use crate::plugin::{Context, Draft, PrRef, Registry, Ticket};
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

    let mut pr = build_pr_from_branch(
        &ctx.config,
        &ctx.branch,
        &mut tags,
        remote_tag,
        args.update_only,
    )?;

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
    config: &Config,
    branch_info: &git::BranchInfo,
    tags: &mut Tags,
    remote_tag: Option<String>,
    update_only: bool,
) -> Result<Draft> {
    if let Some(tag) = remote_tag {
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

fn gather_pr_details(ctx: &Context, registry: &Registry, pr: Draft) -> Result<Draft> {
    let initial_body =
        template::build_editor_body(&ctx.config, &registry.forge.templates(), pr.ticket.as_ref());
    let pr = Draft {
        body: ui::prompt_body(&initial_body)?,
        ..pr
    };

    let reviewers = ui::prompt_reviewers(registry.reviewers(ctx, &pr))?;

    Ok(Draft { reviewers, ..pr })
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
