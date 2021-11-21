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

fn find_repos(root: &Path) -> Option<Vec<Repo>> {
    let mut repos: Vec<Repo> = Vec::new();

    for (path, is_worktree) in find_repos_without_details(root).unwrap() {
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
    Some(repos)
}

fn find_in_tree(path: &Path) -> Option<Tree> {
    let repos: Vec<Repo> = match find_repos(path) {
        Some(vec) => vec,
        None => Vec::new(),
    };

    let mut root = path.to_path_buf();
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
            Cell::new("Status"),
            Cell::new("Branches"),
            Cell::new("HEAD"),
            Cell::new("Remotes"),
        ]);
}

fn add_repo_status(table: &mut Table, repo_name: &str, repo_handle: &git2::Repository) {
    let repo_status = get_repo_status(repo_handle);

    table.add_row(vec![
        repo_name,
        &match repo_status.changes {
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
                                        format!(" [-{}/+{}]", &d1, &d2),
                                }
                            )
                        }
                    }
                )
            })
            .collect::<String>()
            .trim()
            .to_string(),
        &match repo_status.head {
            Some(head) => head,
            None => String::from("Empty"),
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

    add_repo_status(&mut table, &repo_name, &repo_handle.unwrap());

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

            add_repo_status(&mut table, &repo.name, &repo_handle);
        }
        println!("{}", table);
    }
}

pub fn run() {
    let opts = cmd::parse();

    match opts.subcmd {
        cmd::SubCommand::Sync(sync) => {
            let config = match config::read_config(&sync.config) {
                Ok(c) => c,
                Err(e) => {
                    print_error(&e);
                    process::exit(1);
                }
            };
            sync_trees(config);
        }
        cmd::SubCommand::Status(args) => match &args.config {
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
        cmd::SubCommand::Find(find) => {
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

            let config = Config {
                trees: vec![find_in_tree(path).unwrap()],
            };

            let toml = toml::to_string(&config).unwrap();

            print!("{}", toml);
        }
        cmd::SubCommand::Worktree(args) => {
            let dir = match std::env::current_dir() {
                Ok(d) => d,
                Err(e) => {
                    print_error(&format!("Could not open current directory: {}", e));
                    process::exit(1);
                }
            };

            match args.action {
                cmd::WorktreeAction::Add(action_args) => {
                    let repo = match open_repo(&dir, true) {
                        Ok(r) => r,
                        Err(e) => {
                            match e.kind {
                                RepoErrorKind::NotFound => print_error(
                                    "Current directory does not contain a worktree setup",
                                ),
                                _ => print_error(&format!("Error opening repo: {}", e)),
                            }
                            process::exit(1);
                        }
                    };

                    let worktrees = repo
                        .worktrees()
                        .unwrap()
                        .iter()
                        .map(|e| e.unwrap())
                        .collect::<String>();
                    if worktrees.contains(&action_args.name) {
                        print_error("Worktree directory already exists");
                        process::exit(1);
                    }

                    match repo.worktree(&action_args.name, &dir.join(&action_args.name), None) {
                        Ok(_) => print_success(&format!("Worktree {} created", &action_args.name)),
                        Err(e) => {
                            print_error(&format!("Error creating worktree: {}", e));
                            process::exit(1);
                        }
                    };
                }
                cmd::WorktreeAction::Delete(action_args) => {
                    let worktree_dir = dir.join(&action_args.name);
                    if !worktree_dir.exists() {
                        print_error(&format!("{} does not exist", &action_args.name));
                        process::exit(1);
                    }
                    let repo = match open_repo(&worktree_dir, false) {
                        Ok(r) => r,
                        Err(e) => {
                            print_error(&format!("Error opening repo: {}", e));
                            process::exit(1);
                        }
                    };
                    let status = get_repo_status(&repo);
                    if status.changes.is_some() {
                        print_error("Changes found in worktree, refusing to delete!");
                        process::exit(1);
                    }

                    let mut branch = repo
                        .find_branch(&action_args.name, git2::BranchType::Local)
                        .unwrap();
                    match branch.upstream() {
                        Ok(remote_branch) => {
                            let (ahead, behind) = repo
                                .graph_ahead_behind(
                                    branch.get().peel_to_commit().unwrap().id(),
                                    remote_branch.get().peel_to_commit().unwrap().id(),
                                )
                                .unwrap();

                            if (ahead, behind) != (0, 0) {
                                print_error(&format!("Branch {} is not in line with remote branch, refusing to delete worktree!", &action_args.name));
                                process::exit(1);
                            }
                        }
                        Err(_) => {
                            print_error(&format!("No remote tracking branch for branch {} found, refusing to delete worktree!", &action_args.name));
                            process::exit(1);
                        }
                    }

                    match std::fs::remove_dir_all(&worktree_dir) {
                        Ok(_) => print_success(&format!("Worktree {} deleted", &action_args.name)),
                        Err(e) => {
                            print_error(&format!(
                                "Error deleting {}: {}",
                                &worktree_dir.display(),
                                e
                            ));
                            process::exit(1);
                        }
                    }
                    repo.find_worktree(&action_args.name)
                        .unwrap()
                        .prune(None)
                        .unwrap();
                    branch.delete().unwrap();
                }
            }
        }
    }
}
