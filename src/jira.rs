//! The `jira` ticket source: suggests the tickets that `jira.jql` finds.
//!
//! # Configuration
//!
//! - `jira.url`: the ticket link prefix, e.g. "https://company.atlassian.net/browse/".
//!   The API address is the part before `browse/`.
//! - `jira.kind`: `cloud` or `datacenter`.
//! - `jira.jql`: the query for the suggestions.
//!
//! The token comes from the environment, never from config.yaml:
//! - `JIRA_TOKEN`: the API token (Cloud) or the personal access token (Data Center)
//! - `JIRA_USER`: the account email, Cloud only

use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::Deserialize;
use ureq::tls::{RootCerts, TlsConfig};

use crate::config::{Config, JiraKind};
use crate::error::{Error, Result};
use crate::plugin::{Context, Ticket, TicketSource};
use crate::tags::jira_ticket;

const MAX_RESULTS: &str = "20";
const TIMEOUT: Duration = Duration::from_secs(5);

/// Suggests tickets from a Jira search. It detects nothing.
pub struct JiraTickets {
    kind: JiraKind,
    /// The Jira address, ending in `/`
    api_base: String,
    /// The `Authorization` header value
    auth: String,
    jql: String,
}

impl JiraTickets {
    /// Reads `jira` from `config` and the token from the environment
    pub fn from_config(config: &Config) -> Result<Self> {
        Self::new(config, |name| std::env::var(name).ok())
    }

    fn new(config: &Config, env: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let url = config
            .jira_url()
            .ok_or_else(|| Error::Config("set `jira.url` or JIRA_URL".to_string()))?;
        let api_base = api_base(url)?;

        let var = |name: &str| {
            env(name)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| Error::Config(format!("set {}", name)))
        };
        let token = var("JIRA_TOKEN")?;
        let auth = match config.jira.kind {
            JiraKind::Cloud => {
                let user = var("JIRA_USER")?;
                format!("Basic {}", STANDARD.encode(format!("{}:{}", user, token)))
            }
            JiraKind::Datacenter => format!("Bearer {}", token),
        };

        Ok(Self {
            kind: config.jira.kind,
            api_base,
            auth,
            jql: config.jira.jql.clone(),
        })
    }

    fn search_url(&self) -> String {
        let path = match self.kind {
            JiraKind::Cloud => "rest/api/3/search/jql",
            JiraKind::Datacenter => "rest/api/2/search",
        };
        format!("{}{}", self.api_base, path)
    }

    fn search(&self) -> Result<SearchResponse> {
        let agent = ureq::Agent::new_with_config(
            ureq::Agent::config_builder()
                .timeout_global(Some(TIMEOUT))
                .tls_config(
                    TlsConfig::builder()
                        .root_certs(RootCerts::PlatformVerifier)
                        .build(),
                )
                .build(),
        );

        agent
            .get(self.search_url())
            .header("Authorization", &self.auth)
            .header("Accept", "application/json")
            .query("jql", &self.jql)
            .query("fields", "summary")
            .query("maxResults", MAX_RESULTS)
            .call()
            .map_err(|err| Error::Jira(err.to_string()))?
            .body_mut()
            .read_json()
            .map_err(|err| Error::Jira(err.to_string()))
    }
}

impl TicketSource for JiraTickets {
    fn name(&self) -> &'static str {
        "jira"
    }

    fn detect(&self, _ctx: &Context) -> Result<Option<Ticket>> {
        Ok(None)
    }

    fn suggest(&self, ctx: &Context) -> Result<Vec<Ticket>> {
        Ok(self.search()?.tickets(&ctx.config))
    }
}

/// The search response of both API versions, reduced to the fields we read
#[derive(Debug, Deserialize)]
struct SearchResponse {
    issues: Vec<Issue>,
}

#[derive(Debug, Deserialize)]
struct Issue {
    key: String,
    fields: IssueFields,
}

#[derive(Debug, Deserialize)]
struct IssueFields {
    summary: Option<String>,
}

impl SearchResponse {
    fn tickets(self, config: &Config) -> Vec<Ticket> {
        self.issues
            .into_iter()
            .map(|issue| Ticket {
                summary: issue.fields.summary,
                ..jira_ticket(config, issue.key)
            })
            .collect()
    }
}

/// The Jira address from the link prefix: "https://x/browse/" gives "https://x/"
fn api_base(url: &str) -> Result<String> {
    let trimmed = url.trim_end_matches('/');
    trimmed
        .strip_suffix("browse")
        .map(str::to_string)
        .ok_or_else(|| Error::Config(format!("`jira.url` must end with /browse/: {}", url)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(kind: JiraKind) -> Config {
        let mut config = Config::default();
        config.jira.url = Some("https://jira.example.com/browse/".to_string());
        config.jira.kind = kind;
        config
    }

    fn env(vars: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |name| {
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        }
    }

    #[test]
    fn test_api_base() {
        assert_eq!(
            api_base("https://jira.example.com/browse/").unwrap(),
            "https://jira.example.com/"
        );
        assert_eq!(
            api_base("https://example.com/jira/browse").unwrap(),
            "https://example.com/jira/"
        );
        assert!(api_base("https://jira.example.com/").is_err());
    }

    #[test]
    fn test_cloud_uses_basic_auth_and_v3_search() {
        let jira = JiraTickets::new(
            &config(JiraKind::Cloud),
            env(&[("JIRA_USER", "me@example.com"), ("JIRA_TOKEN", "t")]),
        )
        .unwrap();

        assert_eq!(
            jira.auth,
            format!("Basic {}", STANDARD.encode("me@example.com:t"))
        );
        assert_eq!(
            jira.search_url(),
            "https://jira.example.com/rest/api/3/search/jql"
        );
    }

    #[test]
    fn test_datacenter_uses_bearer_auth_and_v2_search() {
        let jira =
            JiraTickets::new(&config(JiraKind::Datacenter), env(&[("JIRA_TOKEN", "t")])).unwrap();

        assert_eq!(jira.auth, "Bearer t");
        assert_eq!(
            jira.search_url(),
            "https://jira.example.com/rest/api/2/search"
        );
    }

    #[test]
    fn test_cloud_without_user_is_an_error() {
        let jira = JiraTickets::new(&config(JiraKind::Cloud), env(&[("JIRA_TOKEN", "t")]));

        assert!(jira.is_err());
    }

    #[test]
    fn test_without_url_is_an_error() {
        let jira = JiraTickets::new(&Config::default(), env(&[("JIRA_TOKEN", "t")]));

        assert!(jira.is_err());
    }

    #[test]
    fn test_search_response_to_tickets() {
        let json = r#"{
            "issues": [
                {"id": "1", "key": "TRACK-1", "fields": {"summary": "Fix login"}},
                {"id": "2", "key": "TRACK-2", "fields": {}}
            ],
            "nextPageToken": "abc"
        }"#;
        let response: SearchResponse = serde_json::from_str(json).unwrap();

        let tickets = response.tickets(&config(JiraKind::Cloud));

        assert_eq!(
            tickets,
            vec![
                Ticket {
                    id: "TRACK-1".to_string(),
                    url: Some("https://jira.example.com/browse/TRACK-1".to_string()),
                    summary: Some("Fix login".to_string()),
                },
                Ticket {
                    id: "TRACK-2".to_string(),
                    url: Some("https://jira.example.com/browse/TRACK-2".to_string()),
                    summary: None,
                },
            ]
        );
    }
}
