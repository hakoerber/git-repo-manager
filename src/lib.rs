#![feature(io_error_more)]
#![feature(const_option_ext)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process;

pub mod config;
pub mod output;
pub mod provider;
pub mod repo;
pub mod table;

use config::Config;
use output::*;

use repo::{clone_repo, detect_remote_type, Remote, RemoteType};

pub use repo::{
    RemoteTrackingStatus, Repo, RepoErrorKind, RepoHandle, WorktreeRemoveFailureReason,
};

const GIT_MAIN_WORKTREE_DIRECTORY: &str = ".git-main-working-tree";
const BRANCH_NAMESPACE_SEPARATOR: &str = "/";

const GIT_CONFIG_BARE_KEY: &str = "core.bare";
const GIT_CONFIG_PUSH_DEFAULT: &str = "push.default";

pub struct Tree {
    root: String,
    repos: Vec<Repo>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() {
        std::env::set_var("HOME", "/home/test");
    }

    #[test]
    fn check_expand_tilde() {
        setup();
        assert_eq!(
            expand_path(Path::new("~/file")),
            Path::new("/home/test/file")
        );
    }

    #[test]
    fn check_expand_invalid_tilde() {
        setup();
        assert_eq!(
            expand_path(Path::new("/home/~/file")),
            Path::new("/home/~/file")
        );
    }

    #[test]
    fn check_expand_home() {
        setup();
        assert_eq!(
            expand_path(Path::new("$HOME/file")),
            Path::new("/home/test/file")
        );
        assert_eq!(
            expand_path(Path::new("${HOME}/file")),
            Path::new("/home/test/file")
        );
    }
}

pub fn path_as_string(path: &Path) -> String {
    path.to_path_buf().into_os_string().into_string().unwrap()
}

pub fn env_home() -> PathBuf {
    match std::env::var("HOME") {
        Ok(path) => Path::new(&path).to_path_buf(),
        Err(e) => {
            print_error(&format!("Unable to read HOME: {}", e));
            process::exit(1);
        }
    }
}

fn expand_path(path: &Path) -> PathBuf {
    fn home_dir() -> Option<PathBuf> {
        Some(env_home())
    }

    let expanded_path = match shellexpand::full_with_context(
        &path_as_string(path),
        home_dir,
        |name| -> Result<Option<String>, &'static str> {
            match name {
                "HOME" => Ok(Some(path_as_string(home_dir().unwrap().as_path()))),
                _ => Ok(None),
            }
        },
    ) {
        Ok(std::borrow::Cow::Borrowed(path)) => path.to_owned(),
        Ok(std::borrow::Cow::Owned(path)) => path,
        Err(e) => {
            print_error(&format!("Unable to expand root: {}", e));
            process::exit(1);
        }
    };

    Path::new(&expanded_path).to_path_buf()
}

pub fn get_token_from_command(command: &str) -> Result<String, String> {
    let output = std::process::Command::new("/usr/bin/env")
        .arg("sh")
        .arg("-c")
        .arg(command)
        .output()
        .map_err(|error| format!("Failed to run token-command: {}", error))?;

    let stderr = String::from_utf8(output.stderr).map_err(|error| error.to_string())?;
    let stdout = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;

    if !output.status.success() {
        if !stderr.is_empty() {
            return Err(format!("Token command failed: {}", stderr));
        } else {
            return Err(String::from("Token command failed."));
        }
    }

    if !stderr.is_empty() {
        return Err(format!("Token command produced stderr: {}", stderr));
    }

    if stdout.is_empty() {
        return Err(String::from("Token command did not produce output"));
    }

    let token = stdout
        .split('\n')
        .next()
        .ok_or_else(|| String::from("Output did not contain any newline"))?;

    Ok(token.to_string())
}

