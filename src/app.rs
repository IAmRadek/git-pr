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
    let mut pr = build_pr_from_branch(&branch_info, &mut tags)?;

    pr.base = select_base_branch(&branch_info)?;

    let mut created_pr_number: Option<u32> = None;

    if !args.update_only {
        pr = gather_pr_details(&config, &branch_info, pr)?;
        created_pr_number = publish_pr(&pr, args.dry_run)?;
    }

    update_related_prs(&config, &pr, created_pr_number, args.dry_run)?;

    Ok(())
}

fn build_pr_from_branch(branch_info: &git::BranchInfo, tags: &mut Tags) -> Result<PullRequest> {
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

        tags.add(selected_tag.clone());
        tags.save()?;

        let default_title = ui::default_pr_title(branch_info);
        let full_title = format!("[{}]: {}", selected_tag, default_title);

        println!("{} PR title: {}", ">".bright_green(), full_title.bright_cyan());

        Ok(PullRequest::new()
            .with_tag(selected_tag)
            .with_title(full_title)
            .with_jira(false))
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
    let initial_body = template::build_editor_body(config, &pr.tag, pr.is_jira);
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
    pr: &PullRequest,
    created_pr_number: Option<u32>,
    dry_run: bool,
) -> Result<()> {
    let mut related_prs = match github::get_user_prs(config.github_user().as_deref()) {
        Ok(prs) => filter_related_prs(prs, &pr.tag),
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
