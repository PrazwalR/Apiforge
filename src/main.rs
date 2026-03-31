use apiforge::cli::{Cli, Commands};
use apiforge::config::Config;
use clap::Parser;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
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

    // Generate a default config
    let default_config = format!(
        r#"[project]
name = "{name}"
language = "rust"

[git]
main_branch = "main"
tag_format = "v{{version}}"
changelog = true
commit_message = "chore: release v{{{{ version }}}}"
remote = "origin"
require_clean = true
require_main_branch = true

[docker]
registry = "aws_ecr"
repository = "{name}"
dockerfile = "Dockerfile"
context = "."
tags = ["{{version}}", "latest"]

[kubernetes]
context = "production"
namespace = "default"
deployment = "{name}"
manifest_path = "k8s/deployment.yaml"
image_field = ".spec.template.spec.containers[0].image"
rollout_timeout = 300
min_ready_percent = 100

[aws]
region = "us-east-1"

# Optional: GitHub release configuration
# [github]
# repository = "owner/repo"
# token = "${{GITHUB_TOKEN}}"
# create_release = true
# prerelease = false
# draft = false

# Optional: Notifications
# [notifications.slack]
# webhook_url = "${{SLACK_WEBHOOK_URL}}"
# message = "{{{{ status_emoji }}}} Release {{{{ version }}}} of {{{{ project }}}}: {{{{ status }}}}"
# notify_on = "both"

# Optional: Health check
# [health_check]
# url = "https://api.example.com/health"
# expected_status = 200
# timeout = 60
# interval = 5
"#,
        name = name
    );

    std::fs::write(&config_path, default_config)?;

    println!("✓ Initialized apiforge for '{}'", name);
    println!("  Created apiforge.toml — edit it to match your project setup.");
    println!("\nNext steps:");
    println!("  1. Edit apiforge.toml with your project settings");
    println!("  2. Run 'apiforge doctor' to validate your environment");
    println!("  3. Run 'apiforge release patch --dry-run' to preview a release");

    Ok(())
}

async fn cmd_doctor(config_path: &str) -> anyhow::Result<()> {
    use colored::Colorize;

    println!("\n{}", "▸ Environment checks".bold().cyan());

    let checks: Vec<(&str, fn() -> bool, &str)> = vec![
        ("git", || which::which("git").is_ok(), "Version control"),
        ("docker", || which::which("docker").is_ok(), "Container builds"),
        ("kubectl", || which::which("kubectl").is_ok(), "Kubernetes deployment"),
        ("aws", || which::which("aws").is_ok(), "AWS CLI (ECR auth)"),
    ];

    let mut all_ok = true;
    for (name, check, purpose) in &checks {
        let (status, color) = if check() {
            ("OK", "green")
        } else {
            all_ok = false;
            ("MISSING", "yellow")
        };
        let status_colored = match color {
            "green" => status.green(),
            "yellow" => status.yellow(),
            _ => status.normal(),
        };
        println!("  {} {} ... {} ({})", "•".dimmed(), name.bold(), status_colored, purpose.dimmed());
    }

    println!("\n{}", "▸ Configuration".bold().cyan());

    let path = PathBuf::from(config_path);
    if path.exists() {
        match Config::from_file(&path) {
            Ok(config) => {
                println!("  {} config ... {}", "•".dimmed(), "OK".green());
                println!("    Project: {}", config.project.name);
                println!("    Language: {:?}", config.project.language);
                println!("    Registry: {:?}", config.docker.registry);
            }
            Err(e) => {
                all_ok = false;
                println!("  {} config ... {} ({})", "•".dimmed(), "INVALID".red(), e);
            }
        }
    } else {
        all_ok = false;
        println!("  {} config ... {} (run `apiforge init`)", "•".dimmed(), "NOT FOUND".yellow());
    }

    println!("\n{}", "▸ Git repository".bold().cyan());

    match apiforge::integrations::git::GitRepo::open() {
        Ok(repo) => {
            println!("  {} repository ... {}", "•".dimmed(), "OK".green());
            if let Ok(branch) = repo.current_branch() {
                println!("    Branch: {}", branch);
            }
            if let Ok(Some(tag)) = repo.get_latest_tag("v*") {
                println!("    Latest tag: {}", tag);
            }
            if let Ok(clean) = repo.is_working_tree_clean() {
                let status = if clean { "clean".green() } else { "dirty".yellow() };
                println!("    Working tree: {}", status);
            }
        }
        Err(_) => {
            all_ok = false;
            println!("  {} repository ... {}", "•".dimmed(), "NOT FOUND".red());
        }
    }

    println!();
    if all_ok {
        println!("{}", "  ✓ All checks passed!".green().bold());
    } else {
        println!("{}", "  ⚠ Some checks failed. Fix the issues above before releasing.".yellow());
    }

    Ok(())
}

