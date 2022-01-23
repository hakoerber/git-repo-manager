use std::path::Path;
use std::process;

mod cmd;

use grm::config;
use grm::output::*;
use grm::provider::Provider;
use grm::repo;

fn main() {
    let opts = cmd::parse();

    match opts.subcmd {
        cmd::SubCommand::Repos(repos) => match repos.action {
            cmd::ReposAction::Sync(sync) => match sync {
                cmd::SyncAction::Config(args) => {
                    let config = match config::read_config(&args.config) {
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
                cmd::SyncAction::Remote(args) => {
                    let users = if args.users.is_empty() {
                        None
                    } else {
                        Some(args.users)
                    };

                    let groups = if args.groups.is_empty() {
                        None
                    } else {
                        Some(args.groups)
                    };

                    let token_process = std::process::Command::new("/usr/bin/env")
                        .arg("sh")
                        .arg("-c")
                        .arg(args.token_command)
                        .output();

                    let token: String = match token_process {
                        Err(error) => {
                            print_error(&format!("Failed to run token-command: {}", error));
                            process::exit(1);
                        }
                        Ok(output) => {
                            let stderr = String::from_utf8(output.stderr).unwrap();
                            let stdout = String::from_utf8(output.stdout).unwrap();

                            if !output.status.success() {
                                if !stderr.is_empty() {
                                    print_error(&format!("Token command failed: {}", stderr));
                                } else {
                                    print_error("Token command failed.");
                                }
                            }
                            if !stderr.is_empty() {
                                print_error(&format!("Token command produced stderr: {}", stderr));
                            }

                            if stdout.is_empty() {
                                print_error("Token command did not produce output");
                            }

                            let token = stdout.split('\n').next().unwrap();

                            token.to_string()
                        }
                    };

                    let filter = grm::provider::Filter::new(users, groups, args.owner);
                    let github = grm::provider::Github::new(filter, token);

                    match github.get_repos() {
                        Ok(repos) => println!("{:?}", repos),
                        Err(error) => {
                            print_error(&format!("Error: {}", error));
                            process::exit(1);
                        }
                    }
                }
            },
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
                        Err(error) => {
                            print_error(&format!("Error getting status: {}", error));
                            process::exit(1);
                        }
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
                        Err(error) => {
                            print_error(&format!("Error getting status: {}", error));
                            process::exit(1);
                        }
                    }
                }
            },
            cmd::ReposAction::Find(find) => match find {
                cmd::FindAction::Local(args) => {
                    let path = Path::new(&args.path);
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

                        match args.format {
                            cmd::ConfigFormat::Toml => {
                                let toml = match config.as_toml() {
                                    Ok(toml) => toml,
                                    Err(error) => {
                                        print_error(&format!(
                                            "Failed converting config to TOML: {}",
                                            &error
                                        ));
                                        process::exit(1);
                                    }
                                };
                                print!("{}", toml);
                            }
                            cmd::ConfigFormat::Yaml => {
                                let yaml = match config.as_yaml() {
                                    Ok(yaml) => yaml,
                                    Err(error) => {
                                        print_error(&format!(
                                            "Failed converting config to YAML: {}",
                                            &error
                                        ));
                                        process::exit(1);
                                    }
                                };
                                print!("{}", yaml);
                            }
                        }
                    }
                    for warning in warnings {
                        print_warning(&warning);
                    }
                }
                cmd::FindAction::Remote(args) => {
                    let users = if args.users.is_empty() {
                        None
                    } else {
                        Some(args.users)
                    };

                    let groups = if args.groups.is_empty() {
                        None
                    } else {
                        Some(args.groups)
                    };

                    let token_process = std::process::Command::new("/usr/bin/env")
                        .arg("sh")
                        .arg("-c")
                        .arg(args.token_command)
                        .output();

                    let token: String = match token_process {
                        Err(error) => {
                            print_error(&format!("Failed to run token-command: {}", error));
                            process::exit(1);
                        }
                        Ok(output) => {
                            let stderr = String::from_utf8(output.stderr).unwrap();
                            let stdout = String::from_utf8(output.stdout).unwrap();

                            if !output.status.success() {
                                if !stderr.is_empty() {
                                    print_error(&format!("Token command failed: {}", stderr));
                                } else {
                                    print_error("Token command failed.");
                                }
                            }
                            if !stderr.is_empty() {
                                print_error(&format!("Token command produced stderr: {}", stderr));
                            }

                            if stdout.is_empty() {
                                print_error("Token command did not produce output");
                            }

                            let token = stdout.split('\n').next().unwrap();

                            token.to_string()
                        }
                    };

                    let filter = grm::provider::Filter::new(users, groups, args.owner);
                    let github = grm::provider::Github::new(filter, token);

                    match github.get_repos() {
                        Ok(repos) => {
                            let mut trees: Vec<config::Tree> = vec![];

                            for (namespace, repolist) in repos {
                                let tree = config::Tree {
                                    root: Path::new(&args.root)
                                        .join(namespace)
                                        .display()
                                        .to_string(),
                                    repos: Some(repolist),
                                };
                                trees.push(tree);
                            }

                            let config = config::Config {
                                trees: config::Trees::from_vec(trees),
                            };

                            match args.format {
                                cmd::ConfigFormat::Toml => {
                                    let toml = match config.as_toml() {
                                        Ok(toml) => toml,
                                        Err(error) => {
                                            print_error(&format!(
                                                "Failed converting config to TOML: {}",
                                                &error
                                            ));
                                            process::exit(1);
                                        }
                                    };
                                    print!("{}", toml);
                                }
                                cmd::ConfigFormat::Yaml => {
                                    let yaml = match config.as_yaml() {
                                        Ok(yaml) => yaml,
                                        Err(error) => {
                                            print_error(&format!(
                                                "Failed converting config to YAML: {}",
                                                &error
                                            ));
                                            process::exit(1);
                                        }
                                    };
                                    print!("{}", yaml);
                                }
                            }
                        }
                        Err(error) => {
                            print_error(&format!("Error: {}", error));
                            process::exit(1);
                        }
                    }
                }
            },
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

                    let mut name: &str = &action_args.name;
                    let subdirectory;
                    let split = name.split_once('/');
                    match split {
                        None => subdirectory = None,
                        Some(split) => {
                            if split.0.is_empty() || split.1.is_empty() {
                                print_error("Worktree name cannot start or end with a slash");
                                process::exit(1);
                            } else {
                                (subdirectory, name) = (Some(Path::new(split.0)), split.1);
                            }
                        }
                    }

                    match grm::add_worktree(&cwd, name, subdirectory, track, action_args.no_track) {
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
                            print_error(&format!(
                                "Error getting worktree configuration: {}",
                                error
                            ));
                            process::exit(1);
                        }
                    };

                    let repo = grm::Repo::open(&cwd, true).unwrap_or_else(|error| {
                        print_error(&format!("Error opening repository: {}", error));
                        process::exit(1);
                    });

                    match repo.remove_worktree(
                        &action_args.name,
                        &worktree_dir,
                        action_args.force,
                        &worktree_config,
                    ) {
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
                        Err(error) => {
                            print_error(&format!("Error getting status: {}", error));
                            process::exit(1);
                        }
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

                    match repo.convert_to_worktree(&cwd) {
                        Ok(_) => print_success("Conversion done"),
                        Err(reason) => {
                            match reason {
                                repo::WorktreeConversionFailureReason::Changes => {
                                    print_error("Changes found in repository, refusing to convert");
                                }
                                repo::WorktreeConversionFailureReason::Ignored => {
                                    print_error("Ignored files found in repository, refusing to convert. Run git clean -f -d -X to remove them manually.");
                                }
                                repo::WorktreeConversionFailureReason::Error(error) => {
                                    print_error(&format!("Error during conversion: {}", error));
                                }
                            }
                            process::exit(1);
                        }
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
                cmd::WorktreeAction::Fetch(_args) => {
                    let repo = grm::Repo::open(&cwd, true).unwrap_or_else(|error| {
                        if error.kind == grm::RepoErrorKind::NotFound {
                            print_error("Directory does not contain a git repository");
                        } else {
                            print_error(&format!("Opening repository failed: {}", error));
                        }
                        process::exit(1);
                    });

                    repo.fetchall().unwrap_or_else(|error| {
                        print_error(&format!("Error fetching remotes: {}", error));
                        process::exit(1);
                    });
                    print_success("Fetched from all remotes");
                }
                cmd::WorktreeAction::Pull(args) => {
                    let repo = grm::Repo::open(&cwd, true).unwrap_or_else(|error| {
                        if error.kind == grm::RepoErrorKind::NotFound {
                            print_error("Directory does not contain a git repository");
                        } else {
                            print_error(&format!("Opening repository failed: {}", error));
                        }
                        process::exit(1);
                    });

                    repo.fetchall().unwrap_or_else(|error| {
                        print_error(&format!("Error fetching remotes: {}", error));
                        process::exit(1);
                    });

                    let mut failures = false;
                    for worktree in repo.get_worktrees().unwrap_or_else(|error| {
                        print_error(&format!("Error getting worktrees: {}", error));
                        process::exit(1);
                    }) {
                        if let Some(warning) = worktree
                            .forward_branch(args.rebase, args.stash)
                            .unwrap_or_else(|error| {
                                print_error(&format!("Error updating worktree branch: {}", error));
                                process::exit(1);
                            })
                        {
                            print_warning(&format!("{}: {}", worktree.name(), warning));
                            failures = true;
                        } else {
                            print_success(&format!("{}: Done", worktree.name()));
                        }
                    }
                    if failures {
                        process::exit(1);
                    }
                }
                cmd::WorktreeAction::Rebase(args) => {
                    if args.rebase && !args.pull {
                        print_error("There is no point in using --rebase without --pull");
                        process::exit(1);
                    }
                    let repo = grm::Repo::open(&cwd, true).unwrap_or_else(|error| {
                        if error.kind == grm::RepoErrorKind::NotFound {
                            print_error("Directory does not contain a git repository");
                        } else {
                            print_error(&format!("Opening repository failed: {}", error));
                        }
                        process::exit(1);
                    });

                    if args.pull {
                        repo.fetchall().unwrap_or_else(|error| {
                            print_error(&format!("Error fetching remotes: {}", error));
                            process::exit(1);
                        });
                    }

                    let config =
                        grm::repo::read_worktree_root_config(&cwd).unwrap_or_else(|error| {
                            print_error(&format!(
                                "Failed to read worktree configuration: {}",
                                error
                            ));
                            process::exit(1);
                        });

                    let worktrees = repo.get_worktrees().unwrap_or_else(|error| {
                        print_error(&format!("Error getting worktrees: {}", error));
                        process::exit(1);
                    });

                    let mut failures = false;

                    for worktree in &worktrees {
                        if args.pull {
                            if let Some(warning) = worktree
                                .forward_branch(args.rebase, args.stash)
                                .unwrap_or_else(|error| {
                                    print_error(&format!(
                                        "Error updating worktree branch: {}",
                                        error
                                    ));
                                    process::exit(1);
                                })
                            {
                                failures = true;
                                print_warning(&format!("{}: {}", worktree.name(), warning));
                            }
                        }
                    }

                    for worktree in &worktrees {
                        if let Some(warning) = worktree
                            .rebase_onto_default(&config, args.stash)
                            .unwrap_or_else(|error| {
                                print_error(&format!("Error rebasing worktree branch: {}", error));
                                process::exit(1);
                            })
                        {
                            failures = true;
                            print_warning(&format!("{}: {}", worktree.name(), warning));
                        } else {
                            print_success(&format!("{}: Done", worktree.name()));
                        }
                    }
                    if failures {
                        process::exit(1);
                    }
                }
            }
        }
    }
}
