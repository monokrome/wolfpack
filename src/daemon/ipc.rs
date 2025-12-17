use anyhow::Result;
use libp2p::PeerId;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

use crate::sync::SyncEngine;

/// Handle an IPC client connection
pub async fn handle_ipc_client(
    stream: tokio::net::UnixStream,
    engine: Arc<Mutex<SyncEngine>>,
    peers: HashMap<PeerId, String>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let response = process_command(line.trim(), &engine, &peers).await;
        writer.write_all(response.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        line.clear();
    }

    Ok(())
}

async fn process_command(
    command: &str,
    engine: &Arc<Mutex<SyncEngine>>,
    peers: &HashMap<PeerId, String>,
) -> String {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return "ERROR: Empty command".to_string();
    }

    match parts[0] {
        "status" => cmd_status(engine, peers).await,
        "peers" => cmd_peers(peers),
        "tabs" => cmd_tabs(engine).await,
        "send" => cmd_send(&parts, engine).await,
        "open" => cmd_open(&parts, engine).await,
        _ => format!("ERROR: Unknown command: {}", parts[0]),
    }
}

async fn cmd_status(engine: &Arc<Mutex<SyncEngine>>, peers: &HashMap<PeerId, String>) -> String {
    let engine = engine.lock().await;
    format!(
        "OK: Device {} - {} peers connected",
        engine.device_id(),
        peers.len()
    )
}

fn cmd_peers(peers: &HashMap<PeerId, String>) -> String {
    if peers.is_empty() {
        return "OK: No peers connected".to_string();
    }
    let list: Vec<String> = peers
        .iter()
        .map(|(id, name)| format!("  {}: {}", id, name))
        .collect();
    format!("OK:\n{}", list.join("\n"))
}

async fn cmd_tabs(engine: &Arc<Mutex<SyncEngine>>) -> String {
    let engine = engine.lock().await;
    match engine.get_pending_tabs() {
        Ok(tabs) if tabs.is_empty() => "OK: No pending tabs".to_string(),
        Ok(tabs) => {
            let list: Vec<String> = tabs
                .iter()
                .map(|t| format!("{}: {} (from {})", t.id, t.url, t.from_device))
                .collect();
            format!("OK:\n{}", list.join("\n"))
        }
        Err(e) => format!("ERROR: {}", e),
    }
}

async fn cmd_send(parts: &[&str], engine: &Arc<Mutex<SyncEngine>>) -> String {
    if parts.len() < 3 {
        return "ERROR: Usage: send <device> <url> [title]".to_string();
    }

    let device = parts[1];
    let url = parts[2];
    let title = if parts.len() > 3 {
        Some(parts[3..].join(" "))
    } else {
        None
    };

    let mut engine = engine.lock().await;
    match engine.send_tab(device, url, title.as_deref()) {
        Ok(_) => format!("OK: Tab queued for {}", device),
        Err(e) => format!("ERROR: {}", e),
    }
}

async fn cmd_open(parts: &[&str], engine: &Arc<Mutex<SyncEngine>>) -> String {
    if parts.len() < 2 {
        return "ERROR: Usage: open <tab_id>".to_string();
    }

    let tab_id = parts[1];
    let mut engine = engine.lock().await;

    let tabs = match engine.get_pending_tabs() {
        Ok(t) => t,
        Err(e) => return format!("ERROR: {}", e),
    };

    let Some(tab) = tabs.iter().find(|t| t.id == tab_id) else {
        return "ERROR: Tab not found".to_string();
    };

    if let Err(e) = engine.open_tab(&tab.url) {
        return format!("ERROR: {}", e);
    }

    if let Err(e) = engine.acknowledge_tab(tab_id) {
        return format!("ERROR: {}", e);
    }

    "OK: Tab opened".to_string()
}
