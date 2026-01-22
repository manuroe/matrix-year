/// Reset crawl metadata and SDK data
///
/// This clears the crawl metadata database and SDK caches while preserving credentials.
use anyhow::{Context, Result};
use std::fs;

use crate::account_selector::AccountSelector;

/// Run the reset command
pub async fn run(user_id: Option<String>) -> Result<()> {
    // Select accounts (with multi-select enabled)
    let mut selector = AccountSelector::new()?;
    let accounts = selector.select_accounts(user_id, true)?;

    eprintln!("🔄 Resetting {} account(s)", accounts.len());

    for (account_id, account_dir) in &accounts {
        eprintln!("🧹 Resetting account: {}", account_id);

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
