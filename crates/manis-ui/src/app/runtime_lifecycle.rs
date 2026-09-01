use super::{
    GroupBenchmarkState, ManagedPolicyRuntimeState, ManisApp, PolicyBenchmarkRun, ProxyPorts,
    apply_proxy_mode_transition,
};
use crate::{
    app_update::{self, AppUpdateError, AvailableUpdate},
    core_update,
    diagnostics::{LogLevel, UiEvent, begin_operation, record_event, record_operation, trace_ui},
    kernel::{self, KernelRuntime},
    localization::{Language, copy},
    mihomo::{
        self, ControllerRuntime, ControllerState, LiveRuntimeSession, LiveStreamPhase,
        LiveStreamStatus, LoadedSnapshot, ManagedRuntimeHealth,
    },
};
use gpui::{Context, Task};
use manis_core::{KernelKind, ProxyMode};
use std::time::Duration;

mod benchmark;
#[path = "runtime_lifecycle/kernel.rs"]
mod kernel_flow;
mod live;
mod model;
mod update;
pub(super) use model::LifecycleSubscriptions;

use update::perform_mihomo_core_update;
pub(super) use update::{
    AppUpdateState, KernelSwitchState, MihomoCoreUpdateOutcome, MihomoCoreUpdateState,
};

struct KernelSwitchFailure {
    message: String,
    proxy_mode_restored: bool,
}

fn perform_kernel_switch(
    requested: KernelKind,
    previous_kind: KernelKind,
    previous_mode: ProxyMode,
    prepare: impl FnOnce() -> Result<KernelRuntime, String>,
    mut save_kernel_kind: impl FnMut(KernelKind) -> Result<(), String>,
    restore_proxy_mode: impl FnOnce(ProxyMode) -> Result<(), String>,
    stop_previous: impl FnOnce() -> Result<(), String>,
) -> Result<KernelRuntime, KernelSwitchFailure> {
    let prepared = prepare().map_err(|message| KernelSwitchFailure {
        message,
        proxy_mode_restored: false,
    })?;
    save_kernel_kind(requested).map_err(|message| KernelSwitchFailure {
        message,
        proxy_mode_restored: false,
    })?;
    let mut proxy_mode_restored = false;
    if previous_mode != ProxyMode::Off {
        if let Err(message) = restore_proxy_mode(previous_mode) {
            let message = message_with_selection_rollback(message, save_kernel_kind(previous_kind));
            return Err(KernelSwitchFailure {
                message,
                proxy_mode_restored: false,
            });
        }
        proxy_mode_restored = true;
    }
    if let Err(message) = stop_previous() {
        let message = message_with_selection_rollback(message, save_kernel_kind(previous_kind));
        return Err(KernelSwitchFailure {
            message,
            proxy_mode_restored,
        });
    }
    Ok(prepared)
}

fn message_with_selection_rollback(message: String, rollback: Result<(), String>) -> String {
    match rollback {
        Ok(()) => message,
        Err(rollback) => {
            format!("{message}; also could not restore the previous kernel selection: {rollback}")
        }
    }
}

