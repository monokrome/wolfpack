mod build;
mod package;

pub use build::{BuildSystem, clone_repo, find_manifest, run_build};
pub use package::{
    ExtensionManifest, compress_xpi, decode_base64, decompress_xpi, encode_base64,
    install_to_profile, package_extension, read_manifest, unpack_extension,
};

use anyhow::Result;
use std::path::Path;
use tempfile::TempDir;
use tracing::info;

use crate::events::ExtensionSource;

/// Full pipeline: git URL -> installed extension + event data
#[allow(clippy::cognitive_complexity)] // Installation pipeline with multiple steps
pub fn install_from_git(
    url: &str,
    ref_spec: &str,
    custom_build_cmd: Option<&str>,
) -> Result<InstallResult> {
    // Create temp directory for build
    let temp_dir = TempDir::new()?;
    let repo_dir = temp_dir.path();

    // Clone
    clone_repo(url, ref_spec, repo_dir)?;

    // Detect or use custom build system
    let build_system = if let Some(cmd) = custom_build_cmd {
        BuildSystem::Custom {
            command: cmd.to_string(),
        }
    } else {
        BuildSystem::detect(repo_dir)?
    };

    info!("Build system: {:?}", build_system);

    // Build
    run_build(repo_dir, &build_system)?;

    // Find manifest
    let extension_dir = find_manifest(repo_dir)?;
    info!("Found extension at {}", extension_dir.display());

    // Package
    let (manifest, xpi_data) = package_extension(&extension_dir)?;

    Ok(InstallResult {
        id: manifest.id,
        name: manifest.name,
        version: manifest.version,
        source: ExtensionSource::Git {
            url: url.to_string(),
            ref_spec: ref_spec.to_string(),
            build_cmd: build_system.to_command_string(),
        },
        xpi_data,
    })
}

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
