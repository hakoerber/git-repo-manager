use std::fmt;

use camino::{Utf8Path as Path, Utf8PathBuf as PathBuf};
use thiserror::Error;

use super::{Warning, config, path};

mod remote;
mod worktree;

pub use remote::{RemoteName, RemoteType, RemoteUrl};
pub use worktree::{
    CleanupWorktreeError, CleanupWorktreeWarningReason, Error as WorktreeError,
    GIT_MAIN_WORKTREE_DIRECTORY, TrackingSelection, Worktree, WorktreeConversionError,
    WorktreeName, WorktreeRemoveError, WorktreeRepoHandle, WorktreeRootConfig, WorktreeSetup,
    WorktreeValidationError,
};

const GIT_CONFIG_BARE_KEY: GitConfigKey = GitConfigKey("core.bare");
const GIT_CONFIG_PUSH_DEFAULT: &str = "push.default";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchName(String);

impl fmt::Display for BranchName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl BranchName {
    pub fn new(from: String) -> Self {
        Self(from)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmoduleName(String);

impl fmt::Display for SubmoduleName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl SubmoduleName {
    pub fn new(from: String) -> Self {
        Self(from)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

#[derive(Clone, Copy)]
pub enum GitPushDefaultSetting {
    Upstream,
}

#[derive(Debug)]
pub struct GitConfigKey(&'static str);

impl GitConfigKey {
    fn as_str(&self) -> &str {
        self.0
    }
}

impl fmt::Display for GitConfigKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Error reading configuration file \"{path}\": {message}")]
    ReadConfig { message: String, path: PathBuf },
    #[error("Error parsing configuration file \"{path}\": {message}")]
    ParseConfig { message: String, path: PathBuf },
    #[error(transparent)]
    Libgit(#[from] git2::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Config(#[from] config::Error),
    #[error("Branch not found")]
    BranchNotFound,
    #[error("Repo not found")]
    RepoNotFound,
    #[error("Could not determine default branch")]
    NoDefaultBranch,
    #[error("Failed getting default branch name: {message}")]
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
    #[error("Could not set {key}: {error}")]
    GitConfigSetError { key: GitConfigKey, error: String },
    #[error("Cannot get changes as this is a bare worktree repository")]
    GettingChangesFromBareWorktree,
    #[error("Trying to push to a non-pushable remote")]
    NonPushableRemote,
    #[error("Pushing \"{local_branch}\" to \"{remote_name}\" ({remote_url}) failed: {message}")]
    PushFailed {
        local_branch: BranchName,
        remote_name: RemoteName,
        remote_url: RemoteUrl,
        message: String,
    },
    #[error(transparent)]
    Path(#[from] path::Error),
    #[error("Branch name is not valid utf-8")]
    BranchNameNotUtf8,
    #[error("Remote name is not valid utf-8")]
    RemoteNameNotUtf8,
    #[error("Remote branch name is not valid utf-8")]
    RemoteBranchNameNotUtf8,
    #[error("Submodule name is not valid utf-8")]
    SubmoduleNameNotUtf8,
    #[error("Submodule name is not valid utf-8")]
    CannotGetBranchName {
        #[source]
        inner: git2::Error,
    },
    #[error("Remote HEAD ({name}) pointer is invalid")]
    InvalidRemoteHeadPointer { name: String },
    #[error("Remote HEAD does not point to a symbolic target")]
    RemoteHeadNoSymbolicTarget,
}

#[derive(Debug)]
pub struct Remote {
    pub name: RemoteName,
    pub url: RemoteUrl,
    pub remote_type: RemoteType,
}

impl From<config::Remote> for Remote {
    fn from(other: config::Remote) -> Self {
        Self {
            name: RemoteName::new(other.name),
            url: RemoteUrl::new(other.url),
            remote_type: other.remote_type.into(),
        }
    }
}

impl From<Remote> for config::Remote {
    fn from(other: Remote) -> Self {
        Self {
            name: other.name.into_string(),
            url: other.url.into_string(),
            remote_type: other.remote_type.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoName(String);

impl RepoName {
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

impl fmt::Display for RepoName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug)]
pub struct RepoNamespace(String);

impl RepoNamespace {
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

#[derive(Debug)]
pub struct Repo {
    pub name: RepoName,
    pub namespace: Option<RepoNamespace>,
    pub worktree_setup: WorktreeSetup,
    pub remotes: Vec<Remote>,
}

impl From<config::Repo> for Repo {
    fn from(other: config::Repo) -> Self {
        let (namespace, name) = if let Some((namespace, name)) = other.name.rsplit_once('/') {
            (Some(namespace.to_owned()), name.to_owned())
        } else {
            (None, other.name)
        };

        Self {
            name: RepoName::new(name),
            namespace: namespace.map(RepoNamespace::new),
            worktree_setup: other.worktree_setup.into(),
            remotes: other.remotes.map_or_else(Vec::new, |remotes| {
                remotes.into_iter().map(Into::into).collect()
            }),
        }
    }
}

impl From<Repo> for config::Repo {
    fn from(other: Repo) -> Self {
        Self {
            name: other.name.into_string(),
            worktree_setup: other.worktree_setup.is_worktree(),
            remotes: Some(other.remotes.into_iter().map(Into::into).collect()),
        }
    }
}

impl Repo {
    pub fn fullname(&self) -> RepoName {
        match self.namespace {
            Some(ref namespace) => {
                RepoName(format!("{}/{}", namespace.as_str(), self.name.as_str()))
            }
            None => RepoName(self.name.as_str().to_owned()),
        }
    }

    pub fn remove_namespace(&mut self) {
        self.namespace = None;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RepoChanges {
    pub files_new: usize,
    pub files_modified: usize,
    pub files_deleted: usize,
}

impl fmt::Display for RepoChanges {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.files_new == 0 && self.files_modified == 0 && self.files_deleted == 0 {
            write!(f, "no changes")
        } else {
            #[expect(
                clippy::useless_let_if_seq,
                reason = "Clearer to set started in the beginning and then modify it in each block"
            )]
            {
                let mut started = false;

                if self.files_new > 0 {
                    write!(f, "{} new", self.files_new)?;
                    started = true;
                }

                if self.files_modified > 0 {
                    if started {
                        write!(f, ", ")?;
                    }
                    write!(f, "{} modified", self.files_modified)?;
                    started = true;
                }

                if self.files_deleted > 0 {
                    if started {
                        write!(f, ", ")?;
                    }
                    write!(f, "{} deleted", self.files_deleted)?;
                }

                Ok(())
            }
        }
    }
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

    pub remotes: Vec<RemoteName>,

    pub head: Option<BranchName>,

    pub changes: Option<RepoChanges>,

    pub worktrees: usize,

    pub submodules: Option<Vec<(SubmoduleName, SubmoduleStatus)>>,

    pub branches: Vec<(BranchName, Option<(BranchName, RemoteTrackingStatus)>)>,
}

impl RepoStatus {
    fn clean(&self) -> bool {
        match self.changes {
            None => true,
            Some(ref changes) => {
                changes.files_new == 0 && changes.files_deleted == 0 && changes.files_modified == 0
            }
        }
    }
}

pub fn detect_remote_type(remote_url: &RemoteUrl) -> Result<RemoteType, Error> {
    let remote_url = remote_url.as_str();

    #[expect(clippy::missing_panics_doc, reason = "regex is valid")]
    let git_regex = regex::Regex::new(r"^[a-zA-Z]+@.*$").expect("regex is valid");
    if remote_url.starts_with("ssh://") {
        return Ok(RemoteType::Ssh);
    }
    #[expect(
        clippy::case_sensitive_file_extension_comparisons,
        reason = "the extension is always lower case"
    )]
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
    pub fn open(path: &Path) -> Result<Self, Error> {
        Self::open_with_worktree_setup(path, WorktreeSetup::NoWorktree)
    }

    pub fn open_with_worktree_setup(
        path: &Path,
        worktree_setup: WorktreeSetup,
    ) -> Result<Self, Error> {
        let open_func = if worktree_setup.is_worktree() {
            git2::Repository::open_bare
        } else {
            git2::Repository::open
        };
        let path = if worktree_setup.is_worktree() {
            path.join(worktree::GIT_MAIN_WORKTREE_DIRECTORY)
        } else {
            path.to_path_buf()
        };
        match open_func(path) {
            Ok(r) => Ok(Self(r)),
            Err(e) => match e.code() {
                git2::ErrorCode::NotFound => Err(Error::RepoNotFound),
                _ => Err(Error::Libgit(e)),
            },
        }
    }

    pub fn path(&self) -> Result<&Path, Error> {
        Ok(path::from_std_path(self.0.path())?)
    }

    pub fn commondir(&self) -> Result<&Path, Error> {
        Ok(path::from_std_path(self.0.commondir())?)
    }

    pub fn stash(&self) -> Result<(), Error> {
        let head_branch = self.head_branch()?;
        let head = head_branch.commit()?;
        let author = head.author();

        // This is honestly quite horrible. The problem is that all stash operations
        // expect a mutable reference (as they, well, mutate the repo after
        // all). But we are heavily using immutable references a lot with this
        // struct. I'm really not sure how to best solve this. Right now, we
        // just open the repo AGAIN. It is safe, as we are only accessing the stash
        // with the second reference, so there are no cross effects. But it just smells.
        let mut repo = Self::open(self.path()?)?;
        repo.0
            .stash_save2(&author, None, Some(git2::StashFlags::INCLUDE_UNTRACKED))?;
        Ok(())
    }

    pub fn stash_pop(&self) -> Result<(), Error> {
        let mut repo = Self::open(self.path()?)?;
        repo.0.stash_pop(
            0,
            Some(git2::StashApplyOptions::new().reinstantiate_index()),
        )?;
        Ok(())
    }

    pub fn rename_remote(&self, remote: &RemoteHandle, new_name: &RemoteName) -> Result<(), Error> {
        let failed_refspecs = self
            .0
            .remote_rename(remote.name()?.as_str(), new_name.as_str())?;

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
            .find_local_branch(&BranchName::new(
                head.shorthand().ok_or(Error::BranchNameNotUtf8)?.to_owned(),
            ))?
            .ok_or(Error::BranchNotFound)?;
        Ok(branch)
    }

    pub fn remote_set_url(&self, name: &RemoteName, url: &RemoteUrl) -> Result<(), Error> {
        Ok(self.0.remote_set_url(name.as_str(), url.as_str())?)
    }

    pub fn remote_delete(&self, name: &RemoteName) -> Result<(), Error> {
        Ok(self.0.remote_delete(name.as_str())?)
    }

    pub fn is_empty(&self) -> Result<bool, Error> {
        Ok(self.0.is_empty()?)
    }

    pub fn is_bare(&self) -> bool {
        self.0.is_bare()
    }

    pub fn remotes(&self) -> Result<Vec<RemoteName>, Error> {
        self.0
            .remotes()?
            .iter()
            .map(|name| {
                name.ok_or(Error::RemoteNameNotUtf8)
                    .map(|s| RemoteName::new(s.to_owned()))
            })
            .collect()
    }

    pub fn new_remote(&self, name: &RemoteName, url: &RemoteUrl) -> Result<(), Error> {
        self.0.remote(name.as_str(), url.as_str())?;
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

    pub fn fetch(&self, remote_name: &RemoteName) -> Result<(), Error> {
        let mut remote = self.0.find_remote(remote_name.as_str())?;

        let mut fetch_options = git2::FetchOptions::new();
        fetch_options.remote_callbacks(get_remote_callbacks());

        for refspec in &remote.fetch_refspecs()? {
            remote.fetch(
                &[refspec.ok_or(Error::RemoteNameNotUtf8)?],
                Some(&mut fetch_options),
                None,
            )?;
        }
        Ok(())
    }

    pub fn init(path: &Path, worktree_setup: WorktreeSetup) -> Result<Self, Error> {
        let repo = if worktree_setup.is_worktree() {
            git2::Repository::init_bare(path.join(worktree::GIT_MAIN_WORKTREE_DIRECTORY))?
        } else {
            git2::Repository::init(path)?
        };

        let repo = Self(repo);

        if worktree_setup.is_worktree() {
            repo.set_config_push(GitPushDefaultSetting::Upstream)?;
        }

        Ok(repo)
    }

    pub fn config(&self) -> Result<git2::Config, Error> {
        Ok(self.0.config()?)
    }

    pub fn prune_worktree(&self, name: &WorktreeName) -> Result<(), Error> {
        let worktree = self.0.find_worktree(name.as_str())?;
        worktree.prune(None)?;
        Ok(())
    }

    pub fn find_remote_branch(
        &self,
        remote_name: &RemoteName,
        branch_name: &BranchName,
    ) -> Result<Option<Branch<'_>>, Error> {
        match self.0.find_branch(
            &format!("{}/{}", remote_name.as_str(), branch_name.as_str()),
            git2::BranchType::Remote,
        ) {
            Ok(branch) => Ok(Some(Branch(branch))),
            Err(e) => match e.code() {
                git2::ErrorCode::NotFound => Ok(None),
                _ => Err(e.into()),
            },
        }
    }

    pub fn find_local_branch(&self, name: &BranchName) -> Result<Option<Branch<'_>>, Error> {
        match self.0.find_branch(name.as_str(), git2::BranchType::Local) {
            Ok(branch) => Ok(Some(Branch(branch))),
            Err(e) => match e.code() {
                git2::ErrorCode::NotFound => Ok(None),
                _ => Err(e.into()),
            },
        }
    }

    pub fn create_branch(&self, name: &BranchName, target: &Commit) -> Result<Branch<'_>, Error> {
        Ok(Branch(self.0.branch(name.as_str(), &target.0, false)?))
    }

    pub fn make_bare(&self, value: bool) -> Result<(), Error> {
        let mut config = self.config()?;

        config
            .set_bool(GIT_CONFIG_BARE_KEY.as_str(), value)
            .map_err(|error| Error::GitConfigSetError {
                key: GIT_CONFIG_BARE_KEY,
                error: error.to_string(),
            })
    }

    /// Converting works like this:
    /// * Check whether there are uncommitted/unpushed changes
    /// * Move the contents of .git dir to the worktree directory
    /// * Remove all files
    /// * Set `core.bare` to `true`
    pub fn convert_to_worktree(&self, root_dir: &Path) -> Result<(), WorktreeConversionError> {
        if let Some(changes) = self
            .status(WorktreeSetup::NoWorktree)
            .map_err(|e| WorktreeConversionError::RepoError(e))?
            .changes
        {
            return Err(WorktreeConversionError::Changes(changes));
        }

        if self
            .has_untracked_files(WorktreeSetup::NoWorktree)
            .map_err(|e| WorktreeConversionError::RepoError(e))?
        {
            return Err(WorktreeConversionError::Ignored);
        }

        std::fs::rename(".git", worktree::GIT_MAIN_WORKTREE_DIRECTORY).map_err(|error| {
            WorktreeConversionError::RenameError(format!("Error moving .git directory: {error}"))
        })?;

        for entry in root_dir
            .read_dir_utf8()
            .map_err(|err| WorktreeConversionError::OpenDirectoryError(err))?
        {
            match entry {
                Ok(entry) => {
                    if entry.file_name() == worktree::GIT_MAIN_WORKTREE_DIRECTORY {
                        continue;
                    }
                    if entry.path().is_file() || entry.path().is_symlink() {
                        if let Err(error) = std::fs::remove_file(entry.path()) {
                            return Err(WorktreeConversionError::RemoveError {
                                path: entry.into_path(),
                                error,
                            });
                        }
                    } else if let Err(error) = std::fs::remove_dir_all(entry.path()) {
                        return Err(WorktreeConversionError::RemoveError {
                            path: entry.into_path(),
                            error,
                        });
                    }
                }
                Err(error) => {
                    return Err(WorktreeConversionError::ReadDirectoryError(error));
                }
            }
        }

        let worktree_repo = WorktreeRepoHandle::open(root_dir)
            .map_err(|error| WorktreeConversionError::RepoError(error))?;

        worktree_repo
            .as_repo()
            .make_bare(true)
            .map_err(|error| WorktreeConversionError::RepoError(error))?;

        worktree_repo
            .as_repo()
            .set_config_push(GitPushDefaultSetting::Upstream)
            .map_err(|error| WorktreeConversionError::RepoError(error))?;

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
                key: GIT_CONFIG_BARE_KEY,
                error: error.to_string(),
            })
    }

