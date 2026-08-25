#![forbid(unsafe_code)]

mod catalog;
mod client;
mod config;
mod error;
mod http;
mod live;
mod models;

pub use catalog::to_policy_catalog;
pub use client::MihomoClient;
pub use config::ControllerConfig;
pub use error::MihomoError;
#[cfg(unix)]
pub use http::UnixSocketTransport;
pub use http::{ControllerTransport, StdHttpTransport};
pub use live::LiveController;
pub use models::{
    Connection, ConnectionMetadata, ConnectionsState, DelayHistory, GroupKind, MihomoLogEntry,
    MihomoPolicyGroup, MihomoSnapshot, ObservedRouteEvidence, PolicyGroup, Proxy, ProxyProvider,
    Rule, RuleExtra, RuntimeConfig, RuntimeTunConfig, VersionInfo,
};
