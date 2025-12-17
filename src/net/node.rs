use anyhow::{Context, Result};
use futures::StreamExt;
use libp2p::{
    Multiaddr, PeerId, Swarm, identify, identity, kad, mdns, noise, request_response,
    swarm::SwarmEvent, tcp, yamux,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, error, info, warn};

use super::behaviour::{WolfpackBehaviour, WolfpackBehaviourEvent};
use super::protocol::{EncryptedEvent, SyncRequest, SyncResponse};

/// Events sent from the network to the application
#[derive(Debug)]
pub enum NetworkEvent {
    /// A new peer was discovered
    PeerDiscovered {
        peer_id: PeerId,
        device_name: Option<String>,
    },

    /// A peer disconnected
    PeerDisconnected { peer_id: PeerId },

    /// Received events from a peer
    EventsReceived {
        from: PeerId,
        events: Vec<EncryptedEvent>,
    },

    /// A peer requested our events
    EventsRequested {
        from: PeerId,
        request_id: request_response::InboundRequestId,
        clock: HashMap<String, u64>,
    },

    /// A peer sent us a tab
    TabReceived {
        from: PeerId,
        url: String,
        title: Option<String>,
        from_device: String,
    },

    /// A peer requested our clock
    ClockRequested {
        from: PeerId,
        request_id: request_response::InboundRequestId,
    },
}

/// Commands sent to the network from the application
#[derive(Debug)]
pub enum NetworkCommand {
    /// Request a peer's clock
    GetClock { peer_id: PeerId },

    /// Request events from a peer
    GetEvents {
        peer_id: PeerId,
        clock: HashMap<String, u64>,
    },

    /// Push events to a peer
    PushEvents {
        peer_id: PeerId,
        events: Vec<EncryptedEvent>,
    },

    /// Send a tab to a peer
    SendTab {
        peer_id: PeerId,
        url: String,
        title: Option<String>,
        from_device: String,
    },

    /// Respond to a clock request
    RespondClock {
        request_id: request_response::InboundRequestId,
        clock: HashMap<String, u64>,
        device_id: String,
        device_name: String,
    },

    /// Respond to an events request
    RespondEvents {
        request_id: request_response::InboundRequestId,
        events: Vec<EncryptedEvent>,
    },

    /// Connect to a known peer address
    Dial { addr: Multiaddr },

    /// Add a bootstrap peer for DHT
    AddBootstrapPeer { peer_id: PeerId, addr: Multiaddr },
}

/// The P2P node
pub struct Node {
    /// Channel to send commands to the swarm
    command_tx: mpsc::Sender<NetworkCommand>,
    /// Channel to receive events from the swarm
    event_rx: mpsc::Receiver<NetworkEvent>,
    /// Our local peer ID
    peer_id: PeerId,
    /// Known peers (peer_id -> device_name)
    peers: Arc<Mutex<HashMap<PeerId, String>>>,
}

impl Node {
    /// Create and start a new P2P node
    pub async fn new(
        device_name: String,
        listen_port: Option<u16>,
        enable_mdns: bool,
        enable_dht: bool,
    ) -> Result<Self> {
        // Generate or load identity
        let local_key = identity::Keypair::generate_ed25519();
        let local_peer_id = local_key.public().to_peer_id();

        info!("Local peer ID: {}", local_peer_id);

        // Build the swarm
        let mut swarm = libp2p::SwarmBuilder::with_existing_identity(local_key.clone())
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_quic()
            .with_relay_client(noise::Config::new, yamux::Config::default)?
            .with_behaviour(|key, relay| WolfpackBehaviour::new(key, relay, enable_mdns))?
            .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
            .build();

        // Listen on TCP
        let listen_addr: Multiaddr =
            format!("/ip4/0.0.0.0/tcp/{}", listen_port.unwrap_or(0)).parse()?;
        swarm.listen_on(listen_addr)?;

        // Listen on QUIC
        let quic_addr: Multiaddr =
            format!("/ip4/0.0.0.0/udp/{}/quic-v1", listen_port.unwrap_or(0)).parse()?;
        swarm.listen_on(quic_addr)?;

        // Set up channels
        let (command_tx, command_rx) = mpsc::channel(100);
        let (event_tx, event_rx) = mpsc::channel(100);
        let peers = Arc::new(Mutex::new(HashMap::new()));

        // Spawn the swarm event loop
        let peers_clone = peers.clone();
        tokio::spawn(run_swarm(
            swarm,
            command_rx,
            event_tx,
            peers_clone,
            device_name,
            enable_dht,
        ));

        Ok(Self {
            command_tx,
            event_rx,
            peer_id: local_peer_id,
            peers,
        })
    }

