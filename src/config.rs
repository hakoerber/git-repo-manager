use std::{
    path::{Path, PathBuf},
    process,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{
    auth,
    output::{print_error, print_warning},
    path, provider,
    provider::{Filter, Provider},
    repo, tree,
};

pub type RemoteProvider = provider::RemoteProvider;
pub type RemoteType = repo::RemoteType;

fn worktree_setup_default() -> bool {
    false
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Config {
    ConfigTrees(ConfigTrees),
    ConfigProvider(ConfigProvider),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigTrees {
    pub trees: Vec<ConfigTree>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigProviderFilter {
    pub access: Option<bool>,
    pub owner: Option<bool>,
    pub users: Option<Vec<String>>,
    pub groups: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigProvider {
    pub provider: RemoteProvider,
    pub token_command: String,
    pub root: String,
    pub filters: Option<ConfigProviderFilter>,

    pub force_ssh: Option<bool>,

    pub api_url: Option<String>,

    pub worktree: Option<bool>,

    pub remote_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteConfig {
    pub name: String,
    pub url: String,
    #[serde(rename = "type")]
    pub remote_type: RemoteType,
}

impl RemoteConfig {
    pub fn from_remote(remote: repo::Remote) -> Self {
        Self {
            name: remote.name,
            url: remote.url,
            remote_type: remote.remote_type,
        }
    }

    pub fn into_remote(self) -> repo::Remote {
        repo::Remote {
            name: self.name,
            url: self.url,
            remote_type: self.remote_type,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepoConfig {
    pub name: String,

    #[serde(default = "worktree_setup_default")]
    pub worktree_setup: bool,

    pub remotes: Option<Vec<RemoteConfig>>,
}

impl RepoConfig {
    pub fn from_repo(repo: repo::Repo) -> Self {
        Self {
            name: repo.name,
            worktree_setup: repo.worktree_setup,
            remotes: Some(
                repo.remotes
                    .into_iter()
                    .map(|r| RemoteConfig::from_remote(r))
                    .collect(),
            ),
        }
    }

    pub fn into_repo(self) -> repo::Repo {
        let (namespace, name) = if let Some((namespace, name)) = self.name.rsplit_once('/') {
            (Some(namespace.to_owned()), name.to_owned())
        } else {
            (None, self.name)
        };

        repo::Repo {
            name,
            namespace,
            worktree_setup: self.worktree_setup,
            remotes: self
                .remotes
                .map(|remotes| remotes.into_iter().map(RemoteConfig::into_remote).collect())
                .unwrap_or_else(|| Vec::new()),
        }
    }
}

impl ConfigTrees {
    pub fn to_config(self) -> Config {
        Config::ConfigTrees(self)
    }

    pub fn from_vec(vec: Vec<ConfigTree>) -> Self {
        Self { trees: vec }
    }

    pub fn from_trees(vec: Vec<tree::Tree>) -> Self {
        Self {
            trees: vec.into_iter().map(ConfigTree::from_tree).collect(),
        }
    }

    pub fn trees(self) -> Vec<ConfigTree> {
        self.trees
    }

    pub fn trees_mut(&mut self) -> &mut Vec<ConfigTree> {
        &mut self.trees
    }

    pub fn trees_ref(&self) -> &Vec<ConfigTree> {
        self.trees.as_ref()
    }
}

#[derive(Error, Debug)]
pub enum SerializationError {
    #[error(transparent)]
    Toml(#[from] toml::ser::Error),
    #[error(transparent)]
    Yaml(#[from] serde_yaml::Error),
}

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Auth(#[from] auth::Error),
    #[error(transparent)]
    Provider(#[from] provider::Error),
    #[error(transparent)]
    Serialization(#[from] SerializationError),
    #[error(transparent)]
    Path(#[from] path::Error),
}

impl Config {
    pub fn get_trees(self) -> Result<Vec<ConfigTree>, Error> {
        match self {
            Self::ConfigTrees(config) => Ok(config.trees),
            Self::ConfigProvider(config) => {
                let token = auth::get_token_from_command(&config.token_command)?;

                let filters = config.filters.unwrap_or(ConfigProviderFilter {
                    access: Some(false),
                    owner: Some(false),
                    users: Some(vec![]),
                    groups: Some(vec![]),
                });

                let filter = Filter::new(
                    filters.users.unwrap_or_default(),
                    filters.groups.unwrap_or_default(),
                    filters.owner.unwrap_or(false),
                    filters.access.unwrap_or(false),
                );

                if filter.empty() {
                    print_warning(
                        "The configuration does not contain any filters, so no repos will match",
                    );
                }

                let repos = match config.provider {
                    RemoteProvider::Github => {
                        match provider::Github::new(filter, token, config.api_url) {
                            Ok(provider) => provider,
                            Err(error) => {
                                print_error(&format!("Error: {error}"));
                                process::exit(1);
                            }
                        }
                        .get_repos(
                            config.worktree.unwrap_or(false),
                            config.force_ssh.unwrap_or(false),
                            config.remote_name,
                        )?
                    }
                    RemoteProvider::Gitlab => {
                        match provider::Gitlab::new(filter, token, config.api_url) {
                            Ok(provider) => provider,
                            Err(error) => {
                                print_error(&format!("Error: {error}"));
                                process::exit(1);
                            }
                        }
                        .get_repos(
                            config.worktree.unwrap_or(false),
                            config.force_ssh.unwrap_or(false),
                            config.remote_name,
                        )?
                    }
                };

                let mut trees = vec![];

                #[expect(clippy::iter_over_hash_type, reason = "fine in this case")]
                for (namespace, namespace_repos) in repos {
                    let repos = namespace_repos
                        .into_iter()
                        .map(RepoConfig::from_repo)
                        .collect();
                    let tree = ConfigTree {
                        root: tree::Root::new(if let Some(namespace) = namespace {
                            PathBuf::from(&config.root).join(namespace)
                        } else {
                            PathBuf::from(&config.root)
                        }),
                        repos: Some(repos),
                    };
                    trees.push(tree);
                }
                Ok(trees)
            }
        }
    }

    pub fn from_trees(trees: Vec<ConfigTree>) -> Self {
        Self::ConfigTrees(ConfigTrees { trees })
    }

    pub fn normalize(&mut self) -> Result<(), Error> {
        if let &mut Self::ConfigTrees(ref mut config) = self {
            let home = path::env_home()?;
            for tree in &mut config.trees_mut().iter_mut() {
                if tree.root.starts_with(&home) {
                    // The tilde is not handled differently, it's just a normal path component for
                    // `Path`. Therefore we can treat it like that during
                    // **output**.
                    //
                    // The `unwrap()` is safe here as we are testing via `starts_with()`
                    // beforehand
                    #[expect(clippy::missing_panics_doc, reason = "explicit checks for prefixes")]
                    let root = {
                        let mut path = tree
                            .root
                            .strip_prefix(&home)
                            .expect("checked for HOME prefix explicitly");
                        if path.starts_with(Path::new("/")) {
                            path = path
                                .strip_prefix(Path::new("/"))
                                .expect("will always be an absolute path");
                        }
                        path
                    };

                    tree.root = tree::Root::new(Path::new("~").join(root.path()));
                }
            }
        }
        Ok(())
    }

    pub fn as_toml(&self) -> Result<String, SerializationError> {
        Ok(toml::to_string(self)?)
    }

    pub fn as_yaml(&self) -> Result<String, SerializationError> {
        Ok(serde_yaml::to_string(self)?)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigTree {
    pub root: tree::Root,
    pub repos: Option<Vec<RepoConfig>>,
}

impl ConfigTree {
    pub fn from_repos(root: &Path, repos: Vec<repo::Repo>) -> Self {
        Self {
            root: tree::Root::new(root.to_path_buf()),
            repos: Some(repos.into_iter().map(RepoConfig::from_repo).collect()),
        }
    }

    pub fn from_tree(tree: tree::Tree) -> Self {
        Self {
            root: tree.root,
            repos: Some(tree.repos.into_iter().map(RepoConfig::from_repo).collect()),
        }
    }
}

#[derive(Debug, Error)]
pub enum ReadConfigError {
    #[error("Configuration file not found at {}", .path)]
    NotFound { path: String },
    #[error("Error reading configuration file at `{}`: {}", .path, .message)]
    Generic { path: String, message: String },
    #[error("Error parsing configuration file at `{}`: {}", .path, .message)]
    Parse { path: String, message: String },
}

pub fn read_config<'a, T>(path: &str) -> Result<T, ReadConfigError>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let content = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            return Err(match e.kind() {
                std::io::ErrorKind::NotFound => ReadConfigError::NotFound {
                    path: path.to_owned(),
                },
                _ => ReadConfigError::Generic {
                    path: path.to_owned(),
                    message: e.to_string(),
                },
            });
        }
    };

    let config: T = match toml::from_str(&content) {
        Ok(c) => c,
        Err(_) => match serde_yaml::from_str(&content) {
            Ok(c) => c,
            Err(e) => {
                return Err(ReadConfigError::Parse {
                    path: path.to_owned(),
                    message: e.to_string(),
                });
            }
        },
    };

    Ok(config)
}
