#![forbid(unsafe_code)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "cmd args are passed by value to make the call hierarchy more obvious"
)]

use std::{
    path::{Path, PathBuf},
    process::{ExitCode, Termination},
};

mod cmd;

use grm::{
    BranchName, RemoteName, auth, config, find_in_tree,
    output::{print, print_error, print_success, print_warning, println},
    provider::{self, Provider},
    repo::{self, WorktreeSetup},
    table, tree,
    worktree::{self, WorktreeName},
};

struct MainError {
    exit_code: Option<ExitCode>,
    message: String,
}

enum MainResult {
    Success,
    Failure(MainError),
}

impl Termination for MainResult {
    fn report(self) -> ExitCode {
        match self {
            Self::Success => ExitCode::SUCCESS,
            Self::Failure(main_error) => {
                print_error(&main_error.message);
                match main_error.exit_code {
                    Some(code) => code,
                    None => ExitCode::FAILURE,
                }
            }
        }
    }
}

fn main() -> MainResult {
    match main_inner() {
        Ok(()) => MainResult::Success,
        Err(e) => MainResult::Failure(e),
    }
}

fn handle_repos_sync_config(args: cmd::Config) -> Result<(), MainError> {
    let config = config::read_config(Path::new(&args.config)).map_err(|e| MainError {
        exit_code: None,
        message: e.to_string(),
    })?;

    if tree::sync_trees(config, args.init_worktree == "true")
        .map_err(|e| MainError {
            exit_code: None,
            message: format!("Sync error: {e}"),
        })?
        .is_success()
    {
        Ok(())
    } else {
        Err(MainError {
            exit_code: None,
            message: "Sync failed".to_owned(),
        })
    }
}

fn handle_repos_sync_remote(args: cmd::SyncRemoteArgs) -> Result<(), MainError> {
    let token = auth::get_token_from_command(&args.token_command).map_err(|e| MainError {
        exit_code: None,
        message: format!("Getting token from command failed: {e}"),
    })?;

    let filter = provider::Filter::new(
        args.users.into_iter().map(provider::User::new).collect(),
        args.groups.into_iter().map(provider::Group::new).collect(),
        args.owner,
        args.access,
    );

    if filter.empty() {
        print_warning("You did not specify any filters, so no repos will match");
    }

    let worktree = (args.worktree == "true").into();

    let repos = match args.provider {
        cmd::RemoteProvider::Github => {
            provider::Github::new(filter, token, args.api_url.map(provider::Url::new))
                .map_err(|e| MainError {
                    exit_code: None,
                    message: format!("Sync error: {e}"),
                })?
                .get_repos(
                    worktree,
                    args.force_ssh,
                    args.remote_name.map(RemoteName::new),
                )
        }
        cmd::RemoteProvider::Gitlab => {
            provider::Gitlab::new(filter, token, args.api_url.map(provider::Url::new))
                .map_err(|e| MainError {
                    exit_code: None,
                    message: format!("Sync error: {e}"),
                })?
                .get_repos(
                    worktree,
                    args.force_ssh,
                    args.remote_name.map(RemoteName::new),
                )
        }
    }
    .map_err(|e| MainError {
        exit_code: None,
        message: format!("Sync error: {e}"),
    })?;

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

    if tree::sync_trees(config, args.init_worktree == "true")
        .map_err(|e| MainError {
            exit_code: None,
            message: format!("Sync error: {e}"),
        })?
        .is_success()
    {
        Ok(())
    } else {
        Err(MainError {
            exit_code: None,
            message: "Sync failed".to_owned(),
        })
    }
}

fn handle_repos_sync(sync: cmd::SyncAction) -> Result<(), MainError> {
    match sync {
        cmd::SyncAction::Config(args) => handle_repos_sync_config(args)?,
        cmd::SyncAction::Remote(args) => handle_repos_sync_remote(args)?,
    }
    Ok(())
}

