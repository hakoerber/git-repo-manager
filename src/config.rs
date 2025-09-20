use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{
    RemoteName, auth,
    output::print_warning,
    path, provider,
    provider::{Filter, Provider},
    repo::{self, WorktreeSetup},
    tree,
};

#[derive(Debug, Deserialize, Serialize, clap::ValueEnum, Clone)]
pub enum RemoteProvider {
    #[serde(alias = "github", alias = "GitHub")]
    Github,
    #[serde(alias = "gitlab", alias = "GitLab")]
    Gitlab,
}

pub const WORKTREE_CONFIG_FILE_NAME: &str = "grm.toml";

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteType {
    Ssh,
    Https,
    File,
}

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
    pub trees: Vec<Tree>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User(String);

impl User {
    pub fn into_username(self) -> String {
        self.0
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Group(String);

impl Group {
    pub fn into_groupname(self) -> String {
        self.0
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigProviderFilter {
    pub access: Option<bool>,
    pub owner: Option<bool>,
    pub users: Option<Vec<User>>,
    pub groups: Option<Vec<Group>>,
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
pub struct Remote {
    pub name: String,
    pub url: String,
    #[serde(rename = "type")]
    pub remote_type: RemoteType,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Repo {
    pub name: String,

    #[serde(default = "worktree_setup_default")]
    pub worktree_setup: bool,

    pub remotes: Option<Vec<Remote>>,
}

impl ConfigTrees {
    pub fn to_config(self) -> Config {
        Config::ConfigTrees(self)
    }

    pub fn from_vec(vec: Vec<Tree>) -> Self {
        Self { trees: vec }
    }

    pub fn from_trees(vec: Vec<tree::Tree>) -> Self {
        Self {
            trees: vec.into_iter().map(Tree::from_tree).collect(),
        }
    }

    pub fn trees(self) -> Vec<Tree> {
        self.trees
    }

    pub fn trees_mut(&mut self) -> &mut Vec<Tree> {
        &mut self.trees
    }

    pub fn trees_ref(&self) -> &Vec<Tree> {
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
    #[error("Error reading configuration file \"{:?}\": {}", .path, .message)]
    ReadConfig { message: String, path: PathBuf },
    #[error("Error parsing configuration file \"{:?}\": {}", .path, .message)]
    ParseConfig { message: String, path: PathBuf },
    #[error("cannot strip prefix \"{:?}\" from \"{:?}\": {}", .prefix, .path, message)]
    StripPrefix {
        path: PathBuf,
        prefix: PathBuf,
        message: String,
    },
}

impl Config {
    pub fn get_trees(self) -> Result<Vec<Tree>, Error> {
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
                    print_warning(
                        "The configuration does not contain any filters, so no repos will match",
                    );
                }

                let repos = match config.provider {
                    RemoteProvider::Github => provider::Github::new(
                        filter,
                        token,
                        config.api_url.map(provider::Url::new),
                    )?
                    .get_repos(
                        config.worktree.unwrap_or(false).into(),
                        config.force_ssh.unwrap_or(false),
                        config.remote_name.map(RemoteName::new),
                    )?,
                    RemoteProvider::Gitlab => provider::Gitlab::new(
                        filter,
                        token,
                        config.api_url.map(provider::Url::new),
                    )?
                    .get_repos(
                        config.worktree.unwrap_or(false).into(),
                        config.force_ssh.unwrap_or(false),
                        config.remote_name.map(RemoteName::new),
                    )?,
                };

                let mut trees = vec![];

                #[expect(clippy::iter_over_hash_type, reason = "fine in this case")]
                for (namespace, namespace_repos) in repos {
                    let repos = namespace_repos.into_iter().map(Into::into).collect();
                    let tree = Tree {
                        root: Root(if let Some(namespace) = namespace {
                            PathBuf::from(&config.root).join(namespace.as_str())
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

    pub fn from_trees(trees: Vec<Tree>) -> Self {
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

                    tree.root = Root::new(Path::new("~").join(root.path()));
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
pub struct Root(PathBuf);

impl Root {
    pub fn new(s: PathBuf) -> Self {
        Self(s)
    }

    pub fn path(&self) -> &Path {
        self.0.as_path()
    }

    pub fn starts_with(&self, base: &Path) -> bool {
        self.0.as_path().starts_with(base)
    }

    pub fn strip_prefix(&self, prefix: &Path) -> Result<Self, Error> {
        Ok(Self(
            self.0
                .as_path()
                .strip_prefix(prefix)
                .map_err(|e| Error::StripPrefix {
                    path: self.0.clone(),
                    prefix: prefix.to_path_buf(),
                    message: e.to_string(),
                })?
                .to_path_buf(),
        ))
    }

    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tree {
    pub root: Root,
    pub repos: Option<Vec<Repo>>,
}

impl Tree {
    pub fn from_repos(root: &Path, repos: Vec<repo::Repo>) -> Self {
        Self {
            root: Root::new(root.to_path_buf()),
            repos: Some(repos.into_iter().map(Into::into).collect()),
        }
    }

    pub fn from_tree(tree: tree::Tree) -> Self {
        Self {
            root: tree.root.into(),
            repos: Some(tree.repos.into_iter().map(Into::into).collect()),
        }
    }
}

#[derive(Debug, Error)]
pub enum ReadConfigError {
    #[error("Configuration file not found at `{:?}`", .path)]
    NotFound { path: PathBuf },
    #[error("Error reading configuration file at `{:?}`: {}", .path, .message)]
    Generic { path: PathBuf, message: String },
    #[error("Error parsing configuration file at `{:?}`: {}", .path, .message)]
    Parse { path: PathBuf, message: String },
}

pub fn read_config<'a, T>(path: &Path) -> Result<T, ReadConfigError>
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackingConfig {
    pub default: bool,
    pub default_remote: String,
    pub default_remote_prefix: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorktreeRootConfig {
    pub persistent_branches: Option<Vec<String>>,
    pub track: Option<TrackingConfig>,
}

pub fn read_worktree_root_config(
    worktree_root: &Path,
) -> Result<Option<WorktreeRootConfig>, Error> {
    let path = worktree_root.join(WORKTREE_CONFIG_FILE_NAME);
    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => return Ok(None),
            _ => {
                return Err(Error::ReadConfig {
                    message: e.to_string(),
                    path,
                });
            }
        },
    };

    let config: WorktreeRootConfig = match toml::from_str(&content) {
        Ok(c) => c,
        Err(e) => {
            return Err(Error::ParseConfig {
                message: e.to_string(),
                path,
            });
        }
    };

    Ok(Some(config))
}
