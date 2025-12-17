mod package;

pub use package::{
    ExtensionManifest, compress_xpi, decode_base64, decompress_xpi, encode_base64,
    install_to_profile, package_extension, read_manifest, unpack_extension,
};

use anyhow::Result;
use std::path::Path;
use tempfile::TempDir;

use crate::events::ExtensionSource;

/// Install from a local XPI file
pub fn install_from_xpi(xpi_path: &Path) -> Result<InstallResult> {
    let xpi_bytes = std::fs::read(xpi_path)?;

    // Compress and encode
    let compressed = compress_xpi(&xpi_bytes)?;
    let xpi_data = encode_base64(&compressed);

    // Extract to temp to read manifest
    let temp_dir = TempDir::new()?;
    let manifest = unpack_extension(&xpi_data, temp_dir.path())?;

    Ok(InstallResult {
        id: manifest.id,
        name: manifest.name,
        version: manifest.version,
        source: ExtensionSource::Local {
            original_path: xpi_path.display().to_string(),
        },
        xpi_data,
    })
}

/// Result of installing an extension
#[derive(Debug, Clone)]
pub struct InstallResult {
    pub id: String,
    pub name: String,
    pub version: String,
    pub source: ExtensionSource,
    pub xpi_data: String,
}
