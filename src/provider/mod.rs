pub mod github;
pub mod gitlab;

use std::{collections::HashMap, fmt};

pub use github::Github;
pub use gitlab::Gitlab;
use thiserror::Error;

use super::{auth, config, repo};

#[derive(Clone)]
pub struct User(String);

impl User {
    pub fn new(name: String) -> Self {
        Self(name)
    }
}

impl From<super::config::User> for User {
    fn from(value: super::config::User) -> Self {
        Self(value.into_username())
    }
}

impl fmt::Display for User {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone)]
pub struct Group(String);

impl Group {
    pub fn new(name: String) -> Self {
        Self(name)
    }
}

impl From<super::config::Group> for Group {
    fn from(value: super::config::Group) -> Self {
        Self(value.into_groupname())
    }
}

impl fmt::Display for Group {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

const DEFAULT_REMOTE_NAME: &str = "origin";

#[derive(Debug, Error)]
pub enum Error {
    #[error("response error: {0}")]
    Response(String),
    #[error("provider error: {0}")]
    Provider(String),
}

#[derive(Debug, clap::ValueEnum, Clone)]
pub enum RemoteProvider {
    Github,
    Gitlab,
}

impl From<config::RemoteProvider> for RemoteProvider {
    fn from(other: config::RemoteProvider) -> Self {
        match other {
            config::RemoteProvider::Github => Self::Github,
            config::RemoteProvider::Gitlab => Self::Gitlab,
        }
    }
}

pub fn escape(s: &str) -> String {
    url_escape::encode_component(s).to_string()
}

pub trait Project {
    fn into_repo_config(
        self,
        provider_name: &str,
        worktree_setup: bool,
        force_ssh: bool,
    ) -> repo::Repo
    where
        Self: Sized,
    {
        repo::Repo {
            name: self.name(),
            namespace: self.namespace(),
            worktree_setup,
            remotes: vec![repo::Remote {
                name: String::from(provider_name),
                url: if force_ssh || self.private() {
                    self.ssh_url()
                } else {
                    self.http_url()
                },
                remote_type: if force_ssh || self.private() {
                    repo::RemoteType::Ssh
                } else {
                    repo::RemoteType::Https
                },
            }],
        }
    }

    fn name(&self) -> String;
    fn namespace(&self) -> Option<String>;
    fn ssh_url(&self) -> String;
    fn http_url(&self) -> String;
    fn private(&self) -> bool;
}

#[derive(Clone)]
pub struct Filter {
    users: Vec<User>,
    groups: Vec<Group>,
    owner: bool,
    access: bool,
}

impl Filter {
    pub fn new(users: Vec<User>, groups: Vec<Group>, owner: bool, access: bool) -> Self {
        Self {
            users,
            groups,
            owner,
            access,
        }
    }

