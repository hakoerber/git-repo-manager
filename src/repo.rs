use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

use git2::Repository;

use super::output::*;
use super::path;
use super::worktree;

const WORKTREE_CONFIG_FILE_NAME: &str = "grm.toml";
const GIT_CONFIG_BARE_KEY: &str = "core.bare";
const GIT_CONFIG_PUSH_DEFAULT: &str = "push.default";

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteType {
    Ssh,
    Https,
    File,
}

#[derive(Debug, Error)]
pub enum WorktreeRemoveFailureReason {
    #[error("Changes found")]
    Changes(String),
    #[error("{}", .0)]
    Error(String),
    #[error("Worktree is not merged")]
    NotMerged(String),
}

#[derive(Debug, Error)]
pub enum WorktreeConversionFailureReason {
    #[error("Changes found")]
    Changes,
    #[error("Ignored files found")]
    Ignored,
    #[error("{}", .0)]
    Error(String),
}

#[derive(Copy, Clone)]
pub enum GitPushDefaultSetting {
    Upstream,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackingConfig {
    pub default: bool,
    pub default_remote: String,
    pub default_remote_prefix: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorktreeRootConfig {
    pub persistent_branches: Option<Vec<String>>,

    pub track: Option<TrackingConfig>,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Error reading configuration file \"{}\": {}", .path, .message)]
    ReadConfig { message: String, path: String },
    #[error("Error parsing configuration file \"{}\": {}", .path, .message)]
    ParseConfig { message: String, path: String },
    #[error(transparent)]
    Libgit(#[from] git2::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Repository not found")]
    NotFound,
    #[error("Could not determine default branch")]
    NoDefaultBranch,
    #[error("Failed getting default branch name: {}", .message)]
    ErrorDefaultBranch { message: String },
    #[error("Remotes using HTTP protocol are not supported")]
    UnsupportedHttpRemote,
    #[error("Remotes using git protocol are not supported")]
    UnsupportedGitRemote,
    #[error("The remote URL starts with an unimplemented protocol")]
    UnimplementedRemoteProtocol,
    #[error("Some non-default refspecs could not be renamed")]
    RefspecRenameFailed,
    #[error("No branch checked out")]
    NoBranchCheckedOut,
    #[error("Remote name is invalid utf-8")]
    RemoteNameInvalidUtf8,
    #[error("Could not set {key}: {error}", key = .key, error = .error)]
    GitConfigSetError { key: String, error: String },
    #[error(transparent)]
    WorktreeConversionFailure(WorktreeConversionFailureReason),
    #[error(transparent)]
    WorktreeRemovalFailure(WorktreeRemoveFailureReason),
    #[error("Cannot get changes as this is a bare worktree repository")]
    GettingChangesFromBareWorktree,
    #[error("Trying to push to a non-pushable remote")]
    NonPushableRemote,
    #[error(
        "Pushing {} to {} ({}) failed: {}",
        .local_branch,
        .remote_name,
        .remote_url,
        .message
    )]
    PushFailed {
        local_branch: String,
        remote_name: String,
        remote_url: String,
        message: String,
    },
}

pub fn read_worktree_root_config(
    worktree_root: &Path,
) -> Result<Option<WorktreeRootConfig>, Error> {
    let path = worktree_root.join(WORKTREE_CONFIG_FILE_NAME);
    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => return Ok(None),
            _ => {
                return Err(Error::ReadConfig {
                    message: e.to_string(),
                    path: path.display().to_string(),
                })
            }
        },
    };

    let config: WorktreeRootConfig = match toml::from_str(&content) {
        Ok(c) => c,
        Err(e) => {
            return Err(Error::ParseConfig {
                message: e.to_string(),
                path: path.display().to_string(),
            })
        }
    };

    Ok(Some(config))
}

#[derive(Debug)]
pub struct Remote {
    pub name: String,
    pub url: String,
    pub remote_type: RemoteType,
}

#[derive(Debug)]
pub struct Repo {
    pub name: String,
    pub namespace: Option<String>,
    pub worktree_setup: bool,
    pub remotes: Option<Vec<Remote>>,
}

impl Repo {
    pub fn fullname(&self) -> String {
        match &self.namespace {
            Some(namespace) => format!("{}/{}", namespace, self.name),
            None => self.name.clone(),
        }
    }

    pub fn remove_namespace(&mut self) {
        self.namespace = None;
    }
}

pub struct RepoChanges {
    pub files_new: usize,
    pub files_modified: usize,
    pub files_deleted: usize,
}

pub enum SubmoduleStatus {
    Clean,
    Uninitialized,
    Changed,
    OutOfDate,
}

pub enum RemoteTrackingStatus {
    UpToDate,
    Ahead(usize),
    Behind(usize),
    Diverged(usize, usize),
}

pub struct RepoStatus {
    pub operation: Option<git2::RepositoryState>,

    pub empty: bool,

    pub remotes: Vec<String>,

    pub head: Option<String>,

    pub changes: Option<RepoChanges>,

    pub worktrees: usize,

    pub submodules: Option<Vec<(String, SubmoduleStatus)>>,

    pub branches: Vec<(String, Option<(String, RemoteTrackingStatus)>)>,
}

pub struct Worktree {
    name: String,
}

impl Worktree {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn forward_branch(&self, rebase: bool, stash: bool) -> Result<Option<String>, Error> {
        let repo = RepoHandle::open(Path::new(&self.name), false)?;

        if let Ok(remote_branch) = repo.find_local_branch(&self.name)?.upstream() {
            let status = repo.status(false)?;
            let mut stashed_changes = false;

            if !status.clean() {
                if stash {
                    repo.stash()?;
                    stashed_changes = true;
                } else {
                    return Ok(Some(String::from("Worktree contains changes")));
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
                    index.add_all(["."].iter(), git2::IndexAddOption::CHECK_PATHSPEC, None)?;

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
                    return Ok(Some(String::from("Worktree cannot be fast forwarded")));
                }

                repo.0.reset(
                    remote_branch.commit()?.0.as_object(),
                    git2::ResetType::Hard,
                    Some(git2::build::CheckoutBuilder::new().safe()),
                )?;
            }
            unstash()?;
        } else {
            return Ok(Some(String::from("No remote branch to rebase onto")));
        };

