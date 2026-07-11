//! Cloud-hosted configuration data for Codex.
//!
//! This crate owns transport, caching, and refresh behavior for cloud-delivered
//! config data. Parsing and composition remain in `codex-config`.

#[cfg(test)]
mod backend;
mod bundle_loader;
#[cfg(test)]
mod cache;
#[cfg(test)]
mod metrics;
#[cfg(test)]
mod service;
#[cfg(test)]
mod validation;

pub use bundle_loader::cloud_config_bundle_loader;
pub use bundle_loader::cloud_config_bundle_loader_for_storage;
