use inquire::autocompletion::Replacement;
use inquire::error::InquireError;
use inquire::ui::{Color, RenderConfig, Styled};
use inquire::{
    set_global_render_config, Autocomplete, CustomUserError, Editor, MultiSelect, Select, Text,
};

use crate::error::Error;
use crate::git::BranchInfo;
use crate::plugin::Ticket;
use crate::tags::Tags;

pub fn init_render_config() {
    let mut style = RenderConfig::default_colored();
    style.prompt_prefix = Styled::new(">").with_fg(Color::LightGreen);
    set_global_render_config(style);
}

/// Ticket completion, in list order. A suggestion shows as "ID  summary" and
/// matches an input that starts its id or occurs in its summary.
#[derive(Clone)]
struct Suggestions(Vec<Ticket>);

impl Suggestions {
    fn matching<'a>(&'a self, input: &'a str) -> impl Iterator<Item = &'a Ticket> + 'a {
        let needle = input.trim().to_lowercase();
        self.0.iter().filter(move |ticket| {
            ticket.id.to_lowercase().starts_with(&needle)
                || ticket
                    .summary
                    .as_deref()
                    .is_some_and(|summary| summary.to_lowercase().contains(&needle))
        })
    }
}

impl Autocomplete for Suggestions {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, CustomUserError> {
        Ok(self.matching(input).map(label).collect())
    }

    /// Tab completes to the id of the highlighted suggestion, else of the first match
    fn get_completion(
        &mut self,
        input: &str,
        highlighted_suggestion: Option<String>,
    ) -> Result<Replacement, CustomUserError> {
        if let Some(highlighted) = highlighted_suggestion {
            return Ok(Some(answer_id(&highlighted).to_string()));
        }
        Ok(self.matching(input).next().map(|ticket| ticket.id.clone()))
    }
}

/// How a suggestion shows at the prompt: "ID" or "ID  summary"
fn label(ticket: &Ticket) -> String {
    match &ticket.summary {
        Some(summary) => format!("{}  {}", ticket.id, summary),
        None => ticket.id.clone(),
    }
}

/// The id in a prompt answer, which is a typed id or a suggestion label
fn answer_id(answer: &str) -> &str {
    answer.split_whitespace().next().unwrap_or("")
}

/// The suggested ticket with the answer's id, else a ticket with only that id.
/// `None` for an empty answer.
fn answer_ticket(answer: &str, suggestions: &[Ticket]) -> Option<Ticket> {
    let id = answer_id(answer);
    if id.is_empty() {
        return None;
    }

    Some(
        suggestions
            .iter()
            .find(|ticket| ticket.id == id)
            .cloned()
            .unwrap_or_else(|| Ticket {
                id: id.to_string(),
                ..Ticket::default()
            }),
    )
}

pub fn prompt_tag(suggestions: Vec<Ticket>) -> Result<Option<Ticket>, Error> {
    let validator = |answer: &str| Tags::optional_validator(answer_id(answer));

    let answer = if suggestions.is_empty() {
        Text::new("PR Tag:").with_validator(validator).prompt()
    } else {
        Text::new("PR Tag:")
            .with_autocomplete(Suggestions(suggestions.clone()))
            .with_validator(validator)
            .prompt()
    }
    .map_err(map_inquire_error)?;

    Ok(answer_ticket(&answer, &suggestions))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ticket(id: &str, summary: Option<&str>) -> Ticket {
        Ticket {
            id: id.to_string(),
            url: Some(format!("https://jira.example.com/browse/{}", id)),
            summary: summary.map(str::to_string),
        }
    }

    fn suggestions() -> Suggestions {
        Suggestions(vec![
            ticket("TRACK-1", Some("Fix login")),
            ticket("TRACK-2", None),
            ticket("OPS-3", Some("Rotate track logs")),
        ])
    }

    #[test]
    fn test_suggestions_show_summary() {
        assert_eq!(
            suggestions().get_suggestions("TRACK").unwrap(),
            vec!["TRACK-1  Fix login", "TRACK-2", "OPS-3  Rotate track logs"]
        );
    }

    #[test]
    fn test_suggestions_match_summary() {
        assert_eq!(
            suggestions().get_suggestions("login").unwrap(),
            vec!["TRACK-1  Fix login"]
        );
    }

    #[test]
    fn test_completion_is_the_id() {
        let mut suggestions = suggestions();

        assert_eq!(
            suggestions
                .get_completion("TR", Some("TRACK-1  Fix login".to_string()))
                .unwrap(),
            Some("TRACK-1".to_string())
        );
        assert_eq!(
            suggestions.get_completion("OPS", None).unwrap(),
            Some("OPS-3".to_string())
        );
    }

    #[test]
    fn test_answer_label_gives_suggested_ticket() {
        let tickets = suggestions().0;

        assert_eq!(
            answer_ticket("TRACK-1  Fix login", &tickets),
            Some(tickets[0].clone())
        );
    }

    #[test]
    fn test_typed_answer_gives_bare_ticket() {
        assert_eq!(
            answer_ticket(" NEW-9 ", &suggestions().0),
            Some(Ticket {
                id: "NEW-9".to_string(),
                ..Ticket::default()
            })
        );
        assert_eq!(answer_ticket("  ", &suggestions().0), None);
    }
}