        Ok(None)
    }

    pub fn rebase_onto_default(
        &self,
        config: &Option<WorktreeRootConfig>,
        stash: bool,
    ) -> Result<Option<String>, Error> {
        let repo = RepoHandle::open(Path::new(&self.name), false)?;

        let guess_default_branch = || repo.default_branch()?.name();

        let default_branch_name = match &config {
            None => guess_default_branch()?,
            Some(config) => match &config.persistent_branches {
                None => guess_default_branch()?,
                Some(persistent_branches) => {
                    if persistent_branches.is_empty() {
                        guess_default_branch()?
                    } else {
                        persistent_branches[0].clone()
                    }
                }
            },
        };

        let status = repo.status(false)?;
        let mut stashed_changes = false;

        if !status.clean() {
            if stash {
                repo.stash()?;
                stashed_changes = true;
            } else {
                return Ok(Some(String::from("Worktree contains changes")));
            }
        }

        let unstash = || -> Result<(), Error> {
            if stashed_changes {
                repo.stash_pop()?;
            }
            Ok(())
        };

        let base_branch = repo.find_local_branch(&default_branch_name)?;
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
            index.add_all(["."].iter(), git2::IndexAddOption::CHECK_PATHSPEC, None)?;

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

impl RepoStatus {
    fn clean(&self) -> bool {
        match &self.changes {
            None => true,
            Some(changes) => {
                changes.files_new == 0 && changes.files_deleted == 0 && changes.files_modified == 0
            }
        }
    }
}

pub fn detect_remote_type(remote_url: &str) -> Result<RemoteType, Error> {
    let git_regex = regex::Regex::new(r"^[a-zA-Z]+@.*$").unwrap();
    if remote_url.starts_with("ssh://") {
        return Ok(RemoteType::Ssh);
    }
    if git_regex.is_match(remote_url) && remote_url.ends_with(".git") {
        return Ok(RemoteType::Ssh);
    }
    if remote_url.starts_with("https://") {
        return Ok(RemoteType::Https);
    }
    if remote_url.starts_with("file://") {
        return Ok(RemoteType::File);
    }
    if remote_url.starts_with("http://") {
        return Err(Error::UnsupportedHttpRemote);
    }
    if remote_url.starts_with("git://") {
        return Err(Error::UnsupportedGitRemote);
    }
    Err(Error::UnimplementedRemoteProtocol)
}

pub struct RepoHandle(git2::Repository);
pub struct Branch<'a>(git2::Branch<'a>);

impl RepoHandle {
    pub fn open(path: &Path, is_worktree: bool) -> Result<Self, Error> {
        let open_func = match is_worktree {
            true => Repository::open_bare,
            false => Repository::open,
        };
        let path = if is_worktree {
            path.join(worktree::GIT_MAIN_WORKTREE_DIRECTORY)
        } else {
            path.to_path_buf()
        };
        match open_func(path) {
            Ok(r) => Ok(Self(r)),
            Err(e) => match e.code() {
                git2::ErrorCode::NotFound => Err(Error::NotFound),
                _ => Err(Error::Libgit(e)),
            },
        }
    }

    pub fn stash(&self) -> Result<(), Error> {
        let head_branch = self.head_branch()?;
        let head = head_branch.commit()?;
        let author = head.author();

        // This is honestly quite horrible. The problem is that all stash operations expect a
        // mutable reference (as they, well, mutate the repo after all). But we are heavily using
        // immutable references a lot with this struct. I'm really not sure how to best solve this.
        // Right now, we just open the repo AGAIN. It is safe, as we are only accessing the stash
        // with the second reference, so there are no cross effects. But it just smells. Also,
        // using `unwrap()` here as we are already sure that the repo is openable(?).
        let mut repo = Self::open(self.0.path(), false).unwrap();
        repo.0
            .stash_save2(&author, None, Some(git2::StashFlags::INCLUDE_UNTRACKED))?;
        Ok(())
    }

    pub fn stash_pop(&self) -> Result<(), Error> {
        let mut repo = RepoHandle::open(self.0.path(), false).unwrap();
        repo.0.stash_pop(
            0,
            Some(git2::StashApplyOptions::new().reinstantiate_index()),
        )?;
        Ok(())
    }

    pub fn rename_remote(&self, remote: &RemoteHandle, new_name: &str) -> Result<(), Error> {
        let failed_refspecs = self.0.remote_rename(&remote.name(), new_name)?;

        if !failed_refspecs.is_empty() {
            return Err(Error::RefspecRenameFailed);
        }

        Ok(())
    }

    pub fn graph_ahead_behind(
        &self,
        local_branch: &Branch,
        remote_branch: &Branch,
    ) -> Result<(usize, usize), Error> {
        Ok(self.0.graph_ahead_behind(
            local_branch.commit()?.id().0,
            remote_branch.commit()?.id().0,
        )?)
    }

    pub fn head_branch(&self) -> Result<Branch<'_>, Error> {
        let head = self.0.head()?;
        if !head.is_branch() {
            return Err(Error::NoBranchCheckedOut);
        }
        // unwrap() is safe here, as we can be certain that a branch with that
        // name exists
        let branch = self
            .find_local_branch(head.shorthand().expect("Branch name is not valid utf-8"))
            .unwrap();
        Ok(branch)
    }

    pub fn remote_set_url(&self, name: &str, url: &str) -> Result<(), Error> {
        Ok(self.0.remote_set_url(name, url)?)
    }

    pub fn remote_delete(&self, name: &str) -> Result<(), Error> {
        Ok(self.0.remote_delete(name)?)
    }

