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
        Io {
            operation: &'static str,
            source: std::io::Error,
        },
        RollbackFailed {
            original: Box<Self>,
            rollback: Box<Self>,
        },
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
                Self::Io { operation, source } => {
                    write!(formatter, "Linux helper could not {operation}: {source}")
                }
                Self::RollbackFailed { original, rollback } => write!(
                    formatter,
                    "{original}; restoring the original DNS route also failed: {rollback}"
                ),
            }
        }
    }

    impl std::error::Error for HelperError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            match self {
                Self::Io { source, .. } => Some(source),
                Self::RollbackFailed { original, .. } => Some(original.as_ref()),
                _ => None,
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
            .map_err(|source| HelperError::Io {
                operation: "inspect the helper process",
                source,
            })?
            .uid()
            != 0
        {
            return Err(HelperError::RequiresRoot);
        }
        let executable =
            std::fs::canonicalize("/proc/self/exe").map_err(|source| HelperError::Io {
                operation: "resolve the helper executable",
                source,
            })?;
        let metadata = executable.metadata().map_err(|source| HelperError::Io {
            operation: "inspect the helper executable",
            source,
        })?;
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
        let processes = std::fs::read_dir("/proc").map_err(|source| HelperError::Io {
            operation: "enumerate running processes",
            source,
        })?;
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
        run_with(action, tun_interface_path().is_dir(), run_resolvectl)
    }

    fn run_with(
        action: Action,
        tun_available: bool,
        mut command: impl FnMut(&[&str], &'static str) -> Result<(), HelperError>,
    ) -> Result<(), HelperError> {
        match action {
            Action::Install => {
                command(
                    &["dns", TUN_DEVICE, TUN_DNS_SERVER],
                    "set the TUN DNS server",
                )?;
                if let Err(error) = command(
                    &["domain", TUN_DEVICE, "~."],
                    "route DNS through the TUN interface",
                ) {
                    return Err(with_rollback(
                        error,
                        command(&["revert", TUN_DEVICE], "restore the original DNS route"),
                    ));
                }
                if let Err(error) = command(&["flush-caches"], "flush the DNS cache") {
                    return Err(with_rollback(
                        error,
                        command(&["revert", TUN_DEVICE], "restore the original DNS route"),
                    ));
                }
                Ok(())
            }
            Action::Restore => {
                if tun_available {
                    command(&["revert", TUN_DEVICE], "restore the original DNS route")?;
                }
                command(&["flush-caches"], "flush the DNS cache")
            }
        }
    }

    fn with_rollback(original: HelperError, rollback: Result<(), HelperError>) -> HelperError {
        match rollback {
            Ok(()) => original,
            Err(rollback) => HelperError::RollbackFailed {
                original: Box::new(original),
                rollback: Box::new(rollback),
            },
        }
    }

    fn run_resolvectl(args: &[&str], action: &'static str) -> Result<(), HelperError> {
        let status = Command::new(RESOLVECTL)
            .args(args)
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .status()
            .map_err(|source| HelperError::Io {
                operation: "start resolvectl",
                source,
            })?;
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
        use super::{Action, HelperError, run_with};

        fn run_script(
            action: Action,
            tun_available: bool,
            failed_calls: &[usize],
        ) -> (Result<(), HelperError>, Vec<String>) {
            let mut calls = Vec::new();
            let mut index = 0;
            let result = run_with(action, tun_available, |args, description| {
                let current = index;
                index += 1;
                calls.push(args.join(" "));
                if failed_calls.contains(&current) {
                    Err(HelperError::CommandFailed(
                        description,
                        Some(
                            i32::try_from(current)
                                .unwrap_or(i32::MAX)
                                .saturating_add(10),
                        ),
                    ))
                } else {
                    Ok(())
                }
            });
            (result, calls)
        }

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

        #[test]
        fn install_runs_each_resolved_step_in_order() {
            let (result, calls) = run_script(Action::Install, true, &[]);

            assert!(result.is_ok());
            assert_eq!(
                calls,
                ["dns Meta 198.18.0.2", "domain Meta ~.", "flush-caches"]
            );
        }

        #[test]
        fn install_rolls_back_a_domain_failure() {
            let (result, calls) = run_script(Action::Install, true, &[1]);

            assert!(matches!(
                result,
                Err(HelperError::CommandFailed(
                    "route DNS through the TUN interface",
                    Some(11)
                ))
            ));
            assert_eq!(
                calls,
                ["dns Meta 198.18.0.2", "domain Meta ~.", "revert Meta"]
            );
        }

        #[test]
        fn install_rolls_back_a_cache_flush_failure() {
            let (result, calls) = run_script(Action::Install, true, &[2]);

            assert!(matches!(
                result,
                Err(HelperError::CommandFailed("flush the DNS cache", Some(12)))
            ));
            assert_eq!(
                calls,
                [
                    "dns Meta 198.18.0.2",
                    "domain Meta ~.",
                    "flush-caches",
                    "revert Meta"
                ]
            );
        }

        #[test]
        fn install_reports_both_operation_and_rollback_failures() {
            let (result, calls) = run_script(Action::Install, true, &[1, 2]);
            let Err(HelperError::RollbackFailed { original, rollback }) = result else {
                panic!("operation and rollback failures must both be retained");
            };

            assert!(matches!(
                *original,
                HelperError::CommandFailed("route DNS through the TUN interface", Some(11))
            ));
            assert!(matches!(
                *rollback,
                HelperError::CommandFailed("restore the original DNS route", Some(12))
            ));
            assert_eq!(
                calls,
                ["dns Meta 198.18.0.2", "domain Meta ~.", "revert Meta"]
            );
        }

        #[test]
        fn restore_reverts_only_when_the_tun_interface_exists() {
            let (without_tun, without_tun_calls) = run_script(Action::Restore, false, &[]);
            let (with_tun, with_tun_calls) = run_script(Action::Restore, true, &[]);

            assert!(without_tun.is_ok());
            assert_eq!(without_tun_calls, ["flush-caches"]);
            assert!(with_tun.is_ok());
            assert_eq!(with_tun_calls, ["revert Meta", "flush-caches"]);
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
