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
mod error;

pub use error::{
    CleanupWorktreeError, CleanupWorktreeWarning, CleanupWorktreeWarningReason, Error,
    WorktreeConversionError, WorktreeRemoveError, WorktreeValidationError,
    WorktreeValidationErrorReason,
};

use std::{fmt, iter, sync::mpsc};

use camino::{Utf8Path as Path, Utf8PathBuf as PathBuf};

use super::{Branch, BranchName, RemoteName, RepoHandle, Warning, config};
use crate::{
    path,
    repo::{self, RepoChanges},
};

pub const GIT_MAIN_WORKTREE_DIRECTORY: &str = ".git-main-working-tree";

pub struct Worktree {
    name: WorktreeName,
}

impl Worktree {
    /// A branch name must never start or end with a slash, and it cannot have two
    /// consecutive slashes
    fn new(name: &str) -> Result<Self, WorktreeValidationError> {
        Ok(Self {
            name: WorktreeName::new(name.to_owned())?,
        })
    }

    pub fn name(&self) -> &WorktreeName {
        &self.name
    }

    fn into_name(self) -> WorktreeName {
        self.name
    }

    pub fn forward_branch(&self, rebase: bool, stash: bool) -> Result<Option<Warning>, Error> {
        let repo = RepoHandle::open(Path::new(&self.name.as_str()))?;

        let branch_name = BranchName::new(self.name.as_str().to_owned());

        if let Some(remote_branch) = repo
            .find_local_branch(&branch_name)?
            .ok_or(Error::BranchNotFound(branch_name))?
            .upstream()?
        {
            let status = repo.status(WorktreeSetup::NoWorktree)?;
            let mut stashed_changes = false;

            if !status.clean() {
                if stash {
                    repo.stash()?;
                    stashed_changes = true;
                } else {
                    return Ok(Some(Warning(String::from("Worktree contains changes"))));
                }
            }

            let unstash = || -> Result<(), Error> {
                if stashed_changes {
                    repo.stash_pop()?;
                }
                Ok(())
            };

            let remote_annotated_commit = repo
                .0
                .find_annotated_commit(remote_branch.commit()?.id().0)?;

            if rebase {
                let mut rebase = repo.0.rebase(
                    None, // use HEAD
                    Some(&remote_annotated_commit),
                    None, // figure out the base yourself, libgit2!
                    Some(&mut git2::RebaseOptions::new()),
                )?;

                while let Some(operation) = rebase.next() {
                    let operation = operation?;

                    // This is required to preserve the commiter of the rebased
                    // commits, which is the expected behavior.
                    let rebased_commit = repo.0.find_commit(operation.id())?;
                    let committer = rebased_commit.committer();

                    // This is effectively adding all files to the index explicitly.
                    // Normal files are already staged, but changed submodules are not.
                    let mut index = repo.0.index()?;
                    index.add_all(iter::once("."), git2::IndexAddOption::CHECK_PATHSPEC, None)?;

                    if let Err(error) = rebase.commit(None, &committer, None) {
                        if error.code() == git2::ErrorCode::Applied {
                            continue;
                        }
                        rebase.abort()?;
                        unstash()?;
                        return Err(error.into());
                    }
                }

                rebase.finish(None)?;
            } else {
                let (analysis, _preference) = repo.0.merge_analysis(&[&remote_annotated_commit])?;

                if analysis.is_up_to_date() {
                    unstash()?;
                    return Ok(None);
                }
                if !analysis.is_fast_forward() {
                    unstash()?;
                    return Ok(Some(Warning(String::from(
                        "Worktree cannot be fast forwarded",
                    ))));
                }

                repo.0.reset(
                    remote_branch.commit()?.0.as_object(),
                    git2::ResetType::Hard,
                    Some(git2::build::CheckoutBuilder::new().safe()),
                )?;
            }
            unstash()?;
        } else {
            return Ok(Some(Warning(String::from(
                "No remote branch to rebase onto",
            ))));
        }

        Ok(None)
    }

