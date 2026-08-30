use std::fs;
use std::path::Path;
use std::sync::atomic::Ordering;

use manis_core::KernelKind;
use manis_engine::{EngineManager, validate_managed_config};
use manis_profile::write_private_atomic;

use super::{
    ControllerRuntime, GeneratedProfileApply, LoadError, LogLevel, ManagedGeneratedProfile,
    compile_saved_profile, fetch_runtime_config, generated_engine_manager, generated_profile_names,
    managed_engine_config, managed_engine_config_for_privilege, record_event,
    render_generated_profile, sync_single_node_provider_files,
};

struct PreparedProfile {
    rendered: String,
    final_name: &'static str,
}

struct RestartRollback<'a> {
    runtime: &'a ControllerRuntime,
    spec: &'a ManagedGeneratedProfile,
    was_privileged: bool,
    restore_tun: bool,
    previous_config: Option<Vec<u8>>,
    final_name: &'a str,
}

pub(super) fn apply_saved_sources(
    runtime: &ControllerRuntime,
    store_dir: &Path,
) -> Result<GeneratedProfileApply, LoadError> {
    let ControllerRuntime::Managed {
        manager,
        apply_lock,
        generated_profile: Some(spec),
        privileged,
        ..
    } = runtime
    else {
        return Err(LoadError::Runtime(match runtime {
            #[cfg(any(test, feature = "snapshot-fixtures"))]
            ControllerRuntime::Fixture { .. } => {
                "test snapshots cannot write runtime configuration".to_owned()
            }
            ControllerRuntime::Invalid { message } => message.clone(),
            ControllerRuntime::Managed { .. } => {
                "managed kernel has no Manis-generated configuration".to_owned()
            }
        }));
    };
    let _apply_guard = apply_lock.lock().map_err(|_poisoned| {
        LoadError::Runtime("configuration apply lock is poisoned".to_owned())
    })?;
    let prepared = prepare_profile(spec, store_dir)?;
    install_profile(
        runtime,
        spec,
        manager,
        privileged.load(Ordering::Acquire),
        &prepared,
    )
}

fn prepare_profile(
    spec: &ManagedGeneratedProfile,
    store_dir: &Path,
) -> Result<PreparedProfile, LoadError> {
    if spec.kernel == KernelKind::Mihomo {
        sync_single_node_provider_files(store_dir, &spec.data_dir)?;
    }
    let profile = compile_saved_profile(store_dir, None, spec.kernel)?;
    let rendered = render_generated_profile(spec, &profile)?;
    let (candidate_name, final_name) = generated_profile_names(spec.kernel);
    let candidate_path = write_private_atomic(&spec.data_dir, candidate_name, rendered.as_bytes())
        .map_err(|_error| {
            LoadError::Runtime("candidate managed configuration could not be written".to_owned())
        })?;
    let validation = validate_managed_config(&managed_engine_config(spec, candidate_path.clone()));
    let _ = fs::remove_file(candidate_path);
    validation?;
    Ok(PreparedProfile {
        rendered,
        final_name,
    })
}

fn install_profile(
    runtime: &ControllerRuntime,
    spec: &ManagedGeneratedProfile,
    manager: &std::sync::Mutex<EngineManager>,
    was_privileged: bool,
    prepared: &PreparedProfile,
) -> Result<GeneratedProfileApply, LoadError> {
    let final_path = spec.data_dir.join(prepared.final_name);
    let previous_config = fs::read(&final_path).ok();
    write_private_atomic(
        &spec.data_dir,
        prepared.final_name,
        prepared.rendered.as_bytes(),
    )
    .map_err(|_error| {
        LoadError::Runtime("managed configuration could not be replaced".to_owned())
    })?;
    let final_config = managed_engine_config_for_privilege(spec, final_path, was_privileged)?;
    let mut manager = manager.lock().map_err(|_poisoned| {
        LoadError::Runtime("managed kernel state lock is poisoned".to_owned())
    })?;
    let running_endpoint = manager.running_endpoint()?;
    let was_running = running_endpoint.is_some();
    let restore_tun = tun_was_enabled(spec, running_endpoint.as_ref())?;
    stop_previous_runtime(&mut manager, was_running, restore_tun)?;
    *manager = generated_engine_manager(spec, final_config, was_privileged).map_err(|error| {
        if restore_tun {
            LoadError::ProxyModeLost(format!(
                "TUN stopped, but the new managed kernel could not be created: {error}"
            ))
        } else {
            error
        }
    })?;
    if was_running && let Err(error) = manager.start() {
        return rollback_failed_restart(
            RestartRollback {
                runtime,
                spec,
                was_privileged,
                restore_tun,
                previous_config,
                final_name: prepared.final_name,
            },
            manager,
            error,
        );
    }
    drop(manager);
    if restore_tun {
        restore_tun_mode(runtime, "managed_config_restart")?;
    }
    Ok(if was_running {
        GeneratedProfileApply::Restarted
    } else {
        GeneratedProfileApply::Updated
    })
}

fn tun_was_enabled(
    spec: &ManagedGeneratedProfile,
    endpoint: Option<&manis_engine::ControllerEndpoint>,
) -> Result<bool, LoadError> {
    if spec.kernel != KernelKind::Mihomo {
        return Ok(false);
    }
    endpoint
        .map(|endpoint| {
            fetch_runtime_config(&endpoint.uri(), spec.controller_secret.as_deref())
                .map(|runtime| runtime.tun.enable)
                .map_err(LoadError::from)
        })
        .transpose()
        .map(Option::unwrap_or_default)
}

