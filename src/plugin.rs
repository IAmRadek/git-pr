//! Plugin extension points for git-pr.
//!
//! Plugins are compiled in. Each step of the PR flow is a trait, and the
//! built-in behavior implements the same traits as optional plugins.

use std::path::PathBuf;

use colored::Colorize;

use crate::config::{Config, ForgeKind};
use crate::error::{Error, Result};
use crate::git::{self, BranchInfo};
use crate::github::{GhForge, GhReviewers};
use crate::related::RelatedPrs;

/// State shared by every plugin during one run
pub struct Context {
    pub branch: BranchInfo,
    pub config: Config,
}

/// The ticket that a PR belongs to
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Ticket {
    /// The ticket identifier (e.g., "TRACK-123")
    pub id: String,
    /// A link to the ticket in its tracker
    pub url: Option<String>,
    /// The ticket title from its tracker
    pub summary: Option<String>,
}

/// A Pull Request being created
#[derive(Debug, Default, Clone)]
pub struct Draft {
    /// The ticket this PR belongs to
    pub ticket: Option<Ticket>,
    /// The title of the PR (e.g., "[TRACK-123]: Add new feature")
    pub title: String,
    /// The full PR body as authored in the editor
    pub body: String,
    /// The base branch to merge into
    pub base: String,
    /// List of usernames to request review from
    pub reviewers: Vec<String>,
    /// Labels to add to the PR
    pub labels: Vec<String>,
}

/// Identifies a PR on a forge
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PrRef {
    /// The repository as "owner/repo"
    pub repo: String,
    /// The PR number within the repository
    pub number: u32,
}

/// A PR that exists on a forge
#[derive(Debug, Clone)]
pub struct RemotePr {
    pub pr: PrRef,
    pub title: String,
    pub body: String,
    /// The head (source) branch name
    pub head: String,
    /// The base (target) branch name
    pub base: String,
}

/// Finds the ticket for the current branch.
/// The first source that returns a ticket wins.
pub trait TicketSource {
    fn detect(&self, ctx: &Context) -> Result<Option<Ticket>>;

    /// Tickets to offer when no source detects one
    fn suggest(&self, _ctx: &Context) -> Result<Vec<Ticket>> {
        Ok(vec![])
    }
}

/// Changes the draft before and after the user edits it.
/// All hooks run, in configuration order.
pub trait DraftHook {
    fn before_edit(&self, _ctx: &Context, _draft: &mut Draft) -> Result<()> {
        Ok(())
    }

    fn after_edit(&self, _ctx: &Context, _draft: &mut Draft) -> Result<()> {
        Ok(())
    }
}

/// Suggests reviewers. The results of all sources are merged.
pub trait ReviewerSource {
    fn reviewers(&self, ctx: &Context, draft: &Draft) -> Result<Vec<String>>;
}

/// The service that hosts the PRs. Exactly one forge is active.
pub trait Forge {
    /// Creates the PR. Returns `None` on a dry run.
    fn create(&self, draft: &Draft, dry_run: bool) -> Result<Option<PrRef>>;

    fn get(&self, pr: &PrRef) -> Result<RemotePr>;

    /// The open PR for the current branch
    fn current_pr(&self) -> Result<Option<RemotePr>>;

    /// Open PRs by the configured user, or by the authenticated user
    fn open_prs(&self) -> Result<Vec<RemotePr>>;

    fn update_body(&self, pr: &PrRef, body: &str, dry_run: bool) -> Result<()>;

    /// PR template files in the repository that exist, highest priority first
    fn templates(&self) -> Vec<PathBuf>;

    /// The text that refers to `pr` in a PR body
    fn link(&self, pr: &PrRef) -> String;
}

/// Runs after the PR step. `created` is `None` on a dry run and with `--update-only`.
pub trait AfterCreate {
    fn run(
        &self,
        ctx: &Context,
        forge: &dyn Forge,
        draft: &Draft,
        created: Option<&PrRef>,
        dry_run: bool,
    ) -> Result<()>;
}

/// The plugins active for one run
pub struct Registry {
    pub forge: Box<dyn Forge>,
    pub reviewer_source: Option<Box<dyn ReviewerSource>>,
    /// Run in order after the PR step
    pub after_create: Vec<Box<dyn AfterCreate>>,
}

impl Registry {
    /// Chooses the plugins for the forge that hosts `origin_url`
    pub fn for_origin(config: &Config, origin_url: &str) -> Result<Self> {
        let host = git::url_host(origin_url).ok_or_else(|| {
            Error::Config(format!("Cannot read the host of origin {}", origin_url))
        })?;

        let mut registry = match config.forge_kind(host) {
            Some(ForgeKind::Github) => Self::github(config),
            None => {
                return Err(Error::Config(format!(
                    "No forge for host {}. Add it under `forges` in config.yaml",
                    host
                )))
            }
        };

        registry.after_create.push(Box::new(RelatedPrs));
        Ok(registry)
    }

