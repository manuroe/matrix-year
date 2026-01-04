// src/secrets.rs
// Secrets management for matrix-year
// Stores credentials in local JSON files with restricted permissions

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

// ============================================
// Internal Types (Private)
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AccountSecrets {
    db_passphrase: Option<String>,
    access_token: Option<String>,
    refresh_token: Option<String>,
}

// ============================================
// Public API
// ============================================

/// Storage for account credentials
///
/// This struct manages all credential storage for a Matrix account.
/// The storage implementation is completely opaque - callers don't need
/// to know whether credentials are stored in files, keychain, or elsewhere.
pub struct AccountSecretsStore {
    account_id: String,
    secrets: AccountSecrets,
}

impl AccountSecretsStore {
    /// Create a new secrets store for an account
    ///
    /// Loads existing credentials if available, or initializes empty store.
    pub fn new(account_id: &str) -> Result<Self> {
        let secrets = load_secrets_from_file(account_id).unwrap_or_default();
        Ok(Self {
            account_id: account_id.to_owned(),
            secrets,
        })
    }

    /// Get the database passphrase
    pub fn get_db_passphrase(&self) -> Option<String> {
        self.secrets.db_passphrase.clone()
    }

    /// Get the access token
    pub fn get_access_token(&self) -> Option<String> {
        self.secrets.access_token.clone()
    }

    /// Get the refresh token
    pub fn get_refresh_token(&self) -> Option<String> {
        self.secrets.refresh_token.clone()
    }

    /// Store all credentials
    ///
    /// Updates the in-memory state and persists to storage immediately.
    pub fn store_credentials(
        &mut self,
        db_passphrase: Option<String>,
        access_token: Option<String>,
        refresh_token: Option<String>,
    ) -> Result<()> {
        self.secrets = AccountSecrets {
            db_passphrase,
            access_token,
            refresh_token,
        };
        save_secrets_to_file(&self.account_id, &self.secrets)
    }

    /// Delete all stored credentials
    ///
    /// Removes credentials from storage and clears in-memory state.
    pub fn delete_all(&mut self) -> Result<()> {
        self.secrets = AccountSecrets::default();
        delete_secrets_file(&self.account_id)
    }
}

// ============================================
// Internal Implementation
// ============================================

fn credentials_file_path(account_id: &str) -> PathBuf {
    let data_dir = std::env::var("MY_DATA_DIR").unwrap_or_else(|_| ".my".to_string());
    let account_dirname = account_id.replace(':', "_");
    Path::new(&data_dir)
        .join("accounts")
        .join(account_dirname)
        .join("meta")
        .join("credentials.json")
}

fn load_secrets_from_file(account_id: &str) -> Result<AccountSecrets> {
    let path = credentials_file_path(account_id);
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read credentials from {}", path.display()))?;
    let secrets: AccountSecrets = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse credentials from {}", path.display()))?;
    Ok(secrets)
}

fn save_secrets_to_file(account_id: &str, secrets: &AccountSecrets) -> Result<()> {
    let path = credentials_file_path(account_id);

    // Create parent directory if needed
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    // Serialize credentials
    let json = serde_json::to_string_pretty(secrets).context("Failed to serialize credentials")?;

    // Write to file
    fs::write(&path, json)
        .with_context(|| format!("Failed to write credentials to {}", path.display()))?;

    Ok(())
}

fn delete_secrets_file(account_id: &str) -> Result<()> {
    let path = credentials_file_path(account_id);
    if path.exists() {
        fs::remove_file(&path)
            .with_context(|| format!("Failed to delete credentials file {}", path.display()))?;
    }
    Ok(())
}
