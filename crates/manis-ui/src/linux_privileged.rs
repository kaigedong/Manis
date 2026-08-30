use std::fmt;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output};

const PKEXEC: &str = "/usr/bin/pkexec";
const SETCAP: &str = "/usr/bin/setcap";
const GETCAP: &str = "/usr/bin/getcap";
const TUN_CAPABILITIES: &str = "cap_net_admin,cap_net_raw=ep";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CapabilityState {
    AlreadyGranted,
    Granted,
}

#[derive(Debug)]
pub(crate) enum LinuxPrivilegeError {
    PackagedCoreUnavailable,
    UnsafePackagedCore,
    Inspect(std::io::Error),
    Authorization(std::io::Error),
    AuthorizationDenied(Option<i32>),
    VerificationFailed,
}

impl fmt::Display for LinuxPrivilegeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PackagedCoreUnavailable => formatter.write_str(
                "Linux TUN requires the packaged Manis Mihomo core; reinstall manis-bin",
            ),
            Self::UnsafePackagedCore => formatter.write_str(
                "Linux TUN refused a packaged Mihomo core that is not safely owned by root",
            ),
            Self::Inspect(error) => {
                write!(formatter, "Mihomo TUN capabilities could not be inspected: {error}")
            }
            Self::Authorization(error) => {
                write!(formatter, "Linux TUN authorization could not be started: {error}")
            }
            Self::AuthorizationDenied(Some(126)) => {
                formatter.write_str("Linux TUN authorization was canceled")
            }
            Self::AuthorizationDenied(code) => write!(
                formatter,
                "Linux TUN authorization was denied{}",
                code.map_or_else(String::new, |code| format!(" (exit code {code})"))
            ),
            Self::VerificationFailed => formatter.write_str(
                "Linux granted authorization, but Mihomo still lacks the capabilities required for TUN",
            ),
        }
    }
}

pub(crate) fn ensure_tun_capabilities() -> Result<(CapabilityState, PathBuf), LinuxPrivilegeError> {
    let binary = packaged_tun_core()?;
    if inspect_tun_capabilities(&binary)? {
        return Ok((CapabilityState::AlreadyGranted, binary));
    }

    let status = Command::new(PKEXEC)
        .arg(SETCAP)
        .arg(TUN_CAPABILITIES)
        .arg(&binary)
        .status()
        .map_err(LinuxPrivilegeError::Authorization)?;
    require_authorized(status)?;
    if !inspect_tun_capabilities(&binary)? {
        return Err(LinuxPrivilegeError::VerificationFailed);
    }
    Ok((CapabilityState::Granted, binary))
}

pub(crate) fn packaged_tun_core() -> Result<PathBuf, LinuxPrivilegeError> {
    let path = crate::core_update::bundled_seed_path()
        .ok_or(LinuxPrivilegeError::PackagedCoreUnavailable)?
        .canonicalize()
        .map_err(|_error| LinuxPrivilegeError::PackagedCoreUnavailable)?;
    let metadata = path
        .metadata()
        .map_err(|_error| LinuxPrivilegeError::PackagedCoreUnavailable)?;
    if !metadata.is_file() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
        return Err(LinuxPrivilegeError::UnsafePackagedCore);
    }
    Ok(path)
}

fn inspect_tun_capabilities(binary: &Path) -> Result<bool, LinuxPrivilegeError> {
    let output = Command::new(GETCAP)
        .arg(binary)
        .output()
        .map_err(LinuxPrivilegeError::Inspect)?;
    parse_getcap_output(&output)
}

fn parse_getcap_output(output: &Output) -> Result<bool, LinuxPrivilegeError> {
    if !output.status.success() {
        return Err(LinuxPrivilegeError::Inspect(std::io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().any(line_has_tun_capabilities))
}

fn line_has_tun_capabilities(line: &str) -> bool {
    let Some(specification) = line.split_ascii_whitespace().next_back() else {
        return false;
    };
    let Some(separator) = specification.rfind(['=', '+']) else {
        return false;
    };
    let (capabilities, flags) = specification.split_at(separator);
    let flags = &flags[1..];
    let capabilities = capabilities.split(',').collect::<Vec<_>>();
    capabilities.contains(&"cap_net_admin")
        && capabilities.contains(&"cap_net_raw")
        && flags.contains('e')
        && flags.contains('p')
}

fn require_authorized(status: ExitStatus) -> Result<(), LinuxPrivilegeError> {
    if status.success() {
        Ok(())
    } else {
        Err(LinuxPrivilegeError::AuthorizationDenied(status.code()))
    }
}

#[cfg(test)]
mod tests {
    use super::line_has_tun_capabilities;

    #[test]
    fn recognizes_required_effective_and_permitted_capabilities() {
        assert!(line_has_tun_capabilities(
            "/home/bobo/.local/share/manis/core/mihomo cap_net_admin,cap_net_raw=ep"
        ));
        assert!(line_has_tun_capabilities(
            "/path with spaces/mihomo cap_net_raw,cap_net_admin+eip"
        ));
    }

    #[test]
    fn rejects_partial_or_non_effective_capabilities() {
        assert!(!line_has_tun_capabilities("/core/mihomo cap_net_admin=ep"));
        assert!(!line_has_tun_capabilities(
            "/core/mihomo cap_net_admin,cap_net_raw=p"
        ));
        assert!(!line_has_tun_capabilities("/core/mihomo"));
        assert!(!line_has_tun_capabilities(""));
    }
}
