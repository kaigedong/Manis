use gpui::{ClipboardItem, Context, Div, ParentElement, Styled, Window, div, prelude::*, px};
use gpui_component::{Disableable, button::Button};

use crate::app::ManisApp;
use crate::{
    components::{ActionRole, action_button},
    diagnostics::{LogLevel, record_event},
    localization::{Language, copy},
    theme::{ControlSize, Space, TextRole, Theme},
};

use super::transfer::{TransferPresentation, TransferProgress};

impl ManisApp {
    /// Opens the actual configuration editor against an isolated fixture store.
    #[cfg(feature = "snapshot-fixtures")]
    #[doc(hidden)]
    pub fn show_configuration_editor_fixture(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.runtime.is_fixture() {
            self.edit_configuration(window, cx);
        }
    }

    pub(super) fn edit_configuration(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let language = self.language();
        if !self.begin_configuration_transfer(
            language.localized(copy::backup::LOADING_CURRENT),
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
        let executor = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = executor
                .spawn(async move { crate::config_backup::read_configuration_for_editing(&store) })
                .await;
            this.update_in(cx, |this, window, cx| match result {
                Ok(text) => {
                    let editor = cx.new(|cx| {
                        gpui_component::input::EditorState::new(window, cx)
                            .language("toml")
                            .soft_wrap(true)
                            .default_value(text)
                    });
                    this.configuration_transfer.editor = Some(editor.clone());
                    this.configuration_transfer.progress = TransferProgress::Idle;
                    this.configuration_transfer.message.clear();
                    language
                        .localized(copy::backup::EDIT)
                        .clone_into(&mut this.status);
                    this.open_configuration_transfer_dialog(window, cx);
                    editor.update(cx, |editor, cx| editor.focus(window, cx));
                    cx.notify();
                }
                Err(error) => {
                    record_event(
                        LogLevel::Error,
                        "configuration.edit.load_failed",
                        error.to_string(),
                    );
                    this.finish_configuration_transfer(
                        language.localized(copy::backup::EDIT_FAILED),
                        true,
                        cx,
                    );
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn preview_configuration_edits(&mut self, cx: &mut Context<Self>) {
        if self.configuration_transfer.is_busy() || self.configuration_transfer.preview.is_some() {
            return;
        }
        let Some(editor) = &self.configuration_transfer.editor else {
            return;
        };
        let text = editor.read(cx).value();
        self.configuration_transfer.progress = TransferProgress::Preparing;
        self.configuration_transfer.failed = false;
        self.language()
            .localized(copy::backup::READING)
            .clone_into(&mut self.configuration_transfer.message);
        self.configuration_transfer
            .message
            .clone_into(&mut self.status);
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move { crate::config_backup::prepare_import(&text) })
                .await;
            this.update(cx, |this, cx| this.finish_configuration_preview(result, cx))
                .ok();
        })
        .detach();
        cx.notify();
    }

    pub(super) fn resume_configuration_editing(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.configuration_transfer.is_busy() {
            return;
        }
        if let Some(editor) = &self.configuration_transfer.editor {
            editor.update(cx, |editor, cx| editor.focus(window, cx));
            self.configuration_transfer.preview = None;
            self.configuration_transfer.failed = false;
            self.configuration_transfer.message.clear();
            self.language()
                .localized(copy::backup::EDIT)
                .clone_into(&mut self.status);
            cx.notify();
        }
    }

    pub(super) fn copy_configuration_to_clipboard(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = &self.configuration_transfer.editor else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(
            editor.read(cx).value().to_string(),
        ));
        self.language()
            .localized(copy::backup::COPIED)
            .clone_into(&mut self.status);
        cx.notify();
    }

    pub(super) fn replace_configuration_from_clipboard(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            self.language()
                .localized(copy::backup::CLIPBOARD_EMPTY)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        };
        let Some(editor) = &self.configuration_transfer.editor else {
            return;
        };
        editor.update(cx, |editor, cx| {
            editor.set_value(text, window, cx);
            editor.focus(window, cx);
        });
        self.configuration_transfer.preview = None;
        self.configuration_transfer.failed = false;
        self.configuration_transfer.message.clear();
        self.language()
            .localized(copy::backup::PASTED)
            .clone_into(&mut self.status);
        cx.notify();
    }

    pub(super) fn configuration_editor_body(
        &self,
        theme: Theme,
        language: Language,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(Space::Md.px())
            .text_size(TextRole::Metadata.size())
            .line_height(TextRole::Metadata.line_height())
            .text_color(theme.text_secondary)
            .child(language.localized(copy::backup::EDIT_DETAIL))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(Space::Sm.px())
                    .child(
                        action_button(
                            "configuration-editor-copy",
                            language.localized(copy::backup::COPY_ALL),
                            ActionRole::Secondary,
                            ControlSize::Compact,
                        )
                        .disabled(self.configuration_transfer.is_busy())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.copy_configuration_to_clipboard(cx);
                        })),
                    )
                    .child(
                        action_button(
                            "configuration-editor-paste",
                            language.localized(copy::backup::PASTE_REPLACE),
                            ActionRole::Secondary,
                            ControlSize::Compact,
                        )
                        .disabled(self.configuration_transfer.is_busy())
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.replace_configuration_from_clipboard(window, cx);
                        })),
                    ),
            )
            .when_some(
                self.configuration_transfer.editor.as_ref(),
                |body, editor| {
                    body.child(
                        gpui_component::input::Editor::new(editor)
                            .aria_label(language.localized(copy::backup::EDITOR_TITLE))
                            .h(px((window.viewport_size().height.as_f32() - 320.0)
                                .clamp(160.0, 560.0)))
                            .disabled(self.configuration_transfer.is_busy()),
                    )
                },
            )
            .child(language.localized(copy::backup::SENSITIVE))
    }

    pub(super) fn configuration_editor_actions(
        &self,
        language: Language,
        cx: &mut Context<Self>,
    ) -> Option<Button> {
        self.configuration_transfer.editor.as_ref()?;
        let preview = self.configuration_transfer.preview.is_some();
        Some(
            action_button(
                "configuration-editor-preview",
                language.localized(if preview {
                    copy::backup::BACK_TO_EDIT
                } else {
                    copy::backup::VALIDATE
                }),
                if preview {
                    ActionRole::Secondary
                } else {
                    ActionRole::Primary
                },
                ControlSize::Standard,
            )
            .disabled(self.configuration_transfer.is_busy())
            .when(
                self.configuration_transfer.is_busy(),
                gpui::Styled::cursor_default,
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                if preview {
                    this.resume_configuration_editing(window, cx);
                } else {
                    this.preview_configuration_edits(cx);
                }
            })),
        )
    }
}
