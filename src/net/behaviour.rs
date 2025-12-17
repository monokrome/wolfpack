use libp2p::{dcutr, identify, kad, mdns, ping, relay, request_response, swarm::NetworkBehaviour};
use std::time::Duration;

use super::protocol::{PROTOCOL_NAME, SyncCodec};

/// Combined network behaviour for wolfpack
#[derive(NetworkBehaviour)]
pub struct WolfpackBehaviour {
    /// mDNS for local network discovery
    pub mdns: mdns::tokio::Behaviour,

    /// Kademlia DHT for internet-wide discovery
    pub kademlia: kad::Behaviour<kad::store::MemoryStore>,

    /// Relay client for NAT traversal
    pub relay_client: relay::client::Behaviour,

    /// DCUtR for direct connection upgrade through relay
    pub dcutr: dcutr::Behaviour,

    /// Identify protocol for peer metadata exchange
    pub identify: identify::Behaviour,

    /// Ping for connection liveness
    pub ping: ping::Behaviour,

    /// Request-response for sync protocol
    pub sync: request_response::Behaviour<SyncCodec>,
}

impl WolfpackBehaviour {
    pub fn new(
        local_key: &libp2p::identity::Keypair,
        relay_client: relay::client::Behaviour,
    ) -> Self {
        let local_peer_id = local_key.public().to_peer_id();

        // mDNS for local discovery
        #[allow(clippy::expect_used)] // mDNS is critical - fail fast if unavailable
        let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), local_peer_id)
            .expect("Failed to create mDNS behaviour");

        // Kademlia DHT
        let store = kad::store::MemoryStore::new(local_peer_id);
        let mut kademlia = kad::Behaviour::new(local_peer_id, store);
        kademlia.set_mode(Some(kad::Mode::Client));

        // Identify protocol
        let identify = identify::Behaviour::new(identify::Config::new(
            "/wolfpack/id/1.0.0".to_string(),
            local_key.public(),
        ));

        // Ping
        let ping = ping::Behaviour::new(ping::Config::new().with_interval(Duration::from_secs(30)));

        // DCUtR for hole punching
        let dcutr = dcutr::Behaviour::new(local_peer_id);

        // Sync request-response protocol
        let sync = request_response::Behaviour::new(
            [(PROTOCOL_NAME, request_response::ProtocolSupport::Full)],
            request_response::Config::default().with_request_timeout(Duration::from_secs(30)),
        );

        Self {
            mdns,
            kademlia,
            relay_client,
            dcutr,
            identify,
            ping,
            sync,
        }
    }
}
