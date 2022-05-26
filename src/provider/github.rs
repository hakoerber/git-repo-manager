use serde::Deserialize;

use super::ApiErrorResponse;
use super::Filter;
use super::JsonError;
use super::Project;
use super::Provider;
use super::SecretToken;

const PROVIDER_NAME: &str = "github";
const ACCEPT_HEADER_JSON: &str = "application/vnd.github.v3+json";
const GITHUB_API_BASEURL: &str =
    option_env!("GITHUB_API_BASEURL").unwrap_or("https://api.github.com");

#[derive(Deserialize)]
pub struct GithubProject {
    pub name: String,
    pub full_name: String,
    pub clone_url: String,
    pub ssh_url: String,
    pub private: bool,
}

#[derive(Deserialize)]
struct GithubUser {
    #[serde(rename = "login")]
    pub username: String,
}

impl Project for GithubProject {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn namespace(&self) -> Option<String> {
        if let Some((namespace, _name)) = self.full_name.rsplit_once('/') {
            Some(namespace.to_string())
        } else {
            None
        }
    }

    fn ssh_url(&self) -> String {
        self.ssh_url.clone()
    }

    fn http_url(&self) -> String {
        self.clone_url.clone()
    }

    fn private(&self) -> bool {
        self.private
    }
}

#[derive(Deserialize)]
pub struct GithubApiErrorResponse {
    pub message: String,
}

impl JsonError for GithubApiErrorResponse {
    fn to_string(self) -> String {
        self.message
    }
}

pub struct Github {
    filter: Filter,
    secret_token: SecretToken,
}

impl Provider for Github {
    type Project = GithubProject;
    type Error = GithubApiErrorResponse;

    fn new(
        filter: Filter,
        secret_token: SecretToken,
        api_url_override: Option<String>,
    ) -> Result<Self, String> {
        if api_url_override.is_some() {
            return Err("API URL overriding is not supported for Github".to_string());
        }
        Ok(Self {
            filter,
            secret_token,
        })
    }

    fn name(&self) -> String {
        String::from(PROVIDER_NAME)
    }

    fn filter(&self) -> Filter {
        self.filter.clone()
    }

    fn secret_token(&self) -> SecretToken {
        self.secret_token.clone()
    }

    fn auth_header_key() -> String {
        "token".to_string()
    }

    fn get_user_projects(
        &self,
        user: &str,
    ) -> Result<Vec<GithubProject>, ApiErrorResponse<GithubApiErrorResponse>> {
        self.call_list(
            &format!("{GITHUB_API_BASEURL}/users/{user}/repos"),
            Some(ACCEPT_HEADER_JSON),
        )
    }

    fn get_group_projects(
        &self,
        group: &str,
    ) -> Result<Vec<GithubProject>, ApiErrorResponse<GithubApiErrorResponse>> {
        self.call_list(
            &format!("{GITHUB_API_BASEURL}/orgs/{group}/repos?type=all"),
            Some(ACCEPT_HEADER_JSON),
        )
    }

    fn get_accessible_projects(
        &self,
    ) -> Result<Vec<GithubProject>, ApiErrorResponse<GithubApiErrorResponse>> {
        self.call_list(
            &format!("{GITHUB_API_BASEURL}/user/repos"),
            Some(ACCEPT_HEADER_JSON),
        )
    }

    fn get_current_user(&self) -> Result<String, ApiErrorResponse<GithubApiErrorResponse>> {
        Ok(super::call::<GithubUser, GithubApiErrorResponse>(
            &format!("{GITHUB_API_BASEURL}/user"),
            &Self::auth_header_key(),
            &self.secret_token(),
            Some(ACCEPT_HEADER_JSON),
        )?
        .username)
    }
}
