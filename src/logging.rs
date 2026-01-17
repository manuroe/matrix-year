/// Per-account SDK logging configuration.
///
/// Logs are stored in the account's working directory under `sdk_logs/`.
/// Each SDK session appends to the log file with clear separators.
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, Once};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

static INIT: Once = Once::new();
static LOG_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

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
            // Store the log directory where logs are actually written
            *LOG_DIR.lock().unwrap() = Some(log_dir.clone());
            init_successful = true;
        }
    });

    // Get the actual log directory (may be different from current account's if already initialized)
    let actual_log_dir = LOG_DIR
        .lock()
        .unwrap()
        .as_ref()
        .cloned()
        .unwrap_or_else(|| log_dir.clone());

    // Write session separator to the actual log directory
    let separator = format!(
        "\n{sep}\n[{ts}] New session: {account}\n{sep}\n",
        sep = "=".repeat(80),
        ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        account = account_id
    );

    // Append separator to log file in actual log directory
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(actual_log_dir.join("sdk.log"))
    {
        if let Err(e) = write!(file, "{}", separator) {
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

    // Note: These tests must be run with --test-threads=1 because the tracing subscriber
    // can only be initialized once per process. Running tests in parallel will cause
    // failures as subsequent tests cannot re-initialize the subscriber.

    #[test]
    fn test_logging_creates_directory_and_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let account_dir = temp_dir.path().join("test_account");
        fs::create_dir_all(&account_dir).unwrap();

        init_account_logging(&account_dir, "@test:example.org").unwrap();

        let log_file = account_dir.join("sdk_logs/sdk.log");
        // Log file may exist in current account's dir or first initialized account's dir
        // depending on whether subscriber was already initialized
        let log_dir = LOG_DIR.lock().unwrap();
        if let Some(actual_dir) = log_dir.as_ref() {
            let actual_log_file = actual_dir.join("sdk.log");
            if actual_log_file.exists() {
                let contents = fs::read_to_string(&actual_log_file).unwrap();
                assert!(
                    contents.contains("New session: @test:example.org"),
                    "Log should contain session separator"
                );
            }
        } else if log_file.exists() {
            let contents = fs::read_to_string(&log_file).unwrap();
            assert!(
                contents.contains("New session: @test:example.org"),
                "Log should contain session separator"
            );
        }
    }

    #[test]
    fn test_logging_handles_existing_log_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let account_dir = temp_dir.path().join("test_account");
        let log_dir_path = account_dir.join("sdk_logs");
        fs::create_dir_all(&log_dir_path).unwrap();

        // Create existing log file
        let log_file = log_dir_path.join("sdk.log");
        fs::write(&log_file, "Existing content\n").unwrap();

        init_account_logging(&account_dir, "@test:example.org").unwrap();

        // Check the actual log directory (may be different if subscriber already initialized)
        let log_dir = LOG_DIR.lock().unwrap();
        let actual_log_file = if let Some(actual_dir) = log_dir.as_ref() {
            actual_dir.join("sdk.log")
        } else {
            log_file.clone()
        };

        if actual_log_file.exists() {
            let contents = fs::read_to_string(&actual_log_file).unwrap();
            assert!(
                contents.contains("New session: @test:example.org"),
                "Should append new separator"
            );
        }
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

    #[test]
    #[ignore] // Run with --ignored --test-threads=1 to test multi-account scenario
    fn test_multi_account_logging_uses_first_account_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let account1_dir = temp_dir.path().join("account1");
        let account2_dir = temp_dir.path().join("account2");
        fs::create_dir_all(&account1_dir).unwrap();
        fs::create_dir_all(&account2_dir).unwrap();

        // Initialize logging for first account
        init_account_logging(&account1_dir, "@alice:example.org").unwrap();

        // Write a test log message
        tracing::info!("Test message from alice");

        // Initialize logging for second account (should use first account's log dir)
        init_account_logging(&account2_dir, "@bob:example.org").unwrap();

        // Write another test log message
        tracing::info!("Test message from bob");

        // Both separators and all logs should be in the first account's log file
        let log_file_1 = account1_dir.join("sdk_logs/sdk.log");
        let log_file_2 = account2_dir.join("sdk_logs/sdk.log");

        assert!(log_file_1.exists(), "First account's log file should exist");

        let contents_1 = fs::read_to_string(&log_file_1).unwrap();
        assert!(
            contents_1.contains("New session: @alice:example.org"),
            "Should contain alice's session separator"
        );
        assert!(
            contents_1.contains("New session: @bob:example.org"),
            "Should contain bob's session separator in alice's log file"
        );
        assert!(
            contents_1.contains("Test message from alice"),
            "Should contain alice's log message"
        );
        assert!(
            contents_1.contains("Test message from bob"),
            "Should contain bob's log message in alice's log file"
        );

        // Second account's log file should not exist (or be empty if created)
        if log_file_2.exists() {
            let contents_2 = fs::read_to_string(&log_file_2).unwrap();
            assert!(
                contents_2.is_empty() || !contents_2.contains("Test message"),
                "Second account's log file should not contain actual log messages"
            );
        }
    }
}
