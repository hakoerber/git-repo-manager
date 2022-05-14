use serde::{Deserialize, Serialize};
use std::process;

use crate::output::*;

use super::repo::RepoConfig;
use std::path::Path;

use crate::get_token_from_command;
use crate::provider;
use crate::provider::Filter;
use crate::provider::Provider;

pub type RemoteProvider = crate::provider::RemoteProvider;

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Config {
    ConfigTree(ConfigTree),
    ConfigProvider(ConfigProvider),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigTree {
    pub trees: Trees,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigProviderFilter {
    pub access: Option<bool>,
    pub owner: Option<bool>,
    pub users: Option<Vec<String>>,
    pub groups: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigProvider {
    pub provider: RemoteProvider,
    pub token_command: String,
    pub root: String,
    pub filters: Option<ConfigProviderFilter>,

    pub force_ssh: Option<bool>,

    pub api_url: Option<String>,

    pub worktree: Option<bool>,
    pub init_worktree: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Trees(Vec<Tree>);

impl Trees {
    pub fn to_config(self) -> Config {
        Config::ConfigTree(ConfigTree { trees: self })
    }

    pub fn from_vec(vec: Vec<Tree>) -> Self {
        Trees(vec)
    }

    pub fn as_vec(self) -> Vec<Tree> {
        self.0
    }

    pub fn as_vec_ref(&self) -> &Vec<Tree> {
        self.0.as_ref()
    }
}

impl Config {
    pub fn trees(self) -> Result<Trees, String> {
        match self {
            Config::ConfigTree(config) => Ok(config.trees),
            Config::ConfigProvider(config) => {
                let token = match get_token_from_command(&config.token_command) {
                    Ok(token) => token,
                    Err(error) => {
                        print_error(&format!("Getting token from command failed: {}", error));
                        process::exit(1);
                    }
                };

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

                let repos = match config.provider {
                    RemoteProvider::Github => {
                        match provider::Github::new(filter, token, config.api_url) {
                            Ok(provider) => provider,
                            Err(error) => {
                                print_error(&format!("Error: {}", error));
                                process::exit(1);
                            }
                        }
                        .get_repos(
                            config.worktree.unwrap_or(false),
                            config.force_ssh.unwrap_or(false),
                        )?
                    }
                    RemoteProvider::Gitlab => {
                        match provider::Gitlab::new(filter, token, config.api_url) {
                            Ok(provider) => provider,
                            Err(error) => {
                                print_error(&format!("Error: {}", error));
                                process::exit(1);
                            }
                        }
                        .get_repos(
                            config.worktree.unwrap_or(false),
                            config.force_ssh.unwrap_or(false),
                        )?
                    }
                };

                let mut trees = vec![];

                for (namespace, namespace_repos) in repos {
                    let tree = Tree {
                        root: crate::path_as_string(&Path::new(&config.root).join(namespace)),
                        repos: Some(namespace_repos),
                    };
                    trees.push(tree);
                }
                Ok(Trees(trees))
            }
        }
    }

    pub fn from_trees(trees: Vec<Tree>) -> Self {
        Config::ConfigTree(ConfigTree {
            trees: Trees::from_vec(trees),
        })
    }

    pub fn normalize(&mut self) {
        if let Config::ConfigTree(config) = self {
            let home = super::env_home().display().to_string();
            for tree in &mut config.trees.0 {
                if tree.root.starts_with(&home) {
                    // The tilde is not handled differently, it's just a normal path component for `Path`.
                    // Therefore we can treat it like that during **output**.
                    //
                    // The `unwrap()` is safe here as we are testing via `starts_with()`
                    // beforehand
                    let mut path = tree.root.strip_prefix(&home).unwrap();
                    if path.starts_with('/') {
                        path = path.strip_prefix('/').unwrap();
                    }

                    tree.root = Path::new("~").join(path).display().to_string();
                }
            }
        }
    }

    pub fn as_toml(&self) -> Result<String, String> {
        match toml::to_string(self) {
            Ok(toml) => Ok(toml),
            Err(error) => Err(error.to_string()),
        }
    }

    pub fn as_yaml(&self) -> Result<String, String> {
        serde_yaml::to_string(self).map_err(|e| e.to_string())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tree {
    pub root: String,
    pub repos: Option<Vec<RepoConfig>>,
}

pub fn read_config<'a, T>(path: &str) -> Result<T, String>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            return Err(format!(
                "Error reading configuration file \"{}\": {}",
                path,
                match e.kind() {
                    std::io::ErrorKind::NotFound => String::from("not found"),
                    _ => e.to_string(),
                }
            ));
        }
    };

    let config: T = match toml::from_str(&content) {
        Ok(c) => c,
        Err(_) => match serde_yaml::from_str(&content) {
            Ok(c) => c,
            Err(e) => {
                return Err(format!(
                    "Error parsing configuration file \"{}\": {}",
                    path, e
                ))
            }
        },
    };

    Ok(config)
}
