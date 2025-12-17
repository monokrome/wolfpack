use anyhow::Result;

use super::ipc;

pub fn show_status() -> Result<()> {
    if !ipc::is_daemon_running() {
        println!("Daemon is not running");
        println!("Start with: wolfpack daemon");
        return Ok(());
    }

    let response = ipc::send_command("status")?;
    println!("{}", response);

    // Also show pending tabs
    let tabs_response = ipc::send_command("tabs")?;
    println!("\nPending tabs:");
    println!("{}", tabs_response);

    Ok(())
}
