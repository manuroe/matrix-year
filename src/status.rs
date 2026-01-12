use crate::login::{account_id_to_dirname, resolve_data_root};
use anyhow::Result;
use std::fs;
use std::path::Path;

/// Status of an account's files and credentials
pub struct AccountStatus {
    pub session_exists: bool,
    pub credentials_exists: bool,
    pub db_passphrase_exists: bool,
    pub access_token_exists: bool,
    pub cross_signing_status: String,
}

/// Check the complete status of an account (files, credentials, verification).
pub async fn check_account_status(account_dir: &Path, account_id: &str) -> Result<AccountStatus> {
    let meta_dir = account_dir.join("meta");
    let session_path = meta_dir.join("session.json");
    let cred_path = meta_dir.join("credentials.json");

    let session_exists = session_path.exists();
    let credentials_exists = cred_path.exists();

    let (db_passphrase_exists, access_token_exists, cross_signing_status) =
        if let Ok(secrets_store) = crate::secrets::AccountSecretsStore::new(account_id) {
            let db = secrets_store.get_db_passphrase().is_some();
            let access = secrets_store.get_access_token().is_some();

            let xsign_status = check_cross_signing_status(account_dir, account_id, &secrets_store)
                .await
                .unwrap_or_else(|_| "⚠ Unable to check".to_string());

            (db, access, xsign_status)
        } else {
            (false, false, "⚠ Unable to check".to_string())
        };

    Ok(AccountStatus {
        session_exists,
        credentials_exists,
        db_passphrase_exists,
        access_token_exists,
        cross_signing_status,
    })
}

/// Check the verification state of an account's cross-signing setup.
pub async fn check_verification_state(
    account_dir: &std::path::Path,
    account_id: &str,
) -> Result<matrix_sdk::encryption::VerificationState> {
    // Check if the SDK database exists
    let sdk_store_dir = account_dir.join("sdk");
    if !sdk_store_dir.exists() {
        anyhow::bail!("SDK database not initialized");
    }

    // Try to check cross-signing by restoring session
    let session_path = account_dir.join("meta/session.json");
    if !session_path.exists() {
        anyhow::bail!("Session metadata not found");
    }

    // Restore the client for this account
    let client = crate::sdk::restore_client_for_account(account_dir, account_id).await?;

    // Perform minimal sync to update verification_state
    crate::sdk::sync_encryption_state(&client).await?;

    // Check if cross-signing is set up on the account
    let xsign_enabled = client
        .encryption()
        .secret_storage()
        .is_enabled()
        .await
        .unwrap_or(false);

    if !xsign_enabled {
        anyhow::bail!("Cross-signing not set up");
    }

    // Return the account-level verification state (cross-signing status)
    Ok(client.encryption().verification_state().get())
}

