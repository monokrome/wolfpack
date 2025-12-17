use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handler {
    pub protocol: String,
    pub handler: String,
}

#[derive(Serialize, Deserialize)]
struct HandlersFile {
    #[serde(rename = "defaultHandlersVersion")]
    default_handlers_version: Option<HashMap<String, u32>>,
    #[serde(rename = "schemes")]
    schemes: HashMap<String, SchemeHandler>,
    #[serde(rename = "mimeTypes", default)]
    mime_types: HashMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
struct SchemeHandler {
    action: u32,
    handlers: Vec<HandlerEntry>,
}

#[derive(Serialize, Deserialize)]
struct HandlerEntry {
    name: String,
    #[serde(rename = "uriTemplate")]
    uri_template: String,
}

pub fn read_handlers(profile_path: &Path) -> Result<Vec<Handler>> {
    let handlers_path = profile_path.join("handlers.json");

    if !handlers_path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&handlers_path)
        .with_context(|| format!("Failed to read {}", handlers_path.display()))?;

    let file: HandlersFile =
        serde_json::from_str(&content).context("Failed to parse handlers.json")?;

    let handlers = file
        .schemes
        .into_iter()
        .filter_map(|(protocol, scheme)| {
            scheme.handlers.first().map(|h| Handler {
                protocol,
                handler: h.uri_template.clone(),
            })
        })
        .collect();

    Ok(handlers)
}

pub fn write_handlers(profile_path: &Path, handlers: &[Handler]) -> Result<()> {
    let handlers_path = profile_path.join("handlers.json");

    // Read existing file to preserve structure, or create new
    let mut file: HandlersFile = if handlers_path.exists() {
        let content = std::fs::read_to_string(&handlers_path)
            .with_context(|| format!("Failed to read {}", handlers_path.display()))?;
        serde_json::from_str(&content).context("Failed to parse handlers.json")?
    } else {
        HandlersFile {
            default_handlers_version: None,
            schemes: HashMap::new(),
            mime_types: HashMap::new(),
        }
    };

    // Update schemes with new handlers
    for handler in handlers {
        file.schemes.insert(
            handler.protocol.clone(),
            SchemeHandler {
                action: 2, // useHelperApp
                handlers: vec![HandlerEntry {
                    name: handler.protocol.clone(),
                    uri_template: handler.handler.clone(),
                }],
            },
        );
    }

    let content = serde_json::to_string_pretty(&file).context("Failed to serialize handlers")?;

    std::fs::write(&handlers_path, content)
        .with_context(|| format!("Failed to write {}", handlers_path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_handlers_roundtrip() {
        let dir = tempdir().unwrap();
        let handlers = vec![Handler {
            protocol: "mailto".to_string(),
            handler: "https://mail.example.com/compose?to=%s".to_string(),
        }];

        write_handlers(dir.path(), &handlers).unwrap();
        let loaded = read_handlers(dir.path()).unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].protocol, "mailto");
        assert!(loaded[0].handler.contains("mail.example.com"));
    }
}
