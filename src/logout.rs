use crate::secrets::SecretsCache;
use anyhow::{Context, Result};
use inquire::MultiSelect;
use matrix_sdk::authentication::matrix::MatrixSession;
use matrix_sdk::authentication::SessionTokens;
use matrix_sdk::ruma::{OwnedDeviceId, OwnedUserId, UserId};
use matrix_sdk::SessionMeta;
use matrix_sdk::{AuthSession, Client};
use std::fs;
use std::path::Path;

use crate::login::{account_id_to_dirname, prompt, resolve_data_root, SessionMetaFile};
use crate::secrets::keyring_delete_all_secrets;

pub async fn run(user_id_flag: Option<String>) -> Result<()> {
    let data_root = resolve_data_root()?;
    let accounts_root = data_root.join("accounts");

    if !accounts_root.exists() {
        eprintln!("No accounts found.");
        return Ok(());
    }

    // List existing accounts
    let mut existing_accounts = Vec::new();
    for entry in fs::read_dir(&accounts_root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let dirname = entry.file_name().to_string_lossy().to_string();
            let uid = dirname.replace('_', ":");
            existing_accounts.push(uid);
        }
    }

    if existing_accounts.is_empty() {
        eprintln!("No accounts found.");
        return Ok(());
    }

    // Determine which account(s) to logout
    let accounts_to_remove = match user_id_flag {
        Some(uid) => vec![uid],
        None => {
            if existing_accounts.len() == 1 {
                // Single account: confirm and proceed
                existing_accounts.clone()
            } else {
                // Multiple accounts: offer interactive checkbox selection
                let selected = MultiSelect::new(
                    "Select accounts to logout (use Space to select, Enter to confirm):",
                    existing_accounts.clone(),
                )
                .with_all_selected_by_default()
                .prompt()?;

                if selected.is_empty() {
                    anyhow::bail!("No accounts selected");
                }
                selected
            }
        }
    };

    // Confirm before proceeding
    if accounts_to_remove.len() == 1 {
        eprintln!("You are about to logout from: {}", accounts_to_remove[0]);
    } else {
        eprintln!(
            "You are about to logout from {} accounts:",
            accounts_to_remove.len()
        );
        for account in &accounts_to_remove {
            eprintln!("  - {}", account);
        }
    }
    let confirm = prompt("Proceed? [y/N]: ")?;
    if !matches!(confirm.trim(), "y" | "Y") {
        eprintln!("Logout cancelled.");
        return Ok(());
    }

    // Use a per-execution cache for secrets
    let mut secrets_cache = SecretsCache::new();
    // Logout from homeserver and remove local data for each account
    for account_id in &accounts_to_remove {
        let account_dir = accounts_root.join(account_id_to_dirname(account_id));

        // Try to logout from the homeserver first
        if let Err(e) = logout_from_homeserver(account_id, &account_dir, &mut secrets_cache).await {
            eprintln!(
                "Warning: Failed to logout from homeserver for {}:",
                account_id
            );
            eprintln!("  → {:#}", e);
            eprintln!("Continuing with local cleanup...");
        }

        // Remove all secrets from keychain (single-entry and legacy)
        match keyring_delete_all_secrets(account_id) {
            Ok(_) => eprintln!(
                "[info] Deleted all secrets from keychain for {}",
                account_id
            ),
            Err(e) => eprintln!(
                "[warn] Failed to delete all secrets from keychain for {}: {:#}",
                account_id, e
            ),
        }

        // Remove account directory (includes SDK database and all local data)
        if account_dir.exists() {
            fs::remove_dir_all(&account_dir)
                .with_context(|| format!("Failed to remove account data for {}", account_id))?;
        }

        eprintln!("Logged out: {}", account_id);
    }

    Ok(())
}

async fn logout_from_homeserver(
    account_id: &str,
    account_dir: &Path,
    secrets_cache: &mut SecretsCache,
) -> Result<()> {
    let sdk_store_dir = account_dir.join("sdk");
    let meta_path = account_dir.join("meta/session.json");

    // Load passphrase using cache
    let passphrase = secrets_cache
        .get_db_passphrase(account_id)?
        .context("No passphrase found for session restore")?;

    // Read homeserver URL from session.json
    let meta_bytes = std::fs::read(&meta_path)?;
    let meta_file: crate::login::SessionMetaFile = serde_json::from_slice(&meta_bytes)?;
    let url =
        url::Url::parse(&meta_file.homeserver).context("Invalid homeserver URL in session.json")?;

    // Build client with stored passphrase and homeserver URL
    let client = Client::builder()
        .homeserver_url(url)
        .sqlite_store(sdk_store_dir, Some(&passphrase))
        .build()
        .await?;

    // Restore session
    if meta_path.exists() {
        let meta_bytes = fs::read(&meta_path)?;
        let meta_file: SessionMetaFile = serde_json::from_slice(&meta_bytes)?;

        let user_id: OwnedUserId = UserId::parse(&meta_file.user_id)?;
        let device_id: OwnedDeviceId = OwnedDeviceId::from(meta_file.device_id.as_str());

        let access_token = secrets_cache.get_access_token(account_id)?;
        let refresh_token = secrets_cache.get_refresh_token(account_id)?;

        if let Some(access_token) = access_token {
            let session_meta = SessionMeta { user_id, device_id };
            let session_tokens = SessionTokens {
                access_token,
                refresh_token,
            };
            let matrix_session = MatrixSession {
                meta: session_meta,
                tokens: session_tokens,
            };

            client
                .restore_session(AuthSession::Matrix(matrix_session))
                .await?;

            // Now logout from the homeserver
            client.matrix_auth().logout().await?;
        }
    }

    Ok(())
}
