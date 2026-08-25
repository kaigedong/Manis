use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum MihomoError {
    InvalidConfig(String),
    InvalidRequestPath(String),
    InvalidResponse(String),
    BodyTooLarge {
        limit: usize,
    },
    HttpStatus {
        status_code: u16,
        reason: String,
        body_preview: String,
    },
    Io(std::io::Error),
    Json {
        endpoint: String,
        source: serde_json::Error,
    },
}

impl fmt::Display for MihomoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(message) => {
                write!(formatter, "invalid controller config: {message}")
            }
            Self::InvalidRequestPath(path) => write!(formatter, "invalid request path: {path}"),
            Self::InvalidResponse(message) => write!(formatter, "invalid HTTP response: {message}"),
            Self::BodyTooLarge { limit } => {
                write!(formatter, "HTTP response body exceeded {limit} bytes")
            }
            Self::HttpStatus {
                status_code,
                reason,
                body_preview,
            } => write!(
                formatter,
                "controller returned HTTP {status_code} {reason}: {body_preview}"
            ),
            Self::Io(source) => write!(formatter, "controller I/O failed: {source}"),
            Self::Json { endpoint, source } => {
                write!(
                    formatter,
                    "failed to parse Mihomo JSON from {endpoint}: {source}"
                )
            }
        }
    }
}

impl Error for MihomoError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(source) => Some(source),
            Self::Json { source, .. } => Some(source),
            Self::InvalidConfig(_)
            | Self::InvalidRequestPath(_)
            | Self::InvalidResponse(_)
            | Self::BodyTooLarge { .. }
            | Self::HttpStatus { .. } => None,
        }
    }
}

impl From<std::io::Error> for MihomoError {
    fn from(source: std::io::Error) -> Self {
        Self::Io(source)
    }
}
