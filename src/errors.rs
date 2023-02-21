#[derive(Debug)]
pub enum Error {
    NotInGitRepo,
    BranchNotClean,
    CannotBeInMainBranch(String),
}