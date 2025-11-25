use inquire::error::InquireError;
use inquire::list_option::ListOption;
use inquire::ui::{Color, RenderConfig, Styled};
use inquire::validator::Validation;
use inquire::{set_global_render_config, CustomUserError, Editor, MultiSelect, Select, Text};

use crate::error::Error;
use crate::git::BranchInfo;
use crate::tags::Tags;

/// Initialize the global render configuration for inquire prompts
pub fn init_render_config() {
    let mut style = RenderConfig::default_colored();
    style.prompt_prefix = Styled::new(">").with_fg(Color::LightGreen);
    set_global_render_config(style);
}

/// Prompt for PR title with autocomplete from commit messages
pub fn prompt_title(branch_info: &BranchInfo) -> Result<String, Error> {
    let default = branch_info.commits.last().map(|s| s.as_str()).unwrap_or("");

    Text::new("PR title:")
        .with_default(default)
        .with_autocomplete(branch_info.clone())
        .prompt()
        .map_err(map_inquire_error)
}

/// Prompt for PR tag with autocomplete from previously used tags
pub fn prompt_tag(tags: &Tags) -> Result<String, Error> {
    if tags.is_empty() {
        Text::new("PR Tag:")
            .with_validator(Tags::validator)
            .prompt()
            .map_err(map_inquire_error)
    } else {
        let default = tags.iter().next().cloned().unwrap_or_default();
        Text::new("PR Tag:")
            .with_autocomplete(tags.clone())
            .with_default(&default)
            .prompt()
            .map_err(map_inquire_error)
    }
}

/// Prompt for PR base branch selection
pub fn prompt_base(bases: Vec<String>) -> Result<String, Error> {
    if bases.len() == 1 {
        return Ok(bases.into_iter().next().unwrap());
    }

    Select::new("PR base:", bases)
        .prompt()
        .map_err(map_inquire_error)
}

/// Prompt for PR description using an editor
pub fn prompt_description(prompt: &str) -> Result<String, Error> {
    Editor::new(prompt)
        .with_formatter(&|x| x.to_string())
        .prompt()
        .map_err(map_inquire_error)
}

/// Prompt for selecting reviewers from a list
pub fn prompt_reviewers(reviewers: Vec<String>) -> Result<Vec<String>, Error> {
    if reviewers.is_empty() {
        return Ok(vec![]);
    }

    MultiSelect::new("Reviewers:", reviewers)
        .with_validator(
            |selected: &[ListOption<&String>]| -> Result<Validation, CustomUserError> {
                if selected.is_empty() {
                    return Ok(Validation::Invalid("Select at least one reviewer".into()));
                }
                Ok(Validation::Valid)
            },
        )
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

/// Map inquire errors to our error type
fn map_inquire_error(err: InquireError) -> Error {
    match err {
        InquireError::OperationCanceled | InquireError::OperationInterrupted => Error::Cancelled,
        _ => Error::Prompt(err.to_string()),
    }
}
