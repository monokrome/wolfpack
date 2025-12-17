use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Extension {
    pub id: String,
    pub name: String,
    pub url: Option<String>,
}

#[derive(Deserialize)]
struct ExtensionsFile {
    addons: Vec<Addon>,
}

#[derive(Deserialize)]
struct Addon {
    id: String,
    #[serde(rename = "type")]
    addon_type: String,
    #[serde(rename = "defaultLocale")]
    default_locale: Option<DefaultLocale>,
    name: Option<String>,
    #[serde(rename = "sourceURI")]
    source_uri: Option<String>,
}

#[derive(Deserialize)]
struct DefaultLocale {
    name: Option<String>,
}

pub fn read_extensions(profile_path: &Path) -> Result<Vec<Extension>> {
    let extensions_path = profile_path.join("extensions.json");
    let content = std::fs::read_to_string(&extensions_path)
        .with_context(|| format!("Failed to read {}", extensions_path.display()))?;

    let file: ExtensionsFile =
        serde_json::from_str(&content).context("Failed to parse extensions.json")?;

    let extensions = file
        .addons
        .into_iter()
        .filter(|addon| addon.addon_type == "extension")
        .map(|addon| {
            let name = addon
                .default_locale
                .and_then(|dl| dl.name)
                .or(addon.name)
                .unwrap_or_else(|| addon.id.clone());

            Extension {
                id: addon.id,
                name,
                url: addon.source_uri,
            }
        })
        .collect();

    Ok(extensions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::find_profile;

    #[test]
    fn test_read_extensions() {
        if let Ok(profile) = find_profile() {
            // Just verify it doesn't error - extensions may be empty in CI
            let _extensions = read_extensions(&profile).unwrap();
        }
    }
}
