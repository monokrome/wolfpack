use anyhow::Result;

pub async fn run_discovery_server(_bind: &str) -> Result<()> {
    // TODO: Implement discovery server
    // - Register device name → public key mapping
    // - Lookup devices by name
    // - Challenge-response authentication
    println!("Discovery server not yet implemented");
    Ok(())
}
