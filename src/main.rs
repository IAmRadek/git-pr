use std::process;

use clap::Parser;
use colored::Colorize;

fn main() {
    let args = git_pr::cli::Args::parse();

    if let Err(err) = git_pr::app::run(args) {
        match err {
            git_pr::Error::Cancelled => {
                // User cancelled, exit silently
            }
            git_pr::Error::NotInGitRepo => {
                eprintln!(
                    "{} Expected to be run in a git repository.",
                    "Error:".bright_red()
                );
            }
            git_pr::Error::BranchNotClean => {
                eprintln!(
                    "{} Branch is not clean. Please commit or stash changes.",
                    "Error:".bright_red()
                );
            }
            git_pr::Error::CannotBeInMainBranch(branch) => {
                eprintln!(
                    "{} Cannot run from main branch: {}",
                    "Error:".bright_red(),
                    branch.bright_cyan()
                );
            }
            git_pr::Error::NoCommits => {
                eprintln!(
                    "{} No commits found on current branch.",
                    "Error:".bright_red()
                );
            }
            _ => {
                eprintln!("{} {}", "Error:".bright_red(), err);
            }
        }
        process::exit(1);
    }
}
