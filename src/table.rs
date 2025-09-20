use std::{
    fmt::{self, Write},
    path::{Path, PathBuf},
};

use comfy_table::{Cell, Table};
use thiserror::Error;

use super::{
    config, path,
    repo::{
        self,
        worktree::{WorktreeName, WorktreeSetup},
    },
    repo::{ProjectName, RepoHandle},
    tree,
};

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Config(#[from] config::Error),
    #[error("repo error: {0}")]
    Repo(#[from] repo::Error),
    #[error("Directory is not a git directory")]
    NotAGitDirectory,
    #[error("Worktree {:?} does not have a directory", .worktree)]
    WorktreeWithoutDirectory { worktree: WorktreeName },
    #[error(transparent)]
    Path(#[from] path::Error),
    #[error(transparent)]
    Fmt(#[from] fmt::Error),
    #[error("Found {:?}, which is not a valid worktree directory!", .path)]
    InvalidWorktreeDirectory { path: PathBuf },
    #[error("{}: Repository does not exist. Run sync?", .name)]
    RepoDoesNotExist { name: ProjectName },
    #[error("{}: No git repository found. Run sync?", .name)]
    RepoNotGit { name: ProjectName },
    #[error("{}: Opening repository failed: {}", .name, .message)]
    RepoOpenFailed { name: ProjectName, message: String },
    #[error("{}: Couldn't add repo status: {}", .name, .message)]
    RepoStatusFailed { name: ProjectName, message: String },
}

fn add_table_header(table: &mut Table) {
    table
        .load_preset(comfy_table::presets::UTF8_FULL)
        .apply_modifier(comfy_table::modifiers::UTF8_ROUND_CORNERS)
        .set_header([
            Cell::new("Repo"),
            Cell::new("Worktree"),
            Cell::new("Status"),
            Cell::new("Branches"),
            Cell::new("HEAD"),
            Cell::new("Remotes"),
        ]);
}

fn add_repo_status(
    table: &mut Table,
    repo_name: Option<&ProjectName>,
    repo_handle: &RepoHandle,
    worktree_setup: WorktreeSetup,
) -> Result<(), Error> {
    let repo_status = repo_handle.status(worktree_setup).map_err(Error::Repo)?;

    let branch_info = {
        let mut acc = String::new();
        for (branch_name, remote_branch) in repo_status.branches {
            writeln!(
                &mut acc,
                "branch: {}{}",
                &branch_name,
                &match remote_branch {
                    None => String::from(" <!local>"),
                    Some((remote_branch_name, remote_tracking_status)) => {
                        format!(
                            " <{}>{}",
                            remote_branch_name,
                            &match remote_tracking_status {
                                repo::RemoteTrackingStatus::UpToDate => String::from(" \u{2714}"),
                                repo::RemoteTrackingStatus::Ahead(d) => format!(" [+{}]", &d),
                                repo::RemoteTrackingStatus::Behind(d) => format!(" [-{}]", &d),
                                repo::RemoteTrackingStatus::Diverged(d1, d2) =>
                                    format!(" [+{}/-{}]", &d1, &d2),
                            }
                        )
                    }
                }
            )?;
        }
        acc.trim().to_owned()
    };

    let remote_status = {
        let mut acc = String::new();
        for remote in repo_status.remotes {
            writeln!(&mut acc, "{remote}")?;
        }

        acc.trim().to_owned()
    };

    table.add_row([
        match repo_name {
            Some(name) => name.as_str(),
            None => "unknown",
        },
        if worktree_setup.is_worktree() {
            "\u{2714}"
        } else {
            ""
        },
        &if worktree_setup.is_worktree() {
            String::new()
        } else {
            match repo_status.changes {
                Some(changes) => {
                    let mut out = Vec::new();
                    if changes.files_new > 0 {
                        out.push(format!("New: {}\n", changes.files_new));
                    }
                    if changes.files_modified > 0 {
                        out.push(format!("Modified: {}\n", changes.files_modified));
                    }
                    if changes.files_deleted > 0 {
                        out.push(format!("Deleted: {}\n", changes.files_deleted));
                    }
                    out.into_iter().collect::<String>().trim().to_owned()
                }
                None => String::from("\u{2714}"),
            }
        },
        &branch_info,
        &if worktree_setup.is_worktree() {
            String::new()
        } else {
            match repo_status.head {
                Some(head) => head.into_string(),
                None => String::from("Empty"),
            }
        },
        &remote_status,
    ]);

    Ok(())
}

// Don't return table, return a type that implements Display(?)
pub fn get_worktree_status_table(
    repo: &RepoHandle,
    directory: &Path,
) -> Result<(impl std::fmt::Display, Vec<Error>), Error> {
    let worktrees = repo.get_worktrees().map_err(Error::Repo)?;
    let mut table = Table::new();

    let mut errors = Vec::new();

    add_worktree_table_header(&mut table);
    for worktree in &worktrees {
        let worktree_dir = &directory.join(worktree.name().as_str());
        if worktree_dir.exists() {
            let repo = match RepoHandle::open(worktree_dir, WorktreeSetup::NoWorktree) {
                Ok(repo) => repo,
                Err(error) => {
                    errors.push(error.into());
                    continue;
                }
            };
            if let Err(error) = add_worktree_status(&mut table, worktree, &repo) {
                errors.push(error);
            }
        } else {
            errors.push(Error::WorktreeWithoutDirectory {
                worktree: worktree.name().clone(),
            });
        }
    }
    for worktree in RepoHandle::find_unmanaged_worktrees(repo, directory).map_err(Error::Repo)? {
        errors.push(Error::InvalidWorktreeDirectory { path: worktree });
    }
    Ok((table, errors))
}

pub fn get_status_table(config: config::Config) -> Result<(Vec<Table>, Vec<Error>), Error> {
    let mut errors = Vec::new();
    let mut tables = Vec::new();

    let trees: Vec<tree::Tree> = config.get_trees()?.into_iter().map(Into::into).collect();

    for tree in trees {
        let repos = tree.repos;

        let root_path = path::expand_path(tree.root.as_path())?;

        let mut table = Table::new();
        add_table_header(&mut table);

        for repo in &repos {
            let repo_path = root_path.join(repo.name.as_str());

            if !repo_path.exists() {
                errors.push(Error::RepoDoesNotExist {
                    name: repo.name.clone(),
                });
                continue;
            }

            let repo_handle = RepoHandle::open(&repo_path, repo.worktree_setup);

            let repo_handle = match repo_handle {
                Ok(repo) => repo,
                Err(error) => {
                    if matches!(error, repo::Error::NotFound) {
                        errors.push(Error::RepoNotGit {
                            name: repo.name.clone(),
                        });
                    } else {
                        errors.push(Error::RepoOpenFailed {
                            name: repo.name.clone(),
                            message: error.to_string(),
                        });
                    }
                    continue;
                }
            };

            if let Err(err) = add_repo_status(
                &mut table,
                Some(&repo.name),
                &repo_handle,
                repo.worktree_setup,
            ) {
                errors.push(Error::RepoStatusFailed {
                    name: repo.name.clone(),
                    message: err.to_string(),
                });
            }
        }

        tables.push(table);
    }

    Ok((tables, errors))
}

fn add_worktree_table_header(table: &mut Table) {
    table
        .load_preset(comfy_table::presets::UTF8_FULL)
        .apply_modifier(comfy_table::modifiers::UTF8_ROUND_CORNERS)
        .set_header([
            Cell::new("Worktree"),
            Cell::new("Status"),
            Cell::new("Branch"),
            Cell::new("Remote branch"),
        ]);
}

fn add_worktree_status(
    table: &mut Table,
    worktree: &repo::Worktree,
    repo: &RepoHandle,
) -> Result<(), Error> {
    let repo_status = repo
        .status(WorktreeSetup::NoWorktree)
        .map_err(Error::Repo)?;

    let local_branch = repo.head_branch().map_err(Error::Repo)?;

    let upstream_output = match local_branch.upstream() {
        Ok(remote_branch) => {
            let remote_branch_name = remote_branch.name().map_err(Error::Repo)?;

            let (ahead, behind) = repo
                .graph_ahead_behind(&local_branch, &remote_branch)
                .map_err(Error::Repo)?;

            format!(
                "{}{}\n",
                &remote_branch_name,
                &match (ahead, behind) {
                    (0, 0) => String::new(),
                    (d, 0) => format!(" [+{}]", &d),
                    (0, d) => format!(" [-{}]", &d),
                    (d1, d2) => format!(" [+{}/-{}]", &d1, &d2),
                },
            )
        }
        Err(_) => String::new(),
    };

    table.add_row([
        worktree.name().as_str(),
        &match repo_status.changes {
            Some(changes) => {
                let mut out = Vec::new();
                if changes.files_new > 0 {
                    out.push(format!("New: {}\n", changes.files_new));
                }
                if changes.files_modified > 0 {
                    out.push(format!("Modified: {}\n", changes.files_modified));
                }
                if changes.files_deleted > 0 {
                    out.push(format!("Deleted: {}\n", changes.files_deleted));
                }
                out.into_iter().collect::<String>().trim().to_owned()
            }
            None => String::from("\u{2714}"),
        },
        local_branch.name().map_err(Error::Repo)?.as_str(),
        &upstream_output,
    ]);

    Ok(())
}

pub fn show_single_repo_status(
    path: &Path,
) -> Result<(impl std::fmt::Display, Vec<String>), Error> {
    let mut table = Table::new();
    let mut warnings = Vec::new();

    let worktree_setup = RepoHandle::detect_worktree(path);
    add_table_header(&mut table);

    let repo_handle = RepoHandle::open(path, worktree_setup);

    if let Err(error) = repo_handle {
        if matches!(error, repo::Error::NotFound) {
            return Err(Error::NotAGitDirectory);
        } else {
            return Err(error.into());
        }
    }

    let repo_name = match path.file_name() {
        None => {
            warnings.push(format!(
                "Cannot detect repo name for path {}. Are you working in /?",
                &path.display()
            ));
            None
        }
        Some(file_name) => match file_name.to_str() {
            None => {
                warnings.push(format!(
                    "Name of repo directory {} is not valid UTF-8",
                    &path.display()
                ));
                None
            }
            Some(name) => Some(ProjectName::new(name.to_owned())),
        },
    };

    add_repo_status(
        &mut table,
        repo_name.as_ref(),
        &repo_handle?,
        worktree_setup,
    )?;

    Ok((table, warnings))
}
