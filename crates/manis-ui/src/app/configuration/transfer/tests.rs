use crate::app::ManisApp;
use gpui::{AppContext as _, ClipboardItem};
use gpui_component::WindowExt as _;

struct BackupPanelHarness(gpui::Entity<ManisApp>);

impl gpui::Render for BackupPanelHarness {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::{InteractiveElement as _, ParentElement as _, Styled as _};
        gpui::div()
            .w(gpui::px(640.0))
            .child(self.0.update(cx, |app, cx| {
                app.configuration_transfer_panel(crate::theme::Theme::light(), true, cx)
                    .debug_selector(|| "backup-card".into())
            }))
    }
}

#[gpui::test]
fn configuration_panel_uses_the_in_app_editor_as_its_only_transfer_entry(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(crate::init);
    let app = cx.new(|_| {
        ManisApp::with_fixture_controller_and_subscription_store(
            "http://127.0.0.1:1",
            unique_temp_store("manis-editor-entry"),
        )
    });
    let (_, cx) = cx.add_window_view(|_, _| BackupPanelHarness(app));
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(cx.debug_bounds("configuration-edit").is_some());
    assert!(cx.debug_bounds("configuration-export").is_none());
    assert!(cx.debug_bounds("configuration-import").is_none());
}

#[gpui::test]
fn transfer_feedback_stays_in_status_bar_without_resizing_the_card(cx: &mut gpui::TestAppContext) {
    cx.update(crate::init);
    let app = cx.new(|_| {
        ManisApp::with_fixture_controller_and_subscription_store(
            "http://127.0.0.1:1",
            unique_temp_store("manis-transfer-layout"),
        )
    });
    let (_, cx) = cx.add_window_view(|_, _| BackupPanelHarness(app.clone()));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let initial = cx.debug_bounds("backup-card").expect("backup card renders");
    for (pending, finished, failed) in [
        (
            super::copy::backup::READING,
            super::copy::backup::INVALID,
            true,
        ),
        (
            super::copy::backup::LOADING_CURRENT,
            super::copy::backup::EDIT_FAILED,
            true,
        ),
    ] {
        cx.update(|window, cx| {
            app.update(cx, |app, cx| {
                let message = app.language().localized(pending);
                assert!(app.begin_configuration_transfer(
                    message,
                    super::TransferPresentation::StatusBar,
                    window,
                    cx
                ));
                assert_eq!(app.status, message);
            });
            window.draw(cx).clear(cx);
        });
        assert_eq!(
            cx.debug_bounds("backup-card"),
            Some(initial),
            "progress must not resize the card"
        );
        cx.update(|window, cx| {
            app.update(cx, |app, cx| {
                let message = app.language().localized(finished);
                app.finish_configuration_transfer(message, failed, cx);
                assert_eq!(app.status, message);
            });
            window.draw(cx).clear(cx);
        });
        assert_eq!(
            cx.debug_bounds("backup-card"),
            Some(initial),
            "results must not resize the card"
        );
    }
}

