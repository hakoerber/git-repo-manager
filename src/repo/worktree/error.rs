use std::fmt;

use super::{BranchName, RemoteName, RepoChanges, WorktreeName, config, path, repo};

use camino::Utf8PathBuf as PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Libgit(#[from] git2::Error),
    #[error(transparent)]
    Repo(#[from] repo::Error),
    #[error(transparent)]
    Config(#[from] config::Error),
    #[error(transparent)]
    InvalidWorktreeName(#[from] WorktreeValidationError),
    #[error("Remote \"{name}\" not found")]
    RemoteNotFound { name: RemoteName },
    #[error("Cannot push to non-pushable remote \"{name}\"")]
    RemoteNotPushable { name: RemoteName },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Current directory does not contain a worktree setup")]
    NotAWorktreeSetup,
    #[error("Worktree {name} already exists")]
    WorktreeAlreadyExists { name: WorktreeName },
    #[error("Branch \"{0}\" not found")]
    BranchNotFound(BranchName),
    #[error("Worktree name is not valid utf-8")]
    WorktreeNameNotUtf8,
    #[error(transparent)]
    Path(#[from] path::Error),
    #[error("Could not determine base directory from \"{git_dir}\"")]
    InvalidBaseDirectory { git_dir: PathBuf },
}

#[derive(Debug, Error)]
#[error("invalid worktree name \"{name}\": {reason}")]
pub struct WorktreeValidationError {
    pub(super) name: String,
    pub(super) reason: WorktreeValidationErrorReason,
}

#[derive(Debug)]
pub enum WorktreeValidationErrorReason {
    SlashAtStartOrEnd,
    ConsecutiveSlashes,
    ContainsWhitespace,
}

impl fmt::Display for WorktreeValidationErrorReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match *self {
                Self::SlashAtStartOrEnd => "Cannot start or end with a slash",
                Self::ConsecutiveSlashes => "Cannot contain two consecutive slashes",
                Self::ContainsWhitespace => "Cannot contain whitespace",
            }
        )
    }
}

#[derive(Debug, Error)]
pub enum WorktreeRemoveError {
    #[error(transparent)]
    RepoError(repo::Error),
    #[error("Worktree at {0} does not exist")]
    DoesNotExist(PathBuf),
    #[error(
        "Branch \"{branch_name}\" is checked out in worktree \"{worktree_name}\", this does not look correct"
    )]
    BranchNameMismatch {
        worktree_name: WorktreeName,
        branch_name: BranchName,
    },
    #[error("Branch {0} not found")]
    BranchNotFound(BranchName),
    #[error("Changes found in worktree: {0}")]
    Changes(RepoChanges),
    #[error("Branch {branch_name} is not merged into any persistent branches")]
    NotMerged { branch_name: BranchName },
    #[error("Branch {branch_name} is not in line with remote branch")]
    NotInSyncWithRemote { branch_name: BranchName },
    #[error("Removing {path} failed: {error}")]
    RemoveError {
        path: PathBuf,
        error: std::io::Error,
    },
    #[error("Error getting directory entry {path}: {error}")]
    ReadDirectoryError {
        path: PathBuf,
        error: std::io::Error,
    },
}

impl From<repo::Error> for WorktreeRemoveError {
    fn from(value: repo::Error) -> Self {
        Self::RepoError(value)
    }
}

#[derive(Debug, Error)]
pub enum WorktreeConversionError {
    #[error(transparent)]
    RepoError(repo::Error),
    #[error("Changes found in worktree: {0}")]
    Changes(RepoChanges),
    #[error("Ignored files found")]
    Ignored,
    #[error("{}", .0)]
    RenameError(String),
    #[error(transparent)]
    Path(#[from] path::Error),
    #[error("Opening directory failed: {0}")]
    OpenDirectoryError(std::io::Error),
    #[error("Removing {path} failed: {error}")]
    RemoveError {
        path: PathBuf,
        error: std::io::Error,
    },
    #[error("Error getting directory entry: {0}")]
    ReadDirectoryError(std::io::Error),
}

impl From<repo::Error> for WorktreeConversionError {
    fn from(value: repo::Error) -> Self {
        Self::RepoError(value)
    }
}

#[derive(Debug, Error)]
pub enum CleanupWorktreeError {
    #[error(transparent)]
    RepoError(#[from] Error),
    #[error(transparent)]
    RemoveError(#[from] WorktreeRemoveError),
    #[error("Could not get default branch: {0}")]
    DefaultBranch(repo::Error),
    #[error("Branch name error: {0}")]
    BranchName(repo::Error),
    #[error("Branch \"{branch_name}\" not found")]
    BranchNotFound { branch_name: BranchName },
}

pub struct CleanupWorktreeWarning {
    pub worktree_name: WorktreeName,
    pub reason: CleanupWorktreeWarningReason,
}

pub enum CleanupWorktreeWarningReason {
    UncommittedChanges(RepoChanges),
    NotMerged { branch_name: BranchName },
    NoDirectory,
}
