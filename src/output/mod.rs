use colored::Colorize;
use comfy_table::{ContentArrangement, Table};

use crate::steps::{StepOutput, StepStatus};
use crate::utils::sanitize_message;

pub struct OutputManager {}

impl OutputManager {
    pub fn new() -> Self {
        Self {}
    }

    pub fn section(&self, title: &str) {
        println!("\n{}", format!("▸ {}", title).bold().cyan());
    }

    pub fn step_status(&self, name: &str, status: &str) {
        println!("  {} {}", name.bold(), status.dimmed());
    }

    pub fn step_ok(&self, name: &str) {
        println!("  {} {}", "✓".green().bold(), name);
    }

    pub fn step_done(&self, name: &str, output: &StepOutput) {
        let icon = match output.status {
            StepStatus::Success => "✓".green().bold(),
            StepStatus::Skipped => "⊘".yellow().bold(),
            StepStatus::Failed => "✗".red().bold(),
        };
        let timing = format!("({}ms)", output.duration_ms).dimmed();
        println!("  {} {} {} {}", icon, name.bold(), output.message, timing);

        // Display dry-run details if present
        if let Some(ref details) = output.dry_run_details {
            self.print_dry_run_details(details);
        }
    }

    fn print_dry_run_details(&self, details: &crate::steps::DryRunDetails) {
        // Print file changes
        for change in &details.file_changes {
            let op_icon = match change.operation {
                crate::steps::FileOperation::Create => "+",
                crate::steps::FileOperation::Modify => "~",
                crate::steps::FileOperation::Delete => "-",
            };
            println!(
                "    {} {} {} {}",
                "├─".dimmed(),
                op_icon.yellow(),
                change.path.dimmed(),
                format!("({:?})", change.operation).dimmed()
            );
            if let Some(ref diff) = change.diff {
                for line in diff.lines() {
                    println!("    {} {}", "│".dimmed(), line.cyan());
                }
            }
        }

        // Print Docker preview
        if let Some(ref docker) = details.docker_preview {
            println!(
                "    {} {} {}",
                "├─".dimmed(),
                "📦".to_string().yellow(),
                format!("Docker image: {}", docker.image_name).dimmed()
            );
            println!(
                "    {} {} {}",
                "│".dimmed(),
                "🏷️".dimmed(),
                format!("Tags: {}", docker.tags.join(", ")).dimmed()
            );
            if let Some(layers) = docker.layers_estimate {
                println!(
                    "    {} {} {}",
                    "│".dimmed(),
                    "📚".dimmed(),
                    format!("Estimated layers: {}", layers).dimmed()
                );
            }
        }

        // Print notes
        for note in &details.notes {
            println!("    {} {} {}", "├─".dimmed(), "ℹ".dimmed(), note.dimmed());
        }
    }

    pub fn step_fail(&self, name: &str, error: &str) {
        let safe_error = sanitize_message(error);
        println!(
            "  {} {} {}",
            "✗".red().bold(),
            name.bold(),
            safe_error.red()
        );
    }

    pub fn blank_line(&self) {
        println!();
    }

    pub fn summary_table(&self, outputs: &[(&str, &StepOutput)]) {
        let mut table = Table::new();
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.set_header(vec!["Step", "Status", "Duration", "Message"]);

        for (name, out) in outputs {
            table.add_row(vec![
                name.to_string(),
                out.status.to_string(),
                format!("{}ms", out.duration_ms),
                out.message.clone(),
            ]);
        }

        println!("{table}");
    }

    pub fn success(&self, msg: &str) {
        println!("\n{}", format!("  ✓ {}", msg).green().bold());
    }

    pub fn error(&self, msg: &str) {
        let safe_msg = sanitize_message(msg);
        println!("\n{}", format!("  ✗ {}", safe_msg).red().bold());
    }

    pub fn info(&self, msg: &str) {
        println!("  {}", msg);
    }

    pub fn warn(&self, msg: &str) {
        println!("  {}", format!("⚠ {}", msg).yellow());
    }
}

impl Default for OutputManager {
    fn default() -> Self {
        Self::new()
    }
}
