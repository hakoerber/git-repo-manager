#![forbid(unsafe_code)]

use std::{
    path::{Path, PathBuf},
    process,
};

mod cmd;

use grm::{
    BranchName, RemoteName, auth, config, find_in_tree,
    output::{print, print_error, print_success, print_warning, println},
    provider::{self, Provider},
    repo, table, tree,
    worktree::{self, WorktreeName},
};

#[expect(clippy::cognitive_complexity, reason = "fine for main()")]
fn main() {
    let opts = cmd::parse();

    match opts.subcmd {
        cmd::SubCommand::Repos(repos) => {
            match repos.action {
                cmd::ReposAction::Sync(sync) => match sync {
                    cmd::SyncAction::Config(args) => {
                        let config = match config::read_config(&args.config) {
                            Ok(config) => config,
                            Err(error) => {
                                print_error(&error.to_string());
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
                                print_error(&format!("Sync error: {error}"));
                                process::exit(1);
                            }
                        }
                    }
                    cmd::SyncAction::Remote(args) => {
                        let token = match auth::get_token_from_command(&args.token_command) {
                            Ok(token) => token,
                            Err(error) => {
                                print_error(&format!("Getting token from command failed: {error}"));
                                process::exit(1);
                            }
                        };

                        let filter = provider::Filter::new(
                            args.users
                                .into_iter()
                                .map(|user| provider::User::new(user))
                                .collect(),
                            args.groups
                                .into_iter()
                                .map(|group| provider::Group::new(group))
                                .collect(),
                            args.owner,
                            args.access,
                        );

                        if filter.empty() {
                            print_warning(
                                "You did not specify any filters, so no repos will match",
                            );
                        }

                        let worktree = args.worktree == "true";

                        let repos =
                            match args.provider {
                                cmd::RemoteProvider::Github => match provider::Github::new(
                                    filter,
                                    token,
                                    args.api_url.map(provider::Url::new),
                                ) {
                                    Ok(provider) => provider,
                                    Err(error) => {
                                        print_error(&format!("Sync error: {error}"));
                                        process::exit(1);
                                    }
                                }
                                .get_repos(worktree, args.force_ssh, args.remote_name),
                                cmd::RemoteProvider::Gitlab => match provider::Gitlab::new(
                                    filter,
                                    token,
                                    args.api_url.map(provider::Url::new),
                                ) {
                                    Ok(provider) => provider,
                                    Err(error) => {
                                        print_error(&format!("Sync error: {error}"));
                                        process::exit(1);
                                    }
                                }
                                .get_repos(worktree, args.force_ssh, args.remote_name),
                            };

                        match repos {
                            Ok(repos) => {
                                let mut trees: Vec<config::Tree> = vec![];

                                #[expect(clippy::iter_over_hash_type, reason = "fine in this case")]
                                for (namespace, repolist) in repos {
                                    let root = if let Some(namespace) = namespace {
                                        PathBuf::from(&args.root).join(namespace.as_str())
                                    } else {
                                        PathBuf::from(&args.root)
                                    };

                                    let tree = config::Tree::from_repos(&root, repolist);
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
                                        print_error(&format!("Sync error: {error}"));
                                        process::exit(1);
                                    }
                                }
                            }
                            Err(error) => {
                                print_error(&format!("Sync error: {error}"));
                                process::exit(1);
                            }
                        }
                    }
                },
                cmd::ReposAction::Status(args) => {
                    if let Some(config_path) = args.config {
                        let config = match config::read_config(&config_path) {
                            Ok(config) => config,
                            Err(error) => {
                                print_error(&error.to_string());
                                process::exit(1);
                            }
                        };
                        match table::get_status_table(config) {
                            Ok((tables, errors)) => {
                                for table in tables {
                                    println(&format!("{table}"));
                                }
                                for error in errors {
                                    print_error(&format!("Error: {error}"));
                                }
                            }
                            Err(error) => {
                                print_error(&format!("Error getting status: {error}"));
                                process::exit(1);
                            }
                        }
                    } else {
                        let dir = match std::env::current_dir() {
                            Ok(dir) => dir,
                            Err(error) => {
                                print_error(&format!("Could not open current directory: {error}"));
                                process::exit(1);
                            }
                        };

                        match table::show_single_repo_status(&dir) {
                            Ok((table, warnings)) => {
                                println(&format!("{table}"));
                                for warning in warnings {
                                    print_warning(&warning);
                                }
                            }
                            Err(error) => {
                                print_error(&format!("Error getting status: {error}"));
                                process::exit(1);
                            }
                        }
                    }
                }
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

                        let exclusion_pattern = args.exclude.as_ref().map(|s|
                        match regex::Regex::new(s) {
                            Ok(regex) => regex,
                            Err(error) => {
                                print_error(&format!(
                                    "Failed to canonicalize path \"{}\". This is a bug. Error message: {}",
                                    &path.display(),
                                    error
                                ));
                                process::exit(1);
                            }
                        }
                    );

                        let (found_repos, warnings) =
                            match find_in_tree(&path, exclusion_pattern.as_ref()) {
                                Ok((repos, warnings)) => (repos, warnings),
                                Err(error) => {
                                    print_error(&error.to_string());
                                    process::exit(1);
                                }
                            };

                        let trees = config::ConfigTrees::from_trees(vec![found_repos]);
                        if trees.trees_ref().iter().all(|t| match t.repos {
                            None => false,
                            Some(ref r) => r.is_empty(),
                        }) {
                            print_warning("No repositories found");
                        } else {
                            let mut config = trees.to_config();

                            if let Err(error) = config.normalize() {
                                print_error(&format!("Path error: {error}"));
                                process::exit(1);
                            }

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
                                    print(&toml);
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
                                    print(&yaml);
                                }
                            }
                        }
                        for warning in warnings {
                            print_warning(&warning);
                        }
                    }
                    cmd::FindAction::Config(args) => {
                        let config: config::ConfigProvider = match config::read_config(&args.config)
                        {
                            Ok(config) => config,
                            Err(error) => {
                                print_error(&error.to_string());
                                process::exit(1);
                            }
                        };

                        let token = match auth::get_token_from_command(&config.token_command) {
                            Ok(token) => token,
                            Err(error) => {
                                print_error(&format!("Getting token from command failed: {error}"));
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
                            filters
                                .users
                                .unwrap_or_default()
                                .into_iter()
                                .map(Into::into)
                                .collect(),
                            filters
                                .groups
                                .unwrap_or_default()
                                .into_iter()
                                .map(Into::into)
                                .collect(),
                            filters.owner.unwrap_or(false),
                            filters.access.unwrap_or(false),
                        );

                        if filter.empty() {
                            print_warning(
                                "You did not specify any filters, so no repos will match",
                            );
                        }

                        let repos = match config.provider.into() {
                            provider::RemoteProvider::Github => {
                                match match provider::Github::new(
                                    filter,
                                    token,
                                    config.api_url.map(provider::Url::new),
                                ) {
                                    Ok(provider) => provider,
                                    Err(error) => {
                                        print_error(&format!("Error: {error}"));
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
                                        print_error(&format!("Error: {error}"));
                                        process::exit(1);
                                    }
                                }
                            }
                            provider::RemoteProvider::Gitlab => {
                                match match provider::Gitlab::new(
                                    filter,
                                    token,
                                    config.api_url.map(provider::Url::new),
                                ) {
                                    Ok(provider) => provider,
                                    Err(error) => {
                                        print_error(&format!("Error: {error}"));
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
                                        print_error(&format!("Error: {error}"));
                                        process::exit(1);
                                    }
                                }
                            }
                        };

                        let mut trees = vec![];

                        #[expect(clippy::iter_over_hash_type, reason = "fine in this case")]
                        for (namespace, namespace_repos) in repos {
                            let tree = config::Tree {
                                root: tree::Root::new(if let Some(namespace) = namespace {
                                    PathBuf::from(&config.root).join(namespace.as_str())
                                } else {
                                    PathBuf::from(&config.root)
                                })
                                .into(),
                                repos: Some(namespace_repos.into_iter().map(Into::into).collect()),
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
                                print(&toml);
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
                                print(&yaml);
                            }
                        }
                    }
                    cmd::FindAction::Remote(args) => {
                        let token = match auth::get_token_from_command(&args.token_command) {
                            Ok(token) => token,
                            Err(error) => {
                                print_error(&format!("Getting token from command failed: {error}"));
                                process::exit(1);
                            }
                        };

                        let filter = provider::Filter::new(
                            args.users
                                .into_iter()
                                .map(|user| provider::User::new(user))
                                .collect(),
                            args.groups
                                .into_iter()
                                .map(|group| provider::Group::new(group))
                                .collect(),
                            args.owner,
                            args.access,
                        );

                        if filter.empty() {
                            print_warning(
                                "You did not specify any filters, so no repos will match",
                            );
                        }

                        let worktree = args.worktree == "true";

                        let repos =
                            match args.provider {
                                cmd::RemoteProvider::Github => match provider::Github::new(
                                    filter,
                                    token,
                                    args.api_url.map(provider::Url::new),
                                ) {
                                    Ok(provider) => provider,
                                    Err(error) => {
                                        print_error(&format!("Error: {error}"));
                                        process::exit(1);
                                    }
                                }
                                .get_repos(worktree, args.force_ssh, args.remote_name),
                                cmd::RemoteProvider::Gitlab => match provider::Gitlab::new(
                                    filter,
                                    token,
                                    args.api_url.map(provider::Url::new),
                                ) {
                                    Ok(provider) => provider,
                                    Err(error) => {
                                        print_error(&format!("Error: {error}"));
                                        process::exit(1);
                                    }
                                }
                                .get_repos(worktree, args.force_ssh, args.remote_name),
                            };

                        let repos = repos.unwrap_or_else(|error| {
                            print_error(&format!("Error: {error}"));
                            process::exit(1);
                        });

                        let mut trees: Vec<config::Tree> = vec![];

                        #[expect(clippy::iter_over_hash_type, reason = "fine in this case")]
                        for (namespace, repolist) in repos {
                            let tree = config::Tree {
                                root: tree::Root::new(if let Some(namespace) = namespace {
                                    PathBuf::from(&args.root).join(namespace.as_str())
                                } else {
                                    PathBuf::from(&args.root)
                                })
                                .into(),
                                repos: Some(repolist.into_iter().map(Into::into).collect()),
                            };
                            trees.push(tree);
                        }

                        let mut config = config::Config::from_trees(trees);

                        if let Err(error) = config.normalize() {
                            print_error(&format!("Path error: {error}"));
                            process::exit(1);
                        }

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
                                print(&toml);
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
                                print(&yaml);
                            }
                        }
                    }
                },
            }
        }
        cmd::SubCommand::Worktree(args) => {
            let cwd = std::env::current_dir().unwrap_or_else(|error| {
                print_error(&format!("Could not open current directory: {error}"));
                process::exit(1);
            });

            match args.action {
                cmd::WorktreeAction::Add(action_args) => {
                    if action_args.track.is_some() && action_args.no_track {
                        print_warning(
                            "You are using --track and --no-track at the same time. --track will be ignored",
                        );
                    }
                    let track = match action_args.track {
                        Some(ref branch) => {
                            let split = branch.split_once('/');

                            let (remote_name, remote_branch_name) = match split {
                                None => {
                                    print_error(
                                        "Tracking branch needs to match the pattern <remote>/<branch_name>, no slash found",
                                    );
                                    process::exit(1);
                                }
                                Some(s) if s.0.is_empty() || s.1.is_empty() => {
                                    print_error(
                                        "Tracking branch needs to match the pattern <remote>/<branch_name>",
                                    );
                                    process::exit(1);
                                }
                                Some((remote_name, remote_branch_name)) => {
                                    (remote_name, remote_branch_name)
                                }
                            };

                            Some((
                                RemoteName::new(remote_name.to_owned()),
                                BranchName::new(remote_branch_name.to_owned()),
                            ))
                        }
                        None => None,
                    };

                    match worktree::add_worktree(
                        &cwd,
                        &WorktreeName::new(action_args.name.clone()),
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
                            print_error(&format!("Error creating worktree: {error}"));
                            process::exit(1);
                        }
                    }
                }
                cmd::WorktreeAction::Delete(action_args) => {
                    let worktree_config: Option<repo::WorktreeRootConfig> =
                        match config::read_worktree_root_config(&cwd) {
                            Ok(config) => config.map(Into::into),
                            Err(error) => {
                                print_error(&format!(
                                    "Error getting worktree configuration: {error}"
                                ));
                                process::exit(1);
                            }
                        };

                    let repo = repo::RepoHandle::open(&cwd, true).unwrap_or_else(|error| {
                        print_error(&format!("Error opening repository: {error}"));
                        process::exit(1);
                    });

                    match repo.remove_worktree(
                        &cwd,
                        &WorktreeName::new(action_args.name.clone()),
                        Path::new(&action_args.name),
                        action_args.force,
                        worktree_config.as_ref(),
                    ) {
                        Ok(()) => print_success(&format!("Worktree {} deleted", &action_args.name)),
                        Err(error) => {
                            match error {
                                repo::Error::WorktreeRemovalFailure(reason) => match reason {
                                    repo::WorktreeRemoveFailureReason::Error(msg) => {
                                        print_error(&msg);
                                        process::exit(1);
                                    }
                                    repo::WorktreeRemoveFailureReason::Changes(changes) => {
                                        print_warning(format!(
                                            "Changes in worktree: {changes}. Refusing to delete"
                                        ));
                                    }
                                    repo::WorktreeRemoveFailureReason::NotMerged(message) => {
                                        print_warning(&message);
                                    }
                                },
                                e => {
                                    print_error(&e.to_string());
                                    process::exit(1);
                                }
                            }
                            process::exit(1);
                        }
                    }
                }
                cmd::WorktreeAction::Status(_args) => {
                    let repo = repo::RepoHandle::open(&cwd, true).unwrap_or_else(|error| {
                        print_error(&format!("Error opening repository: {error}"));
                        process::exit(1);
                    });

                    match table::get_worktree_status_table(&repo, &cwd) {
                        Ok((table, errors)) => {
                            println(&format!("{table}"));
                            for error in errors {
                                print_error(&format!("Error: {error}"));
                            }
                        }
                        Err(error) => {
                            print_error(&format!("Error getting status: {error}"));
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
                        if matches!(error, repo::Error::NotFound) {
                            print_error("Directory does not contain a git repository");
                        } else {
                            print_error(&format!("Opening repository failed: {error}"));
                        }
                        process::exit(1);
                    });

                    match repo.convert_to_worktree(&cwd) {
                        Ok(()) => print_success("Conversion done"),
                        Err(error) => {
                            match error {
                                repo::Error::WorktreeConversionFailure(reason) => match reason {
                                    repo::WorktreeConversionFailureReason::Changes => {
                                        print_error(
                                            "Changes found in repository, refusing to convert",
                                        );
                                    }
                                    repo::WorktreeConversionFailureReason::Ignored => {
                                        print_error(
                                            "Ignored files found in repository, refusing to convert. Run git clean -f -d -X to remove them manually.",
                                        );
                                    }
                                    repo::WorktreeConversionFailureReason::Error(error) => {
                                        print_error(&format!("Error during conversion: {error}"));
                                    }
                                },
                                e => print_error(&e.to_string()),
                            }

                            process::exit(1);
                        }
                    }
                }
                cmd::WorktreeAction::Clean(_args) => {
                    let repo = repo::RepoHandle::open(&cwd, true).unwrap_or_else(|error| {
                        if matches!(error, repo::Error::NotFound) {
                            print_error("Directory does not contain a git repository");
                        } else {
                            print_error(&format!("Opening repository failed: {error}"));
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
                            print_error(&format!("Worktree cleanup failed: {error}"));
                            process::exit(1);
                        }
                    }

                    for unmanaged_worktree in
                        repo.find_unmanaged_worktrees(&cwd).unwrap_or_else(|error| {
                            print_error(&format!("Failed finding unmanaged worktrees: {error}"));
                            process::exit(1);
                        })
                    {
                        print_warning(format!(
                            "Found {}, which is not a valid worktree directory!",
                            &unmanaged_worktree
                        ));
                    }
                }
                cmd::WorktreeAction::Fetch(_args) => {
                    let repo = repo::RepoHandle::open(&cwd, true).unwrap_or_else(|error| {
                        if matches!(error, repo::Error::NotFound) {
                            print_error("Directory does not contain a git repository");
                        } else {
                            print_error(&format!("Opening repository failed: {error}"));
                        }
                        process::exit(1);
                    });

                    repo.fetchall().unwrap_or_else(|error| {
                        print_error(&format!("Error fetching remotes: {error}"));
                        process::exit(1);
                    });
                    print_success("Fetched from all remotes");
                }
                cmd::WorktreeAction::Pull(args) => {
                    let repo = repo::RepoHandle::open(&cwd, true).unwrap_or_else(|error| {
                        if matches!(error, repo::Error::NotFound) {
                            print_error("Directory does not contain a git repository");
                        } else {
                            print_error(&format!("Opening repository failed: {error}"));
                        }
                        process::exit(1);
                    });

                    repo.fetchall().unwrap_or_else(|error| {
                        print_error(&format!("Error fetching remotes: {error}"));
                        process::exit(1);
                    });

                    let mut failures = false;
                    for worktree in repo.get_worktrees().unwrap_or_else(|error| {
                        print_error(&format!("Error getting worktrees: {error}"));
                        process::exit(1);
                    }) {
                        if let Some(warning) = worktree
                            .forward_branch(args.rebase, args.stash)
                            .unwrap_or_else(|error| {
                                print_error(&format!("Error updating worktree branch: {error}"));
                                process::exit(1);
                            })
                        {
                            print_warning(format!("{}: {}", worktree.name(), warning));
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
                        if matches!(error, repo::Error::NotFound) {
                            print_error("Directory does not contain a git repository");
                        } else {
                            print_error(&format!("Opening repository failed: {error}"));
                        }
                        process::exit(1);
                    });

                    if args.pull {
                        repo.fetchall().unwrap_or_else(|error| {
                            print_error(&format!("Error fetching remotes: {error}"));
                            process::exit(1);
                        });
                    }

                    let config = config::read_worktree_root_config(&cwd)
                        .unwrap_or_else(|error| {
                            print_error(&format!("Failed to read worktree configuration: {error}"));
                            process::exit(1);
                        })
                        .map(Into::into);

                    let worktrees = repo.get_worktrees().unwrap_or_else(|error| {
                        print_error(&format!("Error getting worktrees: {error}"));
                        process::exit(1);
                    });

                    let mut failures = false;

                    for worktree in &worktrees {
                        if args.pull {
                            if let Some(warning) = worktree
                                .forward_branch(args.rebase, args.stash)
                                .unwrap_or_else(|error| {
                                    print_error(&format!(
                                        "Error updating worktree branch: {error}"
                                    ));
                                    process::exit(1);
                                })
                            {
                                failures = true;
                                print_warning(format!("{}: {}", worktree.name(), warning));
                            }
                        }
                    }

                    for worktree in &worktrees {
                        if let Some(warning) = worktree
                            .rebase_onto_default(&config, args.stash)
                            .unwrap_or_else(|error| {
                                print_error(&format!("Error rebasing worktree branch: {error}"));
                                process::exit(1);
                            })
                        {
                            failures = true;
                            print_warning(format!("{}: {}", worktree.name(), warning));
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
