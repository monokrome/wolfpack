use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn find_profile() -> Result<PathBuf> {
    let base = librewolf_base_path()?;
    let profiles_ini = base.join("profiles.ini");

    if !profiles_ini.exists() {
        anyhow::bail!("profiles.ini not found in {}", base.display());
    }

    let content = std::fs::read_to_string(&profiles_ini)
        .with_context(|| format!("Failed to read {}", profiles_ini.display()))?;

    parse_profiles_ini(&content, &base)
}

fn parse_profiles_ini(content: &str, base: &Path) -> Result<PathBuf> {
    let ini = ini::Ini::load_from_str(content)
        .context("Failed to parse profiles.ini")?;

    // First try to find default profile from [InstallXXX] section
    // The Default field contains the profile name, which we need to match against Path fields
    let mut install_default: Option<String> = None;
    for (section, props) in ini.iter() {
        if let Some(section_name) = section {
            if section_name.starts_with("Install") {
                if let Some(default_name) = props.get("Default") {
                    install_default = Some(default_name.to_string());
                    break;
                }
            }
        }
    }

    // Collect all profiles with their paths
    let mut profiles = HashMap::new();
    for (section, props) in ini.iter() {
        if let Some(section_name) = section {
            if section_name.starts_with("Profile") {
                if let Some(path_str) = props.get("Path") {
                    let is_default = props.get("Default").map_or(false, |v| v == "1");
                    let is_relative = props.get("IsRelative").map_or(false, |v| v == "1");
                    let name = props.get("Name").map(|s| s.to_string());

                    let profile_path = if is_relative {
                        base.join(path_str)
                    } else {
                        PathBuf::from(path_str)
                    };

                    profiles.insert(
                        path_str.to_string(),
                        (profile_path, is_default, name),
                    );
                }
            }
        }
    }

    // First try to match [InstallXXX] Default= field
    if let Some(default_name) = install_default {
        if let Some((path, _, _)) = profiles.get(&default_name) {
            if path.join("prefs.js").exists() {
                return Ok(path.clone());
            }
        }
    }

    // Fallback: use profile with Default=1
    for (_path_str, (path, is_default, _name)) in profiles {
        if is_default && path.join("prefs.js").exists() {
            return Ok(path);
        }
    }

    anyhow::bail!("No default profile found in {}", base.display())
}

fn librewolf_base_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;

    #[cfg(target_os = "linux")]
    let candidates = vec![home.join(".librewolf"), home.join(".mozilla/firefox")];

    #[cfg(target_os = "macos")]
    let candidates = vec![
        home.join("Library/Application Support/librewolf"),
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
