use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

mod account_selector;
mod commands;
mod logging;
mod sdk;
mod secrets;
mod stats;
mod timefmt;
mod window;

// Help text constants
const HELP_MAIN: &str = "\
my — Matrix recap tool (year, month, week, day, life)

Commands:
    login               Log into a Matrix account
    logout              Log out from a Matrix account
    status              Show account and credential status
    crawl <window>      Crawl Matrix data for a time window
    reset               Reset crawl metadata and SDK data
    render              Render reports from stats files
    <window>            Crawl and render for a time window (shorthand)

Time Windows:
    2025                Year
    2025-03             Month
    2025-W12            Week
    2025-03-15          Day
    life                Entire history

Examples:
    my login
    my 2025                          # Crawl + render year 2025
    my 2025 --output reports         # With custom output directory
    my crawl 2025-03 --user-id @me:example.org
    my render --stats examples/stats/example-stats.json

More help:
    my --help render";

const HELP_RENDER: &str = "\
Render reports from stats files

Usage:
    my render --stats <path> [--formats <list>] [--output <dir>]

Options:
    --stats <path>       Path to stats JSON file (required)
    --formats <list>     Comma-separated formats (md,html). Default: md
    --output <dir>       Output directory (default: current directory)

Examples:
    my render --stats examples/stats/example-stats.json
    my render --stats examples/stats/example-stats.json --formats md
    my render --stats stats.json --output reports";

#[derive(Parser)]
#[command(name = "my", disable_help_flag = true)]
#[command(about = "Matrix year-in-review tool", long_about = None)]
struct Cli {
    /// Show help (global or per topic). Example: my --help render
    #[arg(long, value_name = "TOPIC", num_args = 0..=1, default_missing_value = "")]
    help: Option<String>,

    /// Subcommand or time window (e.g., login, crawl, 2025)
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
        /// List all rooms with their crawl metadata
        #[arg(long)]
        list: bool,
    },
    /// Crawl Matrix messages into the SDK database for a time window
    Crawl {
        /// Time window (e.g. 2025, 2025-03, 2025-W12, 2025-03-15, life)
        window: String,
        /// Matrix user id (e.g. @alice:example.org). If omitted, crawl all accounts.
        #[arg(long)]
        user_id: Option<String>,
    },
    /// Reset crawl metadata and SDK data (keeps credentials)
    Reset {
        /// Matrix user id (e.g. @alice:example.org). If omitted, reset all accounts.
        #[arg(long)]
        user_id: Option<String>,
    },
    /// Render reports from stats files (md, html)
    Render {
        /// Path to JSON stats file
        #[arg(long)]
        stats: PathBuf,
        /// Comma-separated formats (md,html). Empty renders all.
        #[arg(long, default_value = "")]
        formats: String,
        /// Output directory (defaults to current directory)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Crawl and render for a time window (shorthand: my 2025)
    #[command(external_subcommand)]
    Window(Vec<String>),
}

/// Arguments for the window shorthand command parsed via clap
#[derive(Parser, Debug)]
struct WindowArgs {
    /// Time window (e.g. 2025, 2025-03, 2025-W12, 2025-03-15, life)
    window: String,
    /// Matrix user id (optional). If omitted, prompts for selection.
    #[arg(long)]
    user_id: Option<String>,
    /// Comma-separated formats (md,html). Default: md.
    #[arg(long, default_value = "")]
    formats: String,
    /// Output directory (defaults to current directory).
    #[arg(short, long)]
    output: Option<PathBuf>,
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