async fn cmd_release(config_path: &str, args: apiforge::cli::ReleaseArgs) -> anyhow::Result<()> {
    use colored::Colorize;
    use dialoguer::Confirm;

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
    } else if config.project.language == apiforge::config::Language::Node {
        let content = std::fs::read_to_string(&version_path)?;
        let json: serde_json::Value = serde_json::from_str(&content)?;
        json.get("version")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| anyhow::anyhow!("No version in package.json"))?
    } else {
        anyhow::bail!("Language {:?} not yet fully supported", config.project.language);
    };

    let new_version = apiforge::utils::bump_version(&current_version, bump_type)?;
    let new_version_str = new_version.to_string();

    let previous_tag = repo.get_latest_tag(&config.git.tag_format.replace("{version}", "*"))?;

    // Show release plan
    println!("\n{}", "▸ Release Plan".bold().cyan());
    println!("  Project:     {}", config.project.name.bold());
    println!("  Version:     {} → {}", current_version.dimmed(), new_version_str.green().bold());
    println!("  Bump type:   {}", args.bump);
    if let Some(ref tag) = previous_tag {
        println!("  Previous:    {}", tag.dimmed());
    }
    println!();

    // Show what will happen
    println!("{}", "  Steps to execute:".dimmed());
    println!("  1. Validate git repository state");
    println!("  2. Bump version in {}", version_file);
    if config.git.changelog && !args.no_changelog {
        println!("  3. Generate changelog");
    }
    println!("  4. Commit and tag");
    println!("  5. Push to remote");

    if !args.skip_docker {
        println!("  6. Build Docker image");
        println!("  7. Push to {:?}", config.docker.registry);
    }

    if !args.skip_k8s {
        println!("  8. Update Kubernetes deployment");
        println!("  9. Wait for rollout");
    }

    if !args.skip_github && config.github.is_some() {
        println!("  10. Create GitHub release");
    }

    if config.health_check.is_some() {
        println!("  11. Verify service health");
    }

    println!();

    // Confirmation prompt (unless --yes or --dry-run)
    if !args.dry_run && !args.yes {
        let confirmed = Confirm::new()
            .with_prompt("Proceed with release?")
            .default(false)
            .interact()?;

        if !confirmed {
            println!("Release cancelled.");
            return Ok(());
        }
    }

    // Build the orchestrator with all steps
    let mut orchestrator =
        apiforge::orchestrator::ReleaseOrchestrator::new(config.clone(), args.dry_run);

    // Git steps
    orchestrator.add_step(Box::new(apiforge::steps::git::GitPreflightStep::new()));
    orchestrator.add_step(Box::new(apiforge::steps::git::VersionBumpStep::new(
        bump_type,
    )));

    if config.git.changelog && !args.no_changelog {
        orchestrator.add_step(Box::new(apiforge::steps::git::ChangelogStep::new(
            new_version_str.clone(),
            previous_tag.clone(),
        )));
    }

    orchestrator.add_step(Box::new(apiforge::steps::git::GitCommitStep::new(
        new_version_str.clone(),
    )));
    orchestrator.add_step(Box::new(apiforge::steps::git::GitTagStep::new(
        new_version.clone(),
    )));
    orchestrator.add_step(Box::new(apiforge::steps::git::GitPushStep::new(
        new_version.clone(),
    )));

    // Docker steps
    if !args.skip_docker {
        orchestrator.add_step(Box::new(apiforge::steps::docker::DockerBuildStep::new(
            new_version.clone(),
        )));
        orchestrator.add_step(Box::new(apiforge::steps::docker::DockerPushStep::new(
            new_version.clone(),
        )));
    }

    // Kubernetes steps
    if !args.skip_k8s {
        orchestrator.add_step(Box::new(apiforge::steps::kubernetes::K8sUpdateStep::new(
            new_version.clone(),
        )));
        orchestrator.add_step(Box::new(apiforge::steps::kubernetes::K8sRolloutStep::new()));
    }

    // GitHub release
    if !args.skip_github && config.github.is_some() {
        orchestrator.add_step(Box::new(
            apiforge::steps::github::GitHubReleaseStep::new(new_version.clone())
                .with_previous_tag(previous_tag.clone()),
        ));
    }

    // Health check
    if config.health_check.is_some() {
        orchestrator.add_step(Box::new(apiforge::steps::health::HealthCheckStep::new(
            new_version.clone(),
        )));
    }

    // Run the pipeline
    let outputs = orchestrator.run().await?;

    // Success notification
    if !args.skip_notify && config.notifications.is_some() {
        let notify_result = send_success_notification(&config, &new_version).await;
        if let Err(e) = notify_result {
            tracing::warn!("Failed to send notification: {}", e);
        }
    }

    // Record in audit log
    let audit_dir = std::path::Path::new(".apiforge/audit");
    if let Ok(store) = apiforge::audit::AuditStore::open(audit_dir) {
        let record = apiforge::audit::AuditStore::new_record(
            &new_version_str,
            &bump_type.to_string(),
            args.dry_run,
        );
        let _ = store.record(&record);
    }

    // Output results
    if args.output == "json" {
        let result = serde_json::json!({
            "success": true,
            "version": new_version_str,
            "bump_type": bump_type.to_string(),
            "dry_run": args.dry_run,
            "steps": outputs.iter().map(|o| serde_json::json!({
                "status": o.status.to_string(),
                "message": o.message,
                "duration_ms": o.duration_ms
            })).collect::<Vec<_>>()
        });
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("\n{}", format!("✨ Release {} complete!", new_version).green().bold());
        println!("   {} steps executed successfully", outputs.len());
    }

    Ok(())
}

