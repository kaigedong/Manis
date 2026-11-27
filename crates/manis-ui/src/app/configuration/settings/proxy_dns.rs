use gpui::{
    AnyElement, ClipboardItem, Context, Div, Entity, Focusable as _, FontWeight,
    InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Role, SharedString, Stateful,
    StatefulInteractiveElement, Styled, Subscription, Window, div, prelude::*, px,
};
use gpui_component::{Disableable, IconName, button::Button, checkbox::Checkbox, tooltip::Tooltip};
use manis_profile::ProxyDnsServer;

use crate::app::{ManisApp, SourceMutation, SubscriptionStoreError, mutate_saved_sources};
use crate::{
    components::{ActionRole, action_button, row_action_button, style_action_button},
    localization::{Language, Message, copy},
    mihomo::{self, StoredProxyDnsEntry},
    subscription_input::{
        SubscriptionInputChanged, SubscriptionInputSubmitted, SubscriptionTextInput, TextInputSpec,
    },
    theme::{ControlSize, MONO_FAMILY, Radius, Space, TextRole, Theme},
};

use super::{SettingsDisclosureHeader, SettingsDisclosureSection, settings_inset_divider};

const MAX_PROXY_DNS_INPUT_BYTES: usize = 1_024;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::app) enum ProxyDnsInlineEdit {
    #[default]
    Idle,
    Editing(usize),
    Adding,
}

