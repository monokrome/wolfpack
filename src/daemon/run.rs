use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock, broadcast};
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::crypto::KeyPair;
use crate::events::EventLog;
use crate::net::{EncryptedEvent, NetworkEvent, Node};
use crate::profile::{find_profile, is_browser_running};
use crate::state::StateDb;
use crate::sync::SyncEngine;

use super::ipc::handle_ipc_client;
use super::{ApiState, ApiTokenManager, FileWatcher, IpcSocket, PairingManager, PairingState};
use super::{PairingCommand, start_http_api};

fn ipc_socket_path() -> PathBuf {
    dirs::runtime_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("wolfpack.sock")
}

/// Shared daemon context for event handlers
struct DaemonContext {
    engine: Arc<Mutex<SyncEngine>>,
    node: Node,
    config: Config,
    profile_path: PathBuf,
    _watcher: FileWatcher, // Keep watcher alive
}

#[allow(clippy::cognitive_complexity)] // Entry point with multiple initialization checks
pub async fn run_daemon(config: Config) -> Result<()> {
    info!("Starting wolfpack daemon");
    info!("Device: {} ({})", config.device.name, config.device.id);

    // Initialize all daemon components
    let (ctx, ipc, watcher_events, pairing_rx) = initialize_daemon(&config).await?;

    // Run the main event loop
    run_event_loop(ctx, ipc, watcher_events, pairing_rx).await
}

#[allow(clippy::cognitive_complexity)] // Sequential initialization with multiple components
async fn initialize_daemon(
    config: &Config,
) -> Result<(
    DaemonContext,
    IpcSocket,
    broadcast::Receiver<notify::Event>,
    tokio::sync::mpsc::Receiver<PairingCommand>,
)> {
    let keypair = init_keypair()?;
    let public_key_hex = crate::crypto::public_key_to_hex(&keypair.public_key());
    info!("Public key: {}", public_key_hex);

    let pairing_rx = init_http_api(config, &public_key_hex).await?;

    let state_db = init_state_db()?;
    let event_log = EventLog::new(
        config.paths.sync_dir.clone(),
        config.device.id.clone(),
        keypair,
    );

    let sync_engine = SyncEngine::new(config.clone(), event_log, state_db)?;
    let engine = Arc::new(Mutex::new(sync_engine));

    let node = init_p2p_node(config).await?;
    let profile_path = resolve_profile_path(config)?;
    let watcher = FileWatcher::new(&[profile_path.as_path()])?;
    let watcher_events = watcher.events.resubscribe();
    let ipc = init_ipc_socket().await?;

    // Initial profile scan
    scan_profile(&engine, "Initial scan").await;

    info!("Daemon initialized, waiting for events...");

    let ctx = DaemonContext {
        engine,
        node,
        config: config.clone(),
        profile_path,
        _watcher: watcher,
    };

    Ok((ctx, ipc, watcher_events, pairing_rx))
}

fn init_keypair() -> Result<KeyPair> {
    let keys_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("wolfpack")
        .join("keys");
    std::fs::create_dir_all(&keys_dir)?;
    let keypair_path = keys_dir.join("local.key");
    KeyPair::load_or_generate(&keypair_path)
}

async fn init_http_api(
    config: &Config,
    public_key_hex: &str,
) -> Result<tokio::sync::mpsc::Receiver<PairingCommand>> {
    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("wolfpack");
    let token_manager = ApiTokenManager::load_or_create(&data_dir)?;
    info!(
        "API token loaded from: {}",
        token_manager.token_path().display()
    );

    let (pairing_manager, pairing_rx) = PairingManager::new();

    let api_state = Arc::new(RwLock::new(ApiState {
        token_manager,
        pairing_manager,
        device_id: config.device.id.clone(),
        device_name: config.device.name.clone(),
        public_key: public_key_hex.to_string(),
    }));

    let http_port = config.api.port.unwrap_or(9778);
    tokio::spawn(async move {
        if let Err(e) = start_http_api(api_state, http_port).await {
            error!("HTTP API server error: {}", e);
        }
    });
    info!("HTTP API started on port {}", http_port);

    Ok(pairing_rx)
}

fn init_state_db() -> Result<StateDb> {
    let state_db_path = Config::default_state_db();
    StateDb::open(&state_db_path).with_context(|| {
        format!(
            "Failed to open state database at {}",
            state_db_path.display()
        )
    })
}

