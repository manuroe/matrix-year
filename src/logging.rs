/// Per-account SDK logging configuration.
///
/// Logs are stored in the account's working directory under `sdk_logs/`.
/// Each crawl session appends to the log file with clear separators.
use anyhow::{Context, Result};
use std::path::Path;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initializes SDK logging for a specific account.
///
/// Logs are written to `{account_dir}/sdk_logs/sdk.log` with rotation.
/// Each crawl session starts with a separator containing timestamp and account ID.
///
/// # Arguments
///
/// * `account_dir` - Path to the account's working directory
/// * `account_id` - Matrix user ID for log context
pub fn init_account_logging(account_dir: &Path, account_id: &str) -> Result<()> {
    let log_dir = account_dir.join("sdk_logs");
    std::fs::create_dir_all(&log_dir)
        .with_context(|| format!("Failed to create log directory: {}", log_dir.display()))?;

    // Create file appender with rotation (10MB max size, keep 5 files)
    let file_appender = tracing_appender::rolling::never(&log_dir, "sdk.log");

    // Set up formatting layer
    let file_layer = fmt::layer()
        .with_writer(file_appender)
        .with_ansi(false) // No ANSI codes in log files
        .with_target(true)
        .with_thread_ids(false)
        .with_line_number(true);

    // Default to INFO level, but allow override via RUST_LOG env var
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,matrix_sdk=debug"));

    // Initialize the subscriber
    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .try_init()
        .ok(); // Ignore error if already initialized

    // Write session separator with timestamp
    let separator = format!(
        "\n{sep}\n[{ts}] New crawl session: {account}\n{sep}\n",
        sep = "=".repeat(80),
        ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        account = account_id
    );

    // Append separator to log file
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("sdk.log"))
    {
        let _ = writeln!(file, "{}", separator);
    }

    tracing::info!("SDK logging initialized for account: {}", account_id);

    Ok(())
}
