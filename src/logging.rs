/// Per-account SDK logging configuration.
///
/// Logs are stored in the account's working directory under `sdk_logs/`.
/// Each crawl session appends to the log file with clear separators.
use anyhow::{Context, Result};
use std::path::Path;
use std::sync::Once;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

static INIT: Once = Once::new();
static mut CURRENT_LOG_DIR: Option<std::path::PathBuf> = None;

/// Initializes SDK logging for a specific account.
///
/// Logs are written to `{account_dir}/sdk_logs/sdk.log` (no rotation).
/// Each session starts with a separator containing timestamp and account ID.
///
/// **Note:** The tracing subscriber can only be initialized once per process.
/// When processing multiple accounts, only the first account's log directory
/// is used for all subsequent logging. Session separators are still written
/// per-account to delineate operations.
///
/// # Arguments
///
/// * `account_dir` - Path to the account's working directory
/// * `account_id` - Matrix user ID for log context
pub fn init_account_logging(account_dir: &Path, account_id: &str) -> Result<()> {
    let log_dir = account_dir.join("sdk_logs");
    std::fs::create_dir_all(&log_dir)
        .with_context(|| format!("Failed to create log directory: {}", log_dir.display()))?;

    // Initialize the subscriber only once per process
    let mut init_successful = false;
    INIT.call_once(|| {
        // Create file appender (no rotation)
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
        if tracing_subscriber::registry()
            .with(filter)
            .with(file_layer)
            .try_init()
            .is_ok()
        {
            unsafe {
                CURRENT_LOG_DIR = Some(log_dir.clone());
            }
            init_successful = true;
        }
    });

    // Write session separator (always, even for subsequent accounts)
    let separator = format!(
        "\n{sep}\n[{ts}] New session: {account}\n{sep}\n",
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
        if let Err(e) = writeln!(file, "{}", separator) {
            tracing::warn!("Failed to write session separator to log file: {}", e);
        } else if let Err(e) = file.flush() {
            tracing::warn!("Failed to flush session separator to log file: {}", e);
        }
    }

    if init_successful {
        tracing::info!("SDK logging initialized for account: {}", account_id);
    } else {
        tracing::info!("SDK logging session started for account: {}", account_id);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_logging_creates_directory_and_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let account_dir = temp_dir.path().join("test_account");
        fs::create_dir_all(&account_dir).unwrap();

        init_account_logging(&account_dir, "@test:example.org").unwrap();

        let log_file = account_dir.join("sdk_logs/sdk.log");
        assert!(log_file.exists(), "Log file should be created");

        let contents = fs::read_to_string(&log_file).unwrap();
        assert!(
            contents.contains("New session: @test:example.org"),
            "Log should contain session separator"
        );
    }

    #[test]
    fn test_logging_handles_existing_log_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let account_dir = temp_dir.path().join("test_account");
        let log_dir = account_dir.join("sdk_logs");
        fs::create_dir_all(&log_dir).unwrap();

        // Create existing log file
        let log_file = log_dir.join("sdk.log");
        fs::write(&log_file, "Existing content\n").unwrap();

        init_account_logging(&account_dir, "@test:example.org").unwrap();

        let contents = fs::read_to_string(&log_file).unwrap();
        assert!(
            contents.contains("Existing content"),
            "Should preserve existing content"
        );
        assert!(
            contents.contains("New session: @test:example.org"),
            "Should append new separator"
        );
    }

    #[test]
    fn test_logging_fails_when_directory_cannot_be_created() {
        // Try to create log dir in a non-writable location (simulate permission error)
        #[cfg(unix)]
        {
            let result = init_account_logging(Path::new("/root/impossible"), "@test:example.org");
            assert!(
                result.is_err(),
                "Should fail when directory cannot be created"
            );
        }
    }
}
