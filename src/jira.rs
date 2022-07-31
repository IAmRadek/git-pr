use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    pub static ref PATTERN: Regex = Regex::new(r"((\w+)\-(\d+))").unwrap();
}

pub(crate) fn validator(ticket: &str) -> Result<(), String> {
    if PATTERN.is_match(ticket) {
        return Ok(());
    }
    Err("This does not looks like JIRA ticket (eg. TRACK-123)".into())
}

pub(crate) fn find_ticket(commits: Vec<String>) -> Option<(String, String)> {
    for commit in commits {
        if let Some(m) = PATTERN.find(commit.as_str()) {
            return Some((String::from(m.as_str()), String::from(commit.trim())));
        }
    }
    None
}


