#![forbid(unsafe_code)]

mod command;
mod config;
mod error;
mod lifecycle;
mod process;

pub use command::CommandSpec;
pub use config::{ControllerEndpoint, ManagedEngineConfig, validate_managed_config};
pub use error::EngineError;
pub use lifecycle::{EngineManager, EngineState, ProbeStatus, ReadinessPolicy, ReadinessProbe};
pub use process::{ManagedChild, ProcessExit, ProcessSpawner, StdProcessSpawner};

#[cfg(test)]
pub(crate) use process::{CHILD_ENV_ALLOWLIST, resolved_command};

#[cfg(test)]
mod tests;
