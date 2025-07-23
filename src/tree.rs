//! A `Tree` represents a collection of `Repo` instances under a shared root
//! directory.

use std::{
    fs,
    path::{Path, PathBuf},
};

use thiserror::Error;

use super::{
    config,
    output::{print_error, print_repo_action, print_repo_error, print_repo_success, print_warning},
    path, repo,
    worktree::{self, WorktreeName},
};

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Config(#[from] config::Error),
    #[error(transparent)]
    Repo(#[from] repo::Error),
    #[error(transparent)]
    Worktree(#[from] worktree::Error),
    #[error("Failed to open \"{:?}\": Not found", .path)]
    NotFound { path: PathBuf },
    #[error("Failed to open \"{:?}\": {}", .path, .kind)]
    Open {
        path: PathBuf,
        kind: std::io::ErrorKind,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Error accessing directory: {}", .message)]
    DirectoryAccess { message: String },
    #[error("Repo already exists, but is not using a worktree setup")]
    WorktreeExpected,
    #[error("Repo already exists, but is using a worktree setup")]
    WorktreeNotExpected,
    #[error("Repository failed during init: {}", .message)]
    InitFailed { message: String },
    #[error("Repository failed during clone: {}", .message)]
    CloneFailed { message: String },
    #[error(transparent)]
    Path(#[from] path::Error),
}

#[derive(Debug)]
pub struct Root(PathBuf);

impl Root {
    pub fn new(s: PathBuf) -> Self {
        Self(s)
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

#[derive(PartialEq, Eq)]
pub struct RepoPath(PathBuf);

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

pub fn sync_trees(config: config::Config, init_worktree: bool) -> Result<bool, Error> {
    let mut failures = false;

    let mut unmanaged_repos_absolute_paths = vec![];
    let mut managed_repos_absolute_paths = vec![];

    let trees: Vec<Tree> = config.get_trees()?.into_iter().map(Into::into).collect();

    for tree in trees {
        let root_path = path::expand_path(Path::new(&tree.root.0))?;

        for repo in &tree.repos {
            managed_repos_absolute_paths.push(RepoPath(root_path.join(repo.fullname().as_str())));
            match sync_repo(&root_path, repo, init_worktree) {
                Ok(()) => print_repo_success(repo.name.as_str(), "OK"),
                Err(error) => {
                    print_repo_error(repo.name.as_str(), &error.to_string());
                    failures = true;
                }
            }
        }

        match find_unmanaged_repos(&root_path, &tree.repos) {
            Ok(repos) => {
                for path in repos {
                    if !unmanaged_repos_absolute_paths.contains(&path) {
                        unmanaged_repos_absolute_paths.push(path);
                    }
                }
            }
            Err(error) => {
                print_error(&format!("Error getting unmanaged repos: {error}"));
                failures = true;
            }
        }
    }

    for unmanaged_repo_absolute_path in &unmanaged_repos_absolute_paths {
        if managed_repos_absolute_paths
            .iter()
            .any(|managed_repo_absolute_path| {
                managed_repo_absolute_path == unmanaged_repo_absolute_path
            })
        {
            continue;
        }
        print_warning(format!(
            "Found unmanaged repository: \"{}\"",
            path::path_as_string(&unmanaged_repo_absolute_path.0)?
        ));
    }

    Ok(!failures)
}

/// Finds repositories recursively, returning their path
pub fn find_repo_paths(path: &Path) -> Result<Vec<PathBuf>, Error> {
    let mut repos = Vec::new();

    let git_dir = path.join(".git");
    let git_worktree = path.join(worktree::GIT_MAIN_WORKTREE_DIRECTORY);

    if git_dir.exists() || git_worktree.exists() {
        repos.push(path.to_path_buf());
    } else {
        match fs::read_dir(path) {
            Ok(contents) => {
                for content in contents {
                    match content {
                        Ok(entry) => {
                            let path = entry.path();
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

fn sync_repo(root_path: &Path, repo: &repo::Repo, init_worktree: bool) -> Result<(), Error> {
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
        if repo.worktree_setup && !actual_git_directory.exists() {
            return Err(Error::WorktreeExpected);
        }
    } else if let Some(first) = repo.remotes.first() {
        match repo::clone_repo(first, &repo_path, repo.worktree_setup) {
            Ok(()) => {
                print_repo_success(repo.name.as_str(), "Repository successfully cloned");
            }
            Err(e) => {
                return Err(Error::CloneFailed {
                    message: e.to_string(),
                });
            }
        }

        newly_created = true;
    } else {
        print_repo_action(
            repo.name.as_str(),
            "Repository does not have remotes configured, initializing new",
        );
        match repo::RepoHandle::init(&repo_path, repo.worktree_setup) {
            Ok(_repo_handle) => {
                print_repo_success(repo.name.as_str(), "Repository created");
            }
            Err(e) => {
                return Err(Error::InitFailed {
                    message: e.to_string(),
                });
            }
        }
    }

    let repo_handle = match repo::RepoHandle::open(&repo_path, repo.worktree_setup) {
        Ok(repo) => repo,
        Err(error) => {
            if !repo.worktree_setup && repo::RepoHandle::open(&repo_path, true).is_ok() {
                return Err(Error::WorktreeNotExpected);
            } else {
                return Err(error.into());
            }
        }
    };

    if newly_created && repo.worktree_setup && init_worktree {
        match repo_handle.default_branch() {
            Ok(branch) => {
                worktree::add_worktree(
                    &repo_path,
                    &WorktreeName::new(branch.name()?.into_string()),
                    None,
                    false,
                )?;
            }
            Err(_error) => print_repo_error(
                repo.name.as_str(),
                "Could not determine default branch, skipping worktree initializtion",
            ),
        }
    }

    let current_remotes = repo_handle.remotes()?;

    for remote in &repo.remotes {
        let current_remote = repo_handle.find_remote(&remote.name)?;

        if let Some(current_remote) = current_remote {
            let current_url = current_remote.url()?;

            if remote.url != current_url {
                print_repo_action(
                    repo.name.as_str(),
                    &format!("Updating remote {} to \"{}\"", &remote.name, &remote.url),
                );
                repo_handle.remote_set_url(&remote.name, &remote.url)?;
            }
        } else {
            print_repo_action(
                repo.name.as_str(),
                &format!(
                    "Setting up new remote \"{}\" to \"{}\"",
                    &remote.name, &remote.url
                ),
            );
            repo_handle.new_remote(&remote.name, &remote.url)?;
        }
    }

    for current_remote in &current_remotes {
        if !repo.remotes.iter().any(|r| &r.name == current_remote) {
            print_repo_action(
                repo.name.as_str(),
                &format!("Deleting remote \"{}\"", &current_remote),
            );
            repo_handle.remote_delete(current_remote)?;
        }
    }

    Ok(())
}

fn get_actual_git_directory(path: &Path, is_worktree: bool) -> PathBuf {
    if is_worktree {
        path.join(worktree::GIT_MAIN_WORKTREE_DIRECTORY)
    } else {
        path.to_path_buf()
    }
}
