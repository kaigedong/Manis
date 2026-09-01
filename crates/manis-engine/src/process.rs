use std::{
    fs::OpenOptions,
    io,
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::CommandSpec;

const MANAGED_CORE_LOG_FILE: &str = "manis-core.log";
#[cfg(unix)]
pub(crate) const CHILD_ENV_ALLOWLIST: [&str; 6] = [
    "HOME",
    "TMPDIR",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "LANG",
    "LC_ALL",
];
#[cfg(windows)]
pub(crate) const CHILD_ENV_ALLOWLIST: [&str; 6] = [
    "SystemRoot",
    "WINDIR",
    "TEMP",
    "TMP",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
];
#[cfg(not(any(unix, windows)))]
pub(crate) const CHILD_ENV_ALLOWLIST: [&str; 0] = [];

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

    /// Creates synthetic exit information returned by an out-of-process supervisor.
    #[must_use]
    pub const fn from_code(code: i32) -> Self {
        Self {
            success: code == 0,
            code: Some(code),
        }
    }

    /// Reports whether the child exited successfully.
    #[must_use]
    pub const fn is_success(self) -> bool {
        self.success
    }

    pub(crate) const fn code(self) -> Option<i32> {
        self.code
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
                    "kernel config validation timed out",
                ));
            }
            let remaining = timeout.saturating_sub(elapsed);
            thread::sleep(Duration::from_millis(10).min(remaining));
        }
    }

    fn spawn(&mut self, spec: &CommandSpec) -> io::Result<Box<dyn ManagedChild>> {
        resolved_launch_command(spec)?
            .spawn()
            .map(|child| Box::new(StdManagedChild(child)) as Box<dyn ManagedChild>)
    }
}

fn resolved_launch_command(spec: &CommandSpec) -> io::Result<Command> {
    let log_path = spec.current_dir().join(MANAGED_CORE_LOG_FILE);
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let log = options.open(log_path)?;
    let stdout = log.try_clone()?;
    let mut command = resolved_command(spec);
    command.stdout(Stdio::from(stdout)).stderr(Stdio::from(log));
    Ok(command)
}

pub(crate) fn resolved_command(spec: &CommandSpec) -> Command {
    let mut command = Command::new(spec.program());
    command.env_clear();
    for variable in CHILD_ENV_ALLOWLIST {
        if let Some(value) = std::env::var_os(variable) {
            command.env(variable, value);
        }
    }
    command
        .args(spec.args())
        .current_dir(spec.current_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}
