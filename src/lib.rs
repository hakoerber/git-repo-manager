#![forbid(unsafe_code)]

use std::{
    borrow::Cow,
    fmt::{self, Display},
    path::Path,
};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchName(String);

impl fmt::Display for BranchName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl BranchName {
    pub fn new(from: String) -> Self {
        Self(from)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteName(Cow<'static, str>);

impl fmt::Display for RemoteName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl RemoteName {
    pub fn new(from: String) -> Self {
        Self(Cow::Owned(from))
    }

    pub const fn new_static(from: &'static str) -> Self {
        Self(Cow::Borrowed(from))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        match self.0 {
            Cow::Borrowed(s) => s.to_owned(),
            Cow::Owned(s) => s,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteUrl(String);

impl fmt::Display for RemoteUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl RemoteUrl {
    pub fn new(from: String) -> Self {
        Self(from)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmoduleName(String);

impl fmt::Display for SubmoduleName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl SubmoduleName {
    pub fn new(from: String) -> Self {
        Self(from)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
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

        let worktree_setup = repo::RepoHandle::detect_worktree(&path);
        if path == root {
            repo_in_root = true;
        }

        match repo::RepoHandle::open(&path, worktree_setup) {
            Err(error) => {
                warnings.push(Warning(format!(
                    "Error opening repo {}{}: {}",
                    path.display(),
                    if worktree_setup.is_worktree() {
                        " as worktree"
                    } else {
                        ""
                    },
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
                    name: repo::ProjectName::new(name),
                    namespace: namespace.map(repo::ProjectNamespace::new),
                    remotes,
                    worktree_setup,
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
