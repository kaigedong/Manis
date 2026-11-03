use super::{
    Arc, KernelRuntime, Language, LogLevel, Mutex, ProxyMode, ProxyPorts, SystemProxySession,
    TunDnsSession, copy, record_event,
};

pub(super) fn apply_proxy_mode_transition(
    runtime: &KernelRuntime,
    system_proxy: &Arc<Mutex<SystemProxySession>>,
    tun_dns: &Arc<Mutex<TunDnsSession>>,
    previous: ProxyMode,
    requested: ProxyMode,
    ports: ProxyPorts,
    language: Language,
) -> Result<(), String> {
    let mut system = system_proxy.lock().map_err(|_| {
        language
            .localized(copy::app::SYSTEM_PROXY_STATE_LOCK_WAS_DAMAGED)
            .to_owned()
    })?;
    let mut dns = tun_dns.lock().map_err(|_| {
        language
            .localized(copy::app::TUN_DNS_STATE_LOCK_WAS_DAMAGED)
            .to_owned()
    })?;
    match (previous, requested) {
        (ProxyMode::System, ProxyMode::Off) => system
            .disable_with_language(language)
            .map_err(|error| error.to_string()),
        (ProxyMode::Tun, ProxyMode::Off) => disable_tun_with_dns(runtime, &mut dns, language),
        (ProxyMode::Off, ProxyMode::System) => system
            .enable_with_language(ports, language)
            .map_err(|error| error.to_string()),
        (ProxyMode::Tun, ProxyMode::System) => {
            disable_tun_with_dns(runtime, &mut dns, language)?;
            if let Err(error) = system.enable_with_language(ports, language) {
                let rollback = enable_tun_with_dns(runtime, &mut dns, language);
                return Err(match rollback {
                    Ok(()) => error.to_string(),
                    Err(rollback) => {
                        copy::app::tun_mode_rollback_failed(language, &error.to_string(), &rollback)
                    }
                });
            }
            Ok(())
        }
        (ProxyMode::Off, ProxyMode::Tun) => enable_tun_with_dns(runtime, &mut dns, language),
        (ProxyMode::System, ProxyMode::Tun) => {
            system
                .disable_with_language(language)
                .map_err(|error| error.to_string())?;
            if let Err(error) = enable_tun_with_dns(runtime, &mut dns, language) {
                let rollback = system.enable_with_language(ports, language);
                return Err(match rollback {
                    Ok(()) => error,
                    Err(rollback) => copy::app::system_proxy_rollback_failed(
                        language,
                        &error,
                        &rollback.to_string(),
                    ),
                });
            }
            Ok(())
        }
        (ProxyMode::Off, ProxyMode::Off)
        | (ProxyMode::System, ProxyMode::System)
        | (ProxyMode::Tun, ProxyMode::Tun) => Ok(()),
    }
}

fn enable_tun_with_dns(
    runtime: &KernelRuntime,
    dns: &mut TunDnsSession,
    language: Language,
) -> Result<(), String> {
    let log = tun_dns_log_details();
    record_event(
        LogLevel::Info,
        "controller.tun.dns.requested",
        log.prepare_requested,
    );
    dns.prepare_with_language(language).map_err(|error| {
        record_event(
            LogLevel::Error,
            "controller.tun.dns.failed",
            format!("action=prepare error={error}"),
        );
        error.to_string()
    })?;
    record_event(
        LogLevel::Info,
        "controller.tun.dns.succeeded",
        log.prepare_succeeded,
    );
    if let Err(error) = runtime.set_tun_enabled(true) {
        let rollback = dns.disable_with_language(language);
        return Err(match rollback {
            Ok(()) => error.to_string(),
            Err(rollback) => {
                copy::app::dns_rollback_failed(language, &error.to_string(), &rollback.to_string())
            }
        });
    }

    record_event(
        LogLevel::Info,
        "controller.tun.dns.requested",
        log.install_requested,
    );
    if let Err(error) = dns.activate_with_language(language) {
        let dns_rollback = dns.disable_with_language(language);
        let tun_rollback = runtime.set_tun_enabled(false);
        record_event(
            LogLevel::Error,
            "controller.tun.dns.failed",
            format!("action=install error={error}"),
        );
        return Err(match (dns_rollback, tun_rollback) {
            (Ok(()), Ok(())) => error.to_string(),
            (Err(dns_rollback), Ok(())) => copy::app::dns_rollback_failed(
                language,
                &error.to_string(),
                &dns_rollback.to_string(),
            ),
            (Ok(()), Err(tun_rollback)) => copy::app::tun_shutdown_rollback_failed(
                language,
                &error.to_string(),
                &tun_rollback.to_string(),
            ),
            (Err(dns_rollback), Err(tun_rollback)) => copy::app::dns_and_tun_rollback_failed(
                language,
                &error.to_string(),
                &dns_rollback.to_string(),
                &tun_rollback.to_string(),
            ),
        });
    }
    record_event(
        LogLevel::Info,
        "controller.tun.dns.succeeded",
        log.install_succeeded,
    );
    Ok(())
}

