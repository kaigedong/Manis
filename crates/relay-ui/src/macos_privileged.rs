#![cfg(target_os = "macos")]

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use relay_engine::{CommandSpec, ManagedChild, ProcessExit, ProcessSpawner, StdProcessSpawner};

const HELPER_CONTROL_NAME: &str = "relay-helperctl";
const HELPER_PROTOCOL_VERSION: &str = "v2";

/// Process adapter backed by Relay's signed, root launch daemon.
///
/// The adapter never forwards a program path or an arbitrary argument vector to the daemon. It
/// accepts only the exact Mihomo command shape produced by `ManagedEngineConfig` and maps that to
/// the helper's typed `start` operation.
pub(crate) struct MacosPrivilegedProcessSpawner {
    control: PathBuf,
}

impl MacosPrivilegedProcessSpawner {
    pub(crate) fn prepare() -> io::Result<Self> {
        let control = helper_control_path()?;
        let status = run_control(&control, [OsStr::new("status")])?;
        if status.status.success() && is_current_status(&status.stdout) {
            return Ok(Self { control });
        }

        // A registered helper can be outdated or wedged and therefore fail to answer `status`.
        // `reinstall` handles both registered and not-yet-registered services, waiting for an old
        // daemon to be fully reaped before registering the bundled version.
        let registration = run_control(&control, [OsStr::new("reinstall")])?;
        if !registration.status.success() {
            return Err(control_error("register privileged helper", &registration));
        }
        let status = run_control(&control, [OsStr::new("status")])?;
        if !status.status.success() || !is_current_status(&status.stdout) {
            return Err(control_error("connect to privileged helper", &status));
        }
        Ok(Self { control })
    }

    fn parse_launch(spec: &CommandSpec) -> io::Result<LaunchRequest<'_>> {
        let args = spec.args();
        if args.len() != 6 || args[0] != "-d" || args[2] != "-f" || args[4] != "-ext-ctl-unix" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "privileged helper rejected an unexpected Mihomo command shape",
            ));
        }
        let data_dir = Path::new(&args[1]);
        let config = Path::new(&args[3]);
        let controller = Path::new(&args[5]);
        if data_dir != spec.current_dir()
            || config.parent() != Some(data_dir)
            || controller.parent() != Some(data_dir)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "privileged helper requires config and working directory to share one boundary",
            ));
        }
        Ok(LaunchRequest {
            data_dir,
            config,
            controller,
        })
    }
}

impl ProcessSpawner for MacosPrivilegedProcessSpawner {
    fn validate(&mut self, spec: &CommandSpec, timeout: Duration) -> io::Result<ProcessExit> {
        StdProcessSpawner.validate(spec, timeout)
    }

    fn spawn(&mut self, spec: &CommandSpec) -> io::Result<Box<dyn ManagedChild>> {
        let request = Self::parse_launch(spec)?;
        let output = run_control(
            &self.control,
            [
                OsStr::new("start"),
                OsStr::new("--data-dir"),
                request.data_dir.as_os_str(),
                OsStr::new("--config"),
                request.config.as_os_str(),
                OsStr::new("--controller"),
                request.controller.as_os_str(),
            ],
        )?;
        if !output.status.success() {
            return Err(control_error("start privileged Mihomo", &output));
        }
        let pid = parse_pid(&output.stdout, "started")?;
        Ok(Box::new(PrivilegedManagedChild {
            control: self.control.clone(),
            pid,
        }))
    }
}

struct LaunchRequest<'a> {
    data_dir: &'a Path,
    config: &'a Path,
    controller: &'a Path,
}

struct PrivilegedManagedChild {
    control: PathBuf,
    pid: u32,
}

impl ManagedChild for PrivilegedManagedChild {
    fn id(&self) -> u32 {
        self.pid
    }

    fn try_wait(&mut self) -> io::Result<Option<ProcessExit>> {
        let output = run_control(&self.control, [OsStr::new("status")])?;
        if !output.status.success() {
            return Err(control_error("query privileged Mihomo", &output));
        }
        let status = String::from_utf8_lossy(&output.stdout);
        let status = status.trim();
        if status == format!("stopped {HELPER_PROTOCOL_VERSION}") {
            return Ok(Some(ProcessExit::failure()));
        }
        let running_pid = parse_versioned_pid(&output.stdout, "running")?;
        if running_pid != self.pid {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "privileged helper no longer owns the expected Mihomo process",
            ));
        }
        Ok(None)
    }

    fn terminate(&mut self) -> io::Result<ProcessExit> {
        let output = run_control(&self.control, [OsStr::new("stop")])?;
        if !output.status.success() {
            return Err(control_error("stop privileged Mihomo", &output));
        }
        Ok(ProcessExit::success())
    }
}

fn helper_control_path() -> io::Result<PathBuf> {
    let executable = std::env::current_exe()?;
    let directory = executable.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Relay executable has no containing directory",
        )
    })?;
    let control = directory.join(HELPER_CONTROL_NAME);
    if !control.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "TUN requires the signed Relay.app build with its privileged helper",
        ));
    }
    Ok(control)
}

