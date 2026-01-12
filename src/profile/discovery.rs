use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn find_profile() -> Result<PathBuf> {
    let base = librewolf_base_path()?;

    let mut default_release = None;
    let mut default = None;

    for entry in std::fs::read_dir(&base)
        .with_context(|| format!("Failed to read LibreWolf directory: {}", base.display()))?
    {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let path = entry.path();

        if !path.join("prefs.js").exists() {
            continue;
        }

        if name_str.ends_with(".default-release") {
            default_release = Some(path);
        } else if name_str.ends_with(".default") && default.is_none() {
            default = Some(path);
        }
    }

    // Prefer .default-release over .default
    default_release
        .or(default)
        .ok_or_else(|| anyhow::anyhow!("No LibreWolf profile found in {}", base.display()))
}

fn librewolf_base_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;

    #[cfg(target_os = "linux")]
    let candidates = vec![home.join(".librewolf"), home.join(".mozilla/firefox")];

    #[cfg(target_os = "macos")]
    let candidates = vec![
        home.join("Library/Application Support/librewolf/Profiles"),
        home.join("Library/Application Support/LibreWolf"),
        home.join("Library/Application Support/Firefox"),
    ];

    #[cfg(target_os = "windows")]
    let candidates = vec![
        home.join("AppData/Roaming/LibreWolf"),
        home.join("AppData/Roaming/Mozilla/Firefox"),
    ];

    for candidate in candidates {
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    anyhow::bail!("LibreWolf/Firefox directory not found")
}

pub fn is_browser_running(profile_path: &Path) -> bool {
    profile_path.join("lock").exists() || profile_path.join(".parentlock").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_profile_exists() {
        // This test will only pass if LibreWolf/Firefox is installed
        if let Ok(profile) = find_profile() {
            assert!(profile.exists());
            assert!(profile.join("prefs.js").exists());
        }
    }
}
