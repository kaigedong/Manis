#![cfg(target_os = "macos")]

use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use manis_engine::{CommandSpec, ManagedChild, ProcessExit, ProcessSpawner, StdProcessSpawner};

use crate::diagnostics::{LogLevel, record_event};

mod reclaim;

const HELPER_CONTROL_NAME: &str = "manis-helperctl";
const HELPER_PROTOCOL_VERSION: &str = "v8";
const HELPER_REGISTRATION_ATTEMPTS: usize = 2;
const LOCAL_INSTALLER_FAILURE_EXIT: i32 = 2;
const HELPER_READY_ATTEMPTS: usize = 6;
const HELPER_READY_DELAY: Duration = Duration::from_millis(450);
const ROUTE_COMMAND: &str = "/sbin/route";
const TUN_ROUTE_RELEASE_ATTEMPTS: usize = 10;
const TUN_ROUTE_RELEASE_DELAY: Duration = Duration::from_millis(50);

/// Process adapter backed by Manis's signed, root launch daemon.
///
/// The adapter never forwards a program path or an arbitrary argument vector to the daemon. It
/// accepts only the exact Mihomo command shape produced by `ManagedEngineConfig` and maps that to
/// the helper's typed `start` operation.
pub(crate) struct MacosPrivilegedProcessSpawner {
    control: PathBuf,
}

