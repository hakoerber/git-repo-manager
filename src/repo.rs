use serde::{Deserialize, Serialize};
use std::path::Path;

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

pub enum WorktreeRemoveFailureReason {
    Changes(String),
    Error(String),
    NotMerged(String),
}

pub enum WorktreeConversionFailureReason {
    Changes,
    Ignored,
    Error(String),
}

pub enum GitPushDefaultSetting {
    Upstream,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RepoErrorKind {
    NotFound,
    Unknown(String),
}

#[derive(Debug)]
pub struct RepoError {
    pub kind: RepoErrorKind,
}

impl RepoError {
    fn new(kind: RepoErrorKind) -> RepoError {
        RepoError { kind }
    }
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

pub fn read_worktree_root_config(
    worktree_root: &Path,
) -> Result<Option<WorktreeRootConfig>, String> {
    let path = worktree_root.join(WORKTREE_CONFIG_FILE_NAME);
    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => return Ok(None),
            _ => {
                return Err(format!(
                    "Error reading configuration file \"{}\": {}",
                    path.display(),
                    e
                ))
            }
        },
    };

    let config: WorktreeRootConfig = match toml::from_str(&content) {
        Ok(c) => c,
        Err(e) => {
            return Err(format!(
                "Error parsing configuration file \"{}\": {}",
                path.display(),
                e
            ))
        }
    };

    Ok(Some(config))
}

impl std::error::Error for RepoError {}