    pub fn has_untracked_files(&self, worktree_setup: WorktreeSetup) -> Result<bool, Error> {
        if worktree_setup.is_worktree() {
            Err(Error::GettingChangesFromBareWorktree)
        } else {
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

    pub fn status(&self, worktree_setup: WorktreeSetup) -> Result<RepoStatus, Error> {
        let operation = match self.0.state() {
            git2::RepositoryState::Clean => None,
            state => Some(state),
        };

        let empty = self.is_empty()?;

        let remotes = self
            .0
            .remotes()?
            .iter()
            .map(|repo_name| {
                repo_name
                    .ok_or(Error::RemoteNameNotUtf8)
                    .map(|s| RemoteName::new(s.to_owned()))
            })
            .collect::<Result<Vec<RemoteName>, Error>>()?;

        let head = if worktree_setup.is_worktree() || empty {
            None
        } else {
            Some(self.head_branch()?.name()?)
        };

        let changes = if worktree_setup.is_worktree() {
            None
        } else {
            let statuses = self.0.statuses(Some(
                git2::StatusOptions::new()
                    .include_ignored(false)
                    .include_untracked(true),
            ))?;

            if statuses.is_empty() {
                None
            } else {
                let mut files_new: usize = 0;
                let mut files_modified: usize = 0;
                let mut files_deleted: usize = 0;
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
                        files_modified = files_modified.saturating_add(1);
                    } else if status_bits.intersects(git2::Status::INDEX_NEW | git2::Status::WT_NEW)
                    {
                        files_new = files_new.saturating_add(1);
                    } else if status_bits
                        .intersects(git2::Status::INDEX_DELETED | git2::Status::WT_DELETED)
                    {
                        files_deleted = files_deleted.saturating_add(1);
                    }
                }

                #[expect(clippy::missing_panics_doc, reason = "panicking due to bug")]
                {
                    assert!(
                        ((files_new, files_modified, files_deleted) != (0, 0, 0)),
                        "is_empty() returned true, but no file changes were detected. This is a bug!"
                    );
                }

                Some(RepoChanges {
                    files_new,
                    files_modified,
                    files_deleted,
                })
            }
        };

        let worktrees = self.0.worktrees()?.len();

        let submodules = if worktree_setup.is_worktree() {
            None
        } else {
            let mut submodules = Vec::new();
            for submodule in self.0.submodules()? {
                let submodule_name = SubmoduleName::new(
                    submodule
                        .name()
                        .ok_or(Error::SubmoduleNameNotUtf8)?
                        .to_owned(),
                );

                let submodule_status;
                let status = self
                    .0
                    .submodule_status(submodule_name.as_str(), git2::SubmoduleIgnore::None)?;

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
        for branch in self.0.branches(Some(git2::BranchType::Local))? {
            let (local_branch, _branch_type) = branch?;
            let branch_name = BranchName::new(
                local_branch
                    .name()
                    .map_err(|e| Error::CannotGetBranchName { inner: e })?
                    .ok_or(Error::BranchNameNotUtf8)?
                    .to_owned(),
            );
            let remote_branch = match local_branch.upstream() {
                Ok(remote_branch) => {
                    let remote_branch_name = BranchName::new(
                        remote_branch
                            .name()
                            .map_err(|e| Error::CannotGetBranchName { inner: e })?
                            .ok_or(Error::BranchNameNotUtf8)?
                            .to_owned(),
                    );

                    let (ahead, behind) = self.0.graph_ahead_behind(
                        local_branch.get().peel_to_commit()?.id(),
                        remote_branch.get().peel_to_commit()?.id(),
                    )?;

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
        remote_name: &RemoteName,
    ) -> Result<Option<Branch<'_>>, Error> {
        // libgit2's `git_remote_default_branch()` and `Remote::default_branch()`
        // need an actual connection to the remote, so they may fail.
        if let Some(mut remote) = self.find_remote(remote_name)? {
            if remote.connected() {
                let remote = remote; // unmut
                if let Ok(remote_default_branch) = remote.default_branch() {
                    return Ok(Some(
                        self.find_local_branch(&remote_default_branch)?
                            .ok_or(Error::BranchNotFound)?,
                    ));
                }
            }
        }

        // Note that <remote>/HEAD only exists after a normal clone, there is no way to
        // get the remote HEAD afterwards. So this is a "best effort" approach.
        match self.find_remote_branch(remote_name, &BranchName::new("HEAD".to_owned()))? {
            Some(remote_head) => {
                if let Some(pointer_name) = remote_head.as_reference().symbolic_target() {
                    if let Some(local_branch_name) =
                        pointer_name.strip_prefix(&format!("refs/remotes/{remote_name}/"))
                    {
                        Ok(Some(
                            self.find_local_branch(&BranchName(local_branch_name.to_owned()))?
                                .ok_or(Error::BranchNotFound)?,
                        ))
                    } else {
                        Err(Error::InvalidRemoteHeadPointer {
                            name: pointer_name.to_owned(),
                        })
                    }
                } else {
                    Err(Error::RemoteHeadNoSymbolicTarget)
                }
            }
            None => Ok(None),
        }
    }

    pub fn default_branch(&self) -> Result<Branch<'_>, Error> {
        // This is a bit of a guessing game.
        //
        // In the best case, there is only one remote. Then, we can check <remote>/HEAD
        // to get the default remote branch.
        //
        // If there are multiple remotes, we first check whether they all have the same
        // <remote>/HEAD branch. If yes, good! If not, we use whatever "origin" uses, if
        // that exists. If it does not, there is no way to reliably get a remote
        // default branch.
        //
        // In this case, we just try to guess a local branch from a list. If even that
        // does not work, well, bad luck.
        let remotes = self.remotes()?;

        if remotes.len() == 1 {
            #[expect(clippy::missing_panics_doc, reason = "see expect() message")]
            let remote_name = &remotes.first().expect("checked for len above");
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
                        .map(Branch::name)
                        .collect::<Result<Vec<BranchName>, Error>>()?
                        .windows(2)
                        .all(
                            #[expect(
                                clippy::missing_asserts_for_indexing,
                                clippy::indexing_slicing,
                                reason = "windows function always returns two elements"
                            )]
                            |branch_names| branch_names[0] == branch_names[1],
                        ))
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
    pub fn find_remote(&self, remote_name: &RemoteName) -> Result<Option<RemoteHandle<'_>>, Error> {
        let remotes = self.0.remotes()?;

        if !remotes
            .iter()
            .map(|remote| remote.ok_or(Error::RemoteNameNotUtf8))
            .collect::<Result<Vec<_>, Error>>()?
            .into_iter()
            .any(|remote| remote == remote_name.as_str())
        {
            return Ok(None);
        }

        Ok(Some(RemoteHandle(
            self.0.find_remote(remote_name.as_str())?,
        )))
    }
}

pub struct RemoteHandle<'a>(git2::Remote<'a>);
pub struct Commit<'a>(git2::Commit<'a>);
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

    pub fn set_upstream(
        &mut self,
        remote_name: &RemoteName,
        branch_name: &BranchName,
    ) -> Result<(), Error> {
        self.0.set_upstream(Some(&format!(
            "{}/{}",
            remote_name.as_str(),
            branch_name.as_str()
        )))?;
        Ok(())
    }

    pub fn name(&self) -> Result<BranchName, Error> {
        Ok(BranchName::new(
            self.0.name()?.ok_or(Error::BranchNameNotUtf8)?.to_owned(),
        ))
    }

    pub fn upstream(&self) -> Result<Option<Branch<'_>>, Error> {
        let branch = self.0.upstream();
        match branch {
            Ok(branch) => Ok(Some(Branch(branch))),
            Err(err) if err.code() == git2::ErrorCode::NotFound => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    pub fn delete(mut self) -> Result<(), Error> {
        Ok(self.0.delete()?)
    }

    pub fn basename(&self) -> Result<BranchName, Error> {
        let name = self.name()?;
        if let Some((_prefix, basename)) = name.as_str().split_once('/') {
            Ok(BranchName::new(basename.to_owned()))
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
        #[expect(clippy::panic, reason = "there is no good way to bubble up that error")]
        let Some(username) = username_from_url else {
            panic!("Could not get username. This is a bug")
        };
        git2::Cred::ssh_key_from_agent(username)
    });

    callbacks
}

