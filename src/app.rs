use colored::Colorize;

use crate::config::{self, Config};
use crate::error::{Error, Result};
use crate::git;
use crate::github;
use crate::pr::PullRequest;
use crate::tags::Tags;
use crate::template;
use crate::ui;

/// Main application entry point
pub fn run(args: crate::cli::Args) -> Result<()> {
    ui::init_render_config();

    // Ensure config directory exists and load configuration
    config::ensure_config_dir_exists(std::path::Path::new(&args.config));
    let config = Config::load(&args.config)?;

    let branch_info = git::get_branch_bases_and_commits()?;

    if branch_info.commits.is_empty() {
        return Err(Error::NoCommits);
    }

    let tags_path = config::get_tags_path_with_dir(&args.config);
    let mut tags = Tags::from_file(tags_path)?;
    let mut pr = build_pr_from_branch(&branch_info, &mut tags)?;

    pr.base = select_base_branch(&branch_info)?;

    if !args.update_only {
        pr = gather_pr_details(pr)?;
        publish_pr(&config, &pr, args.dry_run)?;
    }

    update_related_prs(&config, &pr, args.dry_run)?;

    Ok(())
}

/// Build initial PR info from branch and commit information
fn build_pr_from_branch(branch_info: &git::BranchInfo, tags: &mut Tags) -> Result<PullRequest> {
    let found_tag = crate::tags::extract_from_vec(branch_info.commits.clone());

    if let Some((tag, commit)) = found_tag {
        tags.add_and_save(tag.clone())?;

        println!("{} PR title: {}", ">".bright_green(), commit.bright_cyan());
        println!("{} PR Tag: {}", ">".bright_green(), tag.bright_cyan());

        Ok(PullRequest::new()
            .with_tag(tag)
            .with_title(commit)
            .with_jira(true)) // TODO: check if it's actually jira
    } else {
        let title = ui::prompt_title(branch_info)?;
        let selected_tag = ui::prompt_tag(tags)?;

        tags.add(selected_tag.clone());
        tags.save()?;

        let full_title = format!("[{}]: {}", selected_tag, title);

        Ok(PullRequest::new()
            .with_tag(selected_tag)
            .with_title(full_title)
            .with_jira(false))
    }
}

/// Select the base branch for the PR
fn select_base_branch(branch_info: &git::BranchInfo) -> Result<String> {
    if branch_info.bases.len() > 1 {
        ui::prompt_base(branch_info.bases.clone())
    } else {
        let base = branch_info.bases[0].clone();
        println!("{} PR base: {}", ">".bright_green(), base.bright_cyan());
        Ok(base)
    }
}

/// Gather PR description, implementation details, and reviewers
fn gather_pr_details(pr: PullRequest) -> Result<PullRequest> {
    let description = ui::prompt_description("What is this PR doing:")?;
    let implementation = ui::prompt_description("Considerations and implementation:")?;

    let reviewers_list = github::get_available_reviewers().unwrap_or_default();
    let reviewers = ui::prompt_reviewers(reviewers_list)?;

    Ok(pr
        .with_description(description)
        .with_implementation(implementation)
        .with_reviewers(reviewers))
}

/// Publish the PR to GitHub
fn publish_pr(config: &Config, pr: &PullRequest, dry_run: bool) -> Result<()> {
    let body = template::make_body(
        config,
        &pr.tag,
        &pr.is_jira,
        &pr.description,
        &pr.implementation,
    );

    match github::publish_pr(
        pr.base.clone(),
        pr.title.clone(),
        body,
        pr.reviewers.clone(),
        dry_run,
    ) {
        Ok(url) => {
            println!("Published at: {}", url);
            Ok(())
        }
        Err(err) => Err(Error::GitHubCli(err)),
    }
}

/// Find and update related PRs with the same tag
fn update_related_prs(config: &Config, pr: &PullRequest, dry_run: bool) -> Result<()> {
    let related_prs = match github::get_user_prs(config.github_user().as_deref()) {
        Ok(prs) => filter_related_prs(prs, &pr.tag),
        Err(err) => {
            return Err(Error::GitHubCli(err));
        }
    };

    if related_prs.is_empty() {
        println!("{} No related PRs found.", ">".bright_green());
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

/// Filter PRs to only those matching the given tag
fn filter_related_prs(prs: Vec<github::PullRequest>, tag: &str) -> Vec<github::PullRequest> {
    prs.into_iter()
        .filter(|pr| {
            if !pr.title.contains(tag) {
                return false;
            }

            match crate::tags::extract_from_str(&pr.title) {
                Some(extracted_tag) => extracted_tag == tag,
                None => {
                    println!(
                        "{} {} {}",
                        "x".bright_red(),
                        pr.title.bright_cyan(),
                        "No tag found".bright_red()
                    );
                    false
                }
            }
        })
        .collect()
}