async fn init_p2p_node(config: &Config) -> Result<Node> {
    let mut node = Node::new(
        config.device.name.clone(),
        config.sync.listen_port,
        config.sync.enable_mdns,
        config.sync.enable_dht,
    )
    .await?;
    info!("P2P node started, peer ID: {}", node.peer_id());

    if config.sync.enable_dht {
        add_bootstrap_peers(&mut node, &config.sync.bootstrap_peers).await;
    }

    Ok(node)
}

async fn add_bootstrap_peers(node: &mut Node, peers: &[String]) {
    for peer_addr in peers {
        if let Ok(addr) = peer_addr.parse() {
            info!("Adding bootstrap peer: {}", peer_addr);
            let _ = node
                .send_command(crate::net::NetworkCommand::Dial { addr })
                .await;
        }
    }
}

fn resolve_profile_path(config: &Config) -> Result<PathBuf> {
    config
        .paths
        .profile
        .clone()
        .map(Ok)
        .unwrap_or_else(find_profile)
}

async fn init_ipc_socket() -> Result<IpcSocket> {
    let path = ipc_socket_path();
    let ipc = IpcSocket::new(&path).await?;
    info!("IPC socket: {}", path.display());
    Ok(ipc)
}

#[allow(clippy::cognitive_complexity)] // Simple match with multiple arms
async fn scan_profile(engine: &Arc<Mutex<SyncEngine>>, context: &str) {
    let mut engine = engine.lock().await;
    match engine.scan_profile() {
        Ok(events) if !events.is_empty() => {
            info!("{}: {} events to sync", context, events.len());
        }
        Err(e) => warn!("{} failed: {}", context, e),
        _ => {}
    }
}

#[allow(clippy::cognitive_complexity)] // tokio::select! event loop pattern
async fn run_event_loop(
    mut ctx: DaemonContext,
    ipc: IpcSocket,
    mut watcher_events: broadcast::Receiver<notify::Event>,
    mut pairing_rx: tokio::sync::mpsc::Receiver<PairingCommand>,
) -> Result<()> {
    let mut browser_was_running = is_browser_running(&ctx.profile_path);
    let mut sync_interval = tokio::time::interval(Duration::from_secs(30));
    let mut pairing_state = PairingState::new();

    loop {
        tokio::select! {
            Some(event) = ctx.node.next_event() => {
                handle_network_event(event, &ctx).await;
            }

            event = watcher_events.recv() => {
                if let Ok(event) = event {
                    handle_profile_change(event, &ctx).await;
                }
            }

            client = ipc.listener().accept() => {
                handle_ipc_accept(client, &ctx).await;
            }

            _ = sync_interval.tick() => {
                handle_periodic_sync(&ctx).await;
            }

            Some(cmd) = pairing_rx.recv() => {
                pairing_state.handle_command(cmd);
            }

            _ = tokio::time::sleep(Duration::from_secs(5)) => {
                browser_was_running = handle_browser_state_check(
                    &ctx,
                    browser_was_running,
                ).await;
            }

            _ = tokio::signal::ctrl_c() => {
                info!("Shutting down...");
                break;
            }
        }
    }

    cleanup_ipc_socket();
    Ok(())
}

async fn handle_profile_change(event: notify::Event, ctx: &DaemonContext) {
    debug!("Profile change: {:?}", event.kind);
    // Debounce by waiting briefly for more events
    tokio::time::sleep(Duration::from_millis(100)).await;
    scan_profile(&ctx.engine, "Profile changed").await;
}

async fn handle_ipc_accept(
    client: std::io::Result<(tokio::net::UnixStream, tokio::net::unix::SocketAddr)>,
    ctx: &DaemonContext,
) {
    match client {
        Ok((stream, _)) => {
            let engine = ctx.engine.clone();
            let node_peers = ctx.node.peers().await;
            tokio::spawn(async move {
                if let Err(e) = handle_ipc_client(stream, engine, node_peers).await {
                    error!("IPC client error: {}", e);
                }
            });
        }
        Err(e) => error!("IPC accept error: {}", e),
    }
}

#[allow(clippy::cognitive_complexity)] // Loop with early return and error handling
async fn handle_periodic_sync(ctx: &DaemonContext) {
    let peers = ctx.node.peers().await;
    if peers.is_empty() {
        return;
    }

    debug!("Periodic sync with {} peers", peers.len());
    for (peer_id, _) in peers {
        if let Err(e) = ctx.node.get_clock(peer_id).await {
            warn!("Failed to request clock from peer: {}", e);
        }
    }
}

