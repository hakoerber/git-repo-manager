#![forbid(unsafe_code)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "cmd args are passed by value to make the call hierarchy more obvious"
)]

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::{ExitCode, Termination},
};

use thiserror::Error;

mod cmd;

use grm::{
    BranchName, RemoteName,
    auth::{self, AuthToken},
    config, find_in_tree,
    output::{print, print_error, print_success, print_warning, println},
    provider::{self, Filter, ProjectNamespace, ProtocolConfig, Provider, RemoteProvider},
    repo::{
        self, RepoChanges,
        worktree::{self, WorktreeName, WorktreeSetup},
    },
    table, tree,
};

#[derive(Debug, Error)]
enum MainError {
    #[error("Failed converting config to TOML: {0}")]
    TomlConversion(config::SerializationError),
    #[error("Failed converting config to YAML: {0}")]
    YamlConversion(config::SerializationError),
    #[error("Getting token from command failed: {0}")]
    TokenCommandFailed(auth::Error),
    #[error("Failed provider initialization: {0}")]
    ProviderInit(provider::Error),
    #[error("Failed getting repositories from provider: {0}")]
    ProviderGetRepo(provider::Error),
    #[error("Failed normalizing config: {0}")]
    ConfigNormalization(config::Error),
    #[error("Failed syncing repositories: {0}")]
    SyncTrees(tree::Error),
    #[error("There were failures during repository sync")]
    SyncTreeHasFailures,
    #[error("Failed reading configuration: {0}")]
    ReadConfig(config::ReadConfigError),
    #[error("Failed reading worktree configuration: {0}")]
    ReadWorktreeConfig(config::Error),
    #[error("Failed generating repo status: {0}")]
    RepoStatus(table::Error),
    #[error("Failed getting current directory: {0}")]
    CurrentDirectory(std::io::Error),
    #[error("Path \"{0}\" does not exist")]
    PathDoesNotExist(PathBuf),
    #[error("Path \"{0}\" is not a directory")]
    PathNotADirectory(PathBuf),
    #[error("Failed to canonicalize path \"{0}\". This is a bug. Error message: {1}")]
    PathCanoncailization(PathBuf, std::io::Error),
    #[error("Failed parsing regex \"{0}\": {1}")]
    ExclusionRegex(String, regex::Error),
    #[error("Failed finding repositories: {0}")]
    FindInTree(grm::Error),
    #[error(
        "Tracking branch needs to match the pattern <remote>/<branch_name>, \
        no slash found"
    )]
    NoSlashInTrackingBranch,
    #[error("Tracking branch needs to match the pattern <remote>/<branch_name>")]
    TrackingBranchWrongFormat,
    #[error("Failed creating worktree: {0}")]
    CreateWorktree(worktree::Error),
    #[error("Failed getting worktree configuration: {0}")]
    WorktreeConfiguration(config::Error),
    #[error("Failed opening repository: {0}")]
    OpenRepo(repo::Error),
    #[error("Failed deleting worktree: {0}")]
    WorktreeRemove(worktree::WorktreeRemoveError),
    #[error("{0}, refusing to delete")]
    WorktreeRemovalRefuse(worktree::WorktreeRemoveError),
    #[error("Directory \"{0}\" does not contain a git repository")]
    DirectoryDoesNotContainRepo(PathBuf),
    #[error("Repo contains changes ({0}), refusing to convert")]
    WorktreeConvertRefuseChanges(RepoChanges),
    #[error(
        "Ignored files found in repository, refusing to convert. \
        Run git clean -f -d -X to remove them manually."
    )]
    WorktreeConvertRefuseIgnored,
    #[error("Failed converting repo to worktree: {0}")]
    WorktreeConvert(worktree::WorktreeConversionError),
    #[error("Failed cleaning worktrees: {0}")]
    WorktreeCleanup(repo::CleanupWorktreeError),
    #[error("Failed finding unmanaged worktrees: {0}")]
    FindUnmanagedWorktrees(repo::Error),
    #[error("Failed fetching remotes: {0}")]
    FetchRemotes(repo::Error),
    #[error("Failed getting worktrees: {0}")]
    GetWorktrees(repo::Error),
    #[error("Failed forwarding worktree branch: {0}")]
    ForwardWorktreeBranch(repo::Error),
    #[error("Failed rebasing worktree branch: {0}")]
    RebaseWorktreeBranch(repo::Error),
    #[error("There is no point in using --rebase without --pull")]
    CmdRebaseWithoutPull,
}