    pub fn is_empty(&self) -> Result<bool, Error> {
        Ok(self.0.is_empty()?)
    }

    pub fn is_bare(&self) -> bool {
        self.0.is_bare()
    }

    pub fn new_worktree(
        &self,
        name: &str,
        directory: &Path,
        target_branch: &Branch,
    ) -> Result<(), Error> {
        self.0.worktree(
            name,
            directory,
            Some(git2::WorktreeAddOptions::new().reference(Some(target_branch.as_reference()))),
        )?;
        Ok(())
    }

    pub fn remotes(&self) -> Result<Vec<String>, Error> {
        Ok(self
            .0
            .remotes()?
            .iter()
            .map(|name| name.expect("Remote name is invalid utf-8"))
            .map(|name| name.to_owned())
            .collect())
    }

    pub fn new_remote(&self, name: &str, url: &str) -> Result<(), Error> {
        self.0.remote(name, url)?;
        Ok(())
    }

    pub fn fetchall(&self) -> Result<(), Error> {
        for remote in self.remotes()? {
            self.fetch(&remote)?;
        }
        Ok(())
    }

    pub fn local_branches(&self) -> Result<Vec<Branch<'_>>, Error> {
        self.0
            .branches(Some(git2::BranchType::Local))?
            .map(|branch| Ok(Branch(branch?.0)))
            .collect::<Result<Vec<Branch>, Error>>()
    }

    pub fn remote_branches(&self) -> Result<Vec<Branch<'_>>, Error> {
        self.0
            .branches(Some(git2::BranchType::Remote))?
            .map(|branch| Ok(Branch(branch?.0)))
            .collect::<Result<Vec<Branch>, Error>>()
    }

    pub fn fetch(&self, remote_name: &str) -> Result<(), Error> {
        let mut remote = self.0.find_remote(remote_name)?;

        let mut fetch_options = git2::FetchOptions::new();
        fetch_options.remote_callbacks(get_remote_callbacks());

        for refspec in &remote.fetch_refspecs()? {
            remote.fetch(
                &[refspec.ok_or(Error::RemoteNameInvalidUtf8)?],
                Some(&mut fetch_options),
                None,
            )?;
        }
        Ok(())
    }

    pub fn init(path: &Path, is_worktree: bool) -> Result<Self, Error> {
        let repo = match is_worktree {
            false => Repository::init(path)?,
            true => Repository::init_bare(path.join(worktree::GIT_MAIN_WORKTREE_DIRECTORY))?,
        };

        let repo = Self(repo);

        if is_worktree {
            repo.set_config_push(GitPushDefaultSetting::Upstream)?;
        }

        Ok(repo)
    }

    pub fn config(&self) -> Result<git2::Config, Error> {
        Ok(self.0.config()?)
    }

    pub fn find_worktree(&self, name: &str) -> Result<(), Error> {
        self.0.find_worktree(name)?;
        Ok(())
    }

    pub fn prune_worktree(&self, name: &str) -> Result<(), Error> {
        let worktree = self.0.find_worktree(name)?;
        worktree.prune(None)?;
        Ok(())
    }

    pub fn find_remote_branch(
        &self,
        remote_name: &str,
        branch_name: &str,
    ) -> Result<Branch<'_>, Error> {
        Ok(Branch(self.0.find_branch(
            &format!("{remote_name}/{branch_name}"),
            git2::BranchType::Remote,
        )?))
    }

    pub fn find_local_branch(&self, name: &str) -> Result<Branch<'_>, Error> {
        Ok(Branch(self.0.find_branch(name, git2::BranchType::Local)?))
    }

    pub fn create_branch(&self, name: &str, target: &Commit) -> Result<Branch<'_>, Error> {
        Ok(Branch(self.0.branch(name, &target.0, false)?))
    }

    pub fn make_bare(&self, value: bool) -> Result<(), Error> {
        let mut config = self.config()?;

        config
            .set_bool(GIT_CONFIG_BARE_KEY, value)
            .map_err(|error| Error::GitConfigSetError {
                key: GIT_CONFIG_BARE_KEY.to_owned(),
                error: error.to_string(),
            })
    }

    pub fn convert_to_worktree(&self, root_dir: &Path) -> Result<(), Error> {
        if self
            .status(false)
            .map_err(|e| {
                Error::WorktreeConversionFailure(WorktreeConversionFailureReason::Error(
                    e.to_string(),
                ))
            })?
            .changes
            .is_some()
        {
            return Err(Error::WorktreeConversionFailure(
                WorktreeConversionFailureReason::Changes,
            ));
        }

        if self.has_untracked_files(false).map_err(|e| {
            Error::WorktreeConversionFailure(WorktreeConversionFailureReason::Error(e.to_string()))
        })? {
            return Err(Error::WorktreeConversionFailure(
                WorktreeConversionFailureReason::Ignored,
            ));
        }

        std::fs::rename(".git", worktree::GIT_MAIN_WORKTREE_DIRECTORY).map_err(|error| {
            Error::WorktreeConversionFailure(WorktreeConversionFailureReason::Error(format!(
                "Error moving .git directory: {error}"
            )))
        })?;

        for entry in match std::fs::read_dir(root_dir) {
            Ok(iterator) => iterator,
            Err(error) => {
                return Err(Error::WorktreeConversionFailure(
                    WorktreeConversionFailureReason::Error(format!(
                        "Opening directory failed: {error}"
                    )),
                ));
            }
        } {
            match entry {
                Ok(entry) => {
                    let path = entry.path();
                    // unwrap is safe here, the path will ALWAYS have a file component
                    if path.file_name().unwrap() == worktree::GIT_MAIN_WORKTREE_DIRECTORY {
                        continue;
                    }
                    if path.is_file() || path.is_symlink() {
                        if let Err(error) = std::fs::remove_file(&path) {
                            return Err(Error::WorktreeConversionFailure(
                                WorktreeConversionFailureReason::Error(format!(
                                    "Failed removing {error}"
                                )),
                            ));
                        }
                    } else if let Err(error) = std::fs::remove_dir_all(&path) {
                        return Err(Error::WorktreeConversionFailure(
                            WorktreeConversionFailureReason::Error(format!(
                                "Failed removing {error}"
                            )),
                        ));
                    }
                }
                Err(error) => {
                    return Err(Error::WorktreeConversionFailure(
                        WorktreeConversionFailureReason::Error(format!(
                            "Error getting directory entry: {error}"
                        )),
                    ));
                }
            }
        }

        let worktree_repo = RepoHandle::open(root_dir, true).map_err(|error| {
            Error::WorktreeConversionFailure(WorktreeConversionFailureReason::Error(format!(
                "Opening newly converted repository failed: {error}"
            )))
        })?;

        worktree_repo.make_bare(true).map_err(|error| {
            Error::WorktreeConversionFailure(WorktreeConversionFailureReason::Error(format!(
                "Error: {error}"
            )))
        })?;

        worktree_repo
            .set_config_push(GitPushDefaultSetting::Upstream)
            .map_err(|error| {
                Error::WorktreeConversionFailure(WorktreeConversionFailureReason::Error(format!(
                    "Error: {error}"
                )))
            })?;

        Ok(())
    }

    pub fn set_config_push(&self, value: GitPushDefaultSetting) -> Result<(), Error> {
        let mut config = self.config()?;

        config
            .set_str(
                GIT_CONFIG_PUSH_DEFAULT,
                match value {
                    GitPushDefaultSetting::Upstream => "upstream",
                },
            )
            .map_err(|error| Error::GitConfigSetError {
                key: GIT_CONFIG_BARE_KEY.to_owned(),
                error: error.to_string(),
            })
    }

    pub fn has_untracked_files(&self, is_worktree: bool) -> Result<bool, Error> {
        match is_worktree {
            true => Err(Error::GettingChangesFromBareWorktree),
            false => {
                let statuses = self
                    .0
                    .statuses(Some(git2::StatusOptions::new().include_ignored(true)))?;

            for status in statuses.iter() {
                let status_bits = status.status();
                if status_bits.intersects(git2::Status::IGNORED) {
                    return Ok(true);
                }
            }

            Ok(false)
        }
    }

    pub fn status(&self, is_worktree: bool) -> Result<RepoStatus, Error> {
        let operation = match self.0.state() {
            git2::RepositoryState::Clean => None,
            state => Some(state),
        };

        let empty = self.is_empty()?;

        let remotes = self
            .0
            .remotes()?
            .iter()
            .map(|repo_name| repo_name.expect("Worktree name is invalid utf-8."))
            .map(|repo_name| repo_name.to_owned())
            .collect::<Vec<String>>();

        let head = if is_worktree || empty {
            None
        } else {
            Some(self.head_branch()?.name()?)
        };

        let changes = match is_worktree {
            true => None,
            false => {
                let statuses = self.0.statuses(Some(
                    git2::StatusOptions::new()
                        .include_ignored(false)
                        .include_untracked(true),
                ))?;

            if statuses.is_empty() {
                None
            } else {
                let mut files_new = 0;
                let mut files_modified = 0;
                let mut files_deleted = 0;
                for status in statuses.iter() {
                    let status_bits = status.status();
                    if status_bits.intersects(
                        git2::Status::INDEX_MODIFIED
                            | git2::Status::INDEX_RENAMED
                            | git2::Status::INDEX_TYPECHANGE
                            | git2::Status::WT_MODIFIED
                            | git2::Status::WT_RENAMED
                            | git2::Status::WT_TYPECHANGE,
                    ) {
                        files_modified += 1;
                    } else if status_bits.intersects(git2::Status::INDEX_NEW | git2::Status::WT_NEW)
                    {
                        files_new += 1;
                    } else if status_bits
                        .intersects(git2::Status::INDEX_DELETED | git2::Status::WT_DELETED)
                    {
                        files_deleted += 1;
                    }
                }
                assert!(
                    (files_new, files_modified, files_deleted) != (0, 0, 0),
                    "is_empty() returned true, but no file changes were detected. This is a bug!"
                );
                Some(RepoChanges {
                    files_new,
                    files_modified,
                    files_deleted,
                })
            }
        };

        let worktrees = self.0.worktrees().unwrap().len();

        let submodules = if is_worktree {
            None
        } else {
            let mut submodules = Vec::new();
            for submodule in self.0.submodules().unwrap() {
                let submodule_name = submodule.name().unwrap().to_string();

                let submodule_status;
                let status = self
                    .0
                    .submodule_status(submodule.name().unwrap(), git2::SubmoduleIgnore::None)
                    .unwrap();

                if status.intersects(
                    git2::SubmoduleStatus::WD_INDEX_MODIFIED
                        | git2::SubmoduleStatus::WD_WD_MODIFIED
                        | git2::SubmoduleStatus::WD_UNTRACKED,
                ) {
                    submodule_status = SubmoduleStatus::Changed;
                } else if status.is_wd_uninitialized() {
                    submodule_status = SubmoduleStatus::Uninitialized;
                } else if status.is_wd_modified() {
                    submodule_status = SubmoduleStatus::OutOfDate;
                } else {
                    submodule_status = SubmoduleStatus::Clean;
                }

                submodules.push((submodule_name, submodule_status));
            }
            Some(submodules)
        };

        let mut branches = Vec::new();
        for (local_branch, _) in self
            .0
            .branches(Some(git2::BranchType::Local))
            .unwrap()
            .map(|branch_name| branch_name.unwrap())
        {
            let branch_name = local_branch.name().unwrap().unwrap().to_string();
            let remote_branch = match local_branch.upstream() {
                Ok(remote_branch) => {
                    let remote_branch_name = remote_branch.name().unwrap().unwrap().to_string();

                    let (ahead, behind) = self
                        .0
                        .graph_ahead_behind(
                            local_branch.get().peel_to_commit().unwrap().id(),
                            remote_branch.get().peel_to_commit().unwrap().id(),
                        )
                        .unwrap();

                    let remote_tracking_status = match (ahead, behind) {
                        (0, 0) => RemoteTrackingStatus::UpToDate,
                        (0, d) => RemoteTrackingStatus::Behind(d),
                        (d, 0) => RemoteTrackingStatus::Ahead(d),
                        (d1, d2) => RemoteTrackingStatus::Diverged(d1, d2),
                    };
                    Some((remote_branch_name, remote_tracking_status))
                }
                // Err => no remote branch
                Err(_) => None,
            };
            branches.push((branch_name, remote_branch));
        }

        Ok(RepoStatus {
            operation,
            empty,
            remotes,
            head,
            changes,
            worktrees,
            submodules,
            branches,
        })
    }

    pub fn get_remote_default_branch(
        &self,
        remote_name: &str,
    ) -> Result<Option<Branch<'_>>, Error> {
        // libgit2's `git_remote_default_branch()` and `Remote::default_branch()`
        // need an actual connection to the remote, so they may fail.
        if let Some(mut remote) = self.find_remote(remote_name)? {
            if remote.connected() {
                let remote = remote; // unmut
                if let Ok(remote_default_branch) = remote.default_branch() {
                    return Ok(Some(self.find_local_branch(&remote_default_branch)?));
                };
            }
        }

        // Note that <remote>/HEAD only exists after a normal clone, there is no way to get the
        // remote HEAD afterwards. So this is a "best effort" approach.
        if let Ok(remote_head) = self.find_remote_branch(remote_name, "HEAD") {
            if let Some(pointer_name) = remote_head.as_reference().symbolic_target() {
                if let Some(local_branch_name) =
                    pointer_name.strip_prefix(&format!("refs/remotes/{remote_name}/"))
                {
                    return Ok(Some(self.find_local_branch(local_branch_name)?));
                } else {
                    eprintln!("Remote HEAD ({pointer_name}) pointer is invalid");
                }
            } else {
                eprintln!("Remote HEAD does not point to a symbolic target");
            }
        }
        Ok(None)
    }

    pub fn default_branch(&self) -> Result<Branch<'_>, Error> {
        // This is a bit of a guessing game.
        //
        // In the best case, there is only one remote. Then, we can check <remote>/HEAD to get the
        // default remote branch.
        //
        // If there are multiple remotes, we first check whether they all have the same
        // <remote>/HEAD branch. If yes, good! If not, we use whatever "origin" uses, if that
        // exists. If it does not, there is no way to reliably get a remote default branch.
        //
        // In this case, we just try to guess a local branch from a list. If even that does not
        // work, well, bad luck.
        let remotes = self.remotes()?;

        if remotes.len() == 1 {
            let remote_name = &remotes[0];
            if let Some(default_branch) = self.get_remote_default_branch(remote_name)? {
                return Ok(default_branch);
            }
        } else {
            let mut default_branches: Vec<Branch> = vec![];
            for remote_name in remotes {
                if let Some(default_branch) = self.get_remote_default_branch(&remote_name)? {
                    default_branches.push(default_branch);
                }
            }

            if !default_branches.is_empty()
                && (default_branches.len() == 1
                    || default_branches
                        .iter()
                        .map(|branch| branch.name())
                        .collect::<Result<Vec<String>, Error>>()?
                        .windows(2)
                        .all(|branch_names| branch_names[0] == branch_names[1]))
            {
                return Ok(default_branches.remove(0));
            }
        }

        for branch_name in &["main", "master"] {
            if let Ok(branch) = self.0.find_branch(branch_name, git2::BranchType::Local) {
                return Ok(Branch(branch));
            }
        }

        Err(Error::NoDefaultBranch)
    }

    // Looks like there is no distinguishing between the error cases
    // "no such remote" and "failed to get remote for some reason".
    // May be a good idea to handle this explicitly, by returning a
    // Result<Option<RemoteHandle>, Error> instead, Returning Ok(None)
    // on "not found" and Err() on an actual error.
    pub fn find_remote(&self, remote_name: &str) -> Result<Option<RemoteHandle<'_>>, Error> {
        let remotes = self.0.remotes()?;

        if !remotes
            .iter()
            .any(|remote| remote.expect("Remote name is invalid utf-8") == remote_name)
        {
            return Ok(None);
        }

        Ok(Some(RemoteHandle(self.0.find_remote(remote_name)?)))
    }

    pub fn get_worktrees(&self) -> Result<Vec<Worktree>, Error> {
        Ok(self
            .0
            .worktrees()?
            .iter()
            .map(|name| name.expect("Worktree name is invalid utf-8"))
            .map(Worktree::new)
            .collect())
    }

    pub fn remove_worktree(
        &self,
        base_dir: &Path,
        name: &str,
        worktree_dir: &Path,
        force: bool,
        worktree_config: &Option<WorktreeRootConfig>,
    ) -> Result<(), Error> {
        let fullpath = base_dir.join(worktree_dir);

        if !fullpath.exists() {
            return Err(Error::WorktreeRemovalFailure(
                WorktreeRemoveFailureReason::Error(format!("{name} does not exist")),
            ));
        }
        let worktree_repo = RepoHandle::open(&fullpath, false).map_err(|error| {
            Error::WorktreeRemovalFailure(WorktreeRemoveFailureReason::Error(format!(
                "Error opening repo: {error}"
            )))
        })?;

        let local_branch = worktree_repo.head_branch().map_err(|error| {
            Error::WorktreeRemovalFailure(WorktreeRemoveFailureReason::Error(format!(
                "Failed getting head branch: {error}"
            )))
        })?;

        let branch_name = local_branch.name().map_err(|error| {
            Error::WorktreeRemovalFailure(WorktreeRemoveFailureReason::Error(format!(
                "Failed getting name of branch: {error}"
            )))
        })?;

        if branch_name != name {
            return Err(Error::WorktreeRemovalFailure(
                WorktreeRemoveFailureReason::Error(format!(
                    "Branch \"{}\" is checked out in worktree \"{}\", this does not look correct",
                    &branch_name,
                    &worktree_dir.display(),
                )),
            ));
        }

        let branch = worktree_repo.find_local_branch(&branch_name).map_err(|e| {
            Error::WorktreeRemovalFailure(WorktreeRemoveFailureReason::Error(e.to_string()))
        })?;

        if !force {
            let status = worktree_repo.status(false).map_err(|e| {
                Error::WorktreeRemovalFailure(WorktreeRemoveFailureReason::Error(e.to_string()))
            })?;
            if status.changes.is_some() {
                return Err(Error::WorktreeRemovalFailure(
                    WorktreeRemoveFailureReason::Changes(String::from("Changes found in worktree")),
                ));
            }

            let mut is_merged_into_persistent_branch = false;
            let mut has_persistent_branches = false;
            if let Some(config) = worktree_config {
                if let Some(branches) = &config.persistent_branches {
                    has_persistent_branches = true;
                    for persistent_branch in branches {
                        let persistent_branch = worktree_repo
                            .find_local_branch(persistent_branch)
                            .map_err(|e| {
                                Error::WorktreeRemovalFailure(WorktreeRemoveFailureReason::Error(
                                    e.to_string(),
                                ))
                            })?;

                        let (ahead, _behind) = worktree_repo
                            .graph_ahead_behind(&branch, &persistent_branch)
                            .unwrap();

                        if ahead == 0 {
                            is_merged_into_persistent_branch = true;
                        }
                    }
                }
            }

            if has_persistent_branches && !is_merged_into_persistent_branch {
                return Err(Error::WorktreeRemovalFailure(
                    WorktreeRemoveFailureReason::NotMerged(format!(
                        "Branch {name} is not merged into any persistent branches"
                    )),
                ));
            }

            if !has_persistent_branches {
                match branch.upstream() {
                    Ok(remote_branch) => {
                        let (ahead, behind) = worktree_repo
                            .graph_ahead_behind(&branch, &remote_branch)
                            .unwrap();

                        if (ahead, behind) != (0, 0) {
                            return Err(Error::WorktreeRemovalFailure(
                                WorktreeRemoveFailureReason::Changes(format!(
                                    "Branch {name} is not in line with remote branch"
                                )),
                            ));
                        }
                    }
                    Err(_) => {
                        return Err(Error::WorktreeRemovalFailure(
                            WorktreeRemoveFailureReason::Changes(format!(
                                "No remote tracking branch for branch {name} found"
                            )),
                        ));
                    }
                }
            }
        }

        // worktree_dir is a relative path, starting from base_dir. We walk it
        // upwards (from subdirectory to parent directories) and remove each
        // component, in case it is empty. Only the leaf directory can be
        // removed unconditionally (as it contains the worktree itself).
        if let Err(e) = std::fs::remove_dir_all(&fullpath) {
            return Err(Error::WorktreeRemovalFailure(
                WorktreeRemoveFailureReason::Error(format!(
                    "Error deleting {}: {}",
                    &worktree_dir.display(),
                    e
                )),
            ));
        }

        if let Some(current_dir) = worktree_dir.parent() {
            for current_dir in current_dir.ancestors() {
                let current_dir = base_dir.join(current_dir);
                if current_dir
                    .read_dir()
                    .map_err(|error| {
                        Error::WorktreeRemovalFailure(WorktreeRemoveFailureReason::Error(format!(
                            "Error reading {}: {}",
                            &current_dir.display(),
                            error
                        )))
                    })?
                    .next()
                    .is_none()
                {
                    if let Err(e) = std::fs::remove_dir(&current_dir) {
                        return Err(Error::WorktreeRemovalFailure(
                            WorktreeRemoveFailureReason::Error(format!(
                                "Error deleting {}: {}",
                                &worktree_dir.display(),
                                e
                            )),
                        ));
                    }
                } else {
                    break;
                }
            }
        }

        self.prune_worktree(name).map_err(|e| {
            Error::WorktreeRemovalFailure(WorktreeRemoveFailureReason::Error(e.to_string()))
        })?;
        branch.delete().map_err(|e| {
            Error::WorktreeRemovalFailure(WorktreeRemoveFailureReason::Error(e.to_string()))
        })?;

        Ok(())
    }

    pub fn cleanup_worktrees(&self, directory: &Path) -> Result<Vec<String>, Error> {
        let mut warnings = Vec::new();

        let worktrees = self.get_worktrees()?;

        let config = read_worktree_root_config(directory)?;

        let guess_default_branch = || self.default_branch()?.name();

        let default_branch_name = match &config {
            None => guess_default_branch()?,
            Some(config) => match &config.persistent_branches {
                None => guess_default_branch()?,
                Some(persistent_branches) => {
                    if persistent_branches.is_empty() {
                        guess_default_branch()?
                    } else {
                        persistent_branches[0].clone()
                    }
                }
            },
        };

        for worktree in worktrees
            .iter()
            .filter(|worktree| worktree.name() != default_branch_name)
            .filter(|worktree| match &config {
                None => true,
                Some(config) => match &config.persistent_branches {
                    None => true,
                    Some(branches) => !branches.iter().any(|branch| branch == worktree.name()),
                },
            })
        {
            let repo_dir = &directory.join(worktree.name());
            if repo_dir.exists() {
                match self.remove_worktree(
                    directory,
                    worktree.name(),
                    Path::new(worktree.name()),
                    false,
                    &config,
                ) {
                    Ok(()) => print_success(&format!("Worktree {} deleted", &worktree.name())),
                    Err(error) => match error {
                        Error::WorktreeRemovalFailure(ref removal_error) => match removal_error {
                            WorktreeRemoveFailureReason::Changes(changes) => {
                                warnings.push(format!(
                                    "Changes found in {}: {}, skipping",
                                    &worktree.name(),
                                    &changes
                                ));
                                continue;
                            }
                            WorktreeRemoveFailureReason::NotMerged(message) => {
                                warnings.push(message.clone());
                                continue;
                            }
                            WorktreeRemoveFailureReason::Error(_) => {
                                return Err(error);
                            }
                        },
                        _ => return Err(error),
                    },
                }
            } else {
                warnings.push(format!(
                    "Worktree {} does not have a directory",
                    &worktree.name()
                ));
            }
        }
        Ok(warnings)
    }

    pub fn find_unmanaged_worktrees(&self, directory: &Path) -> Result<Vec<String>, Error> {
        let worktrees = self.get_worktrees()?;

        let mut unmanaged_worktrees = Vec::new();
        for entry in std::fs::read_dir(directory)? {
            let dirname = path::path_as_string(
                entry?
                    .path()
                    .strip_prefix(directory)
                    // that unwrap() is safe as each entry is
                    // guaranteed to be a subentry of &directory
                    .unwrap(),
            );

            let config = read_worktree_root_config(directory)?;

            let guess_default_branch = || {
                self.default_branch()
                    .map_err(|error| format!("Failed getting default branch: {error}"))?
                    .name()
                    .map_err(|error| format!("Failed getting default branch name: {error}"))
            };

            let default_branch_name = match &config {
                None => guess_default_branch().ok(),
                Some(config) => match &config.persistent_branches {
                    None => guess_default_branch().ok(),
                    Some(persistent_branches) => {
                        if persistent_branches.is_empty() {
                            guess_default_branch().ok()
                        } else {
                            Some(persistent_branches[0].clone())
                        }
                    }
                },
            };

            if dirname == worktree::GIT_MAIN_WORKTREE_DIRECTORY {
                continue;
            }
            if dirname == WORKTREE_CONFIG_FILE_NAME {
                continue;
            }
            if let Some(default_branch_name) = default_branch_name {
                if dirname == default_branch_name {
                    continue;
                }
            }
            if !&worktrees.iter().any(|worktree| worktree.name() == dirname) {
                unmanaged_worktrees.push(dirname);
            }
        }
        Ok(unmanaged_worktrees)
    }

    pub fn detect_worktree(path: &Path) -> bool {
        path.join(worktree::GIT_MAIN_WORKTREE_DIRECTORY).exists()
    }
}

