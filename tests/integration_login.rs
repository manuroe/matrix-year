use anyhow::{Context, Result};
use std::env;
use url::Url;

/// Integration test for login with cross-signing verification
///
/// This test uses MY_DATA_DIR environment variable to isolate test data
/// in a temporary directory, avoiding pollution of the default ~/.my directory.
///
/// Required environment variables:
/// - MY_TEST_HOMESERVER: Matrix homeserver URL (e.g., https://matrix.org)
/// - MY_TEST_USER_ID: Full Matrix user ID (e.g., @testuser:example.org)
/// - MY_TEST_PASSWORD: Account password
/// - MY_TEST_RECOVERY_KEY: Recovery key for cross-signing verification
///
/// Setup:
///   1. Copy .env.template to .env
///   2. Fill in your test credentials
///   3. Run: (set -a && source .env && set +a && cargo test --test integration_login -- --ignored --nocapture)
///
/// The test automatically sets MY_DATA_DIR to a temporary directory.
///
/// For GitHub Actions, these should be set as repository secrets.
#[tokio::test]
#[ignore]
async fn test_login_with_cross_signing() -> Result<()> {
    // Read credentials from environment variables
    let homeserver = env::var("MY_TEST_HOMESERVER")
        .context("MY_TEST_HOMESERVER environment variable not set")?;
    let user_id =
        env::var("MY_TEST_USER_ID").context("MY_TEST_USER_ID environment variable not set")?;
    let password =
        env::var("MY_TEST_PASSWORD").context("MY_TEST_PASSWORD environment variable not set")?;
    let recovery_key = env::var("MY_TEST_RECOVERY_KEY")
        .context("MY_TEST_RECOVERY_KEY environment variable not set")?;

    // Use a temporary directory for this test via MY_DATA_DIR environment variable
    let temp_dir = tempfile::tempdir()?;
    env::set_var("MY_DATA_DIR", temp_dir.path());
    let accounts_root = temp_dir.path().join("accounts");

    println!("Testing login for user: {}", user_id);
    println!("Using accounts directory: {}", accounts_root.display());

    // Step 1: Login with credentials (stores credentials in secrets store)
    // Extract hostname from homeserver URL using proper URL parsing
    let homeserver_host = Url::parse(&homeserver)
        .context("Invalid homeserver URL")?
        .host_str()
        .context("No host in homeserver URL")?
        .to_string();

    let (client, actual_user_id, _restored) =
        my::login::login_with_credentials(&homeserver_host, &user_id, &password, &accounts_root)
            .await
            .context("Login failed")?;

    println!("✓ Login successful for: {}", actual_user_id);

    // Normalize user ID for comparison (handle both full ID and username)
    let expected_user_id = if user_id.starts_with('@') && user_id.contains(':') {
        user_id.clone()
    } else {
        // Extract domain from homeserver URL
        let domain = homeserver
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or("matrix.org");
        format!("@{}:{}", user_id, domain)
    };
    assert_eq!(expected_user_id, actual_user_id, "User ID mismatch");

    // Step 2: Initialize encryption
    my::login::initialize_encryption(&client)
        .await
        .context("Failed to initialize encryption")?;
    println!("✓ Encryption initialized");

    // Step 3: Verify device with recovery key
    my::login::verify_with_recovery_key(&client, &recovery_key)
        .await
        .context("Failed to verify device with recovery key")?;
    println!("✓ Device verified with recovery key");

    // Verify the device is actually verified
    let own_device = client
        .encryption()
        .get_own_device()
        .await
        .context("Failed to get own device")?;

    if let Some(device) = own_device {
        assert!(
            device.is_verified(),
            "Device should be verified after recovery key import"
        );
    } else {
        anyhow::bail!("Could not get own device information");
    }

    // Step 4: Verify encryption state persists in account directory
    // This tests that the SDK database with encryption keys was properly stored
    // by reusing the status module's check_account_status function
    let account_dir = accounts_root.join(my::login::account_id_to_dirname(&actual_user_id));

    let status = my::status::check_account_status(&account_dir, &actual_user_id)
        .await
        .context("Failed to check account status")?;

    assert!(status.session_exists, "Session file should exist");
    println!("  meta/session.json: OK");

    assert!(
        status.db_passphrase_exists,
        "db_passphrase should be stored"
    );
    println!("  Credentials: db_passphrase: OK");

    assert!(status.access_token_exists, "access_token should be stored");
    println!("               access_token: OK");

    assert_eq!(
        &status.cross_signing_status, "✓ Device verified",
        "Cross-signing should show as verified"
    );
    println!("  Cross-signing: {}", status.cross_signing_status);

    // Step 5: Test logout
    my::logout::logout(accounts_root.clone(), &actual_user_id)
        .await
        .context("Failed to logout")?;
    println!("✓ Logout successful");

    // Verify account directory is removed
    assert!(
        !account_dir.exists(),
        "Account directory should be removed after logout"
    );

    println!("✓ Test completed successfully");

    Ok(())
}
