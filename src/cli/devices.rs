use anyhow::{Context, Result};
use std::fs;

use crate::config::Config;

pub fn list_devices() -> Result<()> {
    let config_path = Config::default_path();
    if !config_path.exists() {
        println!("Not initialized. Run: wolfpack init");
        return Ok(());
    }

    let config = Config::load(&config_path)?;
    let keys_dir = config.paths.sync_dir.join("keys");

    println!("This device:");
    println!("  ID: {}", config.device.id);
    println!("  Name: {}", config.device.name);
    println!();

    if !keys_dir.exists() {
        println!("No other devices paired yet.");
        println!("To pair, share your public key with: wolfpack pair");
        return Ok(());
    }

    println!("Known devices:");
    let mut found = false;
    for entry in
        fs::read_dir(&keys_dir).with_context(|| format!("Failed to read {}", keys_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "pub") {
            let name = path.file_stem().unwrap_or_default().to_string_lossy();
            let content = fs::read_to_string(&path)?;
            println!("  {}: {}", name, content.trim());
            found = true;
        }
    }

    if !found {
        println!("  (none)");
    }

    Ok(())
}
