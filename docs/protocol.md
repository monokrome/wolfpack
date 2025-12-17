# Wolfpack Protocol Specification

Version: 1.0

This document specifies the Wolfpack sync protocol in sufficient detail for independent implementation. Any conforming implementation should be able to interoperate with the reference Rust implementation.

## Overview

Wolfpack synchronizes browser state between devices using:
- **Event sourcing**: All changes are immutable events
- **Vector clocks**: Causal ordering without synchronized time
- **E2E encryption**: AES-256-GCM or XChaCha20-Poly1305
- **P2P transport**: libp2p with custom request-response protocol

## Data Types

### Basic Types

| Type | Description | Encoding |
|------|-------------|----------|
| `String` | UTF-8 text | JSON string |
| `u64` | 64-bit unsigned integer | JSON number |
| `bool` | Boolean | JSON true/false |
| `Uuid` | UUID v7 | 36-char string "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx" |
| `DateTime` | UTC timestamp | ISO 8601 "2024-01-15T10:30:00.000Z" |
| `bytes` | Binary data | Base64 string |

### VectorClock

Map of device ID to counter value:

```json
{
  "device-a": 42,
  "device-b": 38
}
```

**Invariants:**
- Counters are non-negative integers
- Counters only increase (never decrease)
- Missing device = counter 0

**Operations:**

```
increment(clock, device):
    clock[device] = clock.get(device, 0) + 1
    return clock

merge(local, remote):
    result = {}
    for device in union(local.keys(), remote.keys()):
        result[device] = max(local.get(device, 0), remote.get(device, 0))
    return result

compare(a, b):
    if all(a[d] <= b[d] for d in union(a.keys(), b.keys())):
        return BEFORE
    if all(b[d] <= a[d] for d in union(a.keys(), b.keys())):
        return AFTER
    return CONCURRENT
```

### EventEnvelope

Every event is wrapped in an envelope:

```json
{
  "id": "01912345-6789-7abc-def0-123456789abc",
  "timestamp": "2024-01-15T10:30:00.000Z",
  "device": "laptop-abc123",
  "clock": {"laptop-abc123": 42, "desktop-def456": 38},
  "event": { ... }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `id` | Uuid | Unique identifier (UUID v7 recommended) |
| `timestamp` | DateTime | Creation time (informational, not used for ordering) |
| `device` | String | Device ID that created this event |
| `clock` | VectorClock | Clock value at event creation |
| `event` | Event | The actual event payload |

## Event Types

Events use tagged JSON encoding:

```json
{
  "type": "EventTypeName",
  "data": { ... }
}
```

### ExtensionAdded

```json
{
  "type": "ExtensionAdded",
  "data": {
    "id": "string (extension ID)",
    "name": "string (display name)",
    "url": "string? (optional source URL)"
  }
}
```

### ExtensionRemoved

```json
{
  "type": "ExtensionRemoved",
  "data": {
    "id": "string"
  }
}
```

### ExtensionInstalled

```json
{
  "type": "ExtensionInstalled",
  "data": {
    "id": "string",
    "name": "string",
    "version": "string",
    "source": ExtensionSource,
    "xpi_data": "string (base64-encoded zstd-compressed XPI)"
  }
}
```

**ExtensionSource variants:**

```json
// Git repository
{"type": "Git", "url": "string", "ref_spec": "string", "build_cmd": "string?"}

// AMO (addons.mozilla.org)
{"type": "Amo", "amo_slug": "string"}

