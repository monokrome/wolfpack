use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Container {
    #[serde(rename = "userContextId")]
    pub user_context_id: u32,
    pub name: String,
    pub icon: String,
    pub color: String,
    #[serde(rename = "public", default)]
    pub is_public: bool,
}

#[derive(Serialize, Deserialize)]
struct ContainersFile {
    version: u32,
    #[serde(rename = "lastUserContextId")]
    last_user_context_id: u32,
    identities: Vec<Container>,
}

pub fn read_containers(profile_path: &Path) -> Result<Vec<Container>> {
    let containers_path = profile_path.join("containers.json");

    if !containers_path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&containers_path)
        .with_context(|| format!("Failed to read {}", containers_path.display()))?;

    let file: ContainersFile =
        serde_json::from_str(&content).context("Failed to parse containers.json")?;

    Ok(file.identities)
}

pub fn write_containers(profile_path: &Path, containers: &[Container]) -> Result<()> {
    let containers_path = profile_path.join("containers.json");

    let last_id = containers
        .iter()
        .map(|c| c.user_context_id)
        .max()
        .unwrap_or(0);

    let file = ContainersFile {
        version: 4,
        last_user_context_id: last_id,
        identities: containers.to_vec(),
    };

    let content = serde_json::to_string_pretty(&file).context("Failed to serialize containers")?;

    std::fs::write(&containers_path, content)
        .with_context(|| format!("Failed to write {}", containers_path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_containers_roundtrip() {
        let dir = tempdir().unwrap();
        let containers = vec![Container {
            user_context_id: 1,
            name: "Work".to_string(),
            icon: "briefcase".to_string(),
            color: "blue".to_string(),
            is_public: true,
        }];

        write_containers(dir.path(), &containers).unwrap();
        let loaded = read_containers(dir.path()).unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "Work");
    }
}