impl MacosPrivilegedProcessSpawner {
    pub(crate) fn recover_if_available() -> io::Result<Option<Self>> {
        let control = match helper_control_path() {
            Ok(control) => control,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let status = run_control(&control, [OsStr::new("status")])?;
        if !status.status.success() || !is_current_status(&status.stdout) {
            return Ok(None);
        }
        if let HelperStatus::Running { pid } = parse_helper_status(&status.stdout)? {
            let stopped = run_stop_control(&control, pid)?;
            if !stopped.status.success() {
                return Err(control_error("stop recovered privileged Mihomo", &stopped));
            }
            record_event(
                LogLevel::Info,
                "helper.recovery.stopped_previous_core",
                format!("pid={pid}"),
            );
        }
        record_event(
            LogLevel::Info,
            "helper.recovery.available",
            "current helper will own the managed Mihomo restart",
        );
        Ok(Some(Self { control }))
    }

    pub(crate) fn prepare() -> io::Result<Self> {
        let control = helper_control_path()?;
        let status = run_control(&control, [OsStr::new("status")])?;
        if status.status.success() && is_current_status(&status.stdout) {
            stage_managed_core(&control)?;
            record_event(
                LogLevel::Info,
                "helper.prepare.succeeded",
                helper_status_detail(&status.stdout),
            );
            return Ok(Self { control });
        }

        record_event(
            LogLevel::Warn,
            "helper.prepare.reinstall_requested",
            control_error("query privileged helper", &status).to_string(),
        );

        // macOS can return from SMAppService registration before the daemon is reachable, and can
        // briefly reject a new registration while approval state is settling. Keep the whole
        // transition inside one user action instead of requiring repeated TUN clicks.
        let mut last_error = control_error("query privileged helper", &status);
        for registration_attempt in 1..=HELPER_REGISTRATION_ATTEMPTS {
            let registration = run_control(&control, [OsStr::new("reinstall")])?;
            if registration.status.success() {
                record_event(
                    LogLevel::Info,
                    "helper.prepare.registration_accepted",
                    format!("attempt={registration_attempt}"),
                );
            } else {
                last_error = control_error("register privileged helper", &registration);
                record_event(
                    LogLevel::Warn,
                    "helper.prepare.registration_deferred",
                    format!("attempt={registration_attempt} error={last_error}"),
                );
                if is_terminal_registration_failure(registration.status.code()) {
                    return Err(last_error);
                }
            }

            match wait_for_current_helper(&control) {
                Ok(status) => {
                    stage_managed_core(&control)?;
                    record_event(
                        LogLevel::Info,
                        "helper.prepare.reinstall_succeeded",
                        format!(
                            "registration_attempt={registration_attempt} {}",
                            helper_status_detail(&status.stdout)
                        ),
                    );
                    return Ok(Self { control });
                }
                Err(error) => last_error = error,
            }
        }
        Err(last_error)
    }

    /// Synchronizes the user-owned, digest-verified core into the fixed root-owned TUN location.
    ///
    /// An unavailable or outdated helper is not an update failure: the next TUN enable action will
    /// register the current helper and stage the same Manis-managed core before it starts.
    pub(crate) fn sync_managed_core_if_available() -> io::Result<bool> {
        let control = match helper_control_path() {
            Ok(control) => control,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        let status = run_control(&control, [OsStr::new("status")])?;
        if !status.status.success() || !is_current_status(&status.stdout) {
            return Ok(false);
        }
        stage_managed_core(&control)?;
        Ok(true)
    }

    /// Stops an unprivileged Manis core left behind by an earlier UI process.
    ///
    /// The controller socket identifies the candidate process, but is not sufficient proof of
    /// ownership on its own. The executable identity must also match the Manis-managed runtime
    /// before the process is terminated; flat argv text is not used as an authorization boundary.
    pub(crate) fn reclaim_stale_ordinary(spec: &CommandSpec) -> io::Result<()> {
        let request = Self::parse_launch(spec)?;
        reclaim::reclaim_stale_ordinary(spec, request.controller)
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

pub(crate) fn existing_tun_route() -> io::Result<Option<String>> {
    let output = Command::new(ROUTE_COMMAND)
        .args(["-n", "get", "1.0.0.1"])
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(parse_existing_tun_route(&output.stdout))
}

pub(crate) fn wait_for_tun_route_release() -> io::Result<bool> {
    for attempt in 0..TUN_ROUTE_RELEASE_ATTEMPTS {
        if existing_tun_route()?.is_none() {
            return Ok(true);
        }
        if attempt + 1 < TUN_ROUTE_RELEASE_ATTEMPTS {
            std::thread::sleep(TUN_ROUTE_RELEASE_DELAY);
        }
    }
    Ok(false)
}

fn parse_existing_tun_route(output: &[u8]) -> Option<String> {
    let output = String::from_utf8_lossy(output);
    let mut interface = None;
    let mut gateway = None;
    let mut destination = None;
    let mut mask = None;
    for line in output.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim() {
            "interface" => interface = Some(value.trim()),
            "gateway" => gateway = Some(value.trim()),
            "destination" => destination = Some(value.trim()),
            "mask" => mask = Some(value.trim()),
            _ => {}
        }
    }
    let interface = interface.filter(|value| value.starts_with("utun"))?;
    if destination != Some("1.0.0.0") || mask != Some("255.0.0.0") {
        return None;
    }
    Some(format!(
        "interface={interface} gateway={}",
        gateway.unwrap_or("unknown")
    ))
}

fn is_terminal_registration_failure(exit_code: Option<i32>) -> bool {
    exit_code == Some(LOCAL_INSTALLER_FAILURE_EXIT)
}

fn wait_for_current_helper(control: &Path) -> io::Result<Output> {
    let mut last_status = None;
    for attempt in 1..=HELPER_READY_ATTEMPTS {
        if attempt > 1 {
            std::thread::sleep(HELPER_READY_DELAY);
        }
        let status = run_control(control, [OsStr::new("status")])?;
        if status.status.success() && is_current_status(&status.stdout) {
            return Ok(status);
        }
        record_event(
            LogLevel::Debug,
            "helper.prepare.waiting",
            format!("attempt={attempt}"),
        );
        last_status = Some(status);
    }
    Err(last_status.map_or_else(
        || io::Error::other("connect to privileged helper failed"),
        |status| control_error("connect to privileged helper", &status),
    ))
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
            record_event(
                LogLevel::Error,
                "helper.mihomo.start_failed",
                control_error("start privileged Mihomo", &output).to_string(),
            );
            return Err(control_error("start privileged Mihomo", &output));
        }
        let pid = parse_pid(&output.stdout, "started")?;
        record_event(
            LogLevel::Info,
            "helper.mihomo.started",
            format!("pid={pid}"),
        );
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
        match parse_helper_status(&output.stdout)? {
            HelperStatus::Stopped { reason } => {
                record_event(
                    LogLevel::Error,
                    "helper.mihomo.exited",
                    format!("expected_pid={} reason={reason}", self.pid),
                );
                Ok(Some(ProcessExit::failure()))
            }
            HelperStatus::Running { pid } if pid == self.pid => Ok(None),
            HelperStatus::Running { pid } => {
                record_event(
                    LogLevel::Error,
                    "helper.mihomo.ownership_lost",
                    format!("expected_pid={} actual_pid={pid}", self.pid),
                );
                Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "privileged helper no longer owns the expected Mihomo process",
                ))
            }
        }
    }

    fn terminate(&mut self) -> io::Result<ProcessExit> {
        let output = run_stop_control(&self.control, self.pid)?;
        if !output.status.success() {
            record_event(
                LogLevel::Error,
                "helper.mihomo.stop_failed",
                control_error("stop privileged Mihomo", &output).to_string(),
            );
            return Err(control_error("stop privileged Mihomo", &output));
        }
        record_event(
            LogLevel::Info,
            "helper.mihomo.stopped",
            format!("pid={}", self.pid),
        );
        Ok(ProcessExit::success())
    }
}

