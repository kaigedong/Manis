use std::{error::Error, fmt, io};

use crate::ProcessExit;

/// Errors from configuration validation and owned child lifecycle operations.
#[derive(Debug)]
pub enum EngineError {
    /// A caller supplied an unsafe or incomplete managed configuration.
    InvalidConfig(String),
    /// An operating-system operation failed.
    Io {
        /// Stable operation label without user secrets.
        operation: &'static str,
        /// Original standard-library error.
        source: io::Error,
    },
    /// The selected kernel rejected the runtime configuration.
    ValidationFailed(ProcessExit),
    /// A second start was requested while an owned child exists.
    AlreadyRunning,
    /// The owned child exited before its controller became ready.
    ExitedEarly(ProcessExit),
    /// The owned child exited after it had become ready.
    Exited(ProcessExit),
    /// The controller did not become ready within the bounded policy.
    ReadinessTimeout { attempts: usize },
}

impl fmt::Display for EngineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(message) => formatter.write_str(message),
            Self::Io { operation, source } => write!(formatter, "{operation}: {source}"),
            Self::ValidationFailed(exit) => {
                write!(formatter, "kernel config validation failed ({exit})")
            }
            Self::AlreadyRunning => {
                formatter.write_str("a managed kernel child is already running")
            }
            Self::ExitedEarly(exit) => {
                write!(formatter, "managed kernel exited before readiness ({exit})")
            }
            Self::Exited(exit) => write!(formatter, "managed kernel exited ({exit})"),
            Self::ReadinessTimeout { attempts } => write!(
                formatter,
                "managed kernel controller was not ready after {attempts} attempts"
            ),
        }
    }
}

impl Error for EngineError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl fmt::Display for ProcessExit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.code() {
            Some(code) => write!(formatter, "exit code {code}"),
            None => formatter.write_str("terminated by signal"),
        }
    }
}
