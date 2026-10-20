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
use crate::tags::CommitTags;
use crate::template::TemplateBody;

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
    /// Asked in order; the first detected ticket wins
    pub ticket_sources: Vec<Box<dyn TicketSource>>,
    /// Run in order around the editor
    pub draft_hooks: Vec<Box<dyn DraftHook>>,
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

        registry.ticket_sources.push(Box::new(CommitTags));
        // First, so later hooks change the filled template
        registry.draft_hooks.insert(
            0,
            Box::new(TemplateBody {
                repo_templates: registry.forge.templates(),
            }),
        );
        registry.after_create.push(Box::new(RelatedPrs));
        Ok(registry)
    }

    fn github(config: &Config) -> Self {
        Self {
            forge: Box::new(GhForge {
                user: config.github_user(),
            }),
            ticket_sources: vec![],
            draft_hooks: vec![],
            reviewer_source: Some(Box::new(GhReviewers)),
            after_create: vec![],
        }
    }

    /// Runs `DraftHook::before_edit` in order. A failing hook prints a warning.
    pub fn before_edit(&self, ctx: &Context, draft: &mut Draft) {
        for hook in &self.draft_hooks {
            if let Err(err) = hook.before_edit(ctx, draft) {
                warn(&err);
            }
        }
    }

    /// Runs `DraftHook::after_edit` in order. A failing hook prints a warning.
    pub fn after_edit(&self, ctx: &Context, draft: &mut Draft) {
        for hook in &self.draft_hooks {
            if let Err(err) = hook.after_edit(ctx, draft) {
                warn(&err);
            }
        }
    }

    /// The first ticket that a source detects. A failing source prints a warning.
    pub fn detect_ticket(&self, ctx: &Context) -> Option<Ticket> {
        self.ticket_sources.iter().find_map(|source| {
            source.detect(ctx).unwrap_or_else(|err| {
                warn(&err);
                None
            })
        })
    }

    /// Tickets from all sources, without duplicate ids, in source order.
    /// A failing source prints a warning.
    pub fn suggest_tickets(&self, ctx: &Context) -> Vec<Ticket> {
        let mut tickets: Vec<Ticket> = Vec::new();

        for source in &self.ticket_sources {
            match source.suggest(ctx) {
                Ok(found) => {
                    for ticket in found {
                        if !tickets.iter().any(|t| t.id == ticket.id) {
                            tickets.push(ticket);
                        }
                    }
                }
                Err(err) => warn(&err),
            }
        }

        tickets
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

    /// Detects `detected`, suggests `suggested`, or fails both when `detected` is "FAIL"
    struct Tickets {
        detected: Option<&'static str>,
        suggested: Vec<&'static str>,
    }

    fn ticket(id: &str) -> Ticket {
        Ticket {
            id: id.to_string(),
            ..Ticket::default()
        }
    }

    impl TicketSource for Tickets {
        fn detect(&self, _ctx: &Context) -> Result<Option<Ticket>> {
            match self.detected {
                Some("FAIL") => Err(Error::Config("broken".into())),
                other => Ok(other.map(ticket)),
            }
        }

        fn suggest(&self, _ctx: &Context) -> Result<Vec<Ticket>> {
            if self.detected == Some("FAIL") {
                return Err(Error::Config("broken".into()));
            }
            Ok(self.suggested.iter().map(|id| ticket(id)).collect())
        }
    }

    fn with_ticket_sources(sources: Vec<Tickets>) -> Registry {
        let mut registry = registry(None);
        registry.ticket_sources = sources
            .into_iter()
            .map(|s| Box::new(s) as Box<dyn TicketSource>)
            .collect();
        registry
    }

    #[test]
    fn test_detect_ticket_first_source_wins_after_failure() {
        let registry = with_ticket_sources(vec![
            Tickets {
                detected: Some("FAIL"),
                suggested: vec![],
            },
            Tickets {
                detected: None,
                suggested: vec![],
            },
            Tickets {
                detected: Some("A-1"),
                suggested: vec![],
            },
            Tickets {
                detected: Some("B-1"),
                suggested: vec![],
            },
        ]);

        assert_eq!(
            registry.detect_ticket(&ctx()).map(|t| t.id),
            Some("A-1".into())
        );
    }

    #[test]
    fn test_suggest_tickets_merged_without_duplicates() {
        let registry = with_ticket_sources(vec![
            Tickets {
                detected: None,
                suggested: vec!["A-1", "A-2"],
            },
            Tickets {
                detected: Some("FAIL"),
                suggested: vec![],
            },
            Tickets {
                detected: None,
                suggested: vec!["A-2", "B-1"],
            },
        ]);

        let ids: Vec<String> = registry
            .suggest_tickets(&ctx())
            .into_iter()
            .map(|t| t.id)
            .collect();

        assert_eq!(ids, vec!["A-1", "A-2", "B-1"]);
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

    /// Appends its text to the body: lowercase before the editor, uppercase after
    struct Append(&'static str);

    impl DraftHook for Append {
        fn before_edit(&self, _ctx: &Context, draft: &mut Draft) -> Result<()> {
            draft.body.push_str(self.0);
            Ok(())
        }

        fn after_edit(&self, _ctx: &Context, draft: &mut Draft) -> Result<()> {
            draft.body.push_str(&self.0.to_uppercase());
            Ok(())
        }
    }

    struct FailingHook;

    impl DraftHook for FailingHook {
        fn before_edit(&self, _ctx: &Context, _draft: &mut Draft) -> Result<()> {
            Err(Error::Config("broken".into()))
        }

        fn after_edit(&self, _ctx: &Context, _draft: &mut Draft) -> Result<()> {
            Err(Error::Config("broken".into()))
        }
    }

    #[test]
    fn test_draft_hooks_run_in_order_past_a_failing_hook() {
        let mut registry = registry(None);
        registry.draft_hooks = vec![
            Box::new(Append("a")),
            Box::new(FailingHook),
            Box::new(Append("b")),
        ];
        let mut draft = Draft::default();

        registry.before_edit(&ctx(), &mut draft);
        assert_eq!(draft.body, "ab");

        registry.after_edit(&ctx(), &mut draft);
        assert_eq!(draft.body, "abAB");
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
