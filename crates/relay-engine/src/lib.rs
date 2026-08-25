#![forbid(unsafe_code)]

use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io;
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_VALIDATION_TIMEOUT: Duration = Duration::from_secs(10);
const RELAY_ENV_VARS: [&str; 8] = [
    "RELAY_MIHOMO_BINARY",
    "RELAY_MIHOMO_CONFIG",
    "RELAY_MIHOMO_CONTROLLER",
    "RELAY_MIHOMO_DATA_DIR",
    "RELAY_MIHOMO_MIXED_PORT",
    "RELAY_MIHOMO_PREVIEW_BINARY",
    "RELAY_MIHOMO_SECRET",
    "RELAY_MIHOMO_SUBSCRIPTION_FILE",
];

/// A private controller address reserved for a Relay-managed Mihomo process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControllerEndpoint {
    /// A filesystem socket on macOS or Linux.
    UnixSocket(PathBuf),
    /// A loopback TCP listener reserved for a future enforceable-auth implementation.
    Tcp(SocketAddr),
    /// A Windows named pipe such as `\\.\pipe\relay-mihomo`.
    NamedPipe(String),
}

impl ControllerEndpoint {
    /// Returns the endpoint syntax consumed by the Relay controller client.
    #[must_use]
    pub fn uri(&self) -> String {
        match self {
            Self::UnixSocket(path) => format!("unix://{}", path.display()),
            Self::Tcp(address) => format!("http://{address}"),
            Self::NamedPipe(name) => format!("pipe://{name}"),
        }
    }

    fn validate(&self, data_dir: &Path) -> Result<(), EngineError> {
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
                return Err(EngineError::InvalidConfig(
                    "managed TCP is disabled until Relay can verify controller secret enforcement"
                        .to_owned(),
                ));
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

/// Paths and controller settings for one isolated Mihomo child process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedEngineConfig {
    binary: PathBuf,
    config_file: PathBuf,
    data_dir: PathBuf,
    controller: ControllerEndpoint,
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
            binary,
            config_file,
            data_dir,
            controller,
        }
    }

    /// Returns the controller endpoint produced after a successful start.
    #[must_use]
    pub const fn controller(&self) -> &ControllerEndpoint {
        &self.controller
    }

    /// Checks path, file, platform, and controller isolation constraints.
    ///
    /// # Errors
    ///
    /// Returns an error when a path is relative, a required file is absent,
    /// or the controller would escape the managed runtime boundary.
    pub fn validate(&self) -> Result<(), EngineError> {
        require_absolute(&self.binary, "Mihomo binary")?;
        require_absolute(&self.config_file, "Mihomo config")?;
        require_absolute_clean(&self.data_dir, "managed data directory")?;
        require_file(&self.binary, "Mihomo binary")?;
        require_file(&self.config_file, "Mihomo config")?;
        if let Ok(metadata) = fs::symlink_metadata(&self.data_dir)
            && (metadata.file_type().is_symlink() || !metadata.is_dir())
        {
            return Err(EngineError::InvalidConfig(
                "managed data directory must be a real directory, not a symlink".to_owned(),
            ));
        }
        self.controller.validate(&self.data_dir)
    }

    /// Builds the `mihomo -t` command used before every managed launch.
    #[must_use]
    pub fn validation_command(&self) -> CommandSpec {
        CommandSpec::new(
            self.binary.clone(),
            vec![
                OsString::from("-t"),
                OsString::from("-d"),
                self.data_dir.clone().into_os_string(),
                OsString::from("-f"),
                self.config_file.clone().into_os_string(),
            ],
            self.data_dir.clone(),
        )
    }

    /// Builds the isolated Mihomo launch command.
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

/// A fully resolved process command without shell interpolation.
#[derive(Clone, PartialEq, Eq)]
pub struct CommandSpec {
    program: PathBuf,
    args: Vec<OsString>,
    current_dir: PathBuf,
}

