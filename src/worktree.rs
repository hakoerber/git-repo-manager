//! This handles worktrees for repositories. Some considerations to take care
//! of:
//!
//! * Which branch to check out / create
//! * Which commit to check out
//! * Whether to track a remote branch, and which
//!
//! There are a general rules. The main goal is to do the least surprising thing
//! in each situation, and to never change existing setups (e.g. tracking,
//! branch states) except when explicitly told to. In 99% of all cases, the
//! workflow will be quite straightforward.
//!
//! * The name of the worktree (and therefore the path) is **always** the same
//!   as the name of the branch.
//! * Never modify existing local branches
//! * Only modify tracking branches for existing local branches if explicitly
//!   requested
//! * By default, do not do remote operations. This means that we do no do any
//!   tracking setup (but of course, the local branch can already have a
//!   tracking branch set up, which will just be left alone)
//! * Be quite lax with finding a remote tracking branch (as using an existing
//!   branch is most likely preferred to creating a new branch)
//!
//! There are a few different options that can be given:
//!
//! * Explicit track (`--track`) and explicit no-track (`--no-track`)
//! * A configuration may specify to enable tracking a remote branch by default
//! * A configuration may specify a prefix for remote branches
//!
//! # How to handle the local branch?
//!
//! That one is easy: If a branch with the desired name already exists, all is
//! well. If not, we create a new one.
//!
//! # Which commit should be checked out?
//!
//! The most imporant rule: If the local branch already existed, just leave it
//! as it is. Only if a new branch is created do we need to answer the question
//! which commit to set it to. Generally, we set the branch to whatever the
//! "default" branch of the repository is (something like "main" or "master").
//! But there are a few cases where we can use remote branches to make the
//! result less surprising.
//!
//! First, if tracking is explicitly disabled, we still try to guess! But we
//! *do* ignore `--track`, as this is how it's done everywhere else.
//!
//! As an example: If `origin/foobar` exists and we run `grm worktree add foobar
//! --no-track`, we create a new worktree called `foobar` that's on the same
//! state as `origin/foobar` (but we will not set up tracking, see below).
//!
//! If tracking is explicitly requested to a certain state, we use that remote
//! branch. If it exists, easy. If not, no more guessing!
//!
//! Now, it's important to select the correct remote. In the easiest case, there
//! is only one remote, so we just use that one. If there is more than one
//! remote, we check whether there is a default remote configured via
//! `track.default_remote`. If yes, we use that one. If not, we have to do the
//! selection process below *for each of them*.  If only one of them returns
//! some branch to track, we use that one. If more than one remote returns
//! information, we only use it if it's identical for each. Otherwise we bail,
//! as there is no point in guessing.
//!
//! The commit selection process looks like this:
//!
//! * If a prefix is specified in the configuration, we look for
//!   `{remote}/{prefix}/{worktree_name}`
//!
//! * We look for `{remote}/{worktree_name}` (yes, this means that even when a
//!   prefix is configured, we use a branch *without* a prefix if one with
//!   prefix does not exist)
//!
//! Note that we may select different branches for different remotes when
//! prefixes is used. If remote1 has a branch with a prefix and remote2 only has
//! a branch *without* a prefix, we select them both when a prefix is used. This
//! could lead to the following situation:
//!
//! * There is `origin/prefix/foobar` and `remote2/foobar`, with different
//!   states
//! * You set `track.default_prefix = "prefix"` (and no default remote!)
//! * You run `grm worktree add prefix/foobar`
//! * Instead of just picking `origin/prefix/foobar`, grm will complain because
//!   it also selected `remote2/foobar`.
//!
//! This is just emergent behavior of the logic above. Fixing it would require
//! additional logic for that edge case. I assume that it's just so rare to get
//! that behavior that it's acceptable for now.
//!
//! Now we either have a commit, we aborted, or we do not have commit. In the
//! last case, as stated above, we check out the "default" branch.
//!
//! # The remote tracking branch
//!
//! First, the only remote operations we do is branch creation! It's
//! unfortunately not possible to defer remote branch creation until the first
//! `git push`, which would be ideal. The remote tracking branch has to already
//! exist, so we have to do the equivalent of `git push --set-upstream` during
//! worktree creation.
//!
//! Whether (and which) remote branch to track works like this:
//!
//! * If `--no-track` is given, we never track a remote branch, except when
//!   branch already has a tracking branch. So we'd be done already!
//!
//! * If `--track` is given, we always track this branch, regardless of anything
//!   else. If the branch exists, cool, otherwise we create it.
//!
//! If neither is given, we only set up tracking if requested in the
//! configuration file (`track.default = true`)
//!
//! The rest of the process is similar to the commit selection above. The only
//! difference is the remote selection.  If there is only one, we use it, as
//! before. Otherwise, we try to use `default_remote` from the configuration, if
//! available.  If not, we do not set up a remote tracking branch. It works like
//! this:
//!
//! * If a prefix is specified in the configuration, we use
//!   `{remote}/{prefix}/{worktree_name}`
//!
//! * If no prefix is specified in the configuration, we use
//!   `{remote}/{worktree_name}`
//!
//! Now that we have a remote, we use the same process as above:
//!
//! * If a prefix is specified in the configuration, we use for
//!   `{remote}/{prefix}/{worktree_name}`
//! * We use for `{remote}/{worktree_name}`
//!
//! ---
//!
//! All this means that in some weird situation, you may end up with the state
//! of a remote branch while not actually tracking that branch. This can only
//! happen in repositories with more than one remote. Imagine the following:
//!
//! The repository has two remotes (`remote1` and `remote2`) which have the
//! exact same remote state. But there is no `default_remote` in the
//! configuration (or no configuration at all). There is a remote branch
//! `foobar`. As both `remote1/foobar` and `remote2/foobar` as the same, the new
//! worktree will use that as the state of the new branch. But as `grm` cannot
//! tell which remote branch to track, it will not set up remote tracking. This
//! behavior may be a bit confusing, but first, there is no good way to resolve
//! this, and second, the situation should be really rare (when having multiple
//! remotes, you would generally have a `default_remote` configured).
//!
//! # Implementation
//!
//! To reduce the chance of bugs, the implementation uses the [typestate
//! pattern](http://cliffle.com/blog/rust-typestate/). Here are the states we
//! are moving through linearily:
//!
//! * Init
//! * A local branch name is set
//! * A local commit to set the new branch to is selected
//! * A remote tracking branch is selected
//! * The new branch is created with all the required settings
//!
//! Don't worry about the lifetime stuff: There is only one single lifetime, as
//! everything (branches, commits) is derived from the single `repo::Repo`
//! instance
//!
//! # Testing
//!
//! There are two types of input to the tests:
//!
//! 1) The parameters passed to `grm`, either via command line or via
//!    configuration file
//! 2) The circumstances in the repository and remotes
//!
//! ## Parameters
//!
//! * The name of the worktree
//!   * Whether it contains slashes or not
//!   * Whether it is invalid
//! * `--track` and `--no-track`
//! * Whether there is a configuration file and what it contains
//!   * Whether `track.default` is enabled or disabled
//!   * Whether `track.default_remote_prefix` is there or missing
//!   * Whether `track.default_remote` is there or missing
//!     * Whether that remote exists or not
//!
//! ## Situations
//!
//! ### The local branch
//!
//! * Whether the branch already exists
//! * Whether the branch has a remote tracking branch and whether it differs
//!   from the desired tracking branch (i.e. `--track` or config)
//!
//! ### Remotes
//!
//! * How many remotes there are, if any
//! * If more than two remotes exist, whether their desired tracking branch
//!   differs
//!
//! ### The remote tracking branch branch
//!
//! * Whether a remote branch with the same name as the worktree exists
//! * Whether a remote branch with the same name as the worktree plus prefix
//!   exists
//!
//! ## Outcomes
//!
//! We have to check the following afterwards:
//!
//! * Does the worktree exist in the correct location?
//! * Does the local branch have the same name as the worktree?
//! * Does the local branch have the correct commit?
//! * Does the local branch track the correct remote branch?
//! * Does that remote branch also exist?
use std::path::Path;

