use std::fs;
use std::path::Path;
use std::sync::atomic::Ordering;

use manis_core::KernelKind;
use manis_engine::{EngineManager, validate_managed_config};
use manis_profile::write_private_atomic;

use super::{
    ControllerRuntime, GeneratedProfileApply, LoadError, LogLevel, ManagedGeneratedProfile,
    compile_saved_profile, fetch_runtime_config, generated_engine_manager, generated_profile_names,
    managed_engine_config, record_event, render_generated_profile, sync_single_node_provider_files,
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
            ControllerRuntime::Fixture { .. } => "测试快照不能写入运行配置".to_owned(),
            ControllerRuntime::Invalid { message } => message.clone(),
            ControllerRuntime::Managed { .. } => "托管内核缺少 Manis 生成配置".to_owned(),
        }));
    };
    let _apply_guard = apply_lock
        .lock()
        .map_err(|_poisoned| LoadError::Runtime("配置应用锁已损坏".to_owned()))?;
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
        .map_err(|_error| LoadError::Runtime("无法写入候选托管配置".to_owned()))?;
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
    .map_err(|_error| LoadError::Runtime("无法替换托管配置".to_owned()))?;
    let final_config = managed_engine_config(spec, final_path);
    let mut manager = manager
        .lock()
        .map_err(|_poisoned| LoadError::Runtime("托管内核状态锁已损坏".to_owned()))?;
    let running_endpoint = manager.running_endpoint()?;
    let was_running = running_endpoint.is_some();
    let restore_tun = tun_was_enabled(spec, running_endpoint.as_ref())?;
    stop_previous_runtime(&mut manager, was_running, restore_tun)?;
    *manager = generated_engine_manager(spec, final_config, was_privileged).map_err(|error| {
        if restore_tun {
            LoadError::ProxyModeLost(format!("TUN 已停止，但无法创建新托管内核：{error}"))
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
                "重载托管配置前无法确认 macOS TUN 路由已释放：{error}"
            ))
        })?
    {
        return Err(LoadError::ProxyModeLost(
            "重载托管配置前 macOS TUN 路由未释放".to_owned(),
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
        let rollback_config = managed_engine_config(
            rollback.spec,
            rollback.spec.data_dir.join(rollback.final_name),
        );
        *manager = generated_engine_manager(
            rollback.spec,
            rollback_config,
            rollback.was_privileged,
        )
        .map_err(|rollback_error| {
            if rollback.restore_tun {
                LoadError::ProxyModeLost(format!(
                    "新配置启动失败（{restart_error}），旧配置管理器恢复也失败：{rollback_error}"
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
                "新配置启动失败（{restart_error}），旧配置已恢复，但 TUN 重新启用失败：{restore_error}"
            ))
        })?;
    } else if rollback.restore_tun {
        return Err(LoadError::ProxyModeLost(format!(
            "新配置启动失败（{restart_error}），且旧配置未能重新启动"
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
    record_event(
        LogLevel::Info,
        "proxy.mode.restore.succeeded",
        format!("mode=tun reason={reason}"),
    );
    Ok(())
}
