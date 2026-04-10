/// Represents a Pull Request being created or updated
#[derive(Debug, Default, Clone)]
pub struct PullRequest {
    /// The title of the PR (e.g., "[TRACK-123]: Add new feature")
    pub title: String,
    /// The tag/ticket identifier (e.g., "TRACK-123")
    pub tag: String,
    /// Whether this PR is tracked by a Jira ticket
    pub is_jira: bool,
    /// The full PR body as authored in the editor
    pub body: String,
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

    /// Sets the full PR body and returns self for chaining
    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body = body.into();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pr_builder_pattern() {
        let pr = PullRequest::new()
            .with_title("[TEST-123]: Test PR")
            .with_tag("TEST-123")
            .with_jira(true)
            .with_body("PR body")
            .with_reviewers(vec!["user1".into(), "user2".into()])
            .with_base("main");

        assert_eq!(pr.title, "[TEST-123]: Test PR");
        assert_eq!(pr.tag, "TEST-123");
        assert!(pr.is_jira);
        assert_eq!(pr.body, "PR body");
        assert_eq!(pr.reviewers, vec!["user1", "user2"]);
        assert_eq!(pr.base, "main");
    }

    #[test]
    fn test_with_body() {
        let pr = PullRequest::new().with_body("Body text");
        assert_eq!(pr.body, "Body text");
    }
}
