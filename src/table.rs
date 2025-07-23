use super::config;
use super::path;
use super::repo;

use comfy_table::{Cell, Table};

use std::fmt::Write;
use std::path::Path;

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
    repo_name: &str,
    repo_handle: &repo::RepoHandle,
    is_worktree: bool,
) -> Result<(), String> {
    let repo_status = repo_handle.status(is_worktree)?;

    table.add_row([
        repo_name,
        if is_worktree { "\u{2714}" } else { "" },
        &if is_worktree {
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
                    out.into_iter().collect::<String>().trim().to_string()
                }
                None => String::from("\u{2714}"),
            }
        },
        repo_status
            .branches
            .iter()
            .fold(String::new(), |mut s, (branch_name, remote_branch)| {
                writeln!(
                    &mut s,
                    "branch: {}{}",
                    &branch_name,
                    &match remote_branch {
                        None => String::from(" <!local>"),
                        Some((remote_branch_name, remote_tracking_status)) => {
                            format!(
                                " <{}>{}",
                                remote_branch_name,
                                &match remote_tracking_status {
                                    repo::RemoteTrackingStatus::UpToDate =>
                                        String::from(" \u{2714}"),
                                    repo::RemoteTrackingStatus::Ahead(d) => format!(" [+{}]", &d),
                                    repo::RemoteTrackingStatus::Behind(d) => format!(" [-{}]", &d),
                                    repo::RemoteTrackingStatus::Diverged(d1, d2) =>
                                        format!(" [+{}/-{}]", &d1, &d2),
                                }
                            )
                        }
                    }
                )
                .unwrap();
                s
            })
            .trim(),
        &if is_worktree {
            String::new()
        } else {
            match repo_status.head {
                Some(head) => head,
                None => String::from("Empty"),
            }
        },
        repo_status
            .remotes
            .iter()
            .fold(String::new(), |mut s, r| {
                writeln!(&mut s, "{r}").unwrap();
                s
            })
            .trim(),
    ]);

    Ok(())
}

// Don't return table, return a type that implements Display(?)
pub fn get_worktree_status_table(
    repo: &repo::RepoHandle,
    directory: &Path,
) -> Result<(impl std::fmt::Display, Vec<String>), String> {
    let worktrees = repo.get_worktrees()?;
    let mut table = Table::new();

    let mut errors = Vec::new();

    add_worktree_table_header(&mut table);
    for worktree in &worktrees {
        let worktree_dir = &directory.join(worktree.name());
        if worktree_dir.exists() {
            let repo = match repo::RepoHandle::open(worktree_dir, false) {
                Ok(repo) => repo,
                Err(error) => {
                    errors.push(format!(
                        "Failed opening repo of worktree {}: {}",
                        &worktree.name(),
                        &error
                    ));
                    continue;
                }
            };
            if let Err(error) = add_worktree_status(&mut table, worktree, &repo) {
                errors.push(error);
            }
        } else {
            errors.push(format!(
                "Worktree {} does not have a directory",
                &worktree.name()
            ));
        }
    }
    for worktree in repo::RepoHandle::find_unmanaged_worktrees(repo, directory)? {
        errors.push(format!(
            "Found {}, which is not a valid worktree directory!",
            &worktree
        ));
    }
    Ok((table, errors))
}

pub fn get_status_table(config: config::Config) -> Result<(Vec<Table>, Vec<String>), String> {
    let mut errors = Vec::new();
    let mut tables = Vec::new();
    for tree in config.trees()? {
        let repos = tree.repos.unwrap_or_default();

        let root_path = path::expand_path(Path::new(&tree.root));

        let mut table = Table::new();
        add_table_header(&mut table);

        for repo in &repos {
            let repo_path = root_path.join(&repo.name);

            if !repo_path.exists() {
                errors.push(format!(
                    "{}: Repository does not exist. Run sync?",
                    &repo.name
                ));
                continue;
            }

            let repo_handle = repo::RepoHandle::open(&repo_path, repo.worktree_setup);

            let repo_handle = match repo_handle {
                Ok(repo) => repo,
                Err(error) => {
                    if error.kind == repo::RepoErrorKind::NotFound {
                        errors.push(format!(
                            "{}: No git repository found. Run sync?",
                            &repo.name
                        ));
                    } else {
                        errors.push(format!(
                            "{}: Opening repository failed: {}",
                            &repo.name, error
                        ));
                    }
                    continue;
                }
            };

            if let Err(err) =
                add_repo_status(&mut table, &repo.name, &repo_handle, repo.worktree_setup)
            {
                errors.push(format!("{}: Couldn't add repo status: {}", &repo.name, err));
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
    repo: &repo::RepoHandle,
) -> Result<(), String> {
    let repo_status = repo.status(false)?;

    let local_branch = repo
        .head_branch()
        .map_err(|error| format!("Failed getting head branch: {error}"))?;

    let upstream_output = match local_branch.upstream() {
        Ok(remote_branch) => {
            let remote_branch_name = remote_branch
                .name()
                .map_err(|error| format!("Failed getting name of remote branch: {error}"))?;

            let (ahead, behind) = repo
                .graph_ahead_behind(&local_branch, &remote_branch)
                .map_err(|error| format!("Failed computing branch deviation: {error}"))?;

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
        worktree.name(),
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
                out.into_iter().collect::<String>().trim().to_string()
            }
            None => String::from("\u{2714}"),
        },
        &local_branch
            .name()
            .map_err(|error| format!("Failed getting name of branch: {error}"))?,
        &upstream_output,
    ]);

    Ok(())
}

pub fn show_single_repo_status(
    path: &Path,
) -> Result<(impl std::fmt::Display, Vec<String>), String> {
    let mut table = Table::new();
    let mut warnings = Vec::new();

    let is_worktree = repo::RepoHandle::detect_worktree(path);
    add_table_header(&mut table);

    let repo_handle = repo::RepoHandle::open(path, is_worktree);

    if let Err(error) = repo_handle {
        return if error.kind == repo::RepoErrorKind::NotFound {
            Err(String::from("Directory is not a git directory"))
        } else {
            return Err(format!("Opening repository failed: {error}"));
        }
    };

    let repo_name = match path.file_name() {
        None => {
            warnings.push(format!(
                "Cannot detect repo name for path {}. Are you working in /?",
                &path.display()
            ));
            String::from("unknown")
        }
        Some(file_name) => match file_name.to_str() {
            None => {
                warnings.push(format!(
                    "Name of repo directory {} is not valid UTF-8",
                    &path.display()
                ));
                String::from("invalid")
            }
            Some(name) => name.to_string(),
        },
    };

    add_repo_status(&mut table, &repo_name, &repo_handle.unwrap(), is_worktree)?;

    Ok((table, warnings))
}