impl fmt::Debug for CommandSpec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut redact_next = false;
        let args = self
            .args
            .iter()
            .map(|argument| {
                if redact_next {
                    redact_next = false;
                    return "<redacted>".to_owned();
                }
                let argument = argument.to_string_lossy().into_owned();
                redact_next = argument == "-secret";
                argument
            })
            .collect::<Vec<_>>();
        formatter
            .debug_struct("CommandSpec")
            .field("program", &self.program)
            .field("args", &args)
            .field("current_dir", &self.current_dir)
            .finish()
    }
}

impl CommandSpec {
    fn new(program: PathBuf, args: Vec<OsString>, current_dir: PathBuf) -> Self {
        Self {
            program,
            args,
            current_dir,
        }
    }

    /// Returns the executable path.
    #[must_use]
    pub fn program(&self) -> &Path {
        &self.program
    }

    /// Returns arguments passed directly to the executable.
    #[must_use]
    pub fn args(&self) -> &[OsString] {
        &self.args
    }
}

/// Portable child exit information.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessExit {
    success: bool,
    code: Option<i32>,
}

impl ProcessExit {
    /// Returns a successful synthetic exit for test adapters.
    #[must_use]
    pub const fn success() -> Self {
        Self {
            success: true,
            code: Some(0),
        }
    }

    /// Returns an unsuccessful synthetic exit for test adapters.
    #[must_use]
    pub const fn failure() -> Self {
        Self {
            success: false,
            code: Some(1),
        }
    }

    /// Reports whether the child exited successfully.
    #[must_use]
    pub const fn is_success(self) -> bool {
        self.success
    }
}

impl From<ExitStatus> for ProcessExit {
    fn from(status: ExitStatus) -> Self {
        Self {
            success: status.success(),
            code: status.code(),
        }
    }
}

/// The only child-process capabilities the manager can own.
pub trait ManagedChild: Send {
    /// Returns the operating-system child identifier for display only.
    fn id(&self) -> u32;
    /// Checks whether this exact owned child has exited.
    ///
    /// # Errors
    ///
    /// Returns an operating-system error when child status cannot be queried.
    fn try_wait(&mut self) -> io::Result<Option<ProcessExit>>;
    /// Terminates and reaps this exact owned child.
    ///
    /// # Errors
    ///
    /// Returns an operating-system error when the child cannot be terminated or reaped.
    fn terminate(&mut self) -> io::Result<ProcessExit>;
}

/// Adapter used to validate and spawn a resolved command without a shell.
pub trait ProcessSpawner: Send {
    /// Runs the validation command to completion.
    ///
    /// # Errors
    ///
    /// Returns an operating-system error when the command cannot be run or awaited.
    fn validate(&mut self, spec: &CommandSpec, timeout: Duration) -> io::Result<ProcessExit>;
    /// Starts a child and transfers its ownership to the manager.
    ///
    /// # Errors
    ///
    /// Returns an operating-system error when the child cannot be spawned.
    fn spawn(&mut self, spec: &CommandSpec) -> io::Result<Box<dyn ManagedChild>>;
}

struct StdManagedChild(Child);

impl ManagedChild for StdManagedChild {
    fn id(&self) -> u32 {
        self.0.id()
    }

    fn try_wait(&mut self) -> io::Result<Option<ProcessExit>> {
        self.0.try_wait().map(|exit| exit.map(ProcessExit::from))
    }

    fn terminate(&mut self) -> io::Result<ProcessExit> {
        if let Some(exit) = self.0.try_wait()? {
            return Ok(exit.into());
        }
        self.0.kill()?;
        self.0.wait().map(ProcessExit::from)
    }
}

/// Standard-library process adapter used in production.
#[derive(Default)]
pub struct StdProcessSpawner;

