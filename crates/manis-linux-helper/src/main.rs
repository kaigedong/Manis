#[cfg(target_os = "linux")]
mod linux {
    use std::env;
    use std::fmt;
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
    use std::path::{Path, PathBuf};
    use std::process::{Command, ExitCode, Stdio};

    const MANAGED_CORE: &str = "/usr/lib/manis/mihomo";
    const RESOLVECTL: &str = "/usr/bin/resolvectl";
    const IP_CANDIDATES: [&str; 4] = ["/usr/sbin/ip", "/sbin/ip", "/usr/bin/ip", "/bin/ip"];
    const TUN_DEVICE: &str = "Meta";
    const TUN_DNS_SERVER: &str = "198.18.0.2";
    /// Tailscale's `MagicDNS` resolver. Mihomo resolves DNS through its own routing table so that
    /// `dns-hijack` cannot be bypassed, which leaves this address without a route to `tailscale0`
    /// and turns `MagicDNS` into fake-IP answers. The main-table lookup below keeps the query on the
    /// Tailscale interface when that route exists, and falls through to the TUN default when it
    /// does not, so it stays correct regardless of which program starts first.
    const TAILSCALE_DNS_ADDRESS: &str = "100.100.100.100/32";
    const POLICY_RULE_PREF: &str = "8000";

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

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum RuleAction {
        Add,
        Delete,
    }

    impl RuleAction {
        const fn argument(self) -> &'static str {
            match self {
                Self::Add => "add",
                Self::Delete => "del",
            }
        }

