use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use tracing::info;

/// Extension manifest data extracted from manifest.json
#[derive(Debug, Clone)]
pub struct ExtensionManifest {
    pub id: String,
    pub name: String,
    pub version: String,
}

/// Read and parse manifest.json
pub fn read_manifest(dir: &Path) -> Result<ExtensionManifest> {
    let manifest_path = dir.join("manifest.json");
    let content = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;

    let manifest: serde_json::Value =
        serde_json::from_str(&content).context("Failed to parse manifest.json")?;

    // Get extension ID from browser_specific_settings or applications
    let id = manifest
        .get("browser_specific_settings")
        .or_else(|| manifest.get("applications"))
        .and_then(|b| b.get("gecko"))
        .and_then(|g| g.get("id"))
        .and_then(|id| id.as_str())
        .map(String::from)
        .unwrap_or_else(|| {
            // Generate ID from name if not specified
            let name = manifest
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("unknown");
            format!("{}@local", name.to_lowercase().replace(' ', "-"))
        });

    let name = manifest
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("Unknown Extension")
        .to_string();

    let version = manifest
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("0.0.0")
        .to_string();

    Ok(ExtensionManifest { id, name, version })
}

fn add_dir_to_zip<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    base_dir: &Path,
    current_dir: &Path,
    options: &zip::write::SimpleFileOptions,
) -> Result<()> {
    for entry in std::fs::read_dir(current_dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = path.strip_prefix(base_dir)?.to_string_lossy();

        // Skip hidden files and common non-extension files
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if file_name.starts_with('.') || file_name == "node_modules" {
            continue;
        }

        if path.is_file() {
            zip.start_file(name.to_string(), *options)?;
            let mut f = File::open(&path)?;
            let mut buffer = Vec::new();
            f.read_to_end(&mut buffer)?;
            zip.write_all(&buffer)?;
        } else if path.is_dir() {
            zip.add_directory(format!("{}/", name), *options)?;
            add_dir_to_zip(zip, base_dir, &path, options)?;
        }
    }

    Ok(())
}

/// Compress XPI data using zstd
pub fn compress_xpi(data: &[u8]) -> Result<Vec<u8>> {
    zstd::encode_all(std::io::Cursor::new(data), 19) // Level 19 for good compression
        .context("Failed to compress XPI")
}

/// Decompress XPI data
pub fn decompress_xpi(data: &[u8]) -> Result<Vec<u8>> {
    zstd::decode_all(std::io::Cursor::new(data)).context("Failed to decompress XPI")
}

/// Encode compressed data as base64
pub fn encode_base64(data: &[u8]) -> String {
    BASE64.encode(data)
}

/// Decode base64 to bytes
pub fn decode_base64(data: &str) -> Result<Vec<u8>> {
    BASE64.decode(data).context("Failed to decode base64")
}

/// Full pipeline: directory -> compressed base64 XPI
#[allow(clippy::cognitive_complexity)] // Packaging pipeline with multiple steps
pub fn package_extension(source_dir: &Path) -> Result<(ExtensionManifest, String)> {
    // Read manifest
    let manifest = read_manifest(source_dir)?;
    info!("Packaging {} v{}", manifest.name, manifest.version);

    // Create XPI in memory
    let mut xpi_data = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut xpi_data);
        let mut zip = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        add_dir_to_zip(&mut zip, source_dir, source_dir, &options)?;
        zip.finish()?;
    }

    info!("XPI size: {} bytes", xpi_data.len());

    // Compress
    let compressed = compress_xpi(&xpi_data)?;
    info!("Compressed size: {} bytes", compressed.len());

    // Encode
    let encoded = encode_base64(&compressed);

    Ok((manifest, encoded))
}

/// Unpack a base64-encoded compressed XPI to a directory
pub fn unpack_extension(xpi_data: &str, target_dir: &Path) -> Result<ExtensionManifest> {
    // Decode
    let compressed = decode_base64(xpi_data)?;

    // Decompress
    let xpi_bytes = decompress_xpi(&compressed)?;

    // Extract
    let cursor = std::io::Cursor::new(xpi_bytes);
    let mut archive = zip::ZipArchive::new(cursor)?;

    std::fs::create_dir_all(target_dir)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = target_dir.join(file.mangled_name());

        if file.is_dir() {
            std::fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut outfile = File::create(&outpath)?;
            std::io::copy(&mut file, &mut outfile)?;
        }
    }

    // Read manifest from extracted files
    read_manifest(target_dir)
}

