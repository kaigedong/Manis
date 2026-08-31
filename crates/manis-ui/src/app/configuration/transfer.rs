use gpui::{
    Context, Div, Entity, FontWeight, ParentElement, Role, Stateful, Styled, Window, div,
    prelude::*, px,
};
use gpui_component::{Disableable, WindowExt as _, button::Button, dialog::Dialog};
use manis_core::ProxyMode;

use crate::app::{ManisApp, SubscriptionFeedback};
use crate::{
    components::{
        ActionRole, dialog_footer_surface, dialog_header_surface, section_heading,
        style_action_button, surface_dialog,
    },
    localization::{Language, Message, copy},
    mihomo::{self},
    theme::{ControlSize, Space, TextRole, Theme},
};

use super::panel_surface;

#[derive(Default)]
pub(super) enum TransferProgress {
    #[default]
    Idle,
    Preparing,
    Replacing,
}

#[derive(Default, PartialEq, Eq)]
pub(super) enum TransferPresentation {
    #[default]
    Dialog,
    StatusBar,
}

#[derive(Default)]
pub(in crate::app) struct ConfigurationTransfer {
    pub(in crate::app) active: bool,
    pub(super) progress: TransferProgress,
    pub(super) presentation: TransferPresentation,
    pub(super) preview: Option<crate::config_backup::PreparedBackup>,
    pub(super) editor: Option<Entity<gpui_component::input::EditorState>>,
    pub(super) message: String,
    pub(super) failed: bool,
    pub(super) output_path: Option<std::path::PathBuf>,
}

impl ConfigurationTransfer {
    pub(in crate::app) fn is_busy(&self) -> bool {
        !matches!(self.progress, TransferProgress::Idle)
    }

    pub(in crate::app) fn is_replacing(&self) -> bool {
        matches!(self.progress, TransferProgress::Replacing)
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

    pub(super) fn finish_configuration_preview(
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
                    if crate::app::apply_proxy_mode_transition(
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

    pub(super) fn configuration_transfer_panel(
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
                        .when(disabled, gpui::Styled::cursor_default)
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
                        .when(disabled, gpui::Styled::cursor_default)
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
                        .when(disabled, gpui::Styled::cursor_default)
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

    pub(super) fn configuration_mutation_busy(&self) -> bool {
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

    pub(super) fn begin_configuration_transfer(
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

    pub(super) fn open_configuration_transfer_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.configuration_transfer.presentation = TransferPresentation::Dialog;
        let app = cx.entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            app.update(cx, |this, cx| {
                this.configuration_transfer_dialog(dialog, window, cx)
            })
        });
    }

    pub(super) fn finish_configuration_transfer(
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
                .when(busy, gpui::Styled::cursor_default)
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
                    .when(busy, gpui::Styled::cursor_default)
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

#[cfg(test)]
mod tests;