impl ProcessSpawner for StdProcessSpawner {
    fn validate(&mut self, spec: &CommandSpec, timeout: Duration) -> io::Result<ProcessExit> {
        let mut child = resolved_command(spec).spawn()?;
        let started = Instant::now();
        loop {
            if let Some(exit) = child.try_wait()? {
                return Ok(exit.into());
            }
            let elapsed = started.elapsed();
            if elapsed >= timeout {
                child.kill()?;
                child.wait()?;
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Mihomo config validation timed out",
                ));
            }
            let remaining = timeout.saturating_sub(elapsed);
            thread::sleep(Duration::from_millis(10).min(remaining));
        }
    }

    fn spawn(&mut self, spec: &CommandSpec) -> io::Result<Box<dyn ManagedChild>> {
        resolved_command(spec)
            .spawn()
            .map(|child| Box::new(StdManagedChild(child)) as Box<dyn ManagedChild>)
    }
}

fn resolved_command(spec: &CommandSpec) -> Command {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(&spec.current_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for variable in RELAY_ENV_VARS {
        command.env_remove(variable);
    }
    command
}

/// Result of one controller readiness probe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeStatus {
    /// The controller accepted and answered a health request.
    Ready,
    /// The child may still be starting.
    Pending,
}

/// Readiness adapter, normally backed by a read-only Mihomo `/version` request.
pub trait ReadinessProbe: Send {
    /// Checks the controller without changing its configuration.
    fn check(&mut self, endpoint: &ControllerEndpoint) -> ProbeStatus;
}

/// Bounded readiness attempts and delay between them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadinessPolicy {
    attempts: usize,
    delay: Duration,
}

impl ReadinessPolicy {
    /// Creates a bounded readiness policy.
    ///
    /// # Errors
    ///
    /// Returns an error when `attempts` is zero.
    pub fn new(attempts: usize, delay: Duration) -> Result<Self, EngineError> {
        if attempts == 0 {
            return Err(EngineError::InvalidConfig(
                "readiness attempts must be greater than zero".to_owned(),
            ));
        }
        Ok(Self { attempts, delay })
    }
}

impl Default for ReadinessPolicy {
    fn default() -> Self {
        Self {
            attempts: 50,
            delay: Duration::from_millis(100),
        }
    }
}

/// Observable lifecycle state for a managed core.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineState {
    /// No child has been started.
    Idle,
    /// Runtime paths are being checked and created.
    Preparing,
    /// Mihomo is checking the supplied configuration with `-t`.
    Validating,
    /// The child exists but its controller is not ready yet.
    Starting,
    /// The controller answered and the owned child is running.
    Ready {
        /// Display-only child identifier. It is never used to terminate a process.
        pid: u32,
        /// Private controller endpoint for this child.
        endpoint: ControllerEndpoint,
    },
    /// The exact owned child is being terminated.
    Stopping,
    /// No owned child remains.
    Stopped,
    /// Start or stop failed; the message contains no subscription or API secret.
    Failed {
        /// Safe lifecycle diagnostic.
        message: String,
    },
}

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
    /// Mihomo rejected the runtime configuration during `-t`.
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
                write!(formatter, "Mihomo config validation failed ({exit})")
            }
            Self::AlreadyRunning => {
                formatter.write_str("a managed Mihomo child is already running")
            }
            Self::ExitedEarly(exit) => {
                write!(formatter, "managed Mihomo exited before readiness ({exit})")
            }
            Self::Exited(exit) => write!(formatter, "managed Mihomo exited ({exit})"),
            Self::ReadinessTimeout { attempts } => write!(
                formatter,
                "managed Mihomo controller was not ready after {attempts} attempts"
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
        match self.code {
            Some(code) => write!(formatter, "exit code {code}"),
            None => formatter.write_str("terminated by signal"),
        }
    }
}

/// Owns at most one child spawned from an isolated managed configuration.
pub struct EngineManager {
    config: ManagedEngineConfig,
    readiness: ReadinessPolicy,
    validation_timeout: Duration,
    state: EngineState,
    child: Option<Box<dyn ManagedChild>>,
    spawner: Box<dyn ProcessSpawner>,
    probe: Box<dyn ReadinessProbe>,
}

impl EngineManager {
    /// Creates a manager using the standard-library process adapter.
    #[must_use]
    pub fn new(
        config: ManagedEngineConfig,
        readiness: ReadinessPolicy,
        probe: Box<dyn ReadinessProbe>,
    ) -> Self {
        Self::with_adapters(
            config,
            readiness,
            Box::<StdProcessSpawner>::default(),
            probe,
        )
    }

