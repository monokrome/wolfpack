# wolfpack

Sync LibreWolf browser data across devices with end-to-end encryption using peer-to-peer networking.

## Features

- **Event-sourced sync**: Changes are tracked as events, not state snapshots
- **E2E encryption**: AES-256-GCM (hardware-accelerated) with XChaCha20-Poly1305 fallback
- **P2P networking**: Built on libp2p with automatic peer discovery
- **Build extensions from source**: Install extensions directly from git repositories
- **Privacy-first**: No cloud services, no telemetry
- **Single binary**: Everything runs in one daemon

### What syncs

- Extensions (full XPI distribution from source builds)
- Multi-Account Containers
- Protocol handlers
- Search engines
- User preferences (whitelisted)
- Send-tab between devices

## Installation

```bash
# Build from source
cargo build --release

# Install to ~/.cargo/bin
cargo install --path .
```

## Quick Start

```bash
# Initialize on each device
wolfpack init --name "laptop"

# Start the daemon
wolfpack daemon
```

### Pairing Devices

To sync between devices, they need to be paired:

```bash
# On device A: Start pairing session
wolfpack pair
# Displays a 6-digit code like: 123456

# On device B: Join with the code
wolfpack pair --code 123456

# Device A will prompt to accept the connection
# Type 'y' to accept
```

Once paired, devices discover each other automatically on the local network via mDNS.

### Using Wolfpack

```bash
# Check daemon status and connected peers
wolfpack status

# Send a tab to another device
wolfpack send "https://example.com" --to desktop

# List paired devices
wolfpack devices
```

## How It Works

1. **Daemon starts** and begins P2P networking
2. **mDNS discovery** finds devices on local network automatically
3. **Optional DHT** enables internet-wide discovery (opt-in)
4. **Profile watcher** monitors LibreWolf for changes
5. **Changes become events** with vector clock timestamps
6. **Events sync** directly between peers with E2E encryption

## Configuration

Config file: `~/.local/share/wolfpack/config.toml`

```toml
[device]
id = "01234567-89ab-cdef-0123-456789abcdef"
name = "laptop"

[paths]
profile = "/home/user/.librewolf/xxxxxxxx.default-release"

[sync]
# Port for P2P connections (0 for random)
listen_port = 0
# Enable DHT for internet-wide discovery (default: false)
enable_dht = false
# Bootstrap peers for DHT (when enabled)
bootstrap_peers = []

[api]
# HTTP API port for pairing and browser extension communication
port = 9778

[prefs]
# Preferences to sync (others are ignored)
whitelist = [
    "browser.startup.homepage",
    "browser.newtabpage.enabled",
]
```

## Commands

| Command | Description |
|---------|-------------|
| `wolfpack init [--name NAME]` | Initialize wolfpack on this device |
| `wolfpack daemon` | Run the sync daemon |
| `wolfpack pair` | Start a pairing session (displays 6-digit code) |
| `wolfpack pair --code CODE` | Join a pairing session with a code |
| `wolfpack devices` | List paired devices |
| `wolfpack send URL --to DEVICE` | Send a tab to another device |
| `wolfpack status` | Show daemon and sync status |
| `wolfpack extension list [--missing]` | List synced extensions |
| `wolfpack extension install URL` | Install extension from git or XPI |
| `wolfpack extension uninstall ID` | Uninstall an extension |

### Extension Installation

Build and sync extensions from source:

```bash
# Install from git repository
wolfpack extension install https://github.com/user/extension-repo

# With specific version
wolfpack extension install https://github.com/user/extension-repo --ref v1.2.3

# From local XPI
wolfpack extension install /path/to/extension.xpi
```

The extension is built, compressed, and synced to all paired devices automatically.

## Networking

Wolfpack uses libp2p for peer-to-peer networking:

- **mDNS**: Automatic discovery on local network (default)
- **Kademlia DHT**: Internet-wide discovery (opt-in via `enable_dht = true`)
- **NAT traversal**: Automatic hole punching via DCUtR protocol
- **Relay fallback**: Circuit relay when direct connection fails

No external servers required for local network sync. For internet sync, you can use public DHT bootstrap nodes or run your own.

## Security

- X25519 key exchange for device pairing
- AES-256-GCM encryption (uses hardware AES-NI when available)
- XChaCha20-Poly1305 fallback for devices without AES hardware
- Deterministic nonces derived from vector clock (no random nonce collisions)
- Transport encryption via libp2p Noise protocol

See [docs/security.md](docs/security.md) for details.

## Architecture

- **Event sourcing**: All changes are immutable events
- **Vector clocks**: Causal ordering across devices
- **CRDT-like merging**: Concurrent changes merge deterministically
- **Offline-first**: Works without network, syncs when peers connect

## Documentation

| Document | Description |
|----------|-------------|
| [docs/architecture.md](docs/architecture.md) | System design and component overview |
| [docs/protocol.md](docs/protocol.md) | Wire format specification (for implementing compatible clients) |
| [docs/events.md](docs/events.md) | Complete event type reference |
| [docs/extensions.md](docs/extensions.md) | Building and syncing extensions from source |
| [docs/security.md](docs/security.md) | Cryptographic design and threat model |
| [docs/configuration.md](docs/configuration.md) | Configuration options |
| [docs/pairing.md](docs/pairing.md) | Device pairing guide |

## License

MIT