fn handle_repos_status(args: cmd::OptionalConfig) -> Result<(), MainError> {
    if let Some(config_path) = args.config {
        let config = config::read_config(Path::new(&config_path)).map_err(|e| MainError {
            exit_code: None,
            message: e.to_string(),
        })?;

        let (tables, errors) = table::get_status_table(config).map_err(|e| MainError {
            exit_code: None,
            message: format!("Error getting status: {e}"),
        })?;

        for table in tables {
            println(&format!("{table}"));
        }
        for error in errors {
            print_error(&format!("Error: {error}"));
        }
    } else {
        let dir = std::env::current_dir().map_err(|e| MainError {
            exit_code: None,
            message: format!("Could not open current directory: {e}"),
        })?;

        let (table, warnings) = table::show_single_repo_status(&dir).map_err(|e| MainError {
            exit_code: None,
            message: format!("Error getting status: {e}"),
        })?;

        println(&format!("{table}"));
        for warning in warnings {
            print_warning(&warning);
        }
    }
    Ok(())
}

fn handle_repos_find_local(args: cmd::FindLocalArgs) -> Result<(), MainError> {
    let path = Path::new(&args.path);
    if !path.exists() {
        return Err(MainError {
            exit_code: None,
            message: format!("Path \"{}\" does not exist", path.display()),
        });
    }
    if !path.is_dir() {
        return Err(MainError {
            exit_code: None,
            message: format!("Path \"{}\" is not a directory", path.display()),
        });
    }

    let path = path.canonicalize().map_err(|e| MainError {
        exit_code: None,
        message: format!(
            "Failed to canonicalize path \"{}\". This is a bug. Error message: {}",
            &path.display(),
            e
        ),
    })?;

    let exclusion_pattern = args
        .exclude
        .as_ref()
        .map(|s| match regex::Regex::new(s) {
            Ok(regex) => Ok(regex),
            Err(error) => Err(MainError {
                exit_code: None,
                message: format!(
                    "Failed to canonicalize path \"{}\". This is a bug. Error message: {}",
                    &path.display(),
                    error
                ),
            }),
        })
        .transpose()?;

    let (found_repos, warnings) =
        find_in_tree(&path, exclusion_pattern.as_ref()).map_err(|e| MainError {
            exit_code: None,
            message: e.to_string(),
        })?;

    let trees = config::ConfigTrees::from_trees(vec![found_repos]);
    if trees.trees_ref().iter().all(|t| match t.repos {
        None => false,
        Some(ref r) => r.is_empty(),
    }) {
        print_warning("No repositories found");
    } else {
        let mut config = trees.to_config();

        if let Err(error) = config.normalize() {
            return Err(MainError {
                exit_code: None,
                message: format!("Path error: {error}"),
            });
        }

        match args.format {
            cmd::ConfigFormat::Toml => {
                let toml = config.as_toml().map_err(|e| MainError {
                    exit_code: None,
                    message: format!("Failed converting config to TOML: {}", &e),
                })?;
                print(&toml);
            }
            cmd::ConfigFormat::Yaml => {
                let yaml = config.as_yaml().map_err(|e| MainError {
                    exit_code: None,
                    message: format!("Failed converting config to YAML: {}", &e),
                })?;
                print(&yaml);
            }
        }
    }
    for warning in warnings {
        print_warning(&warning);
    }
    Ok(())
}

