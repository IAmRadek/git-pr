use std::process;

use clap::Parser;
use colored::Colorize;
use inquire::error::InquireError;
use inquire::list_option::ListOption;
use inquire::ui::{Color, RenderConfig, Styled};
use inquire::validator::Validation;
use inquire::{set_global_render_config, CustomUserError, Editor, MultiSelect, Select, Text};

use tags::tags::Tags;

use crate::errors::Error;

mod cli;
mod config;
mod errors;
mod git;
mod github;
mod jira;
mod tags;
mod template;

#[derive(Debug, Default)]
struct PR {
    title: String,
    tag: String,
    is_jira: bool,
    this_pr: String,
    impl_and_considerations: String,
    reviewers: Vec<String>,
    base: String,
}

fn main() {
    let args = cli::Args::parse();

    let mut style = RenderConfig::default_colored();
    style.prompt_prefix = Styled::new(">").with_fg(Color::LightGreen);
    set_global_render_config(style);

    let mut pr = PR::default();

    let branch_info = match git::get_branch_bases_and_commits() {
        Ok(b) => b,
        Err(err) => {
            match err {
                Error::NotInGitRepo => {
                    println!("Expected to be run in git repository.");
                }
                Error::BranchNotClean => {
                    println!("Branch is not clean. Please commit or stash changes.");
                }
                Error::CannotBeInMainBranch(m) => {
                    println!("Can't be in main branch: {}", m.bright_cyan());
                }
            }
            process::exit(1);
        }
    };
    if branch_info.commits.is_empty() {
        println!("No commits found. Exiting...");
        process::exit(1);
    }

    let mut tags = Tags::from_file(config::get_tags_path()).unwrap();

    let found_tag = tags::tags::extract_from_vec(branch_info.commits.clone());
    if found_tag.is_some() {
        let (tag, commit) = found_tag.unwrap();

        tags.add_and_save(tag.clone()).unwrap();

        pr.tag = tag;
        pr.title = commit;
        pr.is_jira = true; // TODO: check if it's jira

        println!(
            "{} PR title: {}",
            ">".bright_green(),
            pr.title.bright_cyan()
        );
        println!("{} PR Tag: {}", ">".bright_green(), pr.tag.bright_cyan());
    } else {
        let title = Text::new("PR title: ")
            .with_default(branch_info.commits.last().unwrap())
            .with_autocomplete(branch_info.clone())
            .prompt()
            .unwrap();

        let selected_tag = if tags.is_empty() {
            match Text::new("PR Tag:")
                .with_validator(Tags::validator)
                .prompt()
            {
                Ok(tag) => tag,
                Err(err) => {
                    match err {
                        InquireError::OperationInterrupted => {}
                        _ => println!("Something went wrong {:?}", err),
                    }
                    process::exit(1);
                }
            }
        } else {
            match Text::new("PR Tag:")
                .with_autocomplete(tags.clone())
                .with_default(tags.clone().iter().first().unwrap())
                .prompt()
            {
                Ok(tag) => tag,
                Err(err) => {
                    match err {
                        InquireError::OperationInterrupted => {}
                        _ => println!("Something went wrong {:?}", err),
                    }
                    process::exit(1);
                }
            }
        };
        tags.add(selected_tag.clone());
        tags.save().unwrap();

        pr.tag = selected_tag;
        pr.title = format!("[{}]: {}", pr.tag, title);
    }

    pr.base = if branch_info.bases.len() > 1 {
        Select::new("PR base:", branch_info.bases).prompt().unwrap()
    } else {
        let base = branch_info.bases[0].clone();
        println!("{} PR base: {}", ">".bright_green(), base.bright_cyan());
        base
    };

    if !args.update_only {
        pr.this_pr = match Editor::new("What is this PR doing: ")
            .with_formatter(&|x| -> String { x.to_string() })
            .prompt()
        {
            Ok(pr_body) => pr_body,
            Err(err) => {
                match err {
                    InquireError::OperationInterrupted => {}
                    _ => println!("Something went wrong {:?}", err),
                }
                process::exit(1);
            }
        };
        pr.impl_and_considerations = match Editor::new("Considerations and implementation: ")
            .with_formatter(&|x| -> String { x.to_string() })
            .prompt()
        {
            Ok(pr_body) => pr_body,
            Err(err) => {
                match err {
                    InquireError::OperationInterrupted => {}
                    _ => println!("Something went wrong {:?}", err),
                }
                process::exit(1);
            }
        };

        pr.reviewers =
            match MultiSelect::new("Reviewers:", github::get_available_reviewers().unwrap())
                .with_validator(
                    |a: &[ListOption<&String>]| -> Result<Validation, CustomUserError> {
                        if a.is_empty() {
                            return Ok(Validation::Invalid("Select at least one reviewer".into()));
                        }
                        Ok(Validation::Valid)
                    },
                )
                .with_formatter(&|a| -> String {
                    let selected: Vec<String> =
                        a.iter().map(|x| -> String { x.to_string() }).collect();
                    selected.join(", ")
                })
                .prompt()
            {
                Ok(ans) => ans,
                Err(err) => {
                    match err {
                        InquireError::OperationInterrupted => {}
                        _ => println!("Something went wrong {:?}", err),
                    }
                    process::exit(1);
                }
            };

        let body = template::make_body(
            &pr.tag,
            &pr.is_jira,
            &pr.this_pr,
            &pr.impl_and_considerations,
        );

        match github::publish_pr(pr.base, pr.title, body, pr.reviewers, args.dry_run) {
            Ok(url) => {
                println!("Published at: {}", url)
            }
            Err(err) => {
                println!("Something went wrong: {}", err);
                process::exit(1)
            }
        }
    }

    let related_prs = match github::get_user_prs() {
        Ok(prs) => {
            let mut ret: Vec<github::PullRequest> = vec![];
            for each in prs.into_iter() {
                if !each.title.contains(&pr.tag) {
                    continue;
                }
                match tags::tags::extract_from_str(each.title.as_str()) {
                    None => {
                        println!(
                            "{} {} {}",
                            "x".bright_red(),
                            each.title.bright_cyan(),
                            "No tag found".bright_red()
                        );
                    }
                    Some(tag) => {
                        if tag.eq(pr.tag.as_str()) {
                            ret.push(each)
                        }
                    }
                }
            }
            ret
        }
        Err(err) => {
            println!("Something went wrong: {:?}", err);
            process::exit(1);
        }
    };

    if related_prs.is_empty() {
        println!("{} No related prs found. Exiting...", ">".bright_green());
        return;
    }
    println!(
        "{} Found {} related prs. Updating... :)",
        ">".bright_green(),
        related_prs.len()
    );

    for pr in &related_prs {
        let updated_body = template::replace_related_prs(&pr.body, &pr.number, &related_prs);

        match github::update_pr(&pr.number, &pr.resource_path, updated_body, args.dry_run) {
            Ok(e) => {
                println!("{} Updated #{}: {}", "+".bright_green(), pr.number, e);
            }
            Err(err) => {
                println!("{} Updated #{} failed: {}", "x".red(), pr.number, err)
            }
        }
    }
}
