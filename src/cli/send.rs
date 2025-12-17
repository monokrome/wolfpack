use anyhow::Result;

use super::ipc;

pub fn send_tab(url: &str, to_device: &str) -> Result<()> {
    let command = format!("send {} {}", to_device, url);
    let response = ipc::send_command(&command)?;

    if response.starts_with("OK:") {
        println!("Tab sent to {}", to_device);
    } else {
        anyhow::bail!("{}", response);
    }

    Ok(())
}
