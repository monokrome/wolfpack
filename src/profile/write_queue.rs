use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{Container, Handler, write_containers, write_handlers, write_user_js};
use crate::events::PrefValue;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PendingWrite {
    Containers(Vec<Container>),
    Handlers(Vec<Handler>),
    Prefs(HashMap<String, PrefValue>),
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct WriteQueue {
    profile_path: PathBuf,
    pending: Vec<PendingWrite>,
}

impl WriteQueue {
    pub fn new(profile_path: PathBuf) -> Self {
        Self {
            profile_path,
            pending: Vec::new(),
        }
    }

    pub fn load(queue_path: &Path) -> Result<Self> {
        if !queue_path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(queue_path)
            .with_context(|| format!("Failed to read {}", queue_path.display()))?;

        serde_json::from_str(&content).context("Failed to parse write queue")
    }

    pub fn save(&self, queue_path: &Path) -> Result<()> {
        let content =
            serde_json::to_string_pretty(self).context("Failed to serialize write queue")?;

        std::fs::write(queue_path, content)
            .with_context(|| format!("Failed to write {}", queue_path.display()))?;

        Ok(())
    }

    pub fn queue_containers(&mut self, containers: Vec<Container>) {
        // Replace any existing pending containers write
        self.pending
            .retain(|w| !matches!(w, PendingWrite::Containers(_)));
        self.pending.push(PendingWrite::Containers(containers));
    }

    pub fn queue_handlers(&mut self, handlers: Vec<Handler>) {
        self.pending
            .retain(|w| !matches!(w, PendingWrite::Handlers(_)));
        self.pending.push(PendingWrite::Handlers(handlers));
    }

    pub fn queue_prefs(&mut self, prefs: HashMap<String, PrefValue>) {
        self.pending
            .retain(|w| !matches!(w, PendingWrite::Prefs(_)));
        self.pending.push(PendingWrite::Prefs(prefs));
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn flush(&mut self) -> Result<Vec<String>> {
        let mut applied = Vec::new();

        for write in self.pending.drain(..) {
            match write {
                PendingWrite::Containers(containers) => {
                    write_containers(&self.profile_path, &containers)?;
                    applied.push("containers.json".to_string());
                }
                PendingWrite::Handlers(handlers) => {
                    write_handlers(&self.profile_path, &handlers)?;
                    applied.push("handlers.json".to_string());
                }
                PendingWrite::Prefs(prefs) => {
                    write_user_js(&self.profile_path, &prefs)?;
                    applied.push("user.js".to_string());
                }
            }
        }

        Ok(applied)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_write_queue_roundtrip() {
        let dir = tempdir().unwrap();
        let queue_path = dir.path().join("queue.json");
        let profile_path = dir.path().join("profile");
        std::fs::create_dir_all(&profile_path).unwrap();

        let mut queue = WriteQueue::new(profile_path);
        queue.queue_containers(vec![Container {
            user_context_id: 1,
            name: "Test".to_string(),
            icon: "circle".to_string(),
            color: "blue".to_string(),
            is_public: true,
        }]);

        queue.save(&queue_path).unwrap();

        let loaded = WriteQueue::load(&queue_path).unwrap();
        assert!(!loaded.is_empty());
    }

    #[test]
    fn test_write_queue_flush() {
        let dir = tempdir().unwrap();
        let profile_path = dir.path().to_path_buf();

        let mut queue = WriteQueue::new(profile_path.clone());
        queue.queue_containers(vec![Container {
            user_context_id: 1,
            name: "Test".to_string(),
            icon: "circle".to_string(),
            color: "blue".to_string(),
            is_public: true,
        }]);

        let applied = queue.flush().unwrap();
        assert_eq!(applied, vec!["containers.json"]);
        assert!(queue.is_empty());
        assert!(profile_path.join("containers.json").exists());
    }
}
