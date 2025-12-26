use crate::login::{account_id_to_dirname, resolve_data_root};
use crate::secrets::SecretsCache;
use anyhow::Result;
use std::fs;

pub fn run(user_id_flag: Option<String>) -> Result<()> {
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

    // Use a per-execution cache for secrets
    let mut secrets_cache = SecretsCache::new();
    for account_id in &accounts {
        let account_dir = accounts_root.join(account_id_to_dirname(account_id));
        println!("\nAccount: {}", account_id);
        if !account_dir.exists() {
            println!("  [!] Account directory missing: {}", account_dir.display());
            continue;
        }
        let meta_dir = account_dir.join("meta");
        let session_path = meta_dir.join("session.json");
        let cred_path = meta_dir.join("credentials.json");
        println!("  Directory: {}", account_dir.display());
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

        // Check keychain secrets using the new cache
        let db = secrets_cache.get_db_passphrase(account_id).ok().flatten();
        let access = secrets_cache.get_access_token(account_id).ok().flatten();
        let refresh = secrets_cache.get_refresh_token(account_id).ok().flatten();
        println!(
            "  Keychain: db_passphrase: {}",
            if db.is_some() { "OK" } else { "MISSING" }
        );
        println!(
            "            access_token: {}",
            if access.is_some() { "OK" } else { "MISSING" }
        );
        println!(
            "            refresh_token: {}",
            if refresh.is_some() { "OK" } else { "MISSING" }
        );
    }
    Ok(())
}