impl std::fmt::Display for RepoError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self.kind)
    }
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
        self.namespace = None
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

    pub fn forward_branch(&self, rebase: bool, stash: bool) -> Result<Option<String>, String> {
        let repo = RepoHandle::open(Path::new(&self.name), false)
            .map_err(|error| format!("Error opening worktree: {}", error))?;

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

            let unstash = || -> Result<(), String> {
                if stashed_changes {
                    repo.stash_pop()?;
                }
                Ok(())
            };

            let remote_annotated_commit = repo
                .0
                .find_annotated_commit(remote_branch.commit()?.id().0)
                .map_err(convert_libgit2_error)?;

            if rebase {
                let mut rebase = repo
                    .0
                    .rebase(
                        None, // use HEAD
                        Some(&remote_annotated_commit),
                        None, // figure out the base yourself, libgit2!
                        Some(&mut git2::RebaseOptions::new()),
                    )
                    .map_err(convert_libgit2_error)?;

                while let Some(operation) = rebase.next() {
                    let operation = operation.map_err(convert_libgit2_error)?;

                    // This is required to preserve the commiter of the rebased
                    // commits, which is the expected behaviour.
                    let rebased_commit = repo
                        .0
                        .find_commit(operation.id())
                        .map_err(convert_libgit2_error)?;
                    let committer = rebased_commit.committer();

                    // This is effectively adding all files to the index explicitly.
                    // Normal files are already staged, but changed submodules are not.
                    let mut index = repo.0.index().map_err(convert_libgit2_error)?;
                    index
                        .add_all(["."].iter(), git2::IndexAddOption::CHECK_PATHSPEC, None)
                        .map_err(convert_libgit2_error)?;

                    if let Err(error) = rebase.commit(None, &committer, None) {
                        if error.code() == git2::ErrorCode::Applied {
                            continue;
                        }
                        rebase.abort().map_err(convert_libgit2_error)?;
                        unstash()?;
                        return Err(convert_libgit2_error(error));
                    }
                }

                rebase.finish(None).map_err(convert_libgit2_error)?;
            } else {
                let (analysis, _preference) = repo
                    .0
                    .merge_analysis(&[&remote_annotated_commit])
                    .map_err(convert_libgit2_error)?;

                if analysis.is_up_to_date() {
                    unstash()?;
                    return Ok(None);
                }
                if !analysis.is_fast_forward() {
                    unstash()?;
                    return Ok(Some(String::from("Worktree cannot be fast forwarded")));
                }

                repo.0
                    .reset(
                        remote_branch.commit()?.0.as_object(),
                        git2::ResetType::Hard,
                        Some(git2::build::CheckoutBuilder::new().safe()),
                    )
                    .map_err(convert_libgit2_error)?;
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
    ) -> Result<Option<String>, String> {
        let repo = RepoHandle::open(Path::new(&self.name), false)
            .map_err(|error| format!("Error opening worktree: {}", error))?;

        let guess_default_branch = || {
            repo.default_branch()
                .map_err(|_| "Could not determine default branch")?
                .name()
                .map_err(|error| format!("Failed getting default branch name: {}", error))
        };

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

        let unstash = || -> Result<(), String> {
            if stashed_changes {
                repo.stash_pop()?;
            }
            Ok(())
        };

        let base_branch = repo.find_local_branch(&default_branch_name)?;
        let base_annotated_commit = repo
            .0
            .find_annotated_commit(base_branch.commit()?.id().0)
            .map_err(convert_libgit2_error)?;

        let mut rebase = repo
            .0
            .rebase(
                None, // use HEAD
                Some(&base_annotated_commit),
                None, // figure out the base yourself, libgit2!
                Some(&mut git2::RebaseOptions::new()),
            )
            .map_err(convert_libgit2_error)?;

        while let Some(operation) = rebase.next() {
            let operation = operation.map_err(convert_libgit2_error)?;

            // This is required to preserve the commiter of the rebased
            // commits, which is the expected behaviour.
            let rebased_commit = repo
                .0
                .find_commit(operation.id())
                .map_err(convert_libgit2_error)?;
            let committer = rebased_commit.committer();

            // This is effectively adding all files to the index explicitly.
            // Normal files are already staged, but changed submodules are not.
            let mut index = repo.0.index().map_err(convert_libgit2_error)?;
            index
                .add_all(["."].iter(), git2::IndexAddOption::CHECK_PATHSPEC, None)
                .map_err(convert_libgit2_error)?;

            if let Err(error) = rebase.commit(None, &committer, None) {
                if error.code() == git2::ErrorCode::Applied {
                    continue;
                }
                rebase.abort().map_err(convert_libgit2_error)?;
                unstash()?;
                return Err(convert_libgit2_error(error));
            }
        }

        rebase.finish(None).map_err(convert_libgit2_error)?;
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
    fn check_file_remote() {
        assert_eq!(
            detect_remote_type("file:///somedir"),
            Some(RemoteType::File)
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
    if remote_url.starts_with("file://") {
        return Some(RemoteType::File);
    }
    if remote_url.starts_with("http://") {
        unimplemented!("Remotes using HTTP protocol are not supported");
    }
    if remote_url.starts_with("git://") {
        unimplemented!("Remotes using git protocol are not supported");
    }
    None
}

pub struct RepoHandle(git2::Repository);
pub struct Branch<'a>(git2::Branch<'a>);

fn convert_libgit2_error(error: git2::Error) -> String {
    error.message().to_string()
}

impl RepoHandle {
    pub fn open(path: &Path, is_worktree: bool) -> Result<Self, RepoError> {
        let open_func = match is_worktree {
            true => Repository::open_bare,
            false => Repository::open,
        };
        let path = match is_worktree {
            true => path.join(worktree::GIT_MAIN_WORKTREE_DIRECTORY),
            false => path.to_path_buf(),
        };
        match open_func(path) {
            Ok(r) => Ok(Self(r)),
            Err(e) => match e.code() {
                git2::ErrorCode::NotFound => Err(RepoError::new(RepoErrorKind::NotFound)),
                _ => Err(RepoError::new(RepoErrorKind::Unknown(
                    convert_libgit2_error(e),
                ))),
            },
        }
    }

    pub fn stash(&self) -> Result<(), String> {
        let head_branch = self.head_branch()?;
        let head = head_branch.commit()?;
        let author = head.author();

        // This is honestly quite horrible. The problem is that all stash operations expect a
        // mutable reference (as they, well, mutate the repo after all). But we are heavily using
        // immutable references a lot with this struct. I'm really not sure how to best solve this.
        // Right now, we just open the repo AGAIN. It is safe, as we are only accessing the stash
        // with the second reference, so there are no cross effects. But it just smells. Also,
        // using `unwrap()` here as we are already sure that the repo is openable(?).
        let mut repo = RepoHandle::open(self.0.path(), false).unwrap();
        repo.0
            .stash_save2(&author, None, Some(git2::StashFlags::INCLUDE_UNTRACKED))
            .map_err(convert_libgit2_error)?;
        Ok(())
    }

    pub fn stash_pop(&self) -> Result<(), String> {
        let mut repo = RepoHandle::open(self.0.path(), false).unwrap();
        repo.0
            .stash_pop(
                0,
                Some(git2::StashApplyOptions::new().reinstantiate_index()),
            )
            .map_err(convert_libgit2_error)?;
        Ok(())
    }

    pub fn rename_remote(&self, remote: &RemoteHandle, new_name: &str) -> Result<(), String> {
        let failed_refspecs = self
            .0
            .remote_rename(&remote.name(), new_name)
            .map_err(convert_libgit2_error)?;

        if !failed_refspecs.is_empty() {
            return Err(String::from(
                "Some non-default refspecs could not be renamed",
            ));
        }

        Ok(())
    }

    pub fn graph_ahead_behind(
        &self,
        local_branch: &Branch,
        remote_branch: &Branch,
    ) -> Result<(usize, usize), String> {
        self.0
            .graph_ahead_behind(
                local_branch.commit()?.id().0,
                remote_branch.commit()?.id().0,
            )
            .map_err(convert_libgit2_error)
    }

    pub fn head_branch(&self) -> Result<Branch, String> {
        let head = self.0.head().map_err(convert_libgit2_error)?;
        if !head.is_branch() {
            return Err(String::from("No branch checked out"));
        }
        // unwrap() is safe here, as we can be certain that a branch with that
        // name exists
        let branch = self
            .find_local_branch(head.shorthand().expect("Branch name is not valid utf-8"))
            .unwrap();
        Ok(branch)
    }

    pub fn remote_set_url(&self, name: &str, url: &str) -> Result<(), String> {
        self.0
            .remote_set_url(name, url)
            .map_err(convert_libgit2_error)
    }

    pub fn remote_delete(&self, name: &str) -> Result<(), String> {
        self.0.remote_delete(name).map_err(convert_libgit2_error)
    }

    pub fn is_empty(&self) -> Result<bool, String> {
        self.0.is_empty().map_err(convert_libgit2_error)
    }

    pub fn is_bare(&self) -> bool {
        self.0.is_bare()
    }

    pub fn new_worktree(
        &self,
        name: &str,
        directory: &Path,
        target_branch: &Branch,
    ) -> Result<(), String> {
        self.0
            .worktree(
                name,
                directory,
                Some(git2::WorktreeAddOptions::new().reference(Some(target_branch.as_reference()))),
            )
            .map_err(convert_libgit2_error)?;
        Ok(())
    }

    pub fn remotes(&self) -> Result<Vec<String>, String> {
        Ok(self
            .0
            .remotes()
            .map_err(convert_libgit2_error)?
            .iter()
            .map(|name| name.expect("Remote name is invalid utf-8"))
            .map(|name| name.to_owned())
            .collect())
    }

    pub fn new_remote(&self, name: &str, url: &str) -> Result<(), String> {
        self.0.remote(name, url).map_err(convert_libgit2_error)?;
        Ok(())
    }

    pub fn fetchall(&self) -> Result<(), String> {
        for remote in self.remotes()? {
            self.fetch(&remote)?;
        }
        Ok(())
    }

    pub fn local_branches(&self) -> Result<Vec<Branch>, String> {
        self.0
            .branches(Some(git2::BranchType::Local))
            .map_err(convert_libgit2_error)?
            .map(|branch| Ok(Branch(branch.map_err(convert_libgit2_error)?.0)))
            .collect::<Result<Vec<Branch>, String>>()
    }

    pub fn remote_branches(&self) -> Result<Vec<Branch>, String> {
        self.0
            .branches(Some(git2::BranchType::Remote))
            .map_err(convert_libgit2_error)?
            .map(|branch| Ok(Branch(branch.map_err(convert_libgit2_error)?.0)))
            .collect::<Result<Vec<Branch>, String>>()
    }

    pub fn fetch(&self, remote_name: &str) -> Result<(), String> {
        let mut remote = self
            .0
            .find_remote(remote_name)
            .map_err(convert_libgit2_error)?;

        let mut fetch_options = git2::FetchOptions::new();
        fetch_options.remote_callbacks(get_remote_callbacks());

        for refspec in &remote.fetch_refspecs().map_err(convert_libgit2_error)? {
            remote
                .fetch(
                    &[refspec.ok_or("Remote name is invalid utf-8")?],
                    Some(&mut fetch_options),
                    None,
                )
                .map_err(convert_libgit2_error)?;
        }
        Ok(())
    }

    pub fn init(path: &Path, is_worktree: bool) -> Result<Self, String> {
        let repo = match is_worktree {
            false => Repository::init(path).map_err(convert_libgit2_error)?,
            true => Repository::init_bare(path.join(worktree::GIT_MAIN_WORKTREE_DIRECTORY))
                .map_err(convert_libgit2_error)?,
        };

        let repo = RepoHandle(repo);

        if is_worktree {
            repo.set_config_push(GitPushDefaultSetting::Upstream)?;
        }

        Ok(repo)
    }

    pub fn config(&self) -> Result<git2::Config, String> {
        self.0.config().map_err(convert_libgit2_error)
    }

    pub fn find_worktree(&self, name: &str) -> Result<(), String> {
        self.0.find_worktree(name).map_err(convert_libgit2_error)?;
        Ok(())
    }

    pub fn prune_worktree(&self, name: &str) -> Result<(), String> {
        let worktree = self.0.find_worktree(name).map_err(convert_libgit2_error)?;
        worktree.prune(None).map_err(convert_libgit2_error)?;
        Ok(())
    }

    pub fn find_remote_branch(
        &self,
        remote_name: &str,
        branch_name: &str,
    ) -> Result<Branch, String> {
        Ok(Branch(
            self.0
                .find_branch(
                    &format!("{}/{}", remote_name, branch_name),
                    git2::BranchType::Remote,
                )
                .map_err(convert_libgit2_error)?,
        ))
    }

    pub fn find_local_branch(&self, name: &str) -> Result<Branch, String> {
        Ok(Branch(
            self.0
                .find_branch(name, git2::BranchType::Local)
                .map_err(convert_libgit2_error)?,
        ))
    }

    pub fn create_branch(&self, name: &str, target: &Commit) -> Result<Branch, String> {
        Ok(Branch(
            self.0
                .branch(name, &target.0, false)
                .map_err(convert_libgit2_error)?,
        ))
    }

    pub fn make_bare(&self, value: bool) -> Result<(), String> {
        let mut config = self.config()?;

        config
            .set_bool(GIT_CONFIG_BARE_KEY, value)
            .map_err(|error| format!("Could not set {}: {}", GIT_CONFIG_BARE_KEY, error))
    }

    pub fn convert_to_worktree(
        &self,
        root_dir: &Path,
    ) -> Result<(), WorktreeConversionFailureReason> {
        if self
            .status(false)
            .map_err(WorktreeConversionFailureReason::Error)?
            .changes
            .is_some()
        {
            return Err(WorktreeConversionFailureReason::Changes);
        }

        if self
            .has_untracked_files(false)
            .map_err(WorktreeConversionFailureReason::Error)?
        {
            return Err(WorktreeConversionFailureReason::Ignored);
        }

        std::fs::rename(".git", worktree::GIT_MAIN_WORKTREE_DIRECTORY).map_err(|error| {
            WorktreeConversionFailureReason::Error(format!(
                "Error moving .git directory: {}",
                error
            ))
        })?;

        for entry in match std::fs::read_dir(&root_dir) {
            Ok(iterator) => iterator,
            Err(error) => {
                return Err(WorktreeConversionFailureReason::Error(format!(
                    "Opening directory failed: {}",
                    error
                )));
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
                            return Err(WorktreeConversionFailureReason::Error(format!(
                                "Failed removing {}",
                                error
                            )));
                        }
                    } else if let Err(error) = std::fs::remove_dir_all(&path) {
                        return Err(WorktreeConversionFailureReason::Error(format!(
                            "Failed removing {}",
                            error
                        )));
                    }
                }
                Err(error) => {
                    return Err(WorktreeConversionFailureReason::Error(format!(
                        "Error getting directory entry: {}",
                        error
                    )));
                }
            }
        }

        let worktree_repo = RepoHandle::open(root_dir, true).map_err(|error| {
            WorktreeConversionFailureReason::Error(format!(
                "Opening newly converted repository failed: {}",
                error
            ))
        })?;

        worktree_repo
            .make_bare(true)
            .map_err(|error| WorktreeConversionFailureReason::Error(format!("Error: {}", error)))?;

        worktree_repo
            .set_config_push(GitPushDefaultSetting::Upstream)
            .map_err(|error| WorktreeConversionFailureReason::Error(format!("Error: {}", error)))?;

        Ok(())
    }

    pub fn set_config_push(&self, value: GitPushDefaultSetting) -> Result<(), String> {
        let mut config = self.config()?;

        config
            .set_str(
                GIT_CONFIG_PUSH_DEFAULT,
                match value {
                    GitPushDefaultSetting::Upstream => "upstream",
                },
            )
            .map_err(|error| format!("Could not set {}: {}", GIT_CONFIG_PUSH_DEFAULT, error))
    }

    pub fn has_untracked_files(&self, is_worktree: bool) -> Result<bool, String> {
        match is_worktree {
            true => Err(String::from(
                "Cannot get changes as this is a bare worktree repository",
            )),
            false => {
                let statuses = self
                    .0
                    .statuses(Some(git2::StatusOptions::new().include_ignored(true)))
                    .map_err(convert_libgit2_error)?;

                match statuses.is_empty() {
                    true => Ok(false),
                    false => {
                        for status in statuses.iter() {
                            let status_bits = status.status();
                            if status_bits.intersects(git2::Status::IGNORED) {
                                return Ok(true);
                            }
                        }
                        Ok(false)
                    }
                }
            }
        }
    }

    pub fn status(&self, is_worktree: bool) -> Result<RepoStatus, String> {
        let operation = match self.0.state() {
            git2::RepositoryState::Clean => None,
            state => Some(state),
        };

        let empty = self.is_empty()?;

        let remotes = self
            .0
            .remotes()
            .map_err(convert_libgit2_error)?
            .iter()
            .map(|repo_name| repo_name.expect("Worktree name is invalid utf-8."))
            .map(|repo_name| repo_name.to_owned())
            .collect::<Vec<String>>();

        let head = match is_worktree {
            true => None,
            false => match empty {
                true => None,
                false => Some(self.head_branch()?.name()?),
            },
        };

        let changes = match is_worktree {
            true => None,
            false => {
                let statuses = self
                    .0
                    .statuses(Some(
                        git2::StatusOptions::new()
                            .include_ignored(false)
                            .include_untracked(true),
                    ))
                    .map_err(convert_libgit2_error)?;

                match statuses.is_empty() {
                    true => None,
                    false => {
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
                            } else if status_bits
                                .intersects(git2::Status::INDEX_NEW | git2::Status::WT_NEW)
                            {
                                files_new += 1;
                            } else if status_bits
                                .intersects(git2::Status::INDEX_DELETED | git2::Status::WT_DELETED)
                            {
                                files_deleted += 1;
                            }
                        }
                        if (files_new, files_modified, files_deleted) == (0, 0, 0) {
                            panic!(
                                "is_empty() returned true, but no file changes were detected. This is a bug!"
                            );
                        }
                        Some(RepoChanges {
                            files_new,
                            files_modified,
                            files_deleted,
                        })
                    }
                }
            }
        };

        let worktrees = self.0.worktrees().unwrap().len();

        let submodules = match is_worktree {
            true => None,
            false => {
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
            }
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

    pub fn get_remote_default_branch(&self, remote_name: &str) -> Result<Option<Branch>, String> {
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
                    pointer_name.strip_prefix(&format!("refs/remotes/{}/", remote_name))
                {
                    return Ok(Some(self.find_local_branch(local_branch_name)?));
                } else {
                    eprintln!("Remote HEAD ({}) pointer is invalid", pointer_name);
                }
            } else {
                eprintln!("Remote HEAD does not point to a symbolic target");
            }
        }
        Ok(None)
    }

    pub fn default_branch(&self) -> Result<Branch, String> {
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
                    default_branches.push(default_branch)
                }
            }

            if !default_branches.is_empty()
                && (default_branches.len() == 1
                    || default_branches
                        .windows(2)
                        .all(|w| w[0].name() == w[1].name()))
            {
                return Ok(default_branches.remove(0));
            }
        }

        for branch_name in &vec!["main", "master"] {
            if let Ok(branch) = self.0.find_branch(branch_name, git2::BranchType::Local) {
                return Ok(Branch(branch));
            }
        }

        Err(String::from("Could not determine default branch"))
    }

    // Looks like there is no distinguishing between the error cases
    // "no such remote" and "failed to get remote for some reason".
    // May be a good idea to handle this explicitly, by returning a
    // Result<Option<RemoteHandle>, String> instead, Returning Ok(None)
    // on "not found" and Err() on an actual error.
    pub fn find_remote(&self, remote_name: &str) -> Result<Option<RemoteHandle>, String> {
        let remotes = self.0.remotes().map_err(convert_libgit2_error)?;

        if !remotes
            .iter()
            .any(|remote| remote.expect("Remote name is invalid utf-8") == remote_name)
        {
            return Ok(None);
        }

        Ok(Some(RemoteHandle(
            self.0
                .find_remote(remote_name)
                .map_err(convert_libgit2_error)?,
        )))
    }

    pub fn get_worktrees(&self) -> Result<Vec<Worktree>, String> {
        Ok(self
            .0
            .worktrees()
            .map_err(convert_libgit2_error)?
            .iter()
            .map(|name| name.expect("Worktree name is invalid utf-8"))
            .map(Worktree::new)
            .collect())
    }

    pub fn remove_worktree(
        &self,
        name: &str,
        worktree_dir: &Path,
        force: bool,
        worktree_config: &Option<WorktreeRootConfig>,
    ) -> Result<(), WorktreeRemoveFailureReason> {
        if !worktree_dir.exists() {
            return Err(WorktreeRemoveFailureReason::Error(format!(
                "{} does not exist",
                name
            )));
        }
        let worktree_repo = RepoHandle::open(worktree_dir, false).map_err(|error| {
            WorktreeRemoveFailureReason::Error(format!("Error opening repo: {}", error))
        })?;

        let local_branch = worktree_repo.head_branch().map_err(|error| {
            WorktreeRemoveFailureReason::Error(format!("Failed getting head branch: {}", error))
        })?;

        let branch_name = local_branch.name().map_err(|error| {
            WorktreeRemoveFailureReason::Error(format!("Failed getting name of branch: {}", error))
        })?;

        if branch_name != name
            && !branch_name.ends_with(&format!("{}{}", super::BRANCH_NAMESPACE_SEPARATOR, name))
        {
            return Err(WorktreeRemoveFailureReason::Error(format!(
                "Branch {} is checked out in worktree, this does not look correct",
                &branch_name
            )));
        }

        let branch = worktree_repo
            .find_local_branch(&branch_name)
            .map_err(WorktreeRemoveFailureReason::Error)?;

        if !force {
            let status = worktree_repo
                .status(false)
                .map_err(WorktreeRemoveFailureReason::Error)?;
            if status.changes.is_some() {
                return Err(WorktreeRemoveFailureReason::Changes(String::from(
                    "Changes found in worktree",
                )));
            }

            let mut is_merged_into_persistent_branch = false;
            let mut has_persistent_branches = false;
            if let Some(config) = worktree_config {
                if let Some(branches) = &config.persistent_branches {
                    has_persistent_branches = true;
                    for persistent_branch in branches {
                        let persistent_branch = worktree_repo
                            .find_local_branch(persistent_branch)
                            .map_err(WorktreeRemoveFailureReason::Error)?;

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
                return Err(WorktreeRemoveFailureReason::NotMerged(format!(
                    "Branch {} is not merged into any persistent branches",
                    name
                )));
            }

            if !has_persistent_branches {
                match branch.upstream() {
                    Ok(remote_branch) => {
                        let (ahead, behind) = worktree_repo
                            .graph_ahead_behind(&branch, &remote_branch)
                            .unwrap();

                        if (ahead, behind) != (0, 0) {
                            return Err(WorktreeRemoveFailureReason::Changes(format!(
                                "Branch {} is not in line with remote branch",
                                name
                            )));
                        }
                    }
                    Err(_) => {
                        return Err(WorktreeRemoveFailureReason::Changes(format!(
                            "No remote tracking branch for branch {} found",
                            name
                        )));
                    }
                }
            }
        }

        if let Err(e) = std::fs::remove_dir_all(&worktree_dir) {
            return Err(WorktreeRemoveFailureReason::Error(format!(
                "Error deleting {}: {}",
                &worktree_dir.display(),
                e
            )));
        }
        self.prune_worktree(name)
            .map_err(WorktreeRemoveFailureReason::Error)?;
        branch
            .delete()
            .map_err(WorktreeRemoveFailureReason::Error)?;

        Ok(())
    }

    pub fn cleanup_worktrees(&self, directory: &Path) -> Result<Vec<String>, String> {
        let mut warnings = Vec::new();

        let worktrees = self
            .get_worktrees()
            .map_err(|error| format!("Getting worktrees failed: {}", error))?;

        let config = read_worktree_root_config(directory)?;

        let guess_default_branch = || {
            self.default_branch()
                .map_err(|_| "Could not determine default branch")?
                .name()
                .map_err(|error| format!("Failed getting default branch name: {}", error))
        };

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
            let repo_dir = &directory.join(&worktree.name());
            if repo_dir.exists() {
                match self.remove_worktree(worktree.name(), repo_dir, false, &config) {
                    Ok(_) => print_success(&format!("Worktree {} deleted", &worktree.name())),
                    Err(error) => match error {
                        WorktreeRemoveFailureReason::Changes(changes) => {
                            warnings.push(format!(
                                "Changes found in {}: {}, skipping",
                                &worktree.name(),
                                &changes
                            ));
                            continue;
                        }
                        WorktreeRemoveFailureReason::NotMerged(message) => {
                            warnings.push(message);
                            continue;
                        }
                        WorktreeRemoveFailureReason::Error(error) => {
                            return Err(error);
                        }
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

    pub fn find_unmanaged_worktrees(&self, directory: &Path) -> Result<Vec<String>, String> {
        let worktrees = self
            .get_worktrees()
            .map_err(|error| format!("Getting worktrees failed: {}", error))?;

        let mut unmanaged_worktrees = Vec::new();
        for entry in std::fs::read_dir(&directory).map_err(|error| error.to_string())? {
            let dirname = path::path_as_string(
                entry
                    .map_err(|error| error.to_string())?
                    .path()
                    .strip_prefix(&directory)
                    // that unwrap() is safe as each entry is
                    // guaranteed to be a subentry of &directory
                    .unwrap(),
            );

            let config = read_worktree_root_config(directory)?;

            let guess_default_branch = || {
                self.default_branch()
                    .map_err(|error| format!("Failed getting default branch: {}", error))?
                    .name()
                    .map_err(|error| format!("Failed getting default branch name: {}", error))
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
pub struct Reference<'a>(git2::Reference<'a>);
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

    pub(self) fn author(&self) -> git2::Signature {
        self.0.author()
    }
}

impl<'a> Branch<'a> {
    pub fn to_commit(self) -> Result<Commit<'a>, String> {
        Ok(Commit(
            self.0
                .into_reference()
                .peel_to_commit()
                .map_err(convert_libgit2_error)?,
        ))
    }
}

impl Branch<'_> {
    pub fn commit(&self) -> Result<Commit, String> {
        Ok(Commit(
            self.0
                .get()
                .peel_to_commit()
                .map_err(convert_libgit2_error)?,
        ))
    }

    pub fn set_upstream(&mut self, remote_name: &str, branch_name: &str) -> Result<(), String> {
        self.0
            .set_upstream(Some(&format!("{}/{}", remote_name, branch_name)))
            .map_err(convert_libgit2_error)?;
        Ok(())
    }

    pub fn name(&self) -> Result<String, String> {
        self.0
            .name()
            .map(|name| name.expect("Branch name is invalid utf-8"))
            .map_err(convert_libgit2_error)
            .map(|name| name.to_string())
    }

    pub fn upstream(&self) -> Result<Branch, String> {
        Ok(Branch(self.0.upstream().map_err(convert_libgit2_error)?))
    }

    pub fn delete(mut self) -> Result<(), String> {
        self.0.delete().map_err(convert_libgit2_error)
    }

    pub fn basename(&self) -> Result<String, String> {
        let name = self.name()?;
        if let Some((_prefix, basename)) = name.split_once('/') {
            Ok(basename.to_string())
        } else {
            Ok(name)
        }
    }

    // only used internally in this module, exposes libgit2 details
    fn as_reference(&self) -> &git2::Reference {
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
        let username = match username_from_url {
            Some(username) => username,
            None => panic!("Could not get username. This is a bug"),
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

    pub fn default_branch(&self) -> Result<String, String> {
        Ok(self
            .0
            .default_branch()
            .map_err(convert_libgit2_error)?
            .as_str()
            .expect("Remote branch name is not valid utf-8")
            .to_string())
    }

    pub fn is_pushable(&self) -> Result<bool, String> {
        let remote_type = detect_remote_type(self.0.url().expect("Remote name is not valid utf-8"))
            .ok_or_else(|| String::from("Could not detect remote type"))?;
        Ok(matches!(remote_type, RemoteType::Ssh | RemoteType::File))
    }

    pub fn push(
        &mut self,
        local_branch_name: &str,
        remote_branch_name: &str,
        _repo: &RepoHandle,
    ) -> Result<(), String> {
        if !self.is_pushable()? {
            return Err(String::from("Trying to push to a non-pushable remote"));
        }

        let mut push_options = git2::PushOptions::new();
        push_options.remote_callbacks(get_remote_callbacks());

        let push_refspec = format!(
            "+refs/heads/{}:refs/heads/{}",
            local_branch_name, remote_branch_name
        );
        self.0
            .push(&[push_refspec], Some(&mut push_options))
            .map_err(|error| {
                format!(
                    "Pushing {} to {} ({}) failed: {}",
                    local_branch_name,
                    self.name(),
                    self.url(),
                    error
                )
            })?;
        Ok(())
    }
}

pub fn clone_repo(
    remote: &Remote,
    path: &Path,
    is_worktree: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let clone_target = match is_worktree {
        false => path.to_path_buf(),
        true => path.join(worktree::GIT_MAIN_WORKTREE_DIRECTORY),
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
