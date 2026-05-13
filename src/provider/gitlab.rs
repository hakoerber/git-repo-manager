use serde::Deserialize;

use super::{
    ApiError, Error, Filter, JsonError, Project, ProjectName, ProjectNamespace, Provider,
    RemoteUrl, Url, auth, escape,
};

const ACCEPT_HEADER_JSON: &str = "application/json";
const GITLAB_API_BASEURL: Url = Url::new_static(match option_env!("GITLAB_API_BASEURL") {
    Some(url) => url,
    None => "https://gitlab.com",
});

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
    fn name(&self) -> ProjectName {
        ProjectName::new(self.name.clone())
    }

    fn namespace(&self) -> Option<ProjectNamespace> {
        if let Some((namespace, _name)) = self.path_with_namespace.rsplit_once('/') {
            Some(ProjectNamespace::new(namespace.to_owned()))
        } else {
            None
        }
    }

    fn ssh_url(&self) -> RemoteUrl {
        RemoteUrl::new(self.ssh_url_to_repo.clone())
    }

    fn http_url(&self) -> RemoteUrl {
        RemoteUrl::new(self.http_url_to_repo.clone())
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
    api_url_override: Option<Url>,
}

impl Gitlab {
    fn api_url(&self) -> Url {
        Url::new(
            self.api_url_override
                .as_ref()
                .unwrap_or(&GITLAB_API_BASEURL)
                .as_str()
                .trim_end_matches('/')
                .to_owned(),
        )
    }
}

impl Provider for Gitlab {
    type Error = GitlabApiErrorResponse;
    type Project = GitlabProject;

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
        "bearer"
    }

    fn get_user_projects(
        &self,
        user: &super::User,
    ) -> Result<Vec<GitlabProject>, ApiError<GitlabApiErrorResponse>> {
        self.call_list(
            &Url::new(format!(
                "{}/api/v4/users/{}/projects",
                self.api_url().as_str(),
                escape(&user.0)
            )),
            Some(ACCEPT_HEADER_JSON),
        )
    }

    fn get_group_projects(
        &self,
        group: &super::Group,
    ) -> Result<Vec<GitlabProject>, ApiError<GitlabApiErrorResponse>> {
        self.call_list(
            &Url::new(format!(
                "{}/api/v4/groups/{}/projects?include_subgroups=true&archived=false",
                self.api_url().as_str(),
                escape(&group.0),
            )),
            Some(ACCEPT_HEADER_JSON),
        )
    }

    fn get_accessible_projects(
        &self,
    ) -> Result<Vec<GitlabProject>, ApiError<GitlabApiErrorResponse>> {
        self.call_list(
            &Url::new(format!("{}/api/v4/projects", self.api_url().as_str())),
            Some(ACCEPT_HEADER_JSON),
        )
    }

    fn get_current_user(&self) -> Result<super::User, ApiError<GitlabApiErrorResponse>> {
        Ok(super::User(
            super::call::<GitlabUser, GitlabApiErrorResponse>(
                &format!("{}/api/v4/user", self.api_url().as_str()),
                Self::auth_header_key(),
                self.secret_token(),
                Some(ACCEPT_HEADER_JSON),
            )?
            .username,
        ))
    }
}