fn handle_repos_find_config(args: cmd::FindConfigArgs) -> Result<(), MainError> {
    let config: config::ConfigProvider =
        config::read_config(Path::new(&args.config)).map_err(|e| MainError {
            exit_code: None,
            message: e.to_string(),
        })?;

    let token = auth::get_token_from_command(&config.token_command).map_err(|e| MainError {
        exit_code: None,
        message: format!("Getting token from command failed: {e}"),
    })?;

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
        print_warning("You did not specify any filters, so no repos will match");
    }

    let repos = match config.provider.into() {
        provider::RemoteProvider::Github => {
            provider::Github::new(filter, token, config.api_url.map(provider::Url::new))
                .map_err(|e| MainError {
                    exit_code: None,
                    message: format!("Error: {e}"),
                })?
                .get_repos(
                    config.worktree.unwrap_or(false).into(),
                    config.force_ssh.unwrap_or(false).into(),
                    config.remote_name.map(RemoteName::new),
                )
                .map_err(|e| MainError {
                    exit_code: None,
                    message: format!("Error: {e}"),
                })?
        }
        provider::RemoteProvider::Gitlab => {
            provider::Gitlab::new(filter, token, config.api_url.map(provider::Url::new))
                .map_err(|e| MainError {
                    exit_code: None,
                    message: format!("Error: {e}"),
                })?
                .get_repos(
                    config.worktree.unwrap_or(false).into(),
                    config.force_ssh.unwrap_or(false).into(),
                    config.remote_name.map(RemoteName::new),
                )
                .map_err(|e| MainError {
                    exit_code: None,
                    message: format!("Error: {e}"),
                })?
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
            let toml = config.as_toml().map_err(|e| MainError {
                exit_code: None,
                message: format!("Failed converting config to TOML: {e}"),
            })?;
            print(&toml);
        }
        cmd::ConfigFormat::Yaml => {
            let yaml = config.as_yaml().map_err(|e| MainError {
                exit_code: None,
                message: format!("Failed converting config to YAML: {e}"),
            })?;
            print(&yaml);
        }
    }
    Ok(())
}

fn handle_repos_find_remote(args: cmd::FindRemoteArgs) -> Result<(), MainError> {
    let token = auth::get_token_from_command(&args.token_command).map_err(|e| MainError {
        exit_code: None,
        message: format!("Getting token from command failed: {e}"),
    })?;

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
        print_warning("You did not specify any filters, so no repos will match");
    }

    let worktree = (args.worktree == "true").into();

    let repos = match args.provider {
        cmd::RemoteProvider::Github => {
            provider::Github::new(filter, token, args.api_url.map(provider::Url::new))
                .map_err(|e| MainError {
                    exit_code: None,
                    message: format!("Error: {e}"),
                })?
                .get_repos(
                    worktree,
                    args.force_ssh,
                    args.remote_name.map(RemoteName::new),
                )
        }
        cmd::RemoteProvider::Gitlab => {
            provider::Gitlab::new(filter, token, args.api_url.map(provider::Url::new))
                .map_err(|e| MainError {
                    exit_code: None,
                    message: format!("Error: {e}"),
                })?
                .get_repos(
                    worktree,
                    args.force_ssh,
                    args.remote_name.map(RemoteName::new),
                )
        }
    }
    .map_err(|e| MainError {
        exit_code: None,
        message: format!("Error: {e}"),
    })?;

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
        return Err(MainError {
            exit_code: None,
            message: format!("Path error: {error}"),
        });
    }

    match args.format {
        cmd::ConfigFormat::Toml => {
            let toml = config.as_toml().map_err(|e| MainError {
                exit_code: None,
                message: format!("Failed converting config to TOML: {e}"),
            })?;
            print(&toml);
        }
        cmd::ConfigFormat::Yaml => {
            let yaml = config.as_yaml().map_err(|e| MainError {
                exit_code: None,
                message: format!("Failed converting config to YAML: {e}"),
            })?;
            print(&yaml);
        }
    }
    Ok(())
}

fn handle_repos_find(find: cmd::FindAction) -> Result<(), MainError> {
    match find {
        cmd::FindAction::Local(args) => handle_repos_find_local(args)?,
        cmd::FindAction::Config(args) => handle_repos_find_config(args)?,
        cmd::FindAction::Remote(args) => handle_repos_find_remote(args)?,
    }
    Ok(())
}

fn handle_repos(repos: cmd::Repos) -> Result<(), MainError> {
    match repos.action {
        cmd::ReposAction::Sync(sync) => handle_repos_sync(sync)?,
        cmd::ReposAction::Status(args) => handle_repos_status(args)?,
        cmd::ReposAction::Find(find) => handle_repos_find(find)?,
    }
    Ok(())
}