async fn send_success_notification(
    config: &Config,
    version: &semver::Version,
) -> anyhow::Result<()> {
    // This would use the notification steps, simplified here
    if let Some(ref notifications) = config.notifications {
        if let Some(ref slack) = notifications.slack {
            let client = reqwest::Client::new();
            let message = slack
                .message
                .replace("{{ version }}", &version.to_string())
                .replace("{{ project }}", &config.project.name)
                .replace("{{ status }}", "success")
                .replace("{{ status_emoji }}", "✅");

            let payload = serde_json::json!({
                "text": message
            });

            client.post(&slack.webhook_url).json(&payload).send().await?;
        }
    }
    Ok(())
}

async fn cmd_rollback(config_path: &str, args: apiforge::cli::RollbackArgs) -> anyhow::Result<()> {
    use colored::Colorize;

    let path = PathBuf::from(config_path);
    let config = Config::from_file(&path)?;

    let repo = apiforge::integrations::git::GitRepo::open()?;

    // Get the target version
    let target_version = if let Some(ref to_version) = args.to {
        to_version.clone()
    } else {
        // Get previous tag
        let _tags = repo.get_latest_tag("v*")?;
        // Would need to get second-to-last tag
        anyhow::bail!("Automatic rollback target detection not yet implemented. Please specify --to <version>");
    };

    println!("\n{}", "▸ Rollback Plan".bold().cyan());
    println!("  Target version: {}", target_version.bold());

    if args.dry_run {
        println!("\n{}", "[dry-run] Would perform the following:".yellow());
        println!("  1. Update Kubernetes deployment to {}", target_version);
        println!("  2. Wait for rollout");
        println!("  3. Verify health check");
        return Ok(());
    }

    // Perform Kubernetes rollback
    let k8s = apiforge::integrations::kubernetes::K8sClient::new(&config.kubernetes.context).await?;

    // Build the full image name with target version
    let image_base = match config.docker.registry {
        apiforge::config::DockerRegistry::AwsEcr => {
            let aws = apiforge::integrations::aws::AwsClient::new(&config.aws.region).await?;
            let (account_id, _) = aws.get_caller_identity().await?;
            let registry_url = aws.get_ecr_registry_url(&account_id);
            format!("{}/{}", registry_url, config.docker.repository)
        }
        _ => config.docker.repository.clone(),
    };

    let target_image = format!("{}:{}", image_base, target_version.trim_start_matches('v'));

    println!("  Rolling back to: {}", target_image);

    k8s.update_deployment_image(
        &config.kubernetes.namespace,
        &config.kubernetes.deployment,
        &config.kubernetes.image_field,
        &target_image,
    )
    .await?;

    println!("  Waiting for rollout...");

    k8s.wait_for_rollout(
        &config.kubernetes.namespace,
        &config.kubernetes.deployment,
        config.kubernetes.rollout_timeout,
        |status| {
            println!(
                "    {}/{} replicas ready",
                status.ready_replicas, status.desired_replicas
            );
        },
    )
    .await?;

    println!("\n{}", format!("✓ Rollback to {} complete!", target_version).green().bold());

    Ok(())
}

