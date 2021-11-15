use serde::{Deserialize, Serialize};

use super::repo::Repo;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub trees: Vec<Tree>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tree {
    pub root: Option<String>,
    pub repos: Option<Vec<Repo>>,
}

pub fn read_config(path: &str) -> Result<Config, String> {
    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            return Err(format!(
                "Error reading configuration file \"{}\": {}",
                path, e
            ))
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
