use serde::{Deserialize, Serialize};
use std::path::Path;

use git2::{Cred, RemoteCallbacks, Repository};

use crate::output::*;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteType {
    Ssh,
    Https,
}

#[derive(Debug, PartialEq)]
pub enum RepoErrorKind {
    NotFound,
    Unknown(String),
}

#[derive(Debug)]
pub struct RepoError {
    pub kind: RepoErrorKind,
}

impl RepoError {
    fn new(kind: RepoErrorKind) -> RepoError {
        RepoError { kind }
    }
}

impl std::error::Error for RepoError {}

impl std::fmt::Display for RepoError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self.kind)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Remote {
    pub name: String,
    pub url: String,
    #[serde(rename = "type")]
    pub remote_type: RemoteType,
}

fn worktree_setup_default() -> bool {
    false
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Repo {
    pub name: String,

    #[serde(default = "worktree_setup_default")]
    pub worktree_setup: bool,

    pub remotes: Option<Vec<Remote>>,
}

pub struct RepoChanges {
    pub files_new: usize,
    pub files_modified: usize,
    pub files_deleted: usize,
}

pub enum SubmoduleStatus {
    Clean,
    Uninitialized,
    Changed,
    OutOfDate,
}

pub enum RemoteTrackingStatus {
    UpToDate,
    Ahead(usize),
    Behind(usize),
    Diverged(usize, usize),
}

pub struct RepoStatus {
    pub operation: Option<git2::RepositoryState>,

    pub empty: bool,

    pub remotes: Vec<String>,

    pub head: Option<String>,

    pub changes: Option<RepoChanges>,

    pub worktrees: usize,

    pub submodules: Vec<(String, SubmoduleStatus)>,

    pub branches: Vec<(String, Option<(String, RemoteTrackingStatus)>)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_ssh_remote() {
        assert_eq!(
            detect_remote_type("ssh://git@example.com"),
            Some(RemoteType::Ssh)
        );
        assert_eq!(detect_remote_type("git@example.git"), Some(RemoteType::Ssh));
    }

    #[test]
    fn check_https_remote() {
        assert_eq!(
            detect_remote_type("https://example.com"),
            Some(RemoteType::Https)
        );
        assert_eq!(
            detect_remote_type("https://example.com/test.git"),
            Some(RemoteType::Https)
        );
    }

    #[test]
    fn check_invalid_remotes() {
        assert_eq!(detect_remote_type("https//example.com"), None);
        assert_eq!(detect_remote_type("https:example.com"), None);
        assert_eq!(detect_remote_type("ssh//example.com"), None);
        assert_eq!(detect_remote_type("ssh:example.com"), None);
        assert_eq!(detect_remote_type("git@example.com"), None);
    }

    #[test]
    #[should_panic]
    fn check_unsupported_protocol_http() {
        detect_remote_type("http://example.com");
    }

    #[test]
    #[should_panic]
    fn check_unsupported_protocol_git() {
        detect_remote_type("git://example.com");
    }

    #[test]
    #[should_panic]
    fn check_unsupported_protocol_file() {
        detect_remote_type("file:///");
    }
}

pub fn detect_remote_type(remote_url: &str) -> Option<RemoteType> {
    let git_regex = regex::Regex::new(r"^[a-zA-Z]+@.*$").unwrap();
    if remote_url.starts_with("ssh://") {
        return Some(RemoteType::Ssh);
    }
    if git_regex.is_match(remote_url) && remote_url.ends_with(".git") {
        return Some(RemoteType::Ssh);
    }
    if remote_url.starts_with("https://") {
        return Some(RemoteType::Https);
    }
    if remote_url.starts_with("http://") {
        unimplemented!("Remotes using HTTP protocol are not supported");
    }
    if remote_url.starts_with("git://") {
        unimplemented!("Remotes using git protocol are not supported");
    }
    if remote_url.starts_with("file://") || remote_url.starts_with('/') {
        unimplemented!("Remotes using local protocol are not supported");
    }
    None
}

pub fn open_repo(path: &Path, is_worktree: bool) -> Result<Repository, RepoError> {
    let open_func = match is_worktree {
        true => Repository::open_bare,
        false => Repository::open,
    };
    let path = match is_worktree {
        true => path.join(super::GIT_MAIN_WORKTREE_DIRECTORY),
        false => path.to_path_buf(),
    };
    match open_func(path) {
        Ok(r) => Ok(r),
        Err(e) => match e.code() {
            git2::ErrorCode::NotFound => Err(RepoError::new(RepoErrorKind::NotFound)),
            _ => Err(RepoError::new(RepoErrorKind::Unknown(
                e.message().to_string(),
            ))),
        },
    }
}

pub fn init_repo(path: &Path, is_worktree: bool) -> Result<Repository, Box<dyn std::error::Error>> {
    match is_worktree {
        false => match Repository::init(path) {
            Ok(r) => Ok(r),
            Err(e) => Err(Box::new(e)),
        },
        true => match Repository::init_bare(path.join(super::GIT_MAIN_WORKTREE_DIRECTORY)) {
            Ok(r) => Ok(r),
            Err(e) => Err(Box::new(e)),
        },
    }
}

pub fn clone_repo(
    remote: &Remote,
    path: &Path,
    is_worktree: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let clone_target = match is_worktree {
        false => path.to_path_buf(),
        true => path.join(super::GIT_MAIN_WORKTREE_DIRECTORY),
    };

    print_action(&format!(
        "Cloning into \"{}\" from \"{}\"",
        &clone_target.display(),
        &remote.url
    ));
    match remote.remote_type {
        RemoteType::Https => {
            let mut builder = git2::build::RepoBuilder::new();
            builder.bare(is_worktree);
            match builder.clone(&remote.url, &clone_target) {
                Ok(_) => Ok(()),
                Err(e) => Err(Box::new(e)),
            }
        }
        RemoteType::Ssh => {
            let mut callbacks = RemoteCallbacks::new();
            callbacks.credentials(|_url, username_from_url, _allowed_types| {
                Cred::ssh_key_from_agent(username_from_url.unwrap())
            });

            let mut fo = git2::FetchOptions::new();
            fo.remote_callbacks(callbacks);

            let mut builder = git2::build::RepoBuilder::new();
            builder.bare(is_worktree);
            builder.fetch_options(fo);

            match builder.clone(&remote.url, &clone_target) {
                Ok(_) => Ok(()),
                Err(e) => Err(Box::new(e)),
            }
        }
    }
}

pub fn get_repo_status(repo: &git2::Repository) -> RepoStatus {
    let operation = match repo.state() {
        git2::RepositoryState::Clean => None,
        state => Some(state),
    };

    let empty = repo.is_empty().unwrap();

    let remotes = repo
        .remotes()
        .unwrap()
        .iter()
        .map(|repo_name| repo_name.unwrap().to_string())
        .collect::<Vec<String>>();

    let head = match empty {
        true => None,
        false => Some(repo.head().unwrap().shorthand().unwrap().to_string()),
    };

    let statuses = repo
        .statuses(Some(git2::StatusOptions::new().include_ignored(false)))
        .unwrap();

    let changes = match statuses.is_empty() {
        true => None,
        false => {
            let mut files_new = 0;
            let mut files_modified = 0;
            let mut files_deleted = 0;
            for status in statuses.iter() {
                let status_bits = status.status();
                if status_bits.intersects(
                    git2::Status::INDEX_MODIFIED
                        | git2::Status::INDEX_RENAMED
                        | git2::Status::INDEX_TYPECHANGE
                        | git2::Status::WT_MODIFIED
                        | git2::Status::WT_RENAMED
                        | git2::Status::WT_TYPECHANGE,
                ) {
                    files_modified += 1;
                } else if status_bits.intersects(git2::Status::INDEX_NEW | git2::Status::WT_NEW) {
                    files_new += 1;
                } else if status_bits
                    .intersects(git2::Status::INDEX_DELETED | git2::Status::WT_DELETED)
                {
                    files_deleted += 1;
                }
            }
            if (files_new, files_modified, files_deleted) == (0, 0, 0) {
                panic!(
                    "is_empty() returned true, but no file changes were detected. This is a bug!"
                );
            }
            Some(RepoChanges {
                files_new,
                files_modified,
                files_deleted,
            })
        }
    };

    let worktrees = repo.worktrees().unwrap().len();

    let mut submodules = Vec::new();
    for submodule in repo.submodules().unwrap() {
        let submodule_name = submodule.name().unwrap().to_string();

        let submodule_status;
        let status = repo
            .submodule_status(submodule.name().unwrap(), git2::SubmoduleIgnore::None)
            .unwrap();

        if status.intersects(
            git2::SubmoduleStatus::WD_INDEX_MODIFIED
                | git2::SubmoduleStatus::WD_WD_MODIFIED
                | git2::SubmoduleStatus::WD_UNTRACKED,
        ) {
            submodule_status = SubmoduleStatus::Changed;
        } else if status.is_wd_uninitialized() {
            submodule_status = SubmoduleStatus::Uninitialized;
        } else if status.is_wd_modified() {
            submodule_status = SubmoduleStatus::OutOfDate;
        } else {
            submodule_status = SubmoduleStatus::Clean;
        }

        submodules.push((submodule_name, submodule_status));
    }

    let mut branches = Vec::new();
    for (local_branch, _) in repo
        .branches(Some(git2::BranchType::Local))
        .unwrap()
        .map(|branch_name| branch_name.unwrap())
    {
        let branch_name = local_branch.name().unwrap().unwrap().to_string();
        let remote_branch = match local_branch.upstream() {
            Ok(remote_branch) => {
                let remote_branch_name = remote_branch.name().unwrap().unwrap().to_string();

                let (ahead, behind) = repo
                    .graph_ahead_behind(
                        local_branch.get().peel_to_commit().unwrap().id(),
                        remote_branch.get().peel_to_commit().unwrap().id(),
                    )
                    .unwrap();

                let remote_tracking_status = match (ahead, behind) {
                    (0, 0) => RemoteTrackingStatus::UpToDate,
                    (0, d) => RemoteTrackingStatus::Behind(d),
                    (d, 0) => RemoteTrackingStatus::Ahead(d),
                    (d1, d2) => RemoteTrackingStatus::Diverged(d1, d2),
                };
                Some((remote_branch_name, remote_tracking_status))
            }
            // Err => no remote branch
            Err(_) => None,
        };
        branches.push((branch_name, remote_branch));
    }

    RepoStatus {
        operation,
        empty,
        remotes,
        head,
        changes,
        worktrees,
        submodules,
        branches,
    }
}
