use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod login;
mod logout;
mod renderer;
mod secrets;
mod stats;
mod status;

// Help text constants
const HELP_MAIN: &str = "\
my — Matrix recap tool (year, month, week, day, life)

Commands:
    --render [formats]   Render reports (md,html).

Usage:
    my --render [formats] --json-stats <path> [--output <dir>]

More help:
    my --help render";

const HELP_RENDER: &str = "\
Render reports (md,html)

Usage:
    my --render [formats] --json-stats <path> [--output <dir>]

Options:
    --render [formats]   Comma-separated formats (md,html). Empty renders all.
    --json-stats <path>  Optional stats JSON (required for now; DB later). Must include scope: year, month, week, day, or life.
    --output <dir>       Output directory (default: current dir). Filenames are auto-generated based on scope.

Examples:
  my --render md --json-stats examples/example-stats.json --output examples
  my --render md,html --json-stats examples/example-stats.json --output reports";

#[derive(Parser)]
#[command(name = "my", disable_help_flag = true)]
#[command(about = "Matrix year-in-review tool", long_about = None)]
struct Cli {
    /// Render formats (comma-separated: md,html). Renders all if no formats specified.
    #[arg(long)]
    render: Option<String>,

    /// Path to JSON stats file (optional, for development; will use DB later)
    #[arg(long)]
    json_stats: Option<PathBuf>,

    /// Output directory (defaults to current directory)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Show help (global or per topic). Example: my --help render
    #[arg(long, value_name = "TOPIC", num_args = 0..=1, default_missing_value = "")]
    help: Option<String>,

    /// Optional subcommand (e.g., login)
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Log into a Matrix account and securely store credentials
    Login {
        /// Matrix user id (e.g. @alice:example.org). If omitted, interactive selection/creation.
        #[arg(long)]
        user_id: Option<String>,
    },
    /// Log out from a Matrix account and remove stored credentials
    Logout {
        /// Matrix user id (e.g. @alice:example.org). If omitted, interactive selection.
        #[arg(long)]
        user_id: Option<String>,
    },
    /// Show account and credential status
    Status {
        /// Matrix user id (e.g. @alice:example.org). If omitted, show all.
        #[arg(long)]
        user_id: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Some(help_topic) = cli.help {
        let topic = help_topic.trim();
        if topic.is_empty() {
            println!("{}", HELP_MAIN);
        } else if topic.eq_ignore_ascii_case("render") {
            println!("{}", HELP_RENDER);
        } else {
            println!("Unknown help topic: {}", topic);
        }
        return Ok(());
    }

    // Handle subcommands first
    if let Some(cmd) = cli.command {
        match cmd {
            Commands::Login { user_id } => {
                // Run interactive login flow
                tokio::runtime::Runtime::new()
                    .context("Failed to create Tokio runtime")?
                    .block_on(login::run(user_id))?;
                return Ok(());
            }
            Commands::Logout { user_id } => {
                // Run logout flow
                tokio::runtime::Runtime::new()
                    .context("Failed to create Tokio runtime")?
                    .block_on(logout::run(user_id))?;
                return Ok(());
            }
            Commands::Status { user_id } => {
                status::run(user_id)?;
                return Ok(());
            }
        }
    }

    if let Some(render_arg) = cli.render {
        // Load stats
        let stats = if let Some(json_path) = cli.json_stats {
            stats::Stats::load_from_file(&json_path)?
        } else {
            anyhow::bail!("--json-stats is currently required (DB support coming later)");
        };

        // Determine output directory
        let output_dir = cli.output.unwrap_or_else(|| PathBuf::from("."));
        std::fs::create_dir_all(&output_dir).with_context(|| {
            format!(
                "Failed to create output directory: {}",
                output_dir.display()
            )
        })?;

        // Parse formats
        let formats: Vec<&str> = if render_arg.is_empty() {
            // Empty string means render all
            vec!["md"]
        } else {
            render_arg.split(',').map(|s| s.trim()).collect()
        };

        // Render each format
        for format in formats {
            match format {
                "md" => {
                    let markdown = renderer::md::render(&stats)?;
                    let filename = default_md_filename(&stats);
                    let output_path = output_dir.join(filename);
                    std::fs::write(&output_path, markdown)?;
                    eprintln!("Markdown report written to: {}", output_path.display());
                }
                _ => {
                    eprintln!("Warning: Unknown format '{}', skipping", format);
                }
            }
        }

        Ok(())
    } else {
        eprintln!("No action specified. Use --render to generate reports.");
        eprintln!("Example: my --render md --json-stats stats.json");
        Ok(())
    }
}

fn default_md_filename(stats: &stats::Stats) -> String {
    match stats.scope.kind {
        stats::ScopeKind::Year => format!("my-year-{}.md", stats.scope.key),
        stats::ScopeKind::Month => format!("my-month-{}.md", stats.scope.key),
        stats::ScopeKind::Week => format!("my-week-{}.md", stats.scope.key),
        stats::ScopeKind::Day => format!("my-day-{}.md", stats.scope.key),
        stats::ScopeKind::Life => "my-life.md".to_string(),
    }
}
