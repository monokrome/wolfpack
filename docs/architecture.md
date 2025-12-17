# Architecture

## Overview

Wolfpack uses an event-sourced architecture with peer-to-peer networking. All changes are represented as immutable events that sync directly between devices using libp2p.

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   LibreWolf     │     │    Wolfpack     │     │    libp2p       │
│    Profile      │◄───►│     Daemon      │◄───►│   P2P Network   │
└─────────────────┘     └─────────────────┘     └─────────────────┘
                               │
                               ▼
                        ┌─────────────────┐
                        │   SQLite DB     │
                        │  (local state)  │
                        └─────────────────┘
```

**Related documentation:**
- [Protocol Specification](protocol.md) - Wire format and interoperability details
- [Events Reference](events.md) - Complete event type documentation
- [Extensions](extensions.md) - Building and syncing extensions from source

## P2P Networking

Wolfpack uses libp2p for all network communication:

### Transport

- **TCP**: Primary transport with Noise encryption
- **QUIC**: Alternative transport (faster, handles NAT better)
- **Yamux**: Stream multiplexing over connections

### Discovery

- **mDNS**: Automatic local network discovery (default)
- **Kademlia DHT**: Internet-wide peer discovery (opt-in)

### NAT Traversal

- **AutoNAT**: Detects NAT situation
- **DCUtR**: Direct Connection Upgrade through Relay
- **Circuit Relay v2**: Fallback when direct connection fails

### Sync Protocol

Custom request-response protocol over libp2p:

```
GET_CLOCK        → Returns device's vector clock
GET_EVENTS(clock) → Returns events newer than clock
PUSH_EVENTS(events) → Pushes events to peer
SEND_TAB(url, title) → Sends a tab to peer
```

## Event Sourcing

### Why Events?

Traditional sync approaches snapshot entire state and try to merge differences. This breaks down with:
- Concurrent offline edits
- Conflicting changes
- Large state sizes

Event sourcing instead records each change as an immutable fact:

```
Event: ExtensionInstalled
  extension_id: "ublock-origin@gorhill.org"
  timestamp: 2024-01-15T10:30:00Z
  device: "laptop"
  clock: {laptop: 42, desktop: 38}
```

### Event Types

| Event | Description |
|-------|-------------|
| `ExtensionAdded` | Extension tracked (legacy, no XPI data) |
| `ExtensionRemoved` | Extension tracking removed (legacy) |
| `ExtensionInstalled` | Extension with full XPI data synced |
| `ExtensionUninstalled` | Extension and XPI data removed |
| `ContainerAdded` | Multi-Account Container created |
| `ContainerUpdated` | Container properties changed |
| `ContainerRemoved` | Container deleted |
| `HandlerSet` | Protocol handler registered/updated |
| `HandlerRemoved` | Protocol handler unregistered |
| `SearchEngineAdded` | Search engine added |
| `SearchEngineRemoved` | Search engine removed |
| `SearchEngineDefault` | Default search engine changed |
| `PrefSet` | User preference set/changed |
| `PrefRemoved` | User preference removed |
| `TabSent` | Tab sent to specific device |
| `TabReceived` | Tab receipt acknowledged |

See [events.md](events.md) for complete event documentation.

### Event Envelope

Every event is wrapped in an envelope containing metadata:

```rust
struct EventEnvelope {
    id: Uuid,               // Unique event ID (UUID v7)
    timestamp: DateTime<Utc>, // When event was created
    device: String,         // Originating device ID
    clock: VectorClock,     // Causal ordering
    event: Event,           // The event payload
}
```

See [events.md](events.md) for complete event format and JSON examples.

## Vector Clocks

Vector clocks provide causal ordering across distributed devices without requiring synchronized time.

### How They Work

Each device maintains a counter. When creating an event:
1. Increment own counter
2. Include full vector clock in event

When receiving events:
1. Merge incoming clock with local clock
2. Take max of each device's counter

### Example

```
Device A: {A: 0, B: 0}
Device B: {A: 0, B: 0}

A creates event → {A: 1, B: 0}
B creates event → {A: 0, B: 1}

A receives B's event → merge → {A: 1, B: 1}
B receives A's event → merge → {A: 1, B: 1}

Both now have consistent ordering
```

### Conflict Resolution

When events are concurrent (neither happened-before the other), we use deterministic tiebreakers:
1. Compare vector clock sums
2. Compare timestamps
3. Compare device IDs lexicographically

This ensures all devices arrive at the same order.

## State Materialization

Events are stored permanently, but we materialize current state into SQLite for fast queries:

```
Events (append-only)     State (materialized)
─────────────────────    ─────────────────────
ExtensionInstalled A  →  extensions: [A, B]
ExtensionInstalled B  →
ExtensionRemoved A    →  extensions: [B]
```

### Database Schema

```sql
-- Applied events (deduplication)
CREATE TABLE applied_events (
    id TEXT PRIMARY KEY,
    device TEXT NOT NULL,
    timestamp TEXT NOT NULL
);

