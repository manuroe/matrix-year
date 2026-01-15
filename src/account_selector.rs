use anyhow::{Context, Result};
use inquire::MultiSelect;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::login::{account_id_to_dirname, resolve_data_root};

/// Preferences for account selection, stored globally.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Preferences {
    /// Last selected accounts when multi-selection is enabled
    #[serde(default)]
    pub last_selected_multi: Vec<String>,

    /// Last selected account when multi-selection is disabled
    #[serde(default)]
    pub last_selected_single: Option<String>,
}

impl Preferences {
    /// Load preferences from the global preferences file
    pub fn load() -> Result<Self> {
        let data_root = resolve_data_root()?;
        let global_dir = data_root.join("global");
        let prefs_file = global_dir.join("preferences.json");

        if !prefs_file.exists() {
            return Ok(Self::default());
        }

        let contents =
            fs::read_to_string(&prefs_file).context("Failed to read preferences file")?;
        let prefs: Self =
            serde_json::from_str(&contents).context("Failed to parse preferences file")?;
        Ok(prefs)
    }

    /// Save preferences to the global preferences file
    pub fn save(&self) -> Result<()> {
        let data_root = resolve_data_root()?;
        let global_dir = data_root.join("global");
        fs::create_dir_all(&global_dir).context("Failed to create global directory")?;

        let prefs_file = global_dir.join("preferences.json");
        let contents =
            serde_json::to_string_pretty(self).context("Failed to serialize preferences")?;
        fs::write(&prefs_file, contents).context("Failed to write preferences file")?;
        Ok(())
    }
}

/// Account selector handles account discovery and selection with preference memory.
pub struct AccountSelector {
    preferences: Preferences,
}

impl AccountSelector {
    /// Create a new account selector, loading preferences
    pub fn new() -> Result<Self> {
        let preferences = Preferences::load()?;
        Ok(Self { preferences })
    }

