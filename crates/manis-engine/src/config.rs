use std::{
    ffi::OsString,
    fs, io,
    net::SocketAddr,
    path::{Component, Path, PathBuf},
    time::Duration,
};

use manis_core::KernelKind;

use crate::{CommandSpec, EngineError, ProcessSpawner, StdProcessSpawner};

const DEFAULT_VALIDATION_TIMEOUT: Duration = Duration::from_secs(10);

/// A private controller address reserved for a Manis-managed proxy core.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControllerEndpoint {
    /// A filesystem socket on macOS or Linux.
    UnixSocket(PathBuf),
    /// A loopback TCP listener reserved for a future enforceable-auth implementation.
    Tcp(SocketAddr),
    /// A Windows named pipe such as `\\.\pipe\manis-mihomo`.
    NamedPipe(String),
}

impl ControllerEndpoint {
    /// Returns the endpoint syntax consumed by the Manis controller client.
    #[must_use]
    pub fn uri(&self) -> String {
        match self {
            Self::UnixSocket(path) => format!("unix://{}", path.display()),
            Self::Tcp(address) => format!("http://{address}"),
            Self::NamedPipe(name) => format!("pipe://{name}"),
        }
    }

    fn validate(
        &self,
        data_dir: &Path,
        kernel: KernelKind,
        controller_secret_configured: bool,
    ) -> Result<(), EngineError> {
        match self {
            Self::UnixSocket(path) => {
                if !cfg!(unix) {
                    return Err(EngineError::InvalidConfig(
                        "Unix controller sockets are not supported on this platform".to_owned(),
                    ));
                }
                require_absolute_clean(path, "controller socket")?;
                if path.parent() != Some(data_dir) {
                    return Err(EngineError::InvalidConfig(
                        "controller socket must be a direct child of the managed data directory"
                            .to_owned(),
                    ));
                }
            }
            Self::Tcp(address) => {
                if !address.ip().is_loopback() {
                    return Err(EngineError::InvalidConfig(
                        "managed TCP controller must use a loopback address".to_owned(),
                    ));
                }
                if kernel == KernelKind::Mihomo || controller_secret_configured {
                    return Err(EngineError::InvalidConfig(
                        "managed TCP is not supported for the Mihomo controller".to_owned(),
                    ));
                }
            }
            Self::NamedPipe(name) => {
                if !cfg!(windows) {
                    return Err(EngineError::InvalidConfig(
                        "Windows controller pipes are not supported on this platform".to_owned(),
                    ));
                }
                if !name.starts_with(r"\\.\pipe\") || name.chars().any(char::is_control) {
                    return Err(EngineError::InvalidConfig(
                        "managed controller pipe must use the \\\\.\\pipe\\ namespace".to_owned(),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Paths and controller settings for one isolated proxy-core child process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedEngineConfig {
    kernel: KernelKind,
    binary: PathBuf,
    config_file: PathBuf,
    data_dir: PathBuf,
    controller: ControllerEndpoint,
    controller_secret_configured: bool,
}

impl ManagedEngineConfig {
    /// Creates a managed configuration.
    #[must_use]
    pub fn new(
        binary: PathBuf,
        config_file: PathBuf,
        data_dir: PathBuf,
        controller: ControllerEndpoint,
    ) -> Self {
        Self {
            kernel: KernelKind::Mihomo,
            binary,
            config_file,
            data_dir,
            controller,
            controller_secret_configured: false,
        }
    }

    /// Returns the kernel whose command line this configuration builds.
    #[must_use]
    pub const fn kernel(&self) -> KernelKind {
        self.kernel
    }

    /// Returns the controller endpoint produced after a successful start.
    #[must_use]
    pub const fn controller(&self) -> &ControllerEndpoint {
        &self.controller
    }

    pub(crate) fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Checks path, file, platform, and controller isolation constraints.
    ///
    /// # Errors
    ///
    /// Returns an error when a path is relative, a required file is absent,
    /// or the controller would escape the managed runtime boundary.
    pub fn validate(&self) -> Result<(), EngineError> {
        require_absolute(&self.binary, "kernel binary")?;
        require_absolute(&self.config_file, "kernel config")?;
        require_absolute_clean(&self.data_dir, "managed data directory")?;
        require_file(&self.binary, "kernel binary")?;
        require_file(&self.config_file, "kernel config")?;
        if let Ok(metadata) = fs::symlink_metadata(&self.data_dir)
            && (metadata.file_type().is_symlink() || !metadata.is_dir())
        {
            return Err(EngineError::InvalidConfig(
                "managed data directory must be a real directory, not a symlink".to_owned(),
            ));
        }
        self.controller.validate(
            &self.data_dir,
            self.kernel,
            self.controller_secret_configured,
        )
    }

    /// Builds the kernel-specific configuration validation command.
    #[must_use]
    pub fn validation_command(&self) -> CommandSpec {
        let args = vec![
            OsString::from("-t"),
            OsString::from("-d"),
            self.data_dir.clone().into_os_string(),
            OsString::from("-f"),
            self.config_file.clone().into_os_string(),
        ];
        CommandSpec::new(self.binary.clone(), args, self.data_dir.clone())
    }

    /// Builds the kernel-specific isolated launch command.
    #[must_use]
    pub fn launch_command(&self) -> CommandSpec {
        let mut args = vec![
            OsString::from("-d"),
            self.data_dir.clone().into_os_string(),
            OsString::from("-f"),
            self.config_file.clone().into_os_string(),
        ];
        match &self.controller {
            ControllerEndpoint::UnixSocket(path) => {
                args.push(OsString::from("-ext-ctl-unix"));
                args.push(path.clone().into_os_string());
            }
            ControllerEndpoint::Tcp(address) => {
                args.push(OsString::from("-ext-ctl"));
                args.push(OsString::from(address.to_string()));
            }
            ControllerEndpoint::NamedPipe(name) => {
                args.push(OsString::from("-ext-ctl-pipe"));
                args.push(OsString::from(name));
            }
        }
        CommandSpec::new(self.binary.clone(), args, self.data_dir.clone())
    }
}

/// Validates a candidate Manis-managed kernel configuration without starting a child process.
///
/// This is used before replacing a running generated profile, so a rejected candidate cannot
/// force the currently owned core offline.
///
/// # Errors
/// Returns a structured, secret-free lifecycle error when path checks or validation fail.
pub fn validate_managed_config(config: &ManagedEngineConfig) -> Result<(), EngineError> {
    config.validate()?;
    prepare_data_dir(&config.data_dir)?;
    let mut spawner = StdProcessSpawner;
    let exit = spawner
        .validate(&config.validation_command(), DEFAULT_VALIDATION_TIMEOUT)
        .map_err(|source| EngineError::Io {
            operation: "run kernel config validation",
            source,
        })?;
    if exit.is_success() {
        Ok(())
    } else {
        Err(EngineError::ValidationFailed(exit))
    }
}

fn require_absolute(path: &Path, label: &str) -> Result<(), EngineError> {
    if path.is_absolute() {
        Ok(())
    } else {
        Err(EngineError::InvalidConfig(format!(
            "{label} path must be absolute"
        )))
    }
}

fn require_absolute_clean(path: &Path, label: &str) -> Result<(), EngineError> {
    require_absolute(path, label)?;
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Err(EngineError::InvalidConfig(format!(
            "{label} path must not contain . or .. components"
        )));
    }
    Ok(())
}

fn require_file(path: &Path, label: &str) -> Result<(), EngineError> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err(EngineError::InvalidConfig(format!(
            "{label} must be a regular file"
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Err(EngineError::InvalidConfig(
            format!("{label} does not exist"),
        )),
        Err(source) => Err(EngineError::Io {
            operation: "inspect managed engine path",
            source,
        }),
    }
}

pub(crate) fn prepare_data_dir(path: &Path) -> Result<(), EngineError> {
    fs::create_dir_all(path).map_err(|source| EngineError::Io {
        operation: "create managed data directory",
        source,
    })?;
    let metadata = fs::symlink_metadata(path).map_err(|source| EngineError::Io {
        operation: "inspect managed data directory",
        source,
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(EngineError::InvalidConfig(
            "managed data directory must be a real directory, not a symlink".to_owned(),
        ));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = metadata.permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(path, permissions).map_err(|source| EngineError::Io {
            operation: "harden managed data directory permissions",
            source,
        })?;
    }

    Ok(())
}