use thiserror::Error;

use super::{BranchName, RemoteName, config, repo};

pub const GIT_MAIN_WORKTREE_DIRECTORY: &str = ".git-main-working-tree";

struct Init;

enum LocalBranchInfo<'a> {
    NoBranch,
    Branch(repo::Branch<'a>),
}

struct WithLocalBranchName<'a> {
    local_branch_name: BranchName,
    local_branch: LocalBranchInfo<'a>,
}

struct WithLocalTargetSelected<'a> {
    local_branch_name: BranchName,
    local_branch: Option<repo::Branch<'a>>,
    target_commit: Option<Box<repo::Commit<'a>>>,
}

struct WithRemoteTrackingBranch<'a> {
    local_branch_name: BranchName,
    local_branch: Option<repo::Branch<'a>>,
    target_commit: Option<Box<repo::Commit<'a>>>,
    remote_tracking_branch: Option<(RemoteName, BranchName)>,
    prefix: Option<String>,
}

struct Worktree<'a, S: WorktreeState> {
    repo: &'a repo::RepoHandle,
    extra: S,
}

impl<'a> WithLocalBranchName<'a> {
    fn new(name: &BranchName, worktree: &Worktree<'a, Init>) -> Result<Self, Error> {
        Ok(Self {
            local_branch_name: name.clone(),
            local_branch: {
                let branch = worktree.repo.find_local_branch(name)?;
                match branch {
                    Some(branch) => LocalBranchInfo::Branch(branch),
                    None => LocalBranchInfo::NoBranch,
                }
            },
        })
    }
}

