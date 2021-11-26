use std::fs;
use std::path::{Path, PathBuf};
use std::process;

mod cmd;
mod config;
mod output;
pub mod repo;

use config::{Config, Tree};
use output::*;

use comfy_table::{Cell, Table};

use repo::{
    clone_repo, detect_remote_type, get_repo_status, init_repo, open_repo, Remote,
    RemoteTrackingStatus, Repo, RepoErrorKind,
};

const GIT_MAIN_WORKTREE_DIRECTORY: &str = ".git-main-working-tree";
const BRANCH_NAMESPACE_SEPARATOR: &str = "/";

const GIT_CONFIG_BARE_KEY: &str = "core.bare";

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() {
        std::env::set_var("HOME", "/home/test");
    }

    #[test]
    fn check_expand_tilde() {
        setup();
        assert_eq!(
            expand_path(Path::new("~/file")),
            Path::new("/home/test/file")
        );
    }

    #[test]
    fn check_expand_invalid_tilde() {
        setup();
        assert_eq!(
            expand_path(Path::new("/home/~/file")),
            Path::new("/home/~/file")
        );
    }

    #[test]
    fn check_expand_home() {
        setup();
        assert_eq!(
            expand_path(Path::new("$HOME/file")),
            Path::new("/home/test/file")
        );
        assert_eq!(
            expand_path(Path::new("${HOME}/file")),
            Path::new("/home/test/file")
        );
    }
}

fn path_as_string(path: &Path) -> String {
    path.to_path_buf().into_os_string().into_string().unwrap()
}

fn env_home() -> PathBuf {
    match std::env::var("HOME") {
        Ok(path) => Path::new(&path).to_path_buf(),
        Err(e) => {
            print_error(&format!("Unable to read HOME: {}", e));
            process::exit(1);
        }
    }
}

fn expand_path(path: &Path) -> PathBuf {
    fn home_dir() -> Option<PathBuf> {
        Some(env_home())
    }

    let expanded_path = match shellexpand::full_with_context(
        &path_as_string(path),
        home_dir,
        |name| -> Result<Option<String>, &'static str> {
            match name {
                "HOME" => Ok(Some(path_as_string(home_dir().unwrap().as_path()))),
                _ => Ok(None),
            }
        },
    ) {
        Ok(std::borrow::Cow::Borrowed(path)) => path.to_owned(),
        Ok(std::borrow::Cow::Owned(path)) => path,
        Err(e) => {
            print_error(&format!("Unable to expand root: {}", e));
            process::exit(1);
        }
    };

    Path::new(&expanded_path).to_path_buf()
}

fn get_default_branch(repo: &git2::Repository) -> Result<git2::Branch, String> {
    match repo.find_branch("main", git2::BranchType::Local) {
        Ok(branch) => Ok(branch),
        Err(_) => match repo.find_branch("master", git2::BranchType::Local) {
            Ok(branch) => Ok(branch),
            Err(_) => Err(String::from("Could not determine default branch")),
        },
    }
}

