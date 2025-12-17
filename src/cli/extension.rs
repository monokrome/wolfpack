use anyhow::{Context, Result};
use std::path::Path;

use crate::config::Config;
use crate::events::Event;
use crate::extensions::{install_from_xpi, install_to_profile};
use crate::state::StateDb;

/// Load config or use defaults if it doesn't exist
fn load_or_default_config(config_path: &Path) -> Config {
    Config::load(config_path).unwrap_or_default()
}

/// Install an extension from a local XPI file
pub fn install_extension(xpi_path: &Path, config_path: &Path) -> Result<()> {
    let config = load_or_default_config(config_path);
    let profile_dir = config.profile_dir()?;

    println!("Installing extension from {}...", xpi_path.display());

    let result = install_from_xpi(xpi_path)?;

    println!("Loaded {} v{}", result.name, result.version);
    println!("Extension ID: {}", result.id);

    // Store in local state
    let state_path = config.state_db_path();
    std::fs::create_dir_all(state_path.parent().unwrap_or(Path::new(".")))?;
    let db = StateDb::open(&state_path)?;
    db.store_extension_xpi(
        &result.id,
        &result.version,
        &result.source,
        &result.xpi_data,
    )?;
    db.add_extension(&result.id, &result.name, None)?;

    // Install to profile
    install_to_profile(&result.xpi_data, &profile_dir, &result.id)?;

    let installed_path = profile_dir
        .join("extensions")
        .join(format!("{}.xpi", result.id));

    // Verify file exists after install
    if installed_path.exists() {
        let meta = std::fs::metadata(&installed_path)?;
        println!(
            "Verified: {} exists ({} bytes)",
            installed_path.display(),
            meta.len()
        );
    } else {
        println!(
            "WARNING: File does not exist after install: {}",
            installed_path.display()
        );
    }

    // Store pending event for daemon to sync
    store_pending_extension_event(
        &state_path,
        Event::ExtensionInstalled {
            id: result.id.clone(),
            name: result.name.clone(),
            version: result.version,
            source: result.source,
            xpi_data: result.xpi_data,
        },
    )?;

    println!("Restart LibreWolf to activate the extension.");

    Ok(())
}

/// List installed extensions
pub fn list_extensions(config_path: &Path, show_missing: bool) -> Result<()> {
    let config = load_or_default_config(config_path);
    let state_path = config.state_db_path();

    if !state_path.exists() {
        println!("No synced extensions (state database not initialized).");
        return Ok(());
    }

    let db = StateDb::open(&state_path)?;
    let extensions = db.get_extensions()?;

    if extensions.is_empty() {
        println!("No synced extensions.");
        return Ok(());
    }

    println!("Synced extensions:");
    for (id, name, url) in &extensions {
        let installed = db.get_extension_xpi(id)?.is_some();
        let status = if installed { "installed" } else { "missing" };

        if show_missing && installed {
            continue;
        }

        if let Some(url) = url {
            println!("  {} ({}) [{}] - {}", name, id, status, url);
        } else {
            println!("  {} ({}) [{}]", name, id, status);
        }
    }

    Ok(())
}

/// Uninstall an extension
pub fn uninstall_extension(extension_id: &str, config_path: &Path) -> Result<()> {
    let config = load_or_default_config(config_path);
    let state_path = config.state_db_path();

    if !state_path.exists() {
        anyhow::bail!(
            "State database not found. Extension {} may not be tracked.",
            extension_id
        );
    }

    let db = StateDb::open(&state_path)?;

    // Check if extension exists
    let extensions = db.get_extensions()?;
    let found = extensions.iter().any(|(id, _, _)| id == extension_id);
    if !found {
        anyhow::bail!("Extension {} not found in sync database", extension_id);
    }

    // Remove from local state
    db.remove_extension(extension_id)?;
    db.remove_extension_xpi(extension_id)?;

    // Remove from profile
    let profile_dir = config.profile_dir()?;
    let xpi_path = profile_dir
        .join("extensions")
        .join(format!("{}.xpi", extension_id));
    if xpi_path.exists() {
        std::fs::remove_file(&xpi_path)
            .with_context(|| format!("Failed to remove {}", xpi_path.display()))?;
        println!("Removed XPI from profile.");
    }

    // Store pending event for daemon to sync
    store_pending_extension_event(
        &state_path,
        Event::ExtensionUninstalled {
            id: extension_id.to_string(),
        },
    )?;

    println!("Extension {} uninstalled.", extension_id);
    println!("The daemon will sync this removal to other devices.");
    println!("Restart LibreWolf to complete removal.");

    Ok(())
}

/// Store a pending extension event for the daemon to sync
fn store_pending_extension_event(state_path: &Path, event: Event) -> Result<()> {
    let pending_dir = state_path
        .parent()
        .unwrap_or(Path::new("."))
        .join("pending_events");
    std::fs::create_dir_all(&pending_dir)?;

    let event_id = uuid::Uuid::now_v7();
    let event_path = pending_dir.join(format!("{}.json", event_id));
    let event_json = serde_json::to_string_pretty(&event)?;
    std::fs::write(&event_path, event_json)?;

    Ok(())
}
