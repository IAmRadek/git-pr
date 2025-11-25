use std::collections::HashMap;

/// Represents a Pull Request being created or updated
#[derive(Debug, Default, Clone)]
pub struct PullRequest {
    /// The title of the PR (e.g., "[TRACK-123]: Add new feature")
    pub title: String,
    /// The tag/ticket identifier (e.g., "TRACK-123")
    pub tag: String,
    /// Whether this PR is tracked by a Jira ticket (tag found in commit)
    pub is_jira: bool,
    /// Dynamic form fields filled in by the user (field_name -> value)
    pub fields: HashMap<String, String>,
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

    /// Sets a form field value and returns self for chaining
    pub fn with_field(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.insert(name.into(), value.into());
        self
    }

    /// Sets all form fields and returns self for chaining
    pub fn with_fields(mut self, fields: HashMap<String, String>) -> Self {
        self.fields = fields;
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

    /// Gets a field value by name
    pub fn get_field(&self, name: &str) -> Option<&str> {
        self.fields.get(name).map(|s| s.as_str())
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
            .with_field("description", "This is a test")
            .with_field("notes", "Some notes")
            .with_reviewers(vec!["user1".into(), "user2".into()])
            .with_base("main");

        assert_eq!(pr.title, "[TEST-123]: Test PR");
        assert_eq!(pr.tag, "TEST-123");
        assert!(pr.is_jira);
        assert_eq!(pr.get_field("description"), Some("This is a test"));
        assert_eq!(pr.get_field("notes"), Some("Some notes"));
        assert_eq!(pr.reviewers, vec!["user1", "user2"]);
        assert_eq!(pr.base, "main");
    }

    #[test]
    fn test_with_fields_hashmap() {
        let mut fields = HashMap::new();
        fields.insert("field1".to_string(), "value1".to_string());
        fields.insert("field2".to_string(), "value2".to_string());

        let pr = PullRequest::new().with_fields(fields);

        assert_eq!(pr.get_field("field1"), Some("value1"));
        assert_eq!(pr.get_field("field2"), Some("value2"));
        assert_eq!(pr.get_field("nonexistent"), None);
    }
}
