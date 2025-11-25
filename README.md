# git-pr

A highly opinionated CLI tool for creating GitHub Pull Requests with automatic related PR tracking.

## Features

- **Smart Tag Detection**: Automatically extracts ticket/tag identifiers (e.g., `[TRACK-123]`) from commit messages
- **Related PR Tracking**: Automatically updates the "Related PRs" section in all PRs sharing the same tag
- **Reviewer Selection**: Interactive multi-select for choosing reviewers from repository collaborators
- **Base Branch Detection**: Intelligently suggests base branches based on git history
- **Tag Autocomplete**: Remembers previously used tags for quick selection
- **Customizable Templates**: Configure your own PR body template via YAML configuration

## Installation

### From Source

```bash
git clone https://github.com/IAmRadek/git-pr.git
cd git-pr
cargo install --path .
```

### Prerequisites

- [GitHub CLI (`gh`)](https://cli.github.com/) - must be installed and authenticated
- Git repository with at least one commit on the current branch

## Configuration

git-pr uses a YAML configuration file for customization. The configuration file is located at:

```
~/.config/git-pr/config.yaml
```

You can specify a custom config directory using the `--config` flag or `GIT_PR_CONFIG` environment variable.

### Configuration File

Create `~/.config/git-pr/config.yaml` with any of the following options:

```yaml
# Jira Integration Settings
jira:
  # Base URL for Jira ticket links
  # Example: "https://company.atlassian.net/browse/"
  url: "https://yourcompany.atlassian.net/browse/"
  
  # Automatically detect Jira tickets from branch/commit names
  auto_detect: true

# GitHub Settings
github:
  # Your GitHub username (used for fetching your related PRs)
  user: "your-username"
  
  # Default reviewers to suggest when creating PRs
  default_reviewers:
    - teammate1
    - teammate2

# PR Template Settings
template:
  # The PR body template with placeholders
  body: |
    Tracked by <!-- ISSUE_URL -->
    Related PRs:
    <!-- RELATED_PR -->
    - [ABCD-XXXX](https://example.com/ABCD-XXXX)
    <!-- /RELATED_PR -->

    ## This PR...

    <!-- THIS PR -->

    ## Considerations and implementation

    <!-- IMPLEMENTATION -->

  # Advanced: Customize placeholder markers (optional)
  placeholders:
    issue_url: "<!-- ISSUE_URL -->"
    related_pr_start: "<!-- RELATED_PR -->"
    related_pr_end: "<!-- /RELATED_PR -->"
    description: "<!-- THIS PR -->"
    implementation: "<!-- IMPLEMENTATION -->"
    tracking_line_prefix: "Tracked by <!-- ISSUE_URL -->"
```

### Template Placeholders

The PR body template supports the following placeholders:

| Placeholder | Description |
|-------------|-------------|
| `<!-- ISSUE_URL -->` | Replaced with the Jira ticket link (if configured) |
| `<!-- RELATED_PR -->...<!-- /RELATED_PR -->` | Section that gets updated with related PRs |
| `<!-- THIS PR -->` | Replaced with the PR description you enter |
| `<!-- IMPLEMENTATION -->` | Replaced with implementation details you enter |

### Environment Variables

Configuration values can also be set via environment variables. Environment variables are used as fallbacks when the corresponding config file value is not set.

| Variable | Description |
|----------|-------------|
| `GITHUB_USER` | Your GitHub username (fallback for `github.user`) |
| `JIRA_URL` | Base URL for Jira ticket links (fallback for `jira.url`) |
| `GIT_PR_CONFIG` | Custom config directory path (default: `~/.config/git-pr`) |

### Example Setup

Minimal setup using environment variables:

```bash
export GITHUB_USER="your-username"
export JIRA_URL="https://yourcompany.atlassian.net/browse/"
```

Or create a config file for more options:

```bash
mkdir -p ~/.config/git-pr
cat > ~/.config/git-pr/config.yaml << 'EOF'
jira:
  url: "https://yourcompany.atlassian.net/browse/"

github:
  user: "your-username"
  default_reviewers:
    - teammate1
EOF
```

## Usage

### Create a New PR

```bash
# Navigate to your git repository on a feature branch
cd your-repo
git checkout -b feature/TRACK-123-new-feature

# Create commits with tag in message (optional but recommended)
git commit -m "[TRACK-123]: Add new feature"

# Run git-pr
git-pr
```

The tool will guide you through:
1. PR title (auto-filled from commit message if tag is detected)
2. Tag/ticket identifier (with autocomplete from history)
3. Base branch selection (if multiple candidates)
4. PR description (opens your editor)
5. Implementation details (opens your editor)
6. Reviewer selection (multi-select from available reviewers)

### Update Related PRs Only

If you already have a PR and just want to update the "Related PRs" section:

```bash
git-pr --update-only
```

### Dry Run

Preview the commands without making any changes:

```bash
git-pr --dry-run
```

### CLI Options

```
Options:
  -u, --update-only    Only update related PRs without creating a new PR
  -d, --dry-run        Perform a dry run without making any changes
  -c, --config <PATH>  Path to the configuration directory [env: GIT_PR_CONFIG]
  -h, --help           Print help
  -V, --version        Print version
```

## Project Structure

```
src/
├── lib.rs        # Library root with module declarations
├── main.rs       # Thin CLI entry point
├── app.rs        # Main application orchestration
├── cli.rs        # Command-line argument parsing (clap)
├── config.rs     # Configuration loading and management
├── error.rs      # Error types with thiserror
├── git.rs        # Git operations using git2
├── github.rs     # GitHub API interactions via gh CLI
├── jira.rs       # Jira integration (placeholder)
├── pr.rs         # PullRequest model
├── tags.rs       # Tag extraction and management
├── template.rs   # PR body template handling
└── ui.rs         # Interactive prompts (inquire)
```

## How It Works

1. **Configuration Loading**: Reads settings from `~/.config/git-pr/config.yaml` with environment variable fallbacks

2. **Branch Analysis**: Scans the current branch's commits to find potential base branches and extract commit messages

3. **Tag Detection**: Looks for patterns like `[TRACK-123]` in commit messages to auto-fill the PR title and tag

4. **PR Creation**: Uses the GitHub CLI to create the PR with your configured template, title, description, and reviewers

5. **Related PR Discovery**: Queries GitHub for your recent PRs and filters those with matching tags

6. **Bulk Update**: Updates the "Related PRs" section in all matching PRs so they reference each other

## Custom Template Examples

### Minimal Template

```yaml
template:
  body: |
    ## Description
    <!-- THIS PR -->

    ## Implementation
    <!-- IMPLEMENTATION -->
```

### Template with Checklist

```yaml
template:
  body: |
    Tracked by <!-- ISSUE_URL -->

    ## Related PRs
    <!-- RELATED_PR -->
    <!-- /RELATED_PR -->

    ## Description
    <!-- THIS PR -->

    ## Implementation Notes
    <!-- IMPLEMENTATION -->

    ## Checklist
    - [ ] Tests added/updated
    - [ ] Documentation updated
    - [ ] Ready for review
```

### Template with Custom Markers

```yaml
template:
  body: |
    **Ticket**: {{ISSUE}}
    
    {{RELATED_START}}
    {{RELATED_END}}

    ### What
    {{DESCRIPTION}}

    ### How
    {{IMPLEMENTATION}}

  placeholders:
    issue_url: "{{ISSUE}}"
    related_pr_start: "{{RELATED_START}}"
    related_pr_end: "{{RELATED_END}}"
    description: "{{DESCRIPTION}}"
    implementation: "{{IMPLEMENTATION}}"
    tracking_line_prefix: "**Ticket**: {{ISSUE}}"
```

## License

MIT License - see [LICENSE](LICENSE) for details.