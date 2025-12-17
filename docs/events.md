# Event System

Wolfpack uses event sourcing as its core data model. All changes to browser state are represented as immutable events that sync between devices. This document provides a comprehensive reference for the event system.

## Why Event Sourcing?

Traditional sync approaches snapshot entire state and merge differences. This fails with:
- Concurrent offline edits (which change wins?)
- Conflicting changes (data loss)
- Large state sizes (expensive diffs)

Event sourcing records each change as an immutable fact. Events are never modified or deleted. Current state is derived by replaying events in causal order.

Benefits:
- **Conflict-free**: Concurrent events are both applied in deterministic order
- **Auditable**: Full history of all changes
- **Efficient sync**: Only send new events, not full state
- **Offline support**: Create events while disconnected, merge when online

## Event Envelope

Every event is wrapped in an envelope containing metadata:

```rust
struct EventEnvelope {
    id: Uuid,               // Unique UUID v7 (time-sortable)
    timestamp: DateTime<Utc>, // When the event was created
    device: String,         // Device ID that created the event
    clock: VectorClock,     // Causal ordering information
    event: Event,           // The actual event payload
}
```

### Example JSON

```json
{
  "id": "01912345-6789-7abc-def0-123456789abc",
  "timestamp": "2024-01-15T10:30:00.000Z",
  "device": "laptop-abc123",
  "clock": {
    "laptop-abc123": 42,
    "desktop-def456": 38
  },
  "event": {
    "type": "ExtensionInstalled",
    "data": {
      "id": "ublock@gorhill.org",
      "name": "uBlock Origin",
      "version": "1.55.0",
      "source": {
        "type": "Git",
        "url": "https://github.com/gorhill/uBlock",
        "ref_spec": "1.55.0",
        "build_cmd": "npm run build"
      },
      "xpi_data": "KLUv/QBYLA..."
    }
  }
}
```

## Event Types

### Extension Events

#### ExtensionAdded (Legacy)

Tracks extension presence without distributing the XPI. Used when extensions are installed through the browser UI.

