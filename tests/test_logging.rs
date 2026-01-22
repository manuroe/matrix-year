use my::logging;

fn main() -> anyhow::Result<()> {
    // Create a temporary test directory
    let test_dir = tempfile::tempdir()?;
    let account_dir = test_dir.path().join("test_account");
    std::fs::create_dir_all(&account_dir)?;

    println!("Testing logging in: {}", account_dir.display());

    // Test logging initialization
    logging::init_account_logging(&account_dir, "@test:example.org")?;

    // Write some test logs
    tracing::info!("Test info message");
    tracing::debug!("Test debug message from SDK");
    tracing::warn!("Test warning message");

    // Give it a moment to flush
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Check if log file was created
    let log_file = account_dir.join("sdk_logs/sdk.log");
    if log_file.exists() {
        println!("✅ Log file created: {}", log_file.display());
        let contents = std::fs::read_to_string(&log_file)?;
        println!("\n📄 Log contents:");
        println!("{}", "=".repeat(80));
        println!("{}", contents);
        println!("{}", "=".repeat(80));
    } else {
        println!("❌ Log file not created");
    }

    Ok(())
}