-- Extensions metadata
CREATE TABLE extensions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    url TEXT,
    added_at TEXT NOT NULL
);

-- Extension XPI data (compressed, base64)
CREATE TABLE extension_xpi (
    id TEXT PRIMARY KEY,
    version TEXT NOT NULL,
    source_type TEXT NOT NULL,
    source_data TEXT NOT NULL,
    xpi_data TEXT NOT NULL,
    installed_at TEXT NOT NULL
);

-- Multi-Account Containers
CREATE TABLE containers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    color TEXT NOT NULL,
    icon TEXT NOT NULL
);

-- Protocol handlers
CREATE TABLE handlers (
    protocol TEXT PRIMARY KEY,
    handler TEXT NOT NULL
);

-- Search engines
CREATE TABLE search_engines (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    url TEXT NOT NULL,
    is_default INTEGER NOT NULL DEFAULT 0
);

-- User preferences
CREATE TABLE prefs (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    value_type TEXT NOT NULL
);

-- Pending tabs to open
CREATE TABLE pending_tabs (
    id TEXT PRIMARY KEY,
    url TEXT NOT NULL,
    title TEXT,
    sent_by TEXT NOT NULL,
    sent_at TEXT NOT NULL
);

-- Vector clock state
CREATE TABLE vector_clock (
    device TEXT PRIMARY KEY,
    counter INTEGER NOT NULL
);
```

## Encrypted Event Format

Events are encrypted before transmission:

```
┌───────────┬─────────┬──────────────┬──────┬────────────┐
│  Version  │ Cipher  │  Public Key  │Nonce │ Ciphertext │
│  (1 byte) │(1 byte) │  (32 bytes)  │(var) │   (var)    │
└───────────┴─────────┴──────────────┴──────┴────────────┘
```

- **Version**: Format version (currently 2)
- **Cipher**: 1 = AES-256-GCM, 2 = XChaCha20-Poly1305
- **Public Key**: Sender's X25519 public key
- **Nonce**: 12 bytes (AES-GCM) or 24 bytes (XChaCha20)
- **Ciphertext**: Encrypted JSON array of events

## Directory Structure

```
~/.local/share/wolfpack/
├── config.toml          # Configuration
├── sync/
│   ├── state.db         # SQLite state database
│   ├── events/          # Encrypted event files
│   │   └── {device-id}/ # Events from each device
│   ├── keys/            # Public keys from paired devices
│   └── pending_events/  # Events waiting to be synced (from CLI)
└── keys/
    └── local.key        # Private key (never shared)
```

## Daemon Architecture

The daemon runs as a background process with several concurrent tasks:

### 1. P2P Network Loop

Handles libp2p swarm events:
- Peer discovery (mDNS, DHT)
- Connection management
- Sync protocol messages

### 2. Profile Watcher

Monitors LibreWolf profile for changes:
- `extensions.json` - Extension installs/removals
- `containers.json` - Container changes
- `handlers.json` - Protocol handlers
- `search.json.mozlz4` - Search engines
- `prefs.js` / `user.js` - Preferences

Uses `inotify` (Linux) / `FSEvents` (macOS) for efficient watching.

### 3. Extension Manager

Handles extension installation and removal:
- Monitors for pending extension installs from CLI
- Installs XPIs from database to profile
- Removes uninstalled extensions from profile
- See [extensions.md](extensions.md) for details

### 4. IPC Handler

Unix socket for CLI commands:
- `status` - Report sync state
- `peers` - List connected peers
- `tabs` - List pending tabs
- `send <device> <url>` - Queue tab send

## Sync Flow

### Outgoing Changes

```
Profile Change Detected
        │
        ▼
  Read Current State
        │
        ▼
   Diff with DB State
        │
        ▼
  Generate Events
        │
        ▼
 Update Vector Clock
        │
        ▼
    Encrypt Events
        │
        ▼
  Push to Connected Peers
```

### Incoming Changes

```
Events Received from Peer
        │
        ▼
   Decrypt Events
        │
        ▼
 Check for Duplicates
        │
        ▼
  Merge Vector Clock
        │
        ▼
 Materialize to SQLite
        │
        ▼
  Is Browser Running?
    ├── Yes: Queue Writes
    └── No: Apply Immediately
        │
        ▼
 Install Pending Extensions
        │
        ▼
 Remove Uninstalled Extensions
```

See [protocol.md](protocol.md) for complete wire format and sync algorithm.

## Browser Lock Detection

LibreWolf locks its profile when running. Wolfpack detects this via:
- Lock file presence (`lock` / `.parentlock`)
- Profile directory modification timestamps

When the browser is running, profile writes are queued. When it closes, the queue is flushed.