fn sync_repo(root_path: &Path, repo: &Repo, init_worktree: bool) -> Result<(), String> {
    let repo_path = root_path.join(&repo.fullname());
    let actual_git_directory = get_actual_git_directory(&repo_path, repo.worktree_setup);

    let mut newly_created = false;

    if repo_path.exists() {
        if repo.worktree_setup && !actual_git_directory.exists() {
            return Err(String::from(
                "Repo already exists, but is not using a worktree setup",
            ));
        };
    } else if matches!(&repo.remotes, None) || repo.remotes.as_ref().unwrap().is_empty() {
        print_repo_action(
            &repo.name,
            "Repository does not have remotes configured, initializing new",
        );
        match RepoHandle::init(&repo_path, repo.worktree_setup) {
            Ok(r) => {
                print_repo_success(&repo.name, "Repository created");
                Some(r)
            }
            Err(e) => {
                return Err(format!("Repository failed during init: {}", e));
            }
        };
    } else {
        let first = repo.remotes.as_ref().unwrap().first().unwrap();

        match clone_repo(first, &repo_path, repo.worktree_setup) {
            Ok(_) => {
                print_repo_success(&repo.name, "Repository successfully cloned");
            }
            Err(e) => {
                return Err(format!("Repository failed during clone: {}", e));
            }
        };

        newly_created = true;
    }

    let repo_handle = match RepoHandle::open(&repo_path, repo.worktree_setup) {
        Ok(repo) => repo,
        Err(error) => {
            if !repo.worktree_setup && RepoHandle::open(&repo_path, true).is_ok() {
                return Err(String::from(
                    "Repo already exists, but is using a worktree setup",
                ));
            } else {
                return Err(format!("Opening repository failed: {}", error));
            }
        }
    };

    if newly_created && repo.worktree_setup && init_worktree {
        match repo_handle.default_branch() {
            Ok(branch) => {
                add_worktree(&repo_path, &branch.name()?, None, None, false)?;
            }
            Err(_error) => print_repo_error(
                &repo.name,
                "Could not determine default branch, skipping worktree initializtion",
            ),
        }
    }
    if let Some(remotes) = &repo.remotes {
        let current_remotes: Vec<String> = repo_handle
            .remotes()
            .map_err(|error| format!("Repository failed during getting the remotes: {}", error))?;

        for remote in remotes {
            let current_remote = repo_handle.find_remote(&remote.name)?;

            match current_remote {
                Some(current_remote) => {
                    let current_url = current_remote.url();

                    if remote.url != current_url {
                        print_repo_action(
                            &repo.name,
                            &format!("Updating remote {} to \"{}\"", &remote.name, &remote.url),
                        );
                        if let Err(e) = repo_handle.remote_set_url(&remote.name, &remote.url) {
                            return Err(format!("Repository failed during setting of the remote URL for remote \"{}\": {}", &remote.name, e));
                        };
                    }
                }
                None => {
                    print_repo_action(
                        &repo.name,
                        &format!(
                            "Setting up new remote \"{}\" to \"{}\"",
                            &remote.name, &remote.url
                        ),
                    );
                    if let Err(e) = repo_handle.new_remote(&remote.name, &remote.url) {
                        return Err(format!(
                            "Repository failed during setting the remotes: {}",
                            e
                        ));
                    }
                }
            }
        }

        for current_remote in &current_remotes {
            if !remotes.iter().any(|r| &r.name == current_remote) {
                print_repo_action(
                    &repo.name,
                    &format!("Deleting remote \"{}\"", &current_remote,),
                );
                if let Err(e) = repo_handle.remote_delete(current_remote) {
                    return Err(format!(
                        "Repository failed during deleting remote \"{}\": {}",
                        &current_remote, e
                    ));
                }
            }
        }
    }
    Ok(())
}

pub fn find_unmanaged_repos(
    root_path: &Path,
    managed_repos: &[Repo],
) -> Result<Vec<String>, String> {
    let mut unmanaged_repos = Vec::new();

    for repo in find_repo_paths(root_path)? {
        let name = path_as_string(repo.strip_prefix(&root_path).unwrap());
        if !managed_repos.iter().any(|r| r.name == name) {
            unmanaged_repos.push(name);
        }
    }
    Ok(unmanaged_repos)
}

pub fn sync_trees(config: Config, init_worktree: bool) -> Result<bool, String> {
    let mut failures = false;

    let trees = config.trees()?;

    for tree in trees {
        let repos: Vec<Repo> = tree
            .repos
            .unwrap_or_default()
            .into_iter()
            .map(|repo| repo.into_repo())
            .collect();

        let root_path = expand_path(Path::new(&tree.root));

        for repo in &repos {
            match sync_repo(&root_path, repo, init_worktree) {
                Ok(_) => print_repo_success(&repo.name, "OK"),
                Err(error) => {
                    print_repo_error(&repo.name, &error);
                    failures = true;
                }
            }
        }

        match find_unmanaged_repos(&root_path, &repos) {
            Ok(unmanaged_repos) => {
                for name in unmanaged_repos {
                    print_warning(&format!("Found unmanaged repository: {}", name));
                }
            }
            Err(error) => {
                print_error(&format!("Error getting unmanaged repos: {}", error));
                failures = true;
            }
        }
    }

    Ok(!failures)
}

