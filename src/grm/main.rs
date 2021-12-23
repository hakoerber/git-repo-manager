use std::path::Path;
use std::process;

mod cmd;

use grm::repo;
use grm::config;
use grm::output::*;

fn main() {
    let opts = cmd::parse();

    match opts.subcmd {
        cmd::SubCommand::Repos(repos) => match repos.action {
            cmd::ReposAction::Sync(sync) => {
                let config = match config::read_config(&sync.config) {
                    Ok(config) => config,
                    Err(error) => {
                        print_error(&error);
                        process::exit(1);
                    }
                };
                match grm::sync_trees(config) {
                    Ok(success) => {
                        if !success {
                            process::exit(1)
                        }
                    }
                    Err(error) => {
                        print_error(&format!("Error syncing trees: {}", error));
                        process::exit(1);
                    }
                }
            }
            cmd::ReposAction::Status(args) => match &args.config {
                Some(config_path) => {
                    let config = match config::read_config(config_path) {
                        Ok(config) => config,
                        Err(error) => {
                            print_error(&error);
                            process::exit(1);
                        }
                    };
                    match grm::table::get_status_table(config) {
                        Ok((tables, errors)) => {
                            for table in tables {
                                println!("{}", table);
                            }
                            for error in errors {
                                print_error(&format!("Error: {}", error));
                            }
                        }
                        Err(error) => print_error(&format!("Error getting status: {}", error)),
                    }
                }
                None => {
                    let dir = match std::env::current_dir() {
                        Ok(dir) => dir,
                        Err(error) => {
                            print_error(&format!("Could not open current directory: {}", error));
                            process::exit(1);
                        }
                    };

                    match grm::table::show_single_repo_status(&dir) {
                        Ok((table, warnings)) => {
                            println!("{}", table);
                            for warning in warnings {
                                print_warning(&warning);
                            }
                        }
                        Err(error) => print_error(&format!("Error getting status: {}", error)),
                    }
                }
            },
            cmd::ReposAction::Find(find) => {
                let path = Path::new(&find.path);
                if !path.exists() {
                    print_error(&format!("Path \"{}\" does not exist", path.display()));
                    process::exit(1);
                }
                if !path.is_dir() {
                    print_error(&format!("Path \"{}\" is not a directory", path.display()));
                    process::exit(1);
                }

                let path = match path.canonicalize() {
                    Ok(path) => path,
                    Err(error) => {
                        print_error(&format!(
                            "Failed to canonicalize path \"{}\". This is a bug. Error message: {}",
                            &path.display(),
                            error
                        ));
                        process::exit(1);
                    }
                };

                let (found_repos, warnings) = match grm::find_in_tree(&path) {
                    Ok((repos, warnings)) => (repos, warnings),
                    Err(error) => {
                        print_error(&error);
                        process::exit(1);
                    }
                };

                let trees = grm::config::Trees::from_vec(vec![found_repos]);
                if trees.as_vec_ref().iter().all(|t| match &t.repos {
                    None => false,
                    Some(r) => r.is_empty(),
                }) {
                    print_warning("No repositories found");
                } else {
                    let config = trees.to_config();

                    let toml = match config.as_toml() {
                        Ok(toml) => toml,
                        Err(error) => {
                            print_error(&format!("Failed converting config to TOML: {}", &error));
                            process::exit(1);
                        }
                    };

                    print!("{}", toml);
                }
                for warning in warnings {
                    print_warning(&warning);
                }
            }
        },
        cmd::SubCommand::Worktree(args) => {
            let cwd = std::env::current_dir().unwrap_or_else(|error| {
                print_error(&format!("Could not open current directory: {}", error));
                process::exit(1);
            });

            match args.action {
                cmd::WorktreeAction::Add(action_args) => {
                    let track = match &action_args.track {
                        Some(branch) => {
                            let split = branch.split_once('/');

                            if split.is_none()
                                || split.unwrap().0.is_empty()
                                || split.unwrap().1.is_empty()
                            {
                                print_error("Tracking branch needs to match the pattern <remote>/<branch_name>");
                                process::exit(1);
                            };

                            // unwrap() here is safe because we checked for
                            // is_none() explictily before
                            let (remote_name, remote_branch_name) = split.unwrap();

                            Some((remote_name, remote_branch_name))
                        }
                        None => None,
                    };

                    match grm::add_worktree(
                        &cwd,
                        &action_args.name,
                        action_args.branch_namespace.as_deref(),
                        track,
                    ) {
                        Ok(_) => print_success(&format!("Worktree {} created", &action_args.name)),
                        Err(error) => {
                            print_error(&format!("Error creating worktree: {}", error));
                            process::exit(1);
                        }
                    }
                }
                cmd::WorktreeAction::Delete(action_args) => {
                    let worktree_dir = cwd.join(&action_args.name);

                    let worktree_config = match repo::read_worktree_root_config(&cwd) {
                        Ok(config) => config,
                        Err(error) => {
                            print_error(&format!("Error getting worktree configuration: {}", error));
                            process::exit(1);
                        }
                    };

                    let repo = grm::Repo::open(&cwd, true).unwrap_or_else(|error| {
                        print_error(&format!("Error opening repository: {}", error));
                        process::exit(1);
                    });

                    match repo.remove_worktree(&action_args.name, &worktree_dir, action_args.force, &worktree_config)
                    {
                        Ok(_) => print_success(&format!("Worktree {} deleted", &action_args.name)),
                        Err(error) => {
                            match error {
                                grm::WorktreeRemoveFailureReason::Error(msg) => {
                                    print_error(&msg);
                                    process::exit(1);
                                }
                                grm::WorktreeRemoveFailureReason::Changes(changes) => {
                                    print_warning(&format!(
                                        "Changes in worktree: {}. Refusing to delete",
                                        changes
                                    ));
                                }
                                grm::WorktreeRemoveFailureReason::NotMerged(message) => {
                                    print_warning(&message);
                                }
                            }
                            process::exit(1);
                        }
                    }
                }
                cmd::WorktreeAction::Status(_args) => {
                    let repo = grm::Repo::open(&cwd, true).unwrap_or_else(|error| {
                        print_error(&format!("Error opening repository: {}", error));
                        process::exit(1);
                    });

                    match grm::table::get_worktree_status_table(&repo, &cwd) {
                        Ok((table, errors)) => {
                            println!("{}", table);
                            for error in errors {
                                print_error(&format!("Error: {}", error));
                            }
                        }
                        Err(error) => print_error(&format!("Error getting status: {}", error)),
                    }
                }
                cmd::WorktreeAction::Convert(_args) => {
                    // Converting works like this:
                    // * Check whether there are uncommitted/unpushed changes
                    // * Move the contents of .git dir to the worktree directory
                    // * Remove all files
                    // * Set `core.bare` to `true`

                    let repo = grm::Repo::open(&cwd, false).unwrap_or_else(|error| {
                        if error.kind == grm::RepoErrorKind::NotFound {
                            print_error("Directory does not contain a git repository");
                        } else {
                            print_error(&format!("Opening repository failed: {}", error));
                        }
                        process::exit(1);
                    });

                    let status = repo.status(false).unwrap_or_else(|error| {
                        print_error(&format!("Failed getting repo changes: {}", error));
                        process::exit(1);
                    });
                    if status.changes.is_some() {
                        print_error("Changes found in repository, refusing to convert");
                        process::exit(1);
                    }

                    match repo.convert_to_worktree(&cwd) {
                        Ok(_) => print_success("Conversion done"),
                        Err(error) => print_error(&format!("Error during conversion: {}", error)),
                    }
                }
                cmd::WorktreeAction::Clean(_args) => {
                    let repo = grm::Repo::open(&cwd, true).unwrap_or_else(|error| {
                        if error.kind == grm::RepoErrorKind::NotFound {
                            print_error("Directory does not contain a git repository");
                        } else {
                            print_error(&format!("Opening repository failed: {}", error));
                        }
                        process::exit(1);
                    });

                    match repo.cleanup_worktrees(&cwd) {
                        Ok(warnings) => {
                            for warning in warnings {
                                print_warning(&warning);
                            }
                        }
                        Err(error) => {
                            print_error(&format!("Worktree cleanup failed: {}", error));
                            process::exit(1);
                        }
                    }

                    for unmanaged_worktree in
                        repo.find_unmanaged_worktrees(&cwd).unwrap_or_else(|error| {
                            print_error(&format!("Failed finding unmanaged worktrees: {}", error));
                            process::exit(1);
                        })
                    {
                        print_warning(&format!(
                            "Found {}, which is not a valid worktree directory!",
                            &unmanaged_worktree
                        ));
                    }
                }
            }
        }
    }
}
