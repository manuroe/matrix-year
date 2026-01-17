use my::logging;

fn main() -> anyhow::Result<()> {
    // Create a temporary test directory
    let test_dir = tempfile::tempdir()?;
    let account_dir = test_dir.path().join("test_account");
    std::fs::create_dir_all(&account_dir)?;

    println!("Testing SDK logging levels in: {}", account_dir.display());

    // Initialize logging
    logging::init_account_logging(&account_dir, "@test:example.org")?;

    // Simulate logs from different modules at different levels
    tracing::info!(target: "my", "App info message (should appear)");
    tracing::debug!(target: "my", "App debug message (should NOT appear - app is at info level)");
    tracing::info!(target: "matrix_sdk", "SDK info message (should appear)");
    tracing::debug!(target: "matrix_sdk", "SDK debug message (should appear - SDK is at debug level)");
    tracing::trace!(target: "matrix_sdk", "SDK trace message (should NOT appear - filter is debug, not trace)");

    // Give it a moment to flush
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Check if log file was created
    let log_file = account_dir.join("sdk_logs/sdk.log");
    if log_file.exists() {
        let contents = std::fs::read_to_string(&log_file)?;
        println!("\n📄 Log contents:");
        println!("{}", "=".repeat(80));
        println!("{}", contents);
        println!("{}", "=".repeat(80));

        // Verify expected logs are present
        let has_app_info = contents.contains("App info message");
        let has_app_debug = contents.contains("App debug message");
        let has_sdk_info = contents.contains("SDK info message");
        let has_sdk_debug = contents.contains("SDK debug message");
        let has_sdk_trace = contents.contains("SDK trace message");

        println!("\n✅ Verification:");
        println!(
            "  App info:    {} (expected: yes)",
            if has_app_info { "✅" } else { "❌" }
        );
        println!(
            "  App debug:   {} (expected: no)",
            if !has_app_debug { "✅" } else { "❌" }
        );
        println!(
            "  SDK info:    {} (expected: yes)",
            if has_sdk_info { "✅" } else { "❌" }
        );
        println!(
            "  SDK debug:   {} (expected: yes)",
            if has_sdk_debug { "✅" } else { "❌" }
        );
        println!(
            "  SDK trace:   {} (expected: no)",
            if !has_sdk_trace { "✅" } else { "❌" }
        );
    } else {
        println!("❌ Log file not created");
    }

    Ok(())
}
