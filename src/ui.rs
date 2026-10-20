use inquire::autocompletion::Replacement;
use inquire::error::InquireError;
use inquire::ui::{Color, RenderConfig, Styled};
use inquire::{
    set_global_render_config, Autocomplete, CustomUserError, Editor, MultiSelect, Select, Text,
};

use crate::error::Error;
use crate::git::BranchInfo;
use crate::tags::Tags;

pub fn init_render_config() {
    let mut style = RenderConfig::default_colored();
    style.prompt_prefix = Styled::new(">").with_fg(Color::LightGreen);
    set_global_render_config(style);
}

/// Tag completion: suggestions that start with the input, in list order
#[derive(Clone)]
struct Suggestions(Vec<String>);

impl Autocomplete for Suggestions {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, CustomUserError> {
        let mut suggestions = Vec::new();
        for tag in self.0.iter() {
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
        for tag in self.0.iter() {
            if tag.starts_with(input) {
                return Ok(Some(tag.clone()));
            }
        }
        Ok(None)
    }
}

pub fn prompt_tag(suggestions: Vec<String>) -> Result<Option<String>, Error> {
    let normalize = |tag: String| {
        let tag = tag.trim().to_string();
        if tag.is_empty() {
            None
        } else {
            Some(tag)
        }
    };

    if suggestions.is_empty() {
        Text::new("PR Tag:")
            .with_validator(Tags::optional_validator)
            .prompt()
            .map(normalize)
            .map_err(map_inquire_error)
    } else {
        Text::new("PR Tag:")
            .with_autocomplete(Suggestions(suggestions))
            .with_validator(Tags::optional_validator)
            .prompt()
            .map(normalize)
            .map_err(map_inquire_error)
    }
}

pub fn prompt_base(bases: Vec<String>) -> Result<String, Error> {
    if bases.len() == 1 {
        return Ok(bases.into_iter().next().unwrap());
    }

    Select::new("PR base:", bases)
        .prompt()
        .map_err(map_inquire_error)
}

pub fn prompt_body(initial_body: &str) -> Result<String, Error> {
    let body = Editor::new("PR body:")
        .with_predefined_text(initial_body)
        .prompt()
        .map_err(map_inquire_error)?;

    if body.trim().is_empty() {
        return Err(Error::Prompt("PR body cannot be empty".to_string()));
    }

    Ok(body)
}

pub fn prompt_reviewers(reviewers: Vec<String>) -> Result<Vec<String>, Error> {
    if reviewers.is_empty() {
        return Ok(vec![]);
    }

    MultiSelect::new("Reviewers:", reviewers)
        .with_formatter(&|selected| {
            selected
                .iter()
                .map(|opt| opt.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .prompt()
        .map_err(map_inquire_error)
}

pub fn default_pr_title(branch_info: &BranchInfo) -> String {
    branch_info
        .commits
        .last()
        .map(|commit| crate::git::subject(commit).to_string())
        .unwrap_or_else(|| "Update pull request".to_string())
}

fn map_inquire_error(err: InquireError) -> Error {
    match err {
        InquireError::OperationCanceled | InquireError::OperationInterrupted => Error::Cancelled,
        _ => Error::Prompt(err.to_string()),
    }
}