impl MainError {
    fn exit_code(&self) -> ExitCode {
        match *self {
            Self::WorktreeConvertRefuseChanges(..)
            | Self::WorktreeConvertRefuseIgnored
            | Self::WorktreeRemovalRefuse(..) => MainExitCode::Refusal.into(),

            Self::CmdRebaseWithoutPull => MainExitCode::Cli.into(),

            _ => MainExitCode::Failure.into(),
        }
    }
}

enum MainExitCode {
    Success,
    Failure,
    Warnings,
    Refusal,
    Cli,
}

impl From<MainExitCode> for ExitCode {
    fn from(value: MainExitCode) -> Self {
        match value {
            MainExitCode::Success => Self::SUCCESS,
            MainExitCode::Failure => Self::FAILURE,
            MainExitCode::Warnings => Self::from(2),
            MainExitCode::Refusal => Self::from(3),
            MainExitCode::Cli => Self::from(4),
        }
    }
}

enum MainResult {
    Success(MainExitCode),
    Failure(MainError),
}

impl Termination for MainResult {
    fn report(self) -> ExitCode {
        match self {
            Self::Success(exit_code) => exit_code.into(),
            Self::Failure(main_error) => {
                print_error(&main_error.to_string());
                main_error.exit_code()
            }
        }
    }
}

fn main() -> MainResult {
    match main_inner() {
        Ok(exit_code) => MainResult::Success(exit_code),
        Err(e) => MainResult::Failure(e),
    }
}

macro_rules! read_config_fn {
    ($([$id:ident, $config_type:ty]),+$(,)?) => {
        $(
            fn $id(path: &str) -> Result<$config_type, MainError> {
                config::read_config(Path::new(&path))
                    .map_err(|e| MainError::ReadConfig(e))
            }
        )+
    };
}

read_config_fn!(
    [read_config, config::Config],
    [read_provider_config, config::ConfigProvider],
);

fn handle_repos_sync_config(args: cmd::Config) -> HandlerResult {
    tree::sync_trees(read_config(&args.config)?, args.init_worktree)
        .map_err(|e| MainError::SyncTrees(e))?
        .is_success()
        .then_some(MainExitCode::Success)
        .ok_or(MainError::SyncTreeHasFailures)
}

fn get_repos_from_provider(
    provider: RemoteProvider,
    filter: Filter,
    token: AuthToken,
    api_url: Option<String>,
    use_worktree: bool,
    force_ssh: bool,
    remote_name: Option<String>,
) -> Result<HashMap<Option<ProjectNamespace>, Vec<repo::Repo>>, MainError> {
    match provider {
        cmd::RemoteProvider::Github => {
            provider::Github::new(filter, token, api_url.map(provider::Url::new))
                .map_err(|e| MainError::ProviderInit(e))?
                .get_repos(
                    use_worktree.into(),
                    if force_ssh {
                        ProtocolConfig::ForceSsh
                    } else {
                        ProtocolConfig::Default
                    },
                    remote_name.map(RemoteName::new),
                )
        }
        cmd::RemoteProvider::Gitlab => {
            provider::Gitlab::new(filter, token, api_url.map(provider::Url::new))
                .map_err(|e| MainError::ProviderInit(e))?
                .get_repos(
                    use_worktree.into(),
                    if force_ssh {
                        ProtocolConfig::ForceSsh
                    } else {
                        ProtocolConfig::Default
                    },
                    remote_name.map(RemoteName::new),
                )
        }
    }
    .map_err(|e| MainError::ProviderGetRepo(e))
}

