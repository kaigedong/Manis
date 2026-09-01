use gpui_component::WindowExt as _;

use super::{
    ActionRole, AnyElement, Button, Checkbox, Context, ControlSize, Dialog, Disableable, Div,
    Entity, FluentBuilder, Focusable, FontWeight, InteractiveElement, IntoElement, Language,
    ManisApp, Message, ParentElement, ProxySourceEditorActivity, ProxySourceEditorInputs,
    ProxySourceEditorKind, ProxySourceEditorTarget, ProxySourceEditorView, Radius,
    RemoteSourceRefreshInterval, Role, Selectable, Stateful, StatefulInteractiveElement, Styled,
    SubscriptionFeedback, SubscriptionTextInput, TextRole, Theme, Window, action_button, copy,
    dialog_footer_surface, dialog_header_surface, div, field_label, px, refresh_interval_label,
    style_action_button, surface_dialog,
};

impl ManisApp {
    pub(in crate::app) fn open_new_subscription_editor(&mut self, cx: &mut Context<Self>) {
        if self.proxy_source_editor.is_importing() {
            return;
        }
        self.proxy_source_editor.target = ProxySourceEditorTarget::New {
            kind: ProxySourceEditorKind::Subscription,
        };
        self.proxy_source_editor.refresh_interval = RemoteSourceRefreshInterval::Manual;
        self.proxy_source_editor.interval_popover = false;
        self.proxy_source_editor.enabled = true;
        self.proxy_source_editor.error = None;
        self.proxy_source_editor.feedback = SubscriptionFeedback::Idle;
        if let Some(input) = self.proxy_source_editor.name_input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        cx.notify();
    }
    pub(in crate::app) fn open_subscription_editor(&mut self, id: String, cx: &mut Context<Self>) {
        if self.proxy_source_editor.is_importing() {
            return;
        }
        let Some(subscription) = self
            .imported_subscriptions
            .iter()
            .find(|subscription| subscription.id == id)
        else {
            return;
        };
        let name = subscription.name.clone();
        let url = subscription.source.expose_to(str::to_owned);
        self.proxy_source_editor.target = ProxySourceEditorTarget::Subscription { id };
        self.proxy_source_editor.refresh_interval = subscription.refresh_interval;
        self.proxy_source_editor.interval_popover = false;
        self.proxy_source_editor.enabled = subscription.enabled;
        self.proxy_source_editor.error = None;
        self.proxy_source_editor.feedback = SubscriptionFeedback::Idle;
        if let Some(input) = self.proxy_source_editor.name_input.as_ref() {
            input.update(cx, |input, cx| input.set_value_without_event(name, cx));
        }
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, |input, cx| input.set_value_without_event(url, cx));
        }
        cx.notify();
    }

    pub(in crate::app) fn open_single_node_editor(&mut self, id: String, cx: &mut Context<Self>) {
        if self.proxy_source_editor.is_importing() {
            return;
        }
        let Some(saved) = self.saved_single_nodes.iter().find(|saved| saved.id == id) else {
            return;
        };
        let name = saved.name.clone();
        let url = saved.source.expose_to(str::to_owned);
        self.proxy_source_editor.target = ProxySourceEditorTarget::SingleNode { id };
        self.proxy_source_editor.refresh_interval = RemoteSourceRefreshInterval::Manual;
        self.proxy_source_editor.interval_popover = false;
        self.proxy_source_editor.enabled = saved.enabled;
        self.proxy_source_editor.error = None;
        self.proxy_source_editor.feedback = SubscriptionFeedback::Idle;
        if let Some(input) = self.proxy_source_editor.name_input.as_ref() {
            input.update(cx, |input, cx| input.set_value_without_event(name, cx));
        }
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, |input, cx| input.set_value_without_event(url, cx));
        }
        cx.notify();
    }

    pub(in crate::app) fn open_proxy_source_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.proxy_source_editor.is_importing() {
            return;
        }
        let app = cx.entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            app.update(cx, |this, cx| {
                let theme = this.theme();
                this.proxy_source_editor_modal(dialog, theme, this.language(), window, cx)
            })
        });
        if let Some(input) = self.proxy_source_editor.name_input.as_ref() {
            input.focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    pub(in crate::app) fn close_subscription_editor(&mut self, cx: &mut Context<Self>) {
        // Closing the dialog does not cancel its background import.
        if self.proxy_source_editor.is_importing() {
            return;
        }
        self.configuration_add_section = None;
        self.proxy_source_editor.target.reset();
        self.proxy_source_editor.interval_popover = false;
        self.proxy_source_editor.error = None;
        self.proxy_source_editor.feedback = SubscriptionFeedback::Idle;
        if let Some(input) = self.proxy_source_editor.name_input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        if let Some(input) = self.proxy_source_editor.input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        cx.notify();
    }

    pub(in crate::app) fn proxy_source_editor_modal(
        &self,
        dialog: Dialog,
        theme: Theme,
        language: Language,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Dialog {
        let input = self
            .proxy_source_editor
            .input
            .as_ref()
            .expect("subscription input is initialized before rendering")
            .clone();
        let name_input = self
            .proxy_source_editor
            .name_input
            .as_ref()
            .expect("subscription name input is initialized before rendering")
            .clone();
        let viewport = window.viewport_size();
        let view = ProxySourceEditorView {
            direct_input: self.proxy_source_editor.target.kind()
                == ProxySourceEditorKind::SingleNode,
            editing: self.proxy_source_editor.target.editing_id().is_some(),
            activity: if matches!(
                self.proxy_source_editor.feedback,
                SubscriptionFeedback::Importing(_)
            ) {
                ProxySourceEditorActivity::Busy
            } else {
                ProxySourceEditorActivity::Idle
            },
            enabled: self.proxy_source_editor.enabled,
            dialog_width: (viewport.width.as_f32() - 32.0).clamp(300.0, 620.0),
        };
        let interval_select = self.proxy_source_interval_select(view, language, theme, cx);
        let body = self.proxy_source_editor_body(
            ProxySourceEditorInputs {
                source: input.clone(),
                name: name_input,
                interval_select,
            },
            view,
            language,
            theme,
            cx,
        );
        let footer = Self::proxy_source_editor_footer(input, view, language, theme, cx);
        let app = cx.entity();
        surface_dialog(dialog, theme)
            .width(px(view.dialog_width))
            .max_h(px((viewport.height.as_f32() - 32.0).max(320.0)))
            .margin_top(px(((viewport.height.as_f32() - 480.0) / 2.0).max(16.0)))
            .overlay(true)
            .overlay_closable(true)
            .keyboard(true)
            .close_button(false)
            .title(Self::proxy_source_editor_title(view, language, theme))
            .child(body)
            .footer(footer)
            .on_close(move |_, _, cx| {
                app.update(cx, ManisApp::close_subscription_editor);
            })
    }

    fn proxy_source_interval_select(
        &self,
        view: ProxySourceEditorView,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut menu = div().p_1();
        for interval in [
            RemoteSourceRefreshInterval::Manual,
            RemoteSourceRefreshInterval::Hourly,
            RemoteSourceRefreshInterval::SixHours,
            RemoteSourceRefreshInterval::TwelveHours,
            RemoteSourceRefreshInterval::Daily,
        ] {
            let selected = interval == self.proxy_source_editor.refresh_interval;
            menu = menu.child(
                div()
                    .id(format!("subscription-refresh-option-{interval:?}"))
                    .role(Role::Button)
                    .aria_label(refresh_interval_label(interval, language))
                    .tab_stop(true)
                    .focusable()
                    .map(crate::components::primary_button_interaction)
                    .cursor_pointer()
                    .px_3()
                    .py_2()
                    .rounded(Radius::Control.px())
                    .bg(if selected {
                        theme.action_soft
                    } else {
                        theme.surface_high
                    })
                    .hover(move |style| {
                        if selected {
                            style.bg(theme.action_soft)
                        } else {
                            style.bg(theme.button_hover)
                        }
                    })
                    .active(move |style| {
                        if selected {
                            style.bg(theme.action_soft)
                        } else {
                            style.bg(theme.button_active)
                        }
                    })
                    .font_weight(if selected {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_color(if selected {
                        theme.action_primary
                    } else {
                        theme.text_primary
                    })
                    .child(refresh_interval_label(interval, language))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.proxy_source_editor.refresh_interval = interval;
                        this.proxy_source_editor.interval_popover = false;
                        cx.notify();
                    })),
            );
        }
        let trigger = Button::new("subscription-editor-refresh-interval")
            .accessibility_label(
                language.localized(copy::configuration::CHOOSE_SUBSCRIPTION_UPDATE_INTERVAL),
            )
            .dropdown_caret(true)
            .w_full()
            .child(refresh_interval_label(
                self.proxy_source_editor.refresh_interval,
                language,
            ))
            .disabled(view.busy());
        let trigger = style_action_button(trigger, ActionRole::Secondary, ControlSize::Standard)
            .when(view.busy(), gpui::Styled::cursor_default);
        let app = cx.entity();
        crate::components::anchored_popover(
            "subscription-editor-refresh-popover",
            trigger,
            menu,
            (view.dialog_width - 40.0).max(240.0),
            280.0,
        )
        .open(self.proxy_source_editor.interval_popover)
        .on_open_change(move |open, _, cx| {
            app.update(cx, |this, cx| {
                this.proxy_source_editor.interval_popover = *open;
                cx.notify();
            });
        })
        .into_any_element()
    }

    fn proxy_source_editor_body(
        &self,
        inputs: ProxySourceEditorInputs,
        view: ProxySourceEditorView,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id("proxy-source-modal-body")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .px_5()
            .py_4()
            .when(!view.editing, |body| {
                body.child(field_label(
                    language.localized(copy::configuration::SOURCE_TYPE),
                    theme,
                ))
                .child(Self::proxy_source_kind_picker(view, language, theme, cx))
            })
            .child(field_label(
                if view.direct_input {
                    language.localized(copy::configuration::NODE_NAME)
                } else {
                    language.localized(copy::configuration::SOURCE_NAME)
                },
                theme,
            ))
            .child(inputs.name)
            .child(field_label(language.localized(copy::configuration::SOURCE_URL), theme).mt_4())
            .child(inputs.source)
            .when(!view.direct_input, |body| {
                body.child(
                    field_label(
                        language.localized(copy::configuration::UPDATE_INTERVAL),
                        theme,
                    )
                    .mt_4(),
                )
                .child(inputs.interval_select)
            })
            .child(
                Checkbox::new("proxy-source-editor-enabled")
                    .label(language.localized(copy::configuration::USE_THIS_SOURCE))
                    .map(crate::components::primary_button_interaction)
                    .checked(view.enabled)
                    .disabled(view.busy())
                    .tab_stop(!view.busy())
                    .cursor_pointer()
                    .mt_4()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !view.busy() {
                            this.proxy_source_editor.enabled = !view.enabled;
                            cx.notify();
                        }
                    })),
            )
            .when_some(self.proxy_source_editor.error.clone(), |body, error| {
                body.child(
                    div()
                        .mt_3()
                        .text_size(TextRole::Metadata.size())
                        .text_color(theme.status_error)
                        .child(error),
                )
            })
    }

    fn proxy_source_kind_picker(
        view: ProxySourceEditorView,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .mt_1()
            .flex()
            .p_1()
            .gap_1()
            .rounded(Radius::Control.px())
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_base)
            .children(
                [
                    (
                        ProxySourceEditorKind::Subscription,
                        "proxy-source-kind-subscription",
                    ),
                    (
                        ProxySourceEditorKind::SingleNode,
                        "proxy-source-kind-single-node",
                    ),
                ]
                .map(|(kind, id)| {
                    let selected = (kind == ProxySourceEditorKind::SingleNode) == view.direct_input;
                    action_button(
                        id,
                        match kind {
                            ProxySourceEditorKind::Subscription => {
                                language.localized(copy::configuration::SUBSCRIPTION)
                            }
                            ProxySourceEditorKind::SingleNode => {
                                language.localized(copy::configuration::SINGLE_NODE_SOURCE)
                            }
                        },
                        if selected {
                            ActionRole::Primary
                        } else {
                            ActionRole::Secondary
                        },
                        ControlSize::Compact,
                    )
                    .selected(selected)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.proxy_source_editor.target = ProxySourceEditorTarget::New { kind };
                        this.proxy_source_editor.error = None;
                        cx.notify();
                    }))
                }),
            )
    }

    fn proxy_source_editor_footer(
        input: Entity<SubscriptionTextInput>,
        view: ProxySourceEditorView,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        dialog_footer_surface(theme)
            .child(
                style_action_button(
                    Button::new("cancel-proxy-source").label(language.message(Message::Cancel)),
                    ActionRole::Secondary,
                    ControlSize::Standard,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.close_subscription_editor(cx);
                    window.close_dialog(cx);
                })),
            )
            .child(
                style_action_button(
                    Button::new("save-proxy-source")
                        .label(if view.busy() {
                            language.localized(copy::configuration::PROCESSING)
                        } else if view.editing {
                            language.message(Message::SaveChanges)
                        } else {
                            language.localized(copy::configuration::ADD_SOURCE)
                        })
                        .loading(view.busy()),
                    ActionRole::Primary,
                    ControlSize::Standard,
                )
                .when(view.busy(), gpui::Styled::cursor_default)
                .on_click(cx.listener(move |this, _, window, cx| {
                    if !view.busy() && this.submit_source_import(&input, cx) {
                        window.close_dialog(cx);
                    }
                })),
            )
    }

    fn proxy_source_editor_title(
        view: ProxySourceEditorView,
        language: Language,
        theme: Theme,
    ) -> Div {
        dialog_header_surface(theme)
        .child(
            div()
                .text_size(px(17.0))
                .font_weight(TextRole::SectionTitle.weight())
                .child(if view.editing {
                    language.localized(copy::configuration::EDIT_PROXY_SOURCE)
                } else {
                    language.localized(copy::configuration::ADD_PROXY_SOURCE)
                }),
        )
        .child(
            div()
                .mt_1()
                .text_size(TextRole::Metadata.size())
                .text_color(theme.text_secondary)
                .child(if view.direct_input {
                    language.localized(copy::configuration::A_SINGLE_NODE_SOURCE_DOES_NOT_NEED_AN_UPDATE_INTERVAL)
                } else {
                    language.localized(copy::configuration::CHOOSE_A_SUBSCRIPTION_OR_A_SINGLE_NODE_SHARE_LINK)
                }),
        )
    }
}
