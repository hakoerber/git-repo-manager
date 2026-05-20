use serde::Deserialize;

use super::{
    ApiError, Error, JsonError, Project, ProjectName, ProjectNamespace, Provider, RemoteUrl, Url,
    auth,
};

const ACCEPT_HEADER_JSON: &str = "application/json";

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GiteaVisibility {
    Private,
    Internal,
    Public,
}

#[derive(Deserialize)]
pub struct ParentProject;

#[derive(Deserialize)]
pub struct GiteaProject {
    #[serde(rename = "path")]
    pub name: String,
    pub path_with_namespace: String,
    pub http_url_to_repo: String,
    pub ssh_url_to_repo: String,
    pub visibility: GiteaVisibility,
    pub fork: bool,
}

#[derive(Deserialize)]
struct GiteaUser {
    pub username: String,
}

impl Project for GiteaProject {
    fn name(&self) -> ProjectName {
        unimplemented!()
    }

    fn namespace(&self) -> Option<ProjectNamespace> {
        unimplemented!()
    }

    fn ssh_url(&self) -> RemoteUrl {
        unimplemented!()
    }

    fn http_url(&self) -> RemoteUrl {
        unimplemented!()
    }

    fn private(&self) -> bool {
        unimplemented!()
    }

    fn is_fork(&self) -> bool {
        unimplemented!()
    }
}

#[derive(Deserialize)]
pub struct GiteaApiErrorResponse {
    #[serde(alias = "error_description", alias = "error")]
    pub message: String,
}

impl JsonError for GiteaApiErrorResponse {
    fn to_string(self) -> String {
        self.message
    }
}

pub struct Gitea {
    secret_token: auth::AuthToken,
    api_url: Url,
}

impl Provider for Gitea {
    type Error = GiteaApiErrorResponse;
    type Project = GiteaProject;

    const AUTH_HEADER_KEY: &'static str = "foo";

    fn new(secret_token: auth::AuthToken, api_url_override: Option<Url>) -> Result<Self, Error> {
        Ok(Self {
            secret_token,
            api_url: api_url_override
                .ok_or(Error::Provider("gitea always need a URL".to_owned()))?,
        })
    }

    fn secret_token(&self) -> &auth::AuthToken {
        unimplemented!()
    }

    fn get_user_projects(
        &self,
        user: &super::User,
    ) -> Result<Vec<GiteaProject>, ApiError<GiteaApiErrorResponse>> {
        unimplemented!()
    }

    fn get_group_projects(
        &self,
        group: &super::Group,
    ) -> Result<Vec<GiteaProject>, ApiError<GiteaApiErrorResponse>> {
        unimplemented!()
    }

    fn get_accessible_projects(
        &self,
    ) -> Result<Vec<GiteaProject>, ApiError<GiteaApiErrorResponse>> {
        unimplemented!()
    }

    fn get_current_user(&self) -> Result<super::User, ApiError<GiteaApiErrorResponse>> {
        unimplemented!()
    }
}