    pub fn rebase_onto_default(
        &self,
        config: &Option<WorktreeRootConfig>,
        stash: bool,
    ) -> Result<Option<Warning>, Error> {
        let repo = RepoHandle::open(Path::new(&self.name.as_str()))?;

        let guess_default_branch = || repo.default_branch()?.name();

        let default_branch_name = match *config {
            None => guess_default_branch()?,
            Some(ref config) => match config.persistent_branches {
                None => guess_default_branch()?,
                Some(ref persistent_branches) => {
                    if let Some(branch) = persistent_branches.first() {
                        branch.clone()
                    } else {
                        guess_default_branch()?
                    }
                }
            },
        };

        let status = repo.status(WorktreeSetup::NoWorktree)?;
        let mut stashed_changes = false;

        if !status.clean() {
            if stash {
                repo.stash()?;
                stashed_changes = true;
            } else {
                return Ok(Some(Warning("Worktree contains changes".to_owned())));
            }
        }

        let unstash = || -> Result<(), Error> {
            if stashed_changes {
                repo.stash_pop()?;
            }
            Ok(())
        };

        let base_branch = repo
            .find_local_branch(&default_branch_name)?
            .ok_or(Error::BranchNotFound(default_branch_name))?;
        let base_annotated_commit = repo.0.find_annotated_commit(base_branch.commit()?.id().0)?;

        let mut rebase = repo.0.rebase(
            None, // use HEAD
            Some(&base_annotated_commit),
            None, // figure out the base yourself, libgit2!
            Some(&mut git2::RebaseOptions::new()),
        )?;

        while let Some(operation) = rebase.next() {
            let operation = operation?;

            // This is required to preserve the commiter of the rebased
            // commits, which is the expected behavior.
            let rebased_commit = repo.0.find_commit(operation.id())?;
            let committer = rebased_commit.committer();

            // This is effectively adding all files to the index explicitly.
            // Normal files are already staged, but changed submodules are not.
            let mut index = repo.0.index()?;
            index.add_all(iter::once("."), git2::IndexAddOption::CHECK_PATHSPEC, None)?;

            if let Err(error) = rebase.commit(None, &committer, None) {
                if error.code() == git2::ErrorCode::Applied {
                    continue;
                }
                rebase.abort()?;
                unstash()?;
                return Err(error.into());
            }
        }

        rebase.finish(None)?;
        unstash()?;
        Ok(None)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum WorktreeSetup {
    Worktree,
    NoWorktree,
}

impl WorktreeSetup {
    pub fn is_worktree(&self) -> bool {
        *self == Self::Worktree
    }

    pub fn detect(path: &Path) -> Self {
        if path.join(GIT_MAIN_WORKTREE_DIRECTORY).exists() {
            Self::Worktree
        } else {
            Self::NoWorktree
        }
    }
}

impl From<bool> for WorktreeSetup {
    fn from(value: bool) -> Self {
        if value {
            Self::Worktree
        } else {
            Self::NoWorktree
        }
    }
}

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
    target_commit: Option<repo::Commit<'a>>,
}

struct RemoteTrackingBranch {
    remote_name: RemoteName,
    remote_branch_name: BranchName,
    prefix: Option<String>,
}

struct WithRemoteTrackingBranch<'a> {
    local_branch_name: BranchName,
    local_branch: Option<repo::Branch<'a>>,
    target_commit: Option<repo::Commit<'a>>,
    remote_tracking_branch: Option<RemoteTrackingBranch>,
}

struct NewWorktree<'a, S: WorktreeState> {
    repo: &'a WorktreeRepoHandle,
    extra: S,
}

