/// SDK utilities for client management and encryption state synchronization
///
/// This module provides helper functions for:
/// - Restoring a Matrix SDK Client for a given account
/// - Synchronizing encryption state via minimal sliding sync
use anyhow::{Context, Result};
use matrix_sdk::Client;
use std::fs;
use std::path::Path;
use url::Url;

/// Minimum number of sync iterations required to ensure encryption state
/// (including cross-signing verification status) is fully updated after
/// recovery key verification or other encryption-related operations.
///
/// Testing showed that 3 iterations reliably synchronizes the verification
/// state, while fewer iterations may leave the state incomplete.
const MIN_SYNC_ITERATIONS_FOR_VERIFICATION: usize = 3;

/// Restore a Matrix SDK Client for a given account.
///
/// This loads the session metadata and credentials, then recreates the client
/// with the same homeserver and encryption state. Also initializes SDK logging
/// for the account.
pub async fn restore_client_for_account(account_dir: &Path, account_id: &str) -> Result<Client> {
    use matrix_sdk::authentication::matrix::MatrixSession;
    use matrix_sdk::ruma::UserId;
    use matrix_sdk::{SessionMeta, SessionTokens};

    // Initialize SDK logging for this account
    crate::logging::init_account_logging(account_dir, account_id)
        .context("Failed to initialize SDK logging")?;

    let sdk_store_dir = account_dir.join("sdk");
    let session_path = account_dir.join("meta/session.json");

    let session_meta: serde_json::Value = serde_json::from_slice(&fs::read(&session_path)?)?;
    let homeserver = session_meta["homeserver"]
        .as_str()
        .context("Missing homeserver in session.json")?;

    let secrets_store = crate::secrets::AccountSecretsStore::new(account_id)?;

    let passphrase = secrets_store
        .get_db_passphrase()
        .context("No database passphrase stored")?;

    let access_token = secrets_store
        .get_access_token()
        .context("No access token stored")?;

    let homeserver_url = Url::parse(homeserver)?;

    let client = Client::builder()
        .homeserver_url(homeserver_url)
        .sqlite_store(sdk_store_dir, Some(&passphrase))
        .build()
        .await
        .context("Failed to build client")?;

    let user_id_parsed = UserId::parse(account_id)?;
    let device_id_str = session_meta["device_id"]
        .as_str()
        .context("Missing device_id")?
        .to_string();

    let session = MatrixSession {
        meta: SessionMeta {
            user_id: user_id_parsed,
            device_id: device_id_str.into(),
        },
        tokens: SessionTokens {
            access_token,
            refresh_token: secrets_store.get_refresh_token(),
        },
    };

    client
        .restore_session(session)
        .await
        .context("Failed to restore session")?;

    Ok(client)
}

/// Run a minimal sliding sync to update encryption state.
/// This ensures verification_state gets updated without needing a full /sync loop.
pub async fn sync_encryption_state(client: &Client) -> Result<()> {
    use futures_util::StreamExt;
    use matrix_sdk::ruma::assign;

    // Create a minimal sliding sync for encryption only (no room lists)
    let sliding_sync = client
        .sliding_sync("enc-verify")?
        .poll_timeout(std::time::Duration::from_secs(0))
        .with_to_device_extension(assign!(
            matrix_sdk::ruma::api::client::sync::sync_events::v5::request::ToDevice::default(),
            { enabled: Some(true) }
        ))
        .with_e2ee_extension(assign!(
            matrix_sdk::ruma::api::client::sync::sync_events::v5::request::E2EE::default(),
            { enabled: Some(true) }
        ))
        .build()
        .await?;

    let stream = sliding_sync.sync();
    futures_util::pin_mut!(stream);

    // Run minimal sync iterations to ensure verification_state updates
    for _ in 0..MIN_SYNC_ITERATIONS_FOR_VERIFICATION {
        if let Some(result) = stream.next().await {
            result?;
        } else {
            break;
        }
    }

    Ok(())
}
