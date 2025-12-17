use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

use crate::config::Config;
use crate::crypto::{KeyPair, public_key_to_hex};
use crate::daemon::ApiTokenManager;

const API_BASE: &str = "http://127.0.0.1";

#[derive(Serialize)]
struct JoinRequest {
    code: String,
    device_id: String,
    device_name: String,
    public_key: String,
}

#[derive(Deserialize)]
struct PairingSessionResponse {
    code: String,
    expires_in_seconds: u64,
}

#[derive(Deserialize)]
struct JoinResponse {
    status: String,
    device_id: Option<String>,
    device_name: Option<String>,
    #[allow(dead_code)]
    public_key: Option<String>,
}

#[derive(Deserialize)]
struct PendingRequestResponse {
    #[allow(dead_code)]
    pending: bool,
    request: Option<PendingRequest>,
}

#[derive(Deserialize)]
struct PendingRequest {
    device_id: String,
    device_name: String,
    public_key_fingerprint: String,
}

#[derive(Serialize)]
struct RespondRequest {
    accept: bool,
}

pub async fn pair_device(config_path: &Path, code: Option<&str>) -> Result<()> {
    if !config_path.exists() {
        println!("Not initialized. Run: wolfpack init");
        return Ok(());
    }

    let config = Config::load(config_path)?;
    let port = config.api.port.unwrap_or(9778);

    // Load API token
    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("wolfpack");
    let token_manager = ApiTokenManager::load_or_create(&data_dir)?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    match code {
        Some(code) => join_session(&client, port, token_manager.token(), &config, code).await,
        None => initiate_session(&client, port, token_manager.token()).await,
    }
}

#[allow(clippy::too_many_lines)] // Complete user interaction flow
async fn initiate_session(client: &reqwest::Client, port: u16, token: &str) -> Result<()> {
    println!("Starting pairing session...");
    println!();

    // Create pairing session
    let resp: PairingSessionResponse = client
        .post(format!("{API_BASE}:{port}/pair/initiate"))
        .header("X-Wolfpack-Token", token)
        .send()
        .await
        .context("Failed to connect to daemon. Is it running?")?
        .error_for_status()
        .context("Failed to create pairing session")?
        .json()
        .await?;

    println!("╔═══════════════════════════════════════════╗");
    println!("║          PAIRING CODE: {}          ║", resp.code);
    println!("╚═══════════════════════════════════════════╝");
    println!();
    println!("On the other device, run:");
    println!("  wolfpack pair --code {}", resp.code);
    println!();
    println!("Code expires in {} seconds.", resp.expires_in_seconds);
    println!("Waiting for connection...");
    println!();

    // Poll for incoming requests
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;

        let resp: PendingRequestResponse = client
            .get(format!("{API_BASE}:{port}/pair/pending"))
            .header("X-Wolfpack-Token", token)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        if let Some(request) = resp.request {
            println!("Incoming pairing request!");
            println!();
            println!("  Device: {} ({})", request.device_name, request.device_id);
            println!("  Key:    {}", request.public_key_fingerprint);
            println!();

            print!("Accept this device? [y/N] ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let accept = input.trim().eq_ignore_ascii_case("y");

            client
                .post(format!("{API_BASE}:{port}/pair/respond"))
                .header("X-Wolfpack-Token", token)
                .json(&RespondRequest { accept })
                .send()
                .await?
                .error_for_status()?;

            if accept {
                println!();
                println!("Device paired successfully!");
                println!("The devices will now sync automatically when discovered on the network.");
            } else {
                println!("Pairing rejected.");
            }

            return Ok(());
        }
    }
}

#[allow(clippy::too_many_lines)] // Complete user interaction flow
async fn join_session(
    client: &reqwest::Client,
    port: u16,
    token: &str,
    config: &Config,
    code: &str,
) -> Result<()> {
    // Load our keypair
    let keys_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("wolfpack")
        .join("keys");
    std::fs::create_dir_all(&keys_dir)?;

    let keypair_path = keys_dir.join("local.key");
    let keypair = KeyPair::load_or_generate(&keypair_path)?;
    let public_key = public_key_to_hex(&keypair.public_key());

    println!("Joining pairing session...");
    println!();

    let req = JoinRequest {
        code: code.to_string(),
        device_id: config.device.id.clone(),
        device_name: config.device.name.clone(),
        public_key,
    };

    let resp: JoinResponse = client
        .post(format!("{API_BASE}:{port}/pair/join"))
        .header("X-Wolfpack-Token", token)
        .json(&req)
        .send()
        .await
        .context("Failed to connect to daemon. Is it running?")?
        .error_for_status()
        .context("Failed to join pairing session")?
        .json()
        .await?;

    match resp.status.as_str() {
        "accepted" => {
            println!("Pairing accepted!");
            println!();
            if let (Some(name), Some(id)) = (&resp.device_name, &resp.device_id) {
                println!("  Device: {} ({})", name, id);
            }
            println!();
            println!("The devices will now sync automatically when discovered on the network.");
        }
        "rejected" => {
            println!("Pairing was rejected by the other device.");
        }
        "expired" => {
            println!("Pairing code has expired. Ask the other device for a new code.");
        }
        "invalid_code" => {
            println!("Invalid pairing code. Check the code and try again.");
        }
        status => {
            println!("Unknown status: {}", status);
        }
    }

    Ok(())
}
