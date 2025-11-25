# git-pr

A highly opinionated CLI tool for creating GitHub Pull Requests with automatic related PR tracking.

## Features

- **Smart Tag Detection**: Automatically extracts ticket/tag identifiers (e.g., `[TRACK-123]`) from commit messages
- **Related PR Tracking**: Automatically updates the "Related PRs" section in all PRs sharing the same tag
- **Reviewer Selection**: Interactive multi-select for choosing reviewers from repository collaborators
- **Base Branch Detection**: Intelligently suggests base branches based on git history
- **Tag Autocomplete**: Remembers previously used tags for quick selection

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

### Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `GITHUB_USER` | Yes | Your GitHub username (for fetching related PRs) |
| `JIRA_URL` | No | Base URL for Jira ticket links (e.g., `https://company.atlassian.net/browse/`) |
| `GIT_PR_CONFIG` | No | Custom config directory path (default: `~/.config/git-pr`) |

### Example Setup

```bash
export GITHUB_USER="your-username"
export JIRA_URL="https://yourcompany.atlassian.net/browse/"
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
├── config.rs     # Configuration and path management
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

1. **Branch Analysis**: Scans the current branch's commits to find potential base branches and extract commit messages

2. **Tag Detection**: Looks for patterns like `[TRACK-123]` in commit messages to auto-fill the PR title and tag

3. **PR Creation**: Uses the GitHub CLI to create the PR with your title, description, and reviewers

4. **Related PR Discovery**: Queries GitHub for your recent PRs and filters those with matching tags

5. **Bulk Update**: Updates the "Related PRs" section in all matching PRs so they reference each other

## PR Template

The generated PR body follows this structure:

```markdown
Tracked by [TRACK-123](https://jira.example.com/browse/TRACK-123)

Related PRs:
<!-- RELATED_PR -->
- owner/repo/pull/1 - (this pr)
- owner/repo/pull/2
<!-- /RELATED_PR -->

## This PR...

[Your description here]

## Considerations and implementation

[Your implementation details here]
```

## License

MIT License - see [LICENSE](LICENSE) for details.