trait WorktreeState {}

impl WorktreeState for Init {}
impl WorktreeState for WithLocalBranchName<'_> {}
impl WorktreeState for WithLocalTargetSelected<'_> {}
impl WorktreeState for WithRemoteTrackingBranch<'_> {}

impl<'a> Worktree<'a, Init> {
    fn new(repo: &'a repo::RepoHandle) -> Self {
        Self {
            repo,
            extra: Init {},
        }
    }

    fn set_local_branch_name(
        self,
        name: &BranchName,
    ) -> Result<Worktree<'a, WithLocalBranchName<'a>>, Error> {
        Ok(Worktree::<WithLocalBranchName> {
            repo: self.repo,
            extra: WithLocalBranchName::new(name, &self)?,
        })
    }
}

impl<'a, 'b> Worktree<'a, WithLocalBranchName<'b>>
where
    'a: 'b,
{
    // fn check_local_branch(&self) {
    //     let mut branchref = self.extra.local_branch.borrow_mut();
    //     if branchref.is_none() {
    //         let branch = self.repo.find_local_branch(&self.extra.local_branch_name);
    //         *branchref = Some(branch.ok());
    //     }
    // }

    fn local_branch_already_exists(&self) -> bool {
        matches!(
            self.extra.local_branch,
            LocalBranchInfo::Branch(ref _branch)
        )
    }

    fn select_commit(
        self,
        commit: Option<Box<repo::Commit<'b>>>,
    ) -> Worktree<'a, WithLocalTargetSelected<'b>> {
        Worktree::<'a, WithLocalTargetSelected> {
            repo: self.repo,
            extra: WithLocalTargetSelected::<'b> {
                local_branch_name: self.extra.local_branch_name,
                // As we just called `check_local_branch`, we can be sure that
                // `self.extra.local_branch` is set to some `Some` value
                local_branch: match self.extra.local_branch {
                    LocalBranchInfo::NoBranch => None,
                    LocalBranchInfo::Branch(branch) => Some(branch),
                },
                target_commit: commit,
            },
        }
    }
}

impl<'a> Worktree<'a, WithLocalTargetSelected<'a>> {
    fn set_remote_tracking_branch(
        self,
        branch: Option<(&RemoteName, &BranchName)>,
        prefix: Option<&str>,
    ) -> Worktree<'a, WithRemoteTrackingBranch<'a>> {
        Worktree::<WithRemoteTrackingBranch> {
            repo: self.repo,
            extra: WithRemoteTrackingBranch {
                local_branch_name: self.extra.local_branch_name,
                local_branch: self.extra.local_branch,
                target_commit: self.extra.target_commit,
                remote_tracking_branch: branch.map(|(s1, s2)| (s1.clone(), s2.clone())),
                prefix: prefix.map(ToOwned::to_owned),
            },
        }
    }
}

