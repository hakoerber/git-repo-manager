use serde::{Deserialize, Serialize};

use super::repo::RepoConfig;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub trees: Trees,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Trees(Vec<Tree>);

impl Trees {
    pub fn to_config(self) -> Config {
        Config { trees: self }
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
    pub fn as_toml(&self) -> Result<String, String> {
        match toml::to_string(self) {
            Ok(toml) => Ok(toml),
            Err(error) => Err(error.to_string()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tree {
    pub root: String,
    pub repos: Option<Vec<RepoConfig>>,
}

pub fn read_config(path: &str) -> Result<Config, String> {
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

    let config: Config = match toml::from_str(&content) {
        Ok(c) => c,
        Err(e) => {
            return Err(format!(
                "Error parsing configuration file \"{}\": {}",
                path, e
            ))
        }
    };

    Ok(config)
}
