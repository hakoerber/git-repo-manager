use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

use super::config;
use super::output::*;
use super::path;
use super::repo;
use super::worktree;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Config(#[from] config::Error),
    #[error(transparent)]
    Repo(#[from] repo::Error),
    #[error(transparent)]
    Worktree(#[from] worktree::Error),
    #[error("Failed to open \"{}\": Not found", .path)]
    NotFound { path: String },
    #[error("Failed to open \"{}\": {}", .path, .kind)]
    Open {
        path: String,
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
}

pub struct Tree {
    pub root: String,
    pub repos: Vec<repo::Repo>,
}

pub fn find_unmanaged_repos(
    root_path: &Path,
    managed_repos: &[repo::Repo],
) -> Result<Vec<PathBuf>, Error> {
    let mut unmanaged_repos = Vec::new();

    for repo_path in find_repo_paths(root_path)? {
        if !managed_repos
            .iter()
            .any(|r| Path::new(root_path).join(r.fullname()) == repo_path)
        {
            unmanaged_repos.push(repo_path);
        }
    }
    Ok(unmanaged_repos)
}

pub fn sync_trees(config: config::Config, init_worktree: bool) -> Result<bool, Error> {
    let mut failures = false;

    let mut unmanaged_repos_absolute_paths = vec![];
    let mut managed_repos_absolute_paths = vec![];

    let trees = config.get_trees()?;

    for tree in trees {
        let repos: Vec<repo::Repo> = tree
            .repos
            .unwrap_or_default()
            .into_iter()
            .map(|repo| repo.into_repo())
            .collect();

        let root_path = path::expand_path(Path::new(&tree.root));

        for repo in &repos {
            managed_repos_absolute_paths.push(root_path.join(repo.fullname()));
            match sync_repo(&root_path, repo, init_worktree) {
                Ok(()) => print_repo_success(&repo.name, "OK"),
                Err(error) => {
                    print_repo_error(&repo.name, &error.to_string());
                    failures = true;
                }
            }
        }

        match find_unmanaged_repos(&root_path, &repos) {
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
        print_warning(&format!(
            "Found unmanaged repository: \"{}\"",
            path::path_as_string(unmanaged_repo_absolute_path)
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
                    };
                }
            }
            Err(e) => {
                let path = path.display().to_string();
                return Err(match e.kind() {
                    std::io::ErrorKind::NotFound => Error::NotFound { path },
                    kind => Error::Open { path, kind },
                });
            }
        };
    }

    Ok(repos)
}

fn sync_repo(root_path: &Path, repo: &repo::Repo, init_worktree: bool) -> Result<(), Error> {
    let repo_path = root_path.join(repo.fullname());
    let actual_git_directory = get_actual_git_directory(&repo_path, repo.worktree_setup);

    let mut newly_created = false;

    // Syncing a repository can have a few different flows, depending on the repository
    // that is to be cloned and the local directory:
    //
    // * If the local directory already exists, we have to make sure that it matches the
    //   worktree configuration, as there is no way to convert. If the sync is supposed
    //   to be worktree-aware, but the local directory is not, we abort. Note that we could
    //   also automatically convert here. In any case, the other direction (converting a
    //   worktree repository to non-worktree) cannot work, as we'd have to throw away the
    //   worktrees.
    //
    // * If the local directory does not yet exist, we have to actually do something ;). If
    //   no remote is specified, we just initialize a new repository (git init) and are done.
    //
    //   If there are (potentially multiple) remotes configured, we have to clone. We assume
    //   that the first remote is the canonical one that we do the first clone from. After
    //   cloning, we just add the other remotes as usual (as if they were added to the config
    //   afterwards)
    //
    // Branch handling:
    //
    // Handling the branches on checkout is a bit magic. For minimum surprises, we just set
    // up local tracking branches for all remote branches.
    if repo_path.exists() && repo_path.read_dir()?.next().is_some() {
        if repo.worktree_setup && !actual_git_directory.exists() {
            return Err(Error::WorktreeExpected);
        };
    } else if repo.remotes.is_none() || repo.remotes.as_ref().unwrap().is_empty() {
        print_repo_action(
            &repo.name,
            "Repository does not have remotes configured, initializing new",
        );
        match repo::RepoHandle::init(&repo_path, repo.worktree_setup) {
            Ok(r) => {
                print_repo_success(&repo.name, "Repository created");
                Some(r)
            }
            Err(e) => {
                return Err(Error::InitFailed {
                    message: e.to_string(),
                });
            }
        };
    } else {
        let first = repo.remotes.as_ref().unwrap().first().unwrap();

        match repo::clone_repo(first, &repo_path, repo.worktree_setup) {
            Ok(()) => {
                print_repo_success(&repo.name, "Repository successfully cloned");
            }
            Err(e) => {
                return Err(Error::CloneFailed {
                    message: e.to_string(),
                });
            }
        };

        newly_created = true;
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
                worktree::add_worktree(&repo_path, &branch.name()?, None, false)?;
            }
            Err(_error) => print_repo_error(
                &repo.name,
                "Could not determine default branch, skipping worktree initializtion",
            ),
        }
    }
    if let Some(remotes) = &repo.remotes {
        let current_remotes: Vec<String> = repo_handle.remotes()?;

        for remote in remotes {
            let current_remote = repo_handle.find_remote(&remote.name)?;

            match current_remote {
                Some(current_remote) => {
                    let current_url = current_remote.url();

                    if remote.url != current_url {
                        print_repo_action(
                            &repo.name,
                            &format!("Updating remote {} to \"{}\"", &remote.name, &remote.url),
                        );
                        repo_handle.remote_set_url(&remote.name, &remote.url)?;
                    }
                }
                None => {
                    print_repo_action(
                        &repo.name,
                        &format!(
                            "Setting up new remote \"{}\" to \"{}\"",
                            &remote.name, &remote.url
                        ),
                    );
                    repo_handle.new_remote(&remote.name, &remote.url)?;
                }
            }
        }

        for current_remote in &current_remotes {
            if !remotes.iter().any(|r| &r.name == current_remote) {
                print_repo_action(
                    &repo.name,
                    &format!("Deleting remote \"{}\"", &current_remote),
                );
                repo_handle.remote_delete(current_remote)?;
            }
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