// Local file
{"type": "Local", "original_path": "string"}
```

### ExtensionUninstalled

```json
{
  "type": "ExtensionUninstalled",
  "data": {
    "id": "string"
  }
}
```

### ContainerAdded

```json
{
  "type": "ContainerAdded",
  "data": {
    "id": "string",
    "name": "string",
    "color": "string",
    "icon": "string"
  }
}
```

Valid colors: `blue`, `turquoise`, `green`, `yellow`, `orange`, `red`, `pink`, `purple`

Valid icons: `fingerprint`, `briefcase`, `dollar`, `cart`, `vacation`, `gift`, `food`, `fruit`, `pet`, `tree`, `chill`, `circle`, `fence`

### ContainerRemoved

```json
{
  "type": "ContainerRemoved",
  "data": {
    "id": "string"
  }
}
```

### ContainerUpdated

```json
{
  "type": "ContainerUpdated",
  "data": {
    "id": "string",
    "name": "string?",
    "color": "string?",
    "icon": "string?"
  }
}
```

Null fields indicate no change.

### HandlerSet

```json
{
  "type": "HandlerSet",
  "data": {
    "protocol": "string",
    "handler": "string"
  }
}
```

### HandlerRemoved

```json
{
  "type": "HandlerRemoved",
  "data": {
    "protocol": "string"
  }
}
```

### SearchEngineAdded

```json
{
  "type": "SearchEngineAdded",
  "data": {
    "id": "string",
    "name": "string",
    "url": "string"
  }
}
```

### SearchEngineRemoved

```json
{
  "type": "SearchEngineRemoved",
  "data": {
    "id": "string"
  }
}
```

### SearchEngineDefault

```json
{
  "type": "SearchEngineDefault",
  "data": {
    "id": "string"
  }
}
```

### PrefSet

```json
{
  "type": "PrefSet",
  "data": {
    "key": "string",
    "value": PrefValue
  }
}
```

**PrefValue** is untagged:
- Boolean: `true` or `false`
- Integer: JSON number
- String: JSON string

### PrefRemoved

```json
{
  "type": "PrefRemoved",
  "data": {
    "key": "string"
  }
}
```

### TabSent

```json
{
  "type": "TabSent",
  "data": {
    "to_device": "string",
    "url": "string",
    "title": "string?"
  }
}
```

### TabReceived

```json
{
  "type": "TabReceived",
  "data": {
    "event_id": "uuid"
  }
}
```

## Encryption

### Key Exchange

Devices use X25519 for key agreement:

1. Each device generates an X25519 keypair (32-byte private, 32-byte public)
2. Public keys are exchanged via the pairing protocol (see HTTP API below)
3. Shared secret = X25519(local_private, remote_public)

For N devices, each pair computes the same shared secret via Diffie-Hellman.

**Group secret derivation:**

```python
def derive_group_secret(my_keypair, known_public_keys):
    if not known_public_keys:
        return x25519(my_keypair.private, my_keypair.public)

    combined = [0] * 32
    for public_key in known_public_keys:
        shared = x25519(my_keypair.private, public_key)
        combined = xor(combined, shared)
    return combined
```

### Cipher Selection

| ID | Cipher | Key | Nonce | Tag |
|----|--------|-----|-------|-----|
| 1 | AES-256-GCM | 256-bit | 96-bit (12 bytes) | 128-bit |
| 2 | XChaCha20-Poly1305 | 256-bit | 192-bit (24 bytes) | 128-bit |

Selection rule:
- If CPU supports AES-NI → use AES-256-GCM (faster)
- Otherwise → use XChaCha20-Poly1305 (constant-time)

Both ciphers MUST be supported for decryption.

### Nonce Derivation

Nonces are derived deterministically from the device ID and vector clock counter:

**AES-256-GCM (12 bytes):**
```
nonce[0..4]  = sha256(device_id)[0..4]
nonce[4..12] = counter as big-endian u64
```

**XChaCha20-Poly1305 (24 bytes):**
```
nonce[0..8]   = sha256(device_id)[0..8]
nonce[8..16]  = counter as big-endian u64
nonce[16..24] = 0x00 (padding)
```

This ensures:
- Each (device, counter) pair produces a unique nonce
- Counters never repeat (monotonically increasing)
- No random nonce collisions

### Encrypted Event File Format

```
Offset  Size    Field
------  ----    -----
0       1       Version (0x02)
1       1       Cipher ID (0x01 = AES-GCM, 0x02 = XChaCha20)
2       32      Sender public key (X25519)
34      N       Nonce (12 or 24 bytes depending on cipher)
34+N    M       Ciphertext
34+N+M  16      Authentication tag
```

**Version 2 format (current):**
- Version byte: `0x02`
- Cipher byte: `0x01` (AES-GCM) or `0x02` (XChaCha20)
- Public key: 32 bytes
- Nonce: 12 bytes (AES-GCM) or 24 bytes (XChaCha20)
- Ciphertext: Encrypted JSON array of EventEnvelopes
- Tag: 16-byte authentication tag

### Encryption Process

```python
def encrypt_events(events, my_keypair, known_devices, my_device_id, clock):
    # Serialize
    plaintext = json.dumps([envelope.to_dict() for envelope in events]).encode('utf-8')

    # Derive key
    group_secret = derive_group_secret(my_keypair, known_devices)

    # Select cipher
    cipher_id = 0x01 if has_aesni() else 0x02

    # Derive nonce
    counter = clock[my_device_id]
    if cipher_id == 0x01:
        nonce = sha256(my_device_id)[0:4] + counter.to_bytes(8, 'big')
    else:
        nonce = sha256(my_device_id)[0:8] + counter.to_bytes(8, 'big') + bytes(8)

    # Encrypt
    ciphertext, tag = aead_encrypt(cipher_id, group_secret, nonce, plaintext)

    # Build file
    return bytes([0x02, cipher_id]) + my_keypair.public + nonce + ciphertext + tag