pub struct RemoteHandle<'a>(git2::Remote<'a>);
pub struct Commit<'a>(git2::Commit<'a>);
pub struct Reference();
pub struct Oid(git2::Oid);

impl Oid {
    pub fn hex_string(&self) -> String {
        self.0.to_string()
    }
}

impl Commit<'_> {
    pub fn id(&self) -> Oid {
        Oid(self.0.id())
    }

    pub(self) fn author(&self) -> git2::Signature<'_> {
        self.0.author()
    }
}

impl<'a> Branch<'a> {
    pub fn to_commit(self) -> Result<Commit<'a>, Error> {
        Ok(Commit(self.0.into_reference().peel_to_commit()?))
    }
}

impl<'a> Branch<'a> {
    pub fn commit(&self) -> Result<Commit<'_>, Error> {
        Ok(Commit(self.0.get().peel_to_commit()?))
    }

    pub fn commit_owned(self) -> Result<Commit<'a>, Error> {
        Ok(Commit(self.0.into_reference().peel_to_commit()?))
    }

    pub fn set_upstream(&mut self, remote_name: &str, branch_name: &str) -> Result<(), Error> {
        self.0
            .set_upstream(Some(&format!("{remote_name}/{branch_name}")))?;
        Ok(())
    }

    pub fn name(&self) -> Result<String, Error> {
        Ok(self
            .0
            .name()
            .map(|name| name.expect("Branch name is invalid utf-8"))
            .map(|s| s.to_owned())?)
    }

    pub fn upstream(&self) -> Result<Branch<'_>, Error> {
        Ok(Branch(self.0.upstream()?))
    }

    pub fn delete(mut self) -> Result<(), Error> {
        Ok(self.0.delete()?)
    }

    pub fn basename(&self) -> Result<String, Error> {
        let name = self.name()?;
        if let Some((_prefix, basename)) = name.split_once('/') {
            Ok(basename.to_string())
        } else {
            Ok(name)
        }
    }

    // only used internally in this module, exposes libgit2 details
    fn as_reference(&self) -> &git2::Reference<'_> {
        self.0.get()
    }
}

