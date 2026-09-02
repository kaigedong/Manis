use gpui::{
    AnyElement, Context, Div, Entity, Focusable, FontWeight, ParentElement, Role, Stateful, Styled,
    Window, div, prelude::*, px,
};
use gpui_component::{Disableable, Selectable, button::Button, checkbox::Checkbox, dialog::Dialog};

use crate::app::{
    ImportedSubscriptionState, ManisApp, ProxySourceEditorKind, SourceMutation, SourceRuntimeApply,
    SubscriptionFeedback,
};
use crate::{
    components::{
        ActionRole, action_button, dialog_footer_surface, dialog_header_surface, empty_state,
        row_action_button, section_heading, style_action_button, surface_dialog,
    },
    diagnostics::{UiEvent, trace_ui},
    localization::{CountNoun, Language, Message, copy},
    mihomo::{self, RemoteSourceRefreshInterval, SubscriptionStoreError},
    subscription::{SourceKind, validate_single_node_preview, validate_subscription_preview},
    subscription_input::SubscriptionTextInput,
    theme::{ControlSize, Radius, Space, TextRole, Theme},
};

use super::{
    ProxySourceEditorActivity, ProxySourceEditorInputs, ProxySourceEditorView,
    SubscriptionCardActivity, SubscriptionCardPresentation, field_label, panel_surface,
    refresh_interval_label, source_kind_label, source_update_label,
};

mod cards;
mod editor;
mod model;
mod workflow;
pub(in crate::app) use model::{ProxySourceEditorState, ProxySourceEditorTarget};

impl ManisApp {
    pub(super) fn source_panel(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Div {
        let language = self.language();
        let saved_source_count = self.imported_subscriptions.len() + self.saved_single_nodes.len();
        let add_action = action_button(
            "configuration-add-proxy-source",
            language.localized(copy::configuration::ADD_SOURCE),
            ActionRole::Primary,
            ControlSize::Compact,
        )
        .disabled(self.proxy_source_editor.is_importing())
        .when(
            self.proxy_source_editor.is_importing(),
            gpui::Styled::cursor_default,
        )
        .on_click(cx.listener(|this, _, window, cx| {
            this.open_new_subscription_editor(cx);
            this.open_proxy_source_dialog(window, cx);
        }));

        let panel = panel_surface("configuration-source", compact, theme)
            .child(section_heading(
                language.localized(copy::configuration::PROXY_SOURCES),
                language.count(CountNoun::Source, saved_source_count),
                Some(add_action.into_any_element()),
                theme,
            ))
            .when_some(self.source_store_error, |panel, error| {
                panel.child(Self::subscription_error(
                    language.localized(copy::configuration::SOME_LOCAL_SOURCES_COULD_NOT_BE_RESTORED),
                    copy::configuration::subscription_store_error(language, error).to_owned(),
                    Some(language.localized(copy::configuration::OTHER_SAFELY_READABLE_SOURCES_ARE_KEPT_CHECK_THE_USER_DATA)),
                    theme,
                ))
            })
            .child(
                div()
                    .mt(Space::Lg.px())
                    .pt(Space::Md.px())
                    .border_t_1()
                    .border_color(theme.outline_subtle)
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(TextRole::Label.weight())
                            .child(language.localized(copy::common::SAVED)),
                    ),
            )
            .when(saved_source_count == 0, |panel| {
                panel.child(
                    empty_state(
                        language.localized(copy::configuration::NO_PROXY_SOURCES),
                        language.localized(copy::configuration::ADD_A_SUBSCRIPTION_OR_A_SINGLE_NODE_SOURCE),
                        Some(
                        action_button(
                            "configuration-empty-add-proxy-source",
                            language.localized(copy::configuration::ADD_SOURCE),
                            ActionRole::Primary,
                            ControlSize::Compact,
                        )
                        .disabled(self.proxy_source_editor.is_importing())
                        .when(self.proxy_source_editor.is_importing(), gpui::Styled::cursor_default)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_new_subscription_editor(cx);
                            this.open_proxy_source_dialog(window, cx);
                        }))
                            .into_any_element(),
                        ),
                        theme,
                    )
                    .mt(Space::Md.px()),
                )
            })
            .child(self.imported_subscription_cards(theme, cx))
            .child(self.saved_single_node_cards(theme, cx));
        div().w_full().child(panel)
    }

    fn subscription_error(
        title: &'static str,
        message: String,
        recovery: Option<&'static str>,
        theme: Theme,
    ) -> Div {
        div()
            .mt(Space::Md.px())
            .p(Space::Md.px())
            .rounded(Radius::Pane.px())
            .border_1()
            .border_color(theme.outline_strong)
            .bg(theme.surface_low)
            .child(
                div()
                    .text_size(TextRole::Label.size())
                    .line_height(TextRole::Label.line_height())
                    .font_weight(TextRole::Label.weight())
                    .child(title),
            )
            .child(
                div()
                    .mt(Space::Xs.px())
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_secondary)
                    .child(message),
            )
            .when_some(recovery, |card, recovery| {
                card.child(
                    div()
                        .mt(Space::Sm.px())
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .text_color(theme.text_tertiary)
                        .child(recovery),
                )
            })
    }
}

