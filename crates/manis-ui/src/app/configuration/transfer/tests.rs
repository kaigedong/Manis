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
            super::copy::backup::EXPORTING,
            super::copy::backup::EXPORTED,
            false,
        ),
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
fn file_export_produces_an_importable_backup_without_changing_sources(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(crate::init);
    let root = std::env::temp_dir().join(format!(
        "manis-transfer-export-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let store = root.join("subscriptions");
    crate::mihomo::save_routing_mode_in(&store, manis_core::RoutingMode::Direct)
        .expect("fixture routing");
    let output = root.join("export.json");
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
    let app = app.expect("fixture app");
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.choose_configuration_export(window, cx));
        assert!(
            !window.has_active_dialog(cx),
            "export must only show the system save dialog"
        );
    });
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|_| Some(output.clone()));
    cx.run_until_parked();
    let preview = crate::config_backup::read_backup(&output).expect("export is importable");
    assert_eq!(preview.summary().subscriptions, 0);
    assert_eq!(
        crate::mihomo::load_routing_mode_in(&store).expect("source mode"),
        manis_core::RoutingMode::Direct
    );
    app.read_with(cx, |app, _| {
        assert!(!app.configuration_transfer.active);
        assert!(!app.configuration_transfer.failed);
        assert!(!app.configuration_transfer.is_busy());
        assert_eq!(
            app.configuration_transfer.output_path.as_ref(),
            Some(&output)
        );
    });
    cx.update(|window, cx| assert!(!window.has_active_dialog(cx)));
    std::fs::remove_dir_all(root).expect("remove fixture");
}

#[gpui::test]
fn cancelling_export_does_not_open_a_dialog_or_write_configuration(cx: &mut gpui::TestAppContext) {
    cx.update(crate::init);
    let store = unique_temp_store("manis-export-cancel");
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
    let app = app.expect("fixture app");
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.choose_configuration_export(window, cx));
        assert!(!window.has_active_dialog(cx));
    });
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert!(!app.configuration_transfer.active);
        assert!(!app.configuration_transfer.is_busy());
        assert!(app.configuration_transfer.output_path.is_none());
    });
    cx.update(|window, cx| assert!(!window.has_active_dialog(cx)));
    assert!(
        !store.exists(),
        "cancelling export must not write configuration"
    );
}

#[gpui::test]
fn failed_export_unlocks_configuration_and_reports_the_error_without_a_dialog(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(crate::init);
    let store = unique_temp_store("manis-export-failure");
    crate::mihomo::save_routing_mode_in(&store, manis_core::RoutingMode::Direct)
        .expect("fixture routing");
    let output = store.parent().expect("fixture root").join("directory.json");
    std::fs::create_dir(&output).expect("directory cannot be overwritten by export");
    let original = std::fs::read(store.join("routing.mode")).expect("fixture routing");
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
    let app = app.expect("fixture app");
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.choose_configuration_export(window, cx));
    });
    cx.simulate_new_path_selection(|_| Some(output));
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert!(!app.configuration_transfer.active);
        assert!(!app.configuration_transfer.is_busy());
        assert!(app.configuration_transfer.failed);
        assert!(app.configuration_transfer.output_path.is_none());
        assert_eq!(
            app.status,
            app.language().localized(super::copy::backup::EXPORT_FAILED)
        );
    });
    cx.update(|window, cx| assert!(!window.has_active_dialog(cx)));
    assert_eq!(
        std::fs::read(store.join("routing.mode")).expect("routing unchanged"),
        original
    );
    std::fs::remove_dir_all(store.parent().expect("fixture root")).expect("remove fixture");
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
fn cancelling_file_selection_unlocks_configuration_without_writing(cx: &mut gpui::TestAppContext) {
    cx.update(crate::init);
    let store = unique_temp_store("manis-transfer-cancel");
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
    let app = app.expect("fixture app");
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.choose_configuration_import(window, cx));
        assert!(
            !window.has_active_dialog(cx),
            "only the native picker should open"
        );
    });
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|options| {
        assert!(options.files && !options.directories && !options.multiple);
        None
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| assert!(!app.configuration_transfer.active));
    cx.update(|window, cx| assert!(!window.has_active_dialog(cx)));
    assert!(!store.exists(), "cancelling must not create configuration");
}

