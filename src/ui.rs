use inquire::error::InquireError;
use inquire::list_option::ListOption;
use inquire::ui::{Color, RenderConfig, Styled};
use inquire::validator::Validation;
use inquire::{set_global_render_config, CustomUserError, Editor, MultiSelect, Select, Text};

use crate::config::{FieldType, FormField};
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

/// Prompt for a single form field based on its configuration
///
/// Returns `Ok(None)` if the field is optional and the user provides no input.
/// Returns `Err` if the field is required and empty, or on cancellation.
pub fn prompt_field(field: &FormField) -> Result<Option<String>, Error> {
    let result = match field.field_type {
        FieldType::Editor => prompt_editor_field(field)?,
        FieldType::Text => prompt_text_field(field)?,
    };

    // Handle empty results
    if result.trim().is_empty() {
        if field.required {
            return Err(Error::Prompt(format!(
                "Field '{}' is required but was left empty",
                field.name
            )));
        }
        return Ok(None);
    }

    Ok(Some(result))
}

/// Prompt using an editor for multi-line input
fn prompt_editor_field(field: &FormField) -> Result<String, Error> {
    let mut editor = Editor::new(&field.prompt).with_formatter(&|x| {
        // Show a preview of the content
        let preview: String = x.chars().take(50).collect();
        if x.len() > 50 {
            format!("{}...", preview)
        } else {
            preview
        }
    });

    if let Some(default) = &field.default {
        editor = editor.with_predefined_text(default);
    }

    editor.prompt().map_err(map_inquire_error)
}

/// Prompt using single-line text input
fn prompt_text_field(field: &FormField) -> Result<String, Error> {
    let mut text = Text::new(&field.prompt);

    if let Some(default) = &field.default {
        text = text.with_default(default);
    }

    if field.required {
        text = text.with_validator(|input: &str| {
            if input.trim().is_empty() {
                Ok(Validation::Invalid("This field is required".into()))
            } else {
                Ok(Validation::Valid)
            }
        });
    }

    text.prompt().map_err(map_inquire_error)
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