    /// Discover all accounts in the accounts directory.
    /// Returns Vec of (user_id, account_dir) tuples.
    pub fn discover_accounts() -> Result<Vec<(String, PathBuf)>> {
        let data_root = resolve_data_root()?;
        let accounts_root = data_root.join("accounts");

        if !accounts_root.exists() {
            return Ok(Vec::new());
        }

        let mut accounts = Vec::new();
        for entry in fs::read_dir(&accounts_root).context("Failed to read accounts directory")? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let dirname = entry.file_name().to_string_lossy().to_string();
                let uid = dirname.replace('_', ":");
                let account_dir = entry.path();
                accounts.push((uid, account_dir));
            }
        }

        Ok(accounts)
    }

    /// Select accounts based on user_id flag, account count, and preferences.
    /// Returns Vec of (user_id, account_dir) tuples.
    ///
    /// Behavior:
    /// - If user_id_flag is Some, returns that single account (no UI)
    /// - If only one account exists, returns it (no UI)
    /// - If multiple accounts exist:
    ///   - If allow_multi is true: shows multi-select UI with preference pre-selection
    ///   - If allow_multi is false: shows single-select UI or errors if no preference
    pub fn select_accounts(
        &mut self,
        user_id_flag: Option<String>,
        allow_multi: bool,
    ) -> Result<Vec<(String, PathBuf)>> {
        let all_accounts = Self::discover_accounts()?;

        if all_accounts.is_empty() {
            anyhow::bail!("No accounts found. Run 'my login' first.");
        }

        // If user_id is specified, use only that account
        if let Some(uid) = user_id_flag {
            let data_root = resolve_data_root()?;
            let accounts_root = data_root.join("accounts");
            let dirname = account_id_to_dirname(&uid);
            let account_dir = accounts_root.join(dirname);

            if !account_dir.exists() {
                anyhow::bail!("Account not found: {}", uid);
            }

            return Ok(vec![(uid, account_dir)]);
        }

        // If only one account exists, return it without prompting
        if all_accounts.len() == 1 {
            return Ok(all_accounts);
        }

        // Multiple accounts: show interactive selection
        if allow_multi {
            self.select_multi(&all_accounts)
        } else {
            self.select_single(&all_accounts)
        }
    }

    /// Show multi-select UI for choosing multiple accounts
    fn select_multi(
        &mut self,
        all_accounts: &[(String, PathBuf)],
    ) -> Result<Vec<(String, PathBuf)>> {
        let account_ids: Vec<String> = all_accounts.iter().map(|(uid, _)| uid.clone()).collect();

        // Filter saved preferences to only include accounts that still exist
        let last_selected: Vec<String> = self
            .preferences
            .last_selected_multi
            .iter()
            .filter(|uid| account_ids.contains(uid))
            .cloned()
            .collect();

        // Build default indices: either from saved preference or all selected
        let default_indices: Vec<usize> = if last_selected.is_empty() {
            // Default to all selected if no valid saved preference
            (0..account_ids.len()).collect()
        } else {
            // Convert saved account IDs to their indices
            last_selected
                .iter()
                .filter_map(|uid| account_ids.iter().position(|id| id == uid))
                .collect()
        };

        let selected = MultiSelect::new(
            "Select accounts (Space to toggle, Enter to confirm):",
            account_ids.clone(),
        )
        .with_default(&default_indices)
        .prompt()?;

        if selected.is_empty() {
            anyhow::bail!("No accounts selected");
        }

        // Save preference
        self.preferences.last_selected_multi = selected.clone();
        self.preferences.save()?;

        // Build result with account directories
        let result: Vec<(String, PathBuf)> = all_accounts
            .iter()
            .filter(|(uid, _)| selected.contains(uid))
            .cloned()
            .collect();

        Ok(result)
    }

    /// Show single-select UI for choosing one account
    fn select_single(
        &mut self,
        all_accounts: &[(String, PathBuf)],
    ) -> Result<Vec<(String, PathBuf)>> {
        let account_ids: Vec<String> = all_accounts.iter().map(|(uid, _)| uid.clone()).collect();

        // Use saved preference if it exists and is valid
        let default_idx = if let Some(ref last) = self.preferences.last_selected_single {
            account_ids.iter().position(|uid| uid == last)
        } else {
            None
        };

        let selected = if let Some(idx) = default_idx {
            inquire::Select::new("Select account:", account_ids.clone())
                .with_starting_cursor(idx)
                .prompt()?
        } else {
            inquire::Select::new("Select account:", account_ids.clone()).prompt()?
        };

        // Save preference
        self.preferences.last_selected_single = Some(selected.clone());
        self.preferences.save()?;

        // Build result with account directory
        let result: Vec<(String, PathBuf)> = all_accounts
            .iter()
            .filter(|(uid, _)| uid == &selected)
            .cloned()
            .collect();

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::Mutex;
    use tempfile::TempDir;

    // Mutex to ensure tests that modify environment variables don't run concurrently
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    /// Helper to set up a temporary data directory with test accounts
    fn setup_test_env(account_ids: &[&str]) -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let accounts_dir = temp_dir.path().join("accounts");
        fs::create_dir_all(&accounts_dir).unwrap();

        for account_id in account_ids {
            let account_dirname = account_id.replace(':', "_");
            let account_path = accounts_dir.join(&account_dirname);
            fs::create_dir_all(account_path.join("meta")).unwrap();
        }

        temp_dir
    }

    /// Helper to create a preferences file with test data
    fn create_preferences_file(temp_dir: &TempDir, prefs: &Preferences) {
        let global_dir = temp_dir.path().join("global");
        fs::create_dir_all(&global_dir).unwrap();
        let prefs_file = global_dir.join("preferences.json");
        let contents = serde_json::to_string_pretty(prefs).unwrap();
        fs::write(prefs_file, contents).unwrap();
    }

    #[test]
    fn test_preferences_load_missing_file() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        env::set_var("MY_DATA_DIR", temp_dir.path());

        let prefs = Preferences::load().unwrap();
        assert!(prefs.last_selected_multi.is_empty());
        assert!(prefs.last_selected_single.is_none());

        env::remove_var("MY_DATA_DIR");
    }

    #[test]
    fn test_preferences_load_existing_file() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        env::set_var("MY_DATA_DIR", temp_dir.path());

        let test_prefs = Preferences {
            last_selected_multi: vec!["@alice:example.org".to_string()],
            last_selected_single: Some("@bob:example.org".to_string()),
        };
        create_preferences_file(&temp_dir, &test_prefs);

        let loaded = Preferences::load().unwrap();
        assert_eq!(loaded.last_selected_multi, vec!["@alice:example.org"]);
        assert_eq!(
            loaded.last_selected_single,
            Some("@bob:example.org".to_string())
        );

        env::remove_var("MY_DATA_DIR");
    }

    #[test]
    fn test_preferences_save() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        env::set_var("MY_DATA_DIR", temp_dir.path());

        let prefs = Preferences {
            last_selected_multi: vec![
                "@alice:example.org".to_string(),
                "@bob:example.org".to_string(),
            ],
            last_selected_single: Some("@alice:example.org".to_string()),
        };

        prefs.save().unwrap();

        // Verify file was created and can be loaded
        let loaded = Preferences::load().unwrap();
        assert_eq!(loaded.last_selected_multi, prefs.last_selected_multi);
        assert_eq!(loaded.last_selected_single, prefs.last_selected_single);

        env::remove_var("MY_DATA_DIR");
    }

    #[test]
    fn test_discover_accounts_empty() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        env::set_var("MY_DATA_DIR", temp_dir.path());

        let accounts = AccountSelector::discover_accounts().unwrap();
        assert!(accounts.is_empty());

        env::remove_var("MY_DATA_DIR");
    }

    #[test]
    fn test_discover_accounts_single() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let temp_dir = setup_test_env(&["@alice:example.org"]);
        env::set_var("MY_DATA_DIR", temp_dir.path());

        let accounts = AccountSelector::discover_accounts().unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].0, "@alice:example.org");
        assert!(accounts[0].1.exists());

        env::remove_var("MY_DATA_DIR");
    }

    #[test]
    fn test_discover_accounts_multiple() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let temp_dir = setup_test_env(&["@alice:example.org", "@bob:example.com"]);
        env::set_var("MY_DATA_DIR", temp_dir.path());

        let mut accounts = AccountSelector::discover_accounts().unwrap();
        accounts.sort_by(|a, b| a.0.cmp(&b.0));

        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].0, "@alice:example.org");
        assert_eq!(accounts[1].0, "@bob:example.com");

        env::remove_var("MY_DATA_DIR");
    }

    #[test]
    fn test_select_accounts_no_accounts() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        env::set_var("MY_DATA_DIR", temp_dir.path());

        let mut selector = AccountSelector::new().unwrap();
        let result = selector.select_accounts(None, true);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No accounts found"));

        env::remove_var("MY_DATA_DIR");
    }

    #[test]
    fn test_select_accounts_with_user_id_flag() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let temp_dir = setup_test_env(&["@alice:example.org", "@bob:example.com"]);
        env::set_var("MY_DATA_DIR", temp_dir.path());

        let mut selector = AccountSelector::new().unwrap();
        let accounts = selector
            .select_accounts(Some("@alice:example.org".to_string()), true)
            .unwrap();

        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].0, "@alice:example.org");

        env::remove_var("MY_DATA_DIR");
    }

    #[test]
    fn test_select_accounts_with_invalid_user_id() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let temp_dir = setup_test_env(&["@alice:example.org"]);
        env::set_var("MY_DATA_DIR", temp_dir.path());

        let mut selector = AccountSelector::new().unwrap();
        let result = selector.select_accounts(Some("@bob:example.com".to_string()), true);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Account not found"));

        env::remove_var("MY_DATA_DIR");
    }

    #[test]
    fn test_select_accounts_single_auto_select() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let temp_dir = setup_test_env(&["@alice:example.org"]);
        env::set_var("MY_DATA_DIR", temp_dir.path());

        let mut selector = AccountSelector::new().unwrap();
        let accounts = selector.select_accounts(None, true).unwrap();

        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].0, "@alice:example.org");

        env::remove_var("MY_DATA_DIR");
    }

    #[test]
    fn test_account_dirname_conversion() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let temp_dir = setup_test_env(&["@alice:example.org"]);
        env::set_var("MY_DATA_DIR", temp_dir.path());

        let accounts = AccountSelector::discover_accounts().unwrap();
        assert_eq!(accounts[0].0, "@alice:example.org");

        // Verify the directory name has colons replaced with underscores
        let dirname = accounts[0].1.file_name().unwrap().to_string_lossy();
        assert_eq!(dirname, "@alice_example.org");

        env::remove_var("MY_DATA_DIR");
    }

    #[test]
    fn test_preferences_filter_deleted_accounts() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let temp_dir = setup_test_env(&["@alice:example.org"]);
        env::set_var("MY_DATA_DIR", temp_dir.path());

        // Create preferences with an account that doesn't exist
        let prefs = Preferences {
            last_selected_multi: vec![
                "@alice:example.org".to_string(),
                "@deleted:example.org".to_string(),
            ],
            last_selected_single: Some("@deleted:example.org".to_string()),
        };
        create_preferences_file(&temp_dir, &prefs);

        let selector = AccountSelector::new().unwrap();
        let accounts = AccountSelector::discover_accounts().unwrap();

        // Verify only existing account is discovered
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].0, "@alice:example.org");

        // Preferences should still contain the deleted account until filtering
        assert_eq!(selector.preferences.last_selected_multi.len(), 2);

        env::remove_var("MY_DATA_DIR");
    }
}
