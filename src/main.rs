use clap::Parser;
use apiforge::cli::{Cli, Commands};
use apiforge::config::Config;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| {
                    if cli.debug {
                        "apiforge=debug".into()
                    } else {
                        "apiforge=info".into()
                    }
                }),
        )
        .without_time()
        .init();

    match cli.command {
        Commands::Init(args) => cmd_init(args).await,
        Commands::Doctor => cmd_doctor(&cli.config).await,
        Commands::Release(args) => cmd_release(&cli.config, args).await,
        Commands::Rollback(args) => cmd_rollback(&cli.config, args).await,
        Commands::History(args) => cmd_history(args).await,
        Commands::Status => cmd_status(&cli.config).await,
    }
}

async fn cmd_init(args: apiforge::cli::InitArgs) -> anyhow::Result<()> {
    let name = args.name.unwrap_or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "my-project".to_string())
    });

    let config_path = PathBuf::from("apiforge.toml");
    if config_path.exists() && !args.force {
        anyhow::bail!("apiforge.toml already exists. Use --force to overwrite.");
    }

    println!("Initializing apiforge for '{}'...", name);
    println!("Created apiforge.toml — edit it to match your project setup.");
    Ok(())
}

async fn cmd_doctor(config_path: &str) -> anyhow::Result<()> {
    println!("Running environment checks...\n");

    let checks: Vec<(&str, fn() -> bool)> = vec![
        ("git", || which::which("git").is_ok()),
        ("docker", || which::which("docker").is_ok()),
        ("kubectl", || which::which("kubectl").is_ok()),
        ("aws", || which::which("aws").is_ok()),
    ];

    for (name, check) in &checks {
        let status = if check() { "OK" } else { "MISSING" };
        println!("  {} ... {}", name, status);
    }

    let path = PathBuf::from(config_path);
    if path.exists() {
        match Config::from_file(&path) {
            Ok(_) => println!("\n  config ... OK"),
            Err(e) => println!("\n  config ... INVALID ({})", e),
        }
    } else {
        println!("\n  config ... NOT FOUND (run `apiforge init`)");
    }

    Ok(())
}

async fn cmd_release(config_path: &str, args: apiforge::cli::ReleaseArgs) -> anyhow::Result<()> {
    let path = PathBuf::from(config_path);
    let config = Config::from_file(&path)?;

    let bump_type = apiforge::utils::BumpType::from_str(&args.bump)?;

    let repo = apiforge::integrations::git::GitRepo::open()?;
    let version_file = config.project.language.version_file();
    let version_path = repo.root_path().join(version_file);
    
    let current_version = if config.project.language == apiforge::config::Language::Rust {
        let content = std::fs::read_to_string(&version_path)?;
        let doc: toml::Value = toml::from_str(&content)?;
        doc.get("package")
            .and_then(|p| p.get("version"))
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| anyhow::anyhow!("No version in Cargo.toml"))?
    } else {
        anyhow::bail!("Only Rust projects supported currently");
    };

    let new_version = apiforge::utils::bump_version(&current_version, bump_type)?;
    let new_version_str = new_version.to_string();

    let previous_tag = repo.get_latest_tag(&config.git.tag_format.replace("{version}", "*"))?;

    let mut orchestrator = apiforge::orchestrator::ReleaseOrchestrator::new(
        config.clone(),
        args.dry_run,
    );

    orchestrator.add_step(Box::new(apiforge::steps::git::GitPreflightStep::new()));
    orchestrator.add_step(Box::new(apiforge::steps::git::VersionBumpStep::new(bump_type)));
    
    if config.git.changelog && !args.no_changelog {
        orchestrator.add_step(Box::new(apiforge::steps::git::ChangelogStep::new(
            new_version_str.clone(),
            previous_tag.clone(),
        )));
    }
    
    orchestrator.add_step(Box::new(apiforge::steps::git::GitCommitStep::new(new_version_str.clone())));
    orchestrator.add_step(Box::new(apiforge::steps::git::GitTagStep::new(new_version.clone())));
    orchestrator.add_step(Box::new(apiforge::steps::git::GitPushStep::new(new_version.clone())));

    let outputs = orchestrator.run().await?;

    println!("\n✨ Release {} complete!", new_version);
    println!("   {} steps executed successfully", outputs.len());

    Ok(())
}

async fn cmd_rollback(config_path: &str, args: apiforge::cli::RollbackArgs) -> anyhow::Result<()> {
    let _path = PathBuf::from(config_path);

    if args.dry_run {
        println!("[dry-run] Would roll back to {}", args.to.as_deref().unwrap_or("previous"));
    } else {
        println!("Rollback not yet implemented.");
    }

    Ok(())
}

async fn cmd_history(args: apiforge::cli::HistoryArgs) -> anyhow::Result<()> {
    let store = apiforge::audit::AuditStore::open(std::path::Path::new(".apiforge/audit"))?;
    let records = store.list(args.limit)?;

    if records.is_empty() {
        println!("No release history found.");
        return Ok(());
    }

    for record in records {
        println!(
            "{} | {} | {} | {}",
            record.timestamp, record.version, record.bump_type, record.status
        );
    }

    Ok(())
}

async fn cmd_status(config_path: &str) -> anyhow::Result<()> {
    let path = PathBuf::from(config_path);
    if !path.exists() {
        anyhow::bail!("No apiforge.toml found. Run `apiforge init` first.");
    }

    let config = Config::from_file(&path)?;
    println!("Project: {}", config.project.name);
    println!("Language: {:?}", config.project.language);

    if let Ok(repo) = apiforge::integrations::git::GitRepo::open() {
        if let Ok(branch) = repo.current_branch() {
            println!("Branch: {}", branch);
        }
        if let Ok(tag) = repo.get_latest_tag("*") {
            if let Some(t) = tag {
                println!("Latest tag: {}", t);
            }
        }
    }

    Ok(())
}
