//! Plugin extension points for git-pr.
//!
//! Plugins are compiled in. Each step of the PR flow is a trait, and the
//! built-in behavior implements the same traits as optional plugins.

use std::path::PathBuf;

use crate::config::Config;
use crate::error::Result;
use crate::git::BranchInfo;
use crate::github::GhForge;

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

    /// Open PRs by `user`, or by the authenticated user when `user` is `None`
    fn open_prs_by(&self, user: Option<&str>) -> Result<Vec<RemotePr>>;

    fn update_body(&self, pr: &PrRef, body: &str, dry_run: bool) -> Result<()>;

    /// PR template files in the repository that exist, highest priority first
    fn templates(&self) -> Vec<PathBuf>;
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
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            forge: Box::new(GhForge),
        }
    }
}
