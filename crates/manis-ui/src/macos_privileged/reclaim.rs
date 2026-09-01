use std::ffi::OsStr;
use std::io;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use manis_engine::CommandSpec;

use crate::diagnostics::{LogLevel, record_event};

use super::control_error;

const LSOF_COMMAND: &str = "/usr/sbin/lsof";
const PS_COMMAND: &str = "/bin/ps";
const KILL_COMMAND: &str = "/bin/kill";
const ORDINARY_CORE_STOP_ATTEMPTS: usize = 20;
const ORDINARY_CORE_STOP_DELAY: Duration = Duration::from_millis(50);

pub(super) fn reclaim_stale_ordinary(spec: &CommandSpec, controller: &Path) -> io::Result<()> {
    for pid in controller_owner_pids(controller)? {
        let Some(program) = process_program(pid)? else {
            continue;
        };
        if !is_expected_ordinary_process(&program, spec) {
            record_event(
                LogLevel::Error,
                "helper.recovery.ordinary_core_rejected",
                format!("pid={pid} reason=launch_mismatch"),
            );
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("Manis controller socket is owned by an unexpected process (pid {pid})"),
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

fn controller_owner_pids(controller: &Path) -> io::Result<Vec<u32>> {
    let output = Command::new(LSOF_COMMAND)
        .args([OsStr::new("-t"), OsStr::new("--"), controller.as_os_str()])
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    controller_owner_pids_from_output(&output)
}

fn controller_owner_pids_from_output(output: &Output) -> io::Result<Vec<u32>> {
    if output.status.success() {
        return parse_lsof_pids(&output.stdout);
    }
    if output.stdout.is_empty() && output.status.code() == Some(1) {
        return Ok(Vec::new());
    }
    Err(control_error("query controller owner with lsof", output))
}

fn parse_lsof_pids(bytes: &[u8]) -> io::Result<Vec<u32>> {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            (!trimmed.is_empty()).then_some(trimmed)
        })
        .map(|line| {
            let pid = line
                .parse::<u32>()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid lsof pid"))?;
            (pid != 0)
                .then_some(pid)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid lsof pid"))
        })
        .collect()
}

fn process_program(pid: u32) -> io::Result<Option<String>> {
    let output = Command::new(PS_COMMAND)
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    let program = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok((!program.is_empty()).then_some(program))
}

fn is_expected_ordinary_process(program: &str, spec: &CommandSpec) -> bool {
    let program = Path::new(program);
    program == spec.program() || program.file_name() == spec.program().file_name()
}

fn stop_expected_ordinary_process(pid: u32, spec: &CommandSpec) -> io::Result<()> {
    match process_program(pid)? {
        Some(program) if is_expected_ordinary_process(&program, spec) => {}
        Some(_) | None => return Ok(()),
    }

    let signal = Command::new(KILL_COMMAND)
        .args(["-TERM", &pid.to_string()])
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()?;
    if !signal.status.success() && process_program(pid)?.is_some() {
        return Err(control_error("stop stale ordinary Mihomo", &signal));
    }

    for attempt in 0..ORDINARY_CORE_STOP_ATTEMPTS {
        match process_program(pid)? {
            None => return Ok(()),
            Some(program) if !is_expected_ordinary_process(&program, spec) => return Ok(()),
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

#[cfg(test)]
mod tests {
    use std::os::unix::process::ExitStatusExt as _;
    use std::path::PathBuf;
    use std::process::{ExitStatus, Output};

    use manis_engine::{ControllerEndpoint, ManagedEngineConfig};

    use super::{controller_owner_pids_from_output, is_expected_ordinary_process, parse_lsof_pids};

    fn command_output(code: i32, stdout: &[u8], stderr: &[u8]) -> Output {
        Output {
            status: ExitStatus::from_raw(code << 8),
            stdout: stdout.to_vec(),
            stderr: stderr.to_vec(),
        }
    }

    #[test]
    fn ordinary_core_check_matches_only_mihomo_executable() {
        let root = PathBuf::from("/Users/example/Library/Application Support/Manis/mihomo");
        let launch = ManagedEngineConfig::new(
            PathBuf::from("/Applications/Manis.app/Contents/Resources/mihomo/mihomo"),
            root.join("manis-generated.yaml"),
            root.clone(),
            ControllerEndpoint::UnixSocket(root.join("controller.sock")),
        )
        .launch_command();

        assert!(is_expected_ordinary_process(
            &launch.program().to_string_lossy(),
            &launch
        ));
        assert!(is_expected_ordinary_process("mihomo", &launch));
        assert!(!is_expected_ordinary_process("verge-mihomo", &launch));
    }

    #[test]
    fn lsof_pid_output_rejects_invalid_process_ids() {
        assert_eq!(parse_lsof_pids(b"20372\n").unwrap(), vec![20372]);
        assert!(parse_lsof_pids(b"20372\nnot-a-pid\n").is_err());
        assert!(parse_lsof_pids(b"0\n").is_err());
    }

    #[test]
    fn lsof_status_distinguishes_no_match_from_command_failure() {
        assert_eq!(
            controller_owner_pids_from_output(&command_output(0, b"20372\n", b"")).unwrap(),
            vec![20372]
        );
        assert_eq!(
            controller_owner_pids_from_output(&command_output(1, b"", b"")).unwrap(),
            Vec::<u32>::new()
        );
        assert!(controller_owner_pids_from_output(&command_output(2, b"", b"denied")).is_err());
    }
}
