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
    track: Option<(&str, &str)>,
    no_track: bool,
) -> Result<(), String> {
    // A branch name must never start or end with a slash. Everything else is ok.
    if name.starts_with('/') || name.ends_with('/') {
        return Err(format!(
            "Invalid worktree name: {}. It cannot start or end with a slash",
            name
        ));
    }

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

    // We have to create subdirectories first, otherwise adding the worktree
    // will fail
    if name.contains('/') {
        let path = Path::new(&name);
        if let Some(base) = path.parent() {
            // This is a workaround of a bug in libgit2 (?)
            //
            // When *not* doing this, we will receive an error from the `Repository::worktree()`
            // like this:
            //
            // > failed to make directory '/{repo}/.git-main-working-tree/worktrees/dir/test
            //
            // This is a discrepancy between the behaviour of libgit2 and the
            // git CLI when creating worktrees with slashes:
            //
            // The git CLI will create the worktree's configuration directory
            // inside {git_dir}/worktrees/{last_path_component}. Look at this:
            //
            // ```
            // $ git worktree add 1/2/3 -b 1/2/3
            // $ ls .git/worktrees
            // 3
            // ```
            //
            // Interesting: When adding a worktree with a different name but the
            // same final path component, git starts adding a counter suffix to
            // the worktree directories:
            //
            // ```
            // $ git worktree add 1/3/3 -b 1/3/3
            // $ git worktree add 1/4/3 -b 1/4/3
            // $ ls .git/worktrees
            // 3
            // 31
            // 32
            // ```
            //
            // I *guess* that the mapping back from the worktree directory under .git to the actual
            // worktree directory is done via the `gitdir` file inside `.git/worktrees/{worktree}.
            // This means that the actual directory would not matter. You can verify this by
            // just renaming it:
            //
            // ```
            // $ mv .git/worktrees/3 .git/worktrees/foobar
            // $ git worktree list
            // /tmp/       fcc8a2a7 [master]
            // /tmp/1/2/3  fcc8a2a7 [1/2/3]
            // /tmp/1/3/3  fcc8a2a7 [1/3/3]
            // /tmp/1/4/3  fcc8a2a7 [1/4/3]
            // ```
            //
            // => Still works
            //
            // Anyway, libgit2 does not do this: It tries to create the worktree
            // directory inside .git with the exact name of the worktree, including
            // any slashes. It should be this code:
            //
            // https://github.com/libgit2/libgit2/blob/f98dd5438f8d7bfd557b612fdf1605b1c3fb8eaf/src/libgit2/worktree.c#L346
            //
            // As a workaround, we can create the base directory manually for now.
            //
            // Tracking upstream issue: https://github.com/libgit2/libgit2/issues/6327
            std::fs::create_dir_all(
                directory
                    .join(GIT_MAIN_WORKTREE_DIRECTORY)
                    .join("worktrees")
                    .join(base),
            )
            .map_err(|error| error.to_string())?;
            std::fs::create_dir_all(base).map_err(|error| error.to_string())?;
        }
    }

    repo.new_worktree(name, &directory.join(&name), &target_branch)?;

    Ok(())
}
