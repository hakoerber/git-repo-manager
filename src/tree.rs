//! A `Tree` represents a collection of `Repo` instances under a shared root
//! directory.

use std::{fmt, fs, sync::mpsc};

use camino::{Utf8Path as Path, Utf8PathBuf as PathBuf};
use thiserror::Error;

use super::{
    RemoteName, RemoteUrl, SyncTreesMessage, config, path,
    repo::{self, RepoName, TrackingSelection, WorktreeName, WorktreeRepoHandle, WorktreeSetup},
    send_msg,
};

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Config(#[from] config::Error),
    #[error(transparent)]
    Repo(#[from] repo::Error),
    #[error(transparent)]
    Worktree(#[from] repo::WorktreeError),
    #[error("Failed to open \"{path}\": Not found")]
    NotFound { path: PathBuf },
    #[error("Failed to open \"{path}\": {kind}")]
    Open {
        path: PathBuf,
        kind: std::io::ErrorKind,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Error accessing directory: {message}")]
    DirectoryAccess { message: String },
    #[error("Repo already exists, but is not using a worktree setup")]
    WorktreeExpected,
    #[error("Repo already exists, but is using a worktree setup")]
    WorktreeNotExpected,
    #[error("Repository failed during init: {message}")]
    InitFailed { message: String },
    #[error("Repository failed during clone: {message}")]
    CloneFailed { message: String },
    #[error("Could not get trees from config: {message}")]
    TreesFromConfig { message: String },
    #[error(transparent)]
    Path(#[from] path::Error),
    #[error(transparent)]
    WorktreeValidation(#[from] repo::WorktreeValidationError),
}

#[derive(Debug)]
pub struct Root(PathBuf);

impl Root {
    pub fn new(s: PathBuf) -> Self {
        Self(s)
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }
}

impl From<config::Root> for Root {
    fn from(other: config::Root) -> Self {
        Self::new(other.into_path_buf())
    }
}

impl From<Root> for config::Root {
    fn from(other: Root) -> Self {
        Self::new(other.into_path_buf())
    }
}

pub struct Tree {
    pub root: Root,
    pub repos: Vec<repo::Repo>,
}

impl From<config::Tree> for Tree {
    fn from(other: config::Tree) -> Self {
        Self {
            root: other.root.into(),
            repos: other
                .repos
                .map(|repos| repos.into_iter().map(Into::into).collect())
                .unwrap_or_default(),
        }
    }
}

#[derive(PartialEq, Eq, Debug)]
pub struct RepoPath(PathBuf);

impl RepoPath {
    pub fn as_path(&self) -> &Path {
        self.0.as_path()
    }
}

impl fmt::Display for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub fn find_unmanaged_repos(
    root_path: &Path,
    managed_repos: &[repo::Repo],
) -> Result<Vec<RepoPath>, Error> {
    let mut unmanaged_repos = Vec::new();

    for path in find_repo_paths(root_path)? {
        if !managed_repos
            .iter()
            .any(|r| Path::new(root_path).join(r.fullname().as_str()) == path)
        {
            unmanaged_repos.push(RepoPath(path));
        }
    }
    Ok(unmanaged_repos)
}

#[derive(PartialEq, Eq, Copy, Clone)]
pub enum OperationResult {
    Success,
    Failure,
}

impl OperationResult {
    pub fn is_success(self) -> bool {
        self == Self::Success
    }

    pub fn is_failure(self) -> bool {
        !self.is_success()
    }
}

pub enum SyncTreeMessage {
    Cloning((PathBuf, RemoteUrl)),
    Cloned(RepoName),
    Init(RepoName),
    Created(RepoName),
    SyncDone(RepoName),
    SkippingWorktreeInit(RepoName),
    UpdatingRemote((RepoName, RemoteName, RemoteUrl)),
    CreateRemote((RepoName, RemoteName, RemoteUrl)),
    DeleteRemote((RepoName, RemoteName)),
}

pub fn sync_trees(
    trees: Vec<Tree>,
    init_worktree: bool,
    result_channel: &mpsc::SyncSender<SyncTreesMessage>,
) -> Result<(OperationResult, Vec<RepoPath>), Error> {
    let mut failures = false;

    let mut unmanaged_repos = vec![];
    let mut managed_repos = vec![];

    for tree in trees {
        let root_path = path::expand_path(Path::new(&tree.root.0))?;

        for repo in &tree.repos {
            managed_repos.push(RepoPath(root_path.join(repo.fullname().as_str())));
            match sync_repo(&root_path, repo, init_worktree, result_channel) {
                Ok(()) => {
                    send_msg(
                        result_channel,
                        SyncTreesMessage::SyncTreeMessage(Ok(SyncTreeMessage::SyncDone(
                            repo.name.clone(),
                        ))),
                    );
                }
                Err(error) => {
                    send_msg(
                        result_channel,
                        SyncTreesMessage::SyncTreeMessage(Err((repo.name.clone(), error.into()))),
                    );
                    failures = true;
                }
            }
        }

        unmanaged_repos.extend(find_unmanaged_repos(&root_path, &tree.repos)?);
    }

    // It's possible that trees are nested or share a root, which means that a
    // repo that is managed by one tree is detected as unmanaged in another tree.
    // So we need to remove all unmanaged trees that are part of *any* tree.
    unmanaged_repos.retain(|unmanaged_path| {
        !managed_repos
            .iter()
            .any(|managed_path| unmanaged_path == managed_path)
    });

    Ok((
        if failures {
            OperationResult::Failure
        } else {
            OperationResult::Success
        },
        unmanaged_repos,
    ))
}

/// Finds repositories recursively, returning their path
pub fn find_repo_paths(path: &Path) -> Result<Vec<PathBuf>, Error> {
    let mut repos = Vec::new();

    let git_dir = path.join(".git");
    let git_worktree = path.join(repo::GIT_MAIN_WORKTREE_DIRECTORY);

    if git_dir.exists() || git_worktree.exists() {
        repos.push(path.to_path_buf());
    } else {
        match fs::read_dir(path) {
            Ok(contents) => {
                for content in contents {
                    match content {
                        Ok(entry) => {
                            let path = path::from_std_path_buf(entry.path())?;
                            if path.is_symlink() {
                                continue;
                            }
                            if path.is_dir() {
                                match find_repo_paths(&path) {
                                    Ok(ref mut r) => repos.append(r),
                                    Err(error) => return Err(error),
                                }
                            }
                        }
                        Err(e) => {
                            return Err(Error::DirectoryAccess {
                                message: e.to_string(),
                            });
                        }
                    }
                }
            }
            Err(e) => {
                return Err(match e.kind() {
                    std::io::ErrorKind::NotFound => Error::NotFound {
                        path: path.to_path_buf(),
                    },
                    kind => Error::Open {
                        path: path.to_path_buf(),
                        kind,
                    },
                });
            }
        }
    }

    Ok(repos)
}

fn sync_repo(
    root_path: &Path,
    repo: &repo::Repo,
    init_worktree: bool,
    result_channel: &mpsc::SyncSender<SyncTreesMessage>,
) -> Result<(), Error> {
    let repo_path = root_path.join(repo.fullname().as_str());
    let actual_git_directory = get_actual_git_directory(&repo_path, repo.worktree_setup);

    let mut newly_created = false;

    // Syncing a repository can have a few different flows, depending on the
    // repository that is to be cloned and the local directory:
    //
    // * If the local directory already exists, we have to make sure that it matches
    //   the worktree configuration, as there is no way to convert. If the sync is
    //   supposed to be worktree-aware, but the local directory is not, we abort.
    //   Note that we could also automatically convert here. In any case, the other
    //   direction (converting a worktree repository to non-worktree) cannot work,
    //   as we'd have to throw away the worktrees.
    //
    // * If the local directory does not yet exist, we have to actually do something
    //   ;). If no remote is specified, we just initialize a new repository (git
    //   init) and are done.
    //
    //   If there are (potentially multiple) remotes configured, we have to clone.
    // We assume   that the first remote is the canonical one that we do the
    // first clone from. After   cloning, we just add the other remotes as usual
    // (as if they were added to the config   afterwards)
    //
    // Branch handling:
    //
    // Handling the branches on checkout is a bit magic. For minimum surprises, we
    // just set up local tracking branches for all remote branches.
    if repo_path.exists() && repo_path.read_dir()?.next().is_some() {
        if repo.worktree_setup.is_worktree() && !actual_git_directory.exists() {
            return Err(Error::WorktreeExpected);
        }
    } else if let Some(first) = repo.remotes.first() {
        send_msg(
            result_channel,
            SyncTreesMessage::SyncTreeMessage(Ok(SyncTreeMessage::Cloning((
                repo_path.clone(),
                first.url.clone(),
            )))),
        );

        match repo::clone_repo(first, &repo_path, repo.worktree_setup) {
            Ok(()) => send_msg(
                result_channel,
                SyncTreesMessage::SyncTreeMessage(Ok(SyncTreeMessage::Cloned(repo.name.clone()))),
            ),
            Err(e) => {
                return Err(Error::CloneFailed {
                    message: e.to_string(),
                });
            }
        }

        newly_created = true;
    } else {
        send_msg(
            result_channel,
            SyncTreesMessage::SyncTreeMessage(Ok(SyncTreeMessage::Init(repo.name.clone()))),
        );
        match repo::RepoHandle::init(&repo_path, repo.worktree_setup) {
            Ok(_repo_handle) => {
                send_msg(
                    result_channel,
                    SyncTreesMessage::SyncTreeMessage(Ok(SyncTreeMessage::Created(
                        repo.name.clone(),
                    ))),
                );
            }
            Err(e) => {
                return Err(Error::InitFailed {
                    message: e.to_string(),
                });
            }
        }
    }

    let repo_handle =
        match repo::RepoHandle::open_with_worktree_setup(&repo_path, repo.worktree_setup) {
            Ok(repo) => repo,
            Err(error) => {
                if !repo.worktree_setup.is_worktree()
                    && repo::WorktreeRepoHandle::open(&repo_path).is_ok()
                {
                    return Err(Error::WorktreeNotExpected);
                } else {
                    return Err(error.into());
                }
            }
        };

    let repo_handle = if newly_created && repo.worktree_setup.is_worktree() && init_worktree {
        let repo_handle = WorktreeRepoHandle::from_handle_unchecked(repo_handle);

        match repo_handle.default_branch() {
            Ok(branch) => {
                repo::add_worktree(
                    &repo_handle,
                    &WorktreeName::new(branch.name()?.into_string())?,
                    &TrackingSelection::Automatic,
                )?;
            }
            Err(_error) => send_msg(
                result_channel,
                SyncTreesMessage::SyncTreeMessage(Ok(SyncTreeMessage::SkippingWorktreeInit(
                    repo.name.clone(),
                ))),
            ),
        }

        repo_handle.into_handle()
    } else {
        repo_handle
    };

    let current_remotes = repo_handle.remotes()?;

    for remote in &repo.remotes {
        let current_remote = repo_handle.find_remote(&remote.name)?;

        if let Some(current_remote) = current_remote {
            let current_url = current_remote.url()?;

            if remote.url != current_url {
                send_msg(
                    result_channel,
                    SyncTreesMessage::SyncTreeMessage(Ok(SyncTreeMessage::UpdatingRemote((
                        repo.name.clone(),
                        remote.name.clone(),
                        remote.url.clone(),
                    )))),
                );
                repo_handle.remote_set_url(&remote.name, &remote.url)?;
            }
        } else {
            send_msg(
                result_channel,
                SyncTreesMessage::SyncTreeMessage(Ok(SyncTreeMessage::CreateRemote((
                    repo.name.clone(),
                    remote.name.clone(),
                    remote.url.clone(),
                )))),
            );
            repo_handle.new_remote(&remote.name, &remote.url)?;
        }
    }

    for current_remote in &current_remotes {
        if !repo.remotes.iter().any(|r| &r.name == current_remote) {
            send_msg(
                result_channel,
                SyncTreesMessage::SyncTreeMessage(Ok(SyncTreeMessage::DeleteRemote((
                    repo.name.clone(),
                    current_remote.clone(),
                )))),
            );
            repo_handle.remote_delete(current_remote)?;
        }
    }

    Ok(())
}

fn get_actual_git_directory(path: &Path, worktree_setup: WorktreeSetup) -> PathBuf {
    if worktree_setup.is_worktree() {
        path.join(repo::GIT_MAIN_WORKTREE_DIRECTORY)
    } else {
        path.to_path_buf()
    }
}
