use anyhow::Result;

pub async fn run_relay_server(_bind: &str) -> Result<()> {
    // TODO: Implement relay server
    // - Accept encrypted event uploads
    // - Store events by device public key
    // - Allow downloads of events
    println!("Relay server not yet implemented");
    Ok(())
}
