use std::io::{Read, Write};
use std::path::Path;

use inquire::{Autocomplete, CustomUserError};
use inquire::autocompletion::Replacement;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref PATTERN: Regex = Regex::new(r"\[(\w+\-[\w|\d]+)\]").unwrap();
}


pub(crate) fn extract_from_vec(commits: Vec<String>) -> Option<(String, String)> {
    for commit in commits {
        if let Some(tag) = extract_from_str(&commit) {
            return Some((tag, commit));
        }
    }
    None
}

pub(crate) fn extract_from_str(message: &str) -> Option<String> {
    if let Some(m) = PATTERN.find(message) {
        return Some(m.as_str().replace(['[', ']'], ""));
    }
    None
}


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

    fn get_completion(&mut self, input: &str, _highlighted_suggestion: Option<String>) -> Result<Replacement, CustomUserError> {
        for tag in self.tags.iter() {
            if tag.starts_with(input) {
                return Ok(Some(tag.clone()));
            }
        }
        Ok(None)
    }
}


impl Tags {
    pub fn validator(ticket: &str) -> Result<inquire::validator::Validation, inquire::CustomUserError> {
        if PATTERN.is_match(ticket) {
            Ok(inquire::validator::Validation::Valid)
        } else {
            Ok(inquire::validator::Validation::Invalid("This does not looks like valid TAG ticket (eg. TRACK-123)".into()))
        }
    }

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

        let mut tags = Vec::new();
        for line in contents.lines() {
            let tag = line.trim();
            tags.push(tag.to_string());
        }

        Ok(Self {
            file: path.to_str().unwrap().to_string(),
            tags,
        })
    }

    pub fn iter(&self) -> Vec<String> {
        self.tags.clone()
    }

    pub fn add(&mut self, tag: String) {
        if self.tags.contains(&tag) {
            self.tags.retain(|t| t != &tag);
        }
        self.tags.insert(0, tag);

        if self.tags.len() > 10 {
            self.tags.pop();
        }
    }

    pub fn save(self) -> std::io::Result<()> {
        let mut file = std::fs::File::create(self.file)?;
        for tag in self.tags {
            file.write_all(tag.as_bytes())?;
            file.write_all(b"\n")?;
        }
        Ok(())
    }

    pub fn add_and_save(mut self, tag: String) -> std::io::Result<()> {
        self.add(tag);
        self.save()
    }

    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tags() {
        let mut tags = Tags::from_file("pr_tags.txt").unwrap();
        tags.add("TRACK-123".to_string());
        tags.add("TRACK-123".to_string());
        tags.add("TRACK-124".to_string());

        tags.save().unwrap();

        let tags = Tags::from_file("pr_tags.txt").unwrap();
        assert_eq!(tags.tags.len(), 2);
        assert_eq!(tags.tags[0], "TRACK-124");
        assert_eq!(tags.tags[1], "TRACK-123");
    }
}