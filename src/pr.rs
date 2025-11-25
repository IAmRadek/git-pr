/// Represents a Pull Request being created or updated
#[derive(Debug, Default, Clone)]
pub struct PullRequest {
    /// The title of the PR (e.g., "[TRACK-123]: Add new feature")
    pub title: String,
    /// The tag/ticket identifier (e.g., "TRACK-123")
    pub tag: String,
    /// Whether this PR is tracked by a Jira ticket
    pub is_jira: bool,
    /// Description of what this PR does
    pub description: String,
    /// Implementation details and considerations
    pub implementation: String,
    /// List of GitHub usernames to request review from
    pub reviewers: Vec<String>,
    /// The base branch to merge into
    pub base: String,
}

impl PullRequest {
    /// Creates a new PullRequest with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the title and returns self for chaining
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Sets the tag and returns self for chaining
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }

    /// Sets whether this is a Jira ticket
    pub fn with_jira(mut self, is_jira: bool) -> Self {
        self.is_jira = is_jira;
        self
    }

    /// Sets the description and returns self for chaining
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Sets the implementation details and returns self for chaining
    pub fn with_implementation(mut self, implementation: impl Into<String>) -> Self {
        self.implementation = implementation.into();
        self
    }

    /// Sets the reviewers and returns self for chaining
    pub fn with_reviewers(mut self, reviewers: Vec<String>) -> Self {
        self.reviewers = reviewers;
        self
    }

    /// Sets the base branch and returns self for chaining
    pub fn with_base(mut self, base: impl Into<String>) -> Self {
        self.base = base.into();
        self
    }
}