```

### Decryption Process

```python
def decrypt_events(file_data, my_keypair, known_devices):
    version = file_data[0]
    if version != 0x02:
        raise UnsupportedVersion()

    cipher_id = file_data[1]
    sender_public = file_data[2:34]

    nonce_len = 12 if cipher_id == 0x01 else 24
    nonce = file_data[34:34+nonce_len]
    ciphertext = file_data[34+nonce_len:-16]
    tag = file_data[-16:]

    # Derive key (using sender's public key)
    group_secret = derive_group_secret(my_keypair, known_devices)

    # Decrypt
    plaintext = aead_decrypt(cipher_id, group_secret, nonce, ciphertext, tag)

    return json.loads(plaintext.decode('utf-8'))
```

## P2P Protocol

### Transport

Wolfpack uses libp2p with:
- **TCP** with Noise encryption
- **QUIC** (optional, for NAT traversal)
- **Yamux** stream multiplexing

### Discovery

- **mDNS**: Local network (default, most private)
- **Kademlia DHT**: Internet-wide (opt-in)

### Sync Protocol

Protocol ID: `/wolfpack/sync/1.0.0`

Request-response pattern over libp2p streams.

#### Request Types

**GetClock**
```json
{"type": "GetClock"}
```

**GetEvents**
```json
{
  "type": "GetEvents",
  "clock": {"device-a": 10, "device-b": 5}
}
```

**PushEvents**
```json
{
  "type": "PushEvents",
  "events": [EncryptedEvent, ...]
}
```

**SendTab**
```json
{
  "type": "SendTab",
  "url": "https://example.com",
  "title": "Example Page",
  "from_device": "laptop-abc123"
}
```

#### Response Types

**Clock**
```json
{
  "type": "Clock",
  "clock": {"device-a": 42, "device-b": 38},
  "device_id": "desktop-def456",
  "device_name": "Desktop"
}
```

**Events**
```json
{
  "type": "Events",
  "events": [EncryptedEvent, ...]
}
```

**Ack**
```json
{
  "type": "Ack",
  "count": 5
}
```

**TabReceived**
```json
{"type": "TabReceived"}
```

**Error**
```json
{
  "type": "Error",
  "message": "description"
}
```

#### EncryptedEvent

Wire format for encrypted events:

```json
{
  "version": 2,
  "cipher": 1,
  "public_key": "base64...",
  "nonce": "base64...",
  "ciphertext": "base64...",
  "tag": "base64..."
}
```

### Sync Algorithm

**On startup:**
```python
def initial_sync(peer):
    # Get peer's clock
    response = send_request(peer, GetClock())
    remote_clock = response.clock

    # Compare with local clock
    events_to_send = get_events_newer_than(remote_clock)
    events_to_request = get_events_we_need(remote_clock, local_clock)

    # Exchange events
    if events_to_send:
        send_request(peer, PushEvents(events_to_send))

    if events_to_request:
        response = send_request(peer, GetEvents(local_clock))
        apply_events(response.events)