#[cfg(test)]
mod runtime_lifecycle_tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use manis_core::{KernelKind, ProxyMode};

    use gpui::AppContext as _;

    use super::{ControllerRuntime, KernelRuntime, ManisApp, perform_kernel_switch};

    fn fixture_runtime() -> KernelRuntime {
        KernelRuntime::mihomo(ControllerRuntime::Fixture {
            endpoint: "http://127.0.0.1:9090".to_owned(),
        })
    }

    #[test]
    fn active_proxy_is_restored_before_previous_kernel_stops() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let prepared = fixture_runtime();

        let result = perform_kernel_switch(
            KernelKind::SingBox,
            KernelKind::Mihomo,
            ProxyMode::System,
            {
                let calls = calls.clone();
                let prepared = prepared.clone();
                move || {
                    calls.borrow_mut().push("prepare".to_owned());
                    Ok(prepared)
                }
            },
            {
                let calls = calls.clone();
                move |kind| {
                    calls
                        .borrow_mut()
                        .push(format!("save:{}", kind.persistence_key()));
                    Ok(())
                }
            },
            {
                let calls = calls.clone();
                move |mode| {
                    calls.borrow_mut().push(format!("restore:{mode:?}->Off"));
                    Ok(())
                }
            },
            {
                let calls = calls.clone();
                move || {
                    calls.borrow_mut().push("stop".to_owned());
                    Ok(())
                }
            },
        );

        assert!(result.is_ok());
        assert_eq!(
            calls.borrow().as_slice(),
            ["prepare", "save:sing-box", "restore:System->Off", "stop"]
        );
    }

    #[gpui::test]
    fn switching_kernel_reserves_proxy_mode_even_when_proxy_is_off(cx: &mut gpui::TestAppContext) {
        let store = std::env::temp_dir().join(format!(
            "manis-kernel-switch-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        let app = cx.new(|_| {
            ManisApp::with_fixture_controller_and_subscription_store(
                "http://127.0.0.1:9090",
                store.join("subscriptions"),
            )
        });

        app.update(cx, |app, cx| {
            assert_eq!(app.proxy_mode, ProxyMode::Off);
            assert!(app.proxy_mode_busy.is_none());

            app.switch_kernel(KernelKind::SingBox, cx);

            assert_eq!(app.proxy_mode_busy, Some(ProxyMode::Off));
            app.apply_proxy_mode(ProxyMode::System, cx);
            assert_eq!(app.proxy_mode, ProxyMode::Off);
            assert_eq!(app.proxy_mode_busy, Some(ProxyMode::Off));
        });
        let _ = std::fs::remove_dir_all(store);
    }

    #[test]
    fn proxy_cleanup_failure_keeps_previous_kernel_running_and_rolls_back_selection() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let prepared = fixture_runtime();

        let result = perform_kernel_switch(
            KernelKind::SingBox,
            KernelKind::Mihomo,
            ProxyMode::Tun,
            {
                let calls = calls.clone();
                let prepared = prepared.clone();
                move || {
                    calls.borrow_mut().push("prepare".to_owned());
                    Ok(prepared)
                }
            },
            {
                let calls = calls.clone();
                move |kind| {
                    calls
                        .borrow_mut()
                        .push(format!("save:{}", kind.persistence_key()));
                    Ok(())
                }
            },
            {
                let calls = calls.clone();
                move |mode| {
                    calls.borrow_mut().push(format!("restore:{mode:?}->Off"));
                    Err("restore failed".to_owned())
                }
            },
            {
                let calls = calls.clone();
                move || {
                    calls.borrow_mut().push("stop".to_owned());
                    Ok(())
                }
            },
        );

        let Err(failure) = result else {
            panic!("cleanup failure must abort the kernel switch");
        };
        assert_eq!(failure.message, "restore failed");
        assert!(!failure.proxy_mode_restored);
        assert_eq!(
            calls.borrow().as_slice(),
            [
                "prepare",
                "save:sing-box",
                "restore:Tun->Off",
                "save:mihomo"
            ]
        );
    }

    #[test]
    fn stop_failure_reports_restored_proxy_mode() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let prepared = fixture_runtime();

        let result = perform_kernel_switch(
            KernelKind::SingBox,
            KernelKind::Mihomo,
            ProxyMode::System,
            {
                let calls = calls.clone();
                let prepared = prepared.clone();
                move || {
                    calls.borrow_mut().push("prepare".to_owned());
                    Ok(prepared)
                }
            },
            {
                let calls = calls.clone();
                move |kind| {
                    calls
                        .borrow_mut()
                        .push(format!("save:{}", kind.persistence_key()));
                    Ok(())
                }
            },
            {
                let calls = calls.clone();
                move |mode| {
                    calls.borrow_mut().push(format!("restore:{mode:?}->Off"));
                    Ok(())
                }
            },
            {
                let calls = calls.clone();
                move || {
                    calls.borrow_mut().push("stop".to_owned());
                    Err("stop failed".to_owned())
                }
            },
        );

        let Err(failure) = result else {
            panic!("stop failure must fail the kernel switch");
        };
        assert_eq!(failure.message, "stop failed");
        assert!(failure.proxy_mode_restored);
        assert_eq!(
            calls.borrow().as_slice(),
            [
                "prepare",
                "save:sing-box",
                "restore:System->Off",
                "stop",
                "save:mihomo"
            ]
        );
    }

    #[test]
    fn rollback_failure_is_reported_when_cleanup_fails() {
        let prepared = fixture_runtime();

        let result = perform_kernel_switch(
            KernelKind::SingBox,
            KernelKind::Mihomo,
            ProxyMode::System,
            move || Ok(prepared),
            |kind| match kind {
                KernelKind::SingBox => Ok(()),
                KernelKind::Mihomo => Err("selection write failed".to_owned()),
            },
            |_mode| Err("restore failed".to_owned()),
            || Ok(()),
        );

        let Err(failure) = result else {
            panic!("cleanup failure must abort the kernel switch");
        };
        assert_eq!(
            failure.message,
            "restore failed; also could not restore the previous kernel selection: selection write failed"
        );
    }
}
