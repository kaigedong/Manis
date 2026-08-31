use std::fmt;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PKEXEC: &str = "/usr/bin/pkexec";
const GETCAP: &str = "/usr/bin/getcap";
const TUN_DNS_HELPER: &str = "/usr/lib/manis/manis-linux-helper";

#[derive(Debug)]
pub(crate) enum LinuxPrivilegeError {
    PackagedCoreUnavailable,
    UnsafePackagedCore,
    Inspect(std::io::Error),
    Authorization(std::io::Error),
    AuthorizationDenied(Option<i32>),
    VerificationFailed,
    DnsHelperUnavailable,
    UnsafeDnsHelper,
    TunInterfaceUnavailable,
    DnsHelperFailed(Option<i32>),
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
                "the packaged Mihomo core lacks Linux TUN capabilities; reinstall or upgrade manis-bin",
            ),
            Self::DnsHelperUnavailable => formatter.write_str(
                "Linux TUN DNS requires the packaged Manis helper; reinstall or upgrade manis-bin",
            ),
            Self::UnsafeDnsHelper => formatter
                .write_str("Linux TUN DNS refused a helper that is not safely owned by root"),
            Self::TunInterfaceUnavailable => formatter.write_str(
                "Mihomo did not create the expected Linux TUN interface for DNS routing",
            ),
            Self::DnsHelperFailed(code) => write!(
                formatter,
                "the packaged Linux TUN DNS helper failed{}",
                code.map_or_else(String::new, |code| format!(" (exit code {code})"))
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TunDnsHelperAction {
    Install,
    Restore,
}

impl TunDnsHelperAction {
    const fn argument(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Restore => "restore",
        }
    }
}

pub(crate) fn ensure_tun_capabilities() -> Result<PathBuf, LinuxPrivilegeError> {
    let binary = packaged_tun_core()?;
    if inspect_tun_capabilities(&binary)? {
        return Ok(binary);
    }
    Err(LinuxPrivilegeError::VerificationFailed)
}

pub(crate) fn install_tun_dns() -> Result<(), LinuxPrivilegeError> {
    request_tun_dns_helper(TunDnsHelperAction::Install)
}

pub(crate) fn restore_tun_dns() -> Result<(), LinuxPrivilegeError> {
    request_tun_dns_helper(TunDnsHelperAction::Restore)
}

fn request_tun_dns_helper(action: TunDnsHelperAction) -> Result<(), LinuxPrivilegeError> {
    if action == TunDnsHelperAction::Install && !tun_interface_path().is_dir() {
        return Err(LinuxPrivilegeError::TunInterfaceUnavailable);
    }
    let executable = trusted_packaged_dns_helper()?;
    let status = Command::new(PKEXEC)
        .arg(executable)
        .arg(action.argument())
        .status()
        .map_err(LinuxPrivilegeError::Authorization)?;
    if status.success() {
        Ok(())
    } else if status.code() == Some(126) {
        Err(LinuxPrivilegeError::AuthorizationDenied(status.code()))
    } else {
        Err(LinuxPrivilegeError::DnsHelperFailed(status.code()))
    }
}

fn tun_interface_path() -> PathBuf {
    Path::new("/sys/class/net").join(manis_profile::LINUX_TUN_DEVICE)
}

fn trusted_packaged_dns_helper() -> Result<PathBuf, LinuxPrivilegeError> {
    let path = PathBuf::from(TUN_DNS_HELPER);
    let metadata = path
        .metadata()
        .map_err(|_error| LinuxPrivilegeError::DnsHelperUnavailable)?;
    if !metadata.is_file() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
        return Err(LinuxPrivilegeError::UnsafeDnsHelper);
    }
    Ok(path)
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
