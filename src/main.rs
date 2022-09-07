use std::any::Any;
use std::process;

use clap::Parser;
use colored::Colorize;
use git2::{Repository, RepositoryState};
use inquire::{Editor, MultiSelect, set_global_render_config, Text};
use inquire::error::InquireError;
use inquire::ui::{Color, RenderConfig, Styled};

mod github;
mod git;
mod template;
mod jira;


#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, value_parser, default_value_t = false)]
    update_only: bool,
}

fn main() {
    let args = Args::parse();

    let mut style = RenderConfig::default_colored();
    style.prompt_prefix = Styled::new(">").with_fg(Color::LightGreen);
    set_global_render_config(style);

    let (base, jira_ticket, title) = {
        let repo = match Repository::open(".") {
            Ok(repo) => repo,
            Err(_) => {
                println!("Expected to be run in git repository.");
                process::exit(1);
            }
        };
        if repo.state() != RepositoryState::Clean {
            println!("Commit changes first.");
            process::exit(1)
        }

        let head = repo.head().unwrap();
        if is_main(head.shorthand().unwrap()) {
            println!("Can't work in {} branch.", head.shorthand().unwrap());
            process::exit(1);
        }

        let branch_commits = git::get_branch_commits(&repo, head.shorthand().unwrap());
        let (base, commits) = match branch_commits {
            Ok((base, commits)) => (base, commits),
            Err(e) => {
                println!("Error: {}", e);
                process::exit(1);
            }
        };

        match jira::find_ticket(commits) {
            None => {
                if args.update_only {
                    println!("Jira ticket not found. Can't update prs...");
                    process::exit(1);
                }

                match Text::new("Provide Jira ticket:")
                    .with_validator(&jira::validator)
                    .prompt() {
                    Ok(ticket) => {
                        let title = match Text::new("Enter PR title: ")
                            .with_validator(&jira::validator)
                            .prompt() {
                            Ok(title) => title,
                            Err(err) => {
                                match err {
                                    InquireError::OperationInterrupted => {}
                                    _ => println!("Something went wrong {:?}", err),
                                }
                                process::exit(1);
                            }
                        };
                        println!("{} Base branch: {}", ">".bright_green(), base.bright_cyan());

                        (base, ticket.clone(), format!("[{}] : {}", ticket, title))
                    }
                    Err(err) => {
                        match err {
                            InquireError::OperationInterrupted => {}
                            _ => println!("Something went wrong {:?}", err),
                        }
                        process::exit(1);
                    }
                }
            }
            Some((ticket, title)) => {
                println!("{} Jira ticket: {}", ">".bright_green(), ticket.bright_cyan());
                println!("{} PR title: {}", ">".bright_green(), title.bright_cyan());
                println!("{} Base branch: {}", ">".bright_green(), base.bright_cyan());
                (base, ticket, title)
            }
        }
    };

    if !args.update_only {
        let this_pr = match Editor::new("What is this PR doing: ")
            .with_formatter(&|x| -> String { x.to_string() })
            .prompt() {
            Ok(pr_body) => pr_body,
            Err(err) => {
                match err {
                    InquireError::OperationInterrupted => {}
                    _ => println!("Something went wrong {:?}", err),
                }
                process::exit(1);
            }
        };
        let implementation = match Editor::new("Considerations and implementation: ")
            .with_formatter(&|x| -> String { x.to_string() })
            .prompt() {
            Ok(pr_body) => pr_body,
            Err(err) => {
                match err {
                    InquireError::OperationInterrupted => {}
                    _ => println!("Something went wrong {:?}", err),
                }
                process::exit(1);
            }
        };


        let reviewers = match MultiSelect::new("Reviewers:", github::get_available_reviewers().unwrap())
            .with_validator(&|a| {
                if a.len() < 1 {
                    return Err("Select at least one reviewer".into());
                }
                Ok(())
            })
            .with_formatter(&|a| -> String {
                let selected: Vec<String> = a.iter().map(|x| -> String{ x.to_string() }).collect();
                format!("{}", selected.join(", "))
            })
            .prompt() {
            Ok(ans) => { ans }
            Err(err) => {
                match err {
                    InquireError::OperationInterrupted => {}
                    _ => println!("Something went wrong {:?}", err),
                }
                process::exit(1);
            }
        };

        let body = template::make_body(jira_ticket.clone(), this_pr, implementation);

        match github::publish_pr(base, title, body, reviewers) {
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

            for pr in prs.into_iter() {
                if let Some(m) = jira::PATTERN.find(pr.title.as_str()) {
                    if m.as_str().eq(jira_ticket.as_str()) {
                        ret.push(pr)
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

    if related_prs.len() == 0 {
        return;
    }
    println!("{} Found {} related prs. Updating... :)", ">".bright_green(), related_prs.len());

    for pr in &related_prs {
        let updated_body = template::replace_related_prs(&pr.body, &pr.number, &related_prs);

        match github::update_pr(&pr.number, &pr.resource_path, updated_body) {
            Ok(e) => {
                println!("{} Updated #{}: {}", "+".bright_green(), pr.number, e);
            }
            Err(err) => {
                println!("{} Updated #{} failed: {}", "x".red(), pr.number, err)
            }
        }
    }
}

fn is_main(name: &str) -> bool {
    return name == "main" || name == "master";
}