    /// Creates a manager with explicit process and probe adapters.
    ///
    /// This is primarily useful for deterministic lifecycle tests.
    #[must_use]
    pub fn with_adapters(
        config: ManagedEngineConfig,
        readiness: ReadinessPolicy,
        spawner: Box<dyn ProcessSpawner>,
        probe: Box<dyn ReadinessProbe>,
    ) -> Self {
        Self {
            config,
            readiness,
            validation_timeout: DEFAULT_VALIDATION_TIMEOUT,
            state: EngineState::Idle,
            child: None,
            spawner,
            probe,
        }
    }

    /// Returns the current managed lifecycle state.
    #[must_use]
    pub const fn state(&self) -> &EngineState {
        &self.state
    }

    /// Overrides the bounded `mihomo -t` validation timeout.
    ///
    /// # Errors
    ///
    /// Returns an error when `timeout` is zero.
    pub fn with_validation_timeout(mut self, timeout: Duration) -> Result<Self, EngineError> {
        if timeout.is_zero() {
            return Err(EngineError::InvalidConfig(
                "validation timeout must be greater than zero".to_owned(),
            ));
        }
        self.validation_timeout = timeout;
        Ok(self)
    }

    /// Returns the endpoint only while the exact owned child is still running.
    ///
    /// A detected exit is reaped and changes the state to [`EngineState::Failed`], allowing the
    /// next call to [`Self::start`] to create a fresh child.
    ///
    /// # Errors
    ///
    /// Returns an error when the operating system cannot query the owned child or when the child
    /// has exited since its controller became ready.
    pub fn running_endpoint(&mut self) -> Result<Option<ControllerEndpoint>, EngineError> {
        let Some(child) = self.child.as_mut() else {
            return Ok(None);
        };
        let exit = child.try_wait().map_err(|source| EngineError::Io {
            operation: "poll managed Mihomo",
            source,
        })?;
        if let Some(exit) = exit {
            self.child = None;
            return self.fail(EngineError::Exited(exit));
        }
        match &self.state {
            EngineState::Ready { endpoint, .. } => Ok(Some(endpoint.clone())),
            _ => Ok(None),
        }
    }

    /// Validates, starts, and waits for the owned controller to become ready.
    ///
    /// # Errors
    ///
    /// Returns a structured error when validation, spawn, readiness, or cleanup fails.
    pub fn start(&mut self) -> Result<ControllerEndpoint, EngineError> {
        if self.child.is_some() {
            return self.fail(EngineError::AlreadyRunning);
        }
        self.state = EngineState::Preparing;
        if let Err(error) = self.config.validate() {
            return self.fail(error);
        }
        if let Err(error) = prepare_data_dir(&self.config.data_dir) {
            return self.fail(error);
        }

        self.state = EngineState::Validating;
        let validation = match self
            .spawner
            .validate(&self.config.validation_command(), self.validation_timeout)
        {
            Ok(exit) => exit,
            Err(source) => {
                return self.fail(EngineError::Io {
                    operation: "run Mihomo config validation",
                    source,
                });
            }
        };
        if !validation.is_success() {
            return self.fail(EngineError::ValidationFailed(validation));
        }

        self.state = EngineState::Starting;
        let child = match self.spawner.spawn(&self.config.launch_command()) {
            Ok(child) => child,
            Err(source) => {
                return self.fail(EngineError::Io {
                    operation: "spawn managed Mihomo",
                    source,
                });
            }
        };
        let pid = child.id();
        self.child = Some(child);

        for attempt in 0..self.readiness.attempts {
            let Some(child) = self.child.as_mut() else {
                return self.fail(EngineError::InvalidConfig(
                    "managed child ownership was lost during startup".to_owned(),
                ));
            };
            let exit = match child.try_wait() {
                Ok(exit) => exit,
                Err(source) => {
                    let error = EngineError::Io {
                        operation: "poll managed Mihomo",
                        source,
                    };
                    return self.fail_after_cleanup(error);
                }
            };
            if let Some(exit) = exit {
                self.child = None;
                return self.fail(EngineError::ExitedEarly(exit));
            }
            if self.probe.check(&self.config.controller) == ProbeStatus::Ready {
                let endpoint = self.config.controller.clone();
                self.state = EngineState::Ready {
                    pid,
                    endpoint: endpoint.clone(),
                };
                return Ok(endpoint);
            }
            if attempt + 1 < self.readiness.attempts && !self.readiness.delay.is_zero() {
                thread::sleep(self.readiness.delay);
            }
        }

        self.fail_after_cleanup(EngineError::ReadinessTimeout {
            attempts: self.readiness.attempts,
        })
    }