fn disable_tun_with_dns(
    runtime: &KernelRuntime,
    dns: &mut TunDnsSession,
    language: Language,
) -> Result<(), String> {
    record_event(
        LogLevel::Info,
        "controller.tun.dns.requested",
        tun_dns_log_details().restore_requested,
    );
    dns.disable_with_language(language).map_or_else(
        |error| {
            record_event(
                LogLevel::Warn,
                "controller.tun.dns.restore_deferred",
                format!("error={error}"),
            );
            Err(format!(
                "{}: {error}",
                language.localized(
                    copy::app::TUN_IS_DISABLED_BUT_RESTORING_THE_ORIGINAL_DNS_FAILED_RECOVERY
                )
            ))
        },
        |()| {
            record_event(
                LogLevel::Info,
                "controller.tun.dns.succeeded",
                tun_dns_log_details().restore_succeeded,
            );
            Ok(())
        },
    )?;

    if let Err(error) = runtime.set_tun_enabled(false) {
        let dns_rollback = dns
            .prepare_with_language(language)
            .and_then(|()| dns.activate_with_language(language));
        return Err(match dns_rollback {
            Ok(()) => error.to_string(),
            Err(rollback) => copy::app::dns_reactivation_failed(
                language,
                &error.to_string(),
                &rollback.to_string(),
            ),
        });
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub(super) struct TunDnsLogDetails {
    pub(super) prepare_requested: &'static str,
    pub(super) prepare_succeeded: &'static str,
    pub(super) install_requested: &'static str,
    pub(super) install_succeeded: &'static str,
    pub(super) restore_requested: &'static str,
    pub(super) restore_succeeded: &'static str,
}

#[cfg(target_os = "macos")]
pub(super) const fn tun_dns_log_details() -> TunDnsLogDetails {
    TunDnsLogDetails {
        prepare_requested: "action=prepare strategy=system_resolver resolver=114.114.114.114",
        prepare_succeeded: "action=prepare strategy=system_resolver recovery=saved",
        install_requested: "action=install strategy=system_resolver resolver=114.114.114.114",
        install_succeeded: "action=install strategy=system_resolver recovery=retained",
        restore_requested: "action=restore strategy=system_resolver",
        restore_succeeded: "action=restore strategy=system_resolver recovery=removed",
    }
}

#[cfg(target_os = "linux")]
pub(super) const fn tun_dns_log_details() -> TunDnsLogDetails {
    TunDnsLogDetails {
        prepare_requested: "action=prepare strategy=systemd_resolved recovery=pending",
        prepare_succeeded: "action=prepare strategy=systemd_resolved recovery=saved",
        install_requested: "action=install strategy=systemd_resolved link=Meta resolver=198.18.0.2 domain=~.",
        install_succeeded: "action=install strategy=systemd_resolved cache=flushed",
        restore_requested: "action=restore strategy=systemd_resolved link=Meta",
        restore_succeeded: "action=restore strategy=systemd_resolved recovery=removed cache=flushed",
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(super) const fn tun_dns_log_details() -> TunDnsLogDetails {
    TunDnsLogDetails {
        prepare_requested: "action=prepare strategy=kernel_managed",
        prepare_succeeded: "action=prepare strategy=kernel_managed",
        install_requested: "action=install strategy=kernel_managed",
        install_succeeded: "action=install strategy=kernel_managed",
        restore_requested: "action=restore strategy=kernel_managed",
        restore_succeeded: "action=restore strategy=kernel_managed",
    }
}