        const fn description(self) -> &'static str {
            match self {
                Self::Add => "install the Tailscale DNS policy rule",
                Self::Delete => "remove the Tailscale DNS policy rule",
            }
        }

        fn args(self) -> [&'static str; 8] {
            [
                "rule",
                self.argument(),
                "pref",
                POLICY_RULE_PREF,
                "to",
                TAILSCALE_DNS_ADDRESS,
                "lookup",
                "main",
            ]
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
                    "the Linux helper could not {action}{}",
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
        run_with(
            action,
            tun_interface_path().is_dir(),
            run_resolvectl,
            run_policy_rule,
        )
    }

    fn run_with(
        action: Action,
        tun_available: bool,
        mut resolvectl: impl FnMut(&[&str], &'static str) -> Result<(), HelperError>,
        mut policy_rule: impl FnMut(RuleAction) -> Result<(), HelperError>,
    ) -> Result<(), HelperError> {
        match action {
            Action::Install => {
                // Installed before DNS so a failing `ip` never leaves the resolver pointed at a
                // TUN without the Tailscale exception. Without a Tailscale route the main-table
                // lookup finds nothing and routing falls through to the TUN default, so this is a
                // no-op for users who never run Tailscale.
                policy_rule(RuleAction::Add)?;
                resolvectl(
                    &["dns", TUN_DEVICE, TUN_DNS_SERVER],
                    "set the TUN DNS server",
                )?;
                if let Err(error) = resolvectl(
                    &["domain", TUN_DEVICE, "~."],
                    "route DNS through the TUN interface",
                ) {
                    return Err(with_rollback(
                        error,
                        rollback_install(&mut resolvectl, &mut policy_rule),
                    ));
                }
                if let Err(error) = resolvectl(&["flush-caches"], "flush the DNS cache") {
                    return Err(with_rollback(
                        error,
                        rollback_install(&mut resolvectl, &mut policy_rule),
                    ));
                }
                Ok(())
            }
            Action::Restore => {
                // Best effort: a leftover rule only routes Tailscale's own resolver to the real
                // interface, so it must never block restoring the system resolver.
                let _ = policy_rule(RuleAction::Delete);
                if tun_available {
                    resolvectl(&["revert", TUN_DEVICE], "restore the original DNS route")?;
                }
                resolvectl(&["flush-caches"], "flush the DNS cache")
            }
        }
    }

    fn rollback_install(
        resolvectl: &mut impl FnMut(&[&str], &'static str) -> Result<(), HelperError>,
        policy_rule: &mut impl FnMut(RuleAction) -> Result<(), HelperError>,
    ) -> Result<(), HelperError> {
        let dns = resolvectl(&["revert", TUN_DEVICE], "restore the original DNS route");
        let rule = policy_rule(RuleAction::Delete);
        match (dns, rule) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), _) | (_, Err(error)) => Err(error),
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

    /// Installs or removes the Tailscale resolver exception with the system `ip` binary.
    ///
    /// `iproute2` is optional for users who never run Tailscale; when it is absent the rule is
    /// simply skipped so that TUN DNS keeps working. Duplicate adds and missing deletes are
    /// treated as success so repeated activation and stale recovery stay idempotent.
    fn run_policy_rule(action: RuleAction) -> Result<(), HelperError> {
        let Some(binary) = ip_binary() else {
            return Ok(());
        };
        let output = Command::new(binary)
            .args(action.args())
            .env_clear()
            .stdin(Stdio::null())
            .output()
            .map_err(|source| HelperError::Io {
                operation: "start ip",
                source,
            })?;
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        if is_benign_rule_failure(action, &stderr) {
            return Ok(());
        }
        Err(HelperError::CommandFailed(
            action.description(),
            output.status.code(),
        ))
    }

    fn ip_binary() -> Option<&'static str> {
        IP_CANDIDATES
            .into_iter()
            .find(|path| Path::new(path).is_file())
    }

    fn is_benign_rule_failure(action: RuleAction, stderr: &str) -> bool {
        match action {
            RuleAction::Add => stderr.contains("File exists"),
            RuleAction::Delete => {
                stderr.contains("No such file or directory") || stderr.contains("No such process")
            }
        }
    }

    fn tun_interface_path() -> PathBuf {
        Path::new("/sys/class/net").join(TUN_DEVICE)
    }

    #[cfg(test)]
    mod tests {
        use super::{Action, HelperError, RuleAction, is_benign_rule_failure, run_with};

        fn run_script(
            action: Action,
            tun_available: bool,
            failed_calls: &[usize],
            failed_rule_calls: &[usize],
        ) -> (Result<(), HelperError>, Vec<String>) {
            let calls = std::cell::RefCell::new(Vec::new());
            let mut index = 0;
            let mut rule_index = 0;
            let result = run_with(
                action,
                tun_available,
                |args, description| {
                    let current = index;
                    index += 1;
                    calls.borrow_mut().push(args.join(" "));
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
                },
                |rule| {
                    let current = rule_index;
                    rule_index += 1;
                    calls.borrow_mut().push(
                        if rule == RuleAction::Add {
                            "rule add"
                        } else {
                            "rule del"
                        }
                        .to_owned(),
                    );
                    if failed_rule_calls.contains(&current) {
                        Err(HelperError::CommandFailed(
                            rule.description(),
                            Some(
                                i32::try_from(current)
                                    .unwrap_or(i32::MAX)
                                    .saturating_add(20),
                            ),
                        ))
                    } else {
                        Ok(())
                    }
                },
            );
            (result, calls.into_inner())
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
        fn install_installs_the_tailscale_rule_before_touching_dns() {
            let (result, calls) = run_script(Action::Install, true, &[], &[]);

            assert!(result.is_ok());
            assert_eq!(
                calls,
                [
                    "rule add",
                    "dns Meta 198.18.0.2",
                    "domain Meta ~.",
                    "flush-caches"
                ]
            );
        }

        #[test]
        fn install_fails_before_dns_when_the_tailscale_rule_fails() {
            let (result, calls) = run_script(Action::Install, true, &[], &[0]);

            assert!(matches!(
                result,
                Err(HelperError::CommandFailed(
                    "install the Tailscale DNS policy rule",
                    Some(20)
                ))
            ));
            assert_eq!(calls, ["rule add"]);
        }

        #[test]
        fn install_rolls_back_a_domain_failure() {
            let (result, calls) = run_script(Action::Install, true, &[1], &[]);

            assert!(matches!(
                result,
                Err(HelperError::CommandFailed(
                    "route DNS through the TUN interface",
                    Some(11)
                ))
            ));
            assert_eq!(
                calls,
                [
                    "rule add",
                    "dns Meta 198.18.0.2",
                    "domain Meta ~.",
                    "revert Meta",
                    "rule del"
                ]
            );
        }

        #[test]
        fn install_rolls_back_a_cache_flush_failure() {
            let (result, calls) = run_script(Action::Install, true, &[2], &[]);

            assert!(matches!(
                result,
                Err(HelperError::CommandFailed("flush the DNS cache", Some(12)))
            ));
            assert_eq!(
                calls,
                [
                    "rule add",
                    "dns Meta 198.18.0.2",
                    "domain Meta ~.",
                    "flush-caches",
                    "revert Meta",
                    "rule del"
                ]
            );
        }

        #[test]
        fn install_reports_both_operation_and_rollback_failures() {
            let (result, calls) = run_script(Action::Install, true, &[1, 2], &[]);
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
                [
                    "rule add",
                    "dns Meta 198.18.0.2",
                    "domain Meta ~.",
                    "revert Meta",
                    "rule del"
                ]
            );
        }

        #[test]
        fn restore_removes_the_rule_and_reverts_only_when_the_tun_interface_exists() {
            let (without_tun, without_tun_calls) = run_script(Action::Restore, false, &[], &[]);
            let (with_tun, with_tun_calls) = run_script(Action::Restore, true, &[], &[]);

            assert!(without_tun.is_ok());
            assert_eq!(without_tun_calls, ["rule del", "flush-caches"]);
            assert!(with_tun.is_ok());
            assert_eq!(with_tun_calls, ["rule del", "revert Meta", "flush-caches"]);
        }

        #[test]
        fn restore_ignores_a_rule_removal_failure() {
            let (result, calls) = run_script(Action::Restore, true, &[], &[0]);

            assert!(result.is_ok());
            assert_eq!(calls, ["rule del", "revert Meta", "flush-caches"]);
        }

        #[test]
        fn rule_failures_that_only_mean_already_applied_are_benign() {
            assert!(is_benign_rule_failure(
                RuleAction::Add,
                "RTNETLINK answers: File exists\n"
            ));
            assert!(is_benign_rule_failure(
                RuleAction::Delete,
                "RTNETLINK answers: No such file or directory\n"
            ));
            assert!(!is_benign_rule_failure(
                RuleAction::Add,
                "RTNETLINK answers: Operation not permitted\n"
            ));
            assert!(!is_benign_rule_failure(
                RuleAction::Delete,
                "RTNETLINK answers: Operation not permitted\n"
            ));
        }

        #[test]
        fn tailscale_rule_has_a_stable_selector() {
            assert_eq!(
                RuleAction::Add.args(),
                [
                    "rule",
                    "add",
                    "pref",
                    "8000",
                    "to",
                    "100.100.100.100/32",
                    "lookup",
                    "main"
                ]
            );
            assert_eq!(
                RuleAction::Delete.args(),
                [
                    "rule",
                    "del",
                    "pref",
                    "8000",
                    "to",
                    "100.100.100.100/32",
                    "lookup",
                    "main"
                ]
            );
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
