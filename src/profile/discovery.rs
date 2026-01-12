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
    for (section, props) in ini.iter() {
        if let Some(section_name) = section {
            if section_name.starts_with("Install") {
                if let Some(default_path) = props.get("Default") {
                    let profile_path = if default_path.contains('/') || default_path.contains('\\') {
                        // Relative path like "Profiles/xxx.default-default"
                        base.join(default_path)
                    } else {
                        // Just profile name
                        base.join("Profiles").join(default_path)
                    };

                    if profile_path.join("prefs.js").exists() {
                        return Ok(profile_path);
                    }
                }
            }
        }
    }

    // Fallback: look for Default=1 in [ProfileN] sections
    let mut profiles = HashMap::new();
    for (section, props) in ini.iter() {
        if let Some(section_name) = section {
            if section_name.starts_with("Profile") {
                if let Some(path_str) = props.get("Path") {
                    let is_default = props.get("Default").map_or(false, |v| v == "1");
                    let is_relative = props.get("IsRelative").map_or(false, |v| v == "1");

                    let profile_path = if is_relative {
                        base.join(path_str)
                    } else {
                        PathBuf::from(path_str)
                    };

                    profiles.insert(section_name, (profile_path, is_default));
                }
            }
        }
    }

    // Return the profile marked Default=1
    for (_name, (path, is_default)) in profiles {
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
