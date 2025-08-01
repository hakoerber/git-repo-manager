use serde::Deserialize;

use super::{
    ApiError, Error, Filter, JsonError, Project, ProjectName, ProjectNamespace, Provider,
    RemoteUrl, Url, auth, escape,
};

const ACCEPT_HEADER_JSON: &str = "application/vnd.github.v3+json";
const GITHUB_API_BASEURL: Url = Url::new_static(match option_env!("GITHUB_API_BASEURL") {
    Some(url) => url,
    None => "https://api.github.com",
});

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
    fn name(&self) -> ProjectName {
        ProjectName::new(self.name.clone())
    }

    fn namespace(&self) -> Option<ProjectNamespace> {
        if let Some((namespace, _name)) = self.full_name.rsplit_once('/') {
            Some(ProjectNamespace(namespace.to_owned()))
        } else {
            None
        }
    }

    fn ssh_url(&self) -> RemoteUrl {
        RemoteUrl::new(self.ssh_url.clone())
    }

    fn http_url(&self) -> RemoteUrl {
        RemoteUrl::new(self.clone_url.clone())
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
    api_url_override: Option<Url>,
}

impl Github {
    fn api_url(&self) -> Url {
        Url::new(
            self.api_url_override
                .as_ref()
                .map(Url::as_str)
                .unwrap_or(GITHUB_API_BASEURL.as_str())
                .trim_end_matches('/')
                .to_owned(),
        )
    }
}

impl Provider for Github {
    type Error = GithubApiErrorResponse;
    type Project = GithubProject;

    fn new(
        filter: Filter,
        secret_token: auth::AuthToken,
        api_url_override: Option<Url>,
    ) -> Result<Self, Error> {
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
        "token"
    }

    fn get_user_projects(
        &self,
        user: &super::User,
    ) -> Result<Vec<GithubProject>, ApiError<GithubApiErrorResponse>> {
        self.call_list(
            &Url::new(format!(
                "{}/users/{}/repos",
                self.api_url().as_str(),
                escape(&user.0)
            )),
            Some(ACCEPT_HEADER_JSON),
        )
    }

    fn get_group_projects(
        &self,
        group: &super::Group,
    ) -> Result<Vec<GithubProject>, ApiError<GithubApiErrorResponse>> {
        self.call_list(
            &Url::new(format!(
                "{}/orgs/{}/repos?type=all",
                self.api_url().as_str(),
                escape(&group.0)
            )),
            Some(ACCEPT_HEADER_JSON),
        )
    }

    fn get_accessible_projects(
        &self,
    ) -> Result<Vec<GithubProject>, ApiError<GithubApiErrorResponse>> {
        self.call_list(
            &Url::new(format!("{}/user/repos", self.api_url().as_str())),
            Some(ACCEPT_HEADER_JSON),
        )
    }

    fn get_current_user(&self) -> Result<super::User, ApiError<GithubApiErrorResponse>> {
        Ok(super::User(
            super::call::<GithubUser, GithubApiErrorResponse>(
                &format!("{}/user", self.api_url().as_str()),
                Self::auth_header_key(),
                self.secret_token(),
                Some(ACCEPT_HEADER_JSON),
            )?
            .username,
        ))
    }
}