/// Finds repositories recursively, returning their path
fn find_repo_paths(path: &Path) -> Result<Vec<PathBuf>, String> {
    let mut repos = Vec::new();

    let git_dir = path.join(".git");
    let git_worktree = path.join(GIT_MAIN_WORKTREE_DIRECTORY);

    if git_dir.exists() || git_worktree.exists() {
        repos.push(path.to_path_buf());
    } else {
        match fs::read_dir(path) {
            Ok(contents) => {
                for content in contents {
                    match content {
                        Ok(entry) => {
                            let path = entry.path();
                            if path.is_symlink() {
                                continue;
                            }
                            if path.is_dir() {
                                match find_repo_paths(&path) {
                                    Ok(ref mut r) => repos.append(r),
                                    Err(error) => return Err(error),
                                }
                            }
                        }
                        Err(e) => {
                            return Err(format!("Error accessing directory: {}", e));
                        }
                    };
                }
            }
            Err(e) => {
                return Err(format!(
                    "Failed to open \"{}\": {}",
                    &path.display(),
                    match e.kind() {
                        std::io::ErrorKind::NotADirectory =>
                            String::from("directory expected, but path is not a directory"),
                        std::io::ErrorKind::NotFound => String::from("not found"),
                        _ => format!("{:?}", e.kind()),
                    }
                ));
            }
        };
    }

    Ok(repos)
}

fn get_actual_git_directory(path: &Path, is_worktree: bool) -> PathBuf {
    match is_worktree {
        false => path.to_path_buf(),
        true => path.join(GIT_MAIN_WORKTREE_DIRECTORY),
    }
}

/// Find all git repositories under root, recursively
///
/// The bool in the return value specifies whether there is a repository
/// in root itself.
#[allow(clippy::type_complexity)]
fn find_repos(root: &Path) -> Result<Option<(Vec<Repo>, Vec<String>, bool)>, String> {
    let mut repos: Vec<Repo> = Vec::new();
    let mut repo_in_root = false;
    let mut warnings = Vec::new();

    for path in find_repo_paths(root)? {
        let is_worktree = RepoHandle::detect_worktree(&path);
        if path == root {
            repo_in_root = true;
        }

        match RepoHandle::open(&path, is_worktree) {
            Err(error) => {
                warnings.push(format!(
                    "Error opening repo {}{}: {}",
                    path.display(),
                    match is_worktree {
                        true => " as worktree",
                        false => "",
                    },
                    error
                ));
                continue;
            }
            Ok(repo) => {
                let remotes = match repo.remotes() {
                    Ok(remote) => remote,
                    Err(error) => {
                        warnings.push(format!(
                            "{}: Error getting remotes: {}",
                            &path_as_string(&path),
                            error
                        ));
                        continue;
                    }
                };

                let mut results: Vec<Remote> = Vec::new();
                for remote_name in remotes.iter() {
                    match repo.find_remote(remote_name)? {
                        Some(remote) => {
                            let name = remote.name();
                            let url = remote.url();
                            let remote_type = match detect_remote_type(&url) {
                                Some(t) => t,
                                None => {
                                    warnings.push(format!(
                                        "{}: Could not detect remote type of \"{}\"",
                                        &path_as_string(&path),
                                        &url
                                    ));
                                    continue;
                                }
                            };

                            results.push(Remote {
                                name,
                                url,
                                remote_type,
                            });
                        }
                        None => {
                            warnings.push(format!(
                                "{}: Remote {} not found",
                                &path_as_string(&path),
                                remote_name
                            ));
                            continue;
                        }
                    };
                }
                let remotes = results;

                let (namespace, name) = if path == root {
                    (
                        None,
                        match &root.parent() {
                            Some(parent) => path_as_string(path.strip_prefix(parent).unwrap()),
                            None => {
                                warnings.push(String::from("Getting name of the search root failed. Do you have a git repository in \"/\"?"));
                                continue;
                            }
                        },
                    )
                } else {
                    let name = path.strip_prefix(&root).unwrap();
                    let namespace = name.parent().unwrap();
                    (
                        if namespace != Path::new("") {
                            Some(path_as_string(namespace).to_string())
                        } else {
                            None
                        },
                        path_as_string(name),
                    )
                };

                repos.push(Repo {
                    name,
                    namespace,
                    remotes: Some(remotes),
                    worktree_setup: is_worktree,
                });
            }
        }
    }
    Ok(Some((repos, warnings, repo_in_root)))
}

pub fn find_in_tree(path: &Path) -> Result<(Tree, Vec<String>), String> {
    let mut warnings = Vec::new();

    let (repos, repo_in_root): (Vec<Repo>, bool) = match find_repos(path)? {
        Some((vec, mut repo_warnings, repo_in_root)) => {
            warnings.append(&mut repo_warnings);
            (vec, repo_in_root)
        }
        None => (Vec::new(), false),
    };

    let mut root = path.to_path_buf();
    if repo_in_root {
        root = match root.parent() {
            Some(root) => root.to_path_buf(),
            None => {
                return Err(String::from(
                    "Cannot detect root directory. Are you working in /?",
                ));
            }
        }
    }

    Ok((
        Tree {
            root: root.into_os_string().into_string().unwrap(),
            repos,
        },
        warnings,
    ))
}