impl RemoteHandle<'_> {
    pub fn url(&self) -> Result<RemoteUrl, Error> {
        Ok(RemoteUrl::new(
            self.0.url().ok_or(Error::RemoteNameNotUtf8)?.to_owned(),
        ))
    }

    pub fn name(&self) -> Result<RemoteName, Error> {
        Ok(RemoteName::new(
            self.0.name().ok_or(Error::RemoteNameNotUtf8)?.to_owned(),
        ))
    }

    pub fn connected(&mut self) -> bool {
        self.0.connected()
    }

    pub fn default_branch(&self) -> Result<BranchName, Error> {
        Ok(BranchName(
            self.0
                .default_branch()?
                .as_str()
                .ok_or(Error::RemoteBranchNameNotUtf8)?
                .to_owned(),
        ))
    }

    pub fn is_pushable(&self) -> Result<bool, Error> {
        let remote_type = detect_remote_type(&RemoteUrl::new(
            self.0.url().ok_or(Error::RemoteNameNotUtf8)?.to_owned(),
        ))?;
        Ok(matches!(remote_type, RemoteType::Ssh | RemoteType::File))
    }

    pub fn push(
        &mut self,
        local_branch_name: &BranchName,
        remote_branch_name: &BranchName,
        _repo: &RepoHandle,
    ) -> Result<(), Error> {
        if !self.is_pushable()? {
            return Err(Error::NonPushableRemote);
        }

        let mut push_options = git2::PushOptions::new();
        push_options.remote_callbacks(get_remote_callbacks());

        let push_refspec = format!(
            "+refs/heads/{}:refs/heads/{}",
            local_branch_name.as_str(),
            remote_branch_name.as_str()
        );
        self.0
            .push(&[push_refspec], Some(&mut push_options))
            .map_err(|error| Error::PushFailed {
                local_branch: local_branch_name.clone(),
                remote_name: match self.name() {
                    Ok(name) => name,
                    Err(e) => return e,
                },
                remote_url: match self.url() {
                    Ok(url) => url,
                    Err(e) => return e,
                },
                message: error.to_string(),
            })?;
        Ok(())
    }
}

