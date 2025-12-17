pub struct DiscoveryClient {
    _base_url: String,
}

impl DiscoveryClient {
    pub fn new(base_url: String) -> Self {
        Self { _base_url: base_url }
    }

    // TODO: Implement discovery client methods
    // - register_device
    // - lookup_device
    // - verify_ownership
}
