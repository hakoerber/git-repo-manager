use serde::{Deserialize, Serialize};
use std::path::Path;

use git2::{Cred, RemoteCallbacks, Repository};

use crate::output::*;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteType {
    Ssh,
    Https,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Remote {
    pub name: String,
    pub url: String,
    #[serde(rename = "type")]
    pub remote_type: RemoteType,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Repo {
    pub name: String,
    pub remotes: Option<Vec<Remote>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_ssh_remote() {
        assert_eq!(
            detect_remote_type("ssh://git@example.com"),
            Some(RemoteType::Ssh)
        );
        assert_eq!(detect_remote_type("git@example.git"), Some(RemoteType::Ssh));
    }

    #[test]
    fn check_https_remote() {
        assert_eq!(
            detect_remote_type("https://example.com"),
            Some(RemoteType::Https)
        );
        assert_eq!(
            detect_remote_type("https://example.com/test.git"),
            Some(RemoteType::Https)
        );
    }

    #[test]
    fn check_invalid_remotes() {
        assert_eq!(detect_remote_type("https//example.com"), None);
        assert_eq!(detect_remote_type("https:example.com"), None);
        assert_eq!(detect_remote_type("ssh//example.com"), None);
        assert_eq!(detect_remote_type("ssh:example.com"), None);
        assert_eq!(detect_remote_type("git@example.com"), None);
    }

    #[test]
    #[should_panic]
    fn check_unsupported_protocol_http() {
        detect_remote_type("http://example.com");
    }

    #[test]
    #[should_panic]
    fn check_unsupported_protocol_git() {
        detect_remote_type("git://example.com");
    }

    #[test]
    #[should_panic]
    fn check_unsupported_protocol_file() {
        detect_remote_type("file:///");
    }
}

pub fn detect_remote_type(remote_url: &str) -> Option<RemoteType> {
    let git_regex = regex::Regex::new(r"^[a-zA-Z]+@.*$").unwrap();
    if remote_url.starts_with("ssh://") {
        return Some(RemoteType::Ssh);
    }
    if git_regex.is_match(remote_url) && remote_url.ends_with(".git") {
        return Some(RemoteType::Ssh);
    }
    if remote_url.starts_with("https://") {
        return Some(RemoteType::Https);
    }
    if remote_url.starts_with("http://") {
        unimplemented!("Remotes using HTTP protocol are not supported");
    }
    if remote_url.starts_with("git://") {
        unimplemented!("Remotes using git protocol are not supported");
    }
    if remote_url.starts_with("file://") || remote_url.starts_with('/') {
        unimplemented!("Remotes using local protocol are not supported");
    }
    None
}

pub fn open_repo(path: &Path) -> Result<Repository, Box<dyn std::error::Error>> {
    match Repository::open(path) {
        Ok(r) => Ok(r),
        Err(e) => Err(Box::new(e)),
    }
}

pub fn init_repo(path: &Path) -> Result<Repository, Box<dyn std::error::Error>> {
    match Repository::init(path) {
        Ok(r) => Ok(r),
        Err(e) => Err(Box::new(e)),
    }
}

pub fn clone_repo(remote: &Remote, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    print_action(&format!(
        "Cloning into \"{}\" from \"{}\"",
        &path.display(),
        &remote.url
    ));
    match remote.remote_type {
        RemoteType::Https => match Repository::clone(&remote.url, &path) {
            Ok(_) => Ok(()),
            Err(e) => Err(Box::new(e)),
        },
        RemoteType::Ssh => {
            let mut callbacks = RemoteCallbacks::new();
            callbacks.credentials(|_url, username_from_url, _allowed_types| {
                Cred::ssh_key_from_agent(username_from_url.unwrap())
            });

            let mut fo = git2::FetchOptions::new();
            fo.remote_callbacks(callbacks);

            let mut builder = git2::build::RepoBuilder::new();
            builder.fetch_options(fo);

            match builder.clone(&remote.url, path) {
                Ok(_) => Ok(()),
                Err(e) => Err(Box::new(e)),
            }
        }
    }
}
