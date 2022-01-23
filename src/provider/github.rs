use std::collections::HashMap;

use isahc::prelude::*;
use serde::Deserialize;

use crate::{Remote, RemoteType, RepoConfig};

use super::Filter;
use super::Provider;
use super::SecretToken;

#[derive(Deserialize)]
#[serde(untagged)]
enum GithubUserProjectResponse {
    Success(Vec<GithubProject>),
    Failure(GithubFailureResponse),
}

#[derive(Deserialize)]
struct GithubProject {
    pub name: String,
    pub full_name: String,
    pub clone_url: String,
    pub ssh_url: String,
    pub private: bool,
}

impl GithubProject {
    fn into_repo_config(self) -> RepoConfig {
        RepoConfig {
            name: self.name,
            worktree_setup: false,
            remotes: Some(vec![Remote {
                name: String::from("github"),
                url: match self.private {
                    true => self.ssh_url,
                    false => self.clone_url,
                },
                remote_type: match self.private {
                    true => RemoteType::Ssh,
                    false => RemoteType::Https,
                },
            }]),
        }
    }
}

#[derive(Deserialize)]
struct GithubFailureResponse {
    pub message: String,
}

pub struct Github {
    filter: Filter,
    secret_token: SecretToken,
}

impl Github {
    fn get_repo_list_from_uri(
        uri: &str,
        secret_token: &SecretToken,
    ) -> Result<Vec<(String, GithubProject)>, String> {
        let mut repos: Vec<(String, GithubProject)> = vec![];

        let client = isahc::HttpClient::new().map_err(|error| error.to_string())?;

        let request = isahc::Request::builder()
            .uri(uri)
            .header("accept", " application/vnd.github.v3+json")
            .header("authorization", format!("token {}", secret_token))
            .body(())
            .map_err(|error| error.to_string())?;

        let mut response = client.send(request).map_err(|error| error.to_string())?;

        let success = response.status().is_success();

        {
            let response: GithubUserProjectResponse = response
                .json()
                .map_err(|error| format!("Failed deserializing response: {}", error))?;

            if !success {
                match response {
                    GithubUserProjectResponse::Failure(error) => return Err(error.message),
                    _ => return Err(String::from("Unknown response error")),
                }
            }

            match response {
                GithubUserProjectResponse::Failure(error) => {
                    return Err(format!(
                        "Received error response but no error code: {}",
                        error.message
                    ))
                }
                GithubUserProjectResponse::Success(repo_list) => {
                    for repo in repo_list {
                        let (namespace, _name) = repo
                            .full_name
                            .rsplit_once('/')
                            .unwrap_or(("", &repo.full_name));
                        repos.push((namespace.to_string(), repo));
                    }
                }
            }
        }

        let headers = response.headers();

        if let Some(link_header) = headers.get("link") {
            let link_header = link_header.to_str().map_err(|error| error.to_string())?;

            let link_header =
                parse_link_header::parse(link_header).map_err(|error| error.to_string())?;

            let next_page = link_header.get(&Some(String::from("next")));

            if let Some(page) = next_page {
                let following_repos = Github::get_repo_list_from_uri(&page.raw_uri, secret_token)?;
                repos.extend(following_repos);
            }
        }

        Ok(repos)
    }
}

impl Provider for Github {
    fn new(filter: Filter, secret_token: SecretToken) -> Self {
        Github {
            filter,
            secret_token,
        }
    }

    fn get_repos(&self) -> Result<HashMap<String, Vec<RepoConfig>>, String> {
        let mut namespaces: HashMap<String, HashMap<String, RepoConfig>> = HashMap::new();

        let mut register = |namespace: String, repo: GithubProject| {
            let name = repo.name.clone();
            let repo_config = repo.into_repo_config();
            match namespaces.get_mut(&namespace) {
                Some(ns) => match ns.get_mut(&name) {
                    Some(_entry) => {}
                    None => {
                        ns.insert(name, repo_config);
                    }
                },
                None => {
                    let mut ns = HashMap::new();
                    ns.insert(name, repo_config);
                    namespaces.insert(namespace, ns);
                }
            }
        };

        if let Some(users) = &self.filter.users {
            for user in users {
                let repos = Github::get_repo_list_from_uri(
                    &format!("https://api.github.com/users/{}/repos", user),
                    &self.secret_token,
                )?;
                for (namespace, repo) in repos {
                    register(namespace, repo);
                }
            }
        }

        if let Some(groups) = &self.filter.groups {
            for group in groups {
                let repos = Github::get_repo_list_from_uri(
                    &format!("https://api.github.com/orgs/{}/repos", group),
                    &self.secret_token,
                )?;
                for (namespace, repo) in repos {
                    register(namespace, repo);
                }
            }
        }

        if self.filter.owner {
            let repos = Github::get_repo_list_from_uri(
                "https://api.github.com/user/repos?affiliation=owner",
                &self.secret_token,
            )?;
            for (namespace, repo) in repos {
                register(namespace, repo);
            }
        }

        let mut ret: HashMap<String, Vec<RepoConfig>> = HashMap::new();
        for (namespace, repos) in namespaces {
            ret.insert(namespace, repos.into_values().collect());
        }

        Ok(ret)
    }
}