```

**On local change:**
```python
def on_local_event(event):
    increment_clock(local_device_id)
    envelope = wrap_event(event)
    store_locally(envelope)

    for peer in connected_peers:
        send_request(peer, PushEvents([encrypt(envelope)]))
```

**On receiving events:**
```python
def on_receive_events(encrypted_events):
    for encrypted in encrypted_events:
        envelope = decrypt(encrypted)

        if is_duplicate(envelope.id):
            continue

        merge_clock(envelope.clock)
        materialize(envelope.event)
        mark_applied(envelope.id)
```

## State Materialization

Events are applied to local state in order. Each event type has specific materialization rules.

### Extension Events

| Event | Action |
|-------|--------|
| ExtensionAdded | INSERT INTO extensions |
| ExtensionRemoved | DELETE FROM extensions |
| ExtensionInstalled | INSERT INTO extensions, INSERT INTO extension_xpi |
| ExtensionUninstalled | DELETE FROM extensions, DELETE FROM extension_xpi |

### Container Events

| Event | Action |
|-------|--------|
| ContainerAdded | INSERT INTO containers |
| ContainerRemoved | DELETE FROM containers |
| ContainerUpdated | UPDATE containers (non-null fields only) |

### Handler Events

| Event | Action |
|-------|--------|
| HandlerSet | INSERT OR REPLACE INTO handlers |
| HandlerRemoved | DELETE FROM handlers |

### Search Engine Events

| Event | Action |
|-------|--------|
| SearchEngineAdded | INSERT OR REPLACE INTO search_engines |
| SearchEngineRemoved | DELETE FROM search_engines |
| SearchEngineDefault | UPDATE search_engines SET is_default=0; UPDATE search_engines SET is_default=1 WHERE id=? |

### Preference Events

| Event | Action |
|-------|--------|
| PrefSet | INSERT OR REPLACE INTO prefs |
| PrefRemoved | DELETE FROM prefs |

### Tab Events

| Event | Action |
|-------|--------|
| TabSent (to this device) | INSERT INTO pending_tabs |
| TabSent (to other device) | (no action) |
| TabReceived | DELETE FROM pending_tabs WHERE id=event_id |

## Conflict Resolution

When events are concurrent (neither happened-before the other), deterministic tiebreakers determine order:

1. **Clock sum**: Higher sum of all counters = later
2. **Timestamp**: ISO 8601 string comparison
3. **Device ID**: Lexicographic comparison

```python
def compare_concurrent(a, b):
    sum_a = sum(a.clock.values())
    sum_b = sum(b.clock.values())
    if sum_a != sum_b:
        return sum_a - sum_b

    if a.timestamp != b.timestamp:
        return 1 if a.timestamp > b.timestamp else -1

    return 1 if a.device > b.device else -1
```

All implementations MUST use identical tiebreakers to ensure convergence.

## Compression

### XPI Compression

Extension XPIs are compressed with zstd before base64 encoding:

```python
def compress_xpi(xpi_bytes):
    compressed = zstd.compress(xpi_bytes, level=19)
    return base64.b64encode(compressed).decode('ascii')

def decompress_xpi(encoded):
    compressed = base64.b64decode(encoded)
    return zstd.decompress(compressed)