fn sync_trees(config: Config) {
    for tree in config.trees {
        let repos = tree.repos.unwrap_or_default();

        let root_path = expand_path(Path::new(&tree.root));

        for repo in &repos {
            let repo_path = root_path.join(&repo.name);
            let actual_git_directory = get_actual_git_directory(&repo_path, repo.worktree_setup);

            let mut repo_handle = None;

            if repo_path.exists() {
                if repo.worktree_setup && !actual_git_directory.exists() {
                    print_repo_error(
                        &repo.name,
                        "Repo already exists, but is not using a worktree setup",
                    );
                    process::exit(1);
                }
                repo_handle = Some(open_repo(&repo_path, repo.worktree_setup).unwrap_or_else(
                    |error| {
                        print_repo_error(
                            &repo.name,
                            &format!("Opening repository failed: {}", error),
                        );
                        process::exit(1);
                    },
                ));
            } else if matches!(&repo.remotes, None) || repo.remotes.as_ref().unwrap().is_empty() {
                print_repo_action(
                    &repo.name,
                    "Repository does not have remotes configured, initializing new",
                );
                repo_handle = match init_repo(&repo_path, repo.worktree_setup) {
                    Ok(r) => {
                        print_repo_success(&repo.name, "Repository created");
                        Some(r)
                    }
                    Err(e) => {
                        print_repo_error(
                            &repo.name,
                            &format!("Repository failed during init: {}", e),
                        );
                        None
                    }
                }
            } else {
                let first = repo.remotes.as_ref().unwrap().first().unwrap();

                match clone_repo(first, &repo_path, repo.worktree_setup) {
                    Ok(_) => {
                        print_repo_success(&repo.name, "Repository successfully cloned");
                    }
                    Err(e) => {
                        print_repo_error(
                            &repo.name,
                            &format!("Repository failed during clone: {}", e),
                        );
                        continue;
                    }
                };
            }
            if let Some(remotes) = &repo.remotes {
                let repo_handle = repo_handle.unwrap_or_else(|| {
                    open_repo(&repo_path, repo.worktree_setup).unwrap_or_else(|_| process::exit(1))
                });

                let current_remotes: Vec<String> = match repo_handle.remotes() {
                    Ok(r) => r,
                    Err(e) => {
                        print_repo_error(
                            &repo.name,
                            &format!("Repository failed during getting the remotes: {}", e),
                        );
                        continue;
                    }
                }
                .iter()
                .flatten()
                .map(|r| r.to_owned())
                .collect();

                for remote in remotes {
                    if !current_remotes.iter().any(|r| *r == remote.name) {
                        print_repo_action(
                            &repo.name,
                            &format!(
                                "Setting up new remote \"{}\" to \"{}\"",
                                &remote.name, &remote.url
                            ),
                        );
                        if let Err(e) = repo_handle.remote(&remote.name, &remote.url) {
                            print_repo_error(
                                &repo.name,
                                &format!("Repository failed during setting the remotes: {}", e),
                            );
                            continue;
                        }
                    } else {
                        let current_remote = repo_handle.find_remote(&remote.name).unwrap();
                        let current_url = match current_remote.url() {
                            Some(url) => url,
                            None => {
                                print_repo_error(&repo.name, &format!("Repository failed during getting of the remote URL for remote \"{}\". This is most likely caused by a non-utf8 remote name", remote.name));
                                continue;
                            }
                        };
                        if remote.url != current_url {
                            print_repo_action(
                                &repo.name,
                                &format!("Updating remote {} to \"{}\"", &remote.name, &remote.url),
                            );
                            if let Err(e) = repo_handle.remote_set_url(&remote.name, &remote.url) {
                                print_repo_error(&repo.name, &format!("Repository failed during setting of the remote URL for remote \"{}\": {}", &remote.name, e));
                                continue;
                            };
                        }
                    }
                }

                for current_remote in &current_remotes {
                    if !remotes.iter().any(|r| &r.name == current_remote) {
                        print_repo_action(
                            &repo.name,
                            &format!("Deleting remote \"{}\"", &current_remote,),
                        );
                        if let Err(e) = repo_handle.remote_delete(current_remote) {
                            print_repo_error(
                                &repo.name,
                                &format!(
                                    "Repository failed during deleting remote \"{}\": {}",
                                    &current_remote, e
                                ),
                            );
                            continue;
                        }
                    }
                }
            }

            print_repo_success(&repo.name, "OK");
        }

        let current_repos = find_repos_without_details(&root_path).unwrap();
        for (repo, _) in current_repos {
            let name = path_as_string(repo.strip_prefix(&root_path).unwrap());
            if !repos.iter().any(|r| r.name == name) {
                print_warning(&format!("Found unmanaged repository: {}", name));
            }
        }
    }
}

fn find_repos_without_details(path: &Path) -> Option<Vec<(PathBuf, bool)>> {
    let mut repos: Vec<(PathBuf, bool)> = Vec::new();

    let git_dir = path.join(".git");
    let git_worktree = path.join(GIT_MAIN_WORKTREE_DIRECTORY);

    if git_dir.exists() {
        repos.push((path.to_path_buf(), false));
    } else if git_worktree.exists() {
        repos.push((path.to_path_buf(), true));
    } else {
        match fs::read_dir(path) {
            Ok(contents) => {
                for content in contents {
                    match content {
                        Ok(entry) => {
                            let path = entry.path();
                            if path.is_symlink() {
                                continue;
                            }
                            if path.is_dir() {
                                if let Some(mut r) = find_repos_without_details(&path) {
                                    repos.append(&mut r);
                                };
                            }
                        }
                        Err(e) => {
                            print_error(&format!("Error accessing directory: {}", e));
                            continue;
                        }
                    };
                }
            }
            Err(e) => {
                print_error(&format!("Failed to open \"{}\": {}", &path.display(), &e));
                return None;
            }
        };
    }

    Some(repos)
}

