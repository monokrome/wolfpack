# Security

Wolfpack uses strong cryptographic primitives to ensure your browser data remains private, even when synced over untrusted networks.

## Threat Model

### What We Protect Against

- **Network observers**: Cannot read your data (double encryption: Noise + E2E)
- **Malicious peers**: Cannot impersonate your devices without keys
- **Replay attacks**: Vector clocks prevent event replay
- **Nonce reuse**: Deterministic nonces from vector clocks prevent collision
- **DHT observers**: Can see peer IDs but not data content

### What We Don't Protect Against

- **Compromised device**: If an attacker has your device, they have your keys
- **Malicious paired device**: A paired device can read all synced data
- **Side channels**: Connection timing/patterns may leak information
- **DHT metadata**: Peer ID presence is visible in DHT (opt-in feature)

## Cryptographic Primitives

### Key Exchange: X25519

Each device generates an X25519 keypair:
- **Private key**: 32 bytes, never leaves the device
- **Public key**: 32 bytes, shared with paired devices

Key agreement produces a shared secret for encryption.

### Encryption: AES-256-GCM / XChaCha20-Poly1305

Wolfpack supports two AEAD ciphers:

| Cipher | Key | Nonce | Tag | Hardware Accel |
|--------|-----|-------|-----|----------------|
| AES-256-GCM | 256-bit | 96-bit | 128-bit | AES-NI |
| XChaCha20-Poly1305 | 256-bit | 192-bit | 128-bit | None needed |

**Cipher Selection**:
1. If CPU supports AES-NI → use AES-256-GCM
2. Otherwise → use XChaCha20-Poly1305

Both ciphers provide:
- Authenticated encryption (confidentiality + integrity)
- Protection against tampering
- Chosen-ciphertext security

### Nonce Generation

Nonces are derived deterministically from:
1. Device ID hash (4 bytes)
2. Vector clock counter (8 bytes)

```
AES-GCM Nonce (12 bytes):
┌────────────────┬────────────────────────┐
│ Device Hash    │ Counter (big-endian)   │
│ (4 bytes)      │ (8 bytes)              │
└────────────────┴────────────────────────┘

XChaCha20 Nonce (24 bytes):
┌────────────────┬────────────────────────┬──────────────┐
│ Device Hash    │ Counter (big-endian)   │ Padding      │
│ (8 bytes)      │ (8 bytes)              │ (8 bytes)    │
└────────────────┴────────────────────────┴──────────────┘
```

This ensures:
- No random nonce collisions
- Deterministic replay detection
- Each event gets a unique nonce

### Hashing: SHA-256

Used for:
- Device ID to nonce prefix derivation
- Key fingerprinting

## Transport Security

libp2p provides transport-level encryption:

### Noise Protocol

All libp2p connections use the Noise protocol framework:
- XX handshake pattern (mutual authentication)
- Diffie-Hellman key exchange
- ChaCha20-Poly1305 encryption

This provides:
- Forward secrecy per-connection
- Mutual authentication
- Protection against MITM

### Double Encryption

Data is encrypted twice:
1. **E2E layer**: Your wolfpack encryption (AES-GCM/XChaCha20)
2. **Transport layer**: libp2p Noise protocol

Even if Noise is somehow compromised, your data remains protected by E2E encryption.

## Key Management

### Key Storage

```
~/.local/share/wolfpack/
├── keys/
│   └── local.key      # Private key (600 permissions)
└── api.token          # HTTP API token (600 permissions)
```

The private key (`local.key`) is:
- Stored with restricted permissions (600)
- Never transmitted
- Never synced

The API token (`api.token`) is:
- 64-character hex string (256 bits of entropy)
- Required for HTTP API authentication
- Stored with restricted permissions (600)

### Key Format

Keys are stored as hex-encoded strings:
```
# Public key (64 hex chars = 32 bytes)
a1b2c3d4e5f6...
```

### Pairing Process

Wolfpack uses a code-based pairing flow:

1. Device A generates a 6-digit pairing code (valid 5 minutes)
2. User communicates code to Device B (verbally, message, etc.)
3. Device B enters code and sends device info + public key
4. Device A shows request, user confirms device name
5. Device A accepts, both exchange public keys
6. Both devices can now compute shared secret

```
Device A (initiator)        Device B (joiner)
─────────────────────────────────────────────
Generate keypair           Generate keypair
Generate code "847293"
Display code          →    Enter code "847293"
                      ←    Send device info + public key
Verify device name
Accept request        →    Receive confirmation + public key

Compute: DH(A_priv, B_pub)  Compute: DH(B_priv, A_pub)
         = shared_secret             = shared_secret
```

### Pairing Security Properties

| Property | Value |
|----------|-------|
| Code space | 900,000 (6 digits) |
| Code lifetime | 5 minutes |
| Concurrent sessions | 1 per device |
| Attempts per code | 1 (single use) |