fn handle_repos_sync_remote(args: cmd::SyncRemoteArgs) -> HandlerResult {
    let token = auth::get_token_from_command(&args.token_command)
        .map_err(|e| MainError::TokenCommandFailed(e))?;

    let filter = provider::Filter::new(
        args.users.into_iter().map(provider::User::new).collect(),
        args.groups.into_iter().map(provider::Group::new).collect(),
        args.owner,
        args.access,
    );

    if filter.empty() {
        print_warning("You did not specify any filters, so no repos will match");
    }

    let repos = get_repos_from_provider(
        args.provider,
        filter,
        token,
        args.api_url,
        args.worktree,
        args.force_ssh,
        args.remote_name,
    )?;

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

    tree::sync_trees(config, args.init_worktree)
        .map_err(|e| MainError::SyncTrees(e))?
        .is_success()
        .then_some(MainExitCode::Success)
        .ok_or(MainError::SyncTreeHasFailures)
}

fn handle_repos_sync(sync: cmd::SyncAction) -> HandlerResult {
    Ok(match sync {
        cmd::SyncAction::Config(args) => handle_repos_sync_config(args)?,
        cmd::SyncAction::Remote(args) => handle_repos_sync_remote(args)?,
    })
}

fn handle_repos_status(args: cmd::OptionalConfig) -> HandlerResult {
    if let Some(config_path) = args.config {
        let (tables, errors) = table::get_status_table(read_config(&config_path)?)
            .map_err(|e| MainError::RepoStatus(e))?;

        for table in tables {
            println(&format!("{table}"));
        }
        for error in errors {
            print_error(&format!("Error: {error}"));
        }
    } else {
        let dir = std::env::current_dir().map_err(|e| MainError::CurrentDirectory(e))?;

        let (table, warnings) =
            table::show_single_repo_status(&dir).map_err(|e| MainError::RepoStatus(e))?;

        println(&format!("{table}"));
        for warning in warnings {
            print_warning(&warning);
        }
    }
    Ok(MainExitCode::Success)
}

fn handle_repos_find_local(args: cmd::FindLocalArgs) -> HandlerResult {
    let path = Path::new(&args.path);
    if !path.exists() {
        return Err(MainError::PathDoesNotExist(path.to_path_buf()));
    }
    if !path.is_dir() {
        return Err(MainError::PathNotADirectory(path.to_path_buf()));
    }

    let path = path
        .canonicalize()
        .map_err(|e| MainError::PathCanoncailization(path.to_path_buf(), e))?;

    let exclusion_pattern = args
        .exclude
        .map(|s| regex::Regex::new(&s).map_err(|e| MainError::ExclusionRegex(s, e)))
        .transpose()?;

    let (found_repos, warnings) =
        find_in_tree(&path, exclusion_pattern.as_ref()).map_err(|e| MainError::FindInTree(e))?;

    let trees = config::ConfigTrees::from_trees(vec![found_repos]);
    if trees.trees_ref().iter().all(|t| match t.repos {
        None => false,
        Some(ref r) => r.is_empty(),
    }) {
        print_warning("No repositories found");
    } else {
        let mut config = trees.to_config();

        if let Err(e) = config.normalize() {
            return Err(MainError::ConfigNormalization(e));
        }

        print(&config_to_string(config, args.format)?);
    }
    for warning in warnings {
        print_warning(&warning);
    }
    Ok(MainExitCode::Success)
}

fn handle_repos_find_config(args: cmd::FindConfigArgs) -> HandlerResult {
    let config = read_provider_config(&args.config)?;

    let token = auth::get_token_from_command(&config.token_command)
        .map_err(|e| MainError::TokenCommandFailed(e))?;

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

    let repos = get_repos_from_provider(
        config.provider.into(),
        filter,
        token,
        config.api_url,
        config.worktree.unwrap_or(false),
        config.force_ssh.unwrap_or(false),
        config.remote_name,
    )?;

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

    print(&config_to_string(config, args.format)?);
    Ok(MainExitCode::Success)
}

fn config_to_string(
    config: config::Config,
    format: cmd::ConfigFormat,
) -> Result<String, MainError> {
    Ok(match format {
        cmd::ConfigFormat::Toml => config.as_toml().map_err(|e| MainError::TomlConversion(e))?,
        cmd::ConfigFormat::Yaml => config.as_yaml().map_err(|e| MainError::YamlConversion(e))?,
    })
}

