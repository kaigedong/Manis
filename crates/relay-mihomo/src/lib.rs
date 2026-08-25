#![forbid(unsafe_code)]

mod catalog;
mod client;
mod config;
mod error;
mod http;
mod models;

pub use catalog::to_policy_catalog;
pub use client::MihomoClient;
pub use config::ControllerConfig;
pub use error::MihomoError;
#[cfg(unix)]
pub use http::UnixSocketTransport;
pub use http::{ReadonlyTransport, StdHttpTransport};
pub use models::{
    Connection, ConnectionMetadata, ConnectionsState, DelayHistory, GroupKind, MihomoSnapshot,
    ObservedRouteEvidence, PolicyGroup, Proxy, Rule, RuleExtra, VersionInfo,
};