#[gpui::test]
fn file_import_only_opens_preview_after_selection_and_validation(cx: &mut gpui::TestAppContext) {
    cx.update(crate::init);
    let store = unique_temp_store("manis-import-preview");
    crate::mihomo::save_routing_mode_in(&store, manis_core::RoutingMode::Direct)
        .expect("fixture mode");
    let original = std::fs::read(store.join("routing.mode")).expect("fixture contents");
    let input = store.parent().unwrap().join("input.json");
    std::fs::write(&input, "not a backup").expect("invalid fixture");
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
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.choose_configuration_import(window, cx));
        assert!(!window.has_active_dialog(cx));
    });
    cx.simulate_path_prompt_response(|_| Some(vec![input.clone()]));
    cx.run_until_parked();
    cx.update(|window, cx| assert!(!window.has_active_dialog(cx)));
    app.read_with(cx, |app, _| {
        assert!(app.configuration_transfer.failed);
        assert!(!app.configuration_transfer.active);
    });
    crate::config_backup::export_to_file(&store, &input).expect("valid fixture");
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.choose_configuration_import(window, cx));
        assert!(!window.has_active_dialog(cx));
    });
    cx.simulate_path_prompt_response(|_| Some(vec![input.clone()]));
    cx.run_until_parked();
    cx.update(|window, cx| assert!(window.has_active_dialog(cx)));
    app.read_with(cx, |app, _| {
        assert!(
            app.configuration_transfer.active,
            "preview must keep the mutation lock"
        );
        assert!(!app.configuration_transfer.failed);
        assert!(!app.configuration_transfer.is_busy());
        assert!(app.configuration_transfer.preview.is_some());
    });
    assert_eq!(std::fs::read(store.join("routing.mode")).unwrap(), original);
    std::fs::remove_dir_all(store.parent().unwrap()).unwrap();
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
    let original = std::fs::read(store.join("policy-1.policy")).unwrap();
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
    let document: serde_json::Value = serde_json::from_str(&draft).unwrap();
    assert_eq!(
        document["files"][0]["contents"],
        String::from_utf8(original.clone()).unwrap()
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
        std::fs::read(store.join("policy-1.policy")).unwrap(),
        original
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
    let original = std::fs::read(store.join("routing.mode")).unwrap();
    cx.write_to_clipboard(ClipboardItem::new_string(
        "not the current configuration".to_owned(),
    ));
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
        assert!(app.configuration_transfer.active);
        assert!(app.configuration_transfer.preview.is_none());
        app.configuration_transfer
            .editor
            .clone()
            .expect("prefilled editor")
    });
    let current = editor.read_with(cx, |editor, _| editor.value());
    let document: serde_json::Value = serde_json::from_str(&current).unwrap();
    assert!(
        document["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| file["name"] == "routing.mode"
                && file["contents"] == String::from_utf8(original.clone()).unwrap())
    );
    cx.update(|window, cx| {
        assert!(window.has_active_dialog(cx));
        editor.update(cx, |editor, cx| {
            editor.set_value("invalid edits", window, cx);
        });
        app.update(cx, ManisApp::preview_configuration_edits);
    });
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert!(app.configuration_transfer.failed);
        assert!(!app.configuration_transfer.is_busy());
        assert!(app.configuration_transfer.preview.is_none());
        assert!(app.configuration_transfer.active);
    });
    assert_eq!(
        editor.read_with(cx, |editor, _| editor.value()),
        "invalid edits"
    );
    let mut replacement = document;
    for file in replacement["files"].as_array_mut().unwrap() {
        if file["name"] == "routing.mode" {
            file["contents"] = "global".into();
        }
    }
    let replacement = serde_json::to_string_pretty(&replacement).unwrap();
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
    assert_eq!(std::fs::read(store.join("routing.mode")).unwrap(), original);
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
