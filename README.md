# git-pr

A highly opinionated CLI for creating GitHub pull requests with a consistent structure and automatic related PR tracking.

## What It Does

- detects or prompts for a ticket/tag
- suggests or prompts for a base branch
- opens one editor buffer seeded from a PR template
- creates the PR through `gh`
- updates the "Related PRs" section across matching PRs

The authoring flow is intentionally simple: no form fields, no placeholder prompts, one editable PR body.

## Installation

```bash
git clone https://github.com/IAmRadek/git-pr.git
cd git-pr
cargo install --path .
```

## Prerequisites

- `gh` installed and authenticated
- a git repository with at least one commit on the current branch

## Configuration

Default config path:

```text
~/.config/git-pr/config.yaml
```

You can override the config directory with `--config` or `GIT_PR_CONFIG`.

Generate a default config:

```bash
git-pr --init
```

Example:

```yaml
jira:
  url: "https://yourcompany.atlassian.net/browse/"

github:
  user: "your-username"
  default_reviewers:
    - teammate1
    - teammate2

template:
  path: "/absolute/path/to/pull_request_template.md"
  body: |
    Related PRs:
    <!-- RELATED_PR -->
    <!-- /RELATED_PR -->

    ## Summary

    ## Changes

    ## Testing

  markers:
    related_pr_start: "<!-- RELATED_PR -->"
    related_pr_end: "<!-- /RELATED_PR -->"
```

## Template Resolution

The initial PR body is resolved in this order:

1. `.github/PULL_REQUEST_TEMPLATE.md` in the current repository
2. `template.path` from config
3. `template.body` from config

Before the resolved template content, `git-pr` prepends ticket context, for example:

```text
Ticket: TRACK-123
Jira: https://yourcompany.atlassian.net/browse/TRACK-123
```

If the resolved template does not already contain the related-PR marker block, `git-pr` also prepends:

```text
<!-- RELATED_PR -->
<!-- /RELATED_PR -->
```

That prepended block is only a convenience. You can move or copy it anywhere in the template once the editor opens.

## Usage

Create a PR:

```bash
git-pr
```

The flow is:

1. detect or select the ticket
2. select the base branch
3. edit the full PR body in your editor
4. optionally select reviewers

Update related PRs only:

```bash
git-pr --update-only
```

Dry run:

```bash
git-pr --dry-run
```

## Related PR Markers

Related PR updates only touch the configured marker section:

```text
<!-- RELATED_PR -->
<!-- /RELATED_PR -->
```

If those markers exist in the PR body, `git-pr` rewrites that block with links to matching PRs. The rest of the body is left as authored.

## Project Structure

```text
src/
├── app.rs
├── cli.rs
├── config.rs
├── error.rs
├── git.rs
├── github.rs
├── jira.rs
├── lib.rs
├── main.rs
├── pr.rs
├── tags.rs
├── template.rs
└── ui.rs
```

## License

MIT License. See [LICENSE](LICENSE).