fn handle_worktree_add(args: cmd::WorktreeAddArgs) -> Result<(), MainError> {
    let cwd = get_cwd()?;

    if args.track.is_some() && args.no_track {
        print_warning(
            "You are using --track and --no-track at the same time. --track will be ignored",
        );
    }
    let track = args.track.map(|branch| {
        let split = branch.split_once('/');

        let (remote_name, remote_branch_name) = match split {
            None => {
                return Err(MainError {
                    exit_code: None,
                    message: "Tracking branch needs to match the pattern <remote>/<branch_name>, no slash found".to_owned()
                });
            }
            Some(s) if s.0.is_empty() || s.1.is_empty() => {
                return Err(MainError {
                    exit_code: None,
                    message:
                        "Tracking branch needs to match the pattern <remote>/<branch_name>"
                            .to_owned(),
                });
            }
            Some((remote_name, remote_branch_name)) => (remote_name, remote_branch_name),
        };

        Ok((
            RemoteName::new(remote_name.to_owned()),
            BranchName::new(remote_branch_name.to_owned()),
        ))
    }).transpose()?;

    let warnings = worktree::add_worktree(
        &cwd,
        &WorktreeName::new(args.name.clone()),
        track,
        args.no_track,
    )
    .map_err(|e| MainError {
        exit_code: None,
        message: format!("Error creating worktree: {e}"),
    })?;

    if let Some(warnings) = warnings {
        for warning in warnings {
            print_warning(&warning);
        }
    }
    print_success(&format!("Worktree {} created", &args.name));
    Ok(())
}

fn handle_worktree_delete(args: cmd::WorktreeDeleteArgs) -> Result<(), MainError> {
    let cwd = get_cwd()?;

    let worktree_config: Option<repo::WorktreeRootConfig> = config::read_worktree_root_config(&cwd)
        .map_err(|e| MainError {
            exit_code: None,
            message: format!("Error getting worktree configuration: {e}"),
        })?
        .map(Into::into);

    let repo = repo::RepoHandle::open(&cwd, WorktreeSetup::Worktree).map_err(|e| MainError {
        exit_code: None,
        message: format!("Error opening repository: {e}"),
    })?;

    repo.remove_worktree(
        &cwd,
        &WorktreeName::new(args.name.clone()),
        Path::new(&args.name),
        args.force,
        worktree_config.as_ref(),
    )
    .map_err(|e| match e {
        repo::Error::WorktreeRemovalFailure(reason) => match reason {
            repo::WorktreeRemoveFailureReason::Error(message)
            | repo::WorktreeRemoveFailureReason::NotMerged(message) => MainError {
                exit_code: None,
                message,
            },
            repo::WorktreeRemoveFailureReason::Changes(changes) => MainError {
                exit_code: None,
                message: format!("Changes in worktree: {changes}. Refusing to delete"),
            },
        },
        e => MainError {
            exit_code: None,
            message: e.to_string(),
        },
    })?;

    print_success(&format!("Worktree {} deleted", &args.name));

    Ok(())
}

fn handle_worktree_status(_args: cmd::WorktreeStatusArgs) -> Result<(), MainError> {
    let cwd = get_cwd()?;

    let repo = repo::RepoHandle::open(&cwd, WorktreeSetup::Worktree).map_err(|e| MainError {
        exit_code: None,
        message: format!("Error opening repository: {e}"),
    })?;

    let (table, errors) = table::get_worktree_status_table(&repo, &cwd).map_err(|e| MainError {
        exit_code: None,
        message: format!("Error getting status: {e}"),
    })?;

    println(&format!("{table}"));
    for error in errors {
        print_error(&format!("Error: {error}"));
    }

    Ok(())
}

