use crate::login::{account_id_to_dirname, resolve_data_root};
use crate::sdk::restore_client_for_account;
use crate::timefmt::format_timestamp;
use anyhow::{Context, Result};
use matrix_sdk::Client;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use unicode_width::UnicodeWidthStr;

/// Gets the status symbol for a room based on its crawl metadata.
///
/// Returns a single Unicode character representing the crawl status:
/// - `○` for virgin (never crawled)
/// - `✓` for success
/// - `⠧` for in-progress
/// - `✗` for error
/// - `?` for unknown/null status (should not occur in normal usage)
fn get_status_symbol(metadata: &crate::crawl_db::RoomCrawlMetadata) -> &'static str {
    use crate::crawl_db::CrawlStatus;

    match &metadata.last_crawl_status {
        Some(CrawlStatus::Virgin) => "○",
        Some(CrawlStatus::Success) => "✓",
        Some(CrawlStatus::InProgress) => "⠧",
        Some(CrawlStatus::Error(_)) => "✗",
        None => "?", // Unknown/null status
    }
}

/// Gets display names for all rooms from the Matrix client.
///
/// Returns a HashMap mapping room IDs to their display names. Falls back to
/// the room ID itself if the display name is unavailable or the room is not
/// found in the client's room list.
///
/// # Arguments
/// * `client` - Matrix client instance with loaded room cache
/// * `rooms_metadata` - Slice of room metadata containing room IDs to look up
///
/// # Returns
/// HashMap mapping room_id strings to display names (defaults to room_id if unavailable)
async fn get_room_names(
    client: &Client,
    rooms_metadata: &[crate::crawl_db::RoomCrawlMetadata],
) -> HashMap<String, String> {
    let mut room_names = HashMap::new();

    for metadata in rooms_metadata {
        // Parse room ID string into RoomId type
        match metadata.room_id.as_str().try_into() {
            Ok(room_id) => {
                if let Some(room) = client.get_room(room_id) {
                    let name = room
                        .display_name()
                        .await
                        .ok()
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| metadata.room_id.clone());
                    room_names.insert(metadata.room_id.clone(), name);
                } else {
                    room_names.insert(metadata.room_id.clone(), metadata.room_id.clone());
                }
            }
            Err(_) => {
                // Invalid room ID, use the string as-is
                room_names.insert(metadata.room_id.clone(), metadata.room_id.clone());
            }
        }
    }

    room_names
}

/// Lists all rooms with their crawl metadata
pub async fn list_rooms(account_id: &str) -> Result<()> {
    // Open the database
    let data_dir = resolve_data_root()?;
    let account_dirname = account_id_to_dirname(account_id);
    let account_dir = data_dir.join("accounts").join(&account_dirname);

    if !account_dir.exists() {
        anyhow::bail!("Account not found: {}", account_id);
    }

    let db = crate::crawl_db::CrawlDb::init(&account_dir)
        .with_context(|| format!("Failed to open crawl database for {}", account_id))?;

    // Get all rooms sorted by status
    let rooms = db
        .get_all_rooms_sorted()
        .context("Failed to retrieve rooms from database")?;

    if rooms.is_empty() {
        eprintln!("No rooms found in database for {}", account_id);
        return Ok(());
    }

    // Build room names map in a scoped block to ensure client is dropped before printing
    let room_names = {
        // Restore client session to get room names
        let client = restore_client_for_account(&account_dir, account_id)
            .await
            .context("Failed to restore Matrix session")?;

        // Build a map of room_id -> display_name
        let names = get_room_names(&client, &rooms).await;

        // Explicitly drop client before continuing
        drop(client);

        names
    };

    eprintln!("Rooms for {}:\n", account_id);

    // Print each room with its status
    for metadata in rooms {
        let status_symbol = get_status_symbol(&metadata);
        let room_name = room_names
            .get(&metadata.room_id)
            .map(|s| s.as_str())
            .unwrap_or(&metadata.room_id);

        // Format room info with proper alignment
        let truncated_name = truncate_middle(room_name, 40);
        let creation_marker = if metadata.fully_crawled { " 💯" } else { "" };

        if let Some(oldest) = metadata.oldest_event_ts {
            let oldest_str = crate::timefmt::format_timestamp_opt(Some(oldest));
            // Use character-based slicing for UTF-8 safety with Unicode timestamps
            let oldest_short: String = oldest_str.chars().take(16).collect();

            let user_events_str = if metadata.user_events_fetched > 0 {
                format!(" ({} from you)", metadata.user_events_fetched)
            } else {
                String::new()
            };

            eprintln!(
                "  {} {} {:>5} events from {}{}{}",
                status_symbol,
                truncated_name,
                metadata.total_events_fetched,
                &oldest_short,
                user_events_str,
                creation_marker
            );
        } else {
            eprintln!("  {} {}", status_symbol, truncated_name);
        }
    }

    Ok(())
}