The pairing code protects against:
- **Remote attackers**: Must guess 6-digit code
- **Replay attacks**: Codes are single-use
- **MITM attacks**: User verifies device name before accepting

## Event Encryption

### Encryption Flow

```
Events (JSON) → Serialize → Encrypt → Transmit
                              ↓
                    ┌─────────────────────┐
                    │ Version (1 byte)    │
                    │ Cipher (1 byte)     │
                    │ Public Key (32)     │
                    │ Nonce (12 or 24)    │
                    │ Ciphertext (var)    │
                    │ Auth Tag (16)       │
                    └─────────────────────┘
```

### Decryption Flow

```
Receive → Parse Header → Derive Nonce → Decrypt → Deserialize → Events
                             ↓
                   device_id + counter
                   from vector clock
```

## Vector Clock Security

Vector clocks provide:

### Causal Ordering

Events are ordered by causality, not wall clock time. This prevents:
- Clock skew attacks
- Timestamp manipulation

### Replay Detection

Each event has a unique (device_id, counter) pair. Replayed events are detected by:
1. Checking if event ID was already processed
2. Checking if counter is ≤ current known counter

### Nonce Uniqueness

Since nonces are derived from counters:
- Counter never decreases
- Each counter value used exactly once
- Nonce reuse is impossible without counter collision

## HTTP API Security

The daemon exposes a localhost HTTP API for pairing and browser extension communication.

### Defense Layers

| Layer | Protection |
|-------|------------|
| Localhost binding | Only accessible from local machine |
| API token | 256-bit authentication token required |
| Origin validation | Blocks web origins, allows extensions |
| User confirmation | Pairing requires explicit acceptance |

### API Token

- Generated on first run (or `wolfpack init`)
- 64 hex characters (256 bits of entropy)
- Stored in `~/.local/share/wolfpack/api.token`
- Required in `X-Wolfpack-Token` header

### CSRF Protection

The API validates the `Origin` header:

| Origin | Allowed |
|--------|---------|
| (none) | Yes - CLI tools, curl |
| `moz-extension://...` | Yes - Firefox/LibreWolf extensions |
| `chrome-extension://...` | Yes - Chromium extensions |
| `http://...`, `https://...` | **No** - Web pages blocked |

This prevents malicious websites from calling the API even if they somehow learn the token.

### Attack Scenarios

| Attack | Mitigation |
|--------|------------|
| Web page calls API | Origin check blocks |
| Malware reads token | Requires local access (already compromised) |
| CSRF from extension | Extensions can't read other extensions' tokens |
| Network sniffing | Localhost only, token in header |

## P2P Network Security

### Peer Identity

Each node has a libp2p peer ID derived from its Ed25519 key:
- Peer ID is public (visible in DHT if enabled)
- Does not reveal wolfpack encryption keys
- Different from wolfpack device ID

### mDNS Security

mDNS discovery (local network only):
- No authentication (any device can announce)
- Relies on wolfpack's E2E encryption for data security
- Only exposes presence on local network

### DHT Security

When DHT is enabled:
- Peer ID is publicly visible
- Connection patterns may be observable
- Data remains E2E encrypted

For maximum privacy, keep DHT disabled (default).

## Metadata Leakage

Even with encryption, some metadata is visible:

### Visible to Network

- Connection timing and duration
- Data volume transferred
- Peer IDs (if DHT enabled)

### Not Visible

- Event contents
- Extension names, URLs, preferences
- Private keys

### Mitigation

- Keep DHT disabled for local-only sync
- Use Tor if network-level privacy needed (not built-in)

## Comparison with Firefox Sync

| Feature | Firefox Sync | Wolfpack |
|---------|--------------|----------|
| E2E Encryption | Yes (kB/kW) | Yes (X25519/AES-GCM) |
| Key Recovery | Via password | Manual (no recovery) |
| Server Trust | Mozilla | None (P2P) |
| Metadata | Mozilla sees | Network observers only |
| Open Source | Partial | Full |
| Transport | HTTPS | libp2p Noise |

## Forward Secrecy

### Transport Layer

libp2p Noise provides forward secrecy per-connection. Compromise of long-term keys doesn't expose past connections.

### Application Layer

Wolfpack does NOT provide forward secrecy at the E2E layer. If a device private key is compromised:
- All past events can be decrypted
- All future events can be decrypted

To rotate keys:
1. Generate new keypair: delete `local.key`, run `wolfpack pair`
2. Re-pair all devices with new public key
3. Old events remain encrypted with old key

Future versions may implement ratcheting for forward secrecy.

## Auditing

Wolfpack has not been professionally audited. The cryptographic approach is:
- Standard primitives (X25519, AES-GCM, XChaCha20, Noise)
- Standard libraries (`aes-gcm`, `chacha20poly1305`, `x25519-dalek`, `libp2p`)
- No custom cryptography

If you require audited security, wait for a professional review or audit the code yourself.