    pub fn empty(&self) -> bool {
        self.users.is_empty() && self.groups.is_empty() && !self.owner && !self.access
    }
}

#[derive(Debug, Error)]
pub enum ApiError<T>
where
    T: JsonError,
{
    Json(T),
    String(String),
}

impl<T> From<String> for ApiError<T>
where
    T: JsonError,
{
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl<T> From<ureq::http::header::ToStrError> for ApiError<T>
where
    T: JsonError,
{
    fn from(s: ureq::http::header::ToStrError) -> Self {
        Self::String(s.to_string())
    }
}

pub trait JsonError {
    fn to_string(self) -> String;
}

pub trait Provider {
    type Project: serde::de::DeserializeOwned + Project;
    type Error: serde::de::DeserializeOwned + JsonError;

    fn new(
        filter: Filter,
        secret_token: auth::AuthToken,
        api_url_override: Option<String>,
    ) -> Result<Self, Error>
    where
        Self: Sized;

    fn filter(&self) -> &Filter;
    fn secret_token(&self) -> &auth::AuthToken;
    fn auth_header_key() -> &'static str;

    fn get_user_projects(&self, user: &User) -> Result<Vec<Self::Project>, ApiError<Self::Error>>;

    fn get_group_projects(
        &self,
        group: &Group,
    ) -> Result<Vec<Self::Project>, ApiError<Self::Error>>;

    fn get_own_projects(&self) -> Result<Vec<Self::Project>, ApiError<Self::Error>> {
        self.get_user_projects(&self.get_current_user()?)
    }

    fn get_accessible_projects(&self) -> Result<Vec<Self::Project>, ApiError<Self::Error>>;

    fn get_current_user(&self) -> Result<User, ApiError<Self::Error>>;

    ///
    /// Calls the API at specific uri and expects a successful response of
    /// `Vec<T>` back, or an error response U
    ///
    /// Handles paging with "link" HTTP headers properly and reads all pages to
    /// the end.
    fn call_list(
        &self,
        uri: &str,
        accept_header: Option<&str>,
    ) -> Result<Vec<Self::Project>, ApiError<Self::Error>> {
        let mut results = vec![];

        match ureq::get(uri)
            .config()
            .http_status_as_error(false)
            .build()
            .header("accept", accept_header.unwrap_or("application/json"))
            .header(
                "authorization",
                &format!(
                    "{} {}",
                    Self::auth_header_key(),
                    &self.secret_token().access()
                ),
            )
            .call()
        {
            Err(ureq::Error::Http(error)) => return Err(format!("http error: {error}").into()),
            Err(e) => return Err(format!("unknown error: {e}").into()),
            Ok(mut response) => {
                if !response.status().is_success() {
                    let result: Self::Error = response
                        .body_mut()
                        .read_json()
                        .map_err(|error| format!("Failed deserializing error response: {error}"))?;
                    return Err(ApiError::Json(result));
                } else {
                    if let Some(link_header) = response.headers().get("link") {
                        let link_header = parse_link_header::parse(link_header.to_str()?)
                            .map_err(|error| error.to_string())?;

                        let next_page = link_header.get(&Some(String::from("next")));

                        if let Some(page) = next_page {
                            let following_repos = self.call_list(&page.raw_uri, accept_header)?;
                            results.extend(following_repos);
                        }
                    }

                    let result: Vec<Self::Project> = response
                        .body_mut()
                        .read_json()
                        .map_err(|error| format!("Failed deserializing response: {error}"))?;

                    results.extend(result);
                }
            }
        }

        Ok(results)
    }

    fn get_repos(
        &self,
        worktree_setup: bool,
        force_ssh: bool,
        remote_name: Option<String>,
    ) -> Result<HashMap<Option<String>, Vec<repo::Repo>>, Error> {
        let mut repos = vec![];

        if self.filter().owner {
            repos.extend(self.get_own_projects().map_err(|error| {
                Error::Response(match error {
                    ApiError::Json(x) => x.to_string(),
                    ApiError::String(s) => s,
                })
            })?);
        }

        if self.filter().access {
            let accessible_projects = self.get_accessible_projects().map_err(|error| {
                Error::Response(match error {
                    ApiError::Json(x) => x.to_string(),
                    ApiError::String(s) => s,
                })
            })?;

            for accessible_project in accessible_projects {
                let mut already_present = false;
                for repo in &repos {
                    if repo.name() == accessible_project.name()
                        && repo.namespace() == accessible_project.namespace()
                    {
                        already_present = true;
                    }
                }
                if !already_present {
                    repos.push(accessible_project);
                }
            }
        }

        for user in &self.filter().users {
            let user_projects = self.get_user_projects(user).map_err(|error| {
                Error::Response(match error {
                    ApiError::Json(x) => x.to_string(),
                    ApiError::String(s) => s,
                })
            })?;

            for user_project in user_projects {
                let mut already_present = false;
                for repo in &repos {
                    if repo.name() == user_project.name()
                        && repo.namespace() == user_project.namespace()
                    {
                        already_present = true;
                    }
                }
                if !already_present {
                    repos.push(user_project);
                }
            }
        }

        for group in &self.filter().groups {
            let group_projects = self.get_group_projects(group).map_err(|error| {
                Error::Response(format!(
                    "group \"{}\": {}",
                    group,
                    match error {
                        ApiError::Json(x) => x.to_string(),
                        ApiError::String(s) => s,
                    }
                ))
            })?;
            for group_project in group_projects {
                let mut already_present = false;
                for repo in &repos {
                    if repo.name() == group_project.name()
                        && repo.namespace() == group_project.namespace()
                    {
                        already_present = true;
                    }
                }

                if !already_present {
                    repos.push(group_project);
                }
            }
        }

        let mut ret: HashMap<Option<String>, Vec<repo::Repo>> = HashMap::new();

        let remote_name = remote_name.unwrap_or_else(|| DEFAULT_REMOTE_NAME.to_owned());

        for repo in repos {
            let namespace = repo.namespace();

            let mut repo = repo.into_repo_config(&remote_name, worktree_setup, force_ssh);

            // Namespace is already part of the hashmap key. I'm not too happy
            // about the data exchange format here.
            repo.remove_namespace();

            ret.entry(namespace).or_default().push(repo);
        }

        Ok(ret)
    }
}

fn call<T, U>(
    uri: &str,
    auth_header_key: &str,
    secret_token: &auth::AuthToken,
    accept_header: Option<&str>,
) -> Result<T, ApiError<U>>
where
    T: serde::de::DeserializeOwned,
    U: serde::de::DeserializeOwned + JsonError,
{
    let response = match ureq::get(uri)
        .header("accept", accept_header.unwrap_or("application/json"))
        .header(
            "authorization",
            &format!("{} {}", &auth_header_key, &secret_token.access()),
        )
        .call()
    {
        Err(ureq::Error::Http(error)) => return Err(format!("http error: {error}").into()),
        Err(e) => return Err(format!("unknown error: {e}").into()),
        Ok(mut response) => {
            if !response.status().is_success() {
                let result: U = response
                    .body_mut()
                    .read_json()
                    .map_err(|error| format!("Failed deserializing error response: {error}"))?;
                return Err(ApiError::Json(result));
            } else {
                response
                    .body_mut()
                    .read_json()
                    .map_err(|error| format!("Failed deserializing response: {error}"))?
            }
        }
    };

    Ok(response)
}