fn handle_repos_find_remote(args: cmd::FindRemoteArgs) -> HandlerResult {
    let token = auth::get_token_from_command(&args.token_command)
        .map_err(|e| MainError::TokenCommandFailed(e))?;

    let filter = provider::Filter::new(
        args.users.into_iter().map(provider::User::new).collect(),
        args.groups.into_iter().map(provider::Group::new).collect(),
        args.owner,
        args.access,
    );

    if filter.empty() {
        print_warning("You did not specify any filters, so no repos will match");
    }

    let repos = get_repos_from_provider(
        args.provider,
        filter,
        token,
        args.api_url,
        args.worktree,
        args.force_ssh,
        args.remote_name,
    )?;

    let trees = {
        let mut trees = vec![];

        #[expect(clippy::iter_over_hash_type, reason = "fine in this case")]
        for (namespace, repolist) in repos {
            trees.push(config::Tree {
                root: tree::Root::new(if let Some(namespace) = namespace {
                    PathBuf::from(&args.root).join(namespace.as_str())
                } else {
                    PathBuf::from(&args.root)
                })
                .into(),
                repos: Some(repolist.into_iter().map(Into::into).collect()),
            });
        }
        trees
    };

    let mut config = config::Config::from_trees(trees);

    if let Err(e) = config.normalize() {
        return Err(MainError::ConfigNormalization(e));
    }

    print(&config_to_string(config, args.format)?);
    Ok(MainExitCode::Success)
}

type HandlerResult = Result<MainExitCode, MainError>;

fn handle_repos_find(find: cmd::FindAction) -> HandlerResult {
    Ok(match find {
        cmd::FindAction::Local(args) => handle_repos_find_local(args)?,
        cmd::FindAction::Config(args) => handle_repos_find_config(args)?,
        cmd::FindAction::Remote(args) => handle_repos_find_remote(args)?,
    })
}

fn handle_repos(repos: cmd::Repos) -> HandlerResult {
    Ok(match repos.action {
        cmd::ReposAction::Sync(sync) => handle_repos_sync(sync)?,
        cmd::ReposAction::Status(args) => handle_repos_status(args)?,
        cmd::ReposAction::Find(find) => handle_repos_find(find)?,
    })
}

fn handle_worktree_add(args: cmd::WorktreeAddArgs) -> HandlerResult {
    let cwd = get_cwd()?;

    if args.track.is_some() && args.no_track {
        print_warning(
            "You are using --track and --no-track at the same time. --track will be ignored",
        );
    }
    let track = args
        .track
        .map(|branch| {
            let split = branch.split_once('/');

            let (remote_name, remote_branch_name) = match split {
                None => {
                    return Err(MainError::NoSlashInTrackingBranch);
                }
                Some(s) if s.0.is_empty() || s.1.is_empty() => {
                    return Err(MainError::TrackingBranchWrongFormat);
                }
                Some((remote_name, remote_branch_name)) => (remote_name, remote_branch_name),
            };

            Ok((
                RemoteName::new(remote_name.to_owned()),
                BranchName::new(remote_branch_name.to_owned()),
            ))
        })
        .transpose()?;

    let warnings = worktree::add_worktree(
        &cwd,
        &WorktreeName::new(args.name.clone()),
        track,
        args.no_track,
    )
    .map_err(|e| MainError::CreateWorktree(e))?;

    if let Some(warnings) = warnings {
        for warning in warnings {
            print_warning(&warning);
        }
    }

    print_success(&format!("Worktree {} created", &args.name));

    Ok(MainExitCode::Success)
}

fn handle_worktree_delete(args: cmd::WorktreeDeleteArgs) -> HandlerResult {
    let cwd = get_cwd()?;

    let worktree_config: Option<repo::WorktreeRootConfig> = config::read_worktree_root_config(&cwd)
        .map_err(MainError::WorktreeConfiguration)?
        .map(Into::into);

    repo::RepoHandle::open(&cwd, WorktreeSetup::Worktree)
        .map_err(|e| MainError::OpenRepo(e))?
        .remove_worktree(
            &cwd,
            &WorktreeName::new(args.name.clone()),
            Path::new(&args.name),
            args.force,
            worktree_config.as_ref(),
        )
        .map_err(|e| match e {
            repo::worktree::WorktreeRemoveError::Changes(_)
            | repo::worktree::WorktreeRemoveError::NoRemoteTrackingBranch { .. }
            | repo::worktree::WorktreeRemoveError::NotInSyncWithRemote { .. }
            | repo::worktree::WorktreeRemoveError::NotMerged { .. } => {
                MainError::WorktreeRemovalRefuse(e)
            }
            _ => MainError::WorktreeRemove(e),
        })?;

    print_success(&format!("Worktree {} deleted", &args.name));

    Ok(MainExitCode::Success)
}