async fn cmd_history(args: apiforge::cli::HistoryArgs) -> anyhow::Result<()> {
    use colored::Colorize;
    use comfy_table::{ContentArrangement, Table};

    let store = apiforge::audit::AuditStore::open(std::path::Path::new(".apiforge/audit"))?;
    let records = store.list(args.limit)?;

    if records.is_empty() {
        println!("No release history found.");
        println!("Run 'apiforge release patch' to create your first release.");
        return Ok(());
    }

    if args.output == "json" {
        let json = serde_json::to_string_pretty(&records)?;
        println!("{}", json);
        return Ok(());
    }

    let mut table = Table::new();
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec!["Timestamp", "Version", "Type", "Status", "Duration"]);

    for record in records {
        let status_display = match record.status {
            apiforge::audit::ReleaseStatus::Success => "✓ success".green().to_string(),
            apiforge::audit::ReleaseStatus::Failed => "✗ failed".red().to_string(),
            apiforge::audit::ReleaseStatus::RolledBack => "⟲ rolled back".yellow().to_string(),
        };

        // Filter by status if requested
        if let Some(ref filter) = args.filter {
            let matches = match filter.as_str() {
                "success" => record.status == apiforge::audit::ReleaseStatus::Success,
                "failed" => record.status == apiforge::audit::ReleaseStatus::Failed,
                _ => true,
            };
            if !matches {
                continue;
            }
        }

        let dry_run_marker = if record.dry_run { " (dry-run)" } else { "" };

        table.add_row(vec![
            record.timestamp,
            format!("{}{}", record.version, dry_run_marker),
            record.bump_type,
            status_display,
            format!("{}ms", record.duration_ms),
        ]);
    }

    println!("\n{}", "▸ Release History".bold().cyan());
    println!("{table}");

    Ok(())
}

async fn cmd_status(config_path: &str) -> anyhow::Result<()> {
    use colored::Colorize;

    let path = PathBuf::from(config_path);
    if !path.exists() {
        anyhow::bail!("No apiforge.toml found. Run `apiforge init` first.");
    }

    let config = Config::from_file(&path)?;

    println!("\n{}", "▸ Project Status".bold().cyan());
    println!("  Project:  {}", config.project.name.bold());
    println!("  Language: {:?}", config.project.language);

    if let Ok(repo) = apiforge::integrations::git::GitRepo::open() {
        println!("\n{}", "▸ Git".bold().cyan());
        if let Ok(branch) = repo.current_branch() {
            println!("  Branch:      {}", branch);
        }
        if let Ok(Some(tag)) = repo.get_latest_tag("v*") {
            println!("  Latest tag:  {}", tag.green());
        }
        if let Ok(sha) = repo.current_commit_sha() {
            println!("  HEAD:        {}", &sha[..8].dimmed());
        }
    }

    // Try to get current deployed version from Kubernetes
    println!("\n{}", "▸ Kubernetes".bold().cyan());
    match apiforge::integrations::kubernetes::K8sClient::new(&config.kubernetes.context).await {
        Ok(k8s) => {
            println!("  Context:    {}", config.kubernetes.context);
            println!("  Namespace:  {}", config.kubernetes.namespace);

            match k8s.get_deployment(&config.kubernetes.namespace, &config.kubernetes.deployment).await {
                Ok(deployment) => {
                    let image = deployment
                        .spec
                        .as_ref()
                        .and_then(|s| s.template.spec.as_ref())
                        .and_then(|s| s.containers.first())
                        .map(|c| c.image.as_deref().unwrap_or("unknown"))
                        .unwrap_or("unknown");

                    println!("  Deployment: {} ({})", config.kubernetes.deployment, "running".green());
                    println!("  Image:      {}", image);

                    if let Ok(status) = k8s.get_rollout_status(&config.kubernetes.namespace, &config.kubernetes.deployment).await {
                        let ready_status = if status.ready {
                            format!("{}/{} ready", status.ready_replicas, status.desired_replicas).green()
                        } else {
                            format!("{}/{} ready", status.ready_replicas, status.desired_replicas).yellow()
                        };
                        println!("  Replicas:   {}", ready_status);
                    }
                }
                Err(_) => {
                    println!("  Deployment: {} ({})", config.kubernetes.deployment, "not found".red());
                }
            }
        }
        Err(_) => {
            println!("  {} Unable to connect to cluster", "⚠".yellow());
        }
    }

    Ok(())
}