    /// Terminates and reaps only the exact child owned by this manager.
    ///
    /// Calling this method without an owned child is safe and idempotent.
    ///
    /// # Errors
    ///
    /// Returns an error when the operating system cannot terminate or reap the owned child.
    pub fn stop(&mut self) -> Result<(), EngineError> {
        let Some(mut child) = self.child.take() else {
            self.state = EngineState::Stopped;
            return Ok(());
        };
        self.state = EngineState::Stopping;
        match child.terminate() {
            Ok(_) => {
                self.state = EngineState::Stopped;
                Ok(())
            }
            Err(source) => {
                self.child = Some(child);
                self.fail(EngineError::Io {
                    operation: "terminate managed Mihomo",
                    source,
                })
            }
        }
    }

    fn fail<T>(&mut self, error: EngineError) -> Result<T, EngineError> {
        self.state = EngineState::Failed {
            message: error.to_string(),
        };
        Err(error)
    }

    fn fail_after_cleanup<T>(&mut self, error: EngineError) -> Result<T, EngineError> {
        if let Some(mut child) = self.child.take()
            && let Err(source) = child.terminate()
        {
            self.child = Some(child);
            return self.fail(EngineError::Io {
                operation: "clean up managed Mihomo after failed start",
                source,
            });
        }
        self.fail(error)
    }
}

impl Drop for EngineManager {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.terminate();
        }
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