fn handle_worktree_status(_args: cmd::WorktreeStatusArgs) -> HandlerResult {
    let cwd = get_cwd()?;

    let repo = repo::RepoHandle::open(&cwd, WorktreeSetup::Worktree)
        .map_err(|e| MainError::OpenRepo(e))?;

    let (table, errors) =
        table::get_worktree_status_table(&repo, &cwd).map_err(|e| MainError::RepoStatus(e))?;

    println(&format!("{table}"));
    for error in errors {
        print_error(&format!("Error: {error}"));
    }

    Ok(MainExitCode::Success)
}

fn handle_worktree_convert(_args: cmd::WorktreeConvertArgs) -> HandlerResult {
    let cwd = get_cwd()?;

    repo::RepoHandle::open(&cwd, WorktreeSetup::NoWorktree)
        .map_err(|e| {
            if matches!(e, repo::Error::NotFound) {
                MainError::DirectoryDoesNotContainRepo(cwd.clone())
            } else {
                MainError::OpenRepo(e)
            }
        })?
        .convert_to_worktree(&cwd)
        .map_err(|e| match e {
            repo::worktree::WorktreeConversionError::Changes(changes) => {
                MainError::WorktreeConvertRefuseChanges(changes)
            }
            repo::worktree::WorktreeConversionError::Ignored => {
                MainError::WorktreeConvertRefuseIgnored
            }
            _ => MainError::WorktreeConvert(e),
        })?;

    print_success("Conversion done");
    Ok(MainExitCode::Success)
}

fn handle_worktree_clean(_args: cmd::WorktreeCleanArgs) -> HandlerResult {
    let cwd = get_cwd()?;

    let repo = repo::RepoHandle::open(&cwd, WorktreeSetup::Worktree).map_err(|e| {
        if matches!(e, repo::Error::NotFound) {
            MainError::DirectoryDoesNotContainRepo(cwd.clone())
        } else {
            MainError::OpenRepo(e)
        }
    })?;

    let warnings = repo
        .cleanup_worktrees(&cwd)
        .map_err(|e| MainError::WorktreeCleanup(e))?;

    for warning in &warnings {
        print_warning(format!(
            "Skipping worktree {}: {}",
            warning.worktree_name,
            match warning.reason {
                repo::CleanupWorktreeWarningReason::Changes(ref changes) => {
                    format!("changes found ({changes})")
                }
                repo::CleanupWorktreeWarningReason::NotMerged { ref branch_name } => {
                    format!("branch {branch_name} is not merged")
                }
                repo::CleanupWorktreeWarningReason::NoRemoteTrackingBranch { ref branch_name } => {
                    format!("branch {branch_name} does not have a remote tracking branch")
                }
                repo::CleanupWorktreeWarningReason::NoDirectory => {
                    "worktree does not have a directory".to_owned()
                }
            }
        ));
    }

    for unmanaged_worktree in repo
        .find_unmanaged_worktrees(&cwd)
        .map_err(|e| MainError::FindUnmanagedWorktrees(e))?
    {
        print_warning(format!(
            "Found {}, which is not a valid worktree directory!",
            unmanaged_worktree.display()
        ));
    }

    Ok(if warnings.is_empty() {
        MainExitCode::Success
    } else {
        MainExitCode::Warnings
    })
}

fn handle_worktree_fetch(_args: cmd::WorktreeFetchArgs) -> HandlerResult {
    let cwd = get_cwd()?;

    repo::RepoHandle::open(&cwd, WorktreeSetup::Worktree)
        .map_err(|e| {
            if matches!(e, repo::Error::NotFound) {
                MainError::DirectoryDoesNotContainRepo(cwd.clone())
            } else {
                MainError::OpenRepo(e)
            }
        })?
        .fetchall()
        .map_err(MainError::FetchRemotes)?;

    print_success("Fetched from all remotes");
    Ok(MainExitCode::Success)
}