#[gpui::test]
fn active_migration_blocks_source_writes_and_benchmarks(cx: &mut gpui::TestAppContext) {
    use manis_core::RoutingMode;
    let store = std::env::temp_dir().join(format!(
        "manis-transfer-lock-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    crate::mihomo::save_routing_mode_in(&store, RoutingMode::Direct).expect("fixture routing");
    let app = cx.new(|_| {
        ManisApp::with_fixture_controller_and_subscription_store(
            "http://127.0.0.1:1",
            store.clone(),
        )
    });
    app.update(cx, |app, cx| {
        app.configuration_transfer.active = true;
        app.set_language_preference(crate::localization::LanguagePreference::English, cx);
        app.set_single_node_enabled("saved-deadbeef".to_owned(), false, cx);
        app.remove_saved_single_node("saved-deadbeef".to_owned(), cx);
        app.apply_routing_mode(RoutingMode::Global, cx);
        assert!(
            app.begin_group_benchmark("source:fixture".to_owned())
                .is_none()
        );
        assert!(!app.routing_apply_state.is_busy());
        assert_eq!(app.routing_mode, RoutingMode::Direct);
    });
    cx.run_until_parked();
    assert!(!store.join("language.preference").exists());
    assert!(!store.join("benchmarks.state").exists());
    assert_eq!(
        crate::mihomo::load_routing_mode_in(&store).expect("mode unchanged"),
        RoutingMode::Direct
    );
    std::fs::remove_dir_all(store).expect("remove fixture");
}
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_store(prefix: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir()
        .join(format!("{prefix}-{}-{nanos:x}", std::process::id()))
        .join("subscriptions")
}

#[gpui::test]
fn editor_opens_dangling_policy_references_but_requires_repair_before_preview(
    cx: &mut gpui::TestAppContext,
) {
    use manis_core::{ManagedPolicyGroup, NodeIdentity, PolicyCandidateMatcher};
    cx.update(crate::init);
    let store = unique_temp_store("manis-editor-dangling-policy");
    let mut group = ManagedPolicyGroup::new("policy-1", "Test group").unwrap();
    group.matcher = PolicyCandidateMatcher::Explicit(
        [NodeIdentity::new("policy:policy-2", "Removed group").unwrap()]
            .into_iter()
            .collect(),
    );
    crate::mihomo::save_managed_policy_in(&store, &group).unwrap();
    let original = crate::config_toml::read_entry(&store, "policy-1.policy", 1024 * 1024)
        .unwrap()
        .unwrap();
    assert!(crate::config_backup::export_backup(&store).is_err());
    let mut app = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|_| {
            ManisApp::with_fixture_controller_and_subscription_store(
                "http://127.0.0.1:1",
                store.clone(),
            )
        });
        app = Some(entity.clone());
        crate::root(entity, window, cx)
    });
    let app = app.unwrap();
    cx.update(|window, cx| app.update(cx, |app, cx| app.edit_configuration(window, cx)));
    cx.run_until_parked();
    let editor = app.read_with(cx, |app, _| {
        app.configuration_transfer
            .editor
            .clone()
            .expect("invalid references must remain editable")
    });
    let draft = editor.read_with(cx, |editor, _| editor.value());
    assert_eq!(
        crate::config_toml::entries_from_source(&draft)
            .unwrap()
            .get("policy-1.policy"),
        Some(&original)
    );
    cx.update(|window, cx| {
        assert!(window.has_active_dialog(cx));
        app.update(cx, ManisApp::preview_configuration_edits);
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert!(app.configuration_transfer.preview.is_none());
        assert!(app.configuration_transfer.failed);
        assert!(app.configuration_transfer.active);
    });
    assert_eq!(editor.read_with(cx, |editor, _| editor.value()), draft);
    assert_eq!(
        crate::config_toml::read_entry(&store, "policy-1.policy", 1024 * 1024)
            .unwrap()
            .unwrap(),
        original,
    );
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.cancel_configuration_transfer(window, cx));
    });
    std::fs::remove_dir_all(store.parent().unwrap()).unwrap();
}

