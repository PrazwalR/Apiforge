use colored::Colorize;
use comfy_table::{ContentArrangement, Table};

use crate::steps::{StepOutput, StepStatus};

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
    }

    pub fn step_fail(&self, name: &str, error: &str) {
        println!("  {} {} {}", "✗".red().bold(), name.bold(), error.red());
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
        println!("\n{}", format!("  ✗ {}", msg).red().bold());
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
