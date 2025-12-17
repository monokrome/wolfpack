#[cfg(feature = "server")]
mod relay;
#[cfg(feature = "server")]
mod discovery;

#[cfg(feature = "server")]
pub use relay::run_relay_server;
#[cfg(feature = "server")]
pub use discovery::run_discovery_server;