impl<'a> Worktree<'a, WithRemoteTrackingBranch<'a>> {
    fn create(self, directory: &Path) -> Result<Option<Vec<String>>, Error> {
        let mut warnings: Vec<String> = vec![];

        let mut branch = if let Some(branch) = self.extra.local_branch {
            branch
        } else {
            self.repo.create_branch(
                &self.extra.local_branch_name,
                // TECHDEBT
                // We must not call this with `Some()` without a valid target.
                // I'm sure this can be improved, just not sure how.
                &self
                    .extra
                    .target_commit
                    .expect("target_commit must not be empty"),
            )?
        };

        if let Some((remote_name, remote_branch_name)) = self.extra.remote_tracking_branch {
            let remote_branch_with_prefix = if let Some(ref prefix) = self.extra.prefix {
                self.repo
                    .find_remote_branch(
                        &remote_name,
                        &BranchName::new(format!("{prefix}/{remote_branch_name}")),
                    )
                    .ok()
            } else {
                None
            };

            let remote_branch_without_prefix = self
                .repo
                .find_remote_branch(&remote_name, &remote_branch_name)
                .ok();

            let remote_branch = if let Some(ref _prefix) = self.extra.prefix {
                remote_branch_with_prefix
            } else {
                remote_branch_without_prefix
            };

            if let Some(remote_branch) = remote_branch {
                if branch.commit()?.id().hex_string() != remote_branch.commit()?.id().hex_string() {
                    warnings.push(format!("The local branch \"{}\" and the remote branch \"{}/{}\" differ. Make sure to push/pull afterwards!", &self.extra.local_branch_name, &remote_name, &remote_branch_name));
                }

                branch.set_upstream(&remote_name, &remote_branch.basename()?)?;
            } else {
                let Some(mut remote) = self.repo.find_remote(&remote_name)? else {
                    return Err(Error::RemoteNotFound { name: remote_name });
                };

                if !remote.is_pushable()? {
                    return Err(Error::RemoteNotPushable { name: remote_name });
                }

                if let Some(prefix) = self.extra.prefix {
                    remote.push(
                        &self.extra.local_branch_name,
                        &BranchName::new(format!("{prefix}/{remote_branch_name}")),
                        self.repo,
                    )?;

                    branch.set_upstream(
                        &remote_name,
                        &BranchName::new(format!("{prefix}/{remote_branch_name}")),
                    )?;
                } else {
                    remote.push(
                        &self.extra.local_branch_name,
                        &remote_branch_name,
                        self.repo,
                    )?;

                    branch.set_upstream(&remote_name, &remote_branch_name)?;
                }
            }
        }

        let branch_name = self.extra.local_branch_name.into_string();
        // We have to create subdirectories first, otherwise adding the worktree
        // will fail
        if branch_name.contains('/') {
            let path = Path::new(&branch_name);
            if let Some(base) = path.parent() {
                // This is a workaround of a bug in libgit2 (?)
                //
                // When *not* doing this, we will receive an error from the
                // `Repository::worktree()` like this:
                //
                // > failed to make directory '/{repo}/.git-main-working-tree/worktrees/dir/test
                //
                // This is a discrepancy between the behavior of libgit2 and the
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
                // I *guess* that the mapping back from the worktree directory under .git to the
                // actual worktree directory is done via the `gitdir` file
                // inside `.git/worktrees/{worktree}. This means that the actual
                // directory would not matter. You can verify this by
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
                )?;
                std::fs::create_dir_all(base)?;
            }
        }

        self.repo
            .new_worktree(&branch_name, &directory.join(&branch_name), &branch)?;

        Ok(if warnings.is_empty() {
            None
        } else {
            Some(warnings)
        })
    }
}

#[derive(Debug, Error)]
enum WorktreeValidationErrorReason {
    #[error("cannot start or end with a slash")]
    SlashAtStartOrEnd,
    #[error("cannot contain two consecutive slashes")]
    ConsecutiveSlashes,
    #[error("cannot contain whitespace")]
    ContainsWhitespace,
}

#[derive(Debug, Error)]
#[error("invalid worktree name \"{}\": {}", .name, .reason)]
pub struct WorktreeValidationError {
    name: String,
    reason: WorktreeValidationErrorReason,
}

