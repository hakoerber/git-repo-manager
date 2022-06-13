use std::path::Path;

use super::output::*;
use super::repo;

pub const GIT_MAIN_WORKTREE_DIRECTORY: &str = ".git-main-working-tree";

// The logic about the base branch and the tracking branch is as follows:
//
// * If a branch with the same name does not exist and no track is given, use the default
//   branch
//
// * If a branch with the same name exists and no track is given, use that
//
// * If a branch with the same name does not exist and track is given, use the
//   local branch that tracks that branch
//
// * If a branch with the same name exists and track is given, use the locally
//   existing branch. If the locally existing branch is not the local branch to
//   the remote tracking branch, issue a warning
pub fn add_worktree(
    directory: &Path,
    name: &str,
    subdirectory: Option<&Path>,
    track: Option<(&str, &str)>,
    no_track: bool,
) -> Result<(), String> {
    let repo = repo::RepoHandle::open(directory, true).map_err(|error| match error.kind {
        repo::RepoErrorKind::NotFound => {
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

    let mut target_branch = match repo.find_local_branch(name) {
        Ok(branchref) => {
            if !no_track {
                if let Some((remote_name, remote_branch_name)) = track {
                    let remote_branch = repo.find_remote_branch(remote_name, remote_branch_name);
                    if let Ok(remote_branch) = remote_branch {
                        remote_branch_exists = true;
                        if let Ok(local_upstream_branch) = branchref.upstream() {
                            if remote_branch.name()? != local_upstream_branch.name()? {
                                print_warning(&format!(
                                    "You specified a tracking branch ({}/{}) for an existing branch ({}), but \
                                    it differs from the current upstream ({}). Will keep current upstream"
                                , remote_name, remote_branch_name, branchref.name()?, local_upstream_branch.name()?))
                            }
                        }
                    }
                }
            }
            branchref
        }
        Err(_) => {
            let default_checkout = || repo.default_branch()?.to_commit();

            let checkout_commit;

            if no_track {
                checkout_commit = default_checkout()?;
            } else {
                match track {
                    Some((remote_name, remote_branch_name)) => {
                        let remote_branch =
                            repo.find_remote_branch(remote_name, remote_branch_name);
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

            repo.create_branch(name, &checkout_commit)?
        }
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
                                format!("{}{}{}", &prefix, super::BRANCH_NAMESPACE_SEPARATOR, &name)
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