```json
{
  "type": "ExtensionAdded",
  "data": {
    "id": "uBlock0@raymondhill.net",
    "name": "uBlock Origin",
    "url": "https://addons.mozilla.org/firefox/addon/ublock-origin/"
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `id` | String | Extension ID from manifest.json |
| `name` | String | Human-readable extension name |
| `url` | String? | Optional URL where extension was obtained |

#### ExtensionRemoved (Legacy)

Marks an extension as removed (legacy tracking only).

```json
{
  "type": "ExtensionRemoved",
  "data": {
    "id": "uBlock0@raymondhill.net"
  }
}
```

#### ExtensionInstalled

Full extension distribution with compressed XPI data. Created when using `wolfpack extension install`.

```json
{
  "type": "ExtensionInstalled",
  "data": {
    "id": "ublock@gorhill.org",
    "name": "uBlock Origin",
    "version": "1.55.0",
    "source": {
      "type": "Git",
      "url": "https://github.com/gorhill/uBlock",
      "ref_spec": "1.55.0",
      "build_cmd": "npm run build"
    },
    "xpi_data": "KLUv/QBYLAoA..."
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `id` | String | Extension ID from manifest.json |
| `name` | String | Human-readable extension name |
| `version` | String | Extension version |
| `source` | ExtensionSource | Where the extension came from |
| `xpi_data` | String | Zstd-compressed XPI, base64 encoded |

**ExtensionSource variants:**

```json
// From git repository
{
  "type": "Git",
  "url": "https://github.com/user/repo",
  "ref_spec": "main",
  "build_cmd": "npm run build"
}

// From addons.mozilla.org (future)
{
  "type": "Amo",
  "amo_slug": "ublock-origin"
}

// From local XPI file
{
  "type": "Local",
  "original_path": "/path/to/extension.xpi"
}
```

#### ExtensionUninstalled

Removes an extension that was installed via `ExtensionInstalled`.

```json
{
  "type": "ExtensionUninstalled",
  "data": {
    "id": "ublock@gorhill.org"
  }
}
```

### Container Events

Multi-Account Containers (Firefox/LibreWolf feature).

#### ContainerAdded

```json
{
  "type": "ContainerAdded",
  "data": {
    "id": "4",
    "name": "Shopping",
    "color": "pink",
    "icon": "cart"
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `id` | String | Container user context ID |
| `name` | String | Display name |
| `color` | String | Color name (blue, turquoise, green, yellow, orange, red, pink, purple) |
| `icon` | String | Icon name (fingerprint, briefcase, dollar, cart, vacation, gift, food, fruit, pet, tree, chill, circle, fence) |

#### ContainerRemoved

```json
{
  "type": "ContainerRemoved",
  "data": {
    "id": "4"
  }
}
```

#### ContainerUpdated

Partial update to container properties.

```json
{
  "type": "ContainerUpdated",
  "data": {
    "id": "4",
    "name": "Online Shopping",
    "color": null,
    "icon": null
  }
}
```

Fields set to `null` retain their current value.

### Handler Events

Protocol handlers (mailto:, magnet:, etc.).

#### HandlerSet

```json
{
  "type": "HandlerSet",
  "data": {
    "protocol": "mailto",
    "handler": "https://mail.example.com/compose?to=%s"
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `protocol` | String | Protocol scheme (mailto, magnet, irc, etc.) |
| `handler` | String | Handler URL template with %s placeholder |

#### HandlerRemoved

```json
{
  "type": "HandlerRemoved",
  "data": {
    "protocol": "mailto"
  }
}
```

### Search Engine Events

#### SearchEngineAdded

```json
{
  "type": "SearchEngineAdded",
  "data": {
    "id": "ddg",
    "name": "DuckDuckGo",
    "url": "https://duckduckgo.com/?q=%s"
  }
}
```

#### SearchEngineRemoved

```json
{
  "type": "SearchEngineRemoved",
  "data": {
    "id": "ddg"
  }
}
```

#### SearchEngineDefault

Sets the default search engine.

```json
{
  "type": "SearchEngineDefault",
  "data": {
    "id": "ddg"
  }
}
```

### Preference Events

User preferences (about:config values).

#### PrefSet

```json
{
  "type": "PrefSet",
  "data": {
    "key": "browser.startup.homepage",
    "value": "https://example.com"
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `key` | String | Preference name |
| `value` | PrefValue | Boolean, integer, or string |

**PrefValue examples:**

```json
// Boolean
{ "key": "browser.tabs.warnOnClose", "value": true }

// Integer
{ "key": "browser.startup.page", "value": 3 }

// String
{ "key": "browser.startup.homepage", "value": "https://example.com" }
```

#### PrefRemoved

```json
{
  "type": "PrefRemoved",
  "data": {
    "key": "browser.startup.homepage"
  }
}
```

### Tab Events

Send tabs between devices.

#### TabSent

```json
{
  "type": "TabSent",
  "data": {
    "to_device": "desktop-def456",
    "url": "https://example.com/article",
    "title": "Interesting Article"
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `to_device` | String | Target device ID |
| `url` | String | Page URL |
| `title` | String? | Optional page title |

#### TabReceived

Acknowledges receipt of a tab.

```json
{
  "type": "TabReceived",
  "data": {
    "event_id": "01912345-6789-7abc-def0-123456789abc"
  }
}
```

## Vector Clocks

Vector clocks provide causal ordering without synchronized time.

### Structure

```json
{
  "laptop-abc123": 42,
  "desktop-def456": 38,
  "phone-ghi789": 15
}
```

Each entry tracks the highest counter seen from that device.

### Operations

**Increment (when creating an event):**
```
Before: { "laptop": 5, "desktop": 3 }
After:  { "laptop": 6, "desktop": 3 }  // laptop incremented
```

**Merge (when receiving events):**
```
Local:    { "laptop": 6, "desktop": 3 }
Incoming: { "laptop": 4, "desktop": 5 }
Merged:   { "laptop": 6, "desktop": 5 }  // max of each
```

### Causal Ordering

Events can be compared using vector clocks:

- **A happened-before B**: A's clock ≤ B's clock (all entries)
- **B happened-before A**: B's clock ≤ A's clock
- **Concurrent**: Neither dominates (independent events)

### Conflict Resolution

When events are concurrent, deterministic tiebreakers ensure all devices reach the same order:

1. Sum of all clock values (higher = later)
2. Timestamp comparison
3. Device ID lexicographic order

## Nonce Derivation

Encryption nonces are derived from vector clocks to prevent reuse:

```
Nonce = hash(device_id) || counter
```

For AES-256-GCM (12 bytes):
```
┌────────────────┬────────────────────────┐
│ Device Hash    │ Counter (big-endian)   │
│ (4 bytes)      │ (8 bytes)              │
└────────────────┴────────────────────────┘
```

For XChaCha20-Poly1305 (24 bytes):
```
┌────────────────┬────────────────────────┬──────────────┐
│ Device Hash    │ Counter (big-endian)   │ Padding      │
│ (8 bytes)      │ (8 bytes)              │ (8 bytes)    │
└────────────────┴────────────────────────┴──────────────┘
```

Since counters only increase, nonce reuse is impossible.

## Event File Format

Events are stored in encrypted files with this structure:

```
┌───────────┬─────────┬──────────────┬──────┬────────────┐
│  Version  │ Cipher  │  Public Key  │Nonce │ Ciphertext │
│  (1 byte) │(1 byte) │  (32 bytes)  │(var) │   (var)    │
└───────────┴─────────┴──────────────┴──────┴────────────┘
```

| Field | Size | Description |
|-------|------|-------------|
| Version | 1 byte | Format version (currently 2) |
| Cipher | 1 byte | 1 = AES-256-GCM, 2 = XChaCha20-Poly1305 |
| Public Key | 32 bytes | Sender's X25519 public key |
| Nonce | 12 or 24 bytes | Derived from vector clock |
| Ciphertext | Variable | Encrypted JSON array of EventEnvelopes |

## State Materialization

Events are permanent, but current state is materialized into SQLite for efficient queries:

```
Events (append-only)          State (materialized)
──────────────────────        ────────────────────
ExtensionInstalled A      →   extensions: [A]
ExtensionInstalled B      →   extensions: [A, B]
ExtensionUninstalled A    →   extensions: [B]
```

### Database Tables

```sql
-- Applied events (deduplication)
CREATE TABLE applied_events (
    id TEXT PRIMARY KEY,
    device TEXT NOT NULL,
    timestamp TEXT NOT NULL
);

-- Synced extensions
CREATE TABLE extensions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    url TEXT,
    added_at TEXT NOT NULL
);

-- Extension XPI data
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

## Idempotency

Event application is idempotent. Applying the same event twice has no additional effect:

1. Check if `event.id` exists in `applied_events` table
2. If yes, skip
3. If no, apply event and record ID

This ensures:
- Network retries don't cause duplicates
- Events can be safely re-sent
- State converges regardless of receive order

## Event Lifecycle

```
Create Event
     │
     ▼
Wrap in Envelope (add ID, timestamp, clock)
     │
     ▼
Store Locally (materialize to SQLite)
     │
     ▼
Encrypt (derive nonce from clock)
     │
     ▼
Transmit to Peers (via libp2p)
     │
     ▼
Peer Receives
     │
     ▼
Decrypt (verify nonce)
     │
     ▼
Check Duplicate (by event ID)
     │
     ▼
Merge Vector Clock
     │
     ▼
Materialize to State
     │
     ▼
Apply to Profile (if browser not running)
```