fn get_remote_callbacks() -> git2::RemoteCallbacks<'static> {
    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.push_update_reference(|_, status| {
        if let Some(message) = status {
            return Err(git2::Error::new(
                git2::ErrorCode::GenericError,
                git2::ErrorClass::None,
                message,
            ));
        }
        Ok(())
    });

    callbacks.credentials(|_url, username_from_url, _allowed_types| {
        let Some(username) = username_from_url else {
            panic!("Could not get username. This is a bug")
        };
        git2::Cred::ssh_key_from_agent(username)
    });

    callbacks
}

impl RemoteHandle<'_> {
    pub fn url(&self) -> String {
        self.0
            .url()
            .expect("Remote URL is invalid utf-8")
            .to_string()
    }

    pub fn name(&self) -> String {
        self.0
            .name()
            .expect("Remote name is invalid utf-8")
            .to_string()
    }

    pub fn connected(&mut self) -> bool {
        self.0.connected()
    }

    pub fn default_branch(&self) -> Result<String, Error> {
        Ok(self
            .0
            .default_branch()?
            .as_str()
            .expect("Remote branch name is not valid utf-8")
            .to_string())
    }

    pub fn is_pushable(&self) -> Result<bool, Error> {
        let remote_type = detect_remote_type(self.0.url().expect("Remote name is not valid utf-8"))
            .expect("Could not detect remote type");
        Ok(matches!(remote_type, RemoteType::Ssh | RemoteType::File))
    }

    pub fn push(
        &mut self,
        local_branch_name: &str,
        remote_branch_name: &str,
        _repo: &RepoHandle,
    ) -> Result<(), Error> {
        if !self.is_pushable()? {
            return Err(Error::NonPushableRemote);
        }

        let mut push_options = git2::PushOptions::new();
        push_options.remote_callbacks(get_remote_callbacks());

        let push_refspec =
            format!("+refs/heads/{local_branch_name}:refs/heads/{remote_branch_name}");
        self.0
            .push(&[push_refspec], Some(&mut push_options))
            .map_err(|error| Error::PushFailed {
                local_branch: local_branch_name.to_owned(),
                remote_name: self.name(),
                remote_url: self.url(),
                message: error.to_string(),
            })?;
        Ok(())
    }
}

