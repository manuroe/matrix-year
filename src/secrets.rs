// src/secrets.rs
// Keychain secrets management for matrix-year

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountSecrets {
    pub db_passphrase: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
}

pub const SERVICE_NAME: &str = "my.matrix-year";

pub fn keyring_entry(account_id: &str, key_name: &str) -> Result<keyring::Entry> {
    let account = format!("{}#{}", account_id, key_name);
    keyring::Entry::new(SERVICE_NAME, &account).context("keyring entry")
}

pub fn keyring_get_account_secrets(account_id: &str) -> Result<AccountSecrets> {
    let entry = keyring_entry(account_id, "secrets")?;
    match entry.get_password() {
        Ok(json) => {
            let secrets: AccountSecrets = serde_json::from_str(&json)?;
            Ok(secrets)
        }
        Err(keyring::Error::NoEntry) => {
            eprintln!("[info] No single-entry secrets found, trying per-secret keys...");
            let db_passphrase = keyring_get_secret_uncached(account_id, "db_passphrase")
                .ok()
                .flatten();
            let access_token = keyring_get_secret_uncached(account_id, "access_token")
                .ok()
                .flatten();
            let refresh_token = keyring_get_secret_uncached(account_id, "refresh_token")
                .ok()
                .flatten();
            let secrets = AccountSecrets {
                db_passphrase,
                access_token,
                refresh_token,
            };
            if secrets.db_passphrase.is_some()
                || secrets.access_token.is_some()
                || secrets.refresh_token.is_some()
            {
                let _ = keyring_set_account_secrets(account_id, &secrets);
            }
            Ok(secrets)
        }
        Err(e) => Err(anyhow!(e)),
    }
}

pub fn keyring_set_account_secrets(account_id: &str, secrets: &AccountSecrets) -> Result<()> {
    let entry = keyring_entry(account_id, "secrets")?;
    let json = serde_json::to_string(secrets)?;
    entry.set_password(&json).map_err(|e| anyhow!(e))
}

pub fn keyring_get_secret_uncached(account_id: &str, key_name: &str) -> Result<Option<String>> {
    let entry = keyring_entry(account_id, key_name)?;
    match entry.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(anyhow!(e)),
    }
}

#[derive(Default)]
pub struct SecretsCache {
    map: HashMap<String, AccountSecrets>,
}

impl SecretsCache {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn get_account_secrets(&mut self, account_id: &str) -> Result<&AccountSecrets> {
        if !self.map.contains_key(account_id) {
            let secrets = keyring_get_account_secrets(account_id)?;
            self.map.insert(account_id.to_owned(), secrets);
        }
        Ok(self.map.get(account_id).expect("secrets must be present"))
    }

    pub fn get_db_passphrase(&mut self, account_id: &str) -> Result<Option<String>> {
        Ok(self.get_account_secrets(account_id)?.db_passphrase.clone())
    }
    pub fn get_access_token(&mut self, account_id: &str) -> Result<Option<String>> {
        Ok(self.get_account_secrets(account_id)?.access_token.clone())
    }
    pub fn get_refresh_token(&mut self, account_id: &str) -> Result<Option<String>> {
        Ok(self.get_account_secrets(account_id)?.refresh_token.clone())
    }
}

/// Delete all secrets (single-entry and legacy per-secret) for an account from the keychain.
pub fn keyring_delete_all_secrets(account_id: &str) -> Result<()> {
    let mut errors = Vec::new();
    // Try to delete the single-entry blob
    if let Ok(entry) = keyring_entry(account_id, "secrets") {
        if let Err(e) = entry.delete_credential() {
            if !matches!(e, keyring::Error::NoEntry) {
                errors.push(e);
            }
        }
    }
    // Try to delete legacy per-secret keys
    for key in ["db_passphrase", "access_token", "refresh_token"] {
        if let Ok(entry) = keyring_entry(account_id, key) {
            if let Err(e) = entry.delete_credential() {
                if !matches!(e, keyring::Error::NoEntry) {
                    errors.push(e);
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(anyhow!("Failed to delete some secrets: {:?}", errors))
    }
}