```

Level 19 provides good compression ratio for distribution.

## Conformance Requirements

A conforming implementation MUST:

1. **Event format**: Use exact JSON structure specified
2. **Vector clocks**: Implement increment, merge, compare correctly
3. **Encryption**: Support both AES-256-GCM and XChaCha20-Poly1305
4. **Nonce derivation**: Use specified deterministic algorithm
5. **Conflict resolution**: Use identical tiebreakers
6. **Idempotency**: Handle duplicate events gracefully
7. **State materialization**: Apply events in causal order

A conforming implementation MAY:

1. Use alternative P2P transports (as long as messages are compatible)
2. Store state in any database (as long as semantics match)
3. Add additional event types (unknown types should be preserved but not applied)
4. Implement additional features (bookmarks, history, etc.)

## HTTP API for Pairing

The daemon exposes a localhost HTTP API for device pairing. This API is used by the CLI and browser extensions.

### Authentication

All endpoints (except `/health`) require authentication via API token:

```
X-Wolfpack-Token: <64-character-hex-token>
```

The token is stored in `$XDG_DATA_HOME/wolfpack/api.token` (or `~/.local/share/wolfpack/api.token`) with mode 600.

### CSRF Protection

Requests from web browsers are validated:
- Requests with `Origin: moz-extension://...` are allowed (Firefox/LibreWolf extensions)
- Requests with `Origin: chrome-extension://...` are allowed (Chromium extensions)
- Requests with no `Origin` header are allowed (CLI tools, curl)
- Requests from web origins (`http://`, `https://`) are rejected

### Endpoints

#### GET /health

Health check (no authentication required).

**Response:** `200 OK` with body `"OK"`

#### GET /status

Get daemon status.

**Response:**
```json
{
  "status": "running",
  "device_id": "laptop-abc123",
  "device_name": "My Laptop",
  "version": "0.1.0"
}
```

#### POST /pair/initiate

Create a new pairing session (initiator side).

**Response:**
```json
{
  "code": "123456",
  "expires_in_seconds": 300
}
```

The 6-digit code is valid for 5 minutes.

#### POST /pair/join

Join an existing pairing session (joiner side).

**Request:**
```json
{
  "code": "123456",
  "device_id": "desktop-def456",
  "device_name": "My Desktop",
  "public_key": "64-char-hex-x25519-public-key"
}
```

**Response:**
```json
{
  "status": "accepted|rejected|expired|invalid_code",
  "device_id": "laptop-abc123",
  "device_name": "My Laptop",
  "public_key": "64-char-hex-x25519-public-key"
}
```

Status values:
- `accepted`: Pairing successful, response includes initiator's info
- `rejected`: User rejected the pairing request
- `expired`: Code expired (5 minute timeout)
- `invalid_code`: Code doesn't match any active session

#### GET /pair/pending

Check for pending pairing requests (initiator polling).

**Response:**
```json
{
  "pending": true,
  "request": {
    "device_id": "desktop-def456",
    "device_name": "My Desktop",
    "public_key_fingerprint": "a1b2c3d4...89abcdef"
  }
}
```

If no pending request:
```json
{
  "pending": false,
  "request": null
}
```

#### POST /pair/respond

Accept or reject a pending pairing request.

**Request:**
```json
{
  "accept": true
}
```

**Response:**
```json
{
  "status": "ok"
}
```

#### POST /pair/cancel

Cancel the current pairing session.

**Response:**
```json
{
  "status": "ok"
}
```

### Pairing Flow

```
Initiator (Device A)                    Joiner (Device B)
─────────────────────                   ─────────────────
1. POST /pair/initiate
   → code "123456"

2. Display code, wait
                                        3. User enters code

                                        4. POST /pair/join
                                           {code, device_id, name, public_key}

5. GET /pair/pending
   ← pending request

6. User confirms

7. POST /pair/respond
   {accept: true}
                                        8. Receives response:
                                           {status: "accepted", ...}

9. Both devices now have each other's public keys
   and can encrypt/decrypt sync events
```

## Security Considerations

1. **Private keys**: Never transmit, store securely
2. **Public key verification**: Verify fingerprint during pairing to prevent MITM
3. **Nonce uniqueness**: Guaranteed by counter monotonicity
4. **Forward secrecy**: Not provided at application layer (transport layer only)
5. **Event integrity**: AEAD tag prevents tampering
6. **API token**: Stored with restrictive permissions (mode 600)
7. **CSRF protection**: Origin validation prevents cross-site requests
8. **Localhost binding**: HTTP API only accessible from local machine

## Version History

| Version | Changes |
|---------|---------|
| 1.0 | Initial specification |