pub fn clone_repo(
    remote: &Remote,
    path: &Path,
    is_worktree: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let clone_target = if is_worktree {
        path.join(worktree::GIT_MAIN_WORKTREE_DIRECTORY)
    } else {
        path.to_path_buf()
    };

    print_action(&format!(
        "Cloning into \"{}\" from \"{}\"",
        &clone_target.display(),
        &remote.url
    ));
    match remote.remote_type {
        RemoteType::Https | RemoteType::File => {
            let mut builder = git2::build::RepoBuilder::new();

            let fetchopts = git2::FetchOptions::new();

            builder.bare(is_worktree);
            builder.fetch_options(fetchopts);

            builder.clone(&remote.url, &clone_target)?;
        }
        RemoteType::Ssh => {
            let mut fo = git2::FetchOptions::new();
            fo.remote_callbacks(get_remote_callbacks());

            let mut builder = git2::build::RepoBuilder::new();
            builder.bare(is_worktree);
            builder.fetch_options(fo);

            builder.clone(&remote.url, &clone_target)?;
        }
    }

    let repo = RepoHandle::open(&clone_target, false)?;

    if is_worktree {
        repo.set_config_push(GitPushDefaultSetting::Upstream)?;
    }

    if remote.name != "origin" {
        // unwrap() is safe here as the origin remote will always exist after a successful clone.
        // Note that actual errors are handled in the Results Err variant, not in
        // the Ok variant option
        let origin = repo.find_remote("origin")?.unwrap();
        repo.rename_remote(&origin, &remote.name)?;
    }

    // Initialize local branches. For all remote branches, we set up local
    // tracking branches with the same name (just without the remote prefix).
    for remote_branch in repo.remote_branches()? {
        let local_branch_name = remote_branch.basename()?;

        if repo.find_local_branch(&local_branch_name).is_ok() {
            continue;
        }

        // Ignore <remote>/HEAD, as this is not something we can check out
        if local_branch_name == "HEAD" {
            continue;
        }

        let mut local_branch = repo.create_branch(&local_branch_name, &remote_branch.commit()?)?;
        local_branch.set_upstream(&remote.name, &local_branch_name)?;
    }

    // If there is no head_branch, we most likely cloned an empty repository and
    // there is no point in setting any upstreams.
    if let Ok(mut active_branch) = repo.head_branch() {
        active_branch.set_upstream(&remote.name, &active_branch.name()?)?;
    };

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_ssh_remote() -> Result<(), Error> {
        assert_eq!(
            detect_remote_type("ssh://git@example.com")?,
            RemoteType::Ssh
        );
        assert_eq!(detect_remote_type("git@example.git")?, RemoteType::Ssh);
        Ok(())
    }

    #[test]
    fn check_https_remote() -> Result<(), Error> {
        assert_eq!(
            detect_remote_type("https://example.com")?,
            RemoteType::Https
        );
        assert_eq!(
            detect_remote_type("https://example.com/test.git")?,
            RemoteType::Https
        );
        Ok(())
    }

    #[test]
    fn check_file_remote() -> Result<(), Error> {
        assert_eq!(detect_remote_type("file:///somedir")?, RemoteType::File);
        Ok(())
    }

    #[test]
    fn check_invalid_remotes() {
        assert!(matches!(
            detect_remote_type("https//example.com"),
            Err(Error::UnimplementedRemoteProtocol)
        ));
        assert!(matches!(
            detect_remote_type("https:example.com"),
            Err(Error::UnimplementedRemoteProtocol)
        ));
        assert!(matches!(
            detect_remote_type("ssh//example.com"),
            Err(Error::UnimplementedRemoteProtocol)
        ));
        assert!(matches!(
            detect_remote_type("ssh:example.com"),
            Err(Error::UnimplementedRemoteProtocol)
        ));
        assert!(matches!(
            detect_remote_type("git@example.com"),
            Err(Error::UnimplementedRemoteProtocol)
        ));
    }

    #[test]
    fn check_unsupported_protocol_http() {
        assert!(matches!(
            detect_remote_type("http://example.com"),
            Err(Error::UnsupportedHttpRemote)
        ));
    }

    #[test]
    fn check_unsupported_protocol_git() {
        assert!(matches!(
            detect_remote_type("git://example.com"),
            Err(Error::UnsupportedGitRemote)
        ));
    }

    #[test]
    fn repo_check_fullname() {
        let with_namespace = Repo {
            name: "name".to_string(),
            namespace: Some("namespace".to_string()),
            worktree_setup: false,
            remotes: None,
        };

        let without_namespace = Repo {
            name: "name".to_string(),
            namespace: None,
            worktree_setup: false,
            remotes: None,
        };

        assert_eq!(with_namespace.fullname(), "namespace/name");
        assert_eq!(without_namespace.fullname(), "name");
    }
}
