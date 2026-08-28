#![cfg(target_os = "macos")]

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use manis_engine::{CommandSpec, ManagedChild, ProcessExit, ProcessSpawner, StdProcessSpawner};

use crate::diagnostics::{LogLevel, record_event};

const HELPER_CONTROL_NAME: &str = "manis-helperctl";
const HELPER_PROTOCOL_VERSION: &str = "v5";
const HELPER_REGISTRATION_ATTEMPTS: usize = 2;
const LOCAL_INSTALLER_FAILURE_EXIT: i32 = 2;
const HELPER_READY_ATTEMPTS: usize = 6;
const HELPER_READY_DELAY: Duration = Duration::from_millis(450);
const ROUTE_COMMAND: &str = "/sbin/route";
const TUN_ROUTE_RELEASE_ATTEMPTS: usize = 10;
const TUN_ROUTE_RELEASE_DELAY: Duration = Duration::from_millis(50);
const LSOF_COMMAND: &str = "/usr/sbin/lsof";
const PS_COMMAND: &str = "/bin/ps";
const KILL_COMMAND: &str = "/bin/kill";
const ORDINARY_CORE_STOP_ATTEMPTS: usize = 20;
const ORDINARY_CORE_STOP_DELAY: Duration = Duration::from_millis(50);

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
        let control = helper_control_path()?;
        let status = run_control(&control, [OsStr::new("status")])?;
        if !status.status.success() || !is_current_status(&status.stdout) {
            return Ok(None);
        }
        if let HelperStatus::Running { pid } = parse_helper_status(&status.stdout)? {
            let stopped = run_control(&control, [OsStr::new("stop")])?;
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

    /// Stops an unprivileged Manis core left behind by an earlier UI process.
    ///
    /// The controller socket identifies the candidate process, but is not sufficient proof of
    /// ownership on its own. The complete process command must also equal the exact launch shape
    /// for the current Manis runtime before it is terminated.
    pub(crate) fn reclaim_stale_ordinary(spec: &CommandSpec) -> io::Result<()> {
        let request = Self::parse_launch(spec)?;
        for pid in controller_owner_pids(request.controller)? {
            let Some(command) = process_command(pid)? else {
                continue;
            };
            if !is_expected_ordinary_process(&command, spec) {
                record_event(
                    LogLevel::Error,
                    "helper.recovery.ordinary_core_rejected",
                    format!("pid={pid} reason=launch_mismatch"),
                );
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!(
                        "Manis controller socket is owned by an unexpected process (pid {pid})"
                    ),
                ));
            }

            record_event(
                LogLevel::Info,
                "helper.recovery.ordinary_core_detected",
                format!("pid={pid}"),
            );
            stop_expected_ordinary_process(pid, spec)?;
            record_event(
                LogLevel::Info,
                "helper.recovery.ordinary_core_stopped",
                format!("pid={pid}"),
            );
        }
        Ok(())
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

fn controller_owner_pids(controller: &Path) -> io::Result<Vec<u32>> {
    let output = Command::new(LSOF_COMMAND)
        .args([OsStr::new("-t"), OsStr::new("--"), controller.as_os_str()])
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    if output.stdout.is_empty() {
        return Ok(Vec::new());
    }
    parse_lsof_pids(&output.stdout)
}

fn parse_lsof_pids(bytes: &[u8]) -> io::Result<Vec<u32>> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(|line| {
            line.trim()
                .parse::<u32>()
                .ok()
                .filter(|pid| *pid > 0)
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "lsof returned an invalid process identifier",
                    )
                })
        })
        .collect()
}

fn process_command(pid: u32) -> io::Result<Option<String>> {
    let output = Command::new(PS_COMMAND)
        .args(["-ww", "-p", &pid.to_string(), "-o", "command="])
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    let command = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok((!command.is_empty()).then_some(command))
}