fn run_control<'a>(
    control: &Path,
    args: impl IntoIterator<Item = &'a OsStr>,
) -> io::Result<Output> {
    Command::new(control)
        .args(args)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
}

fn parse_pid(bytes: &[u8], prefix: &str) -> io::Result<u32> {
    let output = String::from_utf8_lossy(bytes);
    let mut fields = output.split_whitespace();
    if fields.next() != Some(prefix) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "privileged helper returned an unexpected response",
        ));
    }
    fields
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|pid| *pid > 0 && fields.next().is_none())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "privileged helper returned an invalid process identifier",
            )
        })
}

fn parse_versioned_pid(bytes: &[u8], prefix: &str) -> io::Result<u32> {
    let output = String::from_utf8_lossy(bytes);
    let mut fields = output.split_whitespace();
    if fields.next() != Some(prefix) {
        return Err(invalid_helper_response());
    }
    let pid = fields
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|pid| *pid > 0)
        .ok_or_else(invalid_helper_response)?;
    if fields.next() != Some(HELPER_PROTOCOL_VERSION) || fields.next().is_some() {
        return Err(invalid_helper_response());
    }
    Ok(pid)
}

fn is_current_status(bytes: &[u8]) -> bool {
    let status = String::from_utf8_lossy(bytes);
    status.trim() == format!("stopped {HELPER_PROTOCOL_VERSION}")
        || parse_versioned_pid(bytes, "running").is_ok()
}

fn invalid_helper_response() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "privileged helper returned an unexpected response",
    )
}

fn control_error(operation: &str, output: &Output) -> io::Error {
    let message = String::from_utf8_lossy(&output.stderr);
    let message = message.trim();
    io::Error::other(if message.is_empty() {
        format!("{operation} failed")
    } else {
        format!("{operation} failed: {message}")
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use relay_engine::{ControllerEndpoint, ManagedEngineConfig};

    use super::{MacosPrivilegedProcessSpawner, is_current_status, parse_pid, parse_versioned_pid};

    #[test]
    fn parses_typed_helper_pid_response() {
        assert_eq!(parse_pid(b"started 42\n", "started").unwrap(), 42);
        assert!(parse_pid(b"running 0\n", "running").is_err());
        assert!(parse_pid(b"started 42 extra\n", "started").is_err());
        assert!(parse_pid(b"shell 42\n", "started").is_err());
    }

    #[test]
    fn rejects_outdated_helper_status_and_accepts_current_status() {
        assert!(!is_current_status(b"stopped\n"));
        assert!(!is_current_status(b"running 42\n"));
        assert!(is_current_status(b"stopped v2\n"));
        assert!(is_current_status(b"running 42 v2\n"));
        assert_eq!(
            parse_versioned_pid(b"running 42 v2\n", "running").unwrap(),
            42
        );
    }

    #[test]
    fn accepts_only_the_relay_mihomo_launch_shape() {
        let root = PathBuf::from("/Users/example/Library/Application Support/Relay/mihomo");
        let config = root.join("relay-generated.yaml");
        let controller = root.join("controller.sock");
        let launch = ManagedEngineConfig::new(
            PathBuf::from("/Applications/Relay.app/Contents/Resources/mihomo/mihomo"),
            config.clone(),
            root.clone(),
            ControllerEndpoint::UnixSocket(controller.clone()),
        )
        .launch_command();

        let request = MacosPrivilegedProcessSpawner::parse_launch(&launch).unwrap();

        assert_eq!(request.data_dir, root.as_path());
        assert_eq!(request.config, config.as_path());
        assert_eq!(request.controller, controller.as_path());
    }

    #[test]
    fn rejects_non_relay_mihomo_launch_shapes() {
        let root = PathBuf::from("/Users/example/Library/Application Support/Relay/mihomo");
        let launch = ManagedEngineConfig::new(
            PathBuf::from("/Applications/Relay.app/Contents/Resources/mihomo/mihomo"),
            root.join("relay-generated.yaml"),
            root.clone(),
            ControllerEndpoint::Tcp("127.0.0.1:9090".parse().unwrap()),
        )
        .launch_command();

        assert!(MacosPrivilegedProcessSpawner::parse_launch(&launch).is_err());
    }

    #[test]
    fn rejects_controller_paths_outside_the_runtime() {
        let root = PathBuf::from("/Users/example/Library/Application Support/Relay/mihomo");
        let launch = ManagedEngineConfig::new(
            PathBuf::from("/Applications/Relay.app/Contents/Resources/mihomo/mihomo"),
            root.join("relay-generated.yaml"),
            root.clone(),
            ControllerEndpoint::UnixSocket(root.join("nested/controller.sock")),
        )
        .launch_command();

        assert!(MacosPrivilegedProcessSpawner::parse_launch(&launch).is_err());
    }
}
