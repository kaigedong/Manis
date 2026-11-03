use std::path::Path;
#[cfg(target_os = "macos")]
use std::sync::atomic::Ordering;

#[cfg(target_os = "macos")]
use manis_core::KernelKind;
#[cfg(target_os = "macos")]
use manis_engine::{EngineManager, ReadinessPolicy, validate_managed_config};

use super::{
    ControllerRuntime, GeneratedProfileApply, LoadError, ManagedRuntimeHealth,
    RuntimeProfileSource, RuntimeSnapshot, load, managed_apply, validate_managed_runtime,
};
#[cfg(target_os = "macos")]
use super::{GENERATED_PROFILE_FILE, managed_engine_config, readiness_probe};

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod platform;
mod proxy_policy;

const MANAGED_KERNEL_LOCK_POISONED: &str = "managed kernel state lock is poisoned";

impl ControllerRuntime {
    pub(crate) fn is_fixture(&self) -> bool {
        match self {
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::Fixture { .. } => true,
            _ => false,
        }
    }

    fn controller_secret(&self) -> Option<&str> {
        match self {
            Self::Managed {
                generated_profile: Some(spec),
                ..
            } => spec.controller_secret.as_deref(),
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::Fixture { .. } => None,
            Self::Managed { .. } | Self::Invalid { .. } => None,
        }
    }

    pub(crate) fn stop_managed(&self) -> Result<(), LoadError> {
        if let Self::Managed { manager, .. } = self {
            manager
                .lock()
                .map_err(|_poisoned| LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned()))?
                .stop()?;
        }
        Ok(())
    }

    pub(crate) fn managed_health(&self) -> Result<ManagedRuntimeHealth, LoadError> {
        match self {
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::Fixture { .. } => Ok(ManagedRuntimeHealth::NotManaged),
            Self::Invalid { message } => Err(LoadError::Runtime(message.clone())),
            Self::Managed { manager, .. } => manager
                .lock()
                .map_err(|_poisoned| LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned()))?
                .running_endpoint()
                .map(|endpoint| {
                    if endpoint.is_some() {
                        ManagedRuntimeHealth::Running
                    } else {
                        ManagedRuntimeHealth::Stopped
                    }
                })
                .map_err(LoadError::from),
        }
    }

    pub(crate) fn profile_source(&self) -> RuntimeProfileSource {
        match self {
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::Fixture { .. } => RuntimeProfileSource::FixtureController,
            Self::Managed { profile_source, .. } => *profile_source,
            Self::Invalid { .. } => RuntimeProfileSource::Invalid,
        }
    }

    pub(crate) fn endpoint_label(&self) -> String {
        match self {
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::Fixture { endpoint } => endpoint.clone(),
            Self::Managed { .. } => "Manis managed".to_owned(),
            Self::Invalid { .. } => "not connected".to_owned(),
        }
    }

    pub(crate) fn connect(&self) -> Result<RuntimeSnapshot, LoadError> {
        match self {
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            Self::Fixture { endpoint } => Ok(RuntimeSnapshot {
                endpoint: endpoint.clone(),
                controller_endpoint: endpoint.clone(),
                controller_secret: None,
                snapshot: load(endpoint, None)?,
            }),
            Self::Managed {
                manager,
                generated_profile,
                privileged,
                ..
            } => {
                #[cfg(not(target_os = "macos"))]
                let _ = privileged;
                let secret = generated_profile
                    .as_ref()
                    .and_then(|spec| spec.controller_secret.clone());
                let endpoint = {
                    let mut manager = manager.lock().map_err(|_poisoned| {
                        LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned())
                    })?;
                    let running = manager.running_endpoint()?;
                    #[cfg(target_os = "macos")]
                    if running.is_none()
                        && !privileged.load(Ordering::Acquire)
                        && let Some(spec) = generated_profile
                        && spec.kernel == KernelKind::Mihomo
                    {
                        let config =
                            managed_engine_config(spec, spec.data_dir.join(GENERATED_PROFILE_FILE));
                        validate_managed_config(&config)?;
                        if let Some(spawner) = crate::macos_privileged::MacosPrivilegedProcessSpawner::recover_if_available()
                            .map_err(|error| {
                                LoadError::Runtime(format!(
                                    "macOS TUN helper could not be recovered: {error}"
                                ))
                            })?
                        {
                            crate::macos_privileged::MacosPrivilegedProcessSpawner::reclaim_stale_ordinary(
                                &config.launch_command(),
                            )
                                .map_err(|error| {
                                    LoadError::Runtime(format!(
                                        "stale unprivileged Mihomo process could not be reclaimed: {error}"
                                    ))
                                })?;
                            *manager = EngineManager::with_adapters(
                                config,
                                ReadinessPolicy::default(),
                                Box::new(spawner),
                                readiness_probe(spec),
                            );
                            privileged.store(true, Ordering::Release);
                        }
                    }
                    match running {
                        Some(endpoint) => endpoint,
                        None => manager.start()?,
                    }
                };
                let endpoint = endpoint.uri();
                let snapshot = load(&endpoint, secret.as_deref())?;
                if let Some(spec) = generated_profile
                    && let Err(error) = validate_managed_runtime(spec, &snapshot.runtime)
                {
                    let _ = manager
                        .lock()
                        .map_err(|_poisoned| {
                            LoadError::Runtime(MANAGED_KERNEL_LOCK_POISONED.to_owned())
                        })?
                        .stop();
                    return Err(error);
                }
                Ok(RuntimeSnapshot {
                    endpoint: "Manis managed".to_owned(),
                    controller_endpoint: endpoint.clone(),
                    controller_secret: secret.clone(),
                    snapshot,
                })
            }
            Self::Invalid { message } => Err(LoadError::Runtime(message.clone())),
        }
    }

    pub(crate) fn apply_saved_sources(
        &self,
        store_dir: &Path,
    ) -> Result<GeneratedProfileApply, LoadError> {
        managed_apply::apply_saved_sources(self, store_dir)
    }
}
