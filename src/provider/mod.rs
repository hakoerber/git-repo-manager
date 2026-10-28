pub mod github;
pub mod gitlab;

use std::{borrow::Cow, collections::HashMap, fmt, sync::OnceLock};

pub use github::Github;
pub use gitlab::Gitlab;
use thiserror::Error;
use ureq::tls::{RootCerts, TlsConfig};

use super::{RemoteName, RemoteUrl, Warning, auth, config, repo};

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::new_with_config(
            ureq::config::Config::builder()
                .tls_config(
                    TlsConfig::builder()
                        .root_certs(RootCerts::PlatformVerifier)
                        .build(),
                )
                .build(),
        )
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProtocolConfig {
    Default,
    ForceSsh,
}

impl ProtocolConfig {
    pub fn force_ssh(&self) -> bool {
        *self == Self::ForceSsh
    }
}

pub struct Url(Cow<'static, str>);

impl Url {
    pub fn new(from: String) -> Self {
        Self(Cow::Owned(from))
    }

    pub const fn new_static(from: &'static str) -> Self {
        Self(Cow::Borrowed(from))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
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

const DEFAULT_REMOTE_NAME: RemoteName = RemoteName::new_static("origin");

#[derive(Debug, Error)]
pub enum Error {
    #[error("Response error: {0}")]
    Response(String),
    #[error("Provider error: {0}")]
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

#[derive(PartialEq, Eq)]
pub struct ProjectName(String);

impl ProjectName {
    pub fn new(from: String) -> Self {
        Self(from)
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl From<repo::RepoName> for ProjectName {
    fn from(other: repo::RepoName) -> Self {
        Self(other.into_string())
    }
}

impl From<ProjectName> for repo::RepoName {
    fn from(other: ProjectName) -> Self {
        Self::new(other.into_string())
    }
}

#[derive(PartialEq, Eq, Hash)]
pub struct ProjectNamespace(String);

impl ProjectNamespace {
    pub fn new(from: String) -> Self {
        Self(from)
    }

    pub fn into_string(self) -> String {
        self.0
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<repo::RepoNamespace> for ProjectNamespace {
    fn from(other: repo::RepoNamespace) -> Self {
        Self(other.into_string())
    }
}

impl From<ProjectNamespace> for repo::RepoNamespace {
    fn from(other: ProjectNamespace) -> Self {
        Self::new(other.into_string())
    }
}

pub trait Project {
    fn into_repo_config(
        self,
        remote_name: &RemoteName,
        worktree_setup: repo::WorktreeSetup,
        protocol_config: ProtocolConfig,
    ) -> repo::Repo
    where
        Self: Sized,
    {
        repo::Repo {
            name: self.name().into(),
            namespace: self.namespace().map(Into::into),
            worktree_setup,
            remotes: vec![repo::Remote {
                name: remote_name.clone(),
                url: if protocol_config.force_ssh() || self.private() {
                    self.ssh_url()
                } else {
                    self.http_url()
                },
                remote_type: if protocol_config.force_ssh() || self.private() {
                    repo::RemoteType::Ssh
                } else {
                    repo::RemoteType::Https
                },
            }],
        }
    }

    fn name(&self) -> ProjectName;
    fn namespace(&self) -> Option<ProjectNamespace>;
    fn ssh_url(&self) -> RemoteUrl;
    fn http_url(&self) -> RemoteUrl;
    fn private(&self) -> bool;
    fn is_fork(&self) -> bool;
    fn is_archived(&self) -> bool {
        false
    }
}

/// A repository exclusion, as configured for messages and in lowercase for
/// matching. Forges treat repository paths case-insensitively.
#[derive(Clone)]
struct Exclusion {
    entry: String,
    pattern: String,
}

#[derive(Clone)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent repository filter options"
)]
pub struct Filter {
    users: Vec<User>,
    groups: Vec<Group>,
    owner: bool,
    access: bool,
    fork: bool,
    ignore_archived: bool,
    exclude_repos: Vec<Exclusion>,
}

/// Match the complete path, with `*` matching zero or more characters, including `/`.
fn matches_exclusion(pattern: &str, path: &str) -> bool {
    let mut parts = pattern.split('*');
    let Some(remaining) = path.strip_prefix(parts.next().unwrap_or_default()) else {
        return false;
    };
    let Some(suffix) = parts.next_back() else {
        return remaining.is_empty();
    };
    let Some(mut remaining) = remaining.strip_suffix(suffix) else {
        return false;
    };
    // Reserve the suffix before finding the middle parts, so they cannot overlap.
    for part in parts {
        let Some((_, rest)) = remaining.split_once(part) else {
            return false;
        };
        remaining = rest;
    }
    true
}

/// The full repository path, e.g. `owner/repo` or `group/subgroup/repo`.
fn project_path(project: &impl Project) -> String {
    let name = project.name().into_string();
    match project.namespace() {
        Some(namespace) => format!("{}/{name}", namespace.as_str()),
        None => name,
    }
}

impl Filter {
    pub fn new(
        users: Vec<User>,
        groups: Vec<Group>,
        owner: bool,
        access: bool,
        fork: bool,
    ) -> Self {
        Self {
            users,
            groups,
            owner,
            access,
            fork,
            ignore_archived: false,
            exclude_repos: Vec::new(),
        }
    }

    /// Skip archived projects, and projects whose full path matches one of the
    /// `exclude_repos` patterns. Matching ignores ASCII case. A `*` matches
    /// zero or more characters, including `/`.
    #[must_use]
    pub fn with_exclusions(mut self, ignore_archived: bool, exclude_repos: Vec<String>) -> Self {
        self.ignore_archived = ignore_archived;
        self.exclude_repos = exclude_repos
            .into_iter()
            .map(|entry| Exclusion {
                pattern: entry.to_ascii_lowercase(),
                entry,
            })
            .collect();
        self
    }

    /// Keep the projects that pass the fork, archive, and exclusion filters.
    /// Also return the exclusion entries that match no project. An entry that
    /// matches a project skipped for another reason still counts as matched.
    fn filter_projects<P: Project>(&self, projects: Vec<P>) -> (Vec<P>, Vec<&str>) {
        let mut matched = vec![false; self.exclude_repos.len()];
        let mut kept = Vec::new();
        for project in projects {
            let mut excluded = false;
            if !self.exclude_repos.is_empty() {
                let path = project_path(&project).to_ascii_lowercase();
                for (exclusion, is_matched) in self.exclude_repos.iter().zip(matched.iter_mut()) {
                    if matches_exclusion(&exclusion.pattern, &path) {
                        *is_matched = true;
                        excluded = true;
                    }
                }
            }
            let skipped = (!self.fork && project.is_fork())
                || (self.ignore_archived && project.is_archived());
            if !excluded && !skipped {
                kept.push(project);
            }
        }
        let unmatched = self
            .exclude_repos
            .iter()
            .zip(matched)
            .filter_map(|(exclusion, is_matched)| (!is_matched).then_some(exclusion.entry.as_str()))
            .collect();
        (kept, unmatched)
    }

    pub fn empty(&self) -> bool {
        self.users.is_empty() && self.groups.is_empty() && !self.owner && !self.access
    }
}

impl From<config::ConfigProviderFilter> for Filter {
    fn from(filters: config::ConfigProviderFilter) -> Self {
        Self::new(
            filters
                .users
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
            filters
                .groups
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
            filters.owner.unwrap_or(false),
            filters.access.unwrap_or(false),
            filters.fork.unwrap_or(true),
        )
        .with_exclusions(
            filters.ignore_archived.unwrap_or(false),
            filters.exclude_repos.unwrap_or_default(),
        )
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

/// Repositories found on a provider, grouped by namespace.
pub struct ProviderRepos {
    pub repos: HashMap<Option<ProjectNamespace>, Vec<repo::Repo>>,
    pub warnings: Vec<Warning>,
}

pub trait Provider {
    type Project: serde::de::DeserializeOwned + Project;
    type Error: serde::de::DeserializeOwned + JsonError;

    fn new(
        filter: Filter,
        secret_token: auth::AuthToken,
        api_url_override: Option<Url>,
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
        uri: &Url,
        accept_header: Option<&str>,
    ) -> Result<Vec<Self::Project>, ApiError<Self::Error>> {
        match agent()
            .get(uri.as_str())
            .config()
            .http_status_as_error(false)
            .build()
            .header("accept", accept_header.unwrap_or("application/json"))
            .header(
                "authorization",
                &format!(
                    "{} {}",
                    Self::auth_header_key(),
                    self.secret_token().access()
                ),
            )
            .call()
        {
            Err(ureq::Error::Http(error)) => Err(format!("http error: {error}").into()),
            Err(e) => Err(format!("unknown error: {e}").into()),
            Ok(mut response) => {
                if response.status().is_success() {
                    let mut projects = vec![];

                    if let Some(link_header) = response.headers().get("link") {
                        let link_header = parse_link_header::parse(link_header.to_str()?)
                            .map_err(|error| error.to_string())?;

                        let next_page = link_header.get(&Some(String::from("next")));

                        if let Some(page) = next_page {
                            let following_repos =
                                self.call_list(&Url::new(page.raw_uri.clone()), accept_header)?;
                            projects.extend(following_repos);
                        }
                    }

                    let result: Vec<Self::Project> = response
                        .body_mut()
                        .read_json()
                        .map_err(|error| format!("Failed deserializing response: {error}"))?;

                    projects.extend(result);
                    Ok(projects)
                } else {
                    Err(ApiError::Json(response.body_mut().read_json().map_err(
                        |error| format!("Failed deserializing error response: {error}"),
                    )?))
                }
            }
        }
    }

    fn get_repos(
        &self,
        worktree_setup: repo::WorktreeSetup,
        protocol_config: ProtocolConfig,
        remote_name: Option<RemoteName>,
    ) -> Result<ProviderRepos, Error> {
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

        let (repos, unmatched_exclusions) = self.filter().filter_projects(repos);

        let mut ret: HashMap<Option<ProjectNamespace>, Vec<repo::Repo>> = HashMap::new();

        let remote_name = remote_name.unwrap_or(DEFAULT_REMOTE_NAME);

        for repo in repos {
            let namespace = repo.namespace();

            let mut repo = repo.into_repo_config(&remote_name, worktree_setup, protocol_config);

            // Namespace is already part of the hashmap key. I'm not too happy
            // about the data exchange format here.
            repo.remove_namespace();

            ret.entry(namespace).or_default().push(repo);
        }

        Ok(ProviderRepos {
            repos: ret,
            warnings: unmatched_exclusions
                .into_iter()
                .map(|entry| {
                    Warning(format!(
                        "Exclusion \"{entry}\" did not match any repository"
                    ))
                })
                .collect(),
        })
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
    match agent()
        .get(uri)
        .header("accept", accept_header.unwrap_or("application/json"))
        .header(
            "authorization",
            &format!("{} {}", auth_header_key, secret_token.access()),
        )
        .call()
    {
        Err(ureq::Error::Http(error)) => Err(format!("http error: {error}").into()),
        Err(e) => Err(format!("unknown error: {e}").into()),
        Ok(mut response) => {
            if response.status().is_success() {
                Ok(response
                    .body_mut()
                    .read_json()
                    .map_err(|error| format!("Failed deserializing response: {error}"))?)
            } else {
                Err(ApiError::Json(response.body_mut().read_json().map_err(
                    |error| format!("Failed deserializing error response: {error}"),
                )?))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture<P: serde::de::DeserializeOwned>(
        mut data: serde_json::Value,
        archived: Option<serde_json::Value>,
    ) -> P {
        if let (Some(object), Some(archived)) = (data.as_object_mut(), archived) {
            object.insert("archived".to_owned(), archived);
        }
        serde_json::from_value(data).expect("valid project fixture")
    }

    fn github_fixture(
        path: &str,
        archived: Option<serde_json::Value>,
        fork: bool,
    ) -> github::GithubProject {
        fixture(
            serde_json::json!({
                "name": path.rsplit('/').next().expect("path is not empty"),
                "full_name": path,
                "clone_url": "https://example.com/repo.git",
                "ssh_url": "git@example.com:repo.git",
                "private": false,
                "fork": fork,
            }),
            archived,
        )
    }

    fn gitlab_fixture(path: &str, archived: Option<serde_json::Value>) -> gitlab::GitlabProject {
        fixture(
            serde_json::json!({
                "path": path.rsplit('/').next().expect("path is not empty"),
                "path_with_namespace": path,
                "http_url_to_repo": "https://example.com/repo.git",
                "ssh_url_to_repo": "git@example.com:repo.git",
                "visibility": "private",
            }),
            archived,
        )
    }

    fn github_project(path: &str, archived: bool, fork: bool) -> github::GithubProject {
        github_fixture(path, Some(serde_json::Value::Bool(archived)), fork)
    }

    fn config_filter(filters: &str) -> Filter {
        let config: config::ConfigProviderFilter =
            toml::from_str(filters).expect("valid filter fixture");
        config.into()
    }

    fn paths<P: Project>(projects: &[P]) -> Vec<String> {
        projects.iter().map(project_path).collect()
    }

    fn includes(filter: &Filter, path: &str) -> bool {
        !filter
            .filter_projects(vec![github_project(path, false, false)])
            .0
            .is_empty()
    }

    #[test]
    fn old_config_keeps_archived_repositories_and_forks() {
        let filter = config_filter("access = true");
        assert!(!filter.empty(), "access selects repositories");
        let (kept, unmatched) = filter.filter_projects(vec![
            github_project("org/plain", false, false),
            github_project("org/archived", true, false),
            github_project("org/fork", false, true),
            github_project("org/archived-fork", true, true),
        ]);
        assert_eq!(
            paths(&kept),
            ["org/plain", "org/archived", "org/fork", "org/archived-fork"]
        );
        assert!(unmatched.is_empty(), "no exclusions, no warnings");
    }

    #[test]
    fn archive_filter_is_opt_in() {
        for (ignore_archived, expected) in [
            (false, vec!["org/plain", "org/archived"]),
            (true, vec!["org/plain"]),
        ] {
            let filter = config_filter(&format!(
                "access = true\nignore_archived = {ignore_archived}"
            ));
            let (kept, _) = filter.filter_projects(vec![
                github_project("org/plain", false, false),
                github_project("org/archived", true, false),
            ]);
            assert_eq!(paths(&kept), expected);
        }
    }

    #[test]
    fn exact_entries_match_complete_paths_ignoring_case() {
        let filter = config_filter("access = true\nexclude_repos = ['org/repo', 'Group/Sub/Repo']");
        for path in ["org/repo", "ORG/Repo", "group/sub/repo", "GROUP/SUB/REPO"] {
            assert!(!includes(&filter, path), "{path} is excluded");
        }
        for path in ["other/repo", "org/repo-more", "group/repo", "org"] {
            assert!(includes(&filter, path), "{path} is kept");
        }
        for entry in ["repo", "^org/repo$", "https://example.com/org/repo.git"] {
            let filter = config_filter(&format!("access = true\nexclude_repos = ['{entry}']"));
            assert!(
                includes(&filter, "org/repo"),
                "{entry} is not a bare name, regex, or URL match"
            );
        }
    }

    #[test]
    fn wildcard_patterns_match_complete_paths() {
        for (pattern, path, expected) in [
            ("archived-org/*", "archived-org/repo", true),
            ("archived-org/*", "archived-org/sub/repo", true),
            ("archived-org/*", "other/archived-org/repo", false),
            ("archived-org/*", "archived-org-extra/repo", false),
            ("team/legacy-*", "team/legacy-service", true),
            ("team/legacy-*", "team/new-legacy-service", false),
            ("*/repo", "team/repo", true),
            ("*/repo", "team/sub/repo", true),
            ("*/repo", "team/repo-extra", false),
            ("team/*-config", "team/repo-config", true),
            ("team/*-config", "team/repo-config-old", false),
            ("te*/sub/*-config", "team/sub/repo-config", true),
            ("te*/sub/*-config", "team/other/repo-config", false),
            ("org/re*po", "org/repo", true),
            ("org/re*po", "org/repository", false),
            ("org/repo*repo", "org/repo", false),
            ("org/*ab*ab", "org/ab", false),
            ("org/*ab*ab", "org/abab", true),
            ("org/**repo**", "org/repo", true),
            ("org/**repo**", "org/sub/repo-extra", true),
            ("*", "org/repo", true),
            ("**", "org/repo", true),
            ("*", "", true),
            ("", "", true),
            ("", "org/repo", false),
            ("org/repo", "org/repo", true),
            ("org/repo", "org/repo-extra", false),
            ("org/*.config", "org/repo.config", true),
            ("org/*.config", "org/repo-config", false),
            ("org/*?", "org/repo?", true),
            ("org/*?", "org/repo", false),
            ("org/*[ab]", "org/repo[ab]", true),
            ("org/*[ab]", "org/repoa", false),
            ("org/*{a,b}", "org/repo{a,b}", true),
            ("org/*{a,b}", "org/repoa", false),
            ("org/ø*é", "org/øvelse-é", true),
            ("org/ø*é", "org/øvelse-e", false),
        ] {
            assert_eq!(
                matches_exclusion(pattern, path),
                expected,
                "{pattern}: {path}"
            );
        }
    }

    #[test]
    fn wildcard_matching_agrees_with_anchored_regex() {
        fn words(alphabet: &[char], max_len: usize) -> Vec<String> {
            let mut words = vec![String::new()];
            let mut level = vec![String::new()];
            for _ in 0..max_len {
                level = level
                    .iter()
                    .flat_map(|prefix| {
                        alphabet
                            .iter()
                            .map(move |letter| format!("{prefix}{letter}"))
                    })
                    .collect();
                words.extend(level.iter().cloned());
            }
            words
        }

        let paths = words(&['a', 'b', '/'], 5);
        for pattern in words(&['a', 'b', '/', '*'], 4) {
            let expression = regex::escape(&pattern).replace(r"\*", ".*");
            let reference = regex::Regex::new(&format!(r"\A{expression}\z"))
                .expect("escaped wildcard pattern is valid regex");
            for path in &paths {
                assert_eq!(
                    matches_exclusion(&pattern, path),
                    reference.is_match(path),
                    "pattern {pattern}, path {path}",
                );
            }
        }
    }

    #[test]
    fn wildcard_entries_ignore_case() {
        let filter =
            config_filter("access = true\nexclude_repos = ['ARCHIVED-ORG/*', 'team/Legacy-*']");
        for path in [
            "archived-org/repo",
            "Archived-Org/Sub/Repo",
            "TEAM/legacy-service",
        ] {
            assert!(!includes(&filter, path), "{path} is excluded");
        }
        for path in ["archived-org-extra/repo", "team/new-legacy"] {
            assert!(includes(&filter, path), "{path} is kept");
        }
    }

    #[test]
    fn wildcard_and_exact_entries_work_together() {
        let filter = config_filter(
            "access = true\nexclude_repos = ['archived-org/*', 'other/one-repo', 'team/legacy-*']",
        );
        for path in [
            "archived-org/repo",
            "archived-org/sub/repo",
            "other/one-repo",
            "team/legacy-service",
        ] {
            assert!(!includes(&filter, path), "{path} is excluded");
        }
        for path in [
            "other/another-repo",
            "team/new-service",
            "archived-org-extra/repo",
        ] {
            assert!(includes(&filter, path), "{path} is kept");
        }
    }

    #[test]
    fn exclusions_do_not_select_repositories() {
        assert!(
            config_filter("ignore_archived = true\nexclude_repos = ['org/repo', '*']").empty(),
            "exclusions alone must not select repositories"
        );
        assert!(
            Filter::from(config::ConfigProviderFilter::default()).empty(),
            "missing filters must not select repositories"
        );
        let filter = config_filter("access = true\nexclude_repos = ['*']");
        assert!(!filter.empty(), "access still selects repositories");
        assert!(
            !includes(&filter, "org/repo"),
            "a single star excludes everything"
        );
    }

    #[test]
    fn setting_exclusions_replaces_previous_entries() {
        let filter = config_filter("access = true\nexclude_repos = ['old/*', 'other/repo']")
            .with_exclusions(false, vec!["new/*".to_owned()]);
        assert!(includes(&filter, "old/repo"), "old wildcard was replaced");
        assert!(
            includes(&filter, "other/repo"),
            "old exact entry was replaced"
        );
        assert!(!includes(&filter, "new/repo"), "new wildcard is active");
    }

    #[test]
    fn exclusion_filters_combine_with_fork_filter() {
        let filter = config_filter(
            "access = true\nfork = false\nignore_archived = true\nexclude_repos = ['org/sk*']",
        );
        let mut projects = Vec::new();
        for path in ["org/keep", "org/skip"] {
            for archived in [false, true] {
                for fork in [false, true] {
                    projects.push(github_project(path, archived, fork));
                }
            }
        }
        let (kept, unmatched) = filter.filter_projects(projects);
        assert_eq!(paths(&kept), ["org/keep"]);
        assert!(unmatched.is_empty(), "the exclusion matched");
    }

    #[test]
    fn unmatched_exclusions_are_reported_in_order() {
        let filter = config_filter(
            "access = true\nignore_archived = true\n\
             exclude_repos = ['nosuch/*', 'org/one', 'ORG/ARCHIVED', 'org/typo']",
        );
        let (kept, unmatched) = filter.filter_projects(vec![
            github_project("org/one", false, false),
            github_project("org/two", false, false),
            github_project("org/archived", true, false),
        ]);
        assert_eq!(paths(&kept), ["org/two"]);
        // `ORG/ARCHIVED` matches a project that the archive filter skips anyway.
        assert_eq!(unmatched, ["nosuch/*", "org/typo"]);
    }

    #[test]
    fn config_conversion_preserves_all_inclusion_filters() {
        let filter = config_filter(
            "access = true\nowner = true\nusers = ['alice']\ngroups = ['team/subgroup']",
        );
        assert!(filter.owner, "owner selection is preserved");
        assert!(filter.access, "access selection is preserved");
        assert_eq!(filter.users.len(), 1);
        assert_eq!(filter.users.first().expect("one user").to_string(), "alice");
        assert_eq!(filter.groups.len(), 1);
        assert_eq!(
            filter.groups.first().expect("one group").to_string(),
            "team/subgroup"
        );
        for selection in ["owner = true", "users = ['alice']", "groups = ['team']"] {
            assert!(!config_filter(selection).empty(), "selection is not empty");
        }
    }

    #[test]
    fn archive_metadata_may_be_missing_or_null() {
        for (archived, expected) in [
            (Some(serde_json::Value::Bool(true)), true),
            (Some(serde_json::Value::Bool(false)), false),
            (Some(serde_json::Value::Null), false),
            (None, false),
        ] {
            assert_eq!(
                gitlab_fixture("group/sub/repo", archived.clone()).is_archived(),
                expected
            );
            assert_eq!(
                github_fixture("org/repo", archived, false).is_archived(),
                expected
            );
        }
    }

    #[test]
    fn gitlab_nested_paths_match_exclusions() {
        let filter = config_filter("access = true\nexclude_repos = ['GROUP/SUB/*']");
        let (kept, _) = filter.filter_projects(vec![
            gitlab_fixture("group/sub/repo", None),
            gitlab_fixture("group/sub/deeper/repo", None),
            gitlab_fixture("group/other", None),
        ]);
        assert_eq!(paths(&kept), ["group/other"]);
    }

    #[test]
    fn toml_and_yaml_configs_accept_new_filters() {
        let toml_config: config::ConfigProvider = toml::from_str(
            "provider = 'github'\ntoken_command = 'echo test'\nroot = '.'\n\
             [filters]\naccess = true\nignore_archived = true\nexclude_repos = ['*/repo', 'org/one']",
        )
        .expect("valid TOML provider config");
        let yaml_config: config::ConfigProvider = serde_yaml::from_str(
            "provider: github\ntoken_command: echo test\nroot: .\nfilters:\n  access: true\n  \
             ignore_archived: true\n  exclude_repos:\n    - \"*/repo\"\n    - org/one\n",
        )
        .expect("valid YAML provider config");
        for provider_config in [toml_config, yaml_config] {
            let filter = Filter::from(provider_config.filters.expect("filters are present"));
            let (kept, unmatched) = filter.filter_projects(vec![
                github_project("org/one", false, false),
                github_project("org/two", false, false),
                github_project("team/repo", false, false),
                github_project("org/old", true, false),
            ]);
            assert_eq!(paths(&kept), ["org/two"]);
            assert!(unmatched.is_empty(), "both entries match");
        }
        serde_yaml::from_str::<config::ConfigProvider>(
            "provider: github\ntoken_command: echo test\nroot: .\nfilters:\n  exclude_repos:\n    - */repo\n",
        )
        .expect_err("unquoted YAML entries starting with * are aliases");
    }

    #[test]
    fn invalid_filter_values_are_rejected() {
        for filters in [
            "ignore_archived = 'true'",
            "exclude_repos = 'org/repo'",
            "exclude_repos = [42]",
            "ignore_archive = true",
            "exclude = ['org/repo']",
        ] {
            toml::from_str::<config::ConfigProviderFilter>(filters)
                .expect_err("invalid filter configuration must be rejected");
        }
    }
}
