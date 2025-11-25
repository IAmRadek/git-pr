# git-pr

A highly opinionated CLI tool for creating GitHub Pull Requests with automatic related PR tracking.

## Features

- **Smart Tag Detection**: Automatically extracts ticket/tag identifiers (e.g., `[TRACK-123]`) from commit messages
- **Related PR Tracking**: Automatically updates the "Related PRs" section in all PRs sharing the same tag
- **Reviewer Selection**: Interactive multi-select for choosing reviewers from repository collaborators
- **Base Branch Detection**: Intelligently suggests base branches based on git history
- **Tag Autocomplete**: Remembers previously used tags for quick selection
- **Customizable Templates**: Configure your own PR body template and form fields via YAML configuration
- **Dynamic Form Fields**: Define custom prompts and input types for your PR workflow

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

### Generate Default Configuration

```bash
git-pr --init
```

### Configuration File

Create `~/.config/git-pr/config.yaml` with any of the following options:

```yaml
# Jira Integration Settings
jira:
  # Base URL for Jira ticket links
  # When set and a Jira-style tag is found in commits,
  # a "Tracked by [TAG](url)" line is prepended to the PR body
  url: "https://yourcompany.atlassian.net/browse/"

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
  # The PR body template with form field placeholders
  body: |
    Related PRs:
    <!-- RELATED_PR -->
    <!-- /RELATED_PR -->

    ## This PR...
    {{description}}

    ## Considerations and implementation
    {{implementation}}

  # Markers for the related PRs section (HTML comments for invisibility)
  markers:
    related_pr_start: "<!-- RELATED_PR -->"
    related_pr_end: "<!-- /RELATED_PR -->"

  # Form fields to prompt the user for
  fields:
    - name: description
      prompt: "What is this PR doing:"
      type: editor
      required: true

    - name: implementation
      prompt: "Considerations and implementation:"
      type: editor
      required: false
```

### Form Fields

Form fields define what information to collect when creating a PR. Each field creates a `{{field_name}}` placeholder in the template.

| Option | Description |
|--------|-------------|
| `name` | Field identifier, used as placeholder `{{name}}` in template |
| `prompt` | Text shown to the user when prompting for input |
| `type` | Input type: `editor` (multi-line, opens external editor) or `text` (single-line) |
| `required` | If `true`, the field cannot be left empty |
| `default` | Optional default value to pre-fill |

**Behavior:**
- Required fields must have a non-empty value
- Empty optional fields have their entire line removed from the body
- Fields are prompted in the order they are defined

### Template Placeholders

| Placeholder | Description |
|-------------|-------------|
| `{{field_name}}` | Replaced with user input for the corresponding form field |
| `<!-- RELATED_PR -->...<!-- /RELATED_PR -->` | Section that gets updated with related PRs across all matching PRs |

**Jira Tracking:** When `jira.url` is configured AND a Jira-style tag is detected from commits, a tracking link is automatically prepended to the PR body:

```
Tracked by [TRACK-123](https://yourcompany.atlassian.net/browse/TRACK-123)
```

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
4. Form fields (as defined in your config)
5. Reviewer selection (multi-select from available reviewers)

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
      --init           Generate a default configuration file
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

4. **Form Field Collection**: Prompts for each configured form field (description, implementation, or your custom fields)

5. **PR Creation**: Uses the GitHub CLI to create the PR with your configured template, filled-in fields, and reviewers

6. **Related PR Discovery**: Queries GitHub for your recent PRs and filters those with matching tags

7. **Bulk Update**: Updates the "Related PRs" section in all matching PRs so they reference each other

## Custom Template Examples

### Minimal Template

```yaml
template:
  body: |
    ## Description
    {{description}}
  
  fields:
    - name: description
      prompt: "Describe your changes:"
      type: editor
      required: true
```

### Template with Testing Field

```yaml
template:
  body: |
    Related PRs:
    <!-- RELATED_PR -->
    <!-- /RELATED_PR -->

    ## Summary
    {{summary}}

    ## Changes
    {{changes}}

    ## Testing
    {{testing}}

    ## Checklist
    - [ ] Tests added/updated
    - [ ] Documentation updated
    - [ ] Ready for review

  fields:
    - name: summary
      prompt: "Brief summary (one line):"
      type: text
      required: true

    - name: changes
      prompt: "Detailed list of changes:"
      type: editor
      required: true

    - name: testing
      prompt: "How was this tested:"
      type: text
      required: false
      default: "Manual testing"
```

### Template with Custom Markers

```yaml
template:
  body: |
    {{RELATED_START}}
    {{RELATED_END}}

    ### What
    {{what}}

    ### Why
    {{why}}

    ### How
    {{how}}

  markers:
    related_pr_start: "{{RELATED_START}}"
    related_pr_end: "{{RELATED_END}}"

  fields:
    - name: what
      prompt: "What does this PR do:"
      type: text
      required: true

    - name: why
      prompt: "Why is this change needed:"
      type: editor
      required: true

    - name: how
      prompt: "How is it implemented:"
      type: editor
      required: false
```

## License

MIT License - see [LICENSE](LICENSE) for details.