fn run_stop_control(control: &Path, pid: u32) -> io::Result<Output> {
    let args = stop_arguments(pid);
    run_control(control, args.iter().map(OsString::as_os_str))
}

fn stop_arguments(pid: u32) -> [OsString; 3] {
    [
        OsString::from("stop"),
        OsString::from("--pid"),
        OsString::from(pid.to_string()),
    ]
}

fn helper_control_path() -> io::Result<PathBuf> {
    let executable = std::env::current_exe()?;
    let directory = executable.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Manis executable has no containing directory",
        )
    })?;
    let control = directory.join(HELPER_CONTROL_NAME);
    if !control.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "TUN requires the signed Manis.app build with its privileged helper",
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

fn stage_managed_core(control: &Path) -> io::Result<()> {
    let output = run_control(control, [OsStr::new("stage-core")])?;
    if !output.status.success() {
        return Err(control_error(
            "stage the Manis-managed Mihomo core",
            &output,
        ));
    }
    record_event(
        LogLevel::Info,
        "helper.core.staged",
        String::from_utf8_lossy(&output.stdout).trim(),
    );
    Ok(())
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

fn is_current_status(bytes: &[u8]) -> bool {
    parse_helper_status(bytes).is_ok()
}

#[derive(Debug, Eq, PartialEq)]
enum HelperStatus {
    Running { pid: u32 },
    Stopped { reason: String },
}

fn parse_helper_status(bytes: &[u8]) -> io::Result<HelperStatus> {
    let output = String::from_utf8_lossy(bytes);
    let mut fields = output.split_whitespace();
    match fields.next() {
        Some("running") => {
            let pid = fields
                .next()
                .and_then(|value| value.parse::<u32>().ok())
                .filter(|pid| *pid > 0)
                .ok_or_else(invalid_helper_response)?;
            if fields.next() != Some(HELPER_PROTOCOL_VERSION) || fields.next().is_some() {
                return Err(invalid_helper_response());
            }
            Ok(HelperStatus::Running { pid })
        }
        Some("stopped") => {
            if fields.next() != Some(HELPER_PROTOCOL_VERSION) {
                return Err(invalid_helper_response());
            }
            let reason = fields.next().ok_or_else(invalid_helper_response)?;
            if fields.next().is_some()
                || !reason.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
                })
            {
                return Err(invalid_helper_response());
            }
            Ok(HelperStatus::Stopped {
                reason: reason.to_owned(),
            })
        }
        _ => Err(invalid_helper_response()),
    }
}