fn get_actual_git_directory(path: &Path, is_worktree: bool) -> PathBuf {
    match is_worktree {
        false => path.to_path_buf(),
        true => path.join(GIT_MAIN_WORKTREE_DIRECTORY),
    }
}

fn find_repos(root: &Path) -> Option<(Vec<Repo>, bool)> {
    let mut repos: Vec<Repo> = Vec::new();
    let mut repo_in_root = false;

    for (path, is_worktree) in find_repos_without_details(root).unwrap() {
        if path == root {
            repo_in_root = true;
        }
        let repo = match open_repo(&path, is_worktree) {
            Ok(r) => r,
            Err(e) => {
                print_error(&format!(
                    "Error opening repo {}{}: {}",
                    path.display(),
                    match is_worktree {
                        true => " as worktree",
                        false => "",
                    },
                    e
                ));
                return None;
            }
        };

        let remotes = match repo.remotes() {
            Ok(remotes) => {
                let mut results: Vec<Remote> = Vec::new();
                for remote in remotes.iter() {
                    match remote {
                        Some(remote_name) => {
                            match repo.find_remote(remote_name) {
                                Ok(remote) => {
                                    let name = match remote.name() {
                                        Some(name) => name.to_string(),
                                        None => {
                                            print_repo_error(&path_as_string(&path), &format!("Falied getting name of remote \"{}\". This is most likely caused by a non-utf8 remote name", remote_name));
                                            process::exit(1);
                                        }
                                    };
                                    let url = match remote.url() {
                                        Some(url) => url.to_string(),
                                        None => {
                                            print_repo_error(&path_as_string(&path), &format!("Falied getting URL of remote \"{}\". This is most likely caused by a non-utf8 URL", name));
                                            process::exit(1);
                                        }
                                    };
                                    let remote_type = match detect_remote_type(&url) {
                                        Some(t) => t,
                                        None => {
                                            print_repo_error(
                                                &path_as_string(&path),
                                                &format!(
                                                    "Could not detect remote type of \"{}\"",
                                                    &url
                                                ),
                                            );
                                            process::exit(1);
                                        }
                                    };

                                    results.push(Remote {
                                        name,
                                        url,
                                        remote_type,
                                    });
                                }
                                Err(e) => {
                                    print_repo_error(
                                        &path_as_string(&path),
                                        &format!("Error getting remote {}: {}", remote_name, e),
                                    );
                                    process::exit(1);
                                }
                            };
                        }
                        None => {
                            print_repo_error(&path_as_string(&path), "Error getting remote. This is most likely caused by a non-utf8 remote name");
                            process::exit(1);
                        }
                    };
                }
                Some(results)
            }
            Err(e) => {
                print_repo_error(
                    &path_as_string(&path),
                    &format!("Error getting remotes: {}", e),
                );
                process::exit(1);
            }
        };

        repos.push(Repo {
            name: match path == root {
                true => match &root.parent() {
                    Some(parent) => path_as_string(path.strip_prefix(parent).unwrap()),
                    None => {
                        print_error("Getting name of the search root failed. Do you have a git repository in \"/\"?");
                        process::exit(1);
                    },
                }
                false => path_as_string(path.strip_prefix(&root).unwrap()),
            },
            remotes,
            worktree_setup: is_worktree,
        });
    }
    Some((repos, repo_in_root))
}

fn find_in_tree(path: &Path) -> Option<Tree> {
    let (repos, repo_in_root): (Vec<Repo>, bool) = match find_repos(path) {
        Some((vec, repo_in_root)) => (vec, repo_in_root),
        None => (Vec::new(), false),
    };

    let mut root = path.to_path_buf();
    if repo_in_root {
        root = match root.parent() {
            Some(root) => root.to_path_buf(),
            None => {
                print_error("Cannot detect root directory. Are you working in /?");
                process::exit(1);
            }
        }
    }
    let home = env_home();
    if root.starts_with(&home) {
        // The tilde is not handled differently, it's just a normal path component for `Path`.
        // Therefore we can treat it like that during **output**.
        root = Path::new("~").join(root.strip_prefix(&home).unwrap());
    }

    Some(Tree {
        root: root.into_os_string().into_string().unwrap(),
        repos: Some(repos),
    })
}