pub async fn run(user_id_flag: Option<String>) -> Result<()> {
    let data_root = resolve_data_root()?;
    let accounts_root = data_root.join("accounts");
    if !accounts_root.exists() {
        println!("No accounts found.");
        return Ok(());
    }

    let mut accounts = Vec::new();
    if let Some(uid) = user_id_flag {
        accounts.push(uid);
    } else {
        for entry in fs::read_dir(&accounts_root)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let dirname = entry.file_name().to_string_lossy().to_string();
                let uid = dirname.replace('_', ":");
                accounts.push(uid);
            }
        }
    }

    if accounts.is_empty() {
        println!("No accounts found.");
        return Ok(());
    }

    for account_id in &accounts {
        let account_dir = accounts_root.join(account_id_to_dirname(account_id));
        println!("\nAccount: {}", account_id);
        if !account_dir.exists() {
            println!("  [!] Account directory missing: {}", account_dir.display());
            continue;
        }
        println!("  Directory: {}", account_dir.display());

        // Use check_account_status to get all status info
        match check_account_status(&account_dir, account_id).await {
            Ok(status) => {
                println!(
                    "  meta/session.json: {}",
                    if status.session_exists {
                        "OK"
                    } else {
                        "MISSING"
                    }
                );
                println!(
                    "  meta/credentials.json: {}",
                    if status.credentials_exists {
                        "OK"
                    } else {
                        "MISSING"
                    }
                );
                println!(
                    "  Credentials: db_passphrase: {}",
                    if status.db_passphrase_exists {
                        "OK"
                    } else {
                        "MISSING"
                    }
                );
                println!(
                    "               access_token: {}",
                    if status.access_token_exists {
                        "OK"
                    } else {
                        "MISSING"
                    }
                );
                println!("  Cross-signing: {}", status.cross_signing_status);
            }
            Err(_) => {
                // Fallback: just check files exist
                let meta_dir = account_dir.join("meta");
                let session_path = meta_dir.join("session.json");
                let cred_path = meta_dir.join("credentials.json");
                println!(
                    "  meta/session.json: {}",
                    if session_path.exists() {
                        "OK"
                    } else {
                        "MISSING"
                    }
                );
                println!(
                    "  meta/credentials.json: {}",
                    if cred_path.exists() { "OK" } else { "MISSING" }
                );
                println!("  Credentials: ERROR (failed to load)");
            }
        }

        // Display SDK coverage stats
        match crate::crawl_db::CrawlDb::init(&account_dir) {
            Ok(db) => {
                match db.room_count() {
                    Ok(count) => {
                        println!("  Crawled rooms: {}", count);
                    }
                    Err(e) => {
                        eprintln!("  Error reading room count: {}", e);
                    }
                }

                match db.fully_crawled_room_count() {
                    Ok(count) => {
                        println!("  Fully crawled rooms: {}", count);
                    }
                    Err(e) => {
                        eprintln!("  Error reading fully crawled count: {}", e);
                    }
                }

                match db.get_time_window() {
                    Ok(Some((start, end, account_creation))) => {
                        let start_str = match start {
                            None => {
                                // All rooms fully crawled, use account creation time
                                account_creation
                                    .map(|ts| {
                                        format!("account creation [{}]", format_timestamp(ts))
                                    })
                                    .unwrap_or_else(|| "account creation [unknown]".to_string())
                            }
                            Some(ts) => format_timestamp(ts),
                        };
                        let end_str = end
                            .map(format_timestamp)
                            .unwrap_or_else(|| "unknown".to_string());
                        println!("  Data window: {} to {}", start_str, end_str);
                    }
                    Ok(None) => {
                        println!("  Data window: (no data crawled)");
                    }
                    Err(e) => {
                        eprintln!("  Error reading time window: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("  Error loading crawl database: {}", e);
            }
        }
    }
    Ok(())
}

async fn check_cross_signing_status(
    account_dir: &std::path::Path,
    account_id: &str,
    _secrets_store: &crate::secrets::AccountSecretsStore,
) -> Result<String> {
    // Check if the SDK database exists
    let sdk_store_dir = account_dir.join("sdk");
    if !sdk_store_dir.exists() {
        return Ok("Not initialized".to_string());
    }

    // Try to check cross-signing by restoring session
    let session_path = account_dir.join("meta/session.json");
    if !session_path.exists() {
        return Ok("Unable to check (no session)".to_string());
    }

    // Use the reusable check_verification_state function
    let verification_state = check_verification_state(account_dir, account_id)
        .await
        .unwrap_or(matrix_sdk::encryption::VerificationState::Unknown);

    // Determine status based on account-level cross-signing verification
    match verification_state {
        matrix_sdk::encryption::VerificationState::Verified => Ok("✓ Device verified".to_string()),
        matrix_sdk::encryption::VerificationState::Unverified => {
            Ok("✗ Device not verified".to_string())
        }
        matrix_sdk::encryption::VerificationState::Unknown => {
            Ok("⚠ Device verification unknown".to_string())
        }
    }
}

fn format_timestamp(ts_millis: i64) -> String {
    use std::time::UNIX_EPOCH;
    let duration = std::time::Duration::from_millis(ts_millis as u64);
    let system_time = UNIX_EPOCH + duration;
    match system_time.duration_since(UNIX_EPOCH) {
        Ok(_) => {
            let datetime: chrono::DateTime<chrono::Utc> = system_time.into();
            datetime.format("%Y-%m-%d %H:%M:%S").to_string()
        }
        Err(_) => "invalid timestamp".to_string(),
    }
}
