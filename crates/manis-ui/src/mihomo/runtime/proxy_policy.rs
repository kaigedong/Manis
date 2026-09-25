use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

use manis_core::{KernelKind, RoutingMode};
use manis_engine::EngineManager;

use crate::diagnostics::{LogLevel, record_event};

use super::super::{
    ControllerRuntime, GENERATED_PROFILE_FILE, LoadError, ManagedEngineConfig,
    ManagedGeneratedProfile, ManagedPolicyRuntimeSnapshot, PolicyGroupBenchmarkSnapshot,
    ProxyDelayTarget, compile_managed_generated_profile, fetch_group_delay, fetch_policy_group,
    fetch_proxy_delay_targets_bounded_with_progress, generated_engine_manager,
    reload_mihomo_config, render_generated_profile, render_generated_profile_with_tun,
    running_managed_endpoint, select_global_node_at_endpoint, select_policy_group_candidate,
    set_routing_mode, validate_managed_config, write_private_atomic,
};
use super::MANAGED_KERNEL_LOCK_POISONED;

#[cfg(target_os = "android")]
struct AndroidVpnRollback(bool);

#[cfg(target_os = "android")]
impl Drop for AndroidVpnRollback {
    fn drop(&mut self) {
        if self.0 {
            let _ = manis_android_bridge::stop_vpn_service();
        }
    }
}

impl ControllerRuntime {
    pub(crate) fn set_tun_enabled(&self, enabled: bool) -> Result<(), LoadError> {
        #[cfg(target_os = "android")]
        {
            return self.set_android_tun_enabled(enabled);
        }
        #[cfg(not(target_os = "android"))]
        {
            record_event(
                LogLevel::Info,
                "controller.tun.requested",
                format!(
                    "enabled={enabled} ownership={}",
                    if matches!(self, Self::Managed { .. }) {
                        "managed"
                    } else if self.is_fixture() {
                        "fixture"
                    } else {
                        "invalid"
                    }
                ),
            );
            let (manager, spec) = self.managed_mihomo_tun_parts()?;
            let profile = compile_managed_generated_profile(spec)?;
            let payload = render_generated_profile_with_tun(spec, &profile, enabled)?;
            if enabled {
                self.prepare_tun_activation()?;
            }
            let controller_secret = self.controller_secret();
            let endpoint = running_managed_endpoint(manager)?;
            record_event(
                LogLevel::Info,
                "controller.tun.config_reload.requested",
                format!(
                    "enabled={enabled} method=PUT endpoint=/configs?force=true bytes={}",
                    payload.len()
                ),
            );
            let result = reload_mihomo_config(&endpoint, &payload, enabled, controller_secret)
                .map_err(LoadError::from);
            if result.is_ok() {
                record_event(
                    LogLevel::Info,
                    "controller.tun.config_reload.succeeded",
                    format!("enabled={enabled} rebuild=general,dns,listeners,tun,providers"),
                );
            }
            #[cfg(target_os = "macos")]
            let result = match result {
                Ok(()) if !enabled => self.release_macos_tun_route(),
                result => result,
            };
            #[cfg(target_os = "macos")]
            if enabled && result.is_ok() {
                match crate::macos_privileged::existing_tun_route() {
                    Ok(Some(route)) => {
                        record_event(LogLevel::Info, "controller.tun.route_confirmed", route);
                    }
                    Ok(None) => record_event(
                        LogLevel::Warn,
                        "controller.tun.route_missing",
                        "Mihomo accepted TUN enable but the expected split-default route was not found",
                    ),
                    Err(error) => record_event(
                        LogLevel::Warn,
                        "controller.tun.route_probe_failed",
                        error.to_string(),
                    ),
                }
            }
            match &result {
                Ok(()) => record_event(
                    LogLevel::Info,
                    "controller.tun.succeeded",
                    format!("enabled={enabled} endpoint={endpoint}"),
                ),
                Err(error) => record_event(
                    LogLevel::Error,
                    "controller.tun.failed",
                    format!("enabled={enabled} error={error}"),
                ),
            }
            result
        }
    }

