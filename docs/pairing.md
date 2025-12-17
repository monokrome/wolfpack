# Device Pairing

Pairing establishes a secure channel between devices by exchanging public keys. Once paired, devices can encrypt data that only each other can read.

## Overview

Wolfpack uses a simple code-based pairing flow:

```
┌─────────────────┐                    ┌─────────────────┐
│    Device A     │                    │    Device B     │
│   (initiator)   │                    │    (joiner)     │
│                 │                    │                 │
│ 1. Generate     │                    │                 │
│    6-digit code │                    │                 │
│    "847293"     │                    │                 │
│                 │   share code       │                 │
│                 │ ───────────────►   │ 2. Enter code   │
│                 │                    │    "847293"     │
│                 │   device info +    │                 │
│                 │   public key       │                 │
│ 3. Review       │ ◄───────────────   │                 │
│    request      │                    │                 │
│                 │                    │                 │
│ 4. Accept       │   device info +    │                 │
│    (y)          │   public key       │                 │
│                 │ ───────────────►   │ 5. Receive      │
│                 │                    │    confirmation │
└─────────────────┘                    └─────────────────┘

Both devices now have each other's public keys
and can encrypt/decrypt sync data
```

## Quick Start

### Step 1: Ensure daemon is running on both devices

```bash
# On both devices
wolfpack daemon
```

### Step 2: Initiate pairing on Device A

```bash
wolfpack pair
```

Output:
```
Starting pairing session...

╔═══════════════════════════════════════════╗
║          PAIRING CODE: 847293          ║
╚═══════════════════════════════════════════╝

On the other device, run:
  wolfpack pair --code 847293

Code expires in 300 seconds.
Waiting for connection...
```

### Step 3: Join from Device B

```bash
wolfpack pair --code 847293
```

Output:
```
Joining pairing session...
```

### Step 4: Accept on Device A

Device A will show:
```
Incoming pairing request!

  Device: desktop (019234ab-cdef-7890-1234-567890abcdef)
  Key:    a1b2c3d4...89abcdef

Accept this device? [y/N]
```

Type `y` and press Enter.

### Step 5: Confirmation

Both devices show:
```
Device paired successfully!
The devices will now sync automatically when discovered on the network.
```

## Security Model

### Code Properties

- **6 digits**: 900,000 possible codes (100000-999999)
- **5-minute expiry**: Limited attack window
- **Single use**: Code invalidated after use
- **Rate limited**: One active session at a time

### What to verify

When accepting a pairing request, verify:

1. **Device name**: Does it match the device you're trying to pair?
2. **Timing**: Did you initiate pairing on the other device just now?
3. **Key fingerprint**: If paranoid, compare full keys on both devices

### Threat model

The code-based pairing protects against:

| Threat | Protection |
|--------|------------|
| Remote attacker | Must guess 6-digit code |
| Nearby attacker | Must see/hear code being shared |
| MITM attack | User verifies device name |
| Replay attack | Codes are single-use |
| Brute force | 5-minute expiry limits attempts |

## Pairing Scenarios

### Same Location

If both devices are with you:
1. Run `wolfpack pair` on one device
2. Read the code off the screen
3. Type it on the other device

### Remote Pairing

Devices don't need to be on the same network:

1. Run `wolfpack pair` on Device A
2. Share the code via phone call, Signal, or any channel
3. Run `wolfpack pair --code XXXXXX` on Device B
4. Accept on Device A

The pairing uses the local HTTP API, so network connectivity isn't required during the pairing itself.

### Browser Extension Pairing

Browser extensions can use the HTTP API:

```javascript
// Initiate pairing
const response = await fetch('http://127.0.0.1:9778/pair/initiate', {
  method: 'POST',
  headers: { 'X-Wolfpack-Token': token }
});
const { code } = await response.json();

// Display code to user, poll for request
const pending = await fetch('http://127.0.0.1:9778/pair/pending', {
  headers: { 'X-Wolfpack-Token': token }
});

// Accept/reject
await fetch('http://127.0.0.1:9778/pair/respond', {
  method: 'POST',
  headers: { 'X-Wolfpack-Token': token },
  body: JSON.stringify({ accept: true })
});
```

See [protocol.md](protocol.md) for the full API specification.

## Adding More Devices

For 3+ devices, pair each new device with one existing device:

```
Device A ←──────→ Device B
    ↑
    └────────────→ Device C
```

Each pairing creates a bidirectional trust relationship. Sync data is encrypted for all paired devices.

## Key Storage

After pairing, keys are stored locally:

```
~/.local/share/wolfpack/
├── keys/
│   └── local.key          # Your private key (NEVER share)
├── api.token              # HTTP API authentication token
└── sync/
    └── keys/
        ├── your-device.pub    # Your public key
        └── other-device.pub   # Paired device's public key
```

## Removing a Device

To unpair a device:

```bash
# Find the key file
ls ~/.local/share/wolfpack/sync/keys/

# Remove it
rm ~/.local/share/wolfpack/sync/keys/device-name.pub
```

The removed device:
- Can no longer decrypt new events
- Still has access to previously synced data
- Is not automatically notified

For full revocation, generate new keys on remaining devices and re-pair.

## Troubleshooting

### "Failed to connect to daemon"

The daemon must be running:
```bash
wolfpack daemon
```

### "Invalid pairing code"

- Double-check the 6-digit code
- Code may have expired (5-minute timeout)
- Ask initiator to run `wolfpack pair` again

### "Pairing was rejected"

The initiator typed `n` or didn't respond with `y`. Try again.

### "Code expired"

Pairing codes are valid for 5 minutes. Generate a new one:
```bash
wolfpack pair
```

### Devices paired but not syncing

1. Check both daemons are running: `wolfpack status`
2. Verify devices are on the same network (for mDNS discovery)
3. Or enable DHT in config for internet-wide sync
4. Check firewalls allow the P2P port

## HTTP API Reference

The pairing flow uses a localhost HTTP API on port 9778 (configurable).

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check (no auth) |
| `/status` | GET | Daemon status |
| `/pair/initiate` | POST | Create pairing session |
| `/pair/join` | POST | Join with code |
| `/pair/pending` | GET | Check for incoming request |
| `/pair/respond` | POST | Accept/reject request |
| `/pair/cancel` | POST | Cancel session |

All endpoints except `/health` require the `X-Wolfpack-Token` header.

See [protocol.md](protocol.md) for complete API documentation.
