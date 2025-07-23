use serde::Deserialize;

use super::{ApiError, Error, Filter, JsonError, Project, Provider, auth, escape};

const ACCEPT_HEADER_JSON: &str = "application/vnd.github.v3+json";
const GITHUB_API_BASEURL: &str = match option_env!("GITHUB_API_BASEURL") {
    Some(url) => url,
    None => "https://api.github.com",
};

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
            Some(namespace.to_owned())
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
    secret_token: auth::AuthToken,
}

impl Provider for Github {
    type Error = GithubApiErrorResponse;
    type Project = GithubProject;

    fn new(
        filter: Filter,
        secret_token: auth::AuthToken,
        api_url_override: Option<String>,
    ) -> Result<Self, Error> {
        if api_url_override.is_some() {
            return Err(Error::Provider(
                "API URL overriding is not supported for Github".to_owned(),
            ));
        }
        Ok(Self {
            filter,
            secret_token,
        })
    }

    fn filter(&self) -> &Filter {
        &self.filter
    }

    fn secret_token(&self) -> &auth::AuthToken {
        &self.secret_token
    }

    fn auth_header_key() -> &'static str {
        "token"
    }

    fn get_user_projects(
        &self,
        user: &super::User,
    ) -> Result<Vec<GithubProject>, ApiError<GithubApiErrorResponse>> {
        self.call_list(
            &format!("{GITHUB_API_BASEURL}/users/{}/repos", escape(&user.0)),
            Some(ACCEPT_HEADER_JSON),
        )
    }

    fn get_group_projects(
        &self,
        group: &super::Group,
    ) -> Result<Vec<GithubProject>, ApiError<GithubApiErrorResponse>> {
        self.call_list(
            &format!(
                "{GITHUB_API_BASEURL}/orgs/{}/repos?type=all",
                escape(&group.0)
            ),
            Some(ACCEPT_HEADER_JSON),
        )
    }

    fn get_accessible_projects(
        &self,
    ) -> Result<Vec<GithubProject>, ApiError<GithubApiErrorResponse>> {
        self.call_list(
            &format!("{GITHUB_API_BASEURL}/user/repos"),
            Some(ACCEPT_HEADER_JSON),
        )
    }

    fn get_current_user(&self) -> Result<super::User, ApiError<GithubApiErrorResponse>> {
        Ok(super::User(
            super::call::<GithubUser, GithubApiErrorResponse>(
                &format!("{GITHUB_API_BASEURL}/user"),
                Self::auth_header_key(),
                self.secret_token(),
                Some(ACCEPT_HEADER_JSON),
            )?
            .username,
        ))
    }
}