    #[cfg(target_os = "android")]
    fn set_android_tun_enabled(&self, enabled: bool) -> Result<(), LoadError> {
        use std::os::fd::OwnedFd;

        let Self::Managed {
            manager,
            apply_lock,
            generated_profile: Some(spec),
            ..
        } = self
        else {
            return Err(LoadError::Runtime(
                "Android VPN mode requires a Manis-managed Mihomo profile".to_owned(),
            ));
        };
        let _apply = apply_lock
            .lock()
            .map_err(|_| LoadError::Runtime("managed profile lock was damaged".to_owned()))?;
        let profile = compile_managed_generated_profile(spec)?;
        let tun_fd: Option<OwnedFd> = if enabled {
            Some(manis_android_bridge::request_vpn_fd().map_err(LoadError::Runtime)?)
        } else {
            None
        };
        let mut rollback_vpn_service = AndroidVpnRollback(enabled);
        let payload = match tun_fd.as_ref() {
            Some(_fd) => super::super::render_mihomo_yaml_with_tun_fd(&profile, 3)
                .map_err(|error| LoadError::Runtime(error.to_string()))?,
            None => render_generated_profile(spec, &profile)?,
        };
        let config_file =
            write_private_atomic(&spec.data_dir, GENERATED_PROFILE_FILE, payload.as_bytes())
                .map_err(|_error| {
                    LoadError::Runtime(
                        "private Mihomo configuration could not be written".to_owned(),
                    )
                })?;
        let config = ManagedEngineConfig::new(
            spec.binary.clone(),
            config_file,
            spec.data_dir.clone(),
            spec.controller.clone(),
        );
        let config = if let Some(fd) = tun_fd {
            config.with_inherited_fd(fd)
        } else {
            config
        };
        validate_managed_config(&config).map_err(|error| LoadError::Runtime(error.to_string()))?;
        let replacement = generated_engine_manager(spec, config, false)?;
        {
            let mut current = manager
                .lock()
                .map_err(|_| LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned()))?;
            current
                .stop()
                .map_err(|error| LoadError::Runtime(error.to_string()))?;
            *current = replacement;
            if !enabled {
                // Dropping the old manager above releases its duplicate of the detached VPN fd.
                manis_android_bridge::stop_vpn_service().map_err(LoadError::Runtime)?;
            } else {
                let revoked_runtime = self.clone();
                manis_android_bridge::set_vpn_revocation_handler(move || {
                    if let Err(error) = revoked_runtime.set_tun_enabled(false) {
                        record_event(
                            LogLevel::Error,
                            "controller.tun.revocation_recovery_failed",
                            error.to_string(),
                        );
                    } else {
                        record_event(
                            LogLevel::Warn,
                            "controller.tun.revoked",
                            "Android revoked the VPN interface; Mihomo returned to local proxy mode",
                        );
                    }
                });
            }
            if let Err(error) = current.start() {
                return Err(LoadError::Runtime(error.to_string()));
            }
        }
        rollback_vpn_service.0 = false;
        Ok(())
    }

    fn prepare_tun_activation(&self) -> Result<(), LoadError> {
        #[cfg(target_os = "macos")]
        {
            if let Some(conflict) =
                crate::macos_privileged::existing_tun_route().map_err(|error| {
                    LoadError::Runtime(format!("macOS TUN routes could not be inspected: {error}"))
                })?
            {
                record_event(LogLevel::Error, "controller.tun.conflict", conflict.clone());
                return Err(LoadError::Runtime(format!(
                    "another TUN is using the system proxy route ({conflict}); turn off TUN mode in other proxy applications first"
                )));
            }
            if let Err(error) = self.ensure_privileged_mihomo() {
                record_event(
                    LogLevel::Error,
                    "controller.tun.failed",
                    format!("enabled=true phase=privilege_promotion error={error}"),
                );
                return Err(error);
            }
            record_event(
                LogLevel::Info,
                "controller.tun.interface_selection",
                "strategy=auto-detect-interface",
            );
        }
        #[cfg(target_os = "linux")]
        if let Err(error) = self.ensure_linux_tun_capabilities() {
            record_event(
                LogLevel::Error,
                "controller.tun.failed",
                format!("enabled=true phase=capability_promotion error={error}"),
            );
            return Err(error);
        }
        Ok(())
    }

    fn managed_mihomo_tun_parts(
        &self,
    ) -> Result<(&Arc<Mutex<EngineManager>>, &ManagedGeneratedProfile), LoadError> {
        let Self::Managed {
            manager,
            generated_profile: Some(spec),
            ..
        } = self
        else {
            return Err(LoadError::Runtime(match self {
                #[cfg(any(test, feature = "snapshot-fixtures"))]
                Self::Fixture { .. } => "test snapshots cannot enable TUN mode".to_owned(),
                Self::Invalid { message } => message.clone(),
                Self::Managed { .. } => {
                    "TUN mode requires a Mihomo configuration generated from saved Manis sources"
                        .to_owned()
                }
            }));
        };
        if spec.kernel != KernelKind::Mihomo {
            return Err(LoadError::Runtime(
                "the TUN configuration reload path supports only Mihomo".to_owned(),
            ));
        }
        Ok((manager, spec))
    }

    pub(crate) fn set_routing_mode(&self, mode: RoutingMode) -> Result<(), LoadError> {
        record_event(
            LogLevel::Info,
            "controller.routing.requested",
            format!("mode={mode:?}"),
        );
        let controller_secret = self.controller_secret();
        let endpoint = match self {
            Self::Managed { manager, .. } => {
                let mut manager = manager.lock().map_err(|_poisoned| {
                    LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned())
                })?;
                manager
                    .running_endpoint()?
                    .ok_or_else(|| {
                        LoadError::Runtime(
                            "Mihomo is not running; connect the kernel first".to_owned(),
                        )
                    })?
                    .uri()
            }
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::Fixture { .. } => {
                return Err(LoadError::Runtime(
                    "test snapshots cannot change routing mode".to_owned(),
                ));
            }
            Self::Invalid { message } => return Err(LoadError::Runtime(message.clone())),
        };
        let result = set_routing_mode(&endpoint, mode, controller_secret);
        match &result {
            Ok(()) => record_event(
                LogLevel::Info,
                "controller.routing.succeeded",
                format!("mode={mode:?} endpoint={endpoint}"),
            ),
            Err(error) => record_event(
                LogLevel::Error,
                "controller.routing.failed",
                format!("mode={mode:?} error={error}"),
            ),
        }
        result.map_err(LoadError::from)
    }

    pub(crate) fn select_global_node(
        &self,
        selected_name: &str,
    ) -> Result<ManagedPolicyRuntimeSnapshot, LoadError> {
        let controller_secret = self.controller_secret();
        let Self::Managed {
            manager,
            generated_profile: Some(_),
            ..
        } = self
        else {
            return Err(LoadError::Runtime(
                "the active controller is not managed by Manis; its global exit cannot be changed"
                    .to_owned(),
            ));
        };
        let endpoint = {
            let mut manager = manager
                .lock()
                .map_err(|_poisoned| LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned()))?;
            manager
                .running_endpoint()?
                .ok_or_else(|| {
                    LoadError::Runtime(
                        "Mihomo is not running; start the managed kernel first".to_owned(),
                    )
                })?
                .uri()
        };
        select_global_node_at_endpoint(&endpoint, selected_name, controller_secret)
    }

    pub(crate) fn test_proxy_delay_targets_with_progress(
        &self,
        targets: &[ProxyDelayTarget],
        on_result: impl FnMut(&str, Option<u16>),
    ) -> Result<BTreeMap<String, u16>, LoadError> {
        if targets.is_empty() {
            return Err(LoadError::Runtime(
                "the current group has no nodes that can be benchmarked".to_owned(),
            ));
        }
        let controller_secret = self.controller_secret();
        let endpoint = match self {
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::Fixture { endpoint } => endpoint.clone(),
            Self::Managed { manager, .. } => {
                let endpoint = {
                    let mut manager = manager.lock().map_err(|_poisoned| {
                        LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned())
                    })?;
                    match manager.running_endpoint()? {
                        Some(endpoint) => endpoint,
                        None => manager.start()?,
                    }
                };
                endpoint.uri()
            }
            Self::Invalid { message } => return Err(LoadError::Runtime(message.clone())),
        };

        fetch_proxy_delay_targets_bounded_with_progress(
            &endpoint,
            targets,
            controller_secret,
            on_result,
        )
    }

    pub(crate) fn test_policy_group_delay(
        &self,
        group_name: &str,
        targets: &[ProxyDelayTarget],
    ) -> Result<PolicyGroupBenchmarkSnapshot, LoadError> {
        let controller_secret = self.controller_secret();
        let endpoint = match self {
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::Fixture { endpoint } => endpoint.clone(),
            Self::Managed { manager, .. } => {
                let endpoint = {
                    let mut manager = manager.lock().map_err(|_poisoned| {
                        LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned())
                    })?;
                    match manager.running_endpoint()? {
                        Some(endpoint) => endpoint,
                        None => manager.start()?,
                    }
                };
                endpoint.uri()
            }
            Self::Invalid { message } => return Err(LoadError::Runtime(message.clone())),
        };
        let candidates = targets
            .iter()
            .map(ProxyDelayTarget::name)
            .collect::<BTreeSet<_>>();
        let delays = match fetch_group_delay(&endpoint, group_name, controller_secret) {
            Ok(delays) => delays
                .into_iter()
                .filter(|(name, delay)| candidates.contains(name.as_str()) && *delay > 0)
                .collect::<BTreeMap<_, _>>(),
            Err(error) => {
                record_event(
                    LogLevel::Warn,
                    "group.delay.fallback",
                    format!("group={group_name} error={error}"),
                );
                BTreeMap::new()
            }
        };
        let delays = if delays.is_empty() {
            fetch_proxy_delay_targets_bounded_with_progress(
                &endpoint,
                targets,
                controller_secret,
                |_name, _delay| {},
            )?
        } else {
            delays
        };
        let current = fetch_policy_group(&endpoint, group_name, controller_secret)?.current;
        Ok(PolicyGroupBenchmarkSnapshot { delays, current })
    }

    pub(crate) fn select_policy_candidate(
        &self,
        group_name: &str,
        selected_name: &str,
    ) -> Result<ManagedPolicyRuntimeSnapshot, LoadError> {
        let controller_secret = self.controller_secret();
        let Self::Managed {
            manager,
            generated_profile: Some(_),
            ..
        } = self
        else {
            return Err(LoadError::Runtime(
                "the active controller is not managed by Manis; its policy groups cannot be changed"
                    .to_owned(),
            ));
        };
        let endpoint = {
            let mut manager = manager
                .lock()
                .map_err(|_poisoned| LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned()))?;
            manager
                .running_endpoint()?
                .ok_or_else(|| {
                    LoadError::Runtime(
                        "Mihomo is not running; start the managed kernel first".to_owned(),
                    )
                })?
                .uri()
        };
        select_policy_group_candidate(&endpoint, group_name, selected_name, controller_secret)
    }
}
