use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

use super::mozlz4::decode_mozlz4;

#[derive(Debug, Clone)]
pub struct SearchEngine {
    pub id: String,
    pub name: String,
    pub url: String,
    pub is_default: bool,
}

#[derive(Deserialize)]
struct SearchFile {
    engines: Vec<Engine>,
    #[serde(rename = "metaData")]
    metadata: Option<Metadata>,
}

#[derive(Deserialize)]
struct Engine {
    #[serde(rename = "_name")]
    name: String,
    #[serde(rename = "_loadPath")]
    load_path: Option<String>,
    #[serde(rename = "_metaData")]
    meta_data: Option<EngineMeta>,
}

#[derive(Deserialize)]
struct EngineMeta {
    alias: Option<String>,
}

#[derive(Deserialize)]
struct Metadata {
    #[serde(rename = "defaultEngineId")]
    default_engine_id: Option<String>,
}

pub fn read_search_engines(profile_path: &Path) -> Result<Vec<SearchEngine>> {
    let search_path = profile_path.join("search.json.mozlz4");

    if !search_path.exists() {
        return Ok(Vec::new());
    }

    let compressed = std::fs::read(&search_path)
        .with_context(|| format!("Failed to read {}", search_path.display()))?;

    let decompressed =
        decode_mozlz4(&compressed).context("Failed to decompress search.json.mozlz4")?;

    let file: SearchFile =
        serde_json::from_slice(&decompressed).context("Failed to parse search.json")?;

    let default_id = file
        .metadata
        .and_then(|m| m.default_engine_id)
        .unwrap_or_default();

    let engines = file
        .engines
        .into_iter()
        .map(|engine| {
            let id = engine
                .meta_data
                .and_then(|m| m.alias)
                .unwrap_or_else(|| engine.name.to_lowercase().replace(' ', "-"));

            SearchEngine {
                is_default: id == default_id,
                id,
                name: engine.name,
                url: engine.load_path.unwrap_or_default(),
            }
        })
        .collect();

    Ok(engines)
}