pub fn add_worktree(
    directory: &Path,
    name: &str,
    subdirectory: Option<&Path>,
    track: Option<(&str, &str)>,
    no_track: bool,
) -> Result<(), String> {
    let repo = RepoHandle::open(directory, true).map_err(|error| match error.kind {
        RepoErrorKind::NotFound => {
            String::from("Current directory does not contain a worktree setup")
        }
        _ => format!("Error opening repo: {}", error),
    })?;

    let config = repo::read_worktree_root_config(directory)?;

    if repo.find_worktree(name).is_ok() {
        return Err(format!("Worktree {} already exists", &name));
    }

    let path = match subdirectory {
        Some(dir) => directory.join(dir).join(name),
        None => directory.join(Path::new(name)),
    };

    let mut remote_branch_exists = false;

    let default_checkout = || repo.default_branch()?.to_commit();

    let checkout_commit;
    if no_track {
        checkout_commit = default_checkout()?;
    } else {
        match track {
            Some((remote_name, remote_branch_name)) => {
                let remote_branch = repo.find_remote_branch(remote_name, remote_branch_name);
                match remote_branch {
                    Ok(branch) => {
                        remote_branch_exists = true;
                        checkout_commit = branch.to_commit()?;
                    }
                    Err(_) => {
                        remote_branch_exists = false;
                        checkout_commit = default_checkout()?;
                    }
                }
            }
            None => match &config {
                None => checkout_commit = default_checkout()?,
                Some(config) => match &config.track {
                    None => checkout_commit = default_checkout()?,
                    Some(track_config) => {
                        if track_config.default {
                            let remote_branch =
                                repo.find_remote_branch(&track_config.default_remote, name);
                            match remote_branch {
                                Ok(branch) => {
                                    remote_branch_exists = true;
                                    checkout_commit = branch.to_commit()?;
                                }
                                Err(_) => {
                                    checkout_commit = default_checkout()?;
                                }
                            }
                        } else {
                            checkout_commit = default_checkout()?;
                        }
                    }
                },
            },
        };
    }

    let mut target_branch = match repo.find_local_branch(name) {
        Ok(branchref) => branchref,
        Err(_) => repo.create_branch(name, &checkout_commit)?,
    };

    fn push(
        remote: &mut repo::RemoteHandle,
        branch_name: &str,
        remote_branch_name: &str,
        repo: &repo::RepoHandle,
    ) -> Result<(), String> {
        if !remote.is_pushable()? {
            return Err(format!(
                "Cannot push to non-pushable remote {}",
                remote.url()
            ));
        }
        remote.push(branch_name, remote_branch_name, repo)
    }

    if !no_track {
        if let Some((remote_name, remote_branch_name)) = track {
            if remote_branch_exists {
                target_branch.set_upstream(remote_name, remote_branch_name)?;
            } else {
                let mut remote = repo
                    .find_remote(remote_name)
                    .map_err(|error| format!("Error getting remote {}: {}", remote_name, error))?
                    .ok_or_else(|| format!("Remote {} not found", remote_name))?;

                push(
                    &mut remote,
                    &target_branch.name()?,
                    remote_branch_name,
                    &repo,
                )?;

                target_branch.set_upstream(remote_name, remote_branch_name)?;
            }
        } else if let Some(config) = config {
            if let Some(track_config) = config.track {
                if track_config.default {
                    let remote_name = track_config.default_remote;
                    if remote_branch_exists {
                        target_branch.set_upstream(&remote_name, name)?;
                    } else {
                        let remote_branch_name = match track_config.default_remote_prefix {
                            Some(prefix) => {
                                format!("{}{}{}", &prefix, BRANCH_NAMESPACE_SEPARATOR, &name)
                            }
                            None => name.to_string(),
                        };

                        let mut remote = repo
                            .find_remote(&remote_name)
                            .map_err(|error| {
                                format!("Error getting remote {}: {}", remote_name, error)
                            })?
                            .ok_or_else(|| format!("Remote {} not found", remote_name))?;

                        if !remote.is_pushable()? {
                            return Err(format!(
                                "Cannot push to non-pushable remote {}",
                                remote.url()
                            ));
                        }
                        push(
                            &mut remote,
                            &target_branch.name()?,
                            &remote_branch_name,
                            &repo,
                        )?;

                        target_branch.set_upstream(&remote_name, &remote_branch_name)?;
                    }
                }
            }
        }
    }

    if let Some(subdirectory) = subdirectory {
        std::fs::create_dir_all(subdirectory).map_err(|error| error.to_string())?;
    }
    repo.new_worktree(name, &path, &target_branch)?;

    Ok(())
}