    if let Some(cmd) = cli.command {
        match cmd {
            Commands::Login { user_id } => {
                tokio::runtime::Runtime::new()
                    .context("Failed to create Tokio runtime")?
                    .block_on(commands::login::run(user_id))?;
                return Ok(());
            }
            Commands::Logout { user_id } => {
                tokio::runtime::Runtime::new()
                    .context("Failed to create Tokio runtime")?
                    .block_on(commands::logout::run(user_id))?;
                return Ok(());
            }
            Commands::Status { user_id, list } => {
                tokio::runtime::Runtime::new()
                    .context("Failed to create Tokio runtime")?
                    .block_on(commands::status::run(user_id, list))?;
                return Ok(());
            }
            Commands::Crawl { window, user_id } => {
                let account_stats = tokio::runtime::Runtime::new()
                    .context("Failed to create Tokio runtime")?
                    .block_on(commands::crawl::run(window, user_id))?;

                for (account_id, stats) in account_stats {
                    let data_dir = commands::login::resolve_data_root()?;
                    let account_dirname = commands::login::account_id_to_dirname(&account_id);
                    let account_dir = data_dir.join("accounts").join(&account_dirname);
                    let stats_filename = format!("stats-{}.json", stats.scope.key);
                    let stats_path = account_dir.join(stats_filename);

                    std::fs::create_dir_all(&account_dir).context(format!(
                        "Failed to create account directory: {:?}",
                        account_dir
                    ))?;

                    let stats_json = serde_json::to_string_pretty(&stats)
                        .context("Failed to serialize stats")?;
                    std::fs::write(&stats_path, stats_json)
                        .context(format!("Failed to write stats file: {:?}", stats_path))?;

                    eprintln!("📊 Stats saved: {}", stats_path.display());
                }

                return Ok(());
            }
            Commands::Reset { user_id } => {
                tokio::runtime::Runtime::new()
                    .context("Failed to create Tokio runtime")?
                    .block_on(commands::reset::run(user_id))?;
                return Ok(());
            }
            Commands::Render {
                stats,
                formats,
                output,
            } => {
                handle_render(stats, formats, output)?;
                return Ok(());
            }
            Commands::Window(args) => {
                if args.is_empty() {
                    anyhow::bail!("Window pattern required (e.g., my 2025)");
                }

                let mut argv = vec!["window".to_string()];
                argv.extend(args);
                let parsed = WindowArgs::try_parse_from(argv)?;

                handle_window(parsed.window, parsed.user_id, parsed.formats, parsed.output)?;
                return Ok(());
            }
        }
    }

    eprintln!("No action specified. Try 'my --help' for usage.");
    Ok(())
}

fn handle_window(
    window: String,
    user_id_flag: Option<String>,
    formats: String,
    output: Option<PathBuf>,
) -> Result<()> {
    eprintln!("🔍 Window: {}", window);

    let mut selector = account_selector::AccountSelector::new()?;
    let accounts = selector.select_accounts(user_id_flag.as_ref().cloned(), false)?;

    if accounts.is_empty() {
        anyhow::bail!("No accounts found. Use 'my login' first.");
    } else if accounts.len() > 1 {
        anyhow::bail!(
            "Multiple accounts found. Window command requires exactly one account. \
             Use --user-id to specify which account."
        );
    }

    let (account_id, account_dir) = &accounts[0];
    eprintln!("📱 Account: {}", account_id);

    eprintln!("\n🔄 Crawling {}...", window);
    let account_stats = tokio::runtime::Runtime::new()
        .context("Failed to create Tokio runtime")?
        .block_on(commands::crawl::run(
            window.clone(),
            Some(account_id.clone()),
        ))?;

    let (acc_id, stats) = account_stats
        .into_iter()
        .next()
        .context("Expected exactly one account's stats from crawl::run")?;

    let stats_filename = format!("stats-{}.json", stats.scope.key);
    let stats_path = account_dir.join(stats_filename);

    std::fs::create_dir_all(account_dir).context(format!(
        "Failed to create account directory: {:?}",
        account_dir
    ))?;

    let stats_json = serde_json::to_string_pretty(&stats).context("Failed to serialize stats")?;
    std::fs::write(&stats_path, stats_json)
        .context(format!("Failed to write stats file: {:?}", stats_path))?;

    eprintln!("📊 Stats saved: {}", stats_path.display());

    eprintln!("\n📝 Rendering reports...");
    let output_dir = output.unwrap_or_else(|| PathBuf::from("."));
    render_stats(&stats, &output_dir, &formats)?;

    eprintln!("\n✅ Done! Window {} processed for {}", window, acc_id);

    Ok(())
}

fn handle_render(stats_path: PathBuf, formats: String, output: Option<PathBuf>) -> Result<()> {
    let stats = stats::Stats::load_from_file(&stats_path)?;
    let output_dir = output.unwrap_or_else(|| PathBuf::from("."));
    render_stats(&stats, &output_dir, &formats)?;
    Ok(())
}

fn render_stats(stats: &stats::Stats, output_dir: &Path, formats_arg: &str) -> Result<()> {
    std::fs::create_dir_all(output_dir).with_context(|| {
        format!(
            "Failed to create output directory: {}",
            output_dir.display()
        )
    })?;

    let formats: Vec<&str> = if formats_arg.is_empty() {
        vec!["md"]
    } else {
        formats_arg.split(',').map(|s| s.trim()).collect()
    };

    for format in formats {
        match format {
            "md" => {
                let markdown = commands::render::md::render(stats)?;
                let filename = default_md_filename(stats);
                let output_path = output_dir.join(filename);
                std::fs::write(&output_path, markdown)?;
                eprintln!("📄 Markdown: {}", output_path.display());
            }
            _ => {
                eprintln!("⚠️  Warning: Unknown format '{}', skipping", format);
            }
        }
    }

    Ok(())
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
