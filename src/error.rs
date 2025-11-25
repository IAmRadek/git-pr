use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Not in a git repository")]
    NotInGitRepo,

    #[error("Branch has uncommitted changes")]
    BranchNotClean,

    #[error("Cannot run from main branch: {0}")]
    CannotBeInMainBranch(String),

    #[error("No commits found on current branch")]
    NoCommits,

    #[error("Git error: {0}")]
    Git(#[from] git2::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("GitHub CLI error: {0}")]
    GitHubCli(String),

    #[error("Environment variable not set: {0}")]
    EnvVar(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("User cancelled operation")]
    Cancelled,

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Prompt error: {0}")]
    Prompt(String),
}

impl From<inquire::error::InquireError> for Error {
    fn from(err: inquire::error::InquireError) -> Self {
        match err {
            inquire::error::InquireError::OperationCanceled => Error::Cancelled,
            inquire::error::InquireError::OperationInterrupted => Error::Cancelled,
            other => Error::InvalidInput(other.to_string()),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