#[gpui::test]
fn editor_prefills_current_configuration_and_preserves_invalid_edits(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(crate::init);
    let store = unique_temp_store("manis-config-editor");
    crate::mihomo::save_routing_mode_in(&store, manis_core::RoutingMode::Direct)
        .expect("fixture routing");
    let original = crate::config_toml::read_entry(&store, "routing.mode", 32)
        .unwrap()
        .unwrap();
    let mut app = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|_| {
            ManisApp::with_fixture_controller_and_subscription_store(
                "http://127.0.0.1:1",
                store.clone(),
            )
        });
        app = Some(entity.clone());
        crate::root(entity, window, cx)
    });
    let app = app.unwrap();
    cx.update(|window, cx| app.update(cx, |app, cx| app.edit_configuration(window, cx)));
    cx.run_until_parked();
    let editor = app.read_with(cx, |app, _| {
        assert!(app.configuration_transfer.preview.is_none());
        app.configuration_transfer
            .editor
            .clone()
            .expect("prefilled editor")
    });
    let current = editor.read_with(cx, |editor, _| editor.value());
    assert_eq!(
        crate::config_toml::entries_from_source(&current)
            .unwrap()
            .get("routing.mode"),
        Some(&original)
    );
    cx.update(|window, cx| {
        app.update(cx, ManisApp::copy_configuration_to_clipboard);
        assert_eq!(
            cx.read_from_clipboard().and_then(|item| item.text()),
            Some(current.to_string())
        );
        cx.write_to_clipboard(ClipboardItem::new_string("invalid edits".to_owned()));
        app.update(cx, |app, cx| {
            app.replace_configuration_from_clipboard(window, cx);
        });
        app.update(cx, ManisApp::preview_configuration_edits);
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert!(app.configuration_transfer.failed);
        assert!(app.configuration_transfer.preview.is_none());
        assert!(app.configuration_transfer.active);
    });
    assert_eq!(
        editor.read_with(cx, |editor, _| editor.value()),
        "invalid edits"
    );
    let replacement = current.replace("direct", "global");
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.set_value(replacement.clone(), window, cx);
        });
        app.update(cx, ManisApp::preview_configuration_edits);
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert!(
            app.configuration_transfer.preview.is_some(),
            "corrected input should reach confirmation"
        );
        assert!(!app.configuration_transfer.failed);
    });
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.resume_configuration_editing(window, cx));
    });
    assert_eq!(
        editor.read_with(cx, |editor, _| editor.value()),
        replacement
    );
    app.read_with(cx, |app, _| {
        assert!(app.configuration_transfer.preview.is_none());
    });
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.cancel_configuration_transfer(window, cx));
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert!(!app.configuration_transfer.active);
        assert!(app.configuration_transfer.editor.is_none());
    });
    assert_eq!(
        crate::config_toml::read_entry(&store, "routing.mode", 32)
            .unwrap()
            .unwrap(),
        original,
    );
    std::fs::remove_dir_all(store.parent().unwrap()).unwrap();
}

#[gpui::test]
fn replacement_stop_failure_clears_runtime_only_after_proxy_shutdown(
    cx: &mut gpui::TestAppContext,
) {
    for proxy_off in [false, true] {
        let app = cx.new(|_| {
            ManisApp::with_fixture_controller_and_subscription_store(
                "http://127.0.0.1:1",
                unique_temp_store("manis-replacement-stop"),
            )
        });
        app.update(cx, |app, cx| {
            app.proxy_mode = manis_core::ProxyMode::Tun;
            app.live_generation = 41;
            app.controller = crate::mihomo::ControllerState::Connecting {
                endpoint: "http://127.0.0.1:1".to_owned(),
            };
            app.configuration_transfer.active = true;
            app.configuration_transfer.progress = super::TransferProgress::Replacing;
            let result = if proxy_off {
                super::ConfigurationReplacementOutcome::KernelStopFailed
            } else {
                super::ConfigurationReplacementOutcome::ProxyStopFailed
            };
            app.finish_configuration_replacement(result, app.language(), cx);
            assert_eq!(
                app.proxy_mode,
                if proxy_off {
                    manis_core::ProxyMode::Off
                } else {
                    manis_core::ProxyMode::Tun
                },
            );
            assert_eq!(app.live_generation, if proxy_off { 42 } else { 41 });
            assert_eq!(
                matches!(app.controller, crate::mihomo::ControllerState::Disconnected),
                proxy_off,
            );
            assert!(!app.configuration_transfer.is_busy());
            assert!(app.configuration_transfer.failed);
            assert_eq!(
                app.status,
                app.language().localized(super::copy::backup::STOP_FAILED)
            );
        });
    }
}