    /// Get our local peer ID
    pub fn peer_id(&self) -> &PeerId {
        &self.peer_id
    }

    /// Get list of connected peers
    pub async fn peers(&self) -> HashMap<PeerId, String> {
        self.peers.lock().await.clone()
    }

    /// Receive the next network event
    pub async fn next_event(&mut self) -> Option<NetworkEvent> {
        self.event_rx.recv().await
    }

    /// Send a command to the network
    pub async fn send_command(&self, cmd: NetworkCommand) -> Result<()> {
        self.command_tx
            .send(cmd)
            .await
            .context("Failed to send network command")
    }

    /// Request a peer's clock
    pub async fn get_clock(&self, peer_id: PeerId) -> Result<()> {
        self.send_command(NetworkCommand::GetClock { peer_id })
            .await
    }

    /// Request events from a peer
    pub async fn get_events(&self, peer_id: PeerId, clock: HashMap<String, u64>) -> Result<()> {
        self.send_command(NetworkCommand::GetEvents { peer_id, clock })
            .await
    }

    /// Push events to a peer
    pub async fn push_events(&self, peer_id: PeerId, events: Vec<EncryptedEvent>) -> Result<()> {
        self.send_command(NetworkCommand::PushEvents { peer_id, events })
            .await
    }

    /// Send a tab to a peer
    pub async fn send_tab(
        &self,
        peer_id: PeerId,
        url: String,
        title: Option<String>,
        from_device: String,
    ) -> Result<()> {
        self.send_command(NetworkCommand::SendTab {
            peer_id,
            url,
            title,
            from_device,
        })
        .await
    }
}

/// Run the swarm event loop
#[allow(clippy::cognitive_complexity)] // Core P2P event loop
#[allow(clippy::too_many_arguments)] // Required for swarm coordination
async fn run_swarm(
    mut swarm: Swarm<WolfpackBehaviour>,
    mut command_rx: mpsc::Receiver<NetworkCommand>,
    event_tx: mpsc::Sender<NetworkEvent>,
    peers: Arc<Mutex<HashMap<PeerId, String>>>,
    _device_name: String,
    enable_dht: bool,
) {
    let mut discovered_peers: HashSet<PeerId> = HashSet::new();

    loop {
        tokio::select! {
            // Handle swarm events
            event = swarm.select_next_some() => {
                match event {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        info!("Listening on {}", address);
                    }

                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        info!("Connected to peer: {}", peer_id);
                    }

                    SwarmEvent::ConnectionClosed { peer_id, .. } => {
                        info!("Disconnected from peer: {}", peer_id);
                        peers.lock().await.remove(&peer_id);
                        let _ = event_tx.send(NetworkEvent::PeerDisconnected { peer_id }).await;
                    }

                    SwarmEvent::Behaviour(WolfpackBehaviourEvent::Mdns(event)) => {
                        handle_mdns_event(&mut swarm, event, &mut discovered_peers, &event_tx).await;
                    }

                    SwarmEvent::Behaviour(WolfpackBehaviourEvent::Kademlia(event)) => {
                        if enable_dht {
                            handle_kademlia_event(event);
                        }
                    }

                    SwarmEvent::Behaviour(WolfpackBehaviourEvent::Identify(event)) => {
                        handle_identify_event(&mut swarm, event, &peers, &event_tx, enable_dht).await;
                    }

                    SwarmEvent::Behaviour(WolfpackBehaviourEvent::Sync(event)) => {
                        handle_sync_event(&mut swarm, event, &event_tx).await;
                    }

                    SwarmEvent::Behaviour(WolfpackBehaviourEvent::Ping(event)) => {
                        debug!("Ping event: {:?}", event);
                    }

                    SwarmEvent::Behaviour(WolfpackBehaviourEvent::RelayClient(event)) => {
                        debug!("Relay client event: {:?}", event);
                    }

                    SwarmEvent::Behaviour(WolfpackBehaviourEvent::Dcutr(event)) => {
                        debug!("DCUtR event: {:?}", event);
                    }

                    _ => {}
                }
            }

            // Handle commands from application
            Some(cmd) = command_rx.recv() => {
                handle_command(&mut swarm, cmd).await;
            }
        }
    }
}