fn helper_status_detail(bytes: &[u8]) -> String {
    match parse_helper_status(bytes) {
        Ok(HelperStatus::Running { pid }) => format!("state=running pid={pid}"),
        Ok(HelperStatus::Stopped { reason }) => format!("state=stopped reason={reason}"),
        Err(_) => "state=invalid_response".to_owned(),
    }
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

    use manis_engine::{ControllerEndpoint, ManagedEngineConfig};

    use super::{
        HelperStatus, MacosPrivilegedProcessSpawner, is_current_status,
        is_terminal_registration_failure, parse_existing_tun_route, parse_helper_status, parse_pid,
        stop_arguments,
    };

    #[test]
    fn does_not_repeat_a_failed_local_admin_install() {
        assert!(is_terminal_registration_failure(Some(2)));
        assert!(!is_terminal_registration_failure(Some(1)));
        assert!(!is_terminal_registration_failure(None));
    }

    #[test]
    fn detects_the_mihomo_route_shape_owned_by_an_existing_tun() {
        let conflict = parse_existing_tun_route(
            b"destination: 1.0.0.0\nmask: 255.0.0.0\ngateway: 198.18.0.1\ninterface: utun4\n",
        );
        assert_eq!(
            conflict.as_deref(),
            Some("interface=utun4 gateway=198.18.0.1")
        );
        assert_eq!(
            parse_existing_tun_route(
                b"destination: default\ngateway: 192.168.3.1\ninterface: en1\n"
            ),
            None
        );
    }

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
        assert!(!is_current_status(b"stopped v2\n"));
        assert!(!is_current_status(b"stopped v3 not-started\n"));
        assert!(!is_current_status(b"stopped v4 not-started\n"));
        assert!(!is_current_status(b"stopped v5 not-started\n"));
        assert!(!is_current_status(b"stopped v6 not-started\n"));
        assert!(!is_current_status(b"stopped v7 not-started\n"));
        assert!(!is_current_status(b"running 42 v7\n"));
        assert!(is_current_status(b"stopped v8 not-started\n"));
        assert!(is_current_status(b"running 42 v8\n"));
        assert_eq!(
            parse_helper_status(b"running 42 v8\n").unwrap(),
            HelperStatus::Running { pid: 42 }
        );
        assert_eq!(
            parse_helper_status(b"stopped v8 unexpected-signal-9\n").unwrap(),
            HelperStatus::Stopped {
                reason: "unexpected-signal-9".to_owned()
            }
        );
        assert!(parse_helper_status(b"stopped v3 bad reason\n").is_err());
    }

    #[test]
    fn privileged_stop_requests_carry_the_expected_pid() {
        assert_eq!(
            stop_arguments(42),
            [
                std::ffi::OsString::from("stop"),
                std::ffi::OsString::from("--pid"),
                std::ffi::OsString::from("42"),
            ]
        );
    }

    #[test]
    fn accepts_only_the_manis_mihomo_launch_shape() {
        let root = PathBuf::from("/Users/example/Library/Application Support/Manis/mihomo");
        let config = root.join("manis-generated.yaml");
        let controller = root.join("controller.sock");
        let launch = ManagedEngineConfig::new(
            PathBuf::from("/Applications/Manis.app/Contents/Resources/mihomo/mihomo"),
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
    fn rejects_non_manis_mihomo_launch_shapes() {
        let root = PathBuf::from("/Users/example/Library/Application Support/Manis/mihomo");
        let launch = ManagedEngineConfig::new(
            PathBuf::from("/Applications/Manis.app/Contents/Resources/mihomo/mihomo"),
            root.join("manis-generated.yaml"),
            root.clone(),
            ControllerEndpoint::Tcp("127.0.0.1:9090".parse().unwrap()),
        )
        .launch_command();

        assert!(MacosPrivilegedProcessSpawner::parse_launch(&launch).is_err());
    }

    #[test]
    fn rejects_controller_paths_outside_the_runtime() {
        let root = PathBuf::from("/Users/example/Library/Application Support/Manis/mihomo");
        let launch = ManagedEngineConfig::new(
            PathBuf::from("/Applications/Manis.app/Contents/Resources/mihomo/mihomo"),
            root.join("manis-generated.yaml"),
            root.clone(),
            ControllerEndpoint::UnixSocket(root.join("nested/controller.sock")),
        )
        .launch_command();

        assert!(MacosPrivilegedProcessSpawner::parse_launch(&launch).is_err());
    }
}
