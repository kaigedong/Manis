#[derive(Default)]
enum TransferProgress {
    #[default]
    Idle,
    Preparing,
    Replacing,
}

#[derive(Default, PartialEq, Eq)]
enum TransferPresentation {
    #[default]
    Dialog,
    StatusBar,
}

#[derive(Default)]
pub(super) struct ConfigurationTransfer {
    pub(super) active: bool,
    progress: TransferProgress,
    presentation: TransferPresentation,
    preview: Option<crate::config_backup::PreparedBackup>,
    editor: Option<Entity<gpui_component::input::EditorState>>,
    message: String,
    failed: bool,
    output_path: Option<std::path::PathBuf>,
}

impl ConfigurationTransfer {
    fn is_busy(&self) -> bool {
        !matches!(self.progress, TransferProgress::Idle)
    }

    pub(super) fn is_replacing(&self) -> bool {
        matches!(self.progress, TransferProgress::Replacing)
    }
}

#[cfg(test)]
mod transfer_tests {
    use super::ManisApp;
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
    fn transfer_feedback_stays_in_status_bar_without_resizing_the_card(
        cx: &mut gpui::TestAppContext,
    ) {
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
        let output = root.join("export.manis.json");
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
    fn cancelling_export_does_not_open_a_dialog_or_write_configuration(
        cx: &mut gpui::TestAppContext,
    ) {
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
        let output = store
            .parent()
            .expect("fixture root")
            .join("directory.manis.json");
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
    fn cancelling_file_selection_unlocks_configuration_without_writing(
        cx: &mut gpui::TestAppContext,
    ) {
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
    fn file_import_only_opens_preview_after_selection_and_validation(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(crate::init);
        let store = unique_temp_store("manis-import-preview");
        crate::mihomo::save_routing_mode_in(&store, manis_core::RoutingMode::Direct)
            .expect("fixture mode");
        let original = std::fs::read(store.join("routing.mode")).expect("fixture contents");
        let input = store.parent().unwrap().join("input.manis.json");
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
}

impl ManisApp {
    /// Opens the real import preview using synthetic data in the offscreen renderer.
    #[cfg(feature = "snapshot-fixtures")]
    #[doc(hidden)]
    pub fn show_configuration_backup_fixture(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.runtime.is_fixture()
            && self.begin_configuration_transfer(
                self.language().localized(copy::backup::READING),
                TransferPresentation::Dialog,
                window,
                cx,
            )
        {
            self.finish_configuration_preview(crate::config_backup::prepare_import(text), cx);
        }
    }

    fn choose_configuration_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let language = self.language();
        if !self.begin_configuration_transfer(
            language.localized(copy::backup::EXPORTING),
            TransferPresentation::StatusBar,
            window,
            cx,
        ) {
            return;
        }
        let store = self
            .subscription_store_dir
            .clone()
            .expect("store checked above");
        let initial = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .map_or_else(|| store.clone(), std::path::PathBuf::from);
        let prompt = cx.prompt_for_new_path(&initial, Some("Manis.manis.json"));
        let executor = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            let path = match prompt.await {
                Ok(Ok(Some(path))) => path,
                Ok(Ok(None)) => {
                    this.update(cx, |this, cx| {
                        this.configuration_transfer = ConfigurationTransfer::default();
                        language
                            .localized(copy::backup::EXPORT_CANCELLED)
                            .clone_into(&mut this.status);
                        cx.notify();
                    })
                    .ok();
                    return;
                }
                _ => {
                    this.update(cx, |this, cx| {
                        this.finish_configuration_transfer(
                            language.localized(copy::backup::FILE_ERROR),
                            true,
                            cx,
                        );
                    })
                    .ok();
                    return;
                }
            };
            let output = path.clone();
            let result = executor
                .spawn(async move { crate::config_backup::export_to_file(&store, &path) })
                .await;
            this.update(cx, |this, cx| {
                if result.is_ok() {
                    this.configuration_transfer.output_path = Some(output);
                }
                this.finish_configuration_transfer(
                    language.localized(if result.is_ok() {
                        copy::backup::EXPORTED
                    } else {
                        copy::backup::EXPORT_FAILED
                    }),
                    result.is_err(),
                    cx,
                );
            })
            .ok();
        })
        .detach();
    }

    fn choose_configuration_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let language = self.language();
        if !self.begin_configuration_transfer(
            language.localized(copy::backup::READING),
            TransferPresentation::StatusBar,
            window,
            cx,
        ) {
            return;
        }
        let prompt = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(language.localized(copy::backup::IMPORT).into()),
        });
        let executor = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            let path = match prompt.await {
                Ok(Ok(Some(paths))) if paths.len() == 1 => {
                    paths.into_iter().next().expect("one path")
                }
                Ok(Ok(None)) => {
                    this.update(cx, |this, cx| {
                        this.configuration_transfer = ConfigurationTransfer::default();
                        language
                            .localized(copy::backup::IMPORT_CANCELLED)
                            .clone_into(&mut this.status);
                        cx.notify();
                    })
                    .ok();
                    return;
                }
                _ => {
                    this.update(cx, |this, cx| {
                        this.finish_configuration_transfer(
                            language.localized(copy::backup::FILE_ERROR),
                            true,
                            cx,
                        );
                    })
                    .ok();
                    return;
                }
            };
            let result = executor
                .spawn(async move { crate::config_backup::read_backup(&path) })
                .await;
            this.update_in(cx, |this, window, cx| {
                this.finish_configuration_preview(result, cx);
                if this.configuration_transfer.preview.is_some() {
                    this.open_configuration_transfer_dialog(window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn finish_configuration_preview(
        &mut self,
        result: Result<crate::config_backup::PreparedBackup, crate::config_backup::BackupError>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(preview) => {
                self.configuration_transfer.preview = Some(preview);
                self.configuration_transfer.progress = TransferProgress::Idle;
                self.configuration_transfer.message.clear();
                self.configuration_transfer.failed = false;
                self.language()
                    .localized(copy::backup::PREVIEW)
                    .clone_into(&mut self.status);
                cx.notify();
            }
            Err(_) => self.finish_configuration_transfer(
                self.language().localized(copy::backup::INVALID),
                true,
                cx,
            ),
        }
    }

    fn replace_configuration(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.configuration_transfer.is_busy() || self.configuration_transfer.preview.is_none() {
            return;
        }
        if self.configuration_mutation_busy() {
            self.finish_configuration_transfer(
                self.language().localized(copy::backup::BUSY),
                true,
                cx,
            );
            return;
        }
        let Some(store) = self.subscription_store_dir.clone() else {
            return;
        };
        let Some(preview) = self.configuration_transfer.preview.take() else {
            return;
        };
        let language = self.language();
        self.configuration_transfer.editor = None;
        self.configuration_transfer.progress = TransferProgress::Replacing;
        self.configuration_transfer.failed = false;
        language
            .localized(copy::backup::IMPORTING)
            .clone_into(&mut self.configuration_transfer.message);
        let runtime = self.runtime.clone();
        let system = self.system_proxy.clone();
        let dns = self.tun_dns.clone();
        let previous = self.proxy_mode;
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let (proxy_off, result) = executor
                .spawn(async move {
                    if super::apply_proxy_mode_transition(
                        &runtime,
                        &system,
                        &dns,
                        previous,
                        ProxyMode::Off,
                        crate::system_proxy::ProxyPorts {
                            http: None,
                            socks: None,
                        },
                        language,
                    )
                    .is_err()
                    {
                        return (false, Err(copy::backup::STOP_FAILED));
                    }
                    if runtime.stop_managed().is_err() {
                        return (true, Err(copy::backup::STOP_FAILED));
                    }
                    (true, Ok(crate::config_backup::restore(&store, &preview)))
                })
                .await;
            this.update(cx, |this, cx| {
                if proxy_off {
                    this.proxy_mode = ProxyMode::Off;
                    this.live_generation = this.live_generation.wrapping_add(1);
                    this.live_runtime = None;
                    this.controller = mihomo::ControllerState::Disconnected;
                    this.live_status = mihomo::LiveStreamStatus::default();
                    this.active_connections.clear();
                }
                match result {
                    Ok(Ok(imported)) => {
                        this.configuration_transfer.output_path = Some(imported.backup_dir);
                        language
                            .localized(copy::backup::IMPORTED)
                            .clone_into(&mut this.status);
                        cx.restart();
                    }
                    Ok(Err(error)) => {
                        this.configuration_transfer.output_path = error.backup_dir;
                        this.finish_configuration_transfer(
                            language.localized(if error.rollback_failed {
                                copy::backup::ROLLBACK_FAILED
                            } else {
                                copy::backup::RESTORE_FAILED
                            }),
                            true,
                            cx,
                        );
                    }
                    Err(message) => {
                        this.finish_configuration_transfer(language.localized(message), true, cx);
                    }
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn configuration_transfer_panel(
        &self,
        theme: Theme,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let language = self.language();
        let disabled = self.configuration_transfer.active || self.subscription_store_dir.is_none();
        panel_surface("configuration-transfer", compact, theme)
            .child(section_heading(
                language.localized(copy::backup::TITLE),
                language.localized(copy::backup::DETAIL),
                None,
                theme,
            ))
            .child(
                div()
                    .mt(Space::Md.px())
                    .flex()
                    .flex_wrap()
                    .gap(Space::Sm.px())
                    .child(
                        style_action_button(
                            Button::new("configuration-export")
                                .label(language.localized(copy::backup::EXPORT))
                                .disabled(disabled),
                            ActionRole::Secondary,
                            ControlSize::Standard,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.choose_configuration_export(window, cx);
                        })),
                    )
                    .child(
                        style_action_button(
                            Button::new("configuration-import")
                                .label(language.localized(copy::backup::IMPORT))
                                .disabled(disabled),
                            ActionRole::Secondary,
                            ControlSize::Standard,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.choose_configuration_import(window, cx);
                        })),
                    )
                    .child(
                        style_action_button(
                            Button::new("configuration-edit")
                                .label(language.localized(copy::backup::EDIT))
                                .disabled(disabled),
                            ActionRole::Secondary,
                            ControlSize::Standard,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.edit_configuration(window, cx);
                        })),
                    ),
            )
            .child(
                div()
                    .mt(Space::Md.px())
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_secondary)
                    .child(language.localized(copy::backup::SENSITIVE)),
            )
            .child(
                div()
                    .mt(Space::Xs.px())
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_secondary)
                    .child(language.localized(copy::backup::EXCLUDED)),
            )
            .when_some(
                self.subscription_store_dir
                    .as_deref()
                    .and_then(|store| crate::config_backup::backup_root(store).ok())
                    .filter(|path| path.is_dir()),
                |panel, store| {
                    panel.child(
                        style_action_button(
                            Button::new("configuration-backups")
                                .label(language.localized(copy::backup::SHOW_BACKUPS)),
                            ActionRole::Secondary,
                            ControlSize::Compact,
                        )
                        .mt(Space::Sm.px())
                        .on_click(move |_, _, cx| cx.reveal_path(&store)),
                    )
                },
            )
    }

    fn configuration_mutation_busy(&self) -> bool {
        self.source_refresh_busy()
            || matches!(
                self.proxy_source_editor.feedback,
                SubscriptionFeedback::Importing(_)
            )
            || self.managed_policies.mutation_state.is_busy()
            || self.managed_policies.active_benchmark_generation.is_some()
            || self.routing_apply_state.is_busy()
            || self.kernel_switch_state.is_busy()
            || self.mihomo_core_update_state.is_busy()
            || self.proxy_mode_busy.is_some()
            || self.routing_mode_busy.is_some()
            || self.global_selection_busy.is_some()
            || self.policy_selection_busy.is_some()
            || matches!(self.controller, mihomo::ControllerState::Connecting { .. })
    }

    fn begin_configuration_transfer(
        &mut self,
        message: &'static str,
        presentation: TransferPresentation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.configuration_transfer.active {
            return false;
        }
        let error = if self.subscription_store_dir.is_none() {
            Some(copy::backup::NO_STORE)
        } else if self.configuration_mutation_busy() {
            Some(copy::backup::BUSY)
        } else {
            None
        };
        self.configuration_transfer = ConfigurationTransfer {
            active: error.is_none() || presentation == TransferPresentation::Dialog,
            presentation,
            progress: if error.is_none() {
                TransferProgress::Preparing
            } else {
                TransferProgress::Idle
            },
            failed: error.is_some(),
            message: error
                .map_or(message, |copy| self.language().localized(copy))
                .to_owned(),
            ..ConfigurationTransfer::default()
        };
        if self.configuration_transfer.presentation == TransferPresentation::Dialog {
            self.open_configuration_transfer_dialog(window, cx);
        } else {
            self.configuration_transfer
                .message
                .clone_into(&mut self.status);
        }
        cx.notify();
        error.is_none()
    }

    fn open_configuration_transfer_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.configuration_transfer.presentation = TransferPresentation::Dialog;
        let app = cx.entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            app.update(cx, |this, cx| {
                this.configuration_transfer_dialog(dialog, window, cx)
            })
        });
    }

    fn finish_configuration_transfer(
        &mut self,
        message: &'static str,
        failed: bool,
        cx: &mut Context<Self>,
    ) {
        self.configuration_transfer.progress = TransferProgress::Idle;
        if self.configuration_transfer.presentation == TransferPresentation::StatusBar {
            self.configuration_transfer.active = false;
        }
        self.configuration_transfer.failed = failed;
        message.clone_into(&mut self.configuration_transfer.message);
        message.clone_into(&mut self.status);
        cx.notify();
    }

    fn configuration_transfer_body(
        &self,
        theme: Theme,
        language: Language,
        window: &Window,
    ) -> Stateful<Div> {
        let state = &self.configuration_transfer;
        let mut body = div()
            .id("configuration-transfer-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(Space::Lg.px())
            .flex()
            .flex_col()
            .gap(Space::Md.px());
        if state.preview.is_none() && state.editor.is_some() {
            body = body.child(self.configuration_editor_body(theme, language, window));
        }
        if let Some(preview) = &state.preview {
            let summary = preview.summary();
            body = body
                .child(
                    div()
                        .text_size(TextRole::Label.size())
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(language.localized(copy::backup::PREVIEW)),
                )
                .children(
                    [
                        (copy::backup::SUBSCRIPTIONS, summary.subscriptions),
                        (copy::backup::NODES, summary.single_nodes),
                        (copy::backup::GROUPS, summary.policy_groups),
                        (copy::backup::RULE_SOURCES, summary.rule_sources),
                        (copy::backup::MANUAL_RULES, summary.manual_rules),
                    ]
                    .into_iter()
                    .map(|(label, count)| {
                        div()
                            .flex()
                            .justify_between()
                            .gap(Space::Md.px())
                            .text_size(TextRole::Label.size())
                            .child(language.localized(label))
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(count.to_string()),
                            )
                    }),
                )
                .child(
                    div()
                        .border_t_1()
                        .border_color(theme.outline_subtle)
                        .pt(Space::Md.px())
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .text_color(theme.status_warning)
                        .child(language.localized(copy::backup::REPLACE_NOTICE)),
                )
                .child(
                    div()
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .text_color(theme.text_secondary)
                        .child(language.localized(copy::backup::EXCLUDED)),
                );
        }
        if !state.message.is_empty() {
            body = body.child(
                div()
                    .id("configuration-transfer-message")
                    .role(Role::Status)
                    .text_size(TextRole::Label.size())
                    .line_height(TextRole::Label.line_height())
                    .text_color(if state.failed {
                        theme.status_error
                    } else {
                        theme.text_secondary
                    })
                    .child(state.message.clone()),
            );
        }
        body
    }

    fn configuration_transfer_footer(
        &self,
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> Div {
        let state = &self.configuration_transfer;
        let busy = state.is_busy();
        dialog_footer_surface(theme)
            .flex_wrap()
            .when_some(state.output_path.clone(), |footer, path| {
                footer.child(
                    style_action_button(
                        Button::new("configuration-transfer-show")
                            .label(language.localized(copy::backup::SHOW_FILE)),
                        ActionRole::Secondary,
                        ControlSize::Standard,
                    )
                    .on_click(move |_, _, cx| cx.reveal_path(&path)),
                )
            })
            .child(
                style_action_button(
                    Button::new("configuration-transfer-close")
                        .label(if state.preview.is_some() || state.editor.is_some() {
                            language.message(Message::Cancel)
                        } else {
                            language.localized(copy::backup::DONE)
                        })
                        .disabled(busy),
                    ActionRole::Secondary,
                    ControlSize::Standard,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.cancel_configuration_transfer(window, cx);
                })),
            )
            .children(self.configuration_editor_actions(language, cx))
            .when(state.preview.is_some(), |footer| {
                footer.child(
                    style_action_button(
                        Button::new("configuration-transfer-replace")
                            .label(language.localized(copy::backup::REPLACE))
                            .loading(busy)
                            .disabled(busy),
                        ActionRole::Primary,
                        ControlSize::Standard,
                    )
                    .on_click(
                        cx.listener(|this, _, window, cx| this.replace_configuration(window, cx)),
                    ),
                )
            })
    }

    fn cancel_configuration_transfer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.configuration_transfer.is_busy() {
            self.configuration_transfer = ConfigurationTransfer::default();
            self.language()
                .localized(copy::backup::IMPORT_CANCELLED)
                .clone_into(&mut self.status);
            window.close_dialog(cx);
            cx.notify();
        }
    }

    fn configuration_transfer_dialog(
        &self,
        dialog: Dialog,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Dialog {
        let theme = self.theme();
        let language = self.language();
        let state = &self.configuration_transfer;
        let busy = state.is_busy();
        let body = self.configuration_transfer_body(theme, language, window);
        let editing = state.editor.is_some();
        let footer = self.configuration_transfer_footer(theme, language, cx);
        let app = cx.entity();
        surface_dialog(dialog, theme)
            .width(px((window.viewport_size().width.as_f32() - 32.0)
                .clamp(300.0, if editing { 920.0 } else { 560.0 })))
            .max_h(px(
                (window.viewport_size().height.as_f32() - 32.0).max(280.0)
            ))
            .margin_top(px(if editing {
                16.0
            } else {
                ((window.viewport_size().height.as_f32() - 440.0) / 2.0).max(16.0)
            }))
            .overlay(true)
            .overlay_closable(!busy)
            .keyboard(!busy)
            .close_button(false)
            .title(
                dialog_header_surface(theme)
                    .text_size(TextRole::SectionTitle.size())
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(language.localized(if editing {
                        copy::backup::EDIT
                    } else {
                        copy::backup::TITLE
                    })),
            )
            .child(body)
            .footer(footer)
            .on_close(move |_, _, cx| {
                app.update(cx, |this, cx| {
                    if !this.configuration_transfer.is_busy() {
                        this.configuration_transfer = ConfigurationTransfer::default();
                        cx.notify();
                    }
                });
            })
    }
}