#[cfg(all(test, not(windows)))]
mod subscription_import_concurrency_tests {
    use gpui::AppContext as _;

    use super::ProxySourceEditorTarget;
    use crate::{
        app::{
            ImportSubscriptionError, ImportedSubscription, ImportedSubscriptionState, ManisApp,
            ProxySourceEditorKind, SubscriptionFeedback,
        },
        mihomo::{RemoteSourceRefreshInterval, StoredSubscription, SubscriptionStoreError},
        subscription::SourceKind,
    };

    fn source_fixture() -> ImportedSubscription {
        ImportedSubscription {
            id: "subscription:import-concurrency".to_owned(),
            name: "Existing source".to_owned(),
            source: manis_profile::SecretUrl::parse_subscription("http://127.0.0.1:1/subscription")
                .expect("fixture URL"),
            enabled: true,
            state: ImportedSubscriptionState::Ready(SourceKind::HttpSubscription),
            providers: Vec::new(),
            generation: 7,
            refresh_interval: RemoteSourceRefreshInterval::Hourly,
            last_successful_update_unix_secs: 0,
        }
    }

    fn app_fixture() -> ManisApp {
        let store =
            std::env::temp_dir().join(format!("manis-import-concurrency-{}", std::process::id()));
        let mut app =
            ManisApp::with_fixture_controller_and_subscription_store("http://127.0.0.1:1", store);
        app.imported_subscriptions = vec![source_fixture()];
        app
    }

    #[gpui::test]
    fn scheduled_refresh_waits_for_editor_import(cx: &mut gpui::TestAppContext) {
        let app = cx.new(|_| app_fixture());
        app.update(cx, |app, cx| {
            app.proxy_source_editor.import_generation = 41;
            app.proxy_source_editor.feedback =
                SubscriptionFeedback::Importing(SourceKind::HttpsSubscription);

            app.refresh_next_due_source(cx);

            assert_eq!(app.proxy_source_editor.import_generation, 41);
            assert_eq!(
                app.imported_subscriptions[0].state,
                ImportedSubscriptionState::Ready(SourceKind::HttpSubscription)
            );
            assert!(app.rule_sources.refresh_retry_not_before.is_empty());
        });
    }

    #[gpui::test]
    fn source_toggle_preserves_editor_completion_token(cx: &mut gpui::TestAppContext) {
        let app = cx.new(|_| app_fixture());
        app.update(cx, |app, cx| {
            app.proxy_source_editor.import_generation = 41;
            app.set_subscription_enabled("subscription:import-concurrency", false, cx);
            assert!(matches!(
                app.imported_subscriptions[0].state,
                ImportedSubscriptionState::Refreshing(_)
            ));
            app.proxy_source_editor.feedback =
                SubscriptionFeedback::Importing(SourceKind::SingleNode);
            app.finish_single_node_import(
                41,
                crate::subscription::SubscriptionPreview {
                    kind: SourceKind::SingleNode,
                    nodes: Vec::new(),
                },
                Err(SubscriptionStoreError::StoreUnavailable),
                cx,
            );
            assert!(matches!(
                app.proxy_source_editor.feedback,
                SubscriptionFeedback::StoreFailed(SubscriptionStoreError::StoreUnavailable)
            ));
        });
    }

