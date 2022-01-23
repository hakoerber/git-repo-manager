pub mod github;

pub use github::Github;

use super::RepoConfig;

use std::collections::HashMap;

pub struct Filter {
    users: Option<Vec<String>>,
    groups: Option<Vec<String>>,
    owner: bool,
}

type SecretToken = String;

impl Filter {
    pub fn new(users: Option<Vec<String>>, groups: Option<Vec<String>>, owner: bool) -> Self {
        Filter {
            users,
            groups,
            owner,
        }
    }
}

pub trait Provider {
    fn new(filter: Filter, secret_token: SecretToken) -> Self;
    fn get_repos(&self) -> Result<HashMap<String, Vec<RepoConfig>>, String>;
}