/// Install an extension to a Firefox/LibreWolf profile
pub fn install_to_profile(xpi_data: &str, profile_dir: &Path, extension_id: &str) -> Result<()> {
    let extensions_dir = profile_dir.join("extensions");
    std::fs::create_dir_all(&extensions_dir).with_context(|| {
        format!(
            "Failed to create extensions dir: {}",
            extensions_dir.display()
        )
    })?;

    // Decode and decompress
    let compressed = decode_base64(xpi_data)?;
    let xpi_bytes = decompress_xpi(&compressed)?;

    // Write as {extension_id}.xpi
    let xpi_path = extensions_dir.join(format!("{}.xpi", extension_id));
    std::fs::write(&xpi_path, &xpi_bytes)
        .with_context(|| format!("Failed to write XPI to {}", xpi_path.display()))?;

    // Verify file was written
    if !xpi_path.exists() {
        anyhow::bail!("XPI file was not created at {}", xpi_path.display());
    }

    let written_size = std::fs::metadata(&xpi_path)?.len();
    if written_size != xpi_bytes.len() as u64 {
        anyhow::bail!(
            "XPI size mismatch: wrote {} bytes, file is {} bytes",
            xpi_bytes.len(),
            written_size
        );
    }

    info!(
        "Installed extension to {} ({} bytes)",
        xpi_path.display(),
        written_size
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_compress_decompress_roundtrip() {
        let original = b"Hello, this is some test data for compression!";
        let compressed = compress_xpi(original).unwrap();
        let decompressed = decompress_xpi(&compressed).unwrap();
        assert_eq!(original.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn test_encode_decode_base64_roundtrip() {
        let original = vec![0x00, 0x01, 0x02, 0xfe, 0xff];
        let encoded = encode_base64(&original);
        let decoded = decode_base64(&encoded).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_decode_base64_invalid() {
        let result = decode_base64("not valid base64!!!");
        assert!(result.is_err());
    }

    #[test]
    fn test_read_manifest_with_gecko_id() {
        let dir = tempdir().unwrap();
        let manifest = r#"{
            "manifest_version": 2,
            "name": "Test Extension",
            "version": "1.0.0",
            "browser_specific_settings": {
                "gecko": {
                    "id": "test@example.com"
                }
            }
        }"#;
        std::fs::write(dir.path().join("manifest.json"), manifest).unwrap();

        let result = read_manifest(dir.path()).unwrap();
        assert_eq!(result.id, "test@example.com");
        assert_eq!(result.name, "Test Extension");
        assert_eq!(result.version, "1.0.0");
    }

    #[test]
    fn test_read_manifest_with_applications_gecko() {
        let dir = tempdir().unwrap();
        let manifest = r#"{
            "manifest_version": 2,
            "name": "Test Extension",
            "version": "2.0.0",
            "applications": {
                "gecko": {
                    "id": "legacy@example.com"
                }
            }
        }"#;
        std::fs::write(dir.path().join("manifest.json"), manifest).unwrap();

        let result = read_manifest(dir.path()).unwrap();
        assert_eq!(result.id, "legacy@example.com");
    }

    #[test]
    fn test_read_manifest_generated_id() {
        let dir = tempdir().unwrap();
        let manifest = r#"{
            "manifest_version": 2,
            "name": "My Cool Extension",
            "version": "3.0.0"
        }"#;
        std::fs::write(dir.path().join("manifest.json"), manifest).unwrap();

        let result = read_manifest(dir.path()).unwrap();
        assert_eq!(result.id, "my-cool-extension@local");
        assert_eq!(result.name, "My Cool Extension");
    }

    #[test]
    fn test_read_manifest_defaults() {
        let dir = tempdir().unwrap();
        let manifest = r#"{
            "manifest_version": 2
        }"#;
        std::fs::write(dir.path().join("manifest.json"), manifest).unwrap();

        let result = read_manifest(dir.path()).unwrap();
        assert_eq!(result.name, "Unknown Extension");
        assert_eq!(result.version, "0.0.0");
    }

    #[test]
    fn test_read_manifest_not_found() {
        let dir = tempdir().unwrap();
        let result = read_manifest(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_read_manifest_invalid_json() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("manifest.json"), "not valid json").unwrap();
        let result = read_manifest(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_package_and_unpack_roundtrip() {
        let source_dir = tempdir().unwrap();
        let target_dir = tempdir().unwrap();

        // Create a minimal extension
        let manifest = r#"{
            "manifest_version": 2,
            "name": "Roundtrip Test",
            "version": "1.0.0",
            "browser_specific_settings": {
                "gecko": {
                    "id": "roundtrip@test.com"
                }
            }
        }"#;
        std::fs::write(source_dir.path().join("manifest.json"), manifest).unwrap();
        std::fs::write(
            source_dir.path().join("background.js"),
            "console.log('hello');",
        )
        .unwrap();

        // Package
        let (orig_manifest, xpi_data) = package_extension(source_dir.path()).unwrap();
        assert_eq!(orig_manifest.id, "roundtrip@test.com");
        assert_eq!(orig_manifest.name, "Roundtrip Test");

        // Unpack
        let unpacked_manifest = unpack_extension(&xpi_data, target_dir.path()).unwrap();
        assert_eq!(unpacked_manifest.id, orig_manifest.id);
        assert_eq!(unpacked_manifest.name, orig_manifest.name);
        assert_eq!(unpacked_manifest.version, orig_manifest.version);

        // Verify files exist
        assert!(target_dir.path().join("manifest.json").exists());
        assert!(target_dir.path().join("background.js").exists());
    }

    #[test]
    fn test_package_skips_hidden_files() {
        let source_dir = tempdir().unwrap();

        // Create extension with hidden file
        let manifest = r#"{"manifest_version": 2, "name": "Test", "version": "1.0.0"}"#;
        std::fs::write(source_dir.path().join("manifest.json"), manifest).unwrap();
        std::fs::write(source_dir.path().join(".gitignore"), "node_modules/").unwrap();

        let (_, xpi_data) = package_extension(source_dir.path()).unwrap();

        // Unpack and verify hidden file is not included
        let target_dir = tempdir().unwrap();
        unpack_extension(&xpi_data, target_dir.path()).unwrap();
        assert!(!target_dir.path().join(".gitignore").exists());
    }

    #[test]
    fn test_package_with_subdirectory() {
        let source_dir = tempdir().unwrap();

        let manifest = r#"{"manifest_version": 2, "name": "Test", "version": "1.0.0"}"#;
        std::fs::write(source_dir.path().join("manifest.json"), manifest).unwrap();

        // Create subdirectory
        let sub_dir = source_dir.path().join("icons");
        std::fs::create_dir(&sub_dir).unwrap();
        std::fs::write(sub_dir.join("icon.png"), "fake icon data").unwrap();

        let (_, xpi_data) = package_extension(source_dir.path()).unwrap();

        // Unpack and verify
        let target_dir = tempdir().unwrap();
        unpack_extension(&xpi_data, target_dir.path()).unwrap();
        assert!(target_dir.path().join("icons").join("icon.png").exists());
    }

    #[test]
    fn test_install_to_profile() {
        // First create a valid XPI
        let source_dir = tempdir().unwrap();
        let manifest = r#"{"manifest_version": 2, "name": "Install Test", "version": "1.0.0"}"#;
        std::fs::write(source_dir.path().join("manifest.json"), manifest).unwrap();

        let (_, xpi_data) = package_extension(source_dir.path()).unwrap();

        // Now install to a fake profile
        let profile_dir = tempdir().unwrap();
        install_to_profile(&xpi_data, profile_dir.path(), "test@example.com").unwrap();

        // Verify
        let xpi_path = profile_dir
            .path()
            .join("extensions")
            .join("test@example.com.xpi");
        assert!(xpi_path.exists());
    }

    #[test]
    fn test_install_to_profile_creates_extensions_dir() {
        let source_dir = tempdir().unwrap();
        let manifest = r#"{"manifest_version": 2, "name": "Test", "version": "1.0.0"}"#;
        std::fs::write(source_dir.path().join("manifest.json"), manifest).unwrap();

        let (_, xpi_data) = package_extension(source_dir.path()).unwrap();

        let profile_dir = tempdir().unwrap();
        // Don't pre-create extensions dir
        assert!(!profile_dir.path().join("extensions").exists());

        install_to_profile(&xpi_data, profile_dir.path(), "test@ext").unwrap();

        assert!(profile_dir.path().join("extensions").exists());
    }

    #[test]
    fn test_compress_empty_data() {
        let empty: &[u8] = &[];
        let compressed = compress_xpi(empty).unwrap();
        let decompressed = decompress_xpi(&compressed).unwrap();
        assert!(decompressed.is_empty());
    }

    #[test]
    fn test_compress_large_data() {
        let large_data: Vec<u8> = (0..100_000).map(|i| (i % 256) as u8).collect();
        let compressed = compress_xpi(&large_data).unwrap();
        let decompressed = decompress_xpi(&compressed).unwrap();
        assert_eq!(large_data, decompressed);

        // Compressed should be smaller
        assert!(compressed.len() < large_data.len());
    }
}
