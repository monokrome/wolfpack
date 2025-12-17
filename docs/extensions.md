# Extension Management

Wolfpack can build Firefox/LibreWolf extensions from source and sync them across devices. This ensures you're running extensions built from code you can inspect, rather than trusting pre-built binaries from centralized repositories.

## Why Build from Source?

If LibreWolf doesn't trust Mozilla enough to use Firefox Sync, why trust Mozilla to build your extensions?

Building from source provides:
- **Transparency**: See exactly what code is running
- **Reproducibility**: Same source produces same extension
- **Independence**: No reliance on AMO or other centralized services
- **Version control**: Pin to specific commits or tags

## CLI Commands

### Install from Git

```bash
wolfpack extension install https://github.com/user/extension-repo

# With specific branch/tag/commit
wolfpack extension install https://github.com/user/extension-repo --ref v1.2.3

# With custom build command
wolfpack extension install https://github.com/user/extension-repo --build "make release"
```

### Install from Local XPI

```bash
wolfpack extension install /path/to/extension.xpi
```

### List Extensions

```bash
# List all synced extensions
wolfpack extension list

# Show only extensions not installed on this device
wolfpack extension list --missing
```

Output:
```
Synced extensions:
  uBlock Origin (ublock@gorhill.org) [installed] - https://github.com/gorhill/uBlock
  Bitwarden (bitwarden@bitwarden.com) [installed]
  Dark Reader (dark-reader@nicedoc.io) [missing] - https://github.com/nicedoc/dark-reader
```

### Uninstall

```bash
wolfpack extension uninstall ublock@gorhill.org
```

## Build System Detection

Wolfpack automatically detects how to build extensions:

| Build System | Detection | Command |
|--------------|-----------|---------|
| npm | `package.json` with `build` script | `npm ci && npm run build` |
| pnpm | `pnpm-lock.yaml` | `pnpm install && pnpm run build` |
| yarn | `yarn.lock` | `yarn install && yarn run build` |
| make | `Makefile` | `make` |
| web-ext | `web-ext-config.js` | `web-ext build` |
| none | `manifest.json` in root | (no build needed) |

Override with `--build`:
```bash
wolfpack extension install https://github.com/user/repo --build "npm run build:firefox"
```

## Extension Packaging

Extensions are packaged and compressed for efficient sync:

1. **Build**: Run detected/specified build command
2. **Locate**: Find `manifest.json` in build output
3. **Package**: Create XPI (ZIP) from extension directory
4. **Compress**: Apply zstd compression (level 19)
5. **Encode**: Base64 encode for JSON transport

Typical compression ratios:
- Small extensions (~100KB): 60-70% reduction
- Large extensions (~1MB): 70-80% reduction

## Manifest Requirements

Extensions must have a `manifest.json` with:

```json
{
  "manifest_version": 2,
  "name": "Extension Name",
  "version": "1.0.0",
  "browser_specific_settings": {
    "gecko": {
      "id": "extension@example.com"
    }
  }
}
```

If `browser_specific_settings.gecko.id` is missing, an ID is generated from the name:
```
"My Extension" → "my-extension@local"
```

## Sync Behavior

### Installing Device

When you run `wolfpack extension install`:

1. Extension is cloned/built/packaged
2. XPI stored in local database
3. XPI installed to LibreWolf profile
4. `ExtensionInstalled` event created
5. Event synced to other devices

### Receiving Devices

When other devices receive `ExtensionInstalled`:

1. Event is materialized to database
2. XPI data is stored
3. On next sync cycle, daemon installs XPI to profile
4. LibreWolf loads extension on restart

### Uninstalling

When you run `wolfpack extension uninstall`:

1. XPI removed from profile
2. Extension data removed from database
3. `ExtensionUninstalled` event created
4. Receiving devices remove the extension

## Profile Installation

Extensions are installed as XPI files in the profile:

```
~/.librewolf/xxxxxxxx.default-release/
└── extensions/
    ├── ublock@gorhill.org.xpi
    ├── bitwarden@bitwarden.com.xpi
    └── dark-reader@nicedoc.io.xpi
```

LibreWolf/Firefox loads extensions from this directory on startup.

## Platform Compatibility

WebExtensions are platform-agnostic. An extension built on Linux works on:
- Linux (x86_64, aarch64)
- macOS (Intel, Apple Silicon)
- Windows (x86_64)

The build process may require platform-specific tools (Node.js, npm), but the output XPI contains only JavaScript, HTML, CSS, and JSON.

## Database Storage

Extension data is stored in SQLite:

```sql
-- Extension metadata
CREATE TABLE extensions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    url TEXT,
    added_at TEXT NOT NULL
);

-- Full XPI data (compressed, base64)
CREATE TABLE extension_xpi (
    id TEXT PRIMARY KEY,
    version TEXT NOT NULL,
    source_type TEXT NOT NULL,    -- "git", "amo", "local"
    source_data TEXT NOT NULL,    -- JSON with source details
    xpi_data TEXT NOT NULL,       -- Zstd + base64 encoded XPI
    installed_at TEXT NOT NULL
);
```

## Event Format

### ExtensionInstalled

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

### ExtensionUninstalled

```json
{
  "type": "ExtensionUninstalled",
  "data": {
    "id": "ublock@gorhill.org"
  }
}
```

## Source Types

### Git

Built from a git repository:

```json
{
  "type": "Git",
  "url": "https://github.com/gorhill/uBlock",
  "ref_spec": "1.55.0",
  "build_cmd": "npm run build"
}
```

The `build_cmd` is stored for reference. When updating extensions, you can see what command was used.

### AMO (Future)

Downloaded from addons.mozilla.org:

```json
{
  "type": "Amo",
  "amo_slug": "ublock-origin"
}
```

### Local

Installed from a local XPI file:

```json
{
  "type": "Local",
  "original_path": "/home/user/downloads/extension.xpi"
}
```

The path is metadata only; the actual XPI is embedded in `xpi_data`.

## Updating Extensions

To update an extension:

1. Uninstall the current version:
   ```bash
   wolfpack extension uninstall extension@id
   ```

2. Install the new version:
   ```bash
   wolfpack extension install https://github.com/user/repo --ref v2.0.0
   ```

Future versions may support in-place updates.

## Troubleshooting

### Extension Not Loading

1. Check LibreWolf was restarted after installation
2. Verify XPI exists in profile: `ls ~/.librewolf/*/extensions/`
3. Check `about:addons` for error messages
4. Some extensions require explicit enablement

### Build Failures

```bash
# Check if dependencies are available
node --version
npm --version

# Try building manually to see errors
git clone https://github.com/user/repo
cd repo
npm install
npm run build
```

### Signature Warnings

LibreWolf may warn about unsigned extensions. To allow:

1. Open `about:config`
2. Set `xpinstall.signatures.required` to `false`

LibreWolf has this disabled by default. Firefox requires additional steps.

## Security Considerations

- **Review source**: Before installing, inspect the repository
- **Pin versions**: Use specific tags/commits, not `main`/`master`
- **Build isolation**: Builds run in temporary directories
- **No auto-update**: Extensions don't update automatically

Building from source doesn't guarantee safety. Malicious code in the source will be built and distributed. Always review what you're installing.