fn stop_previous_runtime(
    manager: &mut EngineManager,
    was_running: bool,
    restore_tun: bool,
) -> Result<(), LoadError> {
    if !was_running {
        return Ok(());
    }
    manager.stop()?;
    #[cfg(target_os = "macos")]
    if restore_tun
        && !crate::macos_privileged::wait_for_tun_route_release().map_err(|error| {
            LoadError::ProxyModeLost(format!(
                "macOS TUN route release could not be confirmed before reloading managed configuration: {error}"
            ))
        })?
    {
        return Err(LoadError::ProxyModeLost(
            "macOS TUN route was not released before reloading managed configuration".to_owned(),
        ));
    }
    #[cfg(not(target_os = "macos"))]
    let _ = restore_tun;
    Ok(())
}

fn rollback_failed_restart(
    rollback: RestartRollback<'_>,
    mut manager: std::sync::MutexGuard<'_, EngineManager>,
    error: manis_engine::EngineError,
) -> Result<GeneratedProfileApply, LoadError> {
    let restart_error = error.to_string();
    let mut rollback_running = false;
    if let Some(previous_config) = rollback.previous_config {
        let _ = write_private_atomic(
            &rollback.spec.data_dir,
            rollback.final_name,
            &previous_config,
        );
        let rollback_config = managed_engine_config_for_privilege(
            rollback.spec,
            rollback.spec.data_dir.join(rollback.final_name),
            rollback.was_privileged,
        )?;
        *manager = generated_engine_manager(
            rollback.spec,
            rollback_config,
            rollback.was_privileged,
        )
        .map_err(|rollback_error| {
            if rollback.restore_tun {
                LoadError::ProxyModeLost(format!(
                    "new configuration failed to start ({restart_error}); restoring the previous configuration manager also failed: {rollback_error}"
                ))
            } else {
                rollback_error
            }
        })?;
        rollback_running = manager.start().is_ok();
    }
    drop(manager);
    if rollback.restore_tun && rollback_running {
        restore_tun_mode(rollback.runtime, "managed_config_rollback").map_err(|restore_error| {
            LoadError::ProxyModeLost(format!(
                "new configuration failed to start ({restart_error}); the previous configuration was restored, but TUN could not be re-enabled: {restore_error}"
            ))
        })?;
    } else if rollback.restore_tun {
        return Err(LoadError::ProxyModeLost(format!(
            "new configuration failed to start ({restart_error}), and the previous configuration did not restart"
        )));
    }
    Err(LoadError::Engine(error))
}

fn restore_tun_mode(runtime: &ControllerRuntime, reason: &str) -> Result<(), LoadError> {
    record_event(
        LogLevel::Info,
        "proxy.mode.restore.requested",
        format!("mode=tun reason={reason}"),
    );
    if let Err(error) = runtime.set_tun_enabled(true) {
        record_event(
            LogLevel::Error,
            "proxy.mode.restore.failed",
            format!("mode=tun reason={reason} error={error}"),
        );
        return Err(LoadError::ProxyModeLost(error.to_string()));
    }
    #[cfg(target_os = "linux")]
    if let Err(error) = require_linux_dns_rebind(
        reason,
        || crate::linux_privileged::install_tun_dns().map_err(|error| error.to_string()),
        || crate::linux_privileged::restore_tun_dns().map_err(|error| error.to_string()),
        || {
            runtime
                .set_tun_enabled(false)
                .map_err(|error| error.to_string())
        },
    ) {
        record_event(
            LogLevel::Error,
            "controller.tun.dns.rebind_failed",
            format!("reason={reason} error={error}"),
        );
        return Err(LoadError::ProxyModeLost(error));
    }
    #[cfg(target_os = "linux")]
    record_event(
        LogLevel::Info,
        "controller.tun.dns.rebound",
        format!("reason={reason} link=Meta resolver=198.18.0.2 domain=~."),
    );
    record_event(
        LogLevel::Info,
        "proxy.mode.restore.succeeded",
        format!("mode=tun reason={reason}"),
    );
    Ok(())
}

#[cfg(target_os = "linux")]
fn require_linux_dns_rebind(
    reason: &str,
    rebind: impl FnOnce() -> Result<(), String>,
    restore: impl FnOnce() -> Result<(), String>,
    disable_tun: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let Err(error) = rebind() else {
        return Ok(());
    };
    let dns_cleanup = restore().map_or_else(
        |error| format!("failed ({error})"),
        |()| "succeeded".to_owned(),
    );
    let tun_cleanup = disable_tun().map_or_else(
        |error| format!("failed ({error})"),
        |()| "succeeded".to_owned(),
    );
    Err(format!(
        "TUN restarted after {reason}, but Linux DNS routing could not be rebound: {error}; DNS cleanup: {dns_cleanup}; TUN cleanup: {tun_cleanup}"
    ))
}

#[cfg(all(test, target_os = "linux"))]
mod linux_tests {
    use std::cell::RefCell;

    use super::require_linux_dns_rebind;

    #[test]
    fn linux_tun_dns_rebind_failure_restores_dns_before_disabling_tun() {
        let calls = RefCell::new(Vec::new());
        let result = require_linux_dns_rebind(
            "test_restart",
            || {
                calls.borrow_mut().push("rebind");
                Err("rebind failed".to_owned())
            },
            || {
                calls.borrow_mut().push("restore");
                Ok(())
            },
            || {
                calls.borrow_mut().push("disable");
                Ok(())
            },
        );

        assert_eq!(*calls.borrow(), ["rebind", "restore", "disable"]);
        let error = result.expect_err("a failed DNS rebind must fail the TUN restoration");
        assert!(error.contains("rebind failed"));
        assert!(error.contains("DNS cleanup: succeeded"));
        assert!(error.contains("TUN cleanup: succeeded"));
    }
}