    fn github(config: &Config) -> Self {
        Self {
            forge: Box::new(GhForge {
                user: config.github_user(),
            }),
            reviewer_source: Some(Box::new(GhReviewers)),
            after_create: vec![],
        }
    }

    /// Runs the `AfterCreate` plugins. A failing plugin prints a warning.
    pub fn after_create(
        &self,
        ctx: &Context,
        draft: &Draft,
        created: Option<&PrRef>,
        dry_run: bool,
    ) {
        for plugin in &self.after_create {
            if let Err(err) = plugin.run(ctx, self.forge.as_ref(), draft, created, dry_run) {
                warn(&err);
            }
        }
    }

    /// Reviewers to offer. Without a source, or when the source fails,
    /// prints a warning and returns none.
    pub fn reviewers(&self, ctx: &Context, draft: &Draft) -> Vec<String> {
        let Some(source) = &self.reviewer_source else {
            println!(
                "{} No reviewer source configured; skipping reviewers.",
                "!".bright_red()
            );
            return vec![];
        };

        source.reviewers(ctx, draft).unwrap_or_else(|err| {
            warn(&err);
            vec![]
        })
    }
}

fn warn(err: &Error) {
    println!("{} Plugin failed: {}", "!".yellow(), err);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    struct Fixed(Vec<&'static str>);

    impl ReviewerSource for Fixed {
        fn reviewers(&self, _ctx: &Context, _draft: &Draft) -> Result<Vec<String>> {
            Ok(self.0.iter().map(|r| r.to_string()).collect())
        }
    }

    struct Failing;

    impl ReviewerSource for Failing {
        fn reviewers(&self, _ctx: &Context, _draft: &Draft) -> Result<Vec<String>> {
            Err(Error::Config("broken".into()))
        }
    }

    fn registry(reviewer_source: Option<Box<dyn ReviewerSource>>) -> Registry {
        Registry {
            reviewer_source,
            ..Registry::github(&Config::default())
        }
    }

    fn ctx() -> Context {
        Context {
            branch: BranchInfo {
                current_branch: "feature".into(),
                bases: vec![],
                commits: vec![],
            },
            config: Config::default(),
        }
    }

    struct Recording(Rc<Cell<bool>>);

    impl AfterCreate for Recording {
        fn run(
            &self,
            _ctx: &Context,
            _forge: &dyn Forge,
            _draft: &Draft,
            _created: Option<&PrRef>,
            _dry_run: bool,
        ) -> Result<()> {
            self.0.set(true);
            Ok(())
        }
    }

    struct FailingAfter;

    impl AfterCreate for FailingAfter {
        fn run(
            &self,
            _ctx: &Context,
            _forge: &dyn Forge,
            _draft: &Draft,
            _created: Option<&PrRef>,
            _dry_run: bool,
        ) -> Result<()> {
            Err(Error::Config("broken".into()))
        }
    }

    #[test]
    fn test_after_create_runs_past_a_failing_plugin() {
        let ran = Rc::new(Cell::new(false));
        let mut registry = registry(None);
        registry.after_create = vec![Box::new(FailingAfter), Box::new(Recording(ran.clone()))];

        registry.after_create(&ctx(), &Draft::default(), None, true);

        assert!(ran.get());
    }

    #[test]
    fn test_reviewers_from_source() {
        let registry = registry(Some(Box::new(Fixed(vec!["ann", "bob"]))));

        let reviewers = registry.reviewers(&ctx(), &Draft::default());

        assert_eq!(reviewers, vec!["ann", "bob"]);
    }

    #[test]
    fn test_failing_reviewer_source_gives_none() {
        let registry = registry(Some(Box::new(Failing)));

        assert!(registry.reviewers(&ctx(), &Draft::default()).is_empty());
    }

    #[test]
    fn test_no_reviewer_source_gives_none() {
        let registry = registry(None);

        assert!(registry.reviewers(&ctx(), &Draft::default()).is_empty());
    }

    #[test]
    fn test_for_origin_github() {
        let registry = Registry::for_origin(&Config::default(), "git@github.com:o/r.git");

        assert!(registry.is_ok());
    }

    #[test]
    fn test_for_origin_unknown_host() {
        let registry = Registry::for_origin(&Config::default(), "git@gitlab.example.com:o/r.git");

        assert!(registry.is_err());
    }
}