fn add_table_header(table: &mut Table) {
    table
        .load_preset(comfy_table::presets::UTF8_FULL)
        .apply_modifier(comfy_table::modifiers::UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Repo"),
            Cell::new("Worktree"),
            Cell::new("Status"),
            Cell::new("Branches"),
            Cell::new("HEAD"),
            Cell::new("Remotes"),
        ]);
}

fn add_worktree_table_header(table: &mut Table) {
    table
        .load_preset(comfy_table::presets::UTF8_FULL)
        .apply_modifier(comfy_table::modifiers::UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Worktree"),
            Cell::new("Status"),
            Cell::new("Branch"),
            Cell::new("Remote branch"),
        ]);
}

fn add_repo_status(
    table: &mut Table,
    repo_name: &str,
    repo_handle: &git2::Repository,
    is_worktree: bool,
) {
    let repo_status = get_repo_status(repo_handle, is_worktree);

    table.add_row(vec![
        repo_name,
        match is_worktree {
            true => "\u{2714}",
            false => "",
        },
        &match repo_status.changes {
            None => String::from("-"),
            Some(changes) => match changes {
                Some(changes) => {
                    let mut out = Vec::new();
                    if changes.files_new > 0 {
                        out.push(format!("New: {}\n", changes.files_new))
                    }
                    if changes.files_modified > 0 {
                        out.push(format!("Modified: {}\n", changes.files_modified))
                    }
                    if changes.files_deleted > 0 {
                        out.push(format!("Deleted: {}\n", changes.files_deleted))
                    }
                    out.into_iter().collect::<String>().trim().to_string()
                }
                None => String::from("\u{2714}"),
            },
        },
        &repo_status
            .branches
            .iter()
            .map(|(branch_name, remote_branch)| {
                format!(
                    "branch: {}{}\n",
                    &branch_name,
                    &match remote_branch {
                        None => String::from(" <!local>"),
                        Some((remote_branch_name, remote_tracking_status)) => {
                            format!(
                                " <{}>{}",
                                remote_branch_name,
                                &match remote_tracking_status {
                                    RemoteTrackingStatus::UpToDate => String::from(" \u{2714}"),
                                    RemoteTrackingStatus::Ahead(d) => format!(" [+{}]", &d),
                                    RemoteTrackingStatus::Behind(d) => format!(" [-{}]", &d),
                                    RemoteTrackingStatus::Diverged(d1, d2) =>
                                        format!(" [+{}/-{}]", &d1, &d2),
                                }
                            )
                        }
                    }
                )
            })
            .collect::<String>()
            .trim()
            .to_string(),
        &match is_worktree {
            true => String::from(""),
            false => match repo_status.head {
                Some(head) => head,
                None => String::from("Empty"),
            },
        },
        &repo_status
            .remotes
            .iter()
            .map(|r| format!("{}\n", r))
            .collect::<String>()
            .trim()
            .to_string(),
    ]);
}

fn add_worktree_status(table: &mut Table, worktree_name: &str, repo: &git2::Repository) {
    let repo_status = get_repo_status(repo, false);

    let head = repo.head().unwrap();

    if !head.is_branch() {
        print_error("No branch checked out in worktree");
        process::exit(1);
    }

    let local_branch_name = head.shorthand().unwrap();
    let local_branch = repo
        .find_branch(local_branch_name, git2::BranchType::Local)
        .unwrap();

    let upstream_output = match local_branch.upstream() {
        Ok(remote_branch) => {
            let remote_branch_name = remote_branch.name().unwrap().unwrap().to_string();

            let (ahead, behind) = repo
                .graph_ahead_behind(
                    local_branch.get().peel_to_commit().unwrap().id(),
                    remote_branch.get().peel_to_commit().unwrap().id(),
                )
                .unwrap();

            format!(
                "{}{}\n",
                &remote_branch_name,
                &match (ahead, behind) {
                    (0, 0) => String::from(""),
                    (d, 0) => format!(" [+{}]", &d),
                    (0, d) => format!(" [-{}]", &d),
                    (d1, d2) => format!(" [+{}/-{}]", &d1, &d2),
                },
            )
        }
        Err(_) => String::from(""),
    };

    table.add_row(vec![
        worktree_name,
        &match repo_status.changes {
            None => String::from(""),
            Some(changes) => match changes {
                Some(changes) => {
                    let mut out = Vec::new();
                    if changes.files_new > 0 {
                        out.push(format!("New: {}\n", changes.files_new))
                    }
                    if changes.files_modified > 0 {
                        out.push(format!("Modified: {}\n", changes.files_modified))
                    }
                    if changes.files_deleted > 0 {
                        out.push(format!("Deleted: {}\n", changes.files_deleted))
                    }
                    out.into_iter().collect::<String>().trim().to_string()
                }
                None => String::from("\u{2714}"),
            },
        },
        local_branch_name,
        &upstream_output,
    ]);
}

