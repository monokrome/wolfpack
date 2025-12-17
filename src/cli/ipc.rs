use anyhow::{Context, Result};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

pub fn socket_path() -> PathBuf {
    dirs::runtime_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("wolfpack.sock")
}

pub fn send_command(command: &str) -> Result<String> {
    let path = socket_path();
    let mut stream = UnixStream::connect(&path).with_context(|| {
        format!(
            "Failed to connect to daemon at {}. Is the daemon running?",
            path.display()
        )
    })?;

    stream.write_all(command.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;

    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader.read_line(&mut response)?;

    Ok(response.trim().to_string())
}

pub fn is_daemon_running() -> bool {
    socket_path().exists() && UnixStream::connect(socket_path()).is_ok()
}
