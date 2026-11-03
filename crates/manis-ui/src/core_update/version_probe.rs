use super::{
    Command, CoreUpdateError, CoreUpdateFailureKind, Duration, Instant, MAX_VERSION_OUTPUT_BYTES,
    Path, Stdio, VERSION_PROBE_POLL_INTERVAL, VERSION_PROBE_TIMEOUT, fs, remove_file_if_exists,
    thread, unique_sibling_path,
};

pub(crate) fn validate_binary_version(
    binary: &Path,
    expected_version: &str,
) -> Result<(), CoreUpdateError> {
    let reported = reported_binary_version_with_timeout(binary, VERSION_PROBE_TIMEOUT)?;
    let expected = format!("v{}", expected_version.trim_start_matches('v'));
    if reported == expected {
        Ok(())
    } else {
        Err(CoreUpdateError::VersionMismatch)
    }
}

pub(super) fn reported_binary_version_with_timeout(
    binary: &Path,
    timeout: Duration,
) -> Result<String, CoreUpdateError> {
    let stdout_path = unique_sibling_path(binary, "version-stdout");
    let stderr_path = unique_sibling_path(binary, "version-stderr");
    let stdout = create_version_output_file(&stdout_path)?;
    let stderr = match create_version_output_file(&stderr_path) {
        Ok(file) => file,
        Err(error) => {
            remove_file_if_exists(&stdout_path);
            return Err(error);
        }
    };
    let result = (|| {
        let mut child = Command::new(binary)
            .arg("-v")
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()
            .map_err(|error| {
                CoreUpdateError::caused(
                    CoreUpdateFailureKind::VersionMismatch,
                    "start core version probe",
                    error,
                )
            })?;
        let started = Instant::now();
        let status = loop {
            match child.try_wait().map_err(|error| {
                CoreUpdateError::caused(
                    CoreUpdateFailureKind::VersionMismatch,
                    "poll core version probe",
                    error,
                )
            })? {
                Some(status) => break status,
                None if started.elapsed() < timeout => thread::sleep(VERSION_PROBE_POLL_INTERVAL),
                None => {
                    if let Err(kill_error) = child.kill() {
                        match child.try_wait() {
                            Ok(Some(_status)) => {}
                            Ok(None) => {
                                return Err(CoreUpdateError::caused(
                                    CoreUpdateFailureKind::VersionMismatch,
                                    "terminate timed-out core version probe",
                                    kill_error,
                                ));
                            }
                            Err(wait_error) => {
                                return Err(CoreUpdateError::caused(
                                    CoreUpdateFailureKind::VersionMismatch,
                                    "inspect core version probe after termination failure",
                                    wait_error,
                                ));
                            }
                        }
                    } else {
                        child.wait().map_err(|error| {
                            CoreUpdateError::caused(
                                CoreUpdateFailureKind::VersionMismatch,
                                "reap timed-out core version probe",
                                error,
                            )
                        })?;
                    }
                    return Err(CoreUpdateError::VersionMismatch);
                }
            }
        };
        if !status.success() {
            return Err(CoreUpdateError::VersionMismatch);
        }
        let stdout = read_bounded_version_output(&stdout_path)?;
        let stderr = read_bounded_version_output(&stderr_path)?;
        parse_reported_version(&format!("{stdout} {stderr}"))
            .ok_or(CoreUpdateError::VersionMismatch)
    })();
    remove_file_if_exists(&stdout_path);
    remove_file_if_exists(&stderr_path);
    result
}

pub(super) fn create_version_output_file(path: &Path) -> Result<fs::File, CoreUpdateError> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            CoreUpdateError::caused(
                CoreUpdateFailureKind::VersionMismatch,
                "create core version output",
                error,
            )
        })
}

pub(super) fn read_bounded_version_output(path: &Path) -> Result<String, CoreUpdateError> {
    let metadata = fs::metadata(path).map_err(|error| {
        CoreUpdateError::caused(
            CoreUpdateFailureKind::VersionMismatch,
            "inspect core version output",
            error,
        )
    })?;
    if metadata.len() > MAX_VERSION_OUTPUT_BYTES {
        return Err(CoreUpdateError::VersionMismatch);
    }
    fs::read_to_string(path).map_err(|error| {
        CoreUpdateError::caused(
            CoreUpdateFailureKind::VersionMismatch,
            "read core version output",
            error,
        )
    })
}

pub(super) fn parse_reported_version(reported: &str) -> Option<String> {
    reported
        .split_whitespace()
        .find(|field| {
            let version = field.strip_prefix('v').unwrap_or(field);
            let mut parts = version.split('.');
            parts.next().is_some_and(|part| part.parse::<u64>().is_ok())
                && parts.next().is_some_and(|part| part.parse::<u64>().is_ok())
                && parts.next().is_some_and(|part| {
                    part.trim_end_matches(|character: char| !character.is_ascii_digit())
                        .parse::<u64>()
                        .is_ok()
                })
        })
        .map(|field| {
            let trimmed = field.trim_end_matches(|character: char| {
                !character.is_ascii_alphanumeric() && character != '.' && character != '-'
            });
            if trimmed.starts_with('v') {
                trimmed.to_owned()
            } else {
                format!("v{trimmed}")
            }
        })
}
