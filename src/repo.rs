use serde::{Deserialize, Serialize};
use std::path::Path;

use git2::{Cred, RemoteCallbacks, Repository};

use crate::output::*;

const WORKTREE_CONFIG_FILE_NAME: &str = "grm.toml";

#[derive(Debug, Serialize, Deserialize, PartialEq)]
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

pub enum GitPushDefaultSetting {
    Upstream,
}

#[derive(Debug, PartialEq)]
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
pub struct WorktreeRootConfig {
    pub persistent_branches: Option<Vec<String>>,
}

pub fn read_worktree_root_config(worktree_root: &Path) -> Result<Option<WorktreeRootConfig>, String> {
    let path = worktree_root.join(WORKTREE_CONFIG_FILE_NAME);
    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            match e.kind() {
                std::io::ErrorKind::NotFound => return Ok(None),
                _ => return Err(format!("Error reading configuration file \"{}\": {}", path.display(), e)),
            }
        }
    };

    let config: WorktreeRootConfig = match toml::from_str(&content) {
        Ok(c) => c,
        Err(e) => {
            return Err(format!(
                "Error parsing configuration file \"{}\": {}",
                path.display(), e
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Remote {
    pub name: String,
    pub url: String,
    #[serde(rename = "type")]
    pub remote_type: RemoteType,
}

fn worktree_setup_default() -> bool {
    false
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepoConfig {
    pub name: String,

    #[serde(default = "worktree_setup_default")]
    pub worktree_setup: bool,

    pub remotes: Option<Vec<Remote>>,
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

pub struct Repo(git2::Repository);
pub struct Branch<'a>(git2::Branch<'a>);

fn convert_libgit2_error(error: git2::Error) -> String {
    error.message().to_string()
}

impl Repo {
    pub fn open(path: &Path, is_worktree: bool) -> Result<Self, RepoError> {
        let open_func = match is_worktree {
            true => Repository::open_bare,
            false => Repository::open,
        };
        let path = match is_worktree {
            true => path.join(crate::GIT_MAIN_WORKTREE_DIRECTORY),
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
            .find_local_branch(
                head
                    .shorthand()
                    .expect("Branch name is not valid utf-8"),
            )
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

    pub fn init(path: &Path, is_worktree: bool) -> Result<Self, String> {
        let repo = match is_worktree {
            false => Repository::init(path).map_err(convert_libgit2_error)?,
            true => Repository::init_bare(path.join(crate::GIT_MAIN_WORKTREE_DIRECTORY))
                .map_err(convert_libgit2_error)?,
        };

        let repo = Repo(repo);

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
            .set_bool(crate::GIT_CONFIG_BARE_KEY, value)
            .map_err(|error| format!("Could not set {}: {}", crate::GIT_CONFIG_BARE_KEY, error))
    }

    pub fn convert_to_worktree(&self, root_dir: &Path) -> Result<(), String> {
        std::fs::rename(".git", crate::GIT_MAIN_WORKTREE_DIRECTORY)
            .map_err(|error| format!("Error moving .git directory: {}", error))?;

        for entry in match std::fs::read_dir(&root_dir) {
            Ok(iterator) => iterator,
            Err(error) => {
                return Err(format!("Opening directory failed: {}", error));
            }
        } {
            match entry {
                Ok(entry) => {
                    let path = entry.path();
                    // unwrap is safe here, the path will ALWAYS have a file component
                    if path.file_name().unwrap() == crate::GIT_MAIN_WORKTREE_DIRECTORY {
                        continue;
                    }
                    if path.is_file() || path.is_symlink() {
                        if let Err(error) = std::fs::remove_file(&path) {
                            return Err(format!("Failed removing {}", error));
                        }
                    } else if let Err(error) = std::fs::remove_dir_all(&path) {
                        return Err(format!("Failed removing {}", error));
                    }
                }
                Err(error) => {
                    return Err(format!("Error getting directory entry: {}", error));
                }
            }
        }

        let worktree_repo = Repo::open(root_dir, true)
            .map_err(|error| format!("Opening newly converted repository failed: {}", error))?;

        worktree_repo
            .make_bare(true)
            .map_err(|error| format!("Error: {}", error))?;

        worktree_repo
            .set_config_push(GitPushDefaultSetting::Upstream)
            .map_err(|error| format!("Error: {}", error))?;

        Ok(())
    }

    pub fn set_config_push(&self, value: GitPushDefaultSetting) -> Result<(), String> {
        let mut config = self.config()?;

        config
            .set_str(
                crate::GIT_CONFIG_PUSH_DEFAULT,
                match value {
                    GitPushDefaultSetting::Upstream => "upstream",
                },
            )
            .map_err(|error| {
                format!(
                    "Could not set {}: {}",
                    crate::GIT_CONFIG_PUSH_DEFAULT,
                    error
                )
            })
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
            true => {
                return Err(String::from(
                    "Cannot get changes as this is a bare worktree repository",
                ))
            }
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

    pub fn default_branch(&self) -> Result<Branch, String> {
        match self.0.find_branch("main", git2::BranchType::Local) {
            Ok(branch) => Ok(Branch(branch)),
            Err(_) => match self.0.find_branch("master", git2::BranchType::Local) {
                Ok(branch) => Ok(Branch(branch)),
                Err(_) => Err(String::from("Could not determine default branch")),
            },
        }
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

    pub fn get_worktrees(&self) -> Result<Vec<String>, String> {
        Ok(self
            .0
            .worktrees()
            .map_err(convert_libgit2_error)?
            .iter()
            .map(|name| name.expect("Worktree name is invalid utf-8"))
            .map(|name| name.to_string())
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
        let worktree_repo = Repo::open(worktree_dir, false).map_err(|error| {
            WorktreeRemoveFailureReason::Error(format!("Error opening repo: {}", error))
        })?;

        let local_branch = worktree_repo.head_branch().map_err(|error| {
            WorktreeRemoveFailureReason::Error(format!("Failed getting head branch: {}", error))
        })?;

        let branch_name = local_branch.name().map_err(|error| {
            WorktreeRemoveFailureReason::Error(format!("Failed getting name of branch: {}", error))
        })?;

        if branch_name != name
            && !branch_name.ends_with(&format!("{}{}", crate::BRANCH_NAMESPACE_SEPARATOR, name))
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

        let default_branch = self
            .default_branch()
            .map_err(|error| format!("Failed getting default branch: {}", error))?;

        let default_branch_name = default_branch
            .name()
            .map_err(|error| format!("Failed getting default branch name: {}", error))?;

        let config = read_worktree_root_config(directory)?;

        for worktree in worktrees
            .iter()
            .filter(|worktree| *worktree != &default_branch_name)
            .filter(|worktree| match &config {
                None => true,
                Some(config) => match &config.persistent_branches {
                    None => true,
                    Some(branches) => !branches.contains(worktree),
                },
            })
        {
            let repo_dir = &directory.join(&worktree);
            if repo_dir.exists() {
                match self.remove_worktree(worktree, repo_dir, false, &config) {
                    Ok(_) => print_success(&format!("Worktree {} deleted", &worktree)),
                    Err(error) => match error {
                        WorktreeRemoveFailureReason::Changes(changes) => {
                            warnings.push(format!(
                                "Changes found in {}: {}, skipping",
                                &worktree, &changes
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
                warnings.push(format!("Worktree {} does not have a directory", &worktree));
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
            let dirname = crate::path_as_string(
                entry
                    .map_err(|error| error.to_string())?
                    .path()
                    .strip_prefix(&directory)
                    // that unwrap() is safe as each entry is
                    // guaranteed to be a subentry of &directory
                    .unwrap(),
            );

            let default_branch = self
                .default_branch()
                .map_err(|error| format!("Failed getting default branch: {}", error))?;

            let default_branch_name = default_branch
                .name()
                .map_err(|error| format!("Failed getting default branch name: {}", error))?;

            if dirname == crate::GIT_MAIN_WORKTREE_DIRECTORY {
                continue;
            }
            if dirname == WORKTREE_CONFIG_FILE_NAME {
                continue;
            }
            if dirname == default_branch_name {
                continue;
            }
            if !&worktrees.contains(&dirname) {
                unmanaged_worktrees.push(dirname);
            }
        }
        Ok(unmanaged_worktrees)
    }

    pub fn detect_worktree(path: &Path) -> bool {
        path.join(crate::GIT_MAIN_WORKTREE_DIRECTORY).exists()
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

    // only used internally in this module, exposes libgit2 details
    fn as_reference(&self) -> &git2::Reference {
        self.0.get()
    }
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

    pub fn push(
        &mut self,
        local_branch_name: &str,
        remote_branch_name: &str,
        _repo: &Repo,
    ) -> Result<(), String> {
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
            git2::Cred::ssh_key_from_agent(username_from_url.unwrap())
        });

        let mut push_options = git2::PushOptions::new();
        push_options.remote_callbacks(callbacks);

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
        true => path.join(crate::GIT_MAIN_WORKTREE_DIRECTORY),
    };

    print_action(&format!(
        "Cloning into \"{}\" from \"{}\"",
        &clone_target.display(),
        &remote.url
    ));
    match remote.remote_type {
        RemoteType::Https | RemoteType::File => {
            let mut builder = git2::build::RepoBuilder::new();
            builder.bare(is_worktree);
            builder.clone(&remote.url, &clone_target)?;
        }
        RemoteType::Ssh => {
            let mut callbacks = RemoteCallbacks::new();
            callbacks.credentials(|_url, username_from_url, _allowed_types| {
                Cred::ssh_key_from_agent(username_from_url.unwrap())
            });

            let mut fo = git2::FetchOptions::new();
            fo.remote_callbacks(callbacks);

            let mut builder = git2::build::RepoBuilder::new();
            builder.bare(is_worktree);
            builder.fetch_options(fo);

            builder.clone(&remote.url, &clone_target)?;
        }
    }

    if is_worktree {
        let repo = Repo::open(&clone_target, false)?;
        repo.set_config_push(GitPushDefaultSetting::Upstream)?;
    }

    Ok(())
}