#[allow(clippy::cognitive_complexity)] // mDNS event handling with peer management
async fn handle_mdns_event(
    swarm: &mut Swarm<WolfpackBehaviour>,
    event: mdns::Event,
    discovered_peers: &mut HashSet<PeerId>,
    event_tx: &mpsc::Sender<NetworkEvent>,
) {
    match event {
        mdns::Event::Discovered(peers) => {
            for (peer_id, addr) in peers {
                if discovered_peers.insert(peer_id) {
                    info!("mDNS discovered peer: {} at {}", peer_id, addr);
                    swarm
                        .behaviour_mut()
                        .kademlia
                        .add_address(&peer_id, addr.clone());
                    if let Err(e) = swarm.dial(addr) {
                        warn!("Failed to dial discovered peer: {}", e);
                    }
                    let _ = event_tx
                        .send(NetworkEvent::PeerDiscovered {
                            peer_id,
                            device_name: None,
                        })
                        .await;
                }
            }
        }
        mdns::Event::Expired(peers) => {
            for (peer_id, _) in peers {
                discovered_peers.remove(&peer_id);
                debug!("mDNS peer expired: {}", peer_id);
            }
        }
    }
}

#[allow(clippy::cognitive_complexity)] // Kademlia event logging
fn handle_kademlia_event(event: kad::Event) {
    match event {
        kad::Event::RoutingUpdated { peer, .. } => {
            debug!("Kademlia routing updated for peer: {}", peer);
        }
        kad::Event::OutboundQueryProgressed { result, .. } => {
            debug!("Kademlia query progress: {:?}", result);
        }
        _ => {}
    }
}

async fn handle_identify_event(
    swarm: &mut Swarm<WolfpackBehaviour>,
    event: identify::Event,
    peers: &Arc<Mutex<HashMap<PeerId, String>>>,
    event_tx: &mpsc::Sender<NetworkEvent>,
    enable_dht: bool,
) {
    if let identify::Event::Received { peer_id, info, .. } = event {
        debug!("Identified peer {}: {:?}", peer_id, info.protocol_version);

        // Add addresses to Kademlia
        if enable_dht {
            for addr in info.listen_addrs {
                swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
            }
        }

        // Extract device name from agent version if available
        let device_name = info.agent_version.clone();
        peers.lock().await.insert(peer_id, device_name.clone());

        let _ = event_tx
            .send(NetworkEvent::PeerDiscovered {
                peer_id,
                device_name: Some(device_name),
            })
            .await;
    }
}

#[allow(clippy::cognitive_complexity)] // Request-response event handler
async fn handle_sync_event(
    swarm: &mut Swarm<WolfpackBehaviour>,
    event: request_response::Event<SyncRequest, SyncResponse>,
    event_tx: &mpsc::Sender<NetworkEvent>,
) {
    match event {
        request_response::Event::Message { peer, message } => {
            handle_sync_message(swarm, peer, message, event_tx).await;
        }
        request_response::Event::OutboundFailure { peer, error, .. } => {
            warn!("Outbound request to {} failed: {:?}", peer, error);
        }
        request_response::Event::InboundFailure { peer, error, .. } => {
            warn!("Inbound request from {} failed: {:?}", peer, error);
        }
        request_response::Event::ResponseSent { .. } => {}
    }
}

async fn handle_sync_message(
    swarm: &mut Swarm<WolfpackBehaviour>,
    peer: PeerId,
    message: request_response::Message<SyncRequest, SyncResponse>,
    event_tx: &mpsc::Sender<NetworkEvent>,
) {
    match message {
        request_response::Message::Request {
            request_id,
            request,
            channel,
        } => {
            debug!("Received request from {}: {:?}", peer, request);
            handle_sync_request(swarm, peer, request_id, request, channel, event_tx).await;
        }
        request_response::Message::Response { response, .. } => {
            handle_sync_response(peer, response, event_tx).await;
        }
    }
}

#[allow(clippy::too_many_arguments)] // Protocol handler requires all parameters
async fn handle_sync_request(
    swarm: &mut Swarm<WolfpackBehaviour>,
    peer: PeerId,
    request_id: request_response::InboundRequestId,
    request: SyncRequest,
    channel: request_response::ResponseChannel<SyncResponse>,
    event_tx: &mpsc::Sender<NetworkEvent>,
) {
    match request {
        SyncRequest::GetClock => {
            handle_get_clock(peer, request_id, event_tx).await;
        }
        SyncRequest::GetEvents { clock } => {
            handle_get_events(peer, request_id, clock, event_tx).await;
        }
        SyncRequest::PushEvents { events } => {
            handle_push_events(swarm, peer, events, channel, event_tx).await;
        }
        SyncRequest::SendTab {
            url,
            title,
            from_device,
        } => {
            let tab = TabData {
                url,
                title,
                from_device,
            };
            handle_send_tab(swarm, peer, tab, channel, event_tx).await;
        }
    }
}

