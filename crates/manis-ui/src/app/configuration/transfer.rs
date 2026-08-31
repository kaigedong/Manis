#[derive(Default)]
enum TransferProgress {
    #[default]
    Idle,
    Preparing,
    Replacing,
}

#[derive(Default)]
pub(super) struct ConfigurationTransfer {
    pub(super) active: bool,
    progress: TransferProgress,
    preview: Option<crate::config_backup::PreparedBackup>,
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
        let (_, window_cx) = cx.add_window_view(|window, cx| {
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
        window_cx.update(|window, cx| {
            app.update(cx, |app, cx| app.choose_configuration_export(window, cx));
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
            assert!(!app.configuration_transfer.failed);
            assert!(!app.configuration_transfer.is_busy());
            assert_eq!(
                app.configuration_transfer.output_path.as_ref(),
                Some(&output)
            );
        });
        std::fs::remove_dir_all(root).expect("remove fixture");
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
        let (_, window_cx) = cx.add_window_view(|window, cx| {
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
        window_cx.update(|window, cx| {
            app.update(cx, |app, cx| app.choose_configuration_import(window, cx));
        });
        assert!(cx.did_prompt_for_paths());
        cx.simulate_path_prompt_response(|options| {
            assert!(options.files && !options.directories && !options.multiple);
            None
        });
        cx.run_until_parked();
        app.read_with(cx, |app, _| assert!(!app.configuration_transfer.active));
        assert!(
            !store.exists(),
            "cancelling must not create or modify configuration"
        );
    }

    #[gpui::test]
    fn invalid_clipboard_import_never_changes_configuration(cx: &mut gpui::TestAppContext) {
        cx.update(crate::init);
        let store = unique_temp_store("manis-transfer-invalid");
        crate::mihomo::save_routing_mode_in(&store, manis_core::RoutingMode::Direct)
            .expect("fixture routing");
        let original = std::fs::read(store.join("routing.mode")).expect("read fixture");
        cx.write_to_clipboard(ClipboardItem::new_string(
            "not a Manis configuration".to_owned(),
        ));
        let mut app = None;
        let (_, window_cx) = cx.add_window_view(|window, cx| {
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
        window_cx.update(|window, cx| {
            app.update(cx, |app, cx| app.paste_configuration_import(window, cx));
        });
        cx.run_until_parked();
        app.read_with(cx, |app, _| {
            assert!(app.configuration_transfer.failed);
            assert!(!app.configuration_transfer.is_busy());
            assert!(app.configuration_transfer.preview.is_none());
            assert_eq!(app.routing_mode, manis_core::RoutingMode::Direct);
        });
        assert_eq!(
            std::fs::read(store.join("routing.mode")).expect("fixture remains"),
            original
        );
        std::fs::remove_dir_all(store.parent().expect("fixture root")).expect("remove fixture");
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
                    this.update_in(cx, |this, window, cx| {
                        this.configuration_transfer = ConfigurationTransfer::default();
                        window.close_dialog(cx);
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
        if !self.begin_configuration_transfer(language.localized(copy::backup::READING), window, cx)
        {
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
                    this.update_in(cx, |this, window, cx| {
                        this.configuration_transfer = ConfigurationTransfer::default();
                        window.close_dialog(cx);
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
            this.update(cx, |this, cx| this.finish_configuration_preview(result, cx))
                .ok();
        })
        .detach();
    }

    fn paste_configuration_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let language = self.language();
        if !self.begin_configuration_transfer(language.localized(copy::backup::READING), window, cx)
        {
            return;
        }
        let Some(text) = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .filter(|text| !text.trim().is_empty())
        else {
            self.finish_configuration_transfer(language.localized(copy::backup::NO_TEXT), true, cx);
            return;
        };
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move { crate::config_backup::prepare_import(&text) })
                .await;
            this.update(cx, |this, cx| this.finish_configuration_preview(result, cx))
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
                                .border_1()
                                .border_color(theme.outline_subtle)
                                .bg(theme.surface_low)
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
                                .border_1()
                                .border_color(theme.outline_subtle)
                                .bg(theme.surface_low)
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
                            Button::new("configuration-paste")
                                .label(language.localized(copy::backup::PASTE))
                                .border_1()
                                .border_color(theme.outline_subtle)
                                .bg(theme.surface_low)
                                .disabled(disabled),
                            ActionRole::Secondary,
                            ControlSize::Standard,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.paste_configuration_import(window, cx);
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
                            ActionRole::Quiet,
                            ControlSize::Compact,
                        )
                        .mt(Space::Sm.px())
                        .on_click(move |_, _, cx| {
                            cx.reveal_path(&store);
                        }),
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
            || self.app_update_state.is_busy()
            || self.proxy_mode_busy.is_some()
            || self.routing_mode_busy.is_some()
            || self.global_selection_busy.is_some()
            || self.policy_selection_busy.is_some()
            || matches!(self.controller, mihomo::ControllerState::Connecting { .. })
    }

    fn begin_configuration_transfer(
        &mut self,
        message: &'static str,
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
            active: true,
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
        let app = cx.entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            app.update(cx, |this, cx| {
                this.configuration_transfer_dialog(dialog, window, cx)
            })
        });
        cx.notify();
        error.is_none()
    }

    fn finish_configuration_transfer(
        &mut self,
        message: &'static str,
        failed: bool,
        cx: &mut Context<Self>,
    ) {
        self.configuration_transfer.progress = TransferProgress::Idle;
        self.configuration_transfer.failed = failed;
        message.clone_into(&mut self.configuration_transfer.message);
        message.clone_into(&mut self.status);
        cx.notify();
    }

    fn configuration_transfer_body(&self, theme: Theme, language: Language) -> Stateful<Div> {
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
        let body = self.configuration_transfer_body(theme, language);
        let footer = dialog_footer_surface(theme)
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
                        .label(if state.preview.is_some() {
                            language.message(Message::Cancel)
                        } else {
                            language.localized(copy::backup::DONE)
                        })
                        .disabled(busy),
                    ActionRole::Secondary,
                    ControlSize::Standard,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    if !this.configuration_transfer.is_busy() {
                        this.configuration_transfer = ConfigurationTransfer::default();
                        window.close_dialog(cx);
                        cx.notify();
                    }
                })),
            )
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
            });
        let app = cx.entity();
        surface_dialog(dialog, theme)
            .width(px(
                (window.viewport_size().width.as_f32() - 32.0).clamp(300.0, 560.0)
            ))
            .max_h(px(
                (window.viewport_size().height.as_f32() - 32.0).max(280.0)
            ))
            .margin_top(px(
                ((window.viewport_size().height.as_f32() - 440.0) / 2.0).max(16.0)
            ))
            .overlay(true)
            .overlay_closable(!busy)
            .keyboard(!busy)
            .close_button(false)
            .title(
                dialog_header_surface(theme)
                    .text_size(TextRole::SectionTitle.size())
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(language.localized(copy::backup::TITLE)),
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
