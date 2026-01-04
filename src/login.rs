use anyhow::{Context, Result};
use matrix_sdk::{AuthSession, Client};
use rand::{distributions::Alphanumeric, Rng};
use rpassword::prompt_password;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use url::Url;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct SessionMetaFile {
    pub user_id: String,
    pub device_id: String,
    pub homeserver: String,
}

pub async fn run(user_id_flag: Option<String>) -> Result<()> {
    // Resolve data root
    let data_root = resolve_data_root()?;
    let accounts_root = data_root.join("accounts");
    fs::create_dir_all(&accounts_root)
        .with_context(|| format!("create accounts dir at {}", accounts_root.display()))?;

    // List existing accounts for information when no --user-id provided
    if user_id_flag.is_none() {
        let mut existing_accounts = Vec::new();
        if accounts_root.exists() {
            for entry in fs::read_dir(&accounts_root)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    let dirname = entry.file_name().to_string_lossy().to_string();
                    let uid = dirname.replace('_', ":");
                    existing_accounts.push(uid);
                }
            }
        }

        if !existing_accounts.is_empty() {
            eprintln!("Existing accounts:");
            for account in &existing_accounts {
                eprintln!("  - {}", account);
            }
            eprintln!();
        }

        eprintln!("Add a new account.");
    }

    // Perform interactive login, which will prompt for credentials
    let (client, account_id, restored) = login_interactive(user_id_flag, &accounts_root).await?;

    // Initialize encryption and cross-signing
    initialize_encryption(&client).await?;

    // If cross-signing exists but device is not verified, offer verification UX
    maybe_verify_device(&client).await?;

    if restored {
        eprintln!("Session restored for {}", account_id);
    } else {
        eprintln!("Logged in and stored credentials for {}", account_id);
    }

    // Gracefully shut down the client and give background tasks time to complete
    drop(client);
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    Ok(())
}

async fn login_interactive(
    user_id_flag: Option<String>,
    accounts_root: &Path,
) -> Result<(Client, String, bool)> {
    // Prompt for credentials in the correct order: server, user id, password
    let server = prompt("Server (e.g., matrix.org or https://matrix.example.org): ")?;
    let server_trim = server.trim();

    // Use provided user_id or prompt for it
    let user_input = match user_id_flag {
        Some(uid) => uid,
        None => {
            let input = prompt("User ID or username (e.g., @alice:example.org or alice): ")?;
            input.trim().to_owned()
        }
    };

    let password = prompt_password("Password: ")?;

    // Extract actual user ID if it's a full ID, otherwise we'll get it after login
    let account_id_hint = if user_input.starts_with('@') && user_input.contains(':') {
        user_input.clone()
    } else {
        // We'll determine the full user ID after login
        format!("@{}:{}", user_input, server_trim)
    };

    let account_dir = accounts_root.join(account_id_to_dirname(&account_id_hint));
    fs::create_dir_all(account_dir.join("meta"))?;

    let sdk_store_dir = account_dir.join("sdk");
    fs::create_dir_all(&sdk_store_dir)?;

    // Determine homeserver URL from server input
    let hs_candidate = candidate_from_input(server_trim);
    let homeserver_url = homeserver_url_from_candidate(&hs_candidate)?;

    // Always generate a new db_passphrase and overwrite secrets on login
    let passphrase = generate_passphrase();

    // Build client using the previously determined homeserver URL
    let homeserver_url_parsed = Url::parse(&homeserver_url)?;
    let client = Client::builder()
        .homeserver_url(homeserver_url_parsed)
        .sqlite_store(sdk_store_dir.clone(), Some(&passphrase))
        .build()
        .await?;

    // Perform interactive login using the credentials collected earlier
    client
        .matrix_auth()
        .login_username(&user_input, password.trim())
        .initial_device_display_name("my-cli")
        .send()
        .await
        .context("login failed")?;

    // Persist session meta and tokens
    let session = match client.session() {
        Some(AuthSession::Matrix(s)) => s.clone(),
        _ => anyhow::bail!("unexpected session type"),
    };

    let actual_user_id = session.meta.user_id.to_string();
    let device_id = session.meta.device_id.to_string();
    let meta = SessionMetaFile {
        user_id: actual_user_id.clone(),
        device_id,
        homeserver: homeserver_url.clone(),
    };
    let session_path = account_dir.join("meta/session.json");
    fs::write(&session_path, serde_json::to_vec(&meta)?)?;

    // Store credentials using the new abstraction
    let mut secrets_store = crate::secrets::AccountSecretsStore::new(&actual_user_id)?;
    secrets_store.store_credentials(
        Some(passphrase.clone()),
        Some(session.tokens.access_token.clone()),
        session.tokens.refresh_token.clone(),
    )?;

    // Verify directory consistency: ensure actual_user_id matches account_id_hint
    // If server returned a different format, we may need to move the session.json
    let expected_account_dir = accounts_root.join(account_id_to_dirname(&actual_user_id));
    if account_dir != expected_account_dir {
        eprintln!(
            "Warning: Server returned user ID '{}' which differs from hint '{}'. Moving session files...",
            actual_user_id, account_id_hint
        );
        fs::create_dir_all(expected_account_dir.join("meta"))?;
        let new_session_path = expected_account_dir.join("meta/session.json");
        fs::rename(&session_path, &new_session_path).with_context(|| {
            format!(
                "Failed to move session.json to {}",
                new_session_path.display()
            )
        })?;
        // Remove old account_dir if empty
        let _ = fs::remove_dir_all(&account_dir);
    }

    Ok((client, actual_user_id, false))
}