fn prepare_data_dir(path: &Path) -> Result<(), EngineError> {
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

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    #[cfg(unix)]
    use std::time::Instant;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use super::{
        CommandSpec, ControllerEndpoint, EngineManager, EngineState, ManagedChild,
        ManagedEngineConfig, ProbeStatus, ProcessExit, ProcessSpawner, RELAY_ENV_VARS,
        ReadinessPolicy, ReadinessProbe, resolved_command,
    };

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    struct TempLayout {
        root: PathBuf,
        binary: PathBuf,
        config: PathBuf,
        data_dir: PathBuf,
    }

    impl TempLayout {
        fn new() -> Self {
            let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let root =
                std::env::temp_dir().join(format!("relay-engine-test-{}-{id}", std::process::id()));
            let binary = root.join("mihomo-fixture");
            let config = root.join("config.yaml");
            let data_dir = root.join("runtime");
            fs::create_dir_all(&root).expect("create test root");
            fs::write(&binary, b"fixture").expect("write fake binary");
            fs::write(&config, b"mixed-port: 0\n").expect("write fake config");
            Self {
                root,
                binary,
                config,
                data_dir,
            }
        }

        fn config(&self) -> ManagedEngineConfig {
            ManagedEngineConfig::new(
                self.binary.clone(),
                self.config.clone(),
                self.data_dir.clone(),
                ControllerEndpoint::UnixSocket(self.data_dir.join("controller.sock")),
            )
        }
    }

    impl Drop for TempLayout {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[derive(Default)]
    struct FakeState {
        validations: usize,
        spawns: usize,
        terminates: usize,
        validation_succeeds: bool,
        exits_early: bool,
        terminate_failures_remaining: usize,
    }

    struct FakeSpawner {
        state: Arc<Mutex<FakeState>>,
    }

    struct FakeChild {
        state: Arc<Mutex<FakeState>>,
    }

    impl ManagedChild for FakeChild {
        fn id(&self) -> u32 {
            4242
        }

        fn try_wait(&mut self) -> std::io::Result<Option<ProcessExit>> {
            let state = self.state.lock().expect("fake state");
            Ok(state.exits_early.then(ProcessExit::failure))
        }

        fn terminate(&mut self) -> std::io::Result<ProcessExit> {
            let mut state = self.state.lock().expect("fake state");
            state.terminates += 1;
            if state.terminate_failures_remaining > 0 {
                state.terminate_failures_remaining -= 1;
                return Err(std::io::Error::other("injected terminate failure"));
            }
            Ok(ProcessExit::success())
        }
    }

    impl ProcessSpawner for FakeSpawner {
        fn validate(
            &mut self,
            _spec: &CommandSpec,
            _timeout: Duration,
        ) -> std::io::Result<ProcessExit> {
            let mut state = self.state.lock().expect("fake state");
            state.validations += 1;
            Ok(if state.validation_succeeds {
                ProcessExit::success()
            } else {
                ProcessExit::failure()
            })
        }

        fn spawn(&mut self, _spec: &CommandSpec) -> std::io::Result<Box<dyn ManagedChild>> {
            self.state.lock().expect("fake state").spawns += 1;
            Ok(Box::new(FakeChild {
                state: Arc::clone(&self.state),
            }))
        }
    }

    struct ScriptedProbe {
        results: Vec<ProbeStatus>,
        cursor: usize,
    }

    impl ReadinessProbe for ScriptedProbe {
        fn check(&mut self, _endpoint: &ControllerEndpoint) -> ProbeStatus {
            let result = self
                .results
                .get(self.cursor)
                .copied()
                .unwrap_or(ProbeStatus::Pending);
            self.cursor += 1;
            result
        }
    }

    fn manager(
        config: ManagedEngineConfig,
        state: Arc<Mutex<FakeState>>,
        results: Vec<ProbeStatus>,
        attempts: usize,
    ) -> EngineManager {
        EngineManager::with_adapters(
            config,
            ReadinessPolicy::new(attempts, Duration::ZERO).expect("valid readiness policy"),
            Box::new(FakeSpawner { state }),
            Box::new(ScriptedProbe { results, cursor: 0 }),
        )
    }

    #[test]
    fn builds_validation_and_launch_commands_from_isolated_paths() {
        let layout = TempLayout::new();
        let config = layout.config();
        let validation = config.validation_command();
        let launch = config.launch_command();

        assert_eq!(validation.program(), layout.binary.as_path());
        assert_eq!(
            validation.args(),
            &[
                OsString::from("-t"),
                OsString::from("-d"),
                layout.data_dir.clone().into_os_string(),
                OsString::from("-f"),
                layout.config.clone().into_os_string(),
            ]
        );
        assert_eq!(
            launch.args(),
            &[
                OsString::from("-d"),
                layout.data_dir.clone().into_os_string(),
                OsString::from("-f"),
                layout.config.clone().into_os_string(),
                OsString::from("-ext-ctl-unix"),
                layout.data_dir.join("controller.sock").into_os_string(),
            ]
        );
    }

    #[test]
    fn child_commands_explicitly_remove_relay_environment() {
        let layout = TempLayout::new();
        let command = resolved_command(&layout.config().launch_command());
        let removed = command
            .get_envs()
            .filter_map(|(name, value)| value.is_none().then_some(name))
            .collect::<Vec<_>>();

        for variable in RELAY_ENV_VARS {
            assert!(removed.contains(&std::ffi::OsStr::new(variable)));
        }
    }

    #[test]
    fn rejects_relative_and_non_loopback_runtime_boundaries() {
        let layout = TempLayout::new();
        let relative = ManagedEngineConfig::new(
            PathBuf::from("mihomo"),
            layout.config.clone(),
            layout.data_dir.clone(),
            ControllerEndpoint::UnixSocket(layout.data_dir.join("controller.sock")),
        );
        assert!(relative.validate().is_err());

        let remote = ManagedEngineConfig::new(
            layout.binary.clone(),
            layout.config.clone(),
            layout.data_dir.clone(),
            ControllerEndpoint::Tcp("192.0.2.10:9090".parse().expect("socket address")),
        );
        assert!(remote.validate().is_err());

        let loopback = ManagedEngineConfig::new(
            layout.binary.clone(),
            layout.config.clone(),
            layout.data_dir.clone(),
            ControllerEndpoint::Tcp("127.0.0.1:19090".parse().expect("socket address")),
        );
        assert!(loopback.validate().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn start_creates_a_private_runtime_directory() {
        let layout = TempLayout::new();
        let state = Arc::new(Mutex::new(FakeState {
            validation_succeeds: true,
            ..FakeState::default()
        }));
        let mut manager = manager(layout.config(), state, vec![ProbeStatus::Ready], 1);

        manager.start().expect("engine becomes ready");

        let mode = fs::metadata(&layout.data_dir)
            .expect("runtime metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);
    }

    #[cfg(unix)]
    #[test]
    fn start_rejects_a_controller_nested_below_the_private_runtime() {
        let layout = TempLayout::new();
        let data_dir = layout.root.join("private-runtime");
        let config = ManagedEngineConfig::new(
            layout.binary.clone(),
            layout.config.clone(),
            data_dir.clone(),
            ControllerEndpoint::UnixSocket(data_dir.join("nested/controller.sock")),
        );
        let state = Arc::new(Mutex::new(FakeState {
            validation_succeeds: true,
            ..FakeState::default()
        }));
        let mut manager = manager(config, state, vec![ProbeStatus::Ready], 1);

        assert!(manager.start().is_err());
    }

    #[test]
    fn starts_only_after_validation_and_readiness() {
        let layout = TempLayout::new();
        let state = Arc::new(Mutex::new(FakeState {
            validation_succeeds: true,
            ..FakeState::default()
        }));
        let mut manager = manager(
            layout.config(),
            Arc::clone(&state),
            vec![ProbeStatus::Pending, ProbeStatus::Ready],
            3,
        );

        let endpoint = manager.start().expect("engine becomes ready");

        assert_eq!(endpoint, layout.config().controller().clone());
        assert!(matches!(
            manager.state(),
            EngineState::Ready { pid: 4242, .. }
        ));
        let state = state.lock().expect("fake state");
        assert_eq!(state.validations, 1);
        assert_eq!(state.spawns, 1);
        assert_eq!(state.terminates, 0);
    }

    #[test]
    fn running_endpoint_detects_a_child_that_crashed_after_readiness() {
        let layout = TempLayout::new();
        let state = Arc::new(Mutex::new(FakeState {
            validation_succeeds: true,
            ..FakeState::default()
        }));
        let mut manager = manager(
            layout.config(),
            Arc::clone(&state),
            vec![ProbeStatus::Ready, ProbeStatus::Ready],
            1,
        );
        manager.start().expect("engine becomes ready");
        state.lock().expect("fake state").exits_early = true;

        assert!(manager.running_endpoint().is_err());
        assert!(matches!(manager.state(), EngineState::Failed { .. }));

        state.lock().expect("fake state").exits_early = false;
        assert!(manager.start().is_ok());
    }

    #[test]
    fn timeout_terminates_only_the_owned_child() {
        let layout = TempLayout::new();
        let state = Arc::new(Mutex::new(FakeState {
            validation_succeeds: true,
            ..FakeState::default()
        }));
        let mut manager = manager(
            layout.config(),
            Arc::clone(&state),
            vec![ProbeStatus::Pending, ProbeStatus::Pending],
            2,
        );

        assert!(manager.start().is_err());
        assert!(matches!(manager.state(), EngineState::Failed { .. }));
        assert_eq!(state.lock().expect("fake state").terminates, 1);
    }

    #[test]
    fn early_exit_is_reported_without_terminating_an_unowned_pid() {
        let layout = TempLayout::new();
        let state = Arc::new(Mutex::new(FakeState {
            validation_succeeds: true,
            exits_early: true,
            ..FakeState::default()
        }));
        let mut manager = manager(
            layout.config(),
            Arc::clone(&state),
            vec![ProbeStatus::Pending],
            1,
        );

        assert!(manager.start().is_err());
        assert_eq!(state.lock().expect("fake state").terminates, 0);
    }

    #[test]
    fn stop_is_idempotent_and_drop_cleans_up_a_ready_child() {
        let layout = TempLayout::new();
        let state = Arc::new(Mutex::new(FakeState {
            validation_succeeds: true,
            ..FakeState::default()
        }));
        {
            let mut manager = manager(
                layout.config(),
                Arc::clone(&state),
                vec![ProbeStatus::Ready],
                1,
            );
            manager.start().expect("engine becomes ready");
            manager.stop().expect("first stop");
            manager.stop().expect("second stop");
            assert_eq!(*manager.state(), EngineState::Stopped);
        }
        assert_eq!(state.lock().expect("fake state").terminates, 1);

        {
            let mut manager = manager(
                layout.config(),
                Arc::clone(&state),
                vec![ProbeStatus::Ready],
                1,
            );
            manager.start().expect("engine becomes ready");
        }
        assert_eq!(state.lock().expect("fake state").terminates, 2);
    }

    #[test]
    fn failed_stop_retains_the_owned_child_for_drop_cleanup() {
        let layout = TempLayout::new();
        let state = Arc::new(Mutex::new(FakeState {
            validation_succeeds: true,
            terminate_failures_remaining: 1,
            ..FakeState::default()
        }));
        {
            let mut manager = manager(
                layout.config(),
                Arc::clone(&state),
                vec![ProbeStatus::Ready],
                1,
            );
            manager.start().expect("engine becomes ready");
            assert!(manager.stop().is_err());
            assert_eq!(state.lock().expect("fake state").terminates, 1);
        }
        assert_eq!(state.lock().expect("fake state").terminates, 2);
    }

    #[cfg(unix)]
    #[test]
    fn standard_adapter_runs_validation_and_owns_a_real_fixture_process() {
        let layout = TempLayout::new();
        fs::write(
            &layout.binary,
            b"#!/bin/sh\nif [ \"$1\" = \"-t\" ]; then exit 0; fi\nwhile :; do sleep 60; done\n",
        )
        .expect("write executable fixture");
        let mut permissions = fs::metadata(&layout.binary)
            .expect("fixture metadata")
            .permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&layout.binary, permissions).expect("make fixture executable");

        let mut manager = EngineManager::new(
            layout.config(),
            ReadinessPolicy::new(1, Duration::ZERO).expect("readiness policy"),
            Box::new(ScriptedProbe {
                results: vec![ProbeStatus::Ready],
                cursor: 0,
            }),
        );

        manager.start().expect("fixture process starts");
        assert!(matches!(manager.state(), EngineState::Ready { .. }));
        manager.stop().expect("fixture process stops");
        assert_eq!(*manager.state(), EngineState::Stopped);
    }

    #[cfg(unix)]
    #[test]
    fn standard_validation_times_out_and_reaps_a_hung_fixture() {
        let layout = TempLayout::new();
        fs::write(
            &layout.binary,
            b"#!/bin/sh\nif [ \"$1\" = \"-t\" ]; then while :; do :; done; fi\nexit 0\n",
        )
        .expect("write executable fixture");
        let mut permissions = fs::metadata(&layout.binary)
            .expect("fixture metadata")
            .permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&layout.binary, permissions).expect("make fixture executable");
        let mut manager = EngineManager::new(
            layout.config(),
            ReadinessPolicy::default(),
            Box::new(ScriptedProbe {
                results: vec![ProbeStatus::Ready],
                cursor: 0,
            }),
        )
        .with_validation_timeout(Duration::from_millis(50))
        .expect("validation timeout");

        let started = Instant::now();
        let error = manager.start().expect_err("validation must time out");

        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(matches!(
            error,
            super::EngineError::Io { source, .. }
                if source.kind() == std::io::ErrorKind::TimedOut
        ));
    }
}
