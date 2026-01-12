/// Reset crawl metadata and SDK data
///
/// This clears the crawl metadata database and SDK caches while preserving credentials.
use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

/// Run the reset command
pub async fn run(user_id: Option<String>) -> Result<()> {
    let data_dir = crate::login::resolve_data_root()?;
    let accounts_dir = data_dir.join("accounts");

    if !accounts_dir.exists() {
        eprintln!("No accounts found");
        return Ok(());
    }

    // Get list of accounts to reset
    let accounts = if let Some(ref uid) = user_id {
        vec![PathBuf::from(crate::login::account_id_to_dirname(uid))]
    } else {
        // Reset all accounts
        fs::read_dir(&accounts_dir)
            .context("Failed to read accounts directory")?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                if entry.file_type().ok()?.is_dir() {
                    Some(entry.file_name().into())
                } else {
                    None
                }
            })
            .collect()
    };

    if accounts.is_empty() {
        eprintln!("No accounts to reset");
        return Ok(());
    }

    eprintln!("🔄 Resetting {} account(s)", accounts.len());

    for account_dirname in accounts {
        let account_dir = accounts_dir.join(&account_dirname);

        if !account_dir.exists() {
            eprintln!("⚠️  Account not found: {}", account_dirname.display());
            continue;
        }

        eprintln!("🧹 Resetting account: {}", account_dirname.display());

        // 1. Remove crawl metadata database
        let db_path = account_dir.join("db.sqlite");
        if db_path.exists() {
            fs::remove_file(&db_path)
                .with_context(|| format!("Failed to remove {}", db_path.display()))?;
            eprintln!("  ✓ Removed crawl metadata database");
        }

        // 2. Remove SDK directory (contains event cache, crypto store, etc.)
        // This clears all SDK data while preserving credentials in meta/
        let sdk_dir = account_dir.join("sdk");
        if sdk_dir.exists() {
            fs::remove_dir_all(&sdk_dir).with_context(|| {
                format!("Failed to remove SDK directory at {}", sdk_dir.display())
            })?;
            eprintln!("  ✓ Removed SDK data (event cache, crypto store)");
        }

        eprintln!("  ✅ Reset complete");
    }

    eprintln!("✅ Reset complete for all accounts");
    Ok(())
}
