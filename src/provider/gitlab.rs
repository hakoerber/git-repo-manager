use serde::Deserialize;

use super::auth;
use super::escape;
use super::ApiErrorResponse;
use super::Filter;
use super::JsonError;
use super::Project;
use super::Provider;

const ACCEPT_HEADER_JSON: &str = "application/json";
const GITLAB_API_BASEURL: &str = match option_env!("GITLAB_API_BASEURL") {
    Some(url) => url,
    None => "https://gitlab.com",
};

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GitlabVisibility {
    Private,
    Internal,
    Public,
}

#[derive(Deserialize)]
pub struct GitlabProject {
    #[serde(rename = "path")]
    pub name: String,
    pub path_with_namespace: String,
    pub http_url_to_repo: String,
    pub ssh_url_to_repo: String,
    pub visibility: GitlabVisibility,
}

#[derive(Deserialize)]
struct GitlabUser {
    pub username: String,
}

impl Project for GitlabProject {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn namespace(&self) -> Option<String> {
        if let Some((namespace, _name)) = self.path_with_namespace.rsplit_once('/') {
            Some(namespace.to_string())
        } else {
            None
        }
    }

    fn ssh_url(&self) -> String {
        self.ssh_url_to_repo.clone()
    }

    fn http_url(&self) -> String {
        self.http_url_to_repo.clone()
    }

    fn private(&self) -> bool {
        !matches!(self.visibility, GitlabVisibility::Public)
    }
}

#[derive(Deserialize)]
pub struct GitlabApiErrorResponse {
    #[serde(alias = "error_description", alias = "error")]
    pub message: String,
}

impl JsonError for GitlabApiErrorResponse {
    fn to_string(self) -> String {
        self.message
    }
}

pub struct Gitlab {
    filter: Filter,
    secret_token: auth::AuthToken,
    api_url_override: Option<String>,
}

impl Gitlab {
    fn api_url(&self) -> String {
        self.api_url_override
            .as_ref()
            .unwrap_or(&GITLAB_API_BASEURL.to_string())
            .trim_end_matches('/')
            .to_string()
    }
}

impl Provider for Gitlab {
    type Project = GitlabProject;
    type Error = GitlabApiErrorResponse;

    fn new(
        filter: Filter,
        secret_token: auth::AuthToken,
        api_url_override: Option<String>,
    ) -> Result<Self, String> {
        Ok(Self {
            filter,
            secret_token,
            api_url_override,
        })
    }

    fn filter(&self) -> &Filter {
        &self.filter
    }

    fn secret_token(&self) -> &auth::AuthToken {
        &self.secret_token
    }

    fn auth_header_key() -> &'static str {
        "bearer"
    }

    fn get_user_projects(
        &self,
        user: &str,
    ) -> Result<Vec<GitlabProject>, ApiErrorResponse<GitlabApiErrorResponse>> {
        self.call_list(
            &format!("{}/api/v4/users/{}/projects", self.api_url(), escape(user)),
            Some(ACCEPT_HEADER_JSON),
        )
    }

    fn get_group_projects(
        &self,
        group: &str,
    ) -> Result<Vec<GitlabProject>, ApiErrorResponse<GitlabApiErrorResponse>> {
        self.call_list(
            &format!(
                "{}/api/v4/groups/{}/projects?include_subgroups=true&archived=false",
                self.api_url(),
                escape(group),
            ),
            Some(ACCEPT_HEADER_JSON),
        )
    }

    fn get_accessible_projects(
        &self,
    ) -> Result<Vec<GitlabProject>, ApiErrorResponse<GitlabApiErrorResponse>> {
        self.call_list(
            &format!("{}/api/v4/projects", self.api_url(),),
            Some(ACCEPT_HEADER_JSON),
        )
    }

    fn get_current_user(&self) -> Result<String, ApiErrorResponse<GitlabApiErrorResponse>> {
        Ok(super::call::<GitlabUser, GitlabApiErrorResponse>(
            &format!("{}/api/v4/user", self.api_url()),
            Self::auth_header_key(),
            self.secret_token(),
            Some(ACCEPT_HEADER_JSON),
        )?
        .username)
    }
}
