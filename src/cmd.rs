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
    #[clap(
        short,
        long,
        about = "Path to the configuration file"
    )]
    pub config: Option<String>,
}

#[derive(Parser)]
pub struct Find {
    #[clap(about = "The path to search through")]
    pub path: String,
}

pub fn parse() -> Opts {
    Opts::parse()
}
