# Configuration

Wolfpack stores its configuration at `~/.local/share/wolfpack/config.toml`.

## Full Configuration Reference

```toml
[device]
# Unique device identifier (generated on init)
id = "01234567-89ab-cdef-0123-456789abcdef"
# Human-readable device name
name = "laptop"

[paths]
# Path to LibreWolf profile (auto-detected if not set)
profile = "/home/user/.librewolf/xxxxxxxx.default-release"

[sync]
# Port for P2P connections (0 or omit for random)
listen_port = 0
# Enable DHT for internet-wide discovery (default: false, local-only via mDNS)
enable_dht = false
# Bootstrap peers for DHT (multiaddr format)
bootstrap_peers = [
    "/ip4/104.131.131.82/tcp/4001/p2p/QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ"
]

[api]
# HTTP API port for pairing and browser extension communication
port = 9778

[prefs]
# List of preference keys to sync
# Other preferences are ignored to avoid syncing sensitive data
whitelist = [
    "browser.startup.homepage",
    "browser.newtabpage.enabled",
    "browser.urlbar.placeholderName",
    "browser.search.defaultenginename",
]
```

## Device Section

### `device.id`

A UUID v7 identifier unique to this device. Generated automatically during `wolfpack init`. Do not change this after pairing with other devices.

### `device.name`

Human-readable name for this device. Used when:
- Displaying connected peers
- Targeting send-tab commands
- Identifying event sources

Can be changed at any time. Defaults to your hostname.

## Paths Section

### `paths.profile`

Path to your LibreWolf/Firefox profile directory. If not set, wolfpack attempts auto-detection:

1. Checks `~/.librewolf/` for profiles
2. Falls back to `~/.mozilla/firefox/`
3. Uses the profile with `default-release` in its name
4. Falls back to the first profile found

To find your profile path manually:
```bash
# LibreWolf
ls ~/.librewolf/

# Firefox
ls ~/.mozilla/firefox/
```

## Sync Section

### `sync.listen_port`

Port for P2P connections. Set to `0` or omit for a random port.

If you need a fixed port (e.g., for firewall rules):
```toml
[sync]
listen_port = 44444
```

### `sync.enable_dht`

Enable Kademlia DHT for internet-wide peer discovery. Default: `false`

When disabled (default), wolfpack only discovers peers on the local network via mDNS. This is the most private option.

When enabled, wolfpack joins the public libp2p DHT network, which allows discovering peers across the internet but exposes your peer ID to the DHT.

```toml
[sync]
enable_dht = true
```

### `sync.bootstrap_peers`

Bootstrap peers for DHT discovery. Only used when `enable_dht = true`.

These are well-known peers that help your node join the DHT network. You can use public IPFS bootstrap nodes or run your own.

Format: multiaddr strings
```toml
[sync]
bootstrap_peers = [
    "/ip4/104.131.131.82/tcp/4001/p2p/QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ",
    "/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN"
]
```

## API Section

### `api.port`

Port for the localhost HTTP API. Default: `9778`

The HTTP API is used for:
- Device pairing (`wolfpack pair`)
- Browser extension communication
- Status queries

The API only binds to `127.0.0.1` (localhost) for security.

```toml
[api]
port = 9778
```

To use a different port:
```toml
[api]
port = 8080
```

### API Token

An authentication token is automatically generated and stored at `~/.local/share/wolfpack/api.token`. This token is required for all API requests (except `/health`).

The token is a 64-character hex string with restrictive file permissions (mode 600).

## Prefs Section

### `prefs.whitelist`

List of preference keys that should be synced. Any preference not in this list is ignored.

This is a security measure. Many preferences contain:
- Local paths
- Machine-specific settings
- Potentially sensitive data

Common safe preferences to sync:
```toml
whitelist = [
    # Startup
    "browser.startup.homepage",
    "browser.startup.page",

    # New tab
    "browser.newtabpage.enabled",
    "browser.newtabpage.activity-stream.feeds.topsites",

    # Search
    "browser.urlbar.placeholderName",
    "browser.search.defaultenginename",

    # UI
    "browser.tabs.warnOnClose",
    "browser.tabs.closeWindowWithLastTab",

    # Privacy (LibreWolf already sets good defaults)
    "privacy.donottrackheader.enabled",
]
```

## Environment Variables

### `RUST_LOG`

Controls logging verbosity. Examples:
```bash
# Default (info level)
wolfpack daemon

# Debug logging
RUST_LOG=debug wolfpack daemon

# Trace logging for specific module
RUST_LOG=wolfpack::net=trace wolfpack daemon
```

### `WOLFPACK_CONFIG`

Override config file path:
```bash
WOLFPACK_CONFIG=/path/to/config.toml wolfpack daemon
```

Or use the `--config` flag:
```bash
wolfpack --config /path/to/config.toml daemon
```

## Profile Auto-Detection

When `paths.profile` is not set, wolfpack searches for profiles in this order:

1. `~/.librewolf/*.default-release/`
2. `~/.librewolf/*.default/`
3. `~/.librewolf/*/` (first found)
4. `~/.mozilla/firefox/*.default-release/`
5. `~/.mozilla/firefox/*.default/`
6. `~/.mozilla/firefox/*/` (first found)

The profile must contain at least one of:
- `extensions.json`
- `prefs.js`
- `containers.json`

## Multiple Profiles

If you use multiple profiles, create separate wolfpack configs:

```bash
# Profile 1
wolfpack --config ~/.config/wolfpack/work.toml init --name "work-laptop"
wolfpack --config ~/.config/wolfpack/work.toml daemon

# Profile 2
wolfpack --config ~/.config/wolfpack/personal.toml init --name "personal-laptop"
wolfpack --config ~/.config/wolfpack/personal.toml daemon
```

Each profile needs its own:
- Config file
- Device identity
- Separate daemon instance

## Network Modes

### Local Only (Default)

```toml
[sync]
enable_dht = false
```

- Discovers peers via mDNS on local network
- No internet connectivity required
- Maximum privacy

### Internet-Wide

```toml
[sync]
enable_dht = true
bootstrap_peers = [
    "/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN"
]
```

- Joins public DHT network
- Discovers peers across the internet
- Requires at least one bootstrap peer