fn handle_worktree_pull(args: cmd::WorktreePullArgs) -> HandlerResult {
    let cwd = get_cwd()?;

    let repo = repo::RepoHandle::open(&cwd, WorktreeSetup::Worktree).map_err(|e| {
        if matches!(e, repo::Error::NotFound) {
            MainError::DirectoryDoesNotContainRepo(cwd.clone())
        } else {
            MainError::OpenRepo(e)
        }
    })?;

    if let Err(e) = repo.fetchall() {
        return Err(MainError::FetchRemotes(e));
    }

    let mut failures = false;
    for worktree in repo
        .get_worktrees()
        .map_err(|e| MainError::GetWorktrees(e))?
    {
        if let Some(warning) = worktree
            .forward_branch(args.rebase, args.stash)
            .map_err(|e| MainError::ForwardWorktreeBranch(e))?
        {
            print_warning(format!("{}: {}", worktree.name(), warning));
            failures = true;
        } else {
            print_success(&format!("{}: Done", worktree.name()));
        }
    }

    if failures {
        Ok(MainExitCode::Warnings)
    } else {
        Ok(MainExitCode::Success)
    }
}

fn handle_worktree_rebase(args: cmd::WorktreeRebaseArgs) -> HandlerResult {
    let cwd = get_cwd()?;

    if args.rebase && !args.pull {
        return Err(MainError::CmdRebaseWithoutPull);
    }

    let repo = repo::RepoHandle::open(&cwd, WorktreeSetup::Worktree).map_err(|e| {
        if matches!(e, repo::Error::NotFound) {
            MainError::DirectoryDoesNotContainRepo(cwd.clone())
        } else {
            MainError::OpenRepo(e)
        }
    })?;

    if args.pull {
        if let Err(e) = repo.fetchall() {
            return Err(MainError::FetchRemotes(e));
        }
    }

    let config = config::read_worktree_root_config(&cwd)
        .map_err(|e| MainError::ReadWorktreeConfig(e))?
        .map(Into::into);

    let worktrees = repo
        .get_worktrees()
        .map_err(|e| MainError::GetWorktrees(e))?;

    let mut failures = false;

    for worktree in &worktrees {
        if args.pull {
            if let Some(warning) = worktree
                .forward_branch(args.rebase, args.stash)
                .map_err(|e| MainError::ForwardWorktreeBranch(e))?
            {
                failures = true;
                print_warning(format!("{}: {}", worktree.name(), warning));
            }
        }
    }

    for worktree in &worktrees {
        if let Some(warning) = worktree
            .rebase_onto_default(&config, args.stash)
            .map_err(|e| MainError::RebaseWorktreeBranch(e))?
        {
            failures = true;
            print_warning(format!("{}: {}", worktree.name(), warning));
        } else {
            print_success(&format!("{}: Done", worktree.name()));
        }
    }

    if failures {
        Ok(MainExitCode::Warnings)
    } else {
        Ok(MainExitCode::Success)
    }
}

fn get_cwd() -> Result<PathBuf, MainError> {
    std::env::current_dir().map_err(|e| MainError::CurrentDirectory(e))
}

fn handle_worktree(worktree: cmd::Worktree) -> HandlerResult {
    Ok(match worktree.action {
        cmd::WorktreeAction::Add(args) => handle_worktree_add(args)?,
        cmd::WorktreeAction::Delete(args) => handle_worktree_delete(args)?,
        cmd::WorktreeAction::Status(args) => handle_worktree_status(args)?,
        cmd::WorktreeAction::Convert(args) => handle_worktree_convert(args)?,
        cmd::WorktreeAction::Clean(args) => handle_worktree_clean(args)?,
        cmd::WorktreeAction::Fetch(args) => handle_worktree_fetch(args)?,
        cmd::WorktreeAction::Pull(args) => handle_worktree_pull(args)?,
        cmd::WorktreeAction::Rebase(args) => handle_worktree_rebase(args)?,
    })
}

fn main_inner() -> HandlerResult {
    let opts = cmd::parse();

    Ok(match opts.subcmd {
        cmd::SubCommand::Repos(repos) => handle_repos(repos)?,
        cmd::SubCommand::Worktree(args) => handle_worktree(args)?,
    })
}
