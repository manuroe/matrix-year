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
    /// Logs a warning if credentials file exists but cannot be loaded.
    pub fn new(account_id: &str) -> Result<Self> {
        let secrets = match load_secrets_from_file(account_id) {
            Ok(s) => s,
            Err(e) => {
                let path = credentials_file_path(account_id);
                if path.exists() {
                    eprintln!(
                        "Warning: credentials file exists at {} but could not be loaded: {}",
                        path.display(),
                        e
                    );
                }
                AccountSecrets::default()
            }
        };
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
    let account_dirname = crate::login::account_id_to_dirname(account_id);
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

    // Set restrictive permissions (0600 - owner read/write only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&path)?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&path, perms)
            .with_context(|| format!("Failed to set permissions on {}", path.display()))?;
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::SystemTime;

    // Use a mutex to ensure tests don't run in parallel and interfere with each other
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn test_account_id() -> String {
        "@testuser:example.org".to_string()
    }

    fn setup_test_env() -> String {
        // Create a unique temp directory for each test
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let test_id = format!("test-secrets-{}", nanos);
        let test_dir = std::env::temp_dir().join(test_id);
        std::env::set_var("MY_DATA_DIR", test_dir.to_str().unwrap());
        test_dir.to_string_lossy().to_string()
    }

    fn cleanup_test_env(test_dir: &str) {
        let _ = fs::remove_dir_all(test_dir);
        std::env::remove_var("MY_DATA_DIR");
    }

    #[test]
    fn test_new_store_creates_empty_when_no_file() {
        let _lock = TEST_LOCK.lock().unwrap();
        let test_dir = setup_test_env();
        let account_id = test_account_id();

        let store = AccountSecretsStore::new(&account_id).unwrap();

        assert_eq!(store.get_db_passphrase(), None);
        assert_eq!(store.get_access_token(), None);
        assert_eq!(store.get_refresh_token(), None);

        cleanup_test_env(&test_dir);
    }

    #[test]
    fn test_store_and_retrieve_credentials() {
        let _lock = TEST_LOCK.lock().unwrap();
        let test_dir = setup_test_env();
        let account_id = test_account_id();

        let mut store = AccountSecretsStore::new(&account_id).unwrap();
        store
            .store_credentials(
                Some("test-passphrase".to_string()),
                Some("test-access-token".to_string()),
                Some("test-refresh-token".to_string()),
            )
            .unwrap();

        // Verify in-memory state
        assert_eq!(
            store.get_db_passphrase(),
            Some("test-passphrase".to_string())
        );
        assert_eq!(
            store.get_access_token(),
            Some("test-access-token".to_string())
        );
        assert_eq!(
            store.get_refresh_token(),
            Some("test-refresh-token".to_string())
        );

        // Create a new store instance to verify persistence
        let store2 = AccountSecretsStore::new(&account_id).unwrap();
        assert_eq!(
            store2.get_db_passphrase(),
            Some("test-passphrase".to_string())
        );
        assert_eq!(
            store2.get_access_token(),
            Some("test-access-token".to_string())
        );
        assert_eq!(
            store2.get_refresh_token(),
            Some("test-refresh-token".to_string())
        );

        cleanup_test_env(&test_dir);
    }

    #[test]
    fn test_delete_credentials() {
        let _lock = TEST_LOCK.lock().unwrap();
        let test_dir = setup_test_env();
        let account_id = test_account_id();

        let mut store = AccountSecretsStore::new(&account_id).unwrap();
        store
            .store_credentials(
                Some("test-passphrase".to_string()),
                Some("test-access-token".to_string()),
                None,
            )
            .unwrap();

        // Verify credentials were stored
        assert!(store.get_db_passphrase().is_some());

        // Delete credentials
        store.delete_all().unwrap();

        // Verify in-memory state is cleared
        assert_eq!(store.get_db_passphrase(), None);
        assert_eq!(store.get_access_token(), None);
        assert_eq!(store.get_refresh_token(), None);

        // Verify file is deleted
        let path = credentials_file_path(&account_id);
        assert!(!path.exists());

        cleanup_test_env(&test_dir);
    }

    #[test]
    fn test_partial_credentials() {
        let _lock = TEST_LOCK.lock().unwrap();
        let test_dir = setup_test_env();
        let account_id = test_account_id();

        let mut store = AccountSecretsStore::new(&account_id).unwrap();
        store
            .store_credentials(Some("passphrase".to_string()), None, None)
            .unwrap();

        assert_eq!(store.get_db_passphrase(), Some("passphrase".to_string()));
        assert_eq!(store.get_access_token(), None);
        assert_eq!(store.get_refresh_token(), None);

        cleanup_test_env(&test_dir);
    }

    #[test]
    fn test_corrupted_file_handling() {
        let _lock = TEST_LOCK.lock().unwrap();
        let test_dir = setup_test_env();
        let account_id = test_account_id();

        // Create a corrupted credentials file
        let path = credentials_file_path(&account_id);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "not valid json").unwrap();

        // Should log warning but not fail
        let store = AccountSecretsStore::new(&account_id).unwrap();

        // Should have empty credentials
        assert_eq!(store.get_db_passphrase(), None);
        assert_eq!(store.get_access_token(), None);

        cleanup_test_env(&test_dir);
    }

    #[test]
    fn test_update_credentials() {
        let _lock = TEST_LOCK.lock().unwrap();
        let test_dir = setup_test_env();
        let account_id = test_account_id();

        let mut store = AccountSecretsStore::new(&account_id).unwrap();
        store
            .store_credentials(
                Some("passphrase1".to_string()),
                Some("token1".to_string()),
                None,
            )
            .unwrap();

        // Update with new values
        store
            .store_credentials(
                Some("passphrase2".to_string()),
                Some("token2".to_string()),
                Some("refresh2".to_string()),
            )
            .unwrap();

        assert_eq!(store.get_db_passphrase(), Some("passphrase2".to_string()));
        assert_eq!(store.get_access_token(), Some("token2".to_string()));
        assert_eq!(store.get_refresh_token(), Some("refresh2".to_string()));

        cleanup_test_env(&test_dir);
    }

    #[test]
    #[cfg(unix)]
    fn test_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let _lock = TEST_LOCK.lock().unwrap();
        let test_dir = setup_test_env();
        let account_id = test_account_id();

        let mut store = AccountSecretsStore::new(&account_id).unwrap();
        store
            .store_credentials(
                Some("passphrase".to_string()),
                Some("token".to_string()),
                None,
            )
            .unwrap();

        let path = credentials_file_path(&account_id);
        let metadata = fs::metadata(&path).unwrap();
        let perms = metadata.permissions();

        // Verify permissions are 0600 (owner read/write only)
        // 0o600 in octal is 384 in decimal
        assert_eq!(perms.mode() & 0o777, 0o600, "File permissions should be 0600 (owner read/write only)");

        cleanup_test_env(&test_dir);
    }

    #[test]
    fn test_account_id_to_dirname_usage() {
        let _lock = TEST_LOCK.lock().unwrap();
        let test_dir = setup_test_env();
        let account_id = "@user:example.org";

        let path = credentials_file_path(account_id);

        // Verify the path uses account_id_to_dirname (replaces : with _)
        assert!(path.to_string_lossy().contains("@user_example.org"));

        cleanup_test_env(&test_dir);
    }
}
