use std::io::{Read, Write};
use std::path::Path;

use inquire::autocompletion::Replacement;
use inquire::{Autocomplete, CustomUserError};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref PATTERN: Regex = Regex::new(r"\[(\w+\-?)*]").unwrap();
}

/// Extract a tag from a list of commit messages
/// Returns the first found tag along with the full commit message
pub fn extract_from_vec(commits: Vec<String>) -> Option<(String, String)> {
    for commit in commits {
        if let Some(tag) = extract_from_str(&commit) {
            return Some((tag, commit));
        }
    }
    None
}

/// Extract a tag from a string (e.g., "[TRACK-123]: message" -> "TRACK-123")
pub fn extract_from_str(message: &str) -> Option<String> {
    if let Some(m) = PATTERN.find(message) {
        return Some(m.as_str().replace(['[', ']'], ""));
    }
    None
}

/// Manages a collection of previously used tags with persistence
#[derive(Debug, Default, Clone)]
pub struct Tags {
    file: String,
    tags: Vec<String>,
}

impl Autocomplete for Tags {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, CustomUserError> {
        let mut suggestions = Vec::new();
        for tag in self.tags.iter() {
            if tag.starts_with(input) {
                suggestions.push(tag.clone());
            }
        }
        Ok(suggestions)
    }

    fn get_completion(
        &mut self,
        input: &str,
        _highlighted_suggestion: Option<String>,
    ) -> Result<Replacement, CustomUserError> {
        for tag in self.tags.iter() {
            if tag.starts_with(input) {
                return Ok(Some(tag.clone()));
            }
        }
        Ok(None)
    }
}

impl Tags {
    /// Validator for tag input format
    pub fn validator(
        ticket: &str,
    ) -> Result<inquire::validator::Validation, inquire::CustomUserError> {
        if PATTERN.is_match(ticket) {
            Ok(inquire::validator::Validation::Valid)
        } else {
            Ok(inquire::validator::Validation::Invalid(
                "This does not look like a valid tag (e.g., TRACK-123)".into(),
            ))
        }
    }

    /// Load tags from a file, or create an empty Tags if the file doesn't exist
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, std::io::Error> {
        let path = path.as_ref();

        if !path.exists() {
            return Ok(Self {
                file: path.to_str().unwrap().to_string(),
                tags: Vec::new(),
            });
        }

        let mut file = std::fs::File::open(path)?;

        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let tags: Vec<String> = contents
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect();

        Ok(Self {
            file: path.to_str().unwrap().to_string(),
            tags,
        })
    }

    /// Returns an iterator over the tags
    pub fn iter(&self) -> impl Iterator<Item = &String> {
        self.tags.iter()
    }

    /// Add a tag to the front of the list (most recently used)
    /// Removes duplicates and limits to 10 tags
    pub fn add(&mut self, tag: String) {
        if self.tags.contains(&tag) {
            self.tags.retain(|t| t != &tag);
        }
        self.tags.insert(0, tag);

        if self.tags.len() > 10 {
            self.tags.pop();
        }
    }

    /// Save the tags to the file
    pub fn save(&self) -> std::io::Result<()> {
        let mut file = std::fs::File::create(&self.file)?;
        for tag in &self.tags {
            file.write_all(tag.as_bytes())?;
            file.write_all(b"\n")?;
        }
        Ok(())
    }

    /// Add a tag and immediately save to file
    pub fn add_and_save(&mut self, tag: String) -> std::io::Result<()> {
        self.add(tag);
        self.save()
    }

    /// Check if there are no tags
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_extract_from_str() {
        assert_eq!(
            extract_from_str("[TRACK-123]: Add feature"),
            Some("TRACK-123".to_string())
        );
        assert_eq!(extract_from_str("No tag here"), None);
        assert_eq!(
            extract_from_str("[ABC]: Simple tag"),
            Some("ABC".to_string())
        );
    }

    #[test]
    fn test_extract_from_vec() {
        let commits = vec![
            "No tag here".to_string(),
            "[TRACK-123]: Add feature".to_string(),
            "[TRACK-456]: Another".to_string(),
        ];
        let result = extract_from_vec(commits);
        assert!(result.is_some());
        let (tag, commit) = result.unwrap();
        assert_eq!(tag, "TRACK-123");
        assert_eq!(commit, "[TRACK-123]: Add feature");
    }

    #[test]
    fn test_tags_add_and_save() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        let mut tags = Tags::from_file(path).unwrap();
        tags.add("TRACK-123".to_string());
        tags.add("TRACK-123".to_string()); // Duplicate
        tags.add("TRACK-124".to_string());
        tags.save().unwrap();

        let tags = Tags::from_file(path).unwrap();
        assert_eq!(tags.tags.len(), 2);
        assert_eq!(tags.tags[0], "TRACK-124");
        assert_eq!(tags.tags[1], "TRACK-123");
    }

    #[test]
    fn test_tags_max_limit() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        let mut tags = Tags::from_file(path).unwrap();
        for i in 0..15 {
            tags.add(format!("TAG-{}", i));
        }

        assert_eq!(tags.tags.len(), 10);
        assert_eq!(tags.tags[0], "TAG-14"); // Most recent
    }
}