#[allow(clippy::cognitive_complexity)] // State check with conditional flushing
async fn handle_browser_state_check(ctx: &DaemonContext, was_running: bool) -> bool {
    let browser_running = is_browser_running(&ctx.profile_path);
    if was_running && !browser_running {
        info!("Browser closed, flushing write queue");
        let mut engine = ctx.engine.lock().await;
        match engine.flush_write_queue() {
            Ok(files) if !files.is_empty() => {
                info!("Flushed write queue: {:?}", files);
            }
            Err(e) => warn!("Failed to flush write queue: {}", e),
            _ => {}
        }
    }
    browser_running
}

fn cleanup_ipc_socket() {
    let path = ipc_socket_path();
    if path.exists() {
        let _ = std::fs::remove_file(&path);
    }
}

async fn handle_network_event(event: NetworkEvent, ctx: &DaemonContext) {
    match event {
        NetworkEvent::PeerDiscovered {
            peer_id,
            device_name,
        } => handle_peer_discovered(&ctx.node, peer_id, device_name).await,

        NetworkEvent::PeerDisconnected { peer_id } => {
            info!("Peer disconnected: {}", peer_id);
        }

        NetworkEvent::ClockRequested { from, request_id } => {
            handle_clock_request(ctx, from, request_id).await;
        }

        NetworkEvent::EventsRequested {
            from,
            request_id,
            clock,
        } => handle_events_request(ctx, from, request_id, clock).await,

        NetworkEvent::EventsReceived { from, events } => {
            handle_events_received(ctx, from, events).await;
        }

        NetworkEvent::TabReceived {
            from,
            url,
            title,
            from_device,
        } => handle_tab_received(ctx, from, url, title, from_device).await,
    }
}

#[allow(clippy::cognitive_complexity)] // Simple handler with error logging
async fn handle_peer_discovered(node: &Node, peer_id: libp2p::PeerId, device_name: Option<String>) {
    info!(
        "Peer discovered: {} ({})",
        peer_id,
        device_name.as_deref().unwrap_or("unknown")
    );
    if let Err(e) = node.get_clock(peer_id).await {
        warn!("Failed to request clock from new peer: {}", e);
    }
}

async fn handle_clock_request(
    ctx: &DaemonContext,
    from: libp2p::PeerId,
    request_id: libp2p::request_response::InboundRequestId,
) {
    debug!("Clock requested by {}", from);
    let engine = ctx.engine.lock().await;
    let clock = engine.get_vector_clock();
    let _ = ctx
        .node
        .send_command(crate::net::NetworkCommand::RespondClock {
            request_id,
            clock,
            device_id: ctx.config.device.id.clone(),
            device_name: ctx.config.device.name.clone(),
        })
        .await;
}

#[allow(clippy::cognitive_complexity)] // Async handler with error handling
async fn handle_events_request(
    ctx: &DaemonContext,
    from: libp2p::PeerId,
    request_id: libp2p::request_response::InboundRequestId,
    clock: HashMap<String, u64>,
) {
    debug!("Events requested by {} with clock {:?}", from, clock);
    let engine = ctx.engine.lock().await;
    match engine.get_events_since(&clock) {
        Ok(events) => {
            let _ = ctx
                .node
                .send_command(crate::net::NetworkCommand::RespondEvents { request_id, events })
                .await;
        }
        Err(e) => warn!("Failed to get events for peer: {}", e),
    }
}

#[allow(clippy::cognitive_complexity)] // Async handler with match arms
async fn handle_events_received(
    ctx: &DaemonContext,
    from: libp2p::PeerId,
    events: Vec<EncryptedEvent>,
) {
    info!("Received {} events from {}", events.len(), from);
    let mut engine = ctx.engine.lock().await;
    match engine.apply_remote_events(events) {
        Ok(applied) if applied > 0 => {
            info!("Applied {} events from {}", applied, from);
        }
        Err(e) => warn!("Failed to apply events from {}: {}", from, e),
        _ => {}
    }
}

#[allow(clippy::cognitive_complexity)] // Simple async handler
async fn handle_tab_received(
    ctx: &DaemonContext,
    from: libp2p::PeerId,
    url: String,
    title: Option<String>,
    from_device: String,
) {
    info!("Tab received from {} ({}): {}", from_device, from, url);
    let mut engine = ctx.engine.lock().await;
    if let Err(e) = engine.receive_tab(&url, title.as_deref(), &from_device) {
        warn!("Failed to save received tab: {}", e);
    }
}
