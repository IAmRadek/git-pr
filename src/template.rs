use regex::Regex;

use crate::github::PullRequest;

pub(crate) const TEMPLATE: &str = "Tracked by <!-- ISSUE_URL -->
Related PRs:
<!-- RELATED_PR -->
- [ABCD-XXXX](https://example.com/ABCD-XXXX)
- [ABCD-XXXX](https://example.com/ABCD-XXXX)
<!-- /RELATED_PR -->

## This PR...

<!-- THIS PR -->

## Considerations and implementation

<!-- IMPLEMENTATION -->
";

pub(crate) fn make_body(jira_ticket: &String, is_jira_ticket: &bool, this_pr: &String, implementation: &String) -> String {
    let jira_url = std::env::var("JIRA_URL").expect("Unable to find JIRA_URL env");

    let mut template = TEMPLATE.to_string();
    if *is_jira_ticket {
        template = template.replace("<!-- ISSUE_URL -->", format!("[{}]({}{})", jira_ticket.as_str(), jira_url, jira_ticket.as_str()).as_str());
    } else {
        template = template.replace("Tracked by <!-- ISSUE_URL -->", "");
    }
    template = template.replace("<!-- THIS PR -->", this_pr.as_str());
    template = template.replace("<!-- IMPLEMENTATION -->", implementation.as_str());

    return template;
}

pub(crate) fn replace_related_prs(body: &String, this_pr: &u32, related_prs: &Vec<PullRequest>) -> String {
    let mut related_prs_body: Vec<String> = vec!["<!-- RELATED_PR -->".into()];
    for pr in related_prs {
        let resource_path = pr.resource_path.replacen("/", "", 1);
        if *this_pr == pr.number {
            related_prs_body.push(format!("- {} - (this pr)", resource_path));
        } else {
            related_prs_body.push(format!("- {}", resource_path));
        }
    }
    related_prs_body.push("<!-- /RELATED_PR -->".into());

    let re = Regex::new(r"(?sm)^<!-- RELATED_PR -->(.*)<!-- /RELATED_PR -->").unwrap();
    let result = re.replace_all(body.as_str(), related_prs_body.join("\n"));

    return result.to_string();
}