fn expected_process_command(spec: &CommandSpec) -> String {
    let mut command = spec.program().to_string_lossy().into_owned();
    for argument in spec.args() {
        command.push(' ');
        command.push_str(&argument.to_string_lossy());
    }
    command
}

fn is_expected_ordinary_process(command: &str, spec: &CommandSpec) -> bool {
    command == expected_process_command(spec)
}

fn stop_expected_ordinary_process(pid: u32, spec: &CommandSpec) -> io::Result<()> {
    match process_command(pid)? {
        Some(command) if is_expected_ordinary_process(&command, spec) => {}
        Some(_) | None => return Ok(()),
    }

    let signal = Command::new(KILL_COMMAND)
        .args(["-TERM", &pid.to_string()])
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()?;
    if !signal.status.success() && process_command(pid)?.is_some() {
        return Err(control_error("stop stale ordinary Mihomo", &signal));
    }

    for attempt in 0..ORDINARY_CORE_STOP_ATTEMPTS {
        match process_command(pid)? {
            None => return Ok(()),
            Some(command) if !is_expected_ordinary_process(&command, spec) => return Ok(()),
            Some(_) if attempt + 1 < ORDINARY_CORE_STOP_ATTEMPTS => {
                std::thread::sleep(ORDINARY_CORE_STOP_DELAY);
            }
            Some(_) => {}
        }
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        format!("stale ordinary Mihomo pid {pid} did not stop after SIGTERM"),
    ))
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
        let output = run_control(&self.control, [OsStr::new("stop")])?;
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
        is_expected_ordinary_process, is_terminal_registration_failure, parse_existing_tun_route,
        parse_helper_status, parse_lsof_pids, parse_pid,
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
        assert!(is_current_status(b"stopped v5 not-started\n"));
        assert!(is_current_status(b"running 42 v5\n"));
        assert_eq!(
            parse_helper_status(b"running 42 v5\n").unwrap(),
            HelperStatus::Running { pid: 42 }
        );
        assert_eq!(
            parse_helper_status(b"stopped v5 unexpected-signal-9\n").unwrap(),
            HelperStatus::Stopped {
                reason: "unexpected-signal-9".to_owned()
            }
        );
        assert!(parse_helper_status(b"stopped v3 bad reason\n").is_err());
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

    #[test]
    fn recognizes_only_the_exact_manis_ordinary_process() {
        let root = PathBuf::from("/Users/example/Library/Application Support/Manis/mihomo");
        let launch = ManagedEngineConfig::new(
            PathBuf::from("/Applications/Manis.app/Contents/Resources/mihomo/mihomo"),
            root.join("manis-generated.yaml"),
            root.clone(),
            ControllerEndpoint::UnixSocket(root.join("controller.sock")),
        )
        .launch_command();
        let manis = "/Applications/Manis.app/Contents/Resources/mihomo/mihomo -d /Users/example/Library/Application Support/Manis/mihomo -f /Users/example/Library/Application Support/Manis/mihomo/manis-generated.yaml -ext-ctl-unix /Users/example/Library/Application Support/Manis/mihomo/controller.sock";
        let clash_verge = "/Applications/Clash Verge.app/Contents/MacOS/verge-mihomo -d /Users/example/Library/Application Support/io.github.clash-verge-rev.clash-verge-rev -f /Users/example/Library/Application Support/io.github.clash-verge-rev.clash-verge-rev/clash-verge.yaml -ext-ctl-unix /tmp/verge/verge-mihomo.sock";

        assert!(is_expected_ordinary_process(manis, &launch));
        assert!(!is_expected_ordinary_process(clash_verge, &launch));
    }

    #[test]
    fn parses_only_positive_lsof_process_identifiers() {
        assert_eq!(parse_lsof_pids(b"20372\n42\n").unwrap(), vec![20372, 42]);
        assert!(parse_lsof_pids(b"20372\nnot-a-pid\n").is_err());
        assert!(parse_lsof_pids(b"0\n").is_err());
    }
}
