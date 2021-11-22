use clap::{AppSettings, Parser};

#[derive(Parser)]
#[clap(
    name = clap::crate_name!(),
    version = clap::crate_version!(),
    author = clap::crate_authors!("\n"),
    about = clap::crate_description!(),
    long_version = clap::crate_version!(),
    license = clap::crate_license!(),
    setting = AppSettings::DeriveDisplayOrder,
    setting = AppSettings::PropagateVersion,
    setting = AppSettings::HelpRequired,
)]
pub struct Opts {
    #[clap(subcommand)]
    pub subcmd: SubCommand,
}

#[derive(Parser)]
pub enum SubCommand {
    #[clap(
        visible_alias = "run",
        about = "Synchronize the repositories to the configured values"
    )]
    Sync(Sync),
    #[clap(about = "Generate a repository configuration from an existing file tree")]
    Find(Find),
    #[clap(about = "Show status of configured repositories")]
    Status(OptionalConfig),
    #[clap(visible_alias = "wt", about = "Manage worktrees")]
    Worktree(Worktree),
}

#[derive(Parser)]
#[clap()]
pub struct Sync {
    #[clap(
        short,
        long,
        default_value = "./config.toml",
        about = "Path to the configuration file"
    )]
    pub config: String,
}

#[derive(Parser)]
#[clap()]
pub struct OptionalConfig {
    #[clap(short, long, about = "Path to the configuration file")]
    pub config: Option<String>,
}

#[derive(Parser)]
pub struct Find {
    #[clap(about = "The path to search through")]
    pub path: String,
}

#[derive(Parser)]
pub struct Worktree {
    #[clap(subcommand, name = "action")]
    pub action: WorktreeAction,
}

#[derive(Parser)]
pub enum WorktreeAction {
    #[clap(about = "Add a new worktree")]
    Add(WorktreeAddArgs),
    #[clap(about = "Add an existing worktree")]
    Delete(WorktreeDeleteArgs),
    #[clap(about = "Show state of existing worktrees")]
    Status(WorktreeStatusArgs),
    #[clap(about = "Clean all worktrees that do not contain uncommited/unpushed changes")]
    Clean(WorktreeCleanArgs),
}

#[derive(Parser)]
pub struct WorktreeAddArgs {
    #[clap(about = "Name of the worktree")]
    pub name: String,

    #[clap(
        short = 'p',
        long = "branch-prefix",
        about = "Prefix for the branch name"
    )]
    pub branch_prefix: Option<String>,
    #[clap(short = 't', long = "track", about = "Remote branch to track")]
    pub track: Option<String>,
}
#[derive(Parser)]
pub struct WorktreeDeleteArgs {
    #[clap(about = "Name of the worktree")]
    pub name: String,

    #[clap(
        long = "force",
        about = "Force deletion, even when there are uncommitted/unpushed changes"
    )]
    pub force: bool,
}

#[derive(Parser)]
pub struct WorktreeStatusArgs {}

#[derive(Parser)]
pub struct WorktreeCleanArgs {}

pub fn parse() -> Opts {
    Opts::parse()
}
