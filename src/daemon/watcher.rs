use anyhow::Result;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc;
use tokio::sync::broadcast;

pub struct FileWatcher {
    _watcher: RecommendedWatcher,
    pub events: broadcast::Receiver<notify::Event>,
}

impl FileWatcher {
    pub fn new(paths: &[&Path]) -> Result<Self> {
        let (tx, _rx) = broadcast::channel(100);
        let tx_clone = tx.clone();

        let (sync_tx, sync_rx) = mpsc::channel();

        let mut watcher = RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = sync_tx.send(event);
                }
            },
            Config::default(),
        )?;

        for path in paths {
            watcher.watch(path, RecursiveMode::Recursive)?;
        }

        // Spawn a task to forward events from sync channel to async broadcast
        std::thread::spawn(move || {
            while let Ok(event) = sync_rx.recv() {
                let _ = tx_clone.send(event);
            }
        });

        Ok(Self {
            _watcher: watcher,
            events: tx.subscribe(),
        })
    }
}
