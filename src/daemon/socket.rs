use anyhow::Result;
use std::path::Path;
use tokio::net::UnixListener;

pub struct IpcSocket {
    listener: UnixListener,
}

impl IpcSocket {
    pub async fn new(path: &Path) -> Result<Self> {
        // Remove existing socket if present
        if path.exists() {
            std::fs::remove_file(path)?;
        }

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(path)?;
        Ok(Self { listener })
    }

    pub fn listener(&self) -> &UnixListener {
        &self.listener
    }
}