fn handle_worktree_convert(_args: cmd::WorktreeConvertArgs) -> Result<(), MainError> {
    // Converting works like this:
    // * Check whether there are uncommitted/unpushed changes
    // * Move the contents of .git dir to the worktree directory
    // * Remove all files
    // * Set `core.bare` to `true`

    let cwd = get_cwd()?;

    let repo = repo::RepoHandle::open(&cwd, WorktreeSetup::NoWorktree).map_err(|e| MainError {
        exit_code: None,
        message: if matches!(e, repo::Error::NotFound) {
            "Directory does not contain a git repository".to_owned()
        } else {
            format!("Opening repository failed: {e}")
        },
    })?;

    if let Err(e) = repo.convert_to_worktree(&cwd) {
        Err(MainError {
            exit_code: None,
            message: match e{
                repo::Error::WorktreeConversionFailure(reason) => match reason {
                    repo::WorktreeConversionFailureReason::Changes => {
                        "Changes found in repository, refusing to convert".to_owned()
                    }
                    repo::WorktreeConversionFailureReason::Ignored => {
                            "Ignored files found in repository, refusing to convert. Run git clean -f -d -X to remove them manually.".to_owned()
                    }
                    repo::WorktreeConversionFailureReason::Error(error) => {
                                format!("Error during conversion: {error}")
                            }
                        },
                        e => e.to_string(),
            }
        })
    } else {
        print_success("Conversion done");
        Ok(())
    }
}

fn handle_worktree_clean(_args: cmd::WorktreeCleanArgs) -> Result<(), MainError> {
    let cwd = get_cwd()?;

    let repo = repo::RepoHandle::open(&cwd, WorktreeSetup::Worktree).map_err(|e| MainError {
        exit_code: None,
        message: if matches!(e, repo::Error::NotFound) {
            "Directory does not contain a git repository".to_owned()
        } else {
            format!("Opening repository failed: {e}")
        },
    })?;

    let warnings = repo.cleanup_worktrees(&cwd).map_err(|e| MainError {
        exit_code: None,
        message: format!("Worktree cleanup failed: {e}"),
    })?;

    for warning in warnings {
        print_warning(&warning);
    }

    for unmanaged_worktree in repo.find_unmanaged_worktrees(&cwd).map_err(|e| MainError {
        exit_code: None,
        message: format!("Failed finding unmanaged worktrees: {e}"),
    })? {
        print_warning(format!(
            "Found {}, which is not a valid worktree directory!",
            unmanaged_worktree.display()
        ));
    }
    Ok(())
}

fn handle_worktree_fetch(_args: cmd::WorktreeFetchArgs) -> Result<(), MainError> {
    let cwd = get_cwd()?;

    let repo = repo::RepoHandle::open(&cwd, WorktreeSetup::Worktree).map_err(|e| MainError {
        exit_code: None,
        message: if matches!(e, repo::Error::NotFound) {
            "Directory does not contain a git repository".to_owned()
        } else {
            format!("Opening repository failed: {e}")
        },
    })?;

    if let Err(e) = repo.fetchall() {
        Err(MainError {
            exit_code: None,
            message: format!("Error fetching remotes: {e}"),
        })
    } else {
        print_success("Fetched from all remotes");
        Ok(())
    }
}

fn handle_worktree_pull(args: cmd::WorktreePullArgs) -> Result<(), MainError> {
    let cwd = get_cwd()?;

    let repo = repo::RepoHandle::open(&cwd, WorktreeSetup::Worktree).map_err(|e| MainError {
        exit_code: None,
        message: if matches!(e, repo::Error::NotFound) {
            "Directory does not contain a git repository".to_owned()
        } else {
            format!("Opening repository failed: {e}")
        },
    })?;

    if let Err(e) = repo.fetchall() {
        return Err(MainError {
            exit_code: None,
            message: format!("Error fetching remotes: {e}"),
        });
    }

    let mut failures = false;
    for worktree in repo.get_worktrees().map_err(|e| MainError {
        exit_code: None,
        message: format!("Error getting worktrees: {e}"),
    })? {
        if let Some(warning) = worktree
            .forward_branch(args.rebase, args.stash)
            .map_err(|e| MainError {
                exit_code: None,
                message: format!("Error updating worktree branch: {e}"),
            })?
        {
            print_warning(format!("{}: {}", worktree.name(), warning));
            failures = true;
        } else {
            print_success(&format!("{}: Done", worktree.name()));
        }
    }
    if failures {
        Err(MainError {
            exit_code: None,
            message: "Pull failed".to_owned(),
        })
    } else {
        Ok(())
    }
}

