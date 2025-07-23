#![forbid(unsafe_code)]

use std::{fmt::Display, path::Path};

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
    #[error(transparent)]
    Path(#[from] path::Error),
}

pub struct Warning(String);

impl Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

struct FindResult {
    repos: Repos,
    warnings: Vec<Warning>,
}

enum Repos {
    InSearchRoot(repo::Repo),
    List(Vec<repo::Repo>),
}

/// Find all git repositories under root, recursively
fn find_repos(root: &Path, exclusion_pattern: Option<&regex::Regex>) -> Result<FindResult, Error> {
    let mut repos: Vec<repo::Repo> = Vec::new();
    let mut repo_in_root = false;
    let mut warnings = Vec::new();

    for path in tree::find_repo_paths(root)? {
        if exclusion_pattern
            .as_ref()
            .map(|regex| -> Result<bool, Error> {
                Ok(regex.is_match(&path::path_as_string(&path)?))
            })
            .transpose()?
            .unwrap_or(false)
        {
            warnings.push(Warning(format!(
                "[skipped] {}",
                &path::path_as_string(&path)?
            )));
            continue;
        }

        let is_worktree = repo::RepoHandle::detect_worktree(&path);
        if path == root {
            repo_in_root = true;
        }

        match repo::RepoHandle::open(&path, is_worktree) {
            Err(error) => {
                warnings.push(Warning(format!(
                    "Error opening repo {}{}: {}",
                    path.display(),
                    if is_worktree { " as worktree" } else { "" },
                    error
                )));
            }
            Ok(repo) => {
                let remotes = match repo.remotes() {
                    Ok(remote) => remote,
                    Err(error) => {
                        warnings.push(Warning(format!(
                            "{}: Error getting remotes: {}",
                            &path::path_as_string(&path)?,
                            error
                        )));
                        continue;
                    }
                };

                let mut results: Vec<repo::Remote> = Vec::new();
                for remote_name in remotes {
                    match repo.find_remote(&remote_name)? {
                        Some(remote) => {
                            let name = remote.name()?;
                            let url = remote.url()?;
                            let remote_type = match repo::detect_remote_type(&url) {
                                Ok(t) => t,
                                Err(e) => {
                                    warnings.push(Warning(format!(
                                        "{}: Could not handle URL {}. Reason: {}",
                                        &path::path_as_string(&path)?,
                                        &url,
                                        e
                                    )));
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
                            warnings.push(Warning(format!(
                                "{}: Remote {} not found",
                                &path::path_as_string(&path)?,
                                remote_name
                            )));
                        }
                    }
                }
                let remotes = results;

                let (namespace, name) = if path == root {
                    (
                        None,
                        if let Some(parent) = root.parent() {
                            path::path_as_string(
                                path.strip_prefix(parent)
                                    .expect("checked for prefix explicitly above"),
                            )?
                        } else {
                            warnings.push(Warning(String::from("Getting name of the search root failed. Do you have a git repository in \"/\"?")));
                            continue;
                        },
                    )
                } else {
                    let name = path
                        .strip_prefix(root)
                        .expect("checked for prefix explicitly above");
                    let namespace = name.parent().expect("path always has a parent");
                    (
                        if namespace != Path::new("") {
                            Some(path::path_as_string(namespace)?.clone())
                        } else {
                            None
                        },
                        path::path_as_string(name)?,
                    )
                };

                repos.push(repo::Repo {
                    name,
                    namespace,
                    remotes,
                    worktree_setup: is_worktree,
                });
            }
        }
    }
    Ok(FindResult {
        repos: if repo_in_root {
            #[expect(clippy::panic, reason = "potential bug")]
            Repos::InSearchRoot(if repos.len() != 1 {
                panic!("found multiple repos in root?")
            } else {
                repos
                    .pop()
                    .expect("checked len() above and list cannot be empty")
            })
        } else {
            Repos::List(repos)
        },
        warnings,
    })
}

pub fn find_in_tree(
    path: &Path,
    exclusion_pattern: Option<&regex::Regex>,
) -> Result<(tree::Tree, Vec<Warning>), Error> {
    let mut warnings = Vec::new();

    let mut result = find_repos(path, exclusion_pattern)?;

    warnings.append(&mut result.warnings);

    let (root, repos) = match result.repos {
        Repos::InSearchRoot(repo) => (
            path.parent()
                .ok_or(Error::CannotDetectRootDirectory)?
                .to_path_buf(),
            vec![repo],
        ),
        Repos::List(repos) => (path.to_path_buf(), repos),
    };

    Ok((
        tree::Tree {
            root: tree::Root::new(root),
            repos,
        },
        warnings,
    ))
}