fn show_single_repo_status(path: &Path, is_worktree: bool) {
    let mut table = Table::new();
    add_table_header(&mut table);

    let repo_handle = open_repo(path, is_worktree);

    if let Err(error) = repo_handle {
        if error.kind == RepoErrorKind::NotFound {
            print_error(&"Directory is not a git directory".to_string());
        } else {
            print_error(&format!("Opening repository failed: {}", error));
        }
        process::exit(1);
    };

    let repo_name = match path.file_name() {
        None => {
            print_warning("Cannot detect repo name. Are you working in /?");
            String::from("unknown")
        }
        Some(file_name) => match file_name.to_str() {
            None => {
                print_warning("Name of current directory is not valid UTF-8");
                String::from("invalid")
            }
            Some(name) => name.to_string(),
        },
    };

    add_repo_status(&mut table, &repo_name, &repo_handle.unwrap(), is_worktree);

    println!("{}", table);
}

fn show_status(config: Config) {
    for tree in config.trees {
        let repos = tree.repos.unwrap_or_default();

        let root_path = expand_path(Path::new(&tree.root));

        let mut table = Table::new();
        add_table_header(&mut table);

        for repo in &repos {
            let repo_path = root_path.join(&repo.name);

            if !repo_path.exists() {
                print_repo_error(
                    &repo.name,
                    &"Repository does not exist. Run sync?".to_string(),
                );
                continue;
            }

            let repo_handle = open_repo(&repo_path, repo.worktree_setup);

            if let Err(error) = repo_handle {
                if error.kind == RepoErrorKind::NotFound {
                    print_repo_error(
                        &repo.name,
                        &"No git repository found. Run sync?".to_string(),
                    );
                } else {
                    print_repo_error(&repo.name, &format!("Opening repository failed: {}", error));
                }
                continue;
            };

            let repo_handle = repo_handle.unwrap();

            add_repo_status(&mut table, &repo.name, &repo_handle, repo.worktree_setup);
        }
        println!("{}", table);
    }
}

enum WorktreeRemoveFailureReason {
    Changes(String),
    Error(String),
}

