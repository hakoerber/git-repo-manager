#![forbid(unsafe_code)]

use std::path::Path;
use std::process;

mod cmd;

use grm::auth;
use grm::config;
use grm::find_in_tree;
use grm::output::*;
use grm::path;
use grm::provider;
use grm::provider::Provider;
use grm::repo;
use grm::table;
use grm::tree;
use grm::worktree;

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
                    match tree::sync_trees(config, args.init_worktree == "true") {
                        Ok(success) => {
                            if !success {
                                process::exit(1)
                            }
                        }
                        Err(error) => {
                            print_error(&format!("Sync error: {}", error));
                            process::exit(1);
                        }
                    }
                }
                cmd::SyncAction::Remote(args) => {
                    let token = match auth::get_token_from_command(&args.token_command) {
                        Ok(token) => token,
                        Err(error) => {
                            print_error(&format!("Getting token from command failed: {}", error));
                            process::exit(1);
                        }
                    };

                    let filter =
                        provider::Filter::new(args.users, args.groups, args.owner, args.access);

                    if filter.empty() {
                        print_warning("You did not specify any filters, so no repos will match");
                    }

                    let worktree = args.worktree == "true";

                    let repos = match args.provider {
                        cmd::RemoteProvider::Github => {
                            match provider::Github::new(filter, token, args.api_url) {
                                Ok(provider) => provider,
                                Err(error) => {
                                    print_error(&format!("Sync error: {}", error));
                                    process::exit(1);
                                }
                            }
                            .get_repos(
                                worktree,
                                args.force_ssh,
                                args.remote_name,
                            )
                        }
                        cmd::RemoteProvider::Gitlab => {
                            match provider::Gitlab::new(filter, token, args.api_url) {
                                Ok(provider) => provider,
                                Err(error) => {
                                    print_error(&format!("Sync error: {}", error));
                                    process::exit(1);
                                }
                            }
                            .get_repos(
                                worktree,
                                args.force_ssh,
                                args.remote_name,
                            )
                        }
                    };

                    match repos {
                        Ok(repos) => {
                            let mut trees: Vec<config::ConfigTree> = vec![];

                            for (namespace, repolist) in repos {
                                let root = if let Some(namespace) = namespace {
                                    path::path_as_string(&Path::new(&args.root).join(namespace))
                                } else {
                                    path::path_as_string(Path::new(&args.root))
                                };

                                let tree = config::ConfigTree::from_repos(root, repolist);
                                trees.push(tree);
                            }

                            let config = config::Config::from_trees(trees);

                            match tree::sync_trees(config, args.init_worktree == "true") {
                                Ok(success) => {
                                    if !success {
                                        process::exit(1)
                                    }
                                }
                                Err(error) => {
                                    print_error(&format!("Sync error: {}", error));
                                    process::exit(1);
                                }
                            }
                        }
                        Err(error) => {
                            print_error(&format!("Sync error: {}", error));
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
                    match table::get_status_table(config) {
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

                    match table::show_single_repo_status(&dir) {
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

                    let (found_repos, warnings) = match find_in_tree(&path, args.exclude.as_deref())
                    {
                        Ok((repos, warnings)) => (repos, warnings),
                        Err(error) => {
                            print_error(&error);
                            process::exit(1);
                        }
                    };

                    let trees = config::ConfigTrees::from_trees(vec![found_repos]);
                    if trees.trees_ref().iter().all(|t| match &t.repos {
                        None => false,
                        Some(r) => r.is_empty(),
                    }) {
                        print_warning("No repositories found");
                    } else {
                        let mut config = trees.to_config();

                        config.normalize();

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
                cmd::FindAction::Config(args) => {
                    let config: config::ConfigProvider = match config::read_config(&args.config) {
                        Ok(config) => config,
                        Err(error) => {
                            print_error(&error);
                            process::exit(1);
                        }
                    };

                    let token = match auth::get_token_from_command(&config.token_command) {
                        Ok(token) => token,
                        Err(error) => {
                            print_error(&format!("Getting token from command failed: {}", error));
                            process::exit(1);
                        }
                    };

                    let filters = config.filters.unwrap_or(config::ConfigProviderFilter {
                        access: Some(false),
                        owner: Some(false),
                        users: Some(vec![]),
                        groups: Some(vec![]),
                    });

                    let filter = provider::Filter::new(
                        filters.users.unwrap_or_default(),
                        filters.groups.unwrap_or_default(),
                        filters.owner.unwrap_or(false),
                        filters.access.unwrap_or(false),
                    );

                    if filter.empty() {
                        print_warning("You did not specify any filters, so no repos will match");
                    }

                    let repos = match config.provider {
                        provider::RemoteProvider::Github => {
                            match match provider::Github::new(filter, token, config.api_url) {
                                Ok(provider) => provider,
                                Err(error) => {
                                    print_error(&format!("Error: {}", error));
                                    process::exit(1);
                                }
                            }
                            .get_repos(
                                config.worktree.unwrap_or(false),
                                config.force_ssh.unwrap_or(false),
                                config.remote_name,
                            ) {
                                Ok(provider) => provider,
                                Err(error) => {
                                    print_error(&format!("Error: {}", error));
                                    process::exit(1);
                                }
                            }
                        }
                        provider::RemoteProvider::Gitlab => {
                            match match provider::Gitlab::new(filter, token, config.api_url) {
                                Ok(provider) => provider,
                                Err(error) => {
                                    print_error(&format!("Error: {}", error));
                                    process::exit(1);
                                }
                            }
                            .get_repos(
                                config.worktree.unwrap_or(false),
                                config.force_ssh.unwrap_or(false),
                                config.remote_name,
                            ) {
                                Ok(provider) => provider,
                                Err(error) => {
                                    print_error(&format!("Error: {}", error));
                                    process::exit(1);
                                }
                            }
                        }
                    };

                    let mut trees = vec![];

                    for (namespace, namespace_repos) in repos {
                        let tree = config::ConfigTree {
                            root: if let Some(namespace) = namespace {
                                path::path_as_string(&Path::new(&config.root).join(namespace))
                            } else {
                                path::path_as_string(Path::new(&config.root))
                            },
                            repos: Some(
                                namespace_repos
                                    .into_iter()
                                    .map(config::RepoConfig::from_repo)
                                    .collect(),
                            ),
                        };
                        trees.push(tree);
                    }

                    let config = config::Config::from_trees(trees);

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
                cmd::FindAction::Remote(args) => {
                    let token = match auth::get_token_from_command(&args.token_command) {
                        Ok(token) => token,
                        Err(error) => {
                            print_error(&format!("Getting token from command failed: {}", error));
                            process::exit(1);
                        }
                    };

                    let filter =
                        provider::Filter::new(args.users, args.groups, args.owner, args.access);

                    if filter.empty() {
                        print_warning("You did not specify any filters, so no repos will match");
                    }

                    let worktree = args.worktree == "true";

                    let repos = match args.provider {
                        cmd::RemoteProvider::Github => {
                            match provider::Github::new(filter, token, args.api_url) {
                                Ok(provider) => provider,
                                Err(error) => {
                                    print_error(&format!("Error: {}", error));
                                    process::exit(1);
                                }
                            }
                            .get_repos(
                                worktree,
                                args.force_ssh,
                                args.remote_name,
                            )
                        }
                        cmd::RemoteProvider::Gitlab => {
                            match provider::Gitlab::new(filter, token, args.api_url) {
                                Ok(provider) => provider,
                                Err(error) => {
                                    print_error(&format!("Error: {}", error));
                                    process::exit(1);
                                }
                            }
                            .get_repos(
                                worktree,
                                args.force_ssh,
                                args.remote_name,
                            )
                        }
                    };

                    let repos = repos.unwrap_or_else(|error| {
                        print_error(&format!("Error: {}", error));
                        process::exit(1);
                    });

                    let mut trees: Vec<config::ConfigTree> = vec![];

                    for (namespace, repolist) in repos {
                        let tree = config::ConfigTree {
                            root: if let Some(namespace) = namespace {
                                path::path_as_string(&Path::new(&args.root).join(namespace))
                            } else {
                                path::path_as_string(Path::new(&args.root))
                            },
                            repos: Some(
                                repolist
                                    .into_iter()
                                    .map(config::RepoConfig::from_repo)
                                    .collect(),
                            ),
                        };
                        trees.push(tree);
                    }

                    let mut config = config::Config::from_trees(trees);

                    config.normalize();

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
            },
        },
        cmd::SubCommand::Worktree(args) => {
            let cwd = std::env::current_dir().unwrap_or_else(|error| {
                print_error(&format!("Could not open current directory: {}", error));
                process::exit(1);
            });

            match args.action {
                cmd::WorktreeAction::Add(action_args) => {
                    if action_args.track.is_some() && action_args.no_track {
                        print_warning("You are using --track and --no-track at the same time. --track will be ignored");
                    }
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

                    match worktree::add_worktree(
                        &cwd,
                        &action_args.name,
                        track,
                        action_args.no_track,
                    ) {
                        Ok(warnings) => {
                            if let Some(warnings) = warnings {
                                for warning in warnings {
                                    print_warning(&warning);
                                }
                            }
                            print_success(&format!("Worktree {} created", &action_args.name));
                        }
                        Err(error) => {
                            print_error(&format!("Error creating worktree: {}", error));
                            process::exit(1);
                        }
                    }
                }
                cmd::WorktreeAction::Delete(action_args) => {
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

                    let repo = repo::RepoHandle::open(&cwd, true).unwrap_or_else(|error| {
                        print_error(&format!("Error opening repository: {}", error));
                        process::exit(1);
                    });

                    match repo.remove_worktree(
                        &cwd,
                        &action_args.name,
                        Path::new(&action_args.name),
                        action_args.force,
                        &worktree_config,
                    ) {
                        Ok(_) => print_success(&format!("Worktree {} deleted", &action_args.name)),
                        Err(error) => {
                            match error {
                                repo::WorktreeRemoveFailureReason::Error(msg) => {
                                    print_error(&msg);
                                    process::exit(1);
                                }
                                repo::WorktreeRemoveFailureReason::Changes(changes) => {
                                    print_warning(&format!(
                                        "Changes in worktree: {}. Refusing to delete",
                                        changes
                                    ));
                                }
                                repo::WorktreeRemoveFailureReason::NotMerged(message) => {
                                    print_warning(&message);
                                }
                            }
                            process::exit(1);
                        }
                    }
                }
                cmd::WorktreeAction::Status(_args) => {
                    let repo = repo::RepoHandle::open(&cwd, true).unwrap_or_else(|error| {
                        print_error(&format!("Error opening repository: {}", error));
                        process::exit(1);
                    });

                    match table::get_worktree_status_table(&repo, &cwd) {
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

                    let repo = repo::RepoHandle::open(&cwd, false).unwrap_or_else(|error| {
                        if error.kind == repo::RepoErrorKind::NotFound {
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
                    let repo = repo::RepoHandle::open(&cwd, true).unwrap_or_else(|error| {
                        if error.kind == repo::RepoErrorKind::NotFound {
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
                    let repo = repo::RepoHandle::open(&cwd, true).unwrap_or_else(|error| {
                        if error.kind == repo::RepoErrorKind::NotFound {
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
                    let repo = repo::RepoHandle::open(&cwd, true).unwrap_or_else(|error| {
                        if error.kind == repo::RepoErrorKind::NotFound {
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
                    let repo = repo::RepoHandle::open(&cwd, true).unwrap_or_else(|error| {
                        if error.kind == repo::RepoErrorKind::NotFound {
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

                    let config = repo::read_worktree_root_config(&cwd).unwrap_or_else(|error| {
                        print_error(&format!("Failed to read worktree configuration: {}", error));
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