/// Truncates a string to a maximum display width with middle ellipsis if needed.
///
/// Uses Unicode display width (columns) rather than character count, accounting for
/// emoji (2 columns), CJK characters, and zero-width joiners. Always returns a string
/// padded to exactly `max_width` display columns for proper text alignment.
fn truncate_middle(s: &str, max_width: usize) -> String {
    let display_width = UnicodeWidthStr::width(s);

    if display_width <= max_width {
        // Pad to max_width for alignment
        let padding = max_width - display_width;
        format!("{}{}", s, " ".repeat(padding))
    } else {
        // Truncate with middle ellipsis
        let ellipsis = "…";
        let ellipsis_width = UnicodeWidthStr::width(ellipsis);

        if max_width <= ellipsis_width {
            // Edge case: max_width too small, just truncate from start
            let mut truncated = String::new();
            let mut current_width = 0;
            for ch in s.chars() {
                let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                if current_width + ch_width > max_width {
                    break;
                }
                truncated.push(ch);
                current_width += ch_width;
            }
            let padding = max_width - current_width;
            format!("{}{}", truncated, " ".repeat(padding))
        } else {
            let available = max_width - ellipsis_width;
            let start_width = available.div_ceil(2);
            let end_width = available / 2;

            // Collect start characters up to start_width display columns
            let mut start = String::new();
            let mut current_width = 0;
            for ch in s.chars() {
                let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                if current_width + ch_width > start_width {
                    break;
                }
                start.push(ch);
                current_width += ch_width;
            }

            // Collect end characters up to end_width display columns (from the end)
            let mut end_chars = Vec::new();
            let mut current_width = 0;
            for ch in s.chars().rev() {
                let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                if current_width + ch_width > end_width {
                    break;
                }
                end_chars.push(ch);
                current_width += ch_width;
            }
            end_chars.reverse();
            let end: String = end_chars.into_iter().collect();

            // Build truncated string and pad to exactly max_width display columns
            let mut result = format!("{}{}{}", start, ellipsis, end);
            let result_width = UnicodeWidthStr::width(result.as_str());
            if result_width < max_width {
                let padding = max_width - result_width;
                result.push_str(&" ".repeat(padding));
            }
            result
        }
    }
}

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

pub async fn run(user_id_flag: Option<String>, list: bool) -> Result<()> {
    // Get data root first
    let data_root = resolve_data_root()?;
    let accounts_root = data_root.join("accounts");
    if !accounts_root.exists() {
        println!("No accounts found.");
        return Ok(());
    }

    // If --list is requested, show room listing instead of status
    if list {
        // Determine which accounts to list rooms for
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

        // List rooms for each account
        for account_id in &accounts {
            if accounts.len() > 1 {
                println!("\nAccount: {}", account_id);
            }
            list_rooms(account_id).await?;

            // Add a small delay between accounts to allow SDK cleanup
            if accounts.len() > 1 {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        }
        return Ok(());
    }

    // Otherwise, show normal status

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
                    Ok(Some(window)) => {
                        let start_str = match window.window_start {
                            None => {
                                // All rooms fully crawled, use account creation time
                                window
                                    .account_creation_ts
                                    .map(|ts| {
                                        format!("account creation [{}]", format_timestamp(ts))
                                    })
                                    .unwrap_or_else(|| "account creation [unknown]".to_string())
                            }
                            Some(ts) => format_timestamp(ts),
                        };
                        let end_str = window
                            .window_end
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
