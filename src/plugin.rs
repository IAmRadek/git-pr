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
use crate::gitlab::{GlForge, GlReviewers};
use crate::jira::JiraTickets;
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
    /// The name in the `plugins` configuration and in warnings
    fn name(&self) -> &'static str;

    fn detect(&self, ctx: &Context) -> Result<Option<Ticket>>;

    /// Tickets to offer when no source detects one
    fn suggest(&self, _ctx: &Context) -> Result<Vec<Ticket>> {
        Ok(vec![])
    }
}

/// Changes the draft before and after the user edits it.
/// All hooks run, in configuration order.
pub trait DraftHook {
    /// The name in the `plugins` configuration and in warnings
    fn name(&self) -> &'static str;

    fn before_edit(&self, _ctx: &Context, _draft: &mut Draft) -> Result<()> {
        Ok(())
    }

    fn after_edit(&self, _ctx: &Context, _draft: &mut Draft) -> Result<()> {
        Ok(())
    }
}

/// Suggests reviewers. The origin host chooses one source or none.
pub trait ReviewerSource {
    /// The name in warnings
    fn name(&self) -> &'static str;

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
    /// The name in the `plugins` configuration and in warnings
    fn name(&self) -> &'static str;

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
            Some(ForgeKind::Gitlab) => Self::gitlab(config),
            None => {
                return Err(Error::Config(format!(
                    "No forge for host {}. Add it under `forges` in config.yaml",
                    host
                )))
            }
        };

        registry.add_configured(config);
        Ok(registry)
    }

    /// Adds the plugins named in `plugins`, in order. An unknown name, or a
    /// plugin with a wrong configuration, prints a warning and is skipped.
    fn add_configured(&mut self, config: &Config) {
        let plugins = &config.plugins;

        for name in names(&plugins.ticket_sources, DEFAULT_TICKET_SOURCES) {
            match name.as_str() {
                "commit_tags" => self.ticket_sources.push(Box::new(CommitTags)),
                "jira" => match JiraTickets::from_config(config) {
                    Ok(jira) => self.ticket_sources.push(Box::new(jira)),
                    Err(err) => warn("jira", &err),
                },
                other => unknown_plugin("ticket_sources", other),
            }
        }

        for name in names(&plugins.draft_hooks, DEFAULT_DRAFT_HOOKS) {
            match name.as_str() {
                "template_body" => self.draft_hooks.push(Box::new(TemplateBody {
                    repo_templates: self.forge.templates(),
                })),
                other => unknown_plugin("draft_hooks", other),
            }
        }

        for name in names(&plugins.after_create, DEFAULT_AFTER_CREATE) {
            match name.as_str() {
                "related_prs" => self.after_create.push(Box::new(RelatedPrs)),
                other => unknown_plugin("after_create", other),
            }
        }
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

    fn gitlab(config: &Config) -> Self {
        Self {
            forge: Box::new(GlForge {
                user: config.gitlab.user.clone(),
            }),
            ticket_sources: vec![],
            draft_hooks: vec![],
            reviewer_source: Some(Box::new(GlReviewers)),
            after_create: vec![],
        }
    }

    /// Runs `DraftHook::before_edit` in order. A failing hook prints a warning.
    pub fn before_edit(&self, ctx: &Context, draft: &mut Draft) {
        for hook in &self.draft_hooks {
            if let Err(err) = hook.before_edit(ctx, draft) {
                warn(hook.name(), &err);
            }
        }
    }

    /// Runs `DraftHook::after_edit` in order. A failing hook prints a warning.
    pub fn after_edit(&self, ctx: &Context, draft: &mut Draft) {
        for hook in &self.draft_hooks {
            if let Err(err) = hook.after_edit(ctx, draft) {
                warn(hook.name(), &err);
            }
        }
    }

    /// The first ticket that a source detects. A failing source prints a warning.
    pub fn detect_ticket(&self, ctx: &Context) -> Option<Ticket> {
        self.ticket_sources.iter().find_map(|source| {
            source.detect(ctx).unwrap_or_else(|err| {
                warn(source.name(), &err);
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
                Err(err) => warn(source.name(), &err),
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
                warn(plugin.name(), &err);
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
            warn(source.name(), &err);
            vec![]
        })
    }
}

const DEFAULT_TICKET_SOURCES: &[&str] = &["commit_tags"];
const DEFAULT_DRAFT_HOOKS: &[&str] = &["template_body"];
const DEFAULT_AFTER_CREATE: &[&str] = &["related_prs"];

/// The configured names, or `defaults` when the list is missing
fn names(configured: &Option<Vec<String>>, defaults: &[&str]) -> Vec<String> {
    configured
        .clone()
        .unwrap_or_else(|| defaults.iter().map(|name| name.to_string()).collect())
}

fn unknown_plugin(kind: &str, name: &str) {
    println!(
        "{} Unknown plugin `{}` in plugins.{}; skipping.",
        "!".yellow(),
        name,
        kind
    );
}

fn warn(plugin: &str, err: &Error) {
    println!("{} Plugin `{}` failed: {}", "!".yellow(), plugin, err);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    struct Fixed(Vec<&'static str>);

    impl ReviewerSource for Fixed {
        fn name(&self) -> &'static str {
            "fixed"
        }

        fn reviewers(&self, _ctx: &Context, _draft: &Draft) -> Result<Vec<String>> {
            Ok(self.0.iter().map(|r| r.to_string()).collect())
        }
    }

    struct Failing;

    impl ReviewerSource for Failing {
        fn name(&self) -> &'static str {
            "failing"
        }

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
        fn name(&self) -> &'static str {
            "tickets"
        }

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
        fn name(&self) -> &'static str {
            "recording"
        }

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
        fn name(&self) -> &'static str {
            "failing_after"
        }

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
        fn name(&self) -> &'static str {
            "append"
        }

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
        fn name(&self) -> &'static str {
            "failing_hook"
        }

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
    fn test_for_origin_gitlab() {
        let registry =
            Registry::for_origin(&Config::default(), "git@gitlab.com:group/sub/r.git").unwrap();
        let pr = PrRef {
            repo: "group/sub/r".into(),
            number: 1,
        };

        assert_eq!(registry.forge.link(&pr), "group/sub/r!1");
        assert_eq!(registry.reviewer_source.unwrap().name(), "gitlab_reviewers");
    }

    #[test]
    fn test_for_origin_unknown_host() {
        let registry = Registry::for_origin(&Config::default(), "git@gitlab.example.com:o/r.git");

        assert!(registry.is_err());
    }

    fn configured(yaml: &str) -> Registry {
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        Registry::for_origin(&config, "git@github.com:o/r.git").unwrap()
    }

    fn ticket_source_names(registry: &Registry) -> Vec<&'static str> {
        registry.ticket_sources.iter().map(|p| p.name()).collect()
    }

    fn draft_hook_names(registry: &Registry) -> Vec<&'static str> {
        registry.draft_hooks.iter().map(|p| p.name()).collect()
    }

    fn after_create_names(registry: &Registry) -> Vec<&'static str> {
        registry.after_create.iter().map(|p| p.name()).collect()
    }

    #[test]
    fn test_missing_plugin_lists_use_defaults() {
        let registry = configured("{}");

        // Each built-in's name() matches the name that configures it
        assert_eq!(ticket_source_names(&registry), DEFAULT_TICKET_SOURCES);
        assert_eq!(draft_hook_names(&registry), DEFAULT_DRAFT_HOOKS);
        assert_eq!(after_create_names(&registry), DEFAULT_AFTER_CREATE);
    }

    #[test]
    fn test_empty_plugin_list_turns_kind_off() {
        let registry = configured("plugins: { after_create: [] }");

        assert!(after_create_names(&registry).is_empty());
        assert_eq!(ticket_source_names(&registry), vec!["commit_tags"]);
    }

    #[test]
    fn test_unknown_plugin_is_skipped() {
        let registry = configured("plugins: { after_create: [teams, related_prs] }");

        assert_eq!(after_create_names(&registry), vec!["related_prs"]);
    }

    #[test]
    fn test_jira_without_url_is_skipped() {
        let registry = configured("plugins: { ticket_sources: [commit_tags, jira] }");

        assert_eq!(ticket_source_names(&registry), vec!["commit_tags"]);
    }
}