    #[gpui::test]
    fn edited_source_ignores_an_older_refresh_callback(cx: &mut gpui::TestAppContext) {
        let app = cx.new(|_| app_fixture());
        app.update(cx, |app, cx| {
            let previous = app.imported_subscriptions[0].clone();
            app.merge_imported_subscription(
                StoredSubscription {
                    id: previous.id.clone(),
                    name: "Edited source".to_owned(),
                    source: previous.source,
                    enabled: false,
                    refresh_interval: RemoteSourceRefreshInterval::Manual,
                    last_successful_update_unix_secs: 20,
                    proxy_server_nameservers: Vec::new(),
                },
                &[],
                42,
                SourceKind::HttpSubscription,
            );
            app.finish_subscription_refresh(
                &previous.id,
                previous.generation,
                SourceKind::HttpSubscription,
                Err(ImportSubscriptionError::Store(
                    SubscriptionStoreError::StoreUnavailable,
                )),
                cx,
            );
            assert_eq!(
                app.imported_subscriptions[0].state,
                ImportedSubscriptionState::None
            );
            assert_eq!(app.imported_subscriptions[0].generation, 42);
        });
    }

    #[gpui::test]
    fn closing_or_reopening_editor_preserves_an_inflight_import(cx: &mut gpui::TestAppContext) {
        let app = cx.new(|_| app_fixture());
        app.update(cx, |app, cx| {
            app.proxy_source_editor.feedback =
                SubscriptionFeedback::Importing(SourceKind::HttpsSubscription);
            app.proxy_source_editor.target = ProxySourceEditorTarget::Subscription {
                id: "being-edited".to_owned(),
            };

            app.close_subscription_editor(cx);
            app.open_new_subscription_editor(cx);

            assert!(matches!(
                app.proxy_source_editor.feedback,
                SubscriptionFeedback::Importing(SourceKind::HttpsSubscription)
            ));
            assert_eq!(
                app.proxy_source_editor.target.editing_id(),
                Some("being-edited")
            );
        });
    }

    #[gpui::test]
    fn closing_editor_preserves_kind_and_generation_for_the_next_draft(
        cx: &mut gpui::TestAppContext,
    ) {
        let app = cx.new(|_| app_fixture());
        app.update(cx, |app, cx| {
            app.proxy_source_editor.target = ProxySourceEditorTarget::SingleNode {
                id: "saved-node".to_owned(),
            };
            app.proxy_source_editor.import_generation = 41;

            app.close_subscription_editor(cx);

            assert_eq!(
                app.proxy_source_editor.target.kind(),
                ProxySourceEditorKind::SingleNode
            );
            assert!(app.proxy_source_editor.target.editing_id().is_none());
            assert_eq!(app.proxy_source_editor.import_generation, 41);
        });
    }

    #[gpui::test]
    fn older_editor_completion_does_not_replace_current_import(cx: &mut gpui::TestAppContext) {
        let app = cx.new(|_| app_fixture());
        app.update(cx, |app, cx| {
            app.proxy_source_editor.import_generation = 42;
            app.proxy_source_editor.feedback =
                SubscriptionFeedback::Importing(SourceKind::HttpsSubscription);
            app.finish_subscription_import(
                41,
                SourceKind::HttpsSubscription,
                Err(ImportSubscriptionError::Store(
                    SubscriptionStoreError::StoreUnavailable,
                )),
                cx,
            );
            assert!(matches!(
                app.proxy_source_editor.feedback,
                SubscriptionFeedback::Importing(SourceKind::HttpsSubscription)
            ));
        });
    }
}
