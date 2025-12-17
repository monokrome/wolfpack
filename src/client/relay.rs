pub struct RelayClient {
    _base_url: String,
}

impl RelayClient {
    pub fn new(base_url: String) -> Self {
        Self { _base_url: base_url }
    }

    // TODO: Implement relay client methods
    // - upload_events
    // - download_events
    // - poll_for_updates
}
