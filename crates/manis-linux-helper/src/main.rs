#[cfg(target_os = "linux")]
mod linux {
    use std::env;
    use std::fmt;
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
    use std::path::{Path, PathBuf};
    use std::process::{Command, ExitCode, Stdio};

    const MANAGED_CORE: &str = "/usr/lib/manis/mihomo";
    const RESOLVECTL: &str = "/usr/bin/resolvectl";
    const TUN_DEVICE: &str = "Meta";
    const TUN_DNS_SERVER: &str = "198.18.0.2";

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Action {
        Install,
        Restore,
    }

    impl Action {
        fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, HelperError> {
            let action = match args.next().as_deref() {
                Some("install") => Self::Install,
                Some("restore") => Self::Restore,
                _ => return Err(HelperError::InvalidRequest),
            };
            if args.next().is_some() {
                return Err(HelperError::InvalidRequest);
            }
            Ok(action)
        }
    }

    #[derive(Debug)]
    enum HelperError {
        InvalidRequest,
        RequiresRoot,
        UnsafeExecutable,
        CallerUnavailable,
        ManagedCoreUnavailable,
        TunUnavailable,
        CommandFailed(&'static str, Option<i32>),
        Io(std::io::Error),
    }

    impl fmt::Display for HelperError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::InvalidRequest => formatter.write_str("expected exactly install or restore"),
                Self::RequiresRoot => formatter.write_str("the Manis Linux helper requires root"),
                Self::UnsafeExecutable => {
                    formatter.write_str("the Manis Linux helper executable is not safely owned")
                }
                Self::CallerUnavailable => {
                    formatter.write_str("the PolicyKit caller identity is unavailable")
                }
                Self::ManagedCoreUnavailable => {
                    formatter.write_str("the requesting user has no running packaged Manis core")
                }
                Self::TunUnavailable => {
                    formatter.write_str("the Manis TUN interface is unavailable")
                }
                Self::CommandFailed(action, code) => write!(
                    formatter,
                    "systemd-resolved could not {action}{}",
                    code.map_or_else(String::new, |code| format!(" (exit code {code})"))
                ),
                Self::Io(error) => write!(formatter, "Linux helper inspection failed: {error}"),
            }
        }
    }

    pub(super) fn main() -> ExitCode {
        let result = (|| {
            let action = Action::parse(env::args().skip(1))?;
            verify_root_and_executable()?;
            if action == Action::Install {
                let caller = policykit_caller()?;
                verify_managed_core_for(caller)?;
                if !tun_interface_path().is_dir() {
                    return Err(HelperError::TunUnavailable);
                }
            }
            run(action)
        })();
        match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("manis-linux-helper: {error}");
                ExitCode::FAILURE
            }
        }
    }

    fn verify_root_and_executable() -> Result<(), HelperError> {
        if std::fs::metadata("/proc/self")
            .map_err(HelperError::Io)?
            .uid()
            != 0
        {
            return Err(HelperError::RequiresRoot);
        }
        let executable = std::fs::canonicalize("/proc/self/exe").map_err(HelperError::Io)?;
        let metadata = executable.metadata().map_err(HelperError::Io)?;
        if !metadata.is_file() || metadata.uid() != 0 || metadata.permissions().mode() & 0o022 != 0
        {
            return Err(HelperError::UnsafeExecutable);
        }
        Ok(())
    }

    fn policykit_caller() -> Result<u32, HelperError> {
        let caller = env::var("PKEXEC_UID")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|uid| *uid != 0)
            .ok_or(HelperError::CallerUnavailable)?;
        Ok(caller)
    }

    fn verify_managed_core_for(caller: u32) -> Result<(), HelperError> {
        let expected = Path::new(MANAGED_CORE);
        let processes = std::fs::read_dir("/proc").map_err(HelperError::Io)?;
        for process in processes.flatten() {
            let Some(pid) = process
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<u32>().ok())
            else {
                continue;
            };
            let root = Path::new("/proc").join(pid.to_string());
            let Ok(metadata) = root.metadata() else {
                continue;
            };
            if metadata.uid() != caller {
                continue;
            }
            if std::fs::read_link(root.join("exe")).is_ok_and(|path| path == expected) {
                return Ok(());
            }
        }
        Err(HelperError::ManagedCoreUnavailable)
    }

    fn run(action: Action) -> Result<(), HelperError> {
        match action {
            Action::Install => {
                run_resolvectl(
                    &["dns", TUN_DEVICE, TUN_DNS_SERVER],
                    "set the TUN DNS server",
                )?;
                if let Err(error) = run_resolvectl(
                    &["domain", TUN_DEVICE, "~."],
                    "route DNS through the TUN interface",
                ) {
                    let _ = revert();
                    return Err(error);
                }
                if let Err(error) = flush_cache() {
                    let _ = revert();
                    return Err(error);
                }
                Ok(())
            }
            Action::Restore => {
                if tun_interface_path().is_dir() {
                    revert()?;
                }
                flush_cache()
            }
        }
    }

    fn revert() -> Result<(), HelperError> {
        run_resolvectl(&["revert", TUN_DEVICE], "restore the original DNS route")
    }

    fn flush_cache() -> Result<(), HelperError> {
        run_resolvectl(&["flush-caches"], "flush the DNS cache")
    }

    fn run_resolvectl(args: &[&str], action: &'static str) -> Result<(), HelperError> {
        let status = Command::new(RESOLVECTL)
            .args(args)
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .status()
            .map_err(HelperError::Io)?;
        status
            .success()
            .then_some(())
            .ok_or_else(|| HelperError::CommandFailed(action, status.code()))
    }

    fn tun_interface_path() -> PathBuf {
        Path::new("/sys/class/net").join(TUN_DEVICE)
    }

    #[cfg(test)]
    mod tests {
        use super::{Action, HelperError};

        #[test]
        fn accepts_only_one_fixed_action() {
            assert_eq!(
                Action::parse(["install".to_owned()].into_iter()).unwrap(),
                Action::Install
            );
            assert_eq!(
                Action::parse(["restore".to_owned()].into_iter()).unwrap(),
                Action::Restore
            );
            assert!(matches!(
                Action::parse([].into_iter()),
                Err(HelperError::InvalidRequest)
            ));
            assert!(matches!(
                Action::parse(["install".to_owned(), "extra".to_owned()].into_iter()),
                Err(HelperError::InvalidRequest)
            ));
        }
    }
}

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    linux::main()
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("manis-linux-helper is available only on Linux");
}
