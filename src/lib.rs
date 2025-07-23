#![forbid(unsafe_code)]

use std::path::Path;

use thiserror::Error;

pub mod auth;
pub mod config;
pub mod output;
pub mod path;
pub mod provider;
pub mod repo;
pub mod table;
pub mod tree;
pub mod worktree;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Repo(#[from] repo::Error),
    #[error(transparent)]
    Tree(#[from] tree::Error),
    #[error("invalid regex: {}", .message)]
    InvalidRegex { message: String },
    #[error("Cannot detect root directory. Are you working in /?")]
    CannotDetectRootDirectory,
}

/// Find all git repositories under root, recursively
///
/// The bool in the return value specifies whether there is a repository
/// in root itself.
#[allow(clippy::type_complexity)]
fn find_repos(
    root: &Path,
    exclusion_pattern: Option<&str>,
) -> Result<Option<(Vec<repo::Repo>, Vec<String>, bool)>, Error> {
    let mut repos: Vec<repo::Repo> = Vec::new();
    let mut repo_in_root = false;
    let mut warnings = Vec::new();

    let exlusion_regex: regex::Regex = regex::Regex::new(exclusion_pattern.unwrap_or(r"^$"))
        .map_err(|e| Error::InvalidRegex {
            message: e.to_string(),
        })?;
    for path in tree::find_repo_paths(root)? {
        if exclusion_pattern.is_some() && exlusion_regex.is_match(&path::path_as_string(&path)) {
            warnings.push(format!("[skipped] {}", &path::path_as_string(&path)));
            continue;
        }

        let is_worktree = repo::RepoHandle::detect_worktree(&path);
        if path == root {
            repo_in_root = true;
        }

        match repo::RepoHandle::open(&path, is_worktree) {
            Err(error) => {
                warnings.push(format!(
                    "Error opening repo {}{}: {}",
                    path.display(),
                    if is_worktree { " as worktree" } else { "" },
                    error
                ));
                continue;
            }
            Ok(repo) => {
                let remotes = match repo.remotes() {
                    Ok(remote) => remote,
                    Err(error) => {
                        warnings.push(format!(
                            "{}: Error getting remotes: {}",
                            &path::path_as_string(&path),
                            error
                        ));
                        continue;
                    }
                };

                let mut results: Vec<repo::Remote> = Vec::new();
                for remote_name in remotes {
                    match repo.find_remote(&remote_name)? {
                        Some(remote) => {
                            let name = remote.name();
                            let url = remote.url();
                            let remote_type = match repo::detect_remote_type(&url) {
                                Ok(t) => t,
                                Err(e) => {
                                    warnings.push(format!(
                                        "{}: Could not handle URL {}. Reason: {}",
                                        &path::path_as_string(&path),
                                        &url,
                                        e
                                    ));
                                    continue;
                                }
                            };

                            results.push(repo::Remote {
                                name,
                                url,
                                remote_type,
                            });
                        }
                        None => {
                            warnings.push(format!(
                                "{}: Remote {} not found",
                                &path::path_as_string(&path),
                                remote_name
                            ));
                            continue;
                        }
                    };
                }
                let remotes = results;

                let (namespace, name) = if path == root {
                    (
                        None,
                        match &root.parent() {
                            Some(parent) => {
                                path::path_as_string(path.strip_prefix(parent).unwrap())
                            }
                            None => {
                                warnings.push(String::from("Getting name of the search root failed. Do you have a git repository in \"/\"?"));
                                continue;
                            }
                        },
                    )
                } else {
                    let name = path.strip_prefix(root).unwrap();
                    let namespace = name.parent().unwrap();
                    (
                        if namespace == Path::new("") {
                            None
                        } else {
                            Some(path::path_as_string(namespace).to_string())
                        },
                        path::path_as_string(name),
                    )
                };

                repos.push(repo::Repo {
                    name,
                    namespace,
                    remotes: Some(remotes),
                    worktree_setup: is_worktree,
                });
            }
        }
    }
    Ok(Some((repos, warnings, repo_in_root)))
}

pub fn find_in_tree(
    path: &Path,
    exclusion_pattern: Option<&str>,
) -> Result<(tree::Tree, Vec<String>), Error> {
    let mut warnings = Vec::new();

    let (repos, repo_in_root): (Vec<repo::Repo>, bool) = match find_repos(path, exclusion_pattern)?
    {
        Some((vec, mut repo_warnings, repo_in_root)) => {
            warnings.append(&mut repo_warnings);
            (vec, repo_in_root)
        }
        None => (Vec::new(), false),
    };

    let mut root = path.to_path_buf();
    if repo_in_root {
        root = match root.parent() {
            Some(root) => root.to_path_buf(),
            None => {
                return Err(Error::CannotDetectRootDirectory);
            }
        }
    }

    Ok((
        tree::Tree {
            root: root.into_os_string().into_string().unwrap(),
            repos,
        },
        warnings,
    ))
}
