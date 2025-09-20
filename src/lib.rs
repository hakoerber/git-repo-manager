#![forbid(unsafe_code)]

use std::{
    fmt::Display,
    panic,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

use thiserror::Error;

use crate::{
    config::{Config, ConfigProviderFilter, RemoteProvider, Root},
    provider::{Filter, ProtocolConfig, Provider as _},
    tree::Tree,
};

pub use repo::{BranchName, RemoteName, RemoteUrl, SubmoduleName};

pub mod auth;
pub mod config;
pub mod path;
pub mod provider;
pub mod repo;
pub mod table;
pub mod tree;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Repo(#[from] repo::Error),
    #[error(transparent)]
    Provider(#[from] provider::Error),
    #[error(transparent)]
    Tree(#[from] tree::Error),
    #[error(transparent)]
    Auth(#[from] auth::Error),
    #[error("Invalid regex: {message}")]
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

pub fn exec_with_result_channel<'scope, Args, Func, ReportFunc, R, Ret>(
    f: Func,
    r: ReportFunc,
    args: Args,
) -> Ret
where
    Func: for<'a> FnOnce(Args, &'a mpsc::SyncSender<R>) -> Ret + Send + 'scope,
    ReportFunc: for<'a> FnOnce(&'a mpsc::Receiver<R>) + Send + 'scope,
    Ret: Send,
    R: Send,
    Args: Send,
{
    let (tx, rx) = mpsc::sync_channel::<R>(0);

    thread::scope(|s| {
        let task = s.spawn(move || f(args, &tx));

        let reporter = s.spawn(move || r(&rx));

        if let Err(e) = reporter.join() {
            panic::resume_unwind(e);
        }

        match task.join() {
            Ok(ret) => ret,
            Err(e) => panic::resume_unwind(e),
        }
    })
}

pub fn send_msg<R>(sender: &mpsc::SyncSender<R>, message: R) {
    #[expect(
        clippy::missing_panics_doc,
        reason = "this is a clear bug, cannot be recovered anyway"
    )]
    sender
        .send(message)
        .expect("receiving channel must be open until we are done");
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
                    name: repo::RepoName::new(name),
                    namespace: namespace.map(repo::RepoNamespace::new),
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

pub enum SyncTreesMessage {
    SyncTreeMessage(Result<tree::SyncTreeMessage, (repo::RepoName, Error)>),
    GetTreeWarning(Warning),
}

pub fn get_trees(
    config: Config,
    result_channel: &mpsc::SyncSender<SyncTreesMessage>,
) -> Result<Vec<Tree>, Error> {
    match config {
        Config::ConfigTrees(config) => Ok(config.trees.into_iter().map(Into::into).collect()),
        Config::ConfigProvider(config) => {
            let token = auth::get_token_from_command(&config.token_command)?;

            let filters = config.filters.unwrap_or(ConfigProviderFilter {
                access: Some(false),
                owner: Some(false),
                users: Some(vec![]),
                groups: Some(vec![]),
            });

            let filter = Filter::new(
                filters
                    .users
                    .unwrap_or_default()
                    .into_iter()
                    .map(Into::into)
                    .collect(),
                filters
                    .groups
                    .unwrap_or_default()
                    .into_iter()
                    .map(Into::into)
                    .collect(),
                filters.owner.unwrap_or(false),
                filters.access.unwrap_or(false),
            );

            if filter.empty() {
                send_msg(
                    result_channel,
                    SyncTreesMessage::GetTreeWarning(Warning(
                        "The configuration does not contain any filters, so no repos will match"
                            .to_owned(),
                    )),
                );
            }

            let repos = match config.provider {
                RemoteProvider::Github => {
                    provider::Github::new(filter, token, config.api_url.map(provider::Url::new))?
                        .get_repos(
                            config.worktree.unwrap_or(false).into(),
                            if config.force_ssh.unwrap_or(false) {
                                ProtocolConfig::ForceSsh
                            } else {
                                ProtocolConfig::Default
                            },
                            config.remote_name.map(RemoteName::new),
                        )?
                }
                RemoteProvider::Gitlab => {
                    provider::Gitlab::new(filter, token, config.api_url.map(provider::Url::new))?
                        .get_repos(
                            config.worktree.unwrap_or(false).into(),
                            if config.force_ssh.unwrap_or(false) {
                                ProtocolConfig::ForceSsh
                            } else {
                                ProtocolConfig::Default
                            },
                            config.remote_name.map(RemoteName::new),
                        )?
                }
            };

            let mut trees = vec![];

            #[expect(clippy::iter_over_hash_type, reason = "fine in this case")]
            for (namespace, repos) in repos {
                let tree = Tree {
                    root: Root::from_path_buf(if let Some(namespace) = namespace {
                        PathBuf::from(&config.root).join(namespace.as_str())
                    } else {
                        PathBuf::from(&config.root)
                    })
                    .into(),
                    repos,
                };
                trees.push(tree);
            }
            Ok(trees)
        }
    }
}