fn handle_worktree_rebase(args: cmd::WorktreeRebaseArgs) -> Result<(), MainError> {
    let cwd = get_cwd()?;

    if args.rebase && !args.pull {
        return Err(MainError {
            exit_code: None,
            message: "There is no point in using --rebase without --pull".to_owned(),
        });
    }

    let repo = repo::RepoHandle::open(&cwd, WorktreeSetup::Worktree).map_err(|e| MainError {
        exit_code: None,
        message: if matches!(e, repo::Error::NotFound) {
            "Directory does not contain a git repository".to_owned()
        } else {
            format!("Opening repository failed: {e}")
        },
    })?;

    if args.pull {
        if let Err(e) = repo.fetchall() {
            return Err(MainError {
                exit_code: None,
                message: format!("Error fetching remotes: {e}"),
            });
        }
    }

    let config = config::read_worktree_root_config(&cwd)
        .map_err(|error| MainError {
            exit_code: None,
            message: format!("Failed to read worktree configuration: {error}"),
        })?
        .map(Into::into);

    let worktrees = repo.get_worktrees().map_err(|error| MainError {
        exit_code: None,
        message: format!("Error getting worktrees: {error}"),
    })?;

    let mut failures = false;

    for worktree in &worktrees {
        if args.pull {
            if let Some(warning) =
                worktree
                    .forward_branch(args.rebase, args.stash)
                    .map_err(|error| MainError {
                        exit_code: None,
                        message: format!("Error updating worktree branch: {error}"),
                    })?
            {
                failures = true;
                print_warning(format!("{}: {}", worktree.name(), warning));
            }
        }
    }

    for worktree in &worktrees {
        if let Some(warning) =
            worktree
                .rebase_onto_default(&config, args.stash)
                .map_err(|error| MainError {
                    exit_code: None,
                    message: format!("Error rebasing worktree branch: {error}"),
                })?
        {
            failures = true;
            print_warning(format!("{}: {}", worktree.name(), warning));
        } else {
            print_success(&format!("{}: Done", worktree.name()));
        }
    }

    if failures {
        Err(MainError {
            exit_code: None,
            message: "Rebase failed".to_owned(),
        })
    } else {
        Ok(())
    }
}

fn get_cwd() -> Result<PathBuf, MainError> {
    std::env::current_dir().map_err(|e| MainError {
        message: format!("Could not open current directory: {e}"),
        exit_code: None,
    })
}

fn handle_worktree(worktree: cmd::Worktree) -> Result<(), MainError> {
    match worktree.action {
        cmd::WorktreeAction::Add(args) => handle_worktree_add(args)?,
        cmd::WorktreeAction::Delete(args) => handle_worktree_delete(args)?,
        cmd::WorktreeAction::Status(args) => handle_worktree_status(args)?,
        cmd::WorktreeAction::Convert(args) => handle_worktree_convert(args)?,
        cmd::WorktreeAction::Clean(args) => handle_worktree_clean(args)?,
        cmd::WorktreeAction::Fetch(args) => handle_worktree_fetch(args)?,
        cmd::WorktreeAction::Pull(args) => handle_worktree_pull(args)?,
        cmd::WorktreeAction::Rebase(args) => handle_worktree_rebase(args)?,
    }
    Ok(())
}

fn main_inner() -> Result<(), MainError> {
    let opts = cmd::parse();

    match opts.subcmd {
        cmd::SubCommand::Repos(repos) => handle_repos(repos)?,
        cmd::SubCommand::Worktree(args) => handle_worktree(args)?,
    }

    Ok(())
}
