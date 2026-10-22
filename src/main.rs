use std::process;

use clap::Parser;
use colored::Colorize;

fn main() {
    let args = git_pr::cli::Args::parse();

    // Handle --init flag to generate sample config
    if args.init {
        if let Err(err) = init_config(&args.config) {
            eprintln!("{} {}", "Error:".bright_red(), err);
            process::exit(1);
        }
        return;
    }

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

/// Initialize a sample configuration file in the config directory,
/// or add the sections that an existing file is missing
fn init_config(config_dir: &str) -> Result<(), Box<dyn std::error::Error>> {
    use git_pr::config;
    use std::path::PathBuf;

    let config_path = PathBuf::from(config_dir).join("config.yaml");

    // Ensure config directory exists
    config::ensure_config_dir_exists(std::path::Path::new(config_dir));

    if !config_path.exists() {
        std::fs::write(&config_path, config::SAMPLE_CONFIG)?;
        println!(
            "{} Created configuration file at: {}",
            "Success:".bright_green(),
            config_path.display()
        );
        println!("The comments in the file describe each section.");
        return Ok(());
    }

    // The message goes to stderr, so `git-pr --init > file` writes only the sample
    eprintln!(
        "{} Configuration file exists: {}",
        "!".yellow(),
        config_path.display()
    );
    eprintln!("git-pr did not change it. The default configuration is:\n");
    print!("{}", config::SAMPLE_CONFIG);

    Ok(())
}