fn remove_worktree(
    name: &str,
    worktree_dir: &Path,
    force: bool,
    main_repo: &git2::Repository,
) -> Result<(), WorktreeRemoveFailureReason> {
    if !worktree_dir.exists() {
        return Err(WorktreeRemoveFailureReason::Error(format!(
            "{} does not exist",
            name
        )));
    }
    let worktree_repo = match open_repo(worktree_dir, false) {
        Ok(r) => r,
        Err(e) => {
            return Err(WorktreeRemoveFailureReason::Error(format!(
                "Error opening repo: {}",
                e
            )));
        }
    };

    let head = worktree_repo.head().unwrap();
    if !head.is_branch() {
        return Err(WorktreeRemoveFailureReason::Error(String::from(
            "No branch checked out in worktree",
        )));
    }

    let branch_name = head.shorthand().unwrap();
    if branch_name != name
        && !branch_name.ends_with(&format!("{}{}", BRANCH_NAMESPACE_SEPARATOR, name))
    {
        return Err(WorktreeRemoveFailureReason::Error(format!(
            "Branch {} is checked out in worktree, this does not look correct",
            &branch_name
        )));
    }

    let mut branch = worktree_repo
        .find_branch(branch_name, git2::BranchType::Local)
        .unwrap();

    if !force {
        let status = get_repo_status(&worktree_repo, false);
        if status.changes.unwrap().is_some() {
            return Err(WorktreeRemoveFailureReason::Changes(String::from(
                "Changes found in worktree",
            )));
        }

        match branch.upstream() {
            Ok(remote_branch) => {
                let (ahead, behind) = worktree_repo
                    .graph_ahead_behind(
                        branch.get().peel_to_commit().unwrap().id(),
                        remote_branch.get().peel_to_commit().unwrap().id(),
                    )
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

    if let Err(e) = std::fs::remove_dir_all(&worktree_dir) {
        return Err(WorktreeRemoveFailureReason::Error(format!(
            "Error deleting {}: {}",
            &worktree_dir.display(),
            e
        )));
    }
    main_repo.find_worktree(name).unwrap().prune(None).unwrap();
    branch.delete().unwrap();

    Ok(())
}

pub fn run() {
    let opts = cmd::parse();

    match opts.subcmd {
        cmd::SubCommand::Repos(repos) => match repos.action {
            cmd::ReposAction::Sync(sync) => {
                let config = match config::read_config(&sync.config) {
                    Ok(c) => c,
                    Err(e) => {
                        print_error(&e);
                        process::exit(1);
                    }
                };
                sync_trees(config);
            }
            cmd::ReposAction::Status(args) => match &args.config {
                Some(config_path) => {
                    let config = match config::read_config(config_path) {
                        Ok(c) => c,
                        Err(e) => {
                            print_error(&e);
                            process::exit(1);
                        }
                    };
                    show_status(config);
                }
                None => {
                    let dir = match std::env::current_dir() {
                        Ok(d) => d,
                        Err(e) => {
                            print_error(&format!("Could not open current directory: {}", e));
                            process::exit(1);
                        }
                    };

                    let has_worktree = dir.join(GIT_MAIN_WORKTREE_DIRECTORY).exists();
                    show_single_repo_status(&dir, has_worktree);
                }
            },
            cmd::ReposAction::Find(find) => {
                let path = Path::new(&find.path);
                if !path.exists() {
                    print_error(&format!("Path \"{}\" does not exist", path.display()));
                    process::exit(1);
                }
                let path = &path.canonicalize().unwrap();
                if !path.is_dir() {
                    print_error(&format!("Path \"{}\" is not a directory", path.display()));
                    process::exit(1);
                }

                let trees = vec![find_in_tree(path).unwrap()];
                if trees.iter().all(|t| match &t.repos {
                    None => false,
                    Some(r) => r.is_empty(),
                }) {
                    print_warning("No repositories found");
                } else {
                    let config = Config { trees };

                    let toml = toml::to_string(&config).unwrap();

                    print!("{}", toml);
                }
            }
        },
        cmd::SubCommand::Worktree(args) => {
            let dir = match std::env::current_dir() {
                Ok(d) => d,
                Err(e) => {
                    print_error(&format!("Could not open current directory: {}", e));
                    process::exit(1);
                }
            };

            fn get_repo(dir: &Path) -> git2::Repository {
                match open_repo(dir, true) {
                    Ok(r) => r,
                    Err(e) => {
                        match e.kind {
                            RepoErrorKind::NotFound => {
                                print_error("Current directory does not contain a worktree setup")
                            }
                            _ => print_error(&format!("Error opening repo: {}", e)),
                        }
                        process::exit(1);
                    }
                }
            }

            fn get_worktrees(repo: &git2::Repository) -> Vec<String> {
                repo.worktrees()
                    .unwrap()
                    .iter()
                    .map(|e| e.unwrap().to_string())
                    .collect::<Vec<String>>()
            }

            match args.action {
                cmd::WorktreeAction::Add(action_args) => {
                    let repo = get_repo(&dir);
                    let worktrees = get_worktrees(&repo);
                    if worktrees.contains(&action_args.name) {
                        print_error("Worktree already exists");
                        process::exit(1);
                    }

                    let branch_name = match action_args.branch_namespace {
                        Some(prefix) => format!(
                            "{}{}{}",
                            &prefix, BRANCH_NAMESPACE_SEPARATOR, &action_args.name
                        ),
                        None => action_args.name.clone(),
                    };

                    let mut remote_branch_exists = false;

                    let checkout_commit = match &action_args.track {
                        Some(upstream_branch_name) => {
                            match repo.find_branch(upstream_branch_name, git2::BranchType::Remote) {
                                Ok(branch) => {
                                    remote_branch_exists = true;
                                    branch.into_reference().peel_to_commit().unwrap()
                                }
                                Err(_) => {
                                    remote_branch_exists = false;
                                    get_default_branch(&repo)
                                        .unwrap()
                                        .into_reference()
                                        .peel_to_commit()
                                        .unwrap()
                                }
                            }
                        }
                        None => get_default_branch(&repo)
                            .unwrap()
                            .into_reference()
                            .peel_to_commit()
                            .unwrap(),
                    };

                    let mut target_branch =
                        match repo.find_branch(&branch_name, git2::BranchType::Local) {
                            Ok(branchref) => branchref,
                            Err(_) => repo.branch(&branch_name, &checkout_commit, false).unwrap(),
                        };

                    if let Some(upstream_branch_name) = action_args.track {
                        if remote_branch_exists {
                            target_branch
                                .set_upstream(Some(&upstream_branch_name))
                                .unwrap();
                        } else {
                            print_error(&format!(
                                "Remote branch {} not found",
                                &upstream_branch_name
                            ));
                            let split_at = upstream_branch_name.find('/').unwrap_or(0);
                            if split_at == 0 || split_at >= upstream_branch_name.len() - 1 {
                                print_error("Tracking branch needs to match the pattern <remote>/<branch_name>");
                                process::exit(1);
                            }

                            let (remote_name, remote_branch_name) =
                                &upstream_branch_name.split_at(split_at);
                            // strip the remaining slash
                            let remote_branch_name = &remote_branch_name[1..];

                            let mut remote = match repo.find_remote(remote_name) {
                                Ok(r) => r,
                                Err(_) => {
                                    print_error(&format!("Remote {} not found", remote_name));
                                    process::exit(1);
                                }
                            };

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
                                "+{}:refs/heads/{}",
                                target_branch.get().name().unwrap(),
                                remote_branch_name
                            );
                            remote
                                .push(&[push_refspec], Some(&mut push_options))
                                .unwrap();

                            target_branch
                                .set_upstream(Some(&upstream_branch_name))
                                .unwrap();
                        }
                    };

                    let worktree = repo.worktree(
                        &action_args.name,
                        &dir.join(&action_args.name),
                        Some(git2::WorktreeAddOptions::new().reference(Some(target_branch.get()))),
                    );

                    match worktree {
                        Ok(_) => print_success(&format!("Worktree {} created", &action_args.name)),
                        Err(e) => {
                            print_error(&format!("Error creating worktree: {}", e));
                            process::exit(1);
                        }
                    };
                }

                cmd::WorktreeAction::Delete(action_args) => {
                    let worktree_dir = dir.join(&action_args.name);
                    let repo = get_repo(&dir);

                    match remove_worktree(
                        &action_args.name,
                        &worktree_dir,
                        action_args.force,
                        &repo,
                    ) {
                        Ok(_) => print_success(&format!("Worktree {} deleted", &action_args.name)),
                        Err(error) => {
                            match error {
                                WorktreeRemoveFailureReason::Error(msg) => {
                                    print_error(&msg);
                                    process::exit(1);
                                }
                                WorktreeRemoveFailureReason::Changes(changes) => {
                                    print_warning(&format!(
                                        "Changes in worktree: {}. Refusing to delete",
                                        changes
                                    ));
                                }
                            }
                            process::exit(1);
                        }
                    }
                }
                cmd::WorktreeAction::Status(_args) => {
                    let repo = get_repo(&dir);
                    let worktrees = get_worktrees(&repo);
                    let mut table = Table::new();
                    add_worktree_table_header(&mut table);
                    for worktree in &worktrees {
                        let repo_dir = &dir.join(&worktree);
                        if repo_dir.exists() {
                            let repo = match open_repo(repo_dir, false) {
                                Ok(r) => r,
                                Err(e) => {
                                    print_error(&format!("Error opening repo: {}", e));
                                    process::exit(1);
                                }
                            };
                            add_worktree_status(&mut table, worktree, &repo);
                        } else {
                            print_warning(&format!(
                                "Worktree {} does not have a directory",
                                &worktree
                            ));
                        }
                    }
                    for entry in std::fs::read_dir(&dir).unwrap() {
                        let dirname = path_as_string(
                            &entry
                                .unwrap()
                                .path()
                                .strip_prefix(&dir)
                                .unwrap()
                                .to_path_buf(),
                        );
                        if dirname == GIT_MAIN_WORKTREE_DIRECTORY {
                            continue;
                        }
                        if !&worktrees.contains(&dirname) {
                            print_warning(&format!(
                                "Found {}, which is not a valid worktree directory!",
                                &dirname
                            ));
                        }
                    }
                    println!("{}", table);
                }
                cmd::WorktreeAction::Convert(_args) => {
                    // Converting works like this:
                    // * Check whether there are uncommitted/unpushed changes
                    // * Move the contents of .git dir to the worktree directory
                    // * Remove all files
                    // * Set `core.bare` to `true`

                    let repo = open_repo(&dir, false).unwrap_or_else(|error| {
                        if error.kind == RepoErrorKind::NotFound {
                            print_error("Directory does not contain a git repository");
                        } else {
                            print_error(&format!("Opening repository failed: {}", error));
                        }
                        process::exit(1);
                    });

                    let status = get_repo_status(&repo, false);
                    if status.changes.unwrap().is_some() {
                        print_error("Changes found in repository, refusing to convert");
                    }

                    if let Err(error) = std::fs::rename(".git", GIT_MAIN_WORKTREE_DIRECTORY) {
                        print_error(&format!("Error moving .git directory: {}", error));
                    }

                    for entry in match std::fs::read_dir(&dir) {
                        Ok(iterator) => iterator,
                        Err(error) => {
                            print_error(&format!("Opening directory failed: {}", error));
                            process::exit(1);
                        }
                    } {
                        match entry {
                            Ok(entry) => {
                                let path = entry.path();
                                // The path will ALWAYS have a file component
                                if path.file_name().unwrap() == GIT_MAIN_WORKTREE_DIRECTORY {
                                    continue;
                                }
                                if path.is_file() || path.is_symlink() {
                                    if let Err(error) = std::fs::remove_file(&path) {
                                        print_error(&format!("Failed removing {}", error));
                                        process::exit(1);
                                    }
                                } else if let Err(error) = std::fs::remove_dir_all(&path) {
                                    print_error(&format!("Failed removing {}", error));
                                    process::exit(1);
                                }
                            }
                            Err(error) => {
                                print_error(&format!("Error getting directory entry: {}", error));
                                process::exit(1);
                            }
                        }
                    }

                    let worktree_repo = open_repo(&dir, true).unwrap_or_else(|error| {
                        print_error(&format!(
                            "Opening newly converted repository failed: {}",
                            error
                        ));
                        process::exit(1);
                    });

                    let mut config = worktree_repo.config().unwrap_or_else(|error| {
                        print_error(&format!(
                            "Opening getting repository configuration: {}",
                            error
                        ));
                        process::exit(1);
                    });

                    config
                        .set_bool(GIT_CONFIG_BARE_KEY, true)
                        .unwrap_or_else(|error| {
                            print_error(&format!(
                                "Error setting {}: {}",
                                GIT_CONFIG_BARE_KEY, error
                            ));
                            process::exit(1);
                        });
                }
                cmd::WorktreeAction::Clean(_args) => {
                    let repo = get_repo(&dir);
                    let worktrees = get_worktrees(&repo);
                    for worktree in &worktrees {
                        let repo_dir = &dir.join(&worktree);
                        if repo_dir.exists() {
                            match remove_worktree(worktree, repo_dir, false, &repo) {
                                Ok(_) => print_success(&format!("Worktree {} deleted", &worktree)),
                                Err(error) => match error {
                                    WorktreeRemoveFailureReason::Changes(changes) => {
                                        print_warning(&format!(
                                            "Changes found in {}: {}, skipping",
                                            &worktree, &changes
                                        ));
                                        continue;
                                    }
                                    WorktreeRemoveFailureReason::Error(e) => {
                                        print_error(&e);
                                        process::exit(1);
                                    }
                                },
                            }
                        } else {
                            print_warning(&format!(
                                "Worktree {} does not have a directory",
                                &worktree
                            ));
                        }
                    }
                    for entry in std::fs::read_dir(&dir).unwrap() {
                        let dirname = path_as_string(
                            &entry
                                .unwrap()
                                .path()
                                .strip_prefix(&dir)
                                .unwrap()
                                .to_path_buf(),
                        );
                        if dirname == GIT_MAIN_WORKTREE_DIRECTORY {
                            continue;
                        }
                        if !&worktrees.contains(&dirname) {
                            print_warning(&format!(
                                "Found {}, which is not a valid worktree directory!",
                                &dirname
                            ));
                        }
                    }
                }
            }
        }
    }
}
