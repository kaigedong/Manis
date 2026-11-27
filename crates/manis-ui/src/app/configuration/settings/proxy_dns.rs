use gpui::{
    ClipboardItem, Context, Div, Entity, FontWeight, InteractiveElement, IntoElement,
    ParentElement, SharedString, Stateful, StatefulInteractiveElement, Styled, Window, div,
    prelude::*, px,
};
use gpui_component::{Disableable, IconName, button::Button, tooltip::Tooltip};
use manis_profile::ProxyDnsServer;

use crate::app::{ManisApp, SourceMutation, SubscriptionStoreError, mutate_saved_sources};
use crate::{
    components::{ActionRole, action_button, style_action_button},
    localization::{Language, Message, copy},
    mihomo,
    theme::{ControlSize, MONO_FAMILY, Radius, Space, Theme},
};

use super::{settings_group_card, settings_inset_divider, settings_row, settings_section_header};

#[derive(Default)]
pub(in crate::app) struct ProxyDnsEditorState {
    pub(super) editing: bool,
    pub(super) busy: bool,
    pub(super) input: Option<Entity<gpui_component::input::EditorState>>,
    pub(super) error: Option<ProxyDnsEditError>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::app) enum ProxyDnsEditError {
    InvalidLine(usize),
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
    pub(in crate::app) url: String,
    pub(in crate::app) origin: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::app) struct EffectiveProxyDns {
    pub(in crate::app) source: ProxyDnsSourceKind,
    pub(in crate::app) entries: Vec<EffectiveProxyDnsEntry>,
}

pub(in crate::app) fn parse_proxy_dns_lines(
    input: &str,
) -> Result<Vec<ProxyDnsServer>, ProxyDnsEditError> {
    let mut servers = Vec::new();
    for (index, raw_line) in input.lines().enumerate() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let server = ProxyDnsServer::parse_https(trimmed)
            .map_err(|_| ProxyDnsEditError::InvalidLine(index + 1))?;
        if servers.contains(&server) {
            continue;
        }
        if servers.len() >= mihomo::MAX_SUBSCRIPTION_PROXY_DNS_SERVERS {
            return Err(ProxyDnsEditError::TooMany(
                mihomo::MAX_SUBSCRIPTION_PROXY_DNS_SERVERS,
            ));
        }
        servers.push(server);
    }
    Ok(servers)
}

impl ManisApp {
    pub(in crate::app) fn effective_proxy_dns(&self) -> EffectiveProxyDns {
        if !self.custom_proxy_dns_servers.is_empty() {
            let entries = self
                .custom_proxy_dns_servers
                .iter()
                .map(|server| EffectiveProxyDnsEntry {
                    url: server.as_str().to_owned(),
                    origin: None,
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
                let url = server.as_str();
                if entries.iter().any(|entry| entry.url == url) {
                    continue;
                }
                if entries.len() >= mihomo::MAX_SUBSCRIPTION_PROXY_DNS_SERVERS {
                    break;
                }
                entries.push(EffectiveProxyDnsEntry {
                    url: url.to_owned(),
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
                url: server.as_str().to_owned(),
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
        let source_label = match effective.source {
            ProxyDnsSourceKind::Custom => {
                language.localized(copy::configuration::PROXY_DNS_SOURCE_CUSTOM)
            }
            ProxyDnsSourceKind::Subscription => {
                language.localized(copy::configuration::PROXY_DNS_SOURCE_SUBSCRIPTION)
            }
            ProxyDnsSourceKind::Default => {
                language.localized(copy::configuration::PROXY_DNS_SOURCE_DEFAULT)
            }
        };
        let subtitle = match effective.source {
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
        let summary = copy::configuration::proxy_dns_count_summary(
            language,
            source_label,
            effective.entries.len(),
        );
        let editing = self.proxy_dns_editor.editing;
        let busy = self.proxy_dns_editor.busy || self.configuration_transfer.active;
        let editable = self.subscription_store_dir.is_some() && !busy;

        let trailing = div()
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
                .disabled(effective.entries.is_empty())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.copy_effective_proxy_dns(cx);
                })),
            )
            .when(!editing, |row| {
                row.child(
                    action_button(
                        "proxy-dns-edit",
                        language.localized(copy::configuration::PROXY_DNS_EDIT),
                        ActionRole::Secondary,
                        ControlSize::Compact,
                    )
                    .disabled(!editable)
                    .when(!editable, gpui::Styled::cursor_default)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_proxy_dns_editor(window, cx);
                    })),
                )
            })
            .into_any_element();