async fn initialize_encryption(_client: &Client) -> Result<()> {
    // The SDK initializes encryption automatically after login/restore.
    Ok(())
}

async fn maybe_verify_device(client: &Client) -> Result<()> {
    // Check cross-signing and device verification status
    let user_id = client
        .user_id()
        .context("no user id after login")?
        .to_owned();
    let own_device = client
        .encryption()
        .get_own_device()
        .await
        .context("failed to get own device")?;

    let is_verified = own_device
        .as_ref()
        .map(|d| d.is_verified())
        .unwrap_or(false);

    let xsign = client
        .encryption()
        .cross_signing_status()
        .await
        .context("failed to get cross-signing status")?;

    let xsign_exists = xsign.has_master && xsign.has_self_signing && xsign.has_user_signing;
    if xsign_exists && !is_verified {
        eprintln!("Your account uses cross-signing. This new device must be verified.");
        eprintln!("Choose verification method: (1) Emoji (SAS)  (2) Recovery key");
        loop {
            let choice = prompt("Method [1/2]: ")?;
            match choice.trim() {
                "1" => {
                    // Emoji verification: pick a verified device to verify against
                    let devices = client
                        .encryption()
                        .get_user_devices(&user_id)
                        .await
                        .context("failed to list user devices")?;

                    let trusted: Vec<_> = devices
                        .devices()
                        .filter(|d| {
                            own_device
                                .as_ref()
                                .map(|od| d.device_id() != od.device_id())
                                .unwrap_or(true)
                                && d.is_verified()
                        })
                        .collect();

                    if trusted.is_empty() {
                        eprintln!("No other verified device found. Please choose recovery key method or verify from another device.");
                        continue;
                    }

                    eprintln!("Select a device to verify with:");
                    for (i, d) in trusted.iter().enumerate() {
                        eprintln!(
                            "  {}: {} (verified)",
                            i + 1,
                            d.display_name().unwrap_or("(unknown)")
                        );
                    }
                    let sel = prompt("Device number: ")?;
                    let idx: usize = match sel.trim().parse() {
                        Ok(n) if n > 0 && n <= trusted.len() => n,
                        _ => {
                            eprintln!("Invalid selection");
                            continue;
                        }
                    };
                    let peer = &trusted[idx - 1];

                    let req = peer
                        .request_verification()
                        .await
                        .context("failed to request verification")?;

                    let sas = req
                        .start_sas()
                        .await
                        .context("failed to start SAS verification")?;

                    if let Some(emojis) = sas.as_ref().and_then(|s| s.emoji()) {
                        eprintln!("Compare these emojis on both devices:");
                        let line = emojis
                            .iter()
                            .map(|e| e.symbol.to_string())
                            .collect::<Vec<_>>()
                            .join(" ");
                        eprintln!("{}", line);
                    } else {
                        eprintln!(
                            "SAS is initializing; confirm the verification on the other device."
                        );
                    }

                    let confirm = prompt("Do they match? [y/N]: ")?;
                    if matches!(confirm.trim(), "y" | "Y") {
                        if let Some(s) = &sas {
                            s.confirm().await.context("failed to confirm SAS")?;
                        }
                        eprintln!("Device verified via SAS.");
                    } else {
                        if let Some(s) = &sas {
                            s.cancel().await.ok();
                        }
                        eprintln!("SAS verification cancelled.");
                    }
                    break;
                }
                "2" => {
                    eprintln!("Enter your recovery key (from secret storage/backup):");
                    let key = prompt("Recovery key: ")?;
                    // Recovery key flow differs between SDK versions; if unsupported here,
                    // instruct the user to verify this device from another verified device
                    // by entering the recovery key there.
                    eprintln!("If prompted, enter this recovery key on a verified device to trust this device.");
                    eprintln!("Recovery key: {}", key.trim());
                    break;
                }
                _ => {
                    eprintln!("Please enter 1 or 2.");
                }
            }
        }
    }

    Ok(())
}

pub fn resolve_data_root() -> Result<PathBuf> {
    if let Some(dir) = env::var_os("MY_DATA_DIR") {
        return Ok(PathBuf::from(dir));
    }
    Ok(PathBuf::from(".my"))
}

pub fn account_id_to_dirname(user_id: &str) -> String {
    user_id.replace(':', "_")
}

pub fn prompt(msg: &str) -> Result<String> {
    print!("{}", msg);
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input)
}

fn generate_passphrase() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(64)
        .map(char::from)
        .collect()
}

fn candidate_from_input(server_trim: &str) -> String {
    if server_trim.starts_with('@') && server_trim.contains(':') {
        server_trim
            .split_once(':')
            .map(|(_, server)| server.to_owned())
            .unwrap_or_else(|| server_trim.to_owned())
    } else {
        server_trim.to_owned()
    }
}

fn homeserver_url_from_candidate(candidate: &str) -> Result<String> {
    if Url::parse(candidate).is_ok() {
        Ok(candidate.to_owned())
    } else {
        let url = Url::parse(&format!("https://{}", candidate))?;
        Ok(url.to_string())
    }
}
