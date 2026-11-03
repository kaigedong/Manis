use std::sync::atomic::Ordering;

use manis_core::KernelKind;
use manis_engine::{EngineManager, ReadinessPolicy, validate_managed_config};

use crate::diagnostics::{LogLevel, record_event};

#[cfg(target_os = "macos")]
use super::super::ManagedGeneratedProfile;
use super::super::{
    ControllerRuntime, GENERATED_PROFILE_FILE, LoadError, managed_engine_config, readiness_probe,
};
use super::MANAGED_KERNEL_LOCK_POISONED;

impl ControllerRuntime {
    #[cfg(target_os = "macos")]
    pub(super) fn release_macos_tun_route(&self) -> Result<(), LoadError> {
        if crate::macos_privileged::wait_for_tun_route_release().map_err(|error| {
            LoadError::Runtime(format!(
                "macOS TUN route release could not be confirmed: {error}"
            ))
        })? {
            return Ok(());
        }

        record_event(
            LogLevel::Warn,
            "controller.tun.route_release_restart",
            "Mihomo kept the macOS TUN route after disable; restarting managed core",
        );
        let Self::Managed { manager, .. } = self else {
            return Err(LoadError::Runtime(
                "the TUN route remained after disable, and Manis cannot restart the active kernel"
                    .to_owned(),
            ));
        };
        let mut manager = manager
            .lock()
            .map_err(|_poisoned| LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned()))?;
        manager.stop()?;
        manager.start()?;
        if !crate::macos_privileged::wait_for_tun_route_release().map_err(|error| {
            LoadError::Runtime(format!(
                "macOS TUN route release could not be confirmed after restart: {error}"
            ))
        })? {
            return Err(LoadError::Runtime(
                "the macOS TUN route remained after Mihomo restarted".to_owned(),
            ));
        }
        record_event(
            LogLevel::Info,
            "controller.tun.route_release_succeeded",
            "managed core restarted and macOS TUN route was released",
        );
        Ok(())
    }

    #[cfg(target_os = "macos")]
    pub(super) fn ensure_privileged_mihomo(&self) -> Result<(), LoadError> {
        use crate::macos_privileged::MacosPrivilegedProcessSpawner;

        let Self::Managed {
            manager,
            generated_profile: Some(spec),
            privileged,
            ..
        } = self
        else {
            return Err(LoadError::Runtime(
                "macOS TUN requires a Mihomo configuration generated from saved Manis sources"
                    .to_owned(),
            ));
        };
        if spec.kernel != KernelKind::Mihomo {
            return Err(LoadError::Runtime(
                "the macOS privileged helper currently supports only Mihomo".to_owned(),
            ));
        }
        if privileged.load(Ordering::Acquire) {
            record_event(
                LogLevel::Debug,
                "helper.promotion.skipped",
                "reason=already_privileged",
            );
            return Ok(());
        }

        record_event(
            LogLevel::Info,
            "helper.promotion.requested",
            "kernel=mihomo",
        );
        let spawner = MacosPrivilegedProcessSpawner::prepare().map_err(|error| {
            LoadError::Runtime(format!("macOS TUN helper could not be prepared: {error}"))
        })?;
        let final_path = spec.data_dir.join(GENERATED_PROFILE_FILE);
        let config = managed_engine_config(spec, final_path.clone());
        validate_managed_config(&config)?;

        let mut manager = manager
            .lock()
            .map_err(|_poisoned| LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned()))?;
        let was_running = manager.running_endpoint()?.is_some();
        record_event(
            LogLevel::Info,
            "helper.promotion.prepared",
            format!("ordinary_core_running={was_running}"),
        );
        if was_running {
            manager.stop()?;
            record_event(
                LogLevel::Info,
                "helper.promotion.ordinary_core_stopped",
                "ordinary Mihomo stopped before privileged launch",
            );
        } else {
            MacosPrivilegedProcessSpawner::reclaim_stale_ordinary(&config.launch_command())
                .map_err(|error| {
                    LoadError::Runtime(format!(
                        "stale unprivileged Mihomo process could not be reclaimed: {error}"
                    ))
                })?;
        }
        *manager = EngineManager::with_adapters(
            config,
            ReadinessPolicy::default(),
            Box::new(spawner),
            readiness_probe(spec),
        );
        start_promoted_mihomo(&mut manager, spec, final_path, was_running, privileged)
    }

    #[cfg(target_os = "linux")]
    pub(super) fn ensure_linux_tun_capabilities(&self) -> Result<(), LoadError> {
        let Self::Managed {
            manager,
            generated_profile: Some(spec),
            privileged,
            ..
        } = self
        else {
            return Err(LoadError::Runtime(
                "Linux TUN requires a Mihomo configuration generated from saved Manis sources"
                    .to_owned(),
            ));
        };
        if spec.kernel != KernelKind::Mihomo {
            return Err(LoadError::Runtime(
                "Linux packaged TUN capabilities support only Mihomo".to_owned(),
            ));
        }
        if privileged.load(Ordering::Acquire) {
            record_event(
                LogLevel::Debug,
                "helper.promotion.skipped",
                "platform=linux reason=already_capable",
            );
            return Ok(());
        }

        record_event(
            LogLevel::Info,
            "helper.promotion.requested",
            "platform=linux kernel=mihomo capabilities=cap_net_admin,cap_net_raw",
        );
        let privileged_binary = crate::linux_privileged::ensure_tun_capabilities()
            .map_err(|error| LoadError::Runtime(error.to_string()))?;

        let final_path = spec.data_dir.join(GENERATED_PROFILE_FILE);
        let mut privileged_spec = spec.clone();
        privileged_spec.binary = privileged_binary;
        let config = managed_engine_config(&privileged_spec, final_path.clone());
        validate_managed_config(&config)?;

        let mut manager = manager
            .lock()
            .map_err(|_poisoned| LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned()))?;
        let was_running = manager.running_endpoint()?.is_some();
        if was_running {
            manager.stop()?;
        }
        *manager = EngineManager::new(config, ReadinessPolicy::default(), readiness_probe(spec));
        match manager.start() {
            Ok(_endpoint) => {
                privileged.store(true, Ordering::Release);
                record_event(
                    LogLevel::Info,
                    "helper.promotion.succeeded",
                    format!(
                        "platform=linux capability_state=packaged ordinary_core_restarted={was_running}"
                    ),
                );
                Ok(())
            }
            Err(error) => {
                record_event(
                    LogLevel::Error,
                    "helper.promotion.failed",
                    format!("platform=linux error={error}"),
                );
                let fallback_config = managed_engine_config(spec, final_path);
                *manager = EngineManager::new(
                    fallback_config,
                    ReadinessPolicy::default(),
                    readiness_probe(spec),
                );
                let fallback = was_running.then(|| manager.start().err()).flatten();
                let message = fallback.map_or_else(
                    || format!("Mihomo could not start with Linux TUN capabilities: {error}"),
                    |fallback| {
                        format!(
                            "Mihomo could not start with Linux TUN capabilities: {error}; restoring ordinary Mihomo also failed: {fallback}"
                        )
                    },
                );
                Err(LoadError::Runtime(message))
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn start_promoted_mihomo(
    manager: &mut EngineManager,
    spec: &ManagedGeneratedProfile,
    final_path: std::path::PathBuf,
    was_running: bool,
    privileged: &std::sync::atomic::AtomicBool,
) -> Result<(), LoadError> {
    match manager.start() {
        Ok(_endpoint) => {
            privileged.store(true, Ordering::Release);
            record_event(
                LogLevel::Info,
                "helper.promotion.succeeded",
                "privileged Mihomo controller became ready",
            );
            Ok(())
        }
        Err(privileged_error) => {
            record_event(
                LogLevel::Error,
                "helper.promotion.failed",
                privileged_error.to_string(),
            );
            let fallback_config = managed_engine_config(spec, final_path);
            *manager = EngineManager::new(
                fallback_config,
                ReadinessPolicy::default(),
                readiness_probe(spec),
            );
            let fallback = was_running.then(|| manager.start().err()).flatten();
            let message = fallback.map_or_else(
                || format!("privileged Mihomo failed to start: {privileged_error}"),
                |fallback| {
                    format!(
                        "privileged Mihomo failed to start: {privileged_error}; restoring unprivileged Mihomo also failed: {fallback}"
                    )
                },
            );
            Err(LoadError::Runtime(message))
        }
    }
}