        div()
            .id("configuration-proxy-dns-section")
            .w_full()
            .child(settings_section_header(
                language.localized(copy::configuration::PROXY_DNS_TITLE),
                Some(summary.into()),
                None,
                theme,
            ))
            .child(
                settings_group_card("configuration-proxy-dns", theme)
                    .child(settings_row(
                        IconName::Globe,
                        source_label,
                        Some(subtitle.into()),
                        trailing,
                        theme,
                    ))
                    .child(settings_inset_divider(theme, 56.0))
                    .child(if editing {
                        self.proxy_dns_editor_body(language, theme, busy, cx)
                            .into_any_element()
                    } else {
                        Self::proxy_dns_server_list(&effective, language, theme).into_any_element()
                    }),
            )
    }

    fn proxy_dns_server_list(
        effective: &EffectiveProxyDns,
        language: Language,
        theme: Theme,
    ) -> Div {
        let mut list = div()
            .id("configuration-proxy-dns-list")
            .w_full()
            .px(px(16.0))
            .py(px(10.0))
            .flex()
            .flex_col()
            .gap(px(6.0));
        for (index, entry) in effective.entries.iter().enumerate() {
            let tag: SharedString = match (&entry.origin, effective.source) {
                (Some(subscription_name), _) => subscription_name.clone().into(),
                (None, ProxyDnsSourceKind::Custom) => language
                    .localized(copy::configuration::PROXY_DNS_TAG_CUSTOM)
                    .into(),
                (None, ProxyDnsSourceKind::Default | ProxyDnsSourceKind::Subscription) => language
                    .localized(copy::configuration::PROXY_DNS_TAG_DEFAULT)
                    .into(),
            };
            let tooltip_url: SharedString = entry.url.clone().into();
            let row_id = format!("proxy-dns-entry-{index}");
            list = list.child(
                div()
                    .id(row_id)
                    .w_full()
                    .px(px(10.0))
                    .py(px(7.0))
                    .rounded(Radius::Row.px())
                    .bg(theme.surface_low)
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .tooltip(move |window, cx| Tooltip::new(tooltip_url.clone()).build(window, cx))
                    .child(
                        div()
                            .flex_shrink_0()
                            .min_w(px(18.0))
                            .h(px(18.0))
                            .px(px(5.0))
                            .rounded(px(5.0))
                            .bg(theme.surface_elevated)
                            .flex()
                            .items_center()
                            .justify_center()
                            .font_family(MONO_FAMILY)
                            .text_size(px(10.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_tertiary)
                            .child((index + 1).to_string()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_family(MONO_FAMILY)
                            .text_size(px(12.0))
                            .line_height(px(17.0))
                            .text_color(theme.text_primary)
                            .child(entry.url.clone()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .px(px(7.0))
                            .py(px(2.0))
                            .rounded(px(5.0))
                            .bg(theme.surface_elevated)
                            .text_size(px(11.0))
                            .line_height(px(15.0))
                            .text_color(theme.text_secondary)
                            .child(tag),
                    ),
            );
        }
        div().w_full().child(list)
    }

    fn proxy_dns_editor_body(
        &self,
        language: Language,
        theme: Theme,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let error_text = self.proxy_dns_editor.error.map(|error| match error {
            ProxyDnsEditError::InvalidLine(line) => {
                copy::configuration::proxy_dns_invalid_line(language, line)
            }
            ProxyDnsEditError::TooMany(limit) => {
                copy::configuration::proxy_dns_too_many(language, limit)
            }
        });

        div()
            .id("configuration-proxy-dns-editor")
            .w_full()
            .px(px(16.0))
            .py(px(12.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .when_some(self.proxy_dns_editor.input.as_ref(), |body, editor| {
                body.child(
                    gpui_component::input::Editor::new(editor)
                        .aria_label(language.localized(copy::configuration::PROXY_DNS_EDITOR_LABEL))
                        .h(px(116.0))
                        .disabled(busy),
                )
            })
            .child(
                div()
                    .text_size(px(12.0))
                    .line_height(px(17.0))
                    .text_color(theme.text_tertiary)
                    .child(language.localized(copy::configuration::PROXY_DNS_EDITOR_HINT)),
            )
            .when_some(error_text, |body, message| {
                body.child(
                    div()
                        .id("proxy-dns-editor-error")
                        .text_size(px(12.0))
                        .line_height(px(17.0))
                        .text_color(theme.status_error)
                        .child(message),
                )
            })
            .child(self.proxy_dns_editor_actions(language, busy, cx))
    }

    fn proxy_dns_editor_actions(
        &self,
        language: Language,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let has_custom = !self.custom_proxy_dns_servers.is_empty();
        let save_button = style_action_button(
            Button::new("proxy-dns-save")
                .label(if busy {
                    language.localized(copy::configuration::PROXY_DNS_SAVING)
                } else {
                    language.localized(copy::configuration::PROXY_DNS_SAVE)
                })
                .loading(busy)
                .disabled(busy)
                .tab_stop(!busy),
            ActionRole::Primary,
            ControlSize::Compact,
        )
        .when(busy, gpui::Styled::cursor_default)
        .on_click(cx.listener(|this, _, _, cx| {
            this.save_proxy_dns_editor(cx);
        }));

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(Space::Sm.px())
            .child(div().when(has_custom, |left| {
                left.child(
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
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(Space::Xs.px())
                    .child(
                        action_button(
                            "proxy-dns-cancel",
                            language.message(Message::Cancel),
                            ActionRole::Secondary,
                            ControlSize::Compact,
                        )
                        .disabled(busy)
                        .when(busy, gpui::Styled::cursor_default)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.cancel_proxy_dns_editor(cx);
                        })),
                    )
                    .child(save_button),
            )
    }

    pub(in crate::app) fn copy_effective_proxy_dns(&mut self, cx: &mut Context<Self>) {
        let effective = self.effective_proxy_dns();
        if effective.entries.is_empty() {
            return;
        }
        let text = effective
            .entries
            .into_iter()
            .map(|entry| entry.url)
            .collect::<Vec<_>>()
            .join("\n");
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.language()
            .localized(copy::configuration::PROXY_DNS_COPIED)
            .clone_into(&mut self.status);
        cx.notify();
    }

    pub(in crate::app) fn open_proxy_dns_editor(
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
        let initial = self
            .effective_proxy_dns()
            .entries
            .into_iter()
            .map(|entry| entry.url)
            .collect::<Vec<_>>()
            .join("\n");
        let editor = cx.new(|cx| {
            gpui_component::input::EditorState::new(window, cx)
                .soft_wrap(true)
                .default_value(initial)
        });
        self.proxy_dns_editor.input = Some(editor.clone());
        self.proxy_dns_editor.editing = true;
        self.proxy_dns_editor.error = None;
        editor.update(cx, |editor, cx| editor.focus(window, cx));
        cx.notify();
    }

    pub(in crate::app) fn cancel_proxy_dns_editor(&mut self, cx: &mut Context<Self>) {
        if self.proxy_dns_editor.busy {
            return;
        }
        self.proxy_dns_editor.editing = false;
        self.proxy_dns_editor.input = None;
        self.proxy_dns_editor.error = None;
        cx.notify();
    }

    pub(in crate::app) fn save_proxy_dns_editor(&mut self, cx: &mut Context<Self>) {
        if self.proxy_dns_editor.busy || self.configuration_transfer.active {
            return;
        }
        let Some(editor) = self.proxy_dns_editor.input.as_ref() else {
            return;
        };
        let raw = editor.read(cx).value();
        match parse_proxy_dns_lines(&raw) {
            Ok(servers) => {
                self.proxy_dns_editor.error = None;
                self.apply_custom_proxy_dns_servers(servers, cx);
            }
            Err(error) => {
                self.proxy_dns_editor.error = Some(error);
                cx.notify();
            }
        }
    }

    pub(in crate::app) fn reset_proxy_dns_to_auto(&mut self, cx: &mut Context<Self>) {
        if self.proxy_dns_editor.busy || self.configuration_transfer.active {
            return;
        }
        self.proxy_dns_editor.error = None;
        self.apply_custom_proxy_dns_servers(Vec::new(), cx);
    }

    pub(in crate::app) fn apply_custom_proxy_dns_servers(
        &mut self,
        servers: Vec<ProxyDnsServer>,
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
                        mihomo::save_custom_proxy_dns_servers_in(store_dir, &servers)?;
                        mihomo::load_custom_proxy_dns_servers_in(store_dir)
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
        result: Result<SourceMutation<Vec<ProxyDnsServer>>, SubscriptionStoreError>,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        self.proxy_dns_editor.busy = false;
        match result {
            Ok(SourceMutation::Committed {
                value: saved,
                apply,
            }) => {
                self.custom_proxy_dns_servers = saved;
                self.proxy_dns_editor.editing = false;
                self.proxy_dns_editor.input = None;
                self.proxy_dns_editor.error = None;
                apply.reconcile_proxy_mode(&mut self.proxy_mode);
                self.status = copy::configuration::proxy_dns_saved(
                    language,
                    self.custom_proxy_dns_servers.len(),
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

    use super::{ProxyDnsEditError, ProxyDnsSourceKind, parse_proxy_dns_lines};
    use crate::app::routing_apply::mutate_saved_sources_with_apply;
    use crate::app::{ImportedSubscription, ImportedSubscriptionState, ManisApp};
    use crate::mihomo::{self, RemoteSourceRefreshInterval};
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

    #[test]
    fn parses_deduplicates_and_validates_proxy_dns_lines() {
        let parsed = parse_proxy_dns_lines(
            "\n  https://20.247.42.211:36290/dns-query/clash?site=naiu \n\nhttps://20.247.42.211:36290/dns-query/clash?site=naiu\nhttps://20.239.241.138:55228/dns-query/clash?site=naiu\n",
        )
        .expect("valid lines");
        assert_eq!(parsed.len(), 2);
        assert_eq!(
            parsed[0].as_str(),
            "https://20.247.42.211:36290/dns-query/clash?site=naiu"
        );

        assert!(
            parse_proxy_dns_lines("   \n\n")
                .expect("empty clears")
                .is_empty()
        );
        assert_eq!(
            parse_proxy_dns_lines("https://223.5.5.5/dns-query\nhttp://insecure.example/dns"),
            Err(ProxyDnsEditError::InvalidLine(2))
        );

        let too_many = (0..=mihomo::MAX_SUBSCRIPTION_PROXY_DNS_SERVERS)
            .map(|i| format!("https://192.0.2.{}/dns-query", i + 1))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            parse_proxy_dns_lines(&too_many),
            Err(ProxyDnsEditError::TooMany(
                mihomo::MAX_SUBSCRIPTION_PROXY_DNS_SERVERS
            ))
        );
    }

    #[test]
    fn effective_proxy_dns_prefers_custom_then_enabled_subscriptions_then_default() {
        let mut app = ManisApp::with_fixture_controller("http://127.0.0.1:9090");
        let default = app.effective_proxy_dns();
        assert_eq!(default.source, ProxyDnsSourceKind::Default);
        assert_eq!(default.entries.len(), 2);

        app.imported_subscriptions.push(ImportedSubscription {
            id: "sub:disabled".to_owned(),
            name: "DisabledSub".to_owned(),
            source: manis_profile::SecretUrl::parse_subscription(
                "https://disabled.example.invalid/sub",
            )
            .expect("url"),
            enabled: false,
            state: ImportedSubscriptionState::None,
            providers: Vec::new(),
            generation: 0,
            refresh_interval: RemoteSourceRefreshInterval::Manual,
            last_successful_update_unix_secs: 0,
            proxy_server_nameservers: vec![
                ProxyDnsServer::parse_https("https://198.51.100.1/dns-query").expect("doh"),
            ],
        });
        assert_eq!(
            app.effective_proxy_dns().source,
            ProxyDnsSourceKind::Default
        );

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
            proxy_server_nameservers: vec![
                ProxyDnsServer::parse_https(
                    "https://20.247.42.211:36290/dns-query/clash?site=naiu",
                )
                .expect("doh"),
                ProxyDnsServer::parse_https(
                    "https://20.239.241.138:55228/dns-query/clash?site=naiu",
                )
                .expect("doh"),
            ],
        });
        let from_sub = app.effective_proxy_dns();
        assert_eq!(from_sub.source, ProxyDnsSourceKind::Subscription);
        assert_eq!(from_sub.entries.len(), 2);
        assert_eq!(from_sub.entries[0].origin.as_deref(), Some("NaiU_Net"));

        app.custom_proxy_dns_servers =
            vec![ProxyDnsServer::parse_https("https://203.0.113.10/dns-query").expect("custom")];
        let custom = app.effective_proxy_dns();
        assert_eq!(custom.source, ProxyDnsSourceKind::Custom);
        assert_eq!(custom.entries.len(), 1);
        assert_eq!(custom.entries[0].url, "https://203.0.113.10/dns-query");
        assert_eq!(custom.entries[0].origin, None);
    }

    fn seed_subscription_with_doh(store: &std::path::Path, doh_url: &str) {
        let saved = mihomo::save_subscription_source_with_options_in(
            store,
            "https://naiu.example.invalid/sub",
            "NaiU_Net",
            RemoteSourceRefreshInterval::Hourly,
            true,
        )
        .expect("save subscription");
        mihomo::update_subscription_source_proxy_nameservers_in(
            store,
            &saved.id,
            &[ProxyDnsServer::parse_https(doh_url).expect("doh")],
        )
        .expect("save subscription doh");
    }

    #[gpui::test]
    fn desktop_settings_edits_saves_and_resets_custom_proxy_dns(cx: &mut gpui::TestAppContext) {
        let store = temp_store("editor-round-trip");
        let doh_url = "https://20.247.42.211:36290/dns-query/clash?site=naiu";
        seed_subscription_with_doh(&store, doh_url);

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
                this.primary_workspace = manis_core::PrimaryWorkspace::Configuration;
                this.scroll_to_configuration_section(crate::app::ConfigurationSection::Runtime, cx);
                assert_eq!(
                    this.effective_proxy_dns().source,
                    ProxyDnsSourceKind::Subscription
                );
                this.open_proxy_dns_editor(window, cx);
                assert!(this.proxy_dns_editor.editing);
                let editor = this
                    .proxy_dns_editor
                    .input
                    .as_ref()
                    .expect("editor initialized")
                    .clone();
                assert_eq!(editor.read(cx).value().as_ref(), doh_url);

                // Invalid line stays in editor and surfaces line error.
                editor.update(cx, |state, cx| {
                    state.set_value(
                        "https://192.0.2.99:8443/dns-query\nhttp://insecure.example/dns",
                        window,
                        cx,
                    );
                });
                this.save_proxy_dns_editor(cx);
                assert!(this.proxy_dns_editor.editing);
                assert_eq!(
                    this.proxy_dns_editor.error,
                    Some(ProxyDnsEditError::InvalidLine(2))
                );

                // Valid edit persists via transactional store mutation and updates app state.
                editor.update(cx, |state, cx| {
                    state.set_value(
                        "https://192.0.2.99:8443/dns-query\nhttps://198.51.100.77/dns-query",
                        window,
                        cx,
                    );
                });
                let parsed = parse_proxy_dns_lines(&editor.read(cx).value()).expect("valid lines");
                let mutation = mutate_saved_sources_with_apply(
                    &store,
                    |staged| {
                        mihomo::save_custom_proxy_dns_servers_in(staged, &parsed)?;
                        mihomo::load_custom_proxy_dns_servers_in(staged)
                    },
                    || crate::app::SourceRuntimeApply::MetadataOnly,
                );
                this.finish_proxy_dns_save(mutation, cx);
                assert!(!this.proxy_dns_editor.editing);
                assert_eq!(
                    this.effective_proxy_dns().source,
                    ProxyDnsSourceKind::Custom
                );
                assert_eq!(this.effective_proxy_dns().entries.len(), 2);

                // Resetting clears proxy-dns.state and falls back to subscription DoH.
                let reset = mutate_saved_sources_with_apply(
                    &store,
                    |staged| {
                        mihomo::save_custom_proxy_dns_servers_in(staged, &[])?;
                        mihomo::load_custom_proxy_dns_servers_in(staged)
                    },
                    || crate::app::SourceRuntimeApply::MetadataOnly,
                );
                this.finish_proxy_dns_save(reset, cx);
                assert_eq!(
                    this.effective_proxy_dns().source,
                    ProxyDnsSourceKind::Subscription
                );
                assert!(
                    mihomo::load_custom_proxy_dns_servers_in(&store)
                        .expect("reload cleared")
                        .is_empty()
                );
            });
        });

        cx.run_until_parked();
        let _ = fs::remove_dir_all(store);
    }
}
