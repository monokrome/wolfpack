#[cfg(feature = "client")]
mod relay;
#[cfg(feature = "client")]
mod discovery;

#[cfg(feature = "client")]
pub use relay::RelayClient;
#[cfg(feature = "client")]
pub use discovery::DiscoveryClient;