async fn handle_get_clock(
    peer: PeerId,
    request_id: request_response::InboundRequestId,
    event_tx: &mpsc::Sender<NetworkEvent>,
) {
    let _ = event_tx
        .send(NetworkEvent::ClockRequested {
            from: peer,
            request_id,
        })
        .await;
}

async fn handle_get_events(
    peer: PeerId,
    request_id: request_response::InboundRequestId,
    clock: HashMap<String, u64>,
    event_tx: &mpsc::Sender<NetworkEvent>,
) {
    let _ = event_tx
        .send(NetworkEvent::EventsRequested {
            from: peer,
            request_id,
            clock,
        })
        .await;
}

async fn handle_push_events(
    swarm: &mut Swarm<WolfpackBehaviour>,
    peer: PeerId,
    events: Vec<EncryptedEvent>,
    channel: request_response::ResponseChannel<SyncResponse>,
    event_tx: &mpsc::Sender<NetworkEvent>,
) {
    let count = events.len();
    let _ = event_tx
        .send(NetworkEvent::EventsReceived { from: peer, events })
        .await;
    let _ = swarm
        .behaviour_mut()
        .sync
        .send_response(channel, SyncResponse::Ack { count });
}

/// Tab data for send_tab requests
struct TabData {
    url: String,
    title: Option<String>,
    from_device: String,
}

async fn handle_send_tab(
    swarm: &mut Swarm<WolfpackBehaviour>,
    peer: PeerId,
    tab: TabData,
    channel: request_response::ResponseChannel<SyncResponse>,
    event_tx: &mpsc::Sender<NetworkEvent>,
) {
    let _ = event_tx
        .send(NetworkEvent::TabReceived {
            from: peer,
            url: tab.url,
            title: tab.title,
            from_device: tab.from_device,
        })
        .await;
    let _ = swarm
        .behaviour_mut()
        .sync
        .send_response(channel, SyncResponse::TabReceived);
}

async fn handle_sync_response(
    peer: PeerId,
    response: SyncResponse,
    event_tx: &mpsc::Sender<NetworkEvent>,
) {
    debug!("Received response from {}: {:?}", peer, response);
    if let SyncResponse::Events { events } = response {
        let _ = event_tx
            .send(NetworkEvent::EventsReceived { from: peer, events })
            .await;
    }
}

#[allow(clippy::cognitive_complexity)] // Command handler with many variants
#[allow(clippy::too_many_lines)] // Complete command handling
async fn handle_command(swarm: &mut Swarm<WolfpackBehaviour>, cmd: NetworkCommand) {
    match cmd {
        NetworkCommand::GetClock { peer_id } => {
            swarm
                .behaviour_mut()
                .sync
                .send_request(&peer_id, SyncRequest::GetClock);
        }

        NetworkCommand::GetEvents { peer_id, clock } => {
            swarm
                .behaviour_mut()
                .sync
                .send_request(&peer_id, SyncRequest::GetEvents { clock });
        }

        NetworkCommand::PushEvents { peer_id, events } => {
            swarm
                .behaviour_mut()
                .sync
                .send_request(&peer_id, SyncRequest::PushEvents { events });
        }

        NetworkCommand::SendTab {
            peer_id,
            url,
            title,
            from_device,
        } => {
            swarm.behaviour_mut().sync.send_request(
                &peer_id,
                SyncRequest::SendTab {
                    url,
                    title,
                    from_device,
                },
            );
        }

        NetworkCommand::RespondClock {
            request_id,
            clock,
            device_id: _,
            device_name: _,
        } => {
            // Note: We'd need to store the response channel to respond later
            // This is a simplification - in practice you'd need to track pending requests
            debug!(
                "Would respond to clock request {:?} with {:?}",
                request_id, clock
            );
        }

        NetworkCommand::RespondEvents { request_id, events } => {
            debug!(
                "Would respond to events request {:?} with {} events",
                request_id,
                events.len()
            );
        }

        NetworkCommand::Dial { addr } => {
            if let Err(e) = swarm.dial(addr.clone()) {
                error!("Failed to dial {}: {}", addr, e);
            }
        }

        NetworkCommand::AddBootstrapPeer { peer_id, addr } => {
            swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
            if let Err(e) = swarm.behaviour_mut().kademlia.bootstrap() {
                warn!("Kademlia bootstrap failed: {}", e);
            }
        }
    }
}