pub fn clone_repo(
    remote: &Remote,
    path: &Path,
    worktree_setup: WorktreeSetup,
) -> Result<(), Error> {
    let clone_target = if worktree_setup.is_worktree() {
        path.join(worktree::GIT_MAIN_WORKTREE_DIRECTORY)
    } else {
        path.to_path_buf()
    };

    match remote.remote_type {
        RemoteType::Https | RemoteType::File => {
            let mut builder = git2::build::RepoBuilder::new();

            let fetchopts = git2::FetchOptions::new();

            builder.bare(worktree_setup.is_worktree());
            builder.fetch_options(fetchopts);

            builder.clone(remote.url.as_str(), clone_target.as_std_path())?;
        }
        RemoteType::Ssh => {
            let mut fo = git2::FetchOptions::new();
            fo.remote_callbacks(get_remote_callbacks());

            let mut builder = git2::build::RepoBuilder::new();
            builder.bare(worktree_setup.is_worktree());
            builder.fetch_options(fo);

            builder.clone(remote.url.as_str(), clone_target.as_std_path())?;
        }
    }

    let repo = RepoHandle::open(&clone_target)?;

    if worktree_setup.is_worktree() {
        repo.set_config_push(GitPushDefaultSetting::Upstream)?;
    }

    if remote.name != RemoteName::new("origin".to_owned()) {
        #[expect(clippy::missing_panics_doc, reason = "see expect() message")]
        let origin = repo
            .find_remote(&RemoteName::new("origin".to_owned()))?
            .expect("the remote will always exist after a successful clone");
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
        if local_branch_name.as_str() == "HEAD" {
            continue;
        }

        let mut local_branch = repo.create_branch(&local_branch_name, &remote_branch.commit()?)?;
        local_branch.set_upstream(&remote.name, &local_branch_name)?;
    }

    // If there is no head_branch, we most likely cloned an empty repository and
    // there is no point in setting any upstreams.
    if let Ok(mut active_branch) = repo.head_branch() {
        active_branch.set_upstream(&remote.name, &active_branch.name()?)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_ssh_remote() -> Result<(), Error> {
        assert_eq!(
            detect_remote_type(&RemoteUrl::new("ssh://git@example.com".to_owned()))?,
            RemoteType::Ssh
        );
        assert_eq!(
            detect_remote_type(&RemoteUrl::new("git@example.git".to_owned()))?,
            RemoteType::Ssh
        );
        Ok(())
    }

    #[test]
    fn check_https_remote() -> Result<(), Error> {
        assert_eq!(
            detect_remote_type(&RemoteUrl::new("https://example.com".to_owned()))?,
            RemoteType::Https
        );
        assert_eq!(
            detect_remote_type(&RemoteUrl::new("https://example.com/test.git".to_owned()))?,
            RemoteType::Https
        );
        Ok(())
    }

    #[test]
    fn check_file_remote() -> Result<(), Error> {
        assert_eq!(
            detect_remote_type(&RemoteUrl::new("file:///somedir".to_owned()))?,
            RemoteType::File
        );
        Ok(())
    }

    #[test]
    fn check_invalid_remotes() {
        assert!(matches!(
            detect_remote_type(&RemoteUrl::new("https//example.com".to_owned())),
            Err(Error::UnimplementedRemoteProtocol)
        ));
        assert!(matches!(
            detect_remote_type(&RemoteUrl::new("https:example.com".to_owned())),
            Err(Error::UnimplementedRemoteProtocol)
        ));
        assert!(matches!(
            detect_remote_type(&RemoteUrl::new("ssh//example.com".to_owned())),
            Err(Error::UnimplementedRemoteProtocol)
        ));
        assert!(matches!(
            detect_remote_type(&RemoteUrl::new("ssh:example.com".to_owned())),
            Err(Error::UnimplementedRemoteProtocol)
        ));
        assert!(matches!(
            detect_remote_type(&RemoteUrl::new("git@example.com".to_owned())),
            Err(Error::UnimplementedRemoteProtocol)
        ));
    }

    #[test]
    fn check_unsupported_protocol_http() {
        assert!(matches!(
            detect_remote_type(&RemoteUrl::new("http://example.com".to_owned())),
            Err(Error::UnsupportedHttpRemote)
        ));
    }

    #[test]
    fn check_unsupported_protocol_git() {
        assert!(matches!(
            detect_remote_type(&RemoteUrl::new("git://example.com".to_owned())),
            Err(Error::UnsupportedGitRemote)
        ));
    }

    #[test]
    fn repo_check_fullname() {
        let with_namespace = Repo {
            name: RepoName::new("name".to_owned()),
            namespace: Some(RepoNamespace::new("namespace".to_owned())),
            worktree_setup: WorktreeSetup::NoWorktree,
            remotes: Vec::new(),
        };

        let without_namespace = Repo {
            name: RepoName::new("name".to_owned()),
            namespace: None,
            worktree_setup: WorktreeSetup::NoWorktree,
            remotes: Vec::new(),
        };

        assert_eq!(
            with_namespace.fullname(),
            RepoName::new("namespace/name".to_owned())
        );
        assert_eq!(
            without_namespace.fullname(),
            RepoName::new("name".to_owned())
        );
    }
}
