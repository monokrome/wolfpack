mod devices;
mod extension;
mod ipc;
mod pair;
mod send;
mod status;

pub use devices::list_devices;
pub use extension::{install_extension, list_extensions, uninstall_extension};
pub use ipc::{is_daemon_running, send_command};
pub use pair::pair_device;
pub use send::send_tab;
pub use status::show_status;