#[derive(Default)]
pub(in crate::app) struct ProxyDnsEditorState {
    pub(super) mode: ProxyDnsInlineEdit,
    pub(super) busy: bool,
    pub(super) input: Option<Entity<SubscriptionTextInput>>,
    pub(super) subscriptions: Vec<Subscription>,
    pub(super) error: Option<ProxyDnsEditError>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::app) enum ProxyDnsEditError {
    InvalidUrl,
    Duplicate,
    TooMany(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::app) enum ProxyDnsSourceKind {
    Custom,
    Subscription,
    Default,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::app) struct EffectiveProxyDnsEntry {
    pub(in crate::app) server: ProxyDnsServer,
    pub(in crate::app) enabled: bool,
    pub(in crate::app) origin: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::app) struct EffectiveProxyDns {
    pub(in crate::app) source: ProxyDnsSourceKind,
    pub(in crate::app) entries: Vec<EffectiveProxyDnsEntry>,
}

impl EffectiveProxyDns {
    #[must_use]
    pub(in crate::app) fn enabled_count(&self) -> usize {
        self.entries.iter().filter(|entry| entry.enabled).count()
    }

    #[must_use]
    fn to_stored_entries(&self) -> Vec<StoredProxyDnsEntry> {
        self.entries
            .iter()
            .map(|entry| StoredProxyDnsEntry {
                server: entry.server.clone(),
                enabled: entry.enabled,
            })
            .collect()
    }
}

pub(in crate::app) fn parse_single_proxy_dns(
    input: &str,
) -> Result<ProxyDnsServer, ProxyDnsEditError> {
    let trimmed = input.trim();
    ProxyDnsServer::parse_https(trimmed).map_err(|_| ProxyDnsEditError::InvalidUrl)
}

impl ManisApp {
    fn subscription_proxy_dns_origin(&self, url: &str) -> Option<String> {
        self.imported_subscriptions
            .iter()
            .filter(|subscription| subscription.enabled)
            .find(|subscription| {
                subscription
                    .proxy_server_nameservers
                    .iter()
                    .any(|server| server.as_str() == url)
            })
            .map(|subscription| subscription.name.clone())
    }

    pub(in crate::app) fn effective_proxy_dns(&self) -> EffectiveProxyDns {
        if !self.custom_proxy_dns_entries.is_empty() {
            let entries = self
                .custom_proxy_dns_entries
                .iter()
                .map(|entry| EffectiveProxyDnsEntry {
                    origin: self.subscription_proxy_dns_origin(entry.server.as_str()),
                    server: entry.server.clone(),
                    enabled: entry.enabled,
                })
                .collect();
            return EffectiveProxyDns {
                source: ProxyDnsSourceKind::Custom,
                entries,
            };
        }

        let mut entries: Vec<EffectiveProxyDnsEntry> = Vec::new();
        for subscription in self
            .imported_subscriptions
            .iter()
            .filter(|subscription| subscription.enabled)
        {
            for server in &subscription.proxy_server_nameservers {
                if entries.iter().any(|entry| entry.server == *server) {
                    continue;
                }
                if entries.len() >= mihomo::MAX_SUBSCRIPTION_PROXY_DNS_SERVERS {
                    break;
                }
                entries.push(EffectiveProxyDnsEntry {
                    server: server.clone(),
                    enabled: true,
                    origin: Some(subscription.name.clone()),
                });
            }
            if entries.len() >= mihomo::MAX_SUBSCRIPTION_PROXY_DNS_SERVERS {
                break;
            }
        }
        if !entries.is_empty() {
            return EffectiveProxyDns {
                source: ProxyDnsSourceKind::Subscription,
                entries,
            };
        }

        let entries = manis_profile::default_proxy_dns_servers()
            .into_iter()
            .map(|server| EffectiveProxyDnsEntry {
                server,
                enabled: true,
                origin: None,
            })
            .collect();
        EffectiveProxyDns {
            source: ProxyDnsSourceKind::Default,
            entries,
        }
    }

    pub(in crate::app) fn proxy_dns_panel(
        &self,
        theme: Theme,
        _compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let language = self.language();
        let effective = self.effective_proxy_dns();
        let enabled_count = effective.enabled_count();
        let total_count = effective.entries.len();
        let busy = self.proxy_dns_editor.busy || self.configuration_transfer.active;
        let can_add = self.subscription_store_dir.is_some()
            && !busy
            && total_count < mihomo::MAX_SUBSCRIPTION_PROXY_DNS_SERVERS
            && self.proxy_dns_editor.mode != ProxyDnsInlineEdit::Adding;

        let summary =
            copy::configuration::proxy_dns_enabled_summary(language, enabled_count, total_count);
        let add_action = Some(
            action_button(
                "configuration-add-proxy-dns",
                language.localized(copy::configuration::PROXY_DNS_ADD),
                ActionRole::Primary,
                ControlSize::Compact,
            )
            .debug_selector(|| "configuration-add-proxy-dns".to_owned())
            .icon(IconName::Plus)
            .disabled(!can_add)
            .when(!can_add, gpui::Styled::cursor_default)
            .on_click(cx.listener(|this, _, window, cx| {
                cx.stop_propagation();
                this.open_proxy_dns_inline_add(window, cx);
            }))
            .into_any_element(),
        );

        self.settings_disclosure_card(
            SettingsDisclosureHeader {
                section: SettingsDisclosureSection::ProxyDns,
                icon: IconName::Globe,
                title: language
                    .localized(copy::configuration::PROXY_DNS_TITLE)
                    .into(),
                summary: summary.into(),
                has_issue: false,
                expanded_action: add_action,
            },
            |cx| self.proxy_dns_body(&effective, language, theme, busy, cx),
            theme,
            cx,
        )
    }

    fn proxy_dns_body(
        &self,
        effective: &EffectiveProxyDns,
        language: Language,
        theme: Theme,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let controls_enabled = self.subscription_store_dir.is_some() && !busy;
        let mut body = div().w_full().flex().flex_col();

        for (index, entry) in effective.entries.iter().enumerate() {
            if index > 0 {
                body = body.child(settings_inset_divider(theme, 44.0));
            }
            if self.proxy_dns_editor.mode == ProxyDnsInlineEdit::Editing(index) {
                body = body.child(self.proxy_dns_inline_editor_row(
                    format!("proxy-dns-inline-edit-{index}"),
                    false,
                    language,
                    theme,
                    busy,
                    cx,
                ));
            } else {
                body = body.child(Self::proxy_dns_entry_row(
                    index,
                    entry,
                    effective.source,
                    controls_enabled,
                    language,
                    theme,
                    cx,
                ));
            }
        }

        if self.proxy_dns_editor.mode == ProxyDnsInlineEdit::Adding {
            if !effective.entries.is_empty() {
                body = body.child(settings_inset_divider(theme, 16.0));
            }
            body = body.child(self.proxy_dns_inline_editor_row(
                "proxy-dns-inline-add".to_owned(),
                true,
                language,
                theme,
                busy,
                cx,
            ));
        }

        body.child(settings_inset_divider(theme, 16.0))
            .child(self.proxy_dns_footer(effective, language, theme, busy, cx))
            .into_any_element()
    }

    fn proxy_dns_entry_row(
        index: usize,
        entry: &EffectiveProxyDnsEntry,
        source: ProxyDnsSourceKind,
        controls_enabled: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let url = entry.server.as_str().to_owned();
        let tooltip_url: SharedString = url.clone().into();
        let enabled = entry.enabled;
        let origin_tag: SharedString = match (&entry.origin, source) {
            (Some(subscription_name), _) => subscription_name.clone().into(),
            (None, ProxyDnsSourceKind::Custom) => language
                .localized(copy::configuration::PROXY_DNS_TAG_CUSTOM)
                .into(),
            (None, ProxyDnsSourceKind::Default | ProxyDnsSourceKind::Subscription) => language
                .localized(copy::configuration::PROXY_DNS_TAG_DEFAULT)
                .into(),
        };
        let state_label = if enabled {
            language.localized(copy::configuration::PROXY_DNS_ENABLED)
        } else {
            language.localized(copy::configuration::SOURCE_DISABLED_LABEL)
        };

        div()
            .id(format!("proxy-dns-row-{index}"))
            .role(Role::Button)
            .aria_label(language.localized(copy::configuration::PROXY_DNS_EDIT_THIS))
            .tab_stop(controls_enabled)
            .focusable()
            .map(crate::components::primary_button_interaction)
            .when(controls_enabled, gpui::Styled::cursor_pointer)
            .w_full()
            .min_h(px(56.0))
            .px(px(16.0))
            .py(px(10.0))
            .flex()
            .items_center()
            .gap(px(12.0))
            .when(controls_enabled, |row| {
                row.hover(move |row| row.bg(theme.surface_low))
            })
            .child(
                Checkbox::new(format!("proxy-dns-enabled-{index}"))
                    .block_mouse_except_scroll()
                    .aria_label(url.clone())
                    .flex_shrink_0()
                    .map(crate::components::primary_button_interaction)
                    .checked(enabled)
                    .disabled(!controls_enabled)
                    .tab_stop(controls_enabled)
                    .when(controls_enabled, gpui::Styled::cursor_pointer)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if controls_enabled {
                            this.toggle_proxy_dns_entry(index, cx);
                        }
                    })),
            )
            .child(Self::proxy_dns_entry_summary(
                index,
                url,
                tooltip_url,
                origin_tag,
                state_label,
                enabled,
                theme,
            ))
            .child(Self::proxy_dns_row_actions(
                index,
                controls_enabled,
                language,
                cx,
            ))
            .on_click(cx.listener(move |this, _, window, cx| {
                if controls_enabled {
                    this.open_proxy_dns_inline_edit(index, window, cx);
                }
            }))
    }

    fn proxy_dns_entry_summary(
        index: usize,
        url: String,
        tooltip_url: SharedString,
        origin_tag: SharedString,
        state_label: &'static str,
        enabled: bool,
        theme: Theme,
    ) -> Stateful<Div> {
        let badge_text = if enabled {
            theme.text_secondary
        } else {
            theme.text_tertiary
        };
        div()
            .id(format!("proxy-dns-url-{index}"))
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(3.0))
            .tooltip(move |window, cx| Tooltip::new(tooltip_url.clone()).build(window, cx))
            .child(
                div()
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .font_family(MONO_FAMILY)
                    .text_size(px(12.5))
                    .line_height(px(18.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if enabled {
                        theme.text_primary
                    } else {
                        theme.text_tertiary
                    })
                    .child(url),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .px(px(7.0))
                            .py(px(1.0))
                            .rounded(Radius::Badge.px())
                            .bg(theme.surface_low)
                            .text_size(px(11.0))
                            .line_height(px(15.0))
                            .text_color(badge_text)
                            .child(origin_tag),
                    )
                    .child(
                        div()
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .text_color(theme.text_tertiary)
                            .child(state_label),
                    ),
            )
    }

    fn proxy_dns_row_actions(
        index: usize,
        controls_enabled: bool,
        language: Language,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(6.0))
            .when(controls_enabled, |actions| {
                actions
                    .child(
                        row_action_button(
                            format!("proxy-dns-edit-{index}"),
                            language.localized(copy::configuration::PROXY_DNS_EDIT),
                            ActionRole::Secondary,
                            ControlSize::Compact,
                        )
                        .accessibility_label(
                            language.localized(copy::configuration::PROXY_DNS_EDIT_THIS),
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.open_proxy_dns_inline_edit(index, window, cx);
                            },
                        )),
                    )
                    .child(
                        row_action_button(
                            format!("proxy-dns-remove-{index}"),
                            language.localized(copy::configuration::REMOVE),
                            ActionRole::SubtleDanger,
                            ControlSize::Compact,
                        )
                        .accessibility_label(
                            language.localized(copy::configuration::PROXY_DNS_REMOVE_THIS),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.remove_proxy_dns_entry(index, cx);
                        })),
                    )
            })
    }

    fn proxy_dns_inline_editor_row(
        &self,
        id: String,
        is_add: bool,
        language: Language,
        theme: Theme,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let error_text = self.proxy_dns_editor.error.map(|error| match error {
            ProxyDnsEditError::InvalidUrl => language
                .localized(copy::configuration::PROXY_DNS_INVALID_URL)
                .to_owned(),
            ProxyDnsEditError::Duplicate => language
                .localized(copy::configuration::PROXY_DNS_DUPLICATE)
                .to_owned(),
            ProxyDnsEditError::TooMany(limit) => {
                copy::configuration::proxy_dns_too_many(language, limit)
            }
        });
        let submit_label = if busy {
            language.localized(copy::configuration::PROXY_DNS_SAVING)
        } else if is_add {
            language.localized(copy::configuration::PROXY_DNS_ADD)
        } else {
            language.localized(copy::configuration::PROXY_DNS_SAVE)
        };
        let save_button = style_action_button(
            Button::new("proxy-dns-inline-save")
                .label(submit_label)
                .loading(busy)
                .disabled(busy)
                .tab_stop(!busy),
            ActionRole::Primary,
            ControlSize::Compact,
        )
        .when(busy, gpui::Styled::cursor_default)
        .on_click(cx.listener(|this, _, _, cx| {
            this.submit_proxy_dns_inline_edit(cx);
        }));

        div()
            .id(id)
            .w_full()
            .px(px(16.0))
            .py(px(10.0))
            .bg(theme.surface_low)
            .flex()
            .flex_col()
            .gap(px(8.0))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.cancel_proxy_dns_inline_edit(cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(Space::Xs.px())
                    .when_some(self.proxy_dns_editor.input.as_ref(), |row, input| {
                        row.child(div().flex_1().min_w(px(0.0)).child(input.clone()))
                    })
                    .child(
                        action_button(
                            "proxy-dns-inline-cancel",
                            language.message(Message::Cancel),
                            ActionRole::Secondary,
                            ControlSize::Compact,
                        )
                        .disabled(busy)
                        .when(busy, gpui::Styled::cursor_default)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.cancel_proxy_dns_inline_edit(cx);
                        })),
                    )
                    .child(save_button),
            )
            .when_some(error_text, |col, message| {
                col.child(
                    div()
                        .id("proxy-dns-editor-error")
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(theme.status_error)
                        .child(message),
                )
            })
    }

    fn proxy_dns_footer(
        &self,
        effective: &EffectiveProxyDns,
        language: Language,
        theme: Theme,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let has_custom = !self.custom_proxy_dns_entries.is_empty();
        let subtitle = match effective.source {
            ProxyDnsSourceKind::Custom if effective.enabled_count() == 0 => {
                language.localized(copy::configuration::PROXY_DNS_CUSTOM_FALLBACK_SUBTITLE)
            }
            ProxyDnsSourceKind::Custom => {
                language.localized(copy::configuration::PROXY_DNS_CUSTOM_SUBTITLE)
            }
            ProxyDnsSourceKind::Subscription => {
                language.localized(copy::configuration::PROXY_DNS_SUBSCRIPTION_SUBTITLE)
            }
            ProxyDnsSourceKind::Default => {
                language.localized(copy::configuration::PROXY_DNS_DEFAULT_SUBTITLE)
            }
        };

        div()
            .w_full()
            .px(px(16.0))
            .py(px(10.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(theme.text_tertiary)
                    .child(subtitle),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(Space::Xs.px())
                    .child(
                        action_button(
                            "proxy-dns-copy",
                            language.localized(copy::configuration::PROXY_DNS_COPY),
                            ActionRole::Secondary,
                            ControlSize::Compact,
                        )
                        .icon(IconName::Copy)
                        .disabled(effective.enabled_count() == 0)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.copy_effective_proxy_dns(cx);
                        })),
                    )
                    .when(has_custom, |actions| {
                        actions.child(
                            action_button(
                                "proxy-dns-reset-auto",
                                language.localized(copy::configuration::PROXY_DNS_RESET_AUTO),
                                ActionRole::Secondary,
                                ControlSize::Compact,
                            )
                            .disabled(busy)
                            .when(busy, gpui::Styled::cursor_default)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reset_proxy_dns_to_auto(cx);
                            })),
                        )
                    }),
            )
    }

    fn ensure_proxy_dns_inline_input(
        &mut self,
        initial: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        let theme = self.theme();
        let input = cx.new(|cx| {
            SubscriptionTextInput::new_field(
                TextInputSpec::new(
                    "proxy-dns-inline-input",
                    language.localized(copy::configuration::PROXY_DNS_PLACEHOLDER),
                    MAX_PROXY_DNS_INPUT_BYTES,
                    theme,
                    self.dark,
                ),
                window,
                cx,
            )
        });
        input.update(cx, |field, cx| field.set_value_without_event(initial, cx));
        let changed = cx.subscribe(&input, |this, _, _: &SubscriptionInputChanged, cx| {
            if this.proxy_dns_editor.error.take().is_some() {
                cx.notify();
            }
        });
        let submitted = cx.subscribe(&input, |this, _, _: &SubscriptionInputSubmitted, cx| {
            this.submit_proxy_dns_inline_edit(cx);
        });
        self.proxy_dns_editor.subscriptions = vec![changed, submitted];
        input.focus_handle(cx).focus(window, cx);
        self.proxy_dns_editor.input = Some(input);
    }

    pub(in crate::app) fn open_proxy_dns_inline_edit(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.proxy_dns_editor.busy
            || self.configuration_transfer.active
            || self.subscription_store_dir.is_none()
        {
            return;
        }
        let effective = self.effective_proxy_dns();
        let Some(entry) = effective.entries.get(index) else {
            return;
        };
        let initial = entry.server.as_str().to_owned();
        self.expand_settings_disclosure(SettingsDisclosureSection::ProxyDns);
        self.proxy_dns_editor.mode = ProxyDnsInlineEdit::Editing(index);
        self.proxy_dns_editor.error = None;
        self.ensure_proxy_dns_inline_input(initial, window, cx);
        cx.notify();
    }

    pub(in crate::app) fn open_proxy_dns_inline_add(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.proxy_dns_editor.busy
            || self.configuration_transfer.active
            || self.subscription_store_dir.is_none()
        {
            return;
        }
        let effective = self.effective_proxy_dns();
        if effective.entries.len() >= mihomo::MAX_SUBSCRIPTION_PROXY_DNS_SERVERS {
            self.proxy_dns_editor.error = Some(ProxyDnsEditError::TooMany(
                mihomo::MAX_SUBSCRIPTION_PROXY_DNS_SERVERS,
            ));
            cx.notify();
            return;
        }
        self.expand_settings_disclosure(SettingsDisclosureSection::ProxyDns);
        self.proxy_dns_editor.mode = ProxyDnsInlineEdit::Adding;
        self.proxy_dns_editor.error = None;
        self.ensure_proxy_dns_inline_input(String::new(), window, cx);
        cx.notify();
    }

    pub(in crate::app) fn cancel_proxy_dns_inline_edit(&mut self, cx: &mut Context<Self>) {
        if self.proxy_dns_editor.busy {
            return;
        }
        self.proxy_dns_editor.mode = ProxyDnsInlineEdit::Idle;
        self.proxy_dns_editor.input = None;
        self.proxy_dns_editor.subscriptions.clear();
        self.proxy_dns_editor.error = None;
        cx.notify();
    }

    pub(in crate::app) fn submit_proxy_dns_inline_edit(&mut self, cx: &mut Context<Self>) {
        if self.proxy_dns_editor.busy || self.configuration_transfer.active {
            return;
        }
        let Some(input) = self.proxy_dns_editor.input.as_ref() else {
            return;
        };
        let raw = input.read(cx).value().to_owned();
        let server = match parse_single_proxy_dns(&raw) {
            Ok(server) => server,
            Err(error) => {
                self.proxy_dns_editor.error = Some(error);
                cx.notify();
                return;
            }
        };
        let mut entries = self.effective_proxy_dns().to_stored_entries();
        match self.proxy_dns_editor.mode {
            ProxyDnsInlineEdit::Idle => return,
            ProxyDnsInlineEdit::Editing(index) => {
                let Some(target) = entries.get(index) else {
                    return;
                };
                if entries
                    .iter()
                    .enumerate()
                    .any(|(idx, candidate)| idx != index && candidate.server == server)
                {
                    self.proxy_dns_editor.error = Some(ProxyDnsEditError::Duplicate);
                    cx.notify();
                    return;
                }
                if target.server == server && !self.custom_proxy_dns_entries.is_empty() {
                    self.cancel_proxy_dns_inline_edit(cx);
                    return;
                }
                entries[index].server = server;
            }
            ProxyDnsInlineEdit::Adding => {
                if entries.iter().any(|candidate| candidate.server == server) {
                    self.proxy_dns_editor.error = Some(ProxyDnsEditError::Duplicate);
                    cx.notify();
                    return;
                }
                if entries.len() >= mihomo::MAX_SUBSCRIPTION_PROXY_DNS_SERVERS {
                    self.proxy_dns_editor.error = Some(ProxyDnsEditError::TooMany(
                        mihomo::MAX_SUBSCRIPTION_PROXY_DNS_SERVERS,
                    ));
                    cx.notify();
                    return;
                }
                entries.push(StoredProxyDnsEntry {
                    server,
                    enabled: true,
                });
            }
        }
        self.proxy_dns_editor.error = None;
        self.apply_custom_proxy_dns_entries(entries, cx);
    }

    pub(in crate::app) fn toggle_proxy_dns_entry(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.proxy_dns_editor.busy || self.configuration_transfer.active {
            return;
        }
        let mut entries = self.effective_proxy_dns().to_stored_entries();
        let Some(entry) = entries.get_mut(index) else {
            return;
        };
        entry.enabled = !entry.enabled;
        self.apply_custom_proxy_dns_entries(entries, cx);
    }

    pub(in crate::app) fn remove_proxy_dns_entry(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.proxy_dns_editor.busy || self.configuration_transfer.active {
            return;
        }
        let mut entries = self.effective_proxy_dns().to_stored_entries();
        if index >= entries.len() {
            return;
        }
        entries.remove(index);
        self.apply_custom_proxy_dns_entries(entries, cx);
    }

    pub(in crate::app) fn copy_effective_proxy_dns(&mut self, cx: &mut Context<Self>) {
        let effective = self.effective_proxy_dns();
        let urls: Vec<_> = effective
            .entries
            .into_iter()
            .filter(|entry| entry.enabled)
            .map(|entry| entry.server.as_str().to_owned())
            .collect();
        if urls.is_empty() {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(urls.join("\n")));
        self.language()
            .localized(copy::configuration::PROXY_DNS_COPIED)
            .clone_into(&mut self.status);
        cx.notify();
    }

    pub(in crate::app) fn reset_proxy_dns_to_auto(&mut self, cx: &mut Context<Self>) {
        if self.proxy_dns_editor.busy || self.configuration_transfer.active {
            return;
        }
        self.proxy_dns_editor.error = None;
        self.apply_custom_proxy_dns_entries(Vec::new(), cx);
    }

    pub(in crate::app) fn apply_custom_proxy_dns_entries(
        &mut self,
        entries: Vec<StoredProxyDnsEntry>,
        cx: &mut Context<Self>,
    ) {
        if self.proxy_dns_editor.busy || self.configuration_transfer.active {
            return;
        }
        let language = self.language();
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            language
                .localized(copy::configuration::PROXY_DNS_STORE_UNAVAILABLE)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        };
        self.proxy_dns_editor.busy = true;
        language
            .localized(copy::configuration::PROXY_DNS_APPLYING)
            .clone_into(&mut self.status);
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    mutate_saved_sources(&runtime, &store_dir, |store_dir| {
                        mihomo::save_custom_proxy_dns_entries_in(store_dir, &entries)?;
                        mihomo::load_custom_proxy_dns_entries_in(store_dir)
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_proxy_dns_save(result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn finish_proxy_dns_save(
        &mut self,
        result: Result<SourceMutation<Vec<StoredProxyDnsEntry>>, SubscriptionStoreError>,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        self.proxy_dns_editor.busy = false;
        match result {
            Ok(SourceMutation::Committed {
                value: saved,
                apply,
            }) => {
                self.custom_proxy_dns_entries = saved;
                self.proxy_dns_editor.mode = ProxyDnsInlineEdit::Idle;
                self.proxy_dns_editor.input = None;
                self.proxy_dns_editor.subscriptions.clear();
                self.proxy_dns_editor.error = None;
                apply.reconcile_proxy_mode(&mut self.proxy_mode);
                let active_count = if self.custom_proxy_dns_entries.is_empty() {
                    0
                } else {
                    self.custom_proxy_dns_entries
                        .iter()
                        .filter(|entry| entry.enabled)
                        .count()
                };
                self.status = copy::configuration::proxy_dns_saved(
                    language,
                    active_count,
                    &apply.status_suffix(language),
                );
            }
            Ok(SourceMutation::RollbackAttempted {
                apply,
                rollback_error,
            }) => {
                self.status = format!(
                    "{}{}",
                    language.localized(copy::configuration::PROXY_DNS_SAVE_FAILED),
                    apply.status_suffix_after_rollback_attempt(language, rollback_error.as_ref())
                );
            }
            Err(_) => {
                language
                    .localized(copy::configuration::PROXY_DNS_SAVE_FAILED)
                    .clone_into(&mut self.status);
            }
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use manis_profile::ProxyDnsServer;

    use super::{
        ProxyDnsEditError, ProxyDnsInlineEdit, ProxyDnsSourceKind, parse_single_proxy_dns,
    };
    use crate::app::routing_apply::mutate_saved_sources_with_apply;
    use crate::app::{ImportedSubscription, ImportedSubscriptionState, ManisApp};
    use crate::mihomo::{self, RemoteSourceRefreshInterval, StoredProxyDnsEntry};
    use crate::subscription::SourceKind;

    fn temp_store(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "manis-proxy-dns-ui-{name}-{}-{}",
            std::process::id(),
            mihomo::current_unix_secs()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp store");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).expect("chmod 700");
        }
        dir
    }

    fn seed_subscription_with_doh(store: &std::path::Path, urls: &[&str]) {
        let saved = mihomo::save_subscription_source_with_options_in(
            store,
            "https://naiu.example.invalid/sub",
            "NaiU_Net",
            RemoteSourceRefreshInterval::Hourly,
            true,
        )
        .expect("save subscription");
        let servers: Vec<_> = urls
            .iter()
            .map(|url| ProxyDnsServer::parse_https(url).expect("doh"))
            .collect();
        mihomo::update_subscription_source_proxy_nameservers_in(store, &saved.id, &servers)
            .expect("save subscription doh");
    }

    #[test]
    fn validates_single_proxy_dns_url() {
        assert_eq!(
            parse_single_proxy_dns("  https://20.247.42.211:36290/dns-query/clash?site=naiu ")
                .expect("valid")
                .as_str(),
            "https://20.247.42.211:36290/dns-query/clash?site=naiu"
        );
        assert_eq!(
            parse_single_proxy_dns("http://insecure.example/dns"),
            Err(ProxyDnsEditError::InvalidUrl)
        );
        assert_eq!(
            parse_single_proxy_dns("   "),
            Err(ProxyDnsEditError::InvalidUrl)
        );
    }

    #[test]
    fn effective_proxy_dns_supports_selection_and_origin_labels() {
        let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:9090");
        assert_eq!(
            app.effective_proxy_dns().source,
            ProxyDnsSourceKind::Default
        );

        let doh_a =
            ProxyDnsServer::parse_https("https://20.247.42.211:36290/dns-query/clash?site=naiu")
                .expect("doh");
        let doh_b =
            ProxyDnsServer::parse_https("https://20.239.241.138:55228/dns-query/clash?site=naiu")
                .expect("doh");
        app.imported_subscriptions.push(ImportedSubscription {
            id: "sub:naiu".to_owned(),
            name: "NaiU_Net".to_owned(),
            source: manis_profile::SecretUrl::parse_subscription(
                "https://naiu.example.invalid/sub",
            )
            .expect("url"),
            enabled: true,
            state: ImportedSubscriptionState::Ready(SourceKind::HttpsSubscription),
            providers: Vec::new(),
            generation: 0,
            refresh_interval: RemoteSourceRefreshInterval::Hourly,
            last_successful_update_unix_secs: 100,
            proxy_server_nameservers: vec![doh_a.clone(), doh_b.clone()],
        });

        let from_sub = app.effective_proxy_dns();
        assert_eq!(from_sub.source, ProxyDnsSourceKind::Subscription);
        assert_eq!(from_sub.enabled_count(), 2);

        app.custom_proxy_dns_entries = vec![
            StoredProxyDnsEntry {
                server: doh_a,
                enabled: true,
            },
            StoredProxyDnsEntry {
                server: doh_b,
                enabled: false,
            },
        ];
        let custom = app.effective_proxy_dns();
        assert_eq!(custom.source, ProxyDnsSourceKind::Custom);
        assert_eq!(custom.enabled_count(), 1);
        assert_eq!(custom.entries[0].origin.as_deref(), Some("NaiU_Net"));
        assert!(!custom.entries[1].enabled);
    }

    #[gpui::test]
    fn disclosure_card_supports_inline_edit_selection_and_reset(cx: &mut gpui::TestAppContext) {
        let store = temp_store("inline-round-trip");
        let doh_a = "https://20.247.42.211:36290/dns-query/clash?site=naiu";
        let doh_b = "https://20.239.241.138:55228/dns-query/clash?site=naiu";
        seed_subscription_with_doh(&store, &[doh_a, doh_b]);

        let (app, cx) = cx.add_window_view(|window, cx| {
            gpui_component::init(cx);
            let mut app = ManisApp::with_fixture_controller_and_subscription_store(
                "http://127.0.0.1:1",
                store.clone(),
            );
            app.attach_window(window, cx);
            app
        });

        cx.update(|window, cx| {
            app.update(cx, |this, cx| {
                assert!(
                    !this.is_settings_disclosure_expanded(
                        super::SettingsDisclosureSection::ProxyDns
                    )
                );
                this.open_proxy_dns_inline_edit(0, window, cx);
                assert!(
                    this.is_settings_disclosure_expanded(
                        super::SettingsDisclosureSection::ProxyDns
                    )
                );
                assert_eq!(this.proxy_dns_editor.mode, ProxyDnsInlineEdit::Editing(0));

                // Duplicate URL is rejected inline without closing the editor.
                let input = this.proxy_dns_editor.input.as_ref().expect("input").clone();
                input.update(cx, |field, cx| {
                    field.set_value_without_event(doh_b.to_owned(), cx);
                });
                this.submit_proxy_dns_inline_edit(cx);
                assert_eq!(
                    this.proxy_dns_editor.error,
                    Some(ProxyDnsEditError::Duplicate)
                );

                // Persist toggled selection (disable index 1) and verify compiler only emits enabled entry.
                let entries = vec![
                    StoredProxyDnsEntry {
                        server: ProxyDnsServer::parse_https(doh_a).expect("doh"),
                        enabled: true,
                    },
                    StoredProxyDnsEntry {
                        server: ProxyDnsServer::parse_https(doh_b).expect("doh"),
                        enabled: false,
                    },
                ];
                let mutation = mutate_saved_sources_with_apply(
                    &store,
                    |staged| {
                        mihomo::save_custom_proxy_dns_entries_in(staged, &entries)?;
                        mihomo::load_custom_proxy_dns_entries_in(staged)
                    },
                    || crate::app::SourceRuntimeApply::MetadataOnly,
                );
                this.finish_proxy_dns_save(mutation, cx);
                assert_eq!(this.proxy_dns_editor.mode, ProxyDnsInlineEdit::Idle);
                assert_eq!(this.effective_proxy_dns().enabled_count(), 1);
                assert_eq!(
                    mihomo::load_custom_proxy_dns_servers_in(&store)
                        .expect("enabled")
                        .len(),
                    1
                );

                // Resetting clears proxy-dns.state and restores both subscription DoH servers.
                let reset = mutate_saved_sources_with_apply(
                    &store,
                    |staged| {
                        mihomo::save_custom_proxy_dns_entries_in(staged, &[])?;
                        mihomo::load_custom_proxy_dns_entries_in(staged)
                    },
                    || crate::app::SourceRuntimeApply::MetadataOnly,
                );
                this.finish_proxy_dns_save(reset, cx);
                assert_eq!(
                    this.effective_proxy_dns().source,
                    ProxyDnsSourceKind::Subscription
                );
                assert_eq!(this.effective_proxy_dns().enabled_count(), 2);
            });
        });

        cx.run_until_parked();
        let _ = fs::remove_dir_all(store);
    }
}