impl<'a> WithLocalBranchName<'a> {
    fn new(name: &BranchName, worktree: &NewWorktree<'a, Init>) -> Result<Self, Error> {
        Ok(Self {
            local_branch_name: name.clone(),
            local_branch: {
                let branch = worktree.repo.as_repo().find_local_branch(name)?;
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

impl<'a> NewWorktree<'a, Init> {
    fn new(repo: &'a WorktreeRepoHandle) -> Self {
        Self {
            repo,
            extra: Init {},
        }
    }

    fn set_local_branch_name(
        self,
        name: &BranchName,
    ) -> Result<NewWorktree<'a, WithLocalBranchName<'a>>, Error> {
        Ok(NewWorktree::<WithLocalBranchName> {
            repo: self.repo,
            extra: WithLocalBranchName::new(name, &self)?,
        })
    }
}

impl<'a, 'b> NewWorktree<'a, WithLocalBranchName<'b>>
where
    'a: 'b,
{
    fn local_branch_already_exists(&self) -> bool {
        matches!(
            self.extra.local_branch,
            LocalBranchInfo::Branch(ref _branch)
        )
    }

    fn select_commit(
        self,
        commit: Option<repo::Commit<'b>>,
    ) -> NewWorktree<'a, WithLocalTargetSelected<'b>> {
        NewWorktree::<'a, WithLocalTargetSelected> {
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

impl<'a> NewWorktree<'a, WithLocalTargetSelected<'a>> {
    fn set_remote_tracking_branch(
        self,
        branch: Option<RemoteTrackingBranch>,
    ) -> NewWorktree<'a, WithRemoteTrackingBranch<'a>> {
        NewWorktree::<WithRemoteTrackingBranch> {
            repo: self.repo,
            extra: WithRemoteTrackingBranch {
                local_branch_name: self.extra.local_branch_name,
                local_branch: self.extra.local_branch,
                target_commit: self.extra.target_commit,
                remote_tracking_branch: branch,
            },
        }
    }
}

impl<'a> NewWorktree<'a, WithRemoteTrackingBranch<'a>> {
    fn create(self, directory: &Path) -> Result<Option<Vec<Warning>>, Error> {
        let mut warnings: Vec<Warning> = vec![];

        let mut branch = if let Some(branch) = self.extra.local_branch {
            branch
        } else {
            self.repo.as_repo().create_branch(
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

        if let Some(remote_branch_config) = self.extra.remote_tracking_branch {
            let remote_branch_with_prefix = if let Some(ref prefix) = remote_branch_config.prefix {
                self.repo.as_repo().find_remote_branch(
                    &remote_branch_config.remote_name,
                    &BranchName::new(format!(
                        "{prefix}/{}",
                        remote_branch_config.remote_branch_name
                    )),
                )?
            } else {
                None
            };

            let remote_branch_without_prefix = self.repo.as_repo().find_remote_branch(
                &remote_branch_config.remote_name,
                &remote_branch_config.remote_branch_name,
            )?;

            let remote_branch = if let Some(ref _prefix) = remote_branch_config.prefix {
                remote_branch_with_prefix
            } else {
                remote_branch_without_prefix
            };

            if let Some(remote_branch) = remote_branch {
                if branch.commit()?.id().hex_string() != remote_branch.commit()?.id().hex_string() {
                    warnings.push(Warning(format!("The local branch \"{}\" and the remote branch \"{}/{}\" differ. Make sure to push/pull afterwards!", &self.extra.local_branch_name, &remote_branch_config.remote_name, &remote_branch_config.remote_branch_name)));
                }

                branch.set_upstream(
                    &remote_branch_config.remote_name,
                    &remote_branch.basename()?,
                )?;
            } else {
                let Some(mut remote) = self
                    .repo
                    .as_repo()
                    .find_remote(&remote_branch_config.remote_name)?
                else {
                    return Err(Error::RemoteNotFound {
                        name: remote_branch_config.remote_name,
                    });
                };

                if !remote.is_pushable()? {
                    return Err(Error::RemoteNotPushable {
                        name: remote_branch_config.remote_name,
                    });
                }

                if let Some(prefix) = remote_branch_config.prefix {
                    remote.push(
                        &self.extra.local_branch_name,
                        &BranchName::new(format!(
                            "{prefix}/{}",
                            remote_branch_config.remote_branch_name
                        )),
                        self.repo.as_repo(),
                    )?;

                    branch.set_upstream(
                        &remote_branch_config.remote_name,
                        &BranchName::new(format!(
                            "{prefix}/{}",
                            remote_branch_config.remote_branch_name
                        )),
                    )?;
                } else {
                    remote.push(
                        &self.extra.local_branch_name,
                        &remote_branch_config.remote_branch_name,
                        self.repo.as_repo(),
                    )?;

                    branch.set_upstream(
                        &remote_branch_config.remote_name,
                        &remote_branch_config.remote_branch_name,
                    )?;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeName(String);

impl fmt::Display for WorktreeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl WorktreeName {
    pub fn new(name: String) -> Result<Self, WorktreeValidationError> {
        if name.starts_with('/') || name.ends_with('/') {
            return Err(WorktreeValidationError {
                name,
                reason: WorktreeValidationErrorReason::SlashAtStartOrEnd,
            });
        }

        if name.contains("//") {
            return Err(WorktreeValidationError {
                name,
                reason: WorktreeValidationErrorReason::ConsecutiveSlashes,
            });
        }

        if name.contains(char::is_whitespace) {
            return Err(WorktreeValidationError {
                name,
                reason: WorktreeValidationErrorReason::ContainsWhitespace,
            });
        }

        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub enum TrackingSelection {
    Explicit {
        remote_name: RemoteName,
        remote_branch_name: BranchName,
    },
    Automatic,
    Disabled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_worktree_names() {
        assert!(WorktreeName::new("/leadingslash".to_owned()).is_err());
        assert!(WorktreeName::new("trailingslash/".to_owned()).is_err());
        assert!(WorktreeName::new("//".to_owned()).is_err());
        assert!(WorktreeName::new("test//test".to_owned()).is_err());
        assert!(WorktreeName::new("test test".to_owned()).is_err());
        assert!(WorktreeName::new("test\ttest".to_owned()).is_err());
    }
}

pub struct WorktreeRootConfig {
    pub persistent_branches: Option<Vec<BranchName>>,
    pub track: Option<TrackingConfig>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TrackingDefault {
    Track,
    NoTrack,
}

pub struct TrackingConfig {
    pub default: TrackingDefault,
    pub default_remote: RemoteName,
    pub default_remote_prefix: Option<String>,
}

impl From<config::TrackingConfig> for TrackingConfig {
    fn from(other: config::TrackingConfig) -> Self {
        Self {
            default: if other.default {
                TrackingDefault::Track
            } else {
                TrackingDefault::NoTrack
            },
            default_remote: RemoteName::new(other.default_remote),
            default_remote_prefix: other.default_remote_prefix,
        }
    }
}

impl From<config::WorktreeRootConfig> for WorktreeRootConfig {
    fn from(other: config::WorktreeRootConfig) -> Self {
        Self {
            persistent_branches: other
                .persistent_branches
                .map(|branches| branches.into_iter().map(BranchName::new).collect()),
            track: other.track.map(Into::into),
        }
    }
}

pub struct WorktreeRepoHandle(super::RepoHandle);

impl WorktreeRepoHandle {
    pub fn open(path: &Path) -> Result<Self, super::Error> {
        Ok(Self(super::RepoHandle::open_with_worktree_setup(
            path,
            WorktreeSetup::Worktree,
        )?))
    }

    pub fn as_repo(&self) -> &super::RepoHandle {
        &self.0
    }

    fn base_directory(&self) -> Result<&Path, Error> {
        let commondir = self.0.commondir()?;
        commondir
            .parent()
            .ok_or_else(|| Error::InvalidBaseDirectory {
                git_dir: commondir.to_owned(),
            })
    }

    pub fn from_handle_unchecked(handle: super::RepoHandle) -> Self {
        Self(handle)
    }

    pub fn into_handle(self) -> super::RepoHandle {
        self.0
    }

    fn worktree_exists(&self, name: &WorktreeName) -> Result<bool, Error> {
        match self.0.0.find_worktree(name.as_str()) {
            Ok(_worktree) => Ok(true),
            Err(err) if err.code() == git2::ErrorCode::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    pub fn default_branch(&self) -> Result<Branch<'_>, Error> {
        Ok(self.0.default_branch()?)
    }

    fn find_local_branch(&self, name: &BranchName) -> Result<Option<Branch<'_>>, Error> {
        Ok(self.0.find_local_branch(name)?)
    }

    pub fn cleanup_worktrees(
        &self,
        directory: &Path,
        deletion_notify_channel: &mpsc::SyncSender<WorktreeName>,
    ) -> Result<Vec<CleanupWorktreeWarning>, CleanupWorktreeError> {
        let mut warnings = Vec::new();

        let worktrees = self.get_worktrees()?;

        let config: Option<WorktreeRootConfig> = config::read_worktree_root_config(directory)
            .map_err(|e| <config::Error as Into<Error>>::into(e))?
            .map(Into::into);

        let default_branch = match config {
            None => self.default_branch()?,
            Some(ref config) => match config.persistent_branches.as_ref() {
                None => self.default_branch()?,
                Some(persistent_branches) => {
                    if let Some(branch) = persistent_branches.first() {
                        self.find_local_branch(branch)?.ok_or_else(|| {
                            CleanupWorktreeError::BranchNotFound {
                                branch_name: branch.to_owned(),
                            }
                        })?
                    } else {
                        self.default_branch()?
                    }
                }
            },
        };

        let default_branch_name = default_branch
            .name()
            .map_err(|err| CleanupWorktreeError::BranchName(err))?;

        for worktree in worktrees
            .into_iter()
            .filter(|worktree| worktree.name().as_str() != default_branch_name.as_str())
            .filter(|worktree| match config {
                None => true,
                Some(ref config) => match config.persistent_branches.as_ref() {
                    None => true,
                    Some(branches) => !branches
                        .iter()
                        .any(|branch| branch.as_str() == worktree.name().as_str()),
                },
            })
        {
            let repo_dir = &directory.join(worktree.name().as_str());
            if repo_dir.exists() {
                match self.remove_worktree(
                    directory,
                    worktree.name(),
                    Path::new(worktree.name().as_str()),
                    false,
                    config.as_ref(),
                    &default_branch,
                ) {
                    Ok(()) => {
                        #[expect(
                            clippy::missing_panics_doc,
                            reason = "this is a clear bug, cannot be recovered anyway"
                        )]
                        deletion_notify_channel
                            .send(worktree.into_name())
                            .expect("receiving channel must be open until we are done");
                    }
                    Err(error) => match error {
                        WorktreeRemoveError::Changes(ref changes) => {
                            warnings.push(CleanupWorktreeWarning {
                                worktree_name: worktree.name().to_owned(),
                                reason: CleanupWorktreeWarningReason::UncommittedChanges(*changes),
                            });
                        }
                        WorktreeRemoveError::NotMerged { branch_name } => {
                            warnings.push(CleanupWorktreeWarning {
                                worktree_name: worktree.name().to_owned(),
                                reason: CleanupWorktreeWarningReason::NotMerged { branch_name },
                            });
                        }
                        _ => return Err(CleanupWorktreeError::RemoveError(error)),
                    },
                }
            } else {
                warnings.push(CleanupWorktreeWarning {
                    worktree_name: worktree.name().to_owned(),
                    reason: CleanupWorktreeWarningReason::NoDirectory,
                });
            }
        }
        Ok(warnings)
    }

    pub fn find_unmanaged_worktrees(&self, directory: &Path) -> Result<Vec<PathBuf>, Error> {
        let worktrees = self.get_worktrees()?;

        let mut unmanaged_worktrees = Vec::new();
        for entry in directory.read_dir_utf8()? {
            let entry = entry?;
            #[expect(clippy::missing_panics_doc, reason = "see expect() message")]
            let dirname = entry
                .path()
                .strip_prefix(directory)
                // that unwrap() is safe as each entry is
                // guaranteed to be a subentry of &directory
                .expect("each entry is guaranteed to have the prefix");

            let config: Option<WorktreeRootConfig> =
                config::read_worktree_root_config(directory)?.map(Into::into);

            let guess_default_branch = || {
                self.0
                    .default_branch()
                    .map_err(|error| format!("Failed getting default branch: {error}"))?
                    .name()
                    .map_err(|error| format!("Failed getting default branch name: {error}"))
            };

            let default_branch_name = match config {
                None => guess_default_branch().ok(),
                Some(ref config) => match config.persistent_branches.as_ref() {
                    None => guess_default_branch().ok(),
                    Some(persistent_branches) => {
                        if let Some(branch) = persistent_branches.first() {
                            Some(branch.clone())
                        } else {
                            guess_default_branch().ok()
                        }
                    }
                },
            };

            if dirname == GIT_MAIN_WORKTREE_DIRECTORY {
                continue;
            }

            if dirname == config::WORKTREE_CONFIG_FILE_NAME {
                continue;
            }
            if let Some(default_branch_name) = default_branch_name {
                if dirname == default_branch_name.as_str() {
                    continue;
                }
            }
            if !&worktrees
                .iter()
                .any(|worktree| worktree.name().as_str() == dirname)
            {
                unmanaged_worktrees.push(PathBuf::from(dirname));
            }
        }
        Ok(unmanaged_worktrees)
    }

    pub fn get_worktrees(&self) -> Result<Vec<Worktree>, Error> {
        Ok(self
            .0
            .0
            .worktrees()?
            .iter()
            .map(|remote| remote.ok_or(Error::WorktreeNameNotUtf8))
            .collect::<Result<Vec<_>, Error>>()?
            .into_iter()
            .map(Worktree::new)
            .collect::<Result<Vec<_>, WorktreeValidationError>>()?)
    }

    pub fn remove_worktree(
        &self,
        base_dir: &Path,
        worktree_name: &WorktreeName,
        worktree_dir: &Path,
        force: bool,
        worktree_config: Option<&WorktreeRootConfig>,
        default_branch: &Branch,
    ) -> Result<(), WorktreeRemoveError> {
        //! We remove the worktree only under the following conditions (unless `force` is given):
        //!
        //! * It has no changes
        //! * If it has a remote tracking branch, it does not differ
        //! * It is merged into the default branch
        //! * It is merged into any "persistent branch"
        //!
        //! To clarify: Even if it is merged into local persistent branches, it will still not be
        //! deleted when the remote branch differs

        let fullpath = base_dir.join(worktree_dir);

        if !fullpath.exists() {
            return Err(WorktreeRemoveError::DoesNotExist(fullpath));
        }
        let worktree_repo = RepoHandle::open(&fullpath)?;

        let local_branch = worktree_repo.head_branch()?;

        let branch_name = local_branch.name()?;

        if branch_name.as_str() != worktree_name.as_str() {
            return Err(WorktreeRemoveError::BranchNameMismatch {
                worktree_name: worktree_name.clone(),
                branch_name,
            });
        }

        let branch = worktree_repo
            .find_local_branch(&branch_name)?
            .ok_or_else(|| WorktreeRemoveError::BranchNotFound(branch_name.clone()))?;

        if !force {
            let status = worktree_repo.status(WorktreeSetup::NoWorktree)?;

            if let Some(changes) = status.changes {
                return Err(WorktreeRemoveError::Changes(changes));
            }

            let is_merged_into_default_branch = {
                let (ahead_of_default_branch, _behind) =
                    worktree_repo.graph_ahead_behind(&branch, default_branch)?;

                ahead_of_default_branch == 0
            };

            let mut is_merged_into_persistent_branch = false;
            let mut has_persistent_branches = false;
            if let Some(config) = worktree_config {
                if let Some(branches) = config.persistent_branches.as_ref() {
                    has_persistent_branches = true;
                    for persistent_branch in branches {
                        let persistent_branch = worktree_repo
                            .find_local_branch(persistent_branch)?
                            .ok_or_else(|| {
                                WorktreeRemoveError::BranchNotFound(branch_name.clone())
                            })?;

                        let (ahead, _behind) =
                            worktree_repo.graph_ahead_behind(&branch, &persistent_branch)?;

                        if ahead == 0 {
                            is_merged_into_persistent_branch = true;
                        }
                    }
                }
            }

            let merged_into_default_or_persistent_branches = is_merged_into_default_branch
                || (has_persistent_branches && is_merged_into_persistent_branch);

            if !merged_into_default_or_persistent_branches {
                return Err(WorktreeRemoveError::NotMerged { branch_name });
            }

            if let Some(remote_branch) = branch.upstream()? {
                let (ahead, behind) = worktree_repo.graph_ahead_behind(&branch, &remote_branch)?;

                if (ahead, behind) != (0, 0) {
                    return Err(WorktreeRemoveError::NotInSyncWithRemote { branch_name });
                }
            }
        }

        // worktree_dir is a relative path, starting from base_dir. We walk it
        // upwards (from subdirectory to parent directories) and remove each
        // component, in case it is empty. Only the leaf directory can be
        // removed unconditionally (as it contains the worktree itself).
        if let Err(e) = std::fs::remove_dir_all(&fullpath) {
            return Err(WorktreeRemoveError::RemoveError {
                path: fullpath,
                error: e,
            });
        }

        if let Some(current_dir) = worktree_dir.parent() {
            for current_dir in current_dir.ancestors() {
                let current_dir = base_dir.join(current_dir);
                if current_dir
                    .read_dir()
                    .map_err(|error| WorktreeRemoveError::ReadDirectoryError {
                        path: current_dir.clone(),
                        error,
                    })?
                    .next()
                    .is_none()
                {
                    if let Err(e) = std::fs::remove_dir(&current_dir) {
                        return Err(WorktreeRemoveError::RemoveError {
                            path: current_dir,
                            error: e,
                        });
                    }
                } else {
                    break;
                }
            }
        }

        self.0.prune_worktree(worktree_name)?;
        branch.delete()?;

        Ok(())
    }

    fn new_worktree(
        &self,
        name: &str,
        directory: &Path,
        target_branch: &Branch,
    ) -> Result<(), Error> {
        self.0.0.worktree(
            name,
            directory.as_std_path(),
            Some(git2::WorktreeAddOptions::new().reference(Some(target_branch.as_reference()))),
        )?;
        Ok(())
    }

    pub fn add_worktree(
        &self,
        name: &WorktreeName,
        tracking_selection: TrackingSelection,
    ) -> Result<Vec<Warning>, Error> {
        let mut warnings: Vec<Warning> = vec![];

        let repo_directory = self.base_directory()?;

        let remotes = self.as_repo().remotes()?;

        let config: Option<WorktreeRootConfig> =
            config::read_worktree_root_config(repo_directory)?.map(Into::into);

        if self.worktree_exists(name)? {
            return Err(Error::WorktreeAlreadyExists { name: name.clone() });
        }

        let track_config = config.and_then(|config| config.track);
        let prefix = track_config
            .as_ref()
            .and_then(|track| track.default_remote_prefix.as_ref());

        let default_tracking = track_config
            .as_ref()
            .map_or(TrackingDefault::NoTrack, |track| track.default);

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
        let default_branch_head =
            || Ok::<_, Error>(self.as_repo().default_branch()?.commit_owned()?);

        let worktree = NewWorktree::<Init>::new(self)
            .set_local_branch_name(&BranchName::new(name.as_str().to_owned()))?;

        let get_remote_head = |remote_name: &RemoteName,
                               remote_branch_name: &BranchName|
         -> Result<Option<repo::Commit>, Error> {
            Ok(self
                .as_repo()
                .find_remote_branch(remote_name, remote_branch_name)?
                .map(|branch| branch.commit_owned())
                .transpose()?)
        };

        let worktree = if worktree.local_branch_already_exists() {
            worktree.select_commit(None)
        } else {
            match tracking_selection {
                TrackingSelection::Explicit {
                    ref remote_name,
                    ref remote_branch_name,
                } => worktree.select_commit(Some(
                    self.as_repo()
                        .find_remote_branch(remote_name, remote_branch_name)?
                        .map_or_else(
                            || default_branch_head(),
                            |remote_branch| Ok(remote_branch.commit_owned()?),
                        )?,
                )),
                TrackingSelection::Automatic | TrackingSelection::Disabled => {
                    match remotes.len() {
                        0 => worktree.select_commit(Some(default_branch_head()?)),
                        1 => {
                            #[expect(
                                clippy::indexing_slicing,
                                reason = "checked for len() explicitly"
                            )]
                            let remote_name = &remotes[0];

                            let commit: Option<repo::Commit> = prefix
                                .map(|prefix| {
                                    get_remote_head(
                                        remote_name,
                                        &BranchName::new(format!("{prefix}/{name}")),
                                    )
                                    .transpose()
                                })
                                .flatten()
                                .or_else(|| {
                                    get_remote_head(
                                        remote_name,
                                        &BranchName::new(name.as_str().to_owned()),
                                    )
                                    .transpose()
                                })
                                .or_else(|| Some(default_branch_head()))
                                .transpose()?;

                            worktree.select_commit(commit)
                        }
                        _ => {
                            let x = prefix
                                .map(|prefix| {
                                    Ok(self
                                        .as_repo()
                                        .find_remote_branch(
                                            default_remote.as_ref().unwrap(),
                                            &BranchName::new(format!("{prefix}/{name}")),
                                        )?
                                        .map(|remote_branch| remote_branch.commit_owned()))
                                })
                                .transpose()
                                .map(Option::flatten)
                                .transpose()
                                .map(Result::flatten)
                                .or({
                                    self.as_repo()
                                        .find_remote_branch(
                                            default_remote.as_ref().unwrap(),
                                            &BranchName::new(name.as_str().to_owned()),
                                        )?
                                        .map(|remote_branch| remote_branch.commit_owned())
                                });

                            // let commit = if let Some(ref default_remote) = default_remote {
                            //     x
                            // } else {
                            //     None
                            // }
                            // .transpose()?;

                            let commit = default_remote
                                .map(|default_remote| {
                                    let x = x;
                                    x
                                })
                                .flatten();

                            let commit = commit.or( {
                                let mut commits = vec![];
                                for remote_name in &remotes {
                                    let remote_head: Option<repo::Commit> = ({
                                        if let Some(prefix) = prefix {
                                            self.as_repo().find_remote_branch(
                                                remote_name,
                                                &BranchName::new(format!("{prefix}/{name}")),
                                            )?.map(|remote_branch| remote_branch.commit_owned()).transpose()?
                                        } else {
                                            None
                                        }
                                    })
                                    .or({
                                        self.as_repo().find_remote_branch(remote_name, &BranchName::new(name.as_str().to_owned()))?.map(|remote_branch|remote_branch.commit_owned()).transpose()?
                                    })
                                    .or(None);
                                    commits.push(remote_head);
                                }

                                let mut commits = commits
                                    .into_iter()
                                    .flatten()
                                    // have to collect first because the `flatten()` return
                                    // type does not implement `windows()`
                                    .collect::<Vec<repo::Commit>>();
                                // `flatten()` takes care of `None` values here. If all
                                // remotes return None for the branch, we do *not* abort, we
                                // continue!
                                if commits.is_empty() {
                                    Some(default_branch_head()?)
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
                                        Warning("Branch exists on multiple remotes, but they deviate. Selecting default branch instead".to_owned())
                                    );
                                    Some(default_branch_head()?)
                                } else {
                                    Some(commits.swap_remove(0))
                                }
                            });
                            worktree.select_commit(commit)
                        }
                    }
                }
            }
        };

        let worktree = worktree.set_remote_tracking_branch(match tracking_selection {
            TrackingSelection::Disabled => None,
            TrackingSelection::Explicit {
                remote_name,
                remote_branch_name,
            } => {
                Some(RemoteTrackingBranch {
                    remote_name,
                    remote_branch_name,
                    prefix: None, // Always disable prefixing when explicitly given --track
                })
            }
            TrackingSelection::Automatic => {
                if default_tracking == TrackingDefault::NoTrack {
                    None
                } else {
                    match remotes.len() {
                        0 => None,
                        1 =>
                        {
                            #[expect(
                                clippy::indexing_slicing,
                                reason = "checked for len() explicitly"
                            )]
                            Some(RemoteTrackingBranch {
                                remote_name: remotes[0].clone(),
                                remote_branch_name: BranchName::new(name.as_str().to_owned()),
                                prefix: prefix.cloned(),
                            })
                        }
                        _ => default_remote.map(|default_remote| RemoteTrackingBranch {
                            remote_name: default_remote,
                            remote_branch_name: BranchName::new(name.as_str().to_owned()),
                            prefix: prefix.cloned(),
                        }),
                    }
                }
            }
        });

        worktree.create(repo_directory)?;

        Ok(warnings)
    }
}
