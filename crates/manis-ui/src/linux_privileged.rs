use std::ffi::{OsStr, OsString};
use std::fmt;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output, Stdio};

const PKEXEC: &str = "/usr/bin/pkexec";
const SETCAP: &str = "/usr/bin/setcap";
const GETCAP: &str = "/usr/bin/getcap";
const RESOLVECTL: &str = "/usr/bin/resolvectl";
const TUN_DNS_HELPER_FLAG: &str = "--manis-linux-tun-dns-helper";
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
    UnsafeApplication,
    InvalidDnsHelperRequest,
    DnsHelperRequiresRoot,
    TunInterfaceUnavailable,
    DnsCommandFailed(&'static str, Option<i32>),
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
                "Linux granted authorization, but Mihomo still lacks the capabilities required for TUN",
            ),
            Self::UnsafeApplication => formatter.write_str(
                "Linux TUN DNS requires a root-owned Manis installation; reinstall manis-bin",
            ),
            Self::InvalidDnsHelperRequest => {
                formatter.write_str("Linux TUN DNS helper received an invalid request")
            }
            Self::DnsHelperRequiresRoot => {
                formatter.write_str("Linux TUN DNS helper was not started with administrator access")
            }
            Self::TunInterfaceUnavailable => formatter.write_str(
                "Mihomo did not create the expected Linux TUN interface for DNS routing",
            ),
            Self::DnsCommandFailed(action, code) => write!(
                formatter,
                "systemd-resolved could not {action}{}",
                code.map_or_else(String::new, |code| format!(" (exit code {code})"))
            ),
            Self::DnsHelperFailed(code) => write!(
                formatter,
                "Linux TUN DNS authorization succeeded, but the helper failed{}",
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

pub(crate) fn install_tun_dns() -> Result<(), LinuxPrivilegeError> {
    request_tun_dns_helper(TunDnsHelperAction::Install)
}

pub(crate) fn restore_tun_dns() -> Result<(), LinuxPrivilegeError> {
    if !tun_interface_path().is_dir() {
        return Ok(());
    }
    request_tun_dns_helper(TunDnsHelperAction::Restore)
}

pub(crate) fn run_tun_dns_helper_from_args(
    args: impl IntoIterator<Item = OsString>,
) -> Option<Result<(), LinuxPrivilegeError>> {
    let mut args = args.into_iter();
    if args.next().as_deref() != Some(OsStr::new(TUN_DNS_HELPER_FLAG)) {
        return None;
    }
    let result = (|| {
        let action = match args.next().as_deref() {
            Some(value) if value == OsStr::new("install") => TunDnsHelperAction::Install,
            Some(value) if value == OsStr::new("restore") => TunDnsHelperAction::Restore,
            _ => return Err(LinuxPrivilegeError::InvalidDnsHelperRequest),
        };
        if args.next().is_some() {
            return Err(LinuxPrivilegeError::InvalidDnsHelperRequest);
        }
        let process = std::fs::metadata("/proc/self").map_err(LinuxPrivilegeError::Inspect)?;
        if process.uid() != 0 {
            return Err(LinuxPrivilegeError::DnsHelperRequiresRoot);
        }
        run_tun_dns_helper(action)
    })();
    Some(result)
}

fn request_tun_dns_helper(action: TunDnsHelperAction) -> Result<(), LinuxPrivilegeError> {
    if !tun_interface_path().is_dir() {
        return Err(LinuxPrivilegeError::TunInterfaceUnavailable);
    }
    let executable = trusted_current_executable()?;
    let status = Command::new(PKEXEC)
        .arg(executable)
        .arg(TUN_DNS_HELPER_FLAG)
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

fn run_tun_dns_helper(action: TunDnsHelperAction) -> Result<(), LinuxPrivilegeError> {
    match action {
        TunDnsHelperAction::Install => {
            run_resolvectl(
                &[
                    "dns",
                    manis_profile::LINUX_TUN_DEVICE,
                    manis_profile::LINUX_TUN_DNS_SERVER,
                ],
                "set the TUN DNS server",
            )?;
            if let Err(error) = run_resolvectl(
                &["domain", manis_profile::LINUX_TUN_DEVICE, "~."],
                "route DNS through the TUN interface",
            ) {
                let _ = revert_tun_dns();
                return Err(error);
            }
            if let Err(error) = flush_dns_cache() {
                let _ = revert_tun_dns();
                return Err(error);
            }
            Ok(())
        }
        TunDnsHelperAction::Restore => {
            if tun_interface_path().is_dir() {
                revert_tun_dns()?;
            }
            flush_dns_cache()
        }
    }
}

fn revert_tun_dns() -> Result<(), LinuxPrivilegeError> {
    run_resolvectl(
        &["revert", manis_profile::LINUX_TUN_DEVICE],
        "restore the original DNS route",
    )
}

fn flush_dns_cache() -> Result<(), LinuxPrivilegeError> {
    run_resolvectl(&["flush-caches"], "flush the DNS cache")
}

fn run_resolvectl(args: &[&str], action: &'static str) -> Result<(), LinuxPrivilegeError> {
    let status = Command::new(RESOLVECTL)
        .args(args)
        .env_clear()
        .stdin(Stdio::null())
        .status()
        .map_err(LinuxPrivilegeError::Authorization)?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| LinuxPrivilegeError::DnsCommandFailed(action, status.code()))
}

fn tun_interface_path() -> PathBuf {
    Path::new("/sys/class/net").join(manis_profile::LINUX_TUN_DEVICE)
}

fn trusted_current_executable() -> Result<PathBuf, LinuxPrivilegeError> {
    let path = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .map_err(LinuxPrivilegeError::Authorization)?;
    let metadata = path
        .metadata()
        .map_err(LinuxPrivilegeError::Authorization)?;
    if !metadata.is_file() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
        return Err(LinuxPrivilegeError::UnsafeApplication);
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

fn require_authorized(status: ExitStatus) -> Result<(), LinuxPrivilegeError> {
    if status.success() {
        Ok(())
    } else {
        Err(LinuxPrivilegeError::AuthorizationDenied(status.code()))
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{LinuxPrivilegeError, line_has_tun_capabilities, run_tun_dns_helper_from_args};

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

    #[test]
    fn linux_tun_dns_helper_ignores_normal_arguments_and_rejects_malformed_requests() {
        assert!(run_tun_dns_helper_from_args([OsString::from("--version")]).is_none());
        assert!(matches!(
            run_tun_dns_helper_from_args([
                OsString::from("--manis-linux-tun-dns-helper"),
                OsString::from("unknown"),
            ]),
            Some(Err(LinuxPrivilegeError::InvalidDnsHelperRequest))
        ));
        assert!(matches!(
            run_tun_dns_helper_from_args([
                OsString::from("--manis-linux-tun-dns-helper"),
                OsString::from("install"),
                OsString::from("unexpected"),
            ]),
            Some(Err(LinuxPrivilegeError::InvalidDnsHelperRequest))
        ));
    }
}
