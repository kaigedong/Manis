use std::{fmt, io};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileError {
    InvalidUrl,
    InvalidName,
    InvalidValue(&'static str),
    InvalidVless,
    UnsupportedVless,
    DuplicateName,
    DanglingReference,
    MissingTerminalMatch,
    UnsupportedKernelFeature(&'static str),
    Serialization(&'static str),
}

impl fmt::Display for ProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUrl => formatter.write_str("secret URL is invalid"),
            Self::InvalidName => formatter.write_str("profile name is invalid"),
            Self::InvalidValue(label) => write!(formatter, "profile {label} is invalid"),
            Self::InvalidVless => formatter.write_str("VLESS source is invalid"),
            Self::UnsupportedVless => formatter.write_str("VLESS option is not supported"),
            Self::DuplicateName => formatter.write_str("profile names must be unique"),
            Self::DanglingReference => formatter.write_str("profile contains a dangling reference"),
            Self::MissingTerminalMatch => {
                formatter.write_str("MATCH must be the final profile rule when present")
            }
            Self::Serialization(format) => {
                write!(formatter, "could not serialize {format} configuration")
            }
            Self::UnsupportedKernelFeature(feature) => {
                write!(formatter, "selected kernel does not support {feature}")
            }
        }
    }
}

impl std::error::Error for ProfileError {}

#[derive(Debug)]
pub enum WriteError {
    InvalidFileName,
    InvalidRuntimePath,
    RuntimeDirSymlink,
    RuntimeDirNotDirectory,
    FinalPathSymlink,
    FinalPathNotFile,
    Io(io::Error),
}

impl fmt::Display for WriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFileName => formatter.write_str("generated profile file name is invalid"),
            Self::InvalidRuntimePath => {
                formatter.write_str("profile runtime directory path is invalid")
            }
            Self::RuntimeDirSymlink => {
                formatter.write_str("profile runtime directory cannot be a symlink")
            }
            Self::RuntimeDirNotDirectory => {
                formatter.write_str("profile runtime path must be a directory")
            }
            Self::FinalPathSymlink => {
                formatter.write_str("generated profile path cannot be a symlink")
            }
            Self::FinalPathNotFile => {
                formatter.write_str("generated profile path must be a regular file")
            }
            Self::Io(source) => write!(formatter, "private profile write failed: {source}"),
        }
    }
}

impl std::error::Error for WriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(source) => Some(source),
            _ => None,
        }
    }
}
