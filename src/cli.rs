use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "apiforge",
    about = "Production API release automation CLI",
    version,
    long_about = "From merged code to healthy pods in production — one command, zero tribal knowledge required."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Enable debug output
    #[arg(long, global = true, env = "APIFORGE_DEBUG")]
    pub debug: bool,

    /// Config file path
    #[arg(long, global = true, default_value = "apiforge.toml")]
    pub config: String,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize apiforge in current project
    Init(InitArgs),

    /// Validate environment and dependencies
    Doctor,

    /// Release a new version
    Release(ReleaseArgs),

    /// Roll back to previous release
    Rollback(RollbackArgs),

    /// Show release history
    History(HistoryArgs),

    /// Show current deployment status
    Status,
}

#[derive(Parser)]
pub struct InitArgs {
    /// Project name
    #[arg(long)]
    pub name: Option<String>,

    /// Force overwrite existing config
    #[arg(long)]
    pub force: bool,
}

#[derive(Parser)]
pub struct ReleaseArgs {
    /// Version bump type
    #[arg(value_parser = ["major", "minor", "patch"])]
    pub bump: String,

    /// Preview all steps without making changes
    #[arg(long)]
    pub dry_run: bool,

    /// Skip Docker build and push
    #[arg(long)]
    pub skip_docker: bool,

    /// Skip Kubernetes rollout
    #[arg(long)]
    pub skip_k8s: bool,

    /// Skip GitHub Release creation
    #[arg(long)]
    pub skip_github: bool,

    /// Skip all notifications
    #[arg(long)]
    pub skip_notify: bool,

    /// Skip changelog generation
    #[arg(long)]
    pub no_changelog: bool,

    /// Output format
    #[arg(long, value_parser = ["text", "json"], default_value = "text")]
    pub output: String,

    /// Skip confirmation prompt
    #[arg(long, short)]
    pub yes: bool,
}

#[derive(Parser)]
pub struct RollbackArgs {
    /// Version to roll back to
    #[arg(long)]
    pub to: Option<String>,

    /// Preview rollback without making changes
    #[arg(long)]
    pub dry_run: bool,

    /// Skip notifications
    #[arg(long)]
    pub skip_notify: bool,
}

#[derive(Parser)]
pub struct HistoryArgs {
    /// Maximum number of entries to show
    #[arg(long, default_value = "20")]
    pub limit: usize,

    /// Output format
    #[arg(long, value_parser = ["text", "json"], default_value = "text")]
    pub output: String,

    /// Filter by status
    #[arg(long, value_parser = ["success", "failed"])]
    pub filter: Option<String>,
}