/// A branch name must never start or end with a slash, and it cannot have two
/// consecutive slashes
fn validate_worktree_name(name: &str) -> Result<(), WorktreeValidationError> {
    if name.starts_with('/') || name.ends_with('/') {
        return Err(WorktreeValidationError {
            name: name.to_owned(),
            reason: WorktreeValidationErrorReason::SlashAtStartOrEnd,
        });
    }

    if name.contains("//") {
        return Err(WorktreeValidationError {
            name: name.to_owned(),
            reason: WorktreeValidationErrorReason::ConsecutiveSlashes,
        });
    }

    if name.contains(char::is_whitespace) {
        return Err(WorktreeValidationError {
            name: name.to_owned(),
            reason: WorktreeValidationErrorReason::ContainsWhitespace,
        });
    }

    Ok(())
}

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Repo(#[from] repo::Error),
    #[error(transparent)]
    Config(#[from] config::Error),
    #[error(transparent)]
    InvalidWorktreeName(#[from] WorktreeValidationError),
    #[error("Remote \"{name}\" not found", name = .name)]
    RemoteNotFound { name: RemoteName },
    #[error("Cannot push to non-pushable remote \"{name}\"", name = .name)]
    RemoteNotPushable { name: RemoteName },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Current directory does not contain a worktree setup")]
    NotAWorktreeSetup,
    #[error("Worktree {} already exists", .name)]
    WorktreeAlreadyExists { name: String },
}

// TECHDEBT
//
// Instead of opening the repo & reading configuration inside the function, it
// should be done by the caller and given as a parameter
pub fn add_worktree(
    directory: &Path,
    name: &str,
    track: Option<(RemoteName, BranchName)>,
    no_track: bool,
) -> Result<Option<Vec<String>>, Error> {
    let mut warnings: Vec<String> = vec![];

    validate_worktree_name(name)?;

    let repo = repo::RepoHandle::open(directory, true).map_err(|error| match error {
        repo::Error::NotFound => Error::NotAWorktreeSetup,
        _ => error.into(),
    })?;

    let remotes = &repo.remotes()?;

    let config: Option<repo::WorktreeRootConfig> =
        config::read_worktree_root_config(directory)?.map(Into::into);

    if repo.find_worktree(name).is_ok() {
        return Err(Error::WorktreeAlreadyExists {
            name: name.to_owned(),
        });
    }

    let track_config = config.and_then(|config| config.track);
    let prefix = track_config
        .as_ref()
        .and_then(|track| track.default_remote_prefix.as_ref());
    let enable_tracking = track_config.as_ref().is_some_and(|track| track.default);
    let default_remote = track_config
        .as_ref()
        .map(|track| track.default_remote.clone());

    // Note that we have to define all variables that borrow from `repo`
    // *first*, otherwise we'll receive "borrowed value does not live long
    // enough" errors. This is due to the `repo` reference inside `Worktree` that is
    // passed through each state type.
    //
    // The `commit` variable will be dropped at the end of the scope, together with
    // all worktree variables. It will be done in the opposite direction of
    // delcaration (FILO).
    //
    // So if we define `commit` *after* the respective worktrees, it will be dropped
    // first while still being borrowed by `Worktree`.
    let default_branch_head = repo.default_branch()?.commit_owned()?;

    let worktree =
        Worktree::<Init>::new(&repo).set_local_branch_name(&BranchName::new(name.to_owned()))?;

    let get_remote_head = |remote_name: &RemoteName,
                           remote_branch_name: &str|
     -> Result<Option<Box<repo::Commit>>, Error> {
        if let Ok(remote_branch) =
            repo.find_remote_branch(remote_name, &BranchName::new(remote_branch_name.to_owned()))
        {
            Ok(Some(Box::new(remote_branch.commit_owned()?)))
        } else {
            Ok(None)
        }
    };

    let worktree = if worktree.local_branch_already_exists() {
        worktree.select_commit(None)
    } else {
        #[expect(
            clippy::pattern_type_mismatch,
            reason = "i cannot get this to work properly, but it's fine as it is"
        )]
        if let Some((remote_name, remote_branch_name)) =
            if no_track { None } else { track.as_ref() }
        {
            if let Ok(remote_branch) = repo.find_remote_branch(remote_name, remote_branch_name) {
                worktree.select_commit(Some(Box::new(remote_branch.commit_owned()?)))
            } else {
                worktree.select_commit(Some(Box::new(default_branch_head)))
            }
        } else {
            match remotes.len() {
                0 => worktree.select_commit(Some(Box::new(default_branch_head))),
                1 => {
                    #[expect(clippy::indexing_slicing, reason = "checked for len() explicitly")]
                    let remote_name = &remotes[0];
                    let commit: Option<Box<repo::Commit>> = ({
                        if let Some(prefix) = prefix {
                            get_remote_head(remote_name, &format!("{prefix}/{name}"))?
                        } else {
                            None
                        }
                    })
                    .or(get_remote_head(remote_name, name)?)
                    .or_else(|| Some(Box::new(default_branch_head)));

                    worktree.select_commit(commit)
                }
                _ => {
                    let commit = if let Some(ref default_remote) = default_remote {
                    if let Some(prefix) = prefix {
                        if let Ok(remote_branch) = repo
                            .find_remote_branch(default_remote, &BranchName::new(format!("{prefix}/{name}")))
                        {
                            Some(Box::new(remote_branch.commit_owned()?))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                    .or({
                        if let Ok(remote_branch) =
                            repo.find_remote_branch(default_remote, &BranchName::new(name.to_owned()))
                        {
                            Some(Box::new(remote_branch.commit_owned()?))
                        } else {
                            None
                        }
                    })
                } else {
                    None
                }.or({
                    let mut commits = vec![];
                    for remote_name in remotes {
                        let remote_head: Option<Box<repo::Commit>> = ({
                            if let Some(prefix) = prefix {
                                if let Ok(remote_branch) = repo.find_remote_branch(
                                    remote_name,
                                    &BranchName::new(format!("{prefix}/{name}")),
                                ) {
                                    Some(Box::new(remote_branch.commit_owned()?))
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        })
                        .or({
                            if let Ok(remote_branch) =
                                repo.find_remote_branch(remote_name, &BranchName::new(name.to_owned()))
                            {
                                Some(Box::new(remote_branch.commit_owned()?))
                            } else {
                                None
                            }
                        })
                        .or(None);
                        commits.push(remote_head);
                    }

                    let mut commits = commits
                        .into_iter()
                        .flatten()
                        // have to collect first because the `flatten()` return
                        // typedoes not implement `windows()`
                        .collect::<Vec<Box<repo::Commit>>>();
                    // `flatten()` takes care of `None` values here. If all
                    // remotes return None for the branch, we do *not* abort, we
                    // continue!
                    if commits.is_empty() {
                        Some(Box::new(default_branch_head))
                    } else if commits.len() == 1 {
                        Some(commits.swap_remove(0))
                    } else if commits.windows(2).any(
                        #[expect(
                            clippy::missing_asserts_for_indexing,
                            clippy::indexing_slicing,
                            reason = "windows function always returns two elements"
                        )]
                        |window| {
                            let c1 = &window[0];
                            let c2 = &window[1];
                            (*c1).id().hex_string() != (*c2).id().hex_string()
                        }) {
                        warnings.push(
                            // TODO this should also include the branch
                            // name. BUT: the branch name may be different
                            // between the remotes. Let's just leave it
                            // until I get around to fix that inconsistency
                            // (see module-level doc about), which might be
                            // never, as it's such a rare edge case.
                            "Branch exists on multiple remotes, but they deviate. Selecting default branch instead".to_owned()
                        );
                        Some(Box::new(default_branch_head))
                    } else {
                        Some(commits.swap_remove(0))
                    }
                });
                    worktree.select_commit(commit)
                }
            }
        }
    };

    let worktree = if no_track {
        worktree.set_remote_tracking_branch(None, prefix.map(String::as_str))
    } else if let Some((remote_name, remote_branch_name)) = track {
        worktree.set_remote_tracking_branch(
            Some((&remote_name, &remote_branch_name)),
            None, // Always disable prefixing when explicitly given --track
        )
    } else if !enable_tracking {
        worktree.set_remote_tracking_branch(None, prefix.map(String::as_str))
    } else {
        match remotes.len() {
            0 => worktree.set_remote_tracking_branch(None, prefix.map(String::as_str)),
            1 =>
            {
                #[expect(clippy::indexing_slicing, reason = "checked for len() explicitly")]
                worktree.set_remote_tracking_branch(
                    Some((&remotes[0], &BranchName::new(name.to_owned()))),
                    prefix.map(String::as_str),
                )
            }
            _ => {
                if let Some(default_remote) = default_remote {
                    worktree.set_remote_tracking_branch(
                        Some((&default_remote, &BranchName::new(name.to_owned()))),
                        prefix.map(String::as_str),
                    )
                } else {
                    worktree.set_remote_tracking_branch(None, prefix.map(String::as_str))
                }
            }
        }
    };

    worktree.create(directory)?;

    Ok(if warnings.is_empty() {
        None
    } else {
        Some(warnings)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_worktree_names() {
        assert!(add_worktree(Path::new("/tmp/"), "/leadingslash", None, false).is_err());
        assert!(add_worktree(Path::new("/tmp/"), "trailingslash/", None, false).is_err());
        assert!(add_worktree(Path::new("/tmp/"), "//", None, false).is_err());
        assert!(add_worktree(Path::new("/tmp/"), "test//test", None, false).is_err());
        assert!(add_worktree(Path::new("/tmp/"), "test test", None, false).is_err());
        assert!(add_worktree(Path::new("/tmp/"), "test\ttest", None, false).is_err());
    }
}
