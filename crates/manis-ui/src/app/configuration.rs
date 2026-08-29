use std::path::Path;

use gpui::{
    AnyElement, Context, Div, Entity, Focusable, FontWeight, KeyDownEvent, ParentElement, Role,
    Stateful, StyleRefinement, Styled, Window, div, prelude::*, px,
};
use gpui_component::{
    Disableable, IconName, Selectable, Sizable, Size, WindowExt as _,
    accordion::Accordion,
    button::{Button, ButtonVariant, ButtonVariants},
    checkbox::Checkbox,
    dialog::Dialog,
    menu::{ContextMenuExt, PopupMenuItem},
};
use manis_core::{KernelKind, ProxyMode, WindowSizeClass};
use manis_profile::{QxRuleKind, SecretUrl};

use super::{
    ConfigurationSection, ImportQxRuleError, ImportQxRuleSuccess, ImportedSubscriptionState,
    ManisApp, ManualRulePopover, MihomoCoreUpdateState, ProxySourceEditorKind,
    QxRuleImportFeedback, QxRuleList, QxRuleSourceRefreshState, SourceRuntimeApply,
    SubscriptionFeedback, proxy_mode_label, routing_mode_label,
};
use crate::{
    components::{
        ActionRole, StatusTone, action_button, empty_state, page_heading, section_heading,
        status_badge, style_action_button,
    },
    diagnostics::{LogLevel, UiEvent, begin_operation, record_event, record_operation, trace_ui},
    localization::{CountNoun, Language, LanguagePreference, Message, save_language_preference_in},
    mihomo::{self, RemoteSourceRefreshInterval, RuntimeProfileSource, SubscriptionStoreError},
    rule_source::{download_qx_rule_document, download_qx_rule_document_secret},
    subscription::{SourceKind, validate_single_node_preview, validate_subscription_preview},
    subscription_input::{SubscriptionTextInput, TextInputSpec},
    theme::{ControlSize, Radius, Space, TextRole, Theme},
};

const MAX_MANUAL_RULE_INPUT_BYTES: usize = 1_024;
const MANUAL_RULES_EXPANSION_KEY: &str = "routing-manual-rules";

struct QxRuleSaveRequest {
    url: String,
    target: String,
    editing_id: Option<String>,
    refresh_interval: RemoteSourceRefreshInterval,
}

struct SubscriptionCardPresentation {
    state: String,
    activity: SubscriptionCardActivity,
    controls_enabled: bool,
    updated: String,
}

#[derive(Clone, Copy)]
enum SubscriptionCardActivity {
    Idle { healthy: bool },
    Busy,
}

impl SubscriptionCardActivity {
    const fn is_busy(self) -> bool {
        matches!(self, Self::Busy)
    }

    const fn is_healthy(self) -> bool {
        matches!(self, Self::Idle { healthy: true } | Self::Busy)
    }
}

fn save_qx_rule_source(
    runtime: &super::KernelRuntime,
    store_dir: &Path,
    request: QxRuleSaveRequest,
) -> super::QxRuleImportResult {
    let QxRuleSaveRequest {
        url,
        target,
        editing_id,
        refresh_interval,
    } = request;
    let content = download_qx_rule_document(&url).map_err(ImportQxRuleError::Download)?;
    if QxRuleList::parse(&content).rules.is_empty() {
        return Err(ImportQxRuleError::InvalidDocument);
    }
    if let Some(editing_id) = editing_id {
        return replace_qx_rule_source(
            runtime,
            store_dir,
            &editing_id,
            &url,
            &target,
            &content,
            refresh_interval,
        );
    }
    create_qx_rule_source(
        runtime,
        store_dir,
        &url,
        &target,
        &content,
        refresh_interval,
    )
}

fn replace_qx_rule_source(
    runtime: &super::KernelRuntime,
    store_dir: &Path,
    id: &str,
    url: &str,
    target: &str,
    content: &str,
    refresh_interval: RemoteSourceRefreshInterval,
) -> super::QxRuleImportResult {
    let transaction = super::mutate_saved_sources(runtime, store_dir, || {
        mihomo::replace_qx_rule_source_definition_in(
            store_dir,
            id,
            url,
            target,
            content,
            refresh_interval,
            mihomo::current_unix_secs(),
        )
    })
    .map_err(ImportQxRuleError::Store)?;
    Ok(match transaction.value {
        Some(stored) => ImportQxRuleSuccess::Imported {
            stored,
            apply: transaction.apply,
        },
        None => ImportQxRuleSuccess::RolledBack {
            apply: transaction.apply,
            rollback_error: transaction.rollback_error,
        },
    })
}

fn create_qx_rule_source(
    runtime: &super::KernelRuntime,
    store_dir: &Path,
    url: &str,
    target: &str,
    content: &str,
    refresh_interval: RemoteSourceRefreshInterval,
) -> super::QxRuleImportResult {
    let transaction = super::mutate_saved_sources(runtime, store_dir, || {
        let outcome = mihomo::save_qx_rule_source_in(store_dir, url, target, content)?;
        let mihomo::SaveQxRuleSourceOutcome::Created(mut stored) = outcome else {
            return Ok(outcome);
        };
        if refresh_interval != RemoteSourceRefreshInterval::Manual {
            stored = mihomo::update_qx_rule_source_refresh_interval_in(
                store_dir,
                &stored.id,
                refresh_interval,
            )?;
        }
        Ok(mihomo::SaveQxRuleSourceOutcome::Created(stored))
    })
    .map_err(ImportQxRuleError::Store)?;
    Ok(match transaction.value {
        Some(mihomo::SaveQxRuleSourceOutcome::Created(stored)) => ImportQxRuleSuccess::Imported {
            stored,
            apply: transaction.apply,
        },
        Some(mihomo::SaveQxRuleSourceOutcome::Existing(stored)) => {
            ImportQxRuleSuccess::AlreadyExists { stored }
        }
        None => ImportQxRuleSuccess::RolledBack {
            apply: transaction.apply,
            rollback_error: transaction.rollback_error,
        },
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ManualRuleKeyboardAction {
    Edit,
    Toggle,
    Delete,
}

fn manual_rule_keyboard_action(event: &KeyDownEvent) -> Option<ManualRuleKeyboardAction> {
    manual_rule_keyboard_action_for(
        event.keystroke.key.as_str(),
        event.keystroke.modifiers.modified(),
        event.is_held,
    )
}

fn manual_rule_keyboard_action_for(
    key: &str,
    modified: bool,
    held: bool,
) -> Option<ManualRuleKeyboardAction> {
    if held || modified {
        return None;
    }
    match key {
        "enter" => Some(ManualRuleKeyboardAction::Edit),
        "space" => Some(ManualRuleKeyboardAction::Toggle),
        "backspace" | "delete" => Some(ManualRuleKeyboardAction::Delete),
        _ => None,
    }
}

fn rule_group_is_open(workspace: &manis_core::NodeWorkspaceState, key: &str) -> bool {
    !workspace.is_group_collapsed(key)
}

fn panel_surface(id: &'static str, compact: bool, theme: Theme) -> Stateful<Div> {
    div()
        .id(id)
        .w_full()
        .p(if compact {
            Space::Md.px()
        } else {
            Space::Lg.px()
        })
        .rounded(Radius::Pane.px())
        .border_1()
        .border_color(theme.outline_subtle)
        .bg(theme.surface_high)
}

fn field_label(label: impl Into<gpui::SharedString>, theme: Theme) -> Div {
    let label = label.into();
    div()
        .mb(Space::Xs.px())
        .text_size(TextRole::Label.size())
        .line_height(TextRole::Label.line_height())
        .font_weight(TextRole::Label.weight())
        .text_color(theme.text_secondary)
        .child(label)
}

fn accordion_content_style() -> StyleRefinement {
    let mut style = StyleRefinement::default();
    style.padding.top = Some(px(0.0).into());
    style.padding.right = Some(px(0.0).into());
    style.padding.bottom = Some(px(0.0).into());
    style.padding.left = Some(px(0.0).into());
    style
}

fn accordion_title_style(compact: bool) -> StyleRefinement {
    let mut style = StyleRefinement::default();
    if compact {
        style.padding.right = Some(px(12.0).into());
        style.padding.left = Some(px(12.0).into());
    }
    style
}

fn manual_rule_placeholder(
    kind: crate::manual_rule::ManualRuleKind,
    language: Language,
) -> &'static str {
    use crate::manual_rule::ManualRuleKind;
    match kind {
        ManualRuleKind::Host
        | ManualRuleKind::HostSuffix
        | ManualRuleKind::HostWildcard
        | ManualRuleKind::HostKeyword => "example.com",
        ManualRuleKind::UserAgent => "*abc?",
        ManualRuleKind::IpCidr => "192.168.0.1/24",
        ManualRuleKind::Ip6Cidr => "2001:4860:4860::8888/32",
        ManualRuleKind::GeoIp => language.text("US", "US（国家代码）"),
        ManualRuleKind::IpAsn => "6185",
        ManualRuleKind::DstPort => "22",
        ManualRuleKind::Final => "",
    }
}

fn manual_rule_kind_detail(
    kind: crate::manual_rule::ManualRuleKind,
    language: Language,
) -> &'static str {
    use crate::manual_rule::ManualRuleKind;
    match kind {
        ManualRuleKind::Host => language.text("Exact domain", "完整域名"),
        ManualRuleKind::HostSuffix => language.text("Domain suffix", "域名后缀"),
        ManualRuleKind::HostWildcard => language.text("Wildcard domain", "通配符域名"),
        ManualRuleKind::HostKeyword => language.text("Domain contains keyword", "域名中包含关键词"),
        ManualRuleKind::UserAgent => language.text("Browser user agent", "浏览器标识"),
        ManualRuleKind::IpCidr => language.text("IPv4 address range", "IPv4 地址段"),
        ManualRuleKind::Ip6Cidr => language.text("IPv6 address range", "IPv6 地址段"),
        ManualRuleKind::GeoIp => language.text("Country or region", "国家或地区"),
        ManualRuleKind::IpAsn => language.text("Autonomous system", "自治系统"),
        ManualRuleKind::DstPort => language.text("Destination port", "目标端口"),
        ManualRuleKind::Final => language.text(
            "Fallback for traffic not matched above",
            "兜底处理此前未命中的流量",
        ),
    }
}

fn manual_rule_error_label(
    error: crate::manual_rule::ManualRuleError,
    language: Language,
) -> &'static str {
    use crate::manual_rule::ManualRuleError;
    match error {
        ManualRuleError::Empty => language.text("Enter a match parameter", "请输入匹配参数"),
        ManualRuleError::InvalidDomain => language.text(
            "Enter a plain domain such as example.com",
            "请输入纯域名，例如 example.com",
        ),
        ManualRuleError::InvalidWildcard => language.text(
            "Enter a domain pattern such as *.example.com",
            "请输入域名模式，例如 *.example.com",
        ),
        ManualRuleError::InvalidKeyword => language.text(
            "The parameter cannot contain commas, tabs, or line breaks",
            "参数不能包含逗号、制表符或换行",
        ),
        ManualRuleError::InvalidIpv4Cidr => language.text(
            "Enter an IPv4 CIDR such as 192.168.0.1/24",
            "请输入 IPv4 CIDR，例如 192.168.0.1/24",
        ),
        ManualRuleError::InvalidIpv6Cidr => language.text(
            "Enter an IPv6 CIDR such as 2001:4860:4860::8888/32",
            "请输入 IPv6 CIDR，例如 2001:4860:4860::8888/32",
        ),
        ManualRuleError::InvalidGeoIp => language.text(
            "Enter a two-letter country code such as US",
            "请输入两位国家代码，例如 US",
        ),
        ManualRuleError::InvalidAsn => language.text(
            "Enter an ASN number such as 6185",
            "请输入 ASN 数字，例如 6185",
        ),
        ManualRuleError::InvalidDestinationPort => language.text(
            "Enter a destination port between 1 and 65535",
            "请输入 1 到 65535 之间的目标端口",
        ),
        ManualRuleError::InvalidPolicy => {
            language.text("Choose an existing policy group", "请选择已有策略组")
        }
        ManualRuleError::UnsupportedByKernel => language.text(
            "This rule type cannot be matched exactly by the current kernel",
            "当前内核无法精确匹配这种规则类型",
        ),
        ManualRuleError::Duplicate => {
            language.text("This manual rule already exists", "这条手动规则已经存在")
        }
        ManualRuleError::DuplicateCondition => language.text(
            "The same condition appears more than once",
            "同一个匹配条件不能重复添加",
        ),
        ManualRuleError::TooManyConditions => language.text(
            "A rule can contain at most four conditions",
            "一条规则最多包含四个条件",
        ),
        ManualRuleError::FinalMustStandAlone => language.text(
            "FINAL cannot be combined with another condition",
            "FINAL 不能和其他匹配条件组合",
        ),
        ManualRuleError::FinalHasNoParameter => language.text(
            "FINAL does not need a match parameter",
            "FINAL 不需要匹配参数",
        ),
        ManualRuleError::FinalAlreadyExists => language.text(
            "Only one FINAL rule can be configured",
            "只能配置一条 FINAL 规则",
        ),
    }
}

fn source_kind_label(kind: SourceKind, language: Language) -> &'static str {
    match kind {
        SourceKind::HttpSubscription => language.text("HTTP subscription", "HTTP 订阅"),
        SourceKind::HttpsSubscription => language.text("HTTPS subscription", "HTTPS 订阅"),
        SourceKind::SingleNode => language.text("Single node", "单节点"),
    }
}

fn source_update_label(
    last_successful_update_unix_secs: u64,
    now_unix_secs: u64,
    language: Language,
) -> String {
    if last_successful_update_unix_secs == 0 {
        return language.text("Never updated", "从未更新").to_owned();
    }
    let elapsed = now_unix_secs.saturating_sub(last_successful_update_unix_secs);
    match elapsed {
        0..=59 => language.text("Updated just now", "刚刚更新").to_owned(),
        60..=3_599 => {
            let minutes = elapsed / 60;
            if language == Language::English {
                format!("Updated {minutes} min ago")
            } else {
                format!("{minutes} 分钟前更新")
            }
        }
        3_600..=86_399 => {
            let hours = elapsed / 3_600;
            if language == Language::English {
                format!("Updated {hours} hr ago")
            } else {
                format!("{hours} 小时前更新")
            }
        }
        _ => {
            let days = elapsed / 86_400;
            if language == Language::English {
                format!("Updated {days} d ago")
            } else {
                format!("{days} 天前更新")
            }
        }
    }
}

fn rule_source_expansion_key(source_id: &str) -> String {
    format!("routing-rule-source:{source_id}")
}

fn refresh_interval_label(
    interval: RemoteSourceRefreshInterval,
    language: Language,
) -> &'static str {
    match interval {
        RemoteSourceRefreshInterval::Manual => language.text("Manual", "手动"),
        RemoteSourceRefreshInterval::Hourly => language.text("Every 1 hour", "每 1 小时"),
        RemoteSourceRefreshInterval::SixHours => language.text("Every 6 hours", "每 6 小时"),
        RemoteSourceRefreshInterval::TwelveHours => language.text("Every 12 hours", "每 12 小时"),
        RemoteSourceRefreshInterval::Daily => language.text("Daily", "每天"),
    }
}

fn language_preference_label(preference: LanguagePreference, language: Language) -> &'static str {
    match preference {
        LanguagePreference::FollowSystem => language.text("Follow system", "跟随系统"),
        LanguagePreference::English => "English",
        LanguagePreference::SimplifiedChinese => "中文",
    }
}

fn configuration_section_label(section: ConfigurationSection, language: Language) -> &'static str {
    match section {
        ConfigurationSection::General => language.text("General", "通用"),
        ConfigurationSection::Runtime => language.text("Runtime", "运行内核"),
        ConfigurationSection::ProxySources => language.text("Proxy sources", "代理来源"),
        ConfigurationSection::RuleSources => language.text("Rule sources", "规则来源"),
        ConfigurationSection::Advanced => language.text("Advanced", "高级设置"),
    }
}

fn configuration_section_detail(section: ConfigurationSection, language: Language) -> &'static str {
    match section {
        ConfigurationSection::General => language.text("Interface language", "界面语言"),
        ConfigurationSection::Runtime => language.text("Core and updates", "内核与更新"),
        ConfigurationSection::ProxySources => {
            language.text("Subscriptions and nodes", "订阅与单节点")
        }
        ConfigurationSection::RuleSources => language.text("Remote rule sets", "远程规则集"),
        ConfigurationSection::Advanced => language.text("Network behavior", "网络行为"),
    }
}

impl ManisApp {
    pub(super) fn configuration_workspace(
        &self,
        theme: Theme,
        size_class: WindowSizeClass,
        cx: &mut Context<Self>,
    ) -> Div {
        let compact = size_class == WindowSizeClass::Compact;
        let language = self.language();
        let rule_input = self
            .qx_rule_input
            .as_ref()
            .expect("QX rule input is initialized before rendering")
            .clone();
        let rule_busy =
            self.qx_rule_feedback == QxRuleImportFeedback::Importing || self.source_refresh_busy();
        let selected_section = self.configuration_section;
        let detail: AnyElement = match selected_section {
            ConfigurationSection::General => {
                self.language_panel(theme, compact, cx).into_any_element()
            }
            ConfigurationSection::Runtime => {
                self.kernel_panel(theme, compact, cx).into_any_element()
            }
            ConfigurationSection::ProxySources => {
                self.source_panel(theme, compact, cx).into_any_element()
            }
            ConfigurationSection::RuleSources => self
                .rule_source_manager(rule_input, rule_busy, theme, compact, cx)
                .into_any_element(),
            ConfigurationSection::Advanced => self
                .advanced_configuration_panel(theme, compact)
                .into_any_element(),
        };
        let navigation = self.configuration_navigation(theme, compact, cx);
        let content = div()
            .id("configuration-detail-scroll")
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .overflow_y_scroll()
            .p(if compact { px(12.0) } else { px(24.0) })
            .pb(px(56.0))
            .child(div().w_full().max_w(px(900.0)).mx_auto().child(detail));
        div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.surface_base)
            .child(Self::workspace_header(
                language.message(Message::Configuration),
                language.text(
                    "Manage Manis preferences and data sources",
                    "管理 Manis 偏好与数据来源",
                ),
                configuration_section_label(selected_section, language),
                StatusTone::Neutral,
                theme,
                compact,
            ))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .when(compact, gpui::Styled::flex_col)
                    .child(navigation)
                    .child(content),
            )
    }

    fn configuration_navigation(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Div {
        let language = self.language();
        let navigation = div()
            .flex_shrink_0()
            .bg(theme.surface_low)
            .when(compact, |navigation| {
                navigation
                    .w_full()
                    .px(Space::Md.px())
                    .py(Space::Sm.px())
                    .border_b_1()
            })
            .when(!compact, |navigation| {
                navigation
                    .w(px(228.0))
                    .h_full()
                    .p(Space::Md.px())
                    .border_r_1()
                    .flex()
                    .flex_col()
            })
            .border_color(theme.outline_subtle)
            .when(!compact, |navigation| {
                navigation.child(
                    div()
                        .px(Space::Sm.px())
                        .pb(Space::Sm.px())
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_tertiary)
                        .child(language.text("SETTINGS", "设置")),
                )
            });
        let items =
            div()
                .id("configuration-navigation-items")
                .flex()
                .gap(if compact {
                    Space::Xs.px()
                } else {
                    Space::Sm.px()
                })
                .when(compact, gpui::StatefulInteractiveElement::overflow_x_scroll)
                .when(!compact, gpui::Styled::flex_col)
                .children(ConfigurationSection::ALL.into_iter().map(|section| {
                    self.configuration_navigation_item(section, theme, compact, cx)
                }));
        navigation.child(items).when(!compact, |navigation| {
            navigation.child(
                div()
                    .mt_auto()
                    .px(Space::Sm.px())
                    .pt(Space::Lg.px())
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_tertiary)
                    .child(language.text("Changes are stored locally", "更改仅保存在本机")),
            )
        })
    }

    fn configuration_navigation_item(
        &self,
        section: ConfigurationSection,
        theme: Theme,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let language = self.language();
        let selected = self.configuration_section == section;
        let metadata = match section {
            ConfigurationSection::General => self.language().display_name().to_owned(),
            ConfigurationSection::Runtime => self.runtime.kind().display_name().to_owned(),
            ConfigurationSection::ProxySources => language.count(
                CountNoun::Source,
                self.imported_subscriptions.len() + self.saved_single_nodes.len(),
            ),
            ConfigurationSection::RuleSources => {
                language.count(CountNoun::Source, self.qx_rule_sources.len())
            }
            ConfigurationSection::Advanced => language.text("Managed", "托管").to_owned(),
        };
        div()
            .id(format!("configuration-nav-{}", section.key()))
            .role(Role::Button)
            .aria_label(configuration_section_label(section, language))
            .aria_toggled(if selected {
                gpui::Toggled::True
            } else {
                gpui::Toggled::False
            })
            .tab_stop(true)
            .focusable()
            .cursor_pointer()
            .min_w(if compact { px(104.0) } else { px(0.0) })
            .px(Space::Md.px())
            .py(Space::Sm.px())
            .rounded(Radius::Control.px())
            .border_1()
            .border_color(if selected {
                theme.action_primary
            } else {
                theme.surface_low
            })
            .bg(if selected {
                theme.action_soft
            } else {
                theme.surface_low
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(Space::Sm.px())
                    .child(
                        div()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
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
                            .child(configuration_section_label(section, language)),
                    )
                    .when(!compact, |row| {
                        row.child(
                            div()
                                .text_size(TextRole::Metadata.size())
                                .line_height(TextRole::Metadata.line_height())
                                .text_color(theme.text_tertiary)
                                .child(metadata),
                        )
                    }),
            )
            .when(!compact, |item| {
                item.child(
                    div()
                        .mt(px(2.0))
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .text_color(theme.text_secondary)
                        .child(configuration_section_detail(section, language)),
                )
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.configuration_section = section;
                cx.notify();
            }))
    }

    fn advanced_configuration_panel(&self, theme: Theme, compact: bool) -> Stateful<Div> {
        let language = self.language();
        let profile_source = self.runtime.profile_source();
        let profile_detail = if language == Language::SimplifiedChinese {
            profile_source.detail()
        } else {
            match profile_source {
                #[cfg(any(test, feature = "snapshot-fixtures"))]
                RuntimeProfileSource::FixtureController => "Test snapshot only",
                RuntimeProfileSource::SavedSources => "Compiled from private local sources",
                RuntimeProfileSource::Invalid => "Check local startup arguments",
            }
        };
        panel_surface("configuration-advanced", compact, theme)
            .child(section_heading(
                language.text("Advanced settings", "高级设置"),
                language.text("Current managed network behavior", "当前托管网络行为"),
                Some(
                    status_badge(
                        language.text("Managed", "Manis 托管"),
                        StatusTone::Neutral,
                        theme,
                    )
                    .into_any_element(),
                ),
                theme,
            ))
            .child(Self::advanced_configuration_row(
                language.text("Proxy mode", "代理模式"),
                proxy_mode_label(language, self.proxy_mode),
                language.text("Changed from the main toolbar", "可在主工具栏中切换"),
                theme,
            ))
            .child(Self::advanced_configuration_row(
                language.text("Routing mode", "路由模式"),
                routing_mode_label(language, self.routing_mode),
                language.text("Direct, global, or ordered rules", "直连、全局或有序规则"),
                theme,
            ))
            .child(Self::advanced_configuration_row(
                language.text("Process identification", "进程识别"),
                language.text("Always", "始终识别"),
                language.text(
                    "Used to improve Network Activity",
                    "用于改善网络活动中的进程信息",
                ),
                theme,
            ))
            .child(Self::advanced_configuration_row(
                language.text("DNS and TUN", "DNS 与 TUN"),
                language.text("Automatic", "自动管理"),
                profile_detail,
                theme,
            ))
    }

    fn advanced_configuration_row(
        label: &'static str,
        value: &'static str,
        detail: &'static str,
        theme: Theme,
    ) -> Div {
        div()
            .mt(Space::Md.px())
            .pt(Space::Md.px())
            .border_t_1()
            .border_color(theme.outline_subtle)
            .flex()
            .items_center()
            .justify_between()
            .gap(Space::Lg.px())
            .child(
                div()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(TextRole::Label.weight())
                            .text_color(theme.text_primary)
                            .child(label),
                    )
                    .child(
                        div()
                            .mt(Space::Xs.px())
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .text_color(theme.text_secondary)
                            .child(detail),
                    ),
            )
            .child(status_badge(value, StatusTone::Neutral, theme))
    }

    fn language_panel(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let language = self.language();
        let current_preference = self.language_preference();
        let current_language = language.display_name();
        panel_surface("configuration-language", compact, theme)
            .child(section_heading(
                language.text("Interface language", "界面语言"),
                "",
                Some(
                    status_badge(
                        format!("{} · {current_language}", language.text("Current", "当前")),
                        StatusTone::Neutral,
                        theme,
                    )
                    .into_any_element(),
                ),
                theme,
            ))
            .child(
                div()
                    .mt(Space::Md.px())
                    .grid()
                    .gap(Space::Sm.px())
                    .grid_cols(if compact { 1 } else { 3 })
                    .children(
                        [
                            LanguagePreference::FollowSystem,
                            LanguagePreference::English,
                            LanguagePreference::SimplifiedChinese,
                        ]
                        .into_iter()
                        .map(|preference| {
                            Self::language_option(
                                preference,
                                preference == current_preference,
                                language,
                                theme,
                                cx,
                            )
                        }),
                    ),
            )
    }

    fn language_option(
        preference: LanguagePreference,
        selected: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let label = language_preference_label(preference, language);
        div()
            .id(format!(
                "language-option-{}",
                preference.persistence_key().replace('-', "_")
            ))
            .role(Role::Button)
            .aria_label(format!(
                "{}: {label}",
                language.text("Select language", "选择界面语言")
            ))
            .aria_toggled(if selected {
                gpui::Toggled::True
            } else {
                gpui::Toggled::False
            })
            .tab_stop(true)
            .focusable()
            .cursor_pointer()
            .min_h(px(52.0))
            .px(Space::Md.px())
            .py(Space::Sm.px())
            .rounded(Radius::Row.px())
            .border_1()
            .border_color(if selected {
                theme.action_primary
            } else {
                theme.outline_subtle
            })
            .bg(if selected {
                theme.action_soft
            } else {
                theme.surface_low
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(TextRole::Label.weight())
                            .text_color(if selected {
                                theme.action_primary
                            } else {
                                theme.text_primary
                            })
                            .child(label),
                    )
                    .when(selected, |row| {
                        row.child(
                            div()
                                .text_size(TextRole::Metadata.size())
                                .line_height(TextRole::Metadata.line_height())
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.action_primary)
                                .child(language.text("Selected", "已选择")),
                        )
                    }),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.set_language_preference(preference, cx);
            }))
    }

    fn set_language_preference(&mut self, preference: LanguagePreference, cx: &mut Context<Self>) {
        self.localizer.set_preference(preference);
        let language = self.language();
        match self.subscription_store_dir.as_ref() {
            Some(store_dir) => match save_language_preference_in(store_dir, preference) {
                Ok(_path) => {
                    self.status = format!(
                        "{} · {}",
                        language.text("Language saved", "界面语言已保存"),
                        language_preference_label(preference, language)
                    );
                }
                Err(error) => {
                    self.status = format!(
                        "{}: {error}",
                        language.text(
                            "Language changed but could not be saved",
                            "界面语言已切换，但保存失败"
                        )
                    );
                }
            },
            None => {
                language
                    .text(
                        "Language changed for this session; data directory unavailable.",
                        "界面语言已在本次会话生效；无法确定保存位置。",
                    )
                    .clone_into(&mut self.status);
            }
        }
        if let Some(input) = self.subscription_input.as_ref() {
            input.update(cx, |input, cx| input.set_language(language, cx));
        }
        if let Some(input) = self.subscription_name_input.as_ref() {
            input.update(cx, |input, cx| {
                input.set_placeholder(
                    language.text("For example: My subscription", "例如：我的订阅"),
                    cx,
                );
            });
        }
        if let Some(input) = self.policy_group_name_input.as_ref() {
            input.update(cx, |input, cx| {
                input.set_placeholder(
                    language.text("For example: Hong Kong Auto", "例如：香港自动优选"),
                    cx,
                );
            });
        }
        if let Some(input) = self.policy_group_filter_input.as_ref() {
            input.update(cx, |input, cx| {
                input.set_placeholder(
                    language.text("For example: Hong Kong", "例如：Hong Kong"),
                    cx,
                );
            });
        }
        cx.notify();
    }

    fn kernel_panel(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let language = self.language();
        let active = self.runtime.kind();
        let (sing_box_reason, sing_box_supported) = self.sing_box_support(language);
        let sing_box_enabled = sing_box_supported
            && !self.kernel_switch_state.is_busy()
            && active != KernelKind::SingBox;

        panel_surface("configuration-kernel", compact, theme)
            .child(section_heading(
                language.text("Runtime kernel", "运行内核"),
                "",
                Some(
                    status_badge(
                        if self.kernel_switch_state.is_busy() {
                            language.text("Validating", "正在校验")
                        } else {
                            active.display_name()
                        },
                        if self.kernel_switch_state.is_busy() {
                            StatusTone::Warning
                        } else {
                            StatusTone::Neutral
                        },
                        theme,
                    )
                    .into_any_element(),
                ),
                theme,
            ))
            .child(Self::kernel_option_row(
                KernelKind::Mihomo,
                language.text(
                    "Subscriptions, policy groups, and latency tests",
                    "支持订阅、策略组与测速",
                ),
                !self.kernel_switch_state.is_busy() && active != KernelKind::Mihomo,
                active == KernelKind::Mihomo,
                language,
                theme,
                cx,
            ))
            .child(self.mihomo_core_update_row(language, theme, cx))
            .child(Self::kernel_option_row(
                KernelKind::SingBox,
                sing_box_reason,
                sing_box_enabled,
                active == KernelKind::SingBox,
                language,
                theme,
                cx,
            ))
    }

    fn sing_box_support(&self, language: Language) -> (&'static str, bool) {
        if !mihomo::sing_box_binary_available() {
            return (
                language.text(
                    "sing-box was not found on this device",
                    "本机未检测到 sing-box",
                ),
                false,
            );
        }
        if self
            .imported_subscriptions
            .iter()
            .any(|subscription| subscription.enabled)
        {
            return (
                language.text(
                    "Clash subscriptions are present; Manis needs its native parser first",
                    "当前包含 Clash 订阅，需等待 Manis 原生订阅解析器",
                ),
                false,
            );
        }
        if self.saved_single_nodes.is_empty() {
            return (
                language.text(
                    "At least one saved VLESS node is required",
                    "至少需要一个已保存的 VLESS 节点",
                ),
                false,
            );
        }
        (
            language.text(
                "Supports manual VLESS, selectors, URL tests, and routing rules",
                "支持手动 VLESS、选择器、自动测速与分流规则",
            ),
            true,
        )
    }

    fn mihomo_core_update_row(
        &self,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let updating = self.mihomo_core_update_state.is_busy();
        let enabled = !updating && self.proxy_mode == ProxyMode::Off;
        let missing = matches!(
            self.mihomo_core_update_state,
            MihomoCoreUpdateState::Missing
        );
        let version = match &self.mihomo_core_update_state {
            MihomoCoreUpdateState::Ready(version) if version.is_empty() => {
                language.text("Installed", "已安装")
            }
            MihomoCoreUpdateState::Ready(version) => version.as_str(),
            MihomoCoreUpdateState::Missing => language.text("Not installed", "尚未安装"),
            MihomoCoreUpdateState::Updating => language.text("Updating…", "正在更新…"),
        };
        div()
            .mt(Space::Sm.px())
            .ml(Space::Md.px())
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_secondary)
                    .child(if language == Language::English {
                        format!("Manis-managed stable core · {version}")
                    } else {
                        format!("Manis 托管稳定版内核 · {version}")
                    }),
            )
            .child(
                style_action_button(
                    Button::new("mihomo-core-update")
                        .accessibility_label(language.text(
                            "Download or update the Manis-managed Mihomo core",
                            "下载或更新 Manis 托管的 Mihomo 内核",
                        ))
                        .label(if updating {
                            language.text("Updating…", "更新中…")
                        } else if missing {
                            language.text("Download stable", "下载稳定版")
                        } else {
                            language.text("Check for update", "检查更新")
                        })
                        .icon(IconName::Redo2)
                        .loading(updating)
                        .disabled(!enabled)
                        .tab_stop(enabled),
                    ActionRole::Quiet,
                    ControlSize::Compact,
                )
                .when(enabled, gpui::Styled::cursor_pointer)
                .on_click(cx.listener(|this, _, _, cx| this.update_mihomo_core(cx))),
            )
    }

    fn kernel_option_row(
        kind: KernelKind,
        detail: &str,
        enabled: bool,
        selected: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .mt(Space::Md.px())
            .pt(Space::Md.px())
            .border_t_1()
            .border_color(theme.outline_subtle)
            .flex()
            .items_center()
            .justify_between()
            .gap(Space::Md.px())
            .child(
                div()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(TextRole::Label.weight())
                            .child(kind.display_name()),
                    )
                    .child(
                        div()
                            .mt(Space::Xs.px())
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .text_color(theme.text_secondary)
                            .child(detail.to_owned()),
                    ),
            )
            .child(
                div()
                    .id(format!("kernel-select-{}", kind.persistence_key()))
                    .role(Role::Button)
                    .aria_label(format!(
                        "{} {}",
                        language.text("Switch to", "切换到"),
                        kind.display_name()
                    ))
                    .tab_stop(enabled)
                    .focusable()
                    .when(enabled, gpui::Styled::cursor_pointer)
                    .h(ControlSize::Compact.height())
                    .px(Space::Md.px())
                    .rounded(Radius::Control.px())
                    .border_1()
                    .border_color(if selected {
                        theme.action_primary
                    } else {
                        theme.outline_subtle
                    })
                    .bg(if selected {
                        theme.action_soft
                    } else {
                        theme.surface_high
                    })
                    .text_size(TextRole::Label.size())
                    .line_height(TextRole::Label.line_height())
                    .font_weight(TextRole::Label.weight())
                    .text_color(if selected || enabled {
                        theme.action_primary
                    } else {
                        theme.text_tertiary
                    })
                    .child(if selected {
                        language.text("Current", "当前使用")
                    } else {
                        language.text("Switch and validate", "切换并校验")
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if enabled {
                            this.switch_kernel(kind, cx);
                        }
                    })),
            )
    }

    pub(super) fn routing_rules_workspace(
        &mut self,
        theme: Theme,
        size_class: WindowSizeClass,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let compact = size_class == WindowSizeClass::Compact;
        let language = self.language();
        self.ensure_manual_rule_input(theme, window, cx);
        div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.surface_base)
            .child(Self::workspace_header(
                language.message(Message::RoutingRules),
                language.text(
                    "Inspect the ordered rules that actually participate in matching; manage sources in Settings",
                    "查看最终参与匹配的有序规则；来源请前往配置页管理",
                ),
                language.text("Top-down", "从上到下匹配"),
                StatusTone::Route,
                theme,
                compact,
            ))
            .child(
                div()
                    .id("routing-rules-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .p(if compact { px(12.0) } else { px(20.0) })
                    .pb(px(56.0))
                    .child(
                        div()
                            .max_w(px(1040.0))
                            .mx_auto()
                            .child(self.active_rules_panel(theme, language, compact, cx)),
                    ),
            )
    }

    fn workspace_header(
        title: &'static str,
        detail: &'static str,
        badge: &'static str,
        badge_tone: StatusTone,
        theme: Theme,
        compact: bool,
    ) -> Div {
        div()
            .px(if compact {
                Space::Lg.px()
            } else {
                Space::Xl.px()
            })
            .py(Space::Md.px())
            .flex_shrink_0()
            .border_b_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .flex()
            .items_start()
            .justify_between()
            .gap(Space::Lg.px())
            .child(page_heading(
                title,
                if compact { "" } else { detail },
                Some(status_badge(badge, badge_tone, theme).into_any_element()),
                theme,
            ))
    }

    fn source_panel(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Div {
        let language = self.language();
        let saved_source_count = self.imported_subscriptions.len() + self.saved_single_nodes.len();
        let add_action = action_button(
            "configuration-add-proxy-source",
            language.text("Add source", "添加来源"),
            ActionRole::Primary,
            ControlSize::Compact,
        )
        .bg(theme.action_primary)
        .text_color(theme.action_on_primary)
        .on_click(cx.listener(|this, _, window, cx| {
            this.open_new_subscription_editor(cx);
            this.open_proxy_source_dialog(window, cx);
        }));

        let panel = panel_surface("configuration-source", compact, theme)
            .child(section_heading(
                language.text("Proxy sources", "代理来源"),
                language.count(CountNoun::Source, saved_source_count),
                Some(add_action.into_any_element()),
                theme,
            ))
            .when_some(self.source_store_error, |panel, error| {
                panel.child(Self::subscription_error(
                    language.text("Some local sources could not be restored", "部分本地来源未能恢复"),
                    error.to_string(),
                    Some(language.text(
                        "Other safely readable sources are kept; check the user data directory permissions.",
                        "其余可安全读取的来源仍然保留；可检查用户数据目录权限。",
                    )),
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
                            .child(language.text("Saved", "已保存")),
                    ),
            )
            .when(saved_source_count == 0, |panel| {
                panel.child(
                    empty_state(
                        language.text("No proxy sources", "暂无代理来源"),
                        language.text(
                            "Add a subscription or a single-node source.",
                            "添加订阅或单节点来源。",
                        ),
                        Some(
                            action_button(
                                "configuration-empty-add-proxy-source",
                                language.text("Add source", "添加来源"),
                                ActionRole::Primary,
                                ControlSize::Compact,
                            )
                            .bg(theme.action_primary)
                            .text_color(theme.action_on_primary)
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

    fn open_new_subscription_editor(&mut self, cx: &mut Context<Self>) {
        self.subscription_editor_source_id = None;
        self.single_node_editor_source_id = None;
        self.proxy_source_editor_kind = ProxySourceEditorKind::Subscription;
        self.subscription_editor_refresh_interval = RemoteSourceRefreshInterval::Manual;
        self.subscription_editor_interval_popover = false;
        self.subscription_editor_enabled = true;
        self.subscription_editor_error = None;
        self.subscription_feedback = SubscriptionFeedback::Idle;
        if let Some(input) = self.subscription_name_input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        if let Some(input) = self.subscription_input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        cx.notify();
    }

    fn open_subscription_editor(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(subscription) = self
            .imported_subscriptions
            .iter()
            .find(|subscription| subscription.id == id)
        else {
            return;
        };
        let name = subscription.name.clone();
        let url = subscription.source.expose_to(str::to_owned);
        self.subscription_editor_source_id = Some(id);
        self.single_node_editor_source_id = None;
        self.proxy_source_editor_kind = ProxySourceEditorKind::Subscription;
        self.subscription_editor_refresh_interval = subscription.refresh_interval;
        self.subscription_editor_interval_popover = false;
        self.subscription_editor_enabled = subscription.enabled;
        self.subscription_editor_error = None;
        self.subscription_feedback = SubscriptionFeedback::Idle;
        if let Some(input) = self.subscription_name_input.as_ref() {
            input.update(cx, |input, cx| input.set_value_without_event(name, cx));
        }
        if let Some(input) = self.subscription_input.as_ref() {
            input.update(cx, |input, cx| input.set_value_without_event(url, cx));
        }
        cx.notify();
    }

    fn open_single_node_editor(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(saved) = self.saved_single_nodes.iter().find(|saved| saved.id == id) else {
            return;
        };
        let name = saved.name.clone();
        let url = saved.source.expose_to(str::to_owned);
        self.subscription_editor_source_id = None;
        self.single_node_editor_source_id = Some(id);
        self.proxy_source_editor_kind = ProxySourceEditorKind::SingleNode;
        self.subscription_editor_refresh_interval = RemoteSourceRefreshInterval::Manual;
        self.subscription_editor_interval_popover = false;
        self.subscription_editor_enabled = saved.enabled;
        self.subscription_editor_error = None;
        self.subscription_feedback = SubscriptionFeedback::Idle;
        if let Some(input) = self.subscription_name_input.as_ref() {
            input.update(cx, |input, cx| input.set_value_without_event(name, cx));
        }
        if let Some(input) = self.subscription_input.as_ref() {
            input.update(cx, |input, cx| input.set_value_without_event(url, cx));
        }
        cx.notify();
    }

    fn open_proxy_source_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let app = cx.entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            app.update(cx, |this, cx| {
                let theme = this.theme();
                this.proxy_source_editor_modal(dialog, theme, this.language(), window, cx)
            })
        });
        if let Some(input) = self.subscription_name_input.as_ref() {
            input.focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    fn close_subscription_editor(&mut self, cx: &mut Context<Self>) {
        self.configuration_add_section = None;
        self.subscription_editor_source_id = None;
        self.single_node_editor_source_id = None;
        self.subscription_editor_interval_popover = false;
        self.subscription_editor_error = None;
        self.subscription_feedback = SubscriptionFeedback::Idle;
        if let Some(input) = self.subscription_name_input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        if let Some(input) = self.subscription_input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        cx.notify();
    }

    #[allow(clippy::too_many_lines)]
    fn proxy_source_editor_modal(
        &self,
        dialog: Dialog,
        theme: Theme,
        language: Language,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Dialog {
        let input = self
            .subscription_input
            .as_ref()
            .expect("subscription input is initialized before rendering")
            .clone();
        let name_input = self
            .subscription_name_input
            .as_ref()
            .expect("subscription name input is initialized before rendering")
            .clone();
        let direct_input = self.proxy_source_editor_kind == ProxySourceEditorKind::SingleNode;
        let editing = self.subscription_editor_source_id.is_some()
            || self.single_node_editor_source_id.is_some();
        let busy = matches!(
            self.subscription_feedback,
            SubscriptionFeedback::Importing(_)
        );
        let enabled = self.subscription_editor_enabled;
        let save_input = input.clone();
        let app = cx.entity();
        let viewport = window.viewport_size();
        let dialog_width = (viewport.width.as_f32() - 32.0).clamp(300.0, 620.0);
        let interval_open = self.subscription_editor_interval_popover;
        let mut interval_menu = div().p_1();
        for interval in [
            RemoteSourceRefreshInterval::Manual,
            RemoteSourceRefreshInterval::Hourly,
            RemoteSourceRefreshInterval::SixHours,
            RemoteSourceRefreshInterval::TwelveHours,
            RemoteSourceRefreshInterval::Daily,
        ] {
            let selected = interval == self.subscription_editor_refresh_interval;
            interval_menu = interval_menu.child(
                div()
                    .id(format!("subscription-refresh-option-{interval:?}"))
                    .role(Role::Button)
                    .aria_label(refresh_interval_label(interval, language))
                    .tab_stop(true)
                    .focusable()
                    .cursor_pointer()
                    .px_3()
                    .py_2()
                    .rounded(Radius::Control.px())
                    .bg(if selected {
                        theme.action_soft
                    } else {
                        theme.surface_high
                    })
                    .text_size(TextRole::Label.size())
                    .line_height(TextRole::Label.line_height())
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
                        this.subscription_editor_refresh_interval = interval;
                        this.subscription_editor_interval_popover = false;
                        cx.notify();
                    })),
            );
        }
        let interval_trigger = Button::new("subscription-editor-refresh-interval")
            .accessibility_label(
                language.text("Choose subscription update interval", "选择订阅更新间隔"),
            )
            .dropdown_caret(true)
            .with_variant(ButtonVariant::Default)
            .with_size(ControlSize::Standard.component_size())
            .h(ControlSize::Standard.height())
            .w_full()
            .child(
                div()
                    .w_full()
                    .text_size(TextRole::Label.size())
                    .line_height(TextRole::Label.line_height())
                    .font_weight(TextRole::Label.weight())
                    .child(refresh_interval_label(
                        self.subscription_editor_refresh_interval,
                        language,
                    )),
            )
            .disabled(busy);
        let interval_app = app.clone();
        let interval_select = crate::components::anchored_popover(
            "subscription-editor-refresh-popover",
            interval_trigger,
            interval_menu,
            (dialog_width - 40.0).max(240.0),
            280.0,
        )
        .open(interval_open)
        .on_open_change(move |open, _, cx| {
            interval_app.update(cx, |this, cx| {
                this.subscription_editor_interval_popover = *open;
                cx.notify();
            });
        });

        let body = div()
            .id("proxy-source-modal-body")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .px_5()
            .py_4()
            .when(!editing, |body| {
                body.child(field_label(language.text("Source type", "来源类型"), theme))
                    .child(
                        div()
                            .mt_1()
                            .flex()
                            .gap_2()
                            .child(
                                action_button(
                                    "proxy-source-kind-subscription",
                                    language.text("Subscription", "订阅来源"),
                                    if direct_input {
                                        ActionRole::Secondary
                                    } else {
                                        ActionRole::Primary
                                    },
                                    ControlSize::Compact,
                                )
                                .cursor_pointer()
                                .bg(if direct_input {
                                    theme.surface_high
                                } else {
                                    theme.action_primary
                                })
                                .text_color(if direct_input {
                                    theme.text_secondary
                                } else {
                                    theme.action_on_primary
                                })
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.proxy_source_editor_kind =
                                            ProxySourceEditorKind::Subscription;
                                        this.subscription_editor_error = None;
                                        cx.notify();
                                    },
                                )),
                            )
                            .child(
                                action_button(
                                    "proxy-source-kind-single-node",
                                    language.text("Single node", "单节点来源"),
                                    if direct_input {
                                        ActionRole::Primary
                                    } else {
                                        ActionRole::Secondary
                                    },
                                    ControlSize::Compact,
                                )
                                .cursor_pointer()
                                .bg(if direct_input {
                                    theme.action_primary
                                } else {
                                    theme.surface_high
                                })
                                .text_color(if direct_input {
                                    theme.action_on_primary
                                } else {
                                    theme.text_secondary
                                })
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.proxy_source_editor_kind =
                                            ProxySourceEditorKind::SingleNode;
                                        this.subscription_editor_error = None;
                                        cx.notify();
                                    },
                                )),
                            ),
                    )
            })
            .child(field_label(
                if direct_input {
                    language.text("Node name", "节点名称")
                } else {
                    language.text("Source name", "来源名称")
                },
                theme,
            ))
            .child(name_input)
            .child(field_label(language.text("Source URL", "来源 URL"), theme).mt_4())
            .child(input)
            .when(!direct_input, |body| {
                body.child(field_label(language.text("Update interval", "更新间隔"), theme).mt_4())
                    .child(interval_select)
            })
            .child(
                Checkbox::new("proxy-source-editor-enabled")
                    .label(language.text("Use this source", "使用此来源"))
                    .checked(enabled)
                    .disabled(busy)
                    .tab_stop(!busy)
                    .cursor_pointer()
                    .mt_4()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !busy {
                            this.subscription_editor_enabled = !enabled;
                            cx.notify();
                        }
                    })),
            )
            .when_some(self.subscription_editor_error.clone(), |body, error| {
                body.child(
                    div()
                        .mt_3()
                        .text_size(TextRole::Metadata.size())
                        .text_color(theme.status_error)
                        .child(error),
                )
            });

        let footer = div()
            .flex_shrink_0()
            .px_5()
            .py_4()
            .border_t_1()
            .border_color(theme.outline_subtle)
            .flex()
            .items_center()
            .justify_end()
            .gap_2()
            .child(
                style_action_button(
                    Button::new("cancel-proxy-source")
                        .accessibility_label(language.text("Cancel source editing", "取消编辑来源"))
                        .label(language.message(Message::Cancel)),
                    ActionRole::Secondary,
                    ControlSize::Standard,
                )
                .px(Space::Lg.px())
                .cursor_pointer()
                .border_color(theme.outline_subtle)
                .bg(theme.surface_high)
                .text_color(theme.text_primary)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.close_subscription_editor(cx);
                    window.close_dialog(cx);
                })),
            )
            .child(
                style_action_button(
                    Button::new("save-proxy-source")
                        .accessibility_label(language.text("Save proxy source", "保存代理来源"))
                        .label(if busy {
                            language.text("Processing…", "正在处理…")
                        } else if editing {
                            language.message(Message::SaveChanges)
                        } else {
                            language.text("Add source", "添加来源")
                        })
                        .loading(busy),
                    ActionRole::Primary,
                    ControlSize::Standard,
                )
                .px(Space::Lg.px())
                .when(!busy, gpui::Styled::cursor_pointer)
                .bg(if busy {
                    theme.action_soft
                } else {
                    theme.action_primary
                })
                .text_color(if busy {
                    theme.action_primary
                } else {
                    theme.action_on_primary
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    if !busy && this.submit_source_import(&save_input, cx) {
                        window.close_dialog(cx);
                    }
                })),
            );

        dialog
            .width(px(dialog_width))
            .max_h(px((viewport.height.as_f32() - 32.0).max(320.0)))
            .margin_top(px(((viewport.height.as_f32() - 480.0) / 2.0).max(16.0)))
            .overlay(true)
            .overlay_closable(true)
            .keyboard(true)
            .close_button(false)
            .p_0()
            .rounded_md()
            .bg(theme.surface_high)
            .overflow_hidden()
            .title(
                div()
                    .px_5()
                    .py_4()
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .child(
                        div()
                            .text_size(px(17.0))
                            .font_weight(TextRole::SectionTitle.weight())
                            .child(if editing {
                                language.text("Edit proxy source", "编辑代理来源")
                            } else {
                                language.text("Add proxy source", "添加代理来源")
                            }),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(TextRole::Metadata.size())
                            .text_color(theme.text_secondary)
                            .child(if direct_input {
                                language.text(
                                    "A single-node source does not need an update interval.",
                                    "单节点来源不需要更新间隔。",
                                )
                            } else {
                                language.text(
                                    "Choose a subscription or a single-node share link.",
                                    "请选择订阅来源或单节点分享链接。",
                                )
                            }),
                    ),
            )
            .child(body)
            .footer(footer)
            .on_close(move |_, _, cx| {
                app.update(cx, ManisApp::close_subscription_editor);
            })
    }

    fn submit_source_import(
        &mut self,
        input: &Entity<SubscriptionTextInput>,
        cx: &mut Context<Self>,
    ) -> bool {
        let name = self
            .subscription_name_input
            .as_ref()
            .map(|input| input.read(cx).value().trim().to_owned())
            .unwrap_or_default();
        if name.is_empty() {
            self.subscription_editor_error = Some(
                self.language()
                    .text("Enter a source name", "请输入来源名称")
                    .to_owned(),
            );
            cx.notify();
            return false;
        }
        self.subscription_editor_error = None;
        let (input_value, result) = {
            let input = input.read(cx);
            (
                input.value().to_owned(),
                match self.proxy_source_editor_kind {
                    ProxySourceEditorKind::Subscription => {
                        validate_subscription_preview(input.value())
                    }
                    ProxySourceEditorKind::SingleNode => {
                        validate_single_node_preview(input.value())
                    }
                },
            )
        };
        match result {
            Ok(preview) if preview.kind == SourceKind::SingleNode => {
                if self.subscription_editor_source_id.is_some() {
                    self.subscription_editor_error = Some(
                        self.language()
                            .text(
                                "An existing subscription must keep an HTTP/HTTPS URL",
                                "现有订阅必须使用 HTTP/HTTPS URL",
                            )
                            .to_owned(),
                    );
                    cx.notify();
                    return false;
                }
                self.import_single_node(input_value, name, preview, cx)
            }
            Ok(preview) => {
                if self.single_node_editor_source_id.is_some() {
                    self.subscription_editor_error = Some(
                        self.language()
                            .text(
                                "This source must remain a single-node share link",
                                "此来源必须保持为单节点分享链接",
                            )
                            .to_owned(),
                    );
                    cx.notify();
                    return false;
                }
                trace_ui(UiEvent::SourceRecognitionSucceeded);
                self.import_remote_subscription(
                    super::SubscriptionImportRequest {
                        input: input_value,
                        name,
                        refresh_interval: self.subscription_editor_refresh_interval,
                        enabled: self.subscription_editor_enabled,
                        editing_id: self.subscription_editor_source_id.clone(),
                        kind: preview.kind,
                    },
                    cx,
                );
                true
            }
            Err(error) => {
                self.subscription_feedback = SubscriptionFeedback::InvalidInput(error);
                self.status = format!(
                    "{}: {error}",
                    self.language()
                        .text("Source recognition failed", "来源识别失败")
                );
                trace_ui(UiEvent::SourceRecognitionFailed);
                cx.notify();
                false
            }
        }
    }

    fn import_single_node(
        &mut self,
        input_value: String,
        name: String,
        preview: crate::subscription::SubscriptionPreview,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            self.subscription_feedback =
                SubscriptionFeedback::StoreFailed(SubscriptionStoreError::DataDirectoryUnavailable);
            self.language()
                .text(
                    "Could not determine where to save the node",
                    "无法确定节点保存位置",
                )
                .clone_into(&mut self.status);
            trace_ui(UiEvent::SourceImportFailed);
            cx.notify();
            return false;
        };
        self.subscription_preview_generation = self.subscription_preview_generation.wrapping_add(1);
        let generation = self.subscription_preview_generation;
        self.subscription_feedback = SubscriptionFeedback::Importing(SourceKind::SingleNode);
        self.language()
            .text(
                "Validating and saving single-node source",
                "正在验证并保存单节点来源",
            )
            .clone_into(&mut self.status);
        if let Some(input) = self.subscription_input.as_ref() {
            input.update(cx, |input, cx| input.set_enabled(false, cx));
        }
        let runtime = self.runtime.clone();
        let editing_id = self.single_node_editor_source_id.clone();
        let enabled = self.subscription_editor_enabled;
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    let providers = mihomo::preview_single_node(&input_value)?;
                    let transaction = super::mutate_saved_sources(&runtime, &store_dir, || {
                        if let Some(id) = editing_id {
                            mihomo::update_single_node_source_in(
                                &store_dir,
                                &id,
                                &input_value,
                                &name,
                                enabled,
                            )
                        } else {
                            mihomo::save_single_node_source_with_options_in(
                                &store_dir,
                                &input_value,
                                &name,
                                enabled,
                            )
                        }
                    })?;
                    Ok::<_, SubscriptionStoreError>((transaction, providers))
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_single_node_import(generation, preview, result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
        true
    }

    fn finish_single_node_import(
        &mut self,
        generation: u64,
        preview: crate::subscription::SubscriptionPreview,
        result: super::SingleNodeImportResult,
        cx: &mut Context<Self>,
    ) {
        if self.subscription_preview_generation != generation {
            return;
        }
        if let Some(input) = self.subscription_input.as_ref() {
            input.update(cx, |input, cx| input.set_enabled(true, cx));
        }
        match result {
            Ok((transaction, providers)) => {
                self.finish_saved_single_node(transaction, providers, preview, cx);
            }
            Err(error) => {
                self.subscription_feedback = SubscriptionFeedback::StoreFailed(error);
                self.status = format!(
                    "{}: {error}",
                    self.language()
                        .text("Single-node source save failed", "单节点来源保存失败")
                );
                trace_ui(UiEvent::SourceImportFailed);
                cx.notify();
            }
        }
    }

    fn finish_saved_single_node(
        &mut self,
        mut transaction: super::SourceMutation<mihomo::StoredSingleNode>,
        providers: Vec<mihomo::LoadedProvider>,
        preview: crate::subscription::SubscriptionPreview,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        transaction.apply.reconcile_proxy_mode(&mut self.proxy_mode);
        let Some(stored) = transaction.value.take() else {
            self.subscription_feedback =
                SubscriptionFeedback::StoreFailed(SubscriptionStoreError::StoreUnavailable);
            self.status = format!(
                "{}{}",
                language.text("Single-node source save failed", "单节点来源保存失败"),
                transaction
                    .apply
                    .status_suffix_after_source_rollback(language)
            );
            trace_ui(UiEvent::SourceImportFailed);
            cx.notify();
            return;
        };
        if let Some(existing) = self
            .saved_single_nodes
            .iter_mut()
            .find(|node| node.id == stored.id)
        {
            *existing = stored;
        } else {
            self.saved_single_nodes.push(stored);
        }
        self.subscription_preview_providers = providers;
        self.subscription_feedback = SubscriptionFeedback::Valid(preview);
        if let Some(input) = self.subscription_input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        if let Some(input) = self.subscription_name_input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        self.configuration_add_section = None;
        self.status = if language == Language::English {
            format!(
                "Single-node source saved · Added to Saved group{}",
                transaction.apply.status_suffix(language)
            )
        } else {
            format!(
                "单节点来源已保存 · 已加入“已保存”分组{}",
                transaction.apply.status_suffix(language)
            )
        };
        trace_ui(UiEvent::SourceImportSucceeded);
        cx.notify();
    }

    fn imported_subscription_cards(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let language = self.language();
        let now = mihomo::current_unix_secs();
        div().children(self.imported_subscriptions.iter().map(|subscription| {
            let presentation = self.subscription_card_presentation(subscription, now, language);
            Self::imported_subscription_card(subscription, &presentation, language, theme, cx)
        }))
    }

    fn subscription_card_presentation(
        &self,
        subscription: &super::ImportedSubscription,
        now: u64,
        language: Language,
    ) -> SubscriptionCardPresentation {
        let node_count = subscription
            .providers
            .iter()
            .map(|provider| provider.nodes.len())
            .sum::<usize>();
        let (state, activity) = match &subscription.state {
            ImportedSubscriptionState::None => (
                language.text("Disabled", "未启用").to_owned(),
                SubscriptionCardActivity::Idle { healthy: true },
            ),
            ImportedSubscriptionState::Pending(_) | ImportedSubscriptionState::Refreshing(_) => (
                language.text("Updating…", "正在更新…").to_owned(),
                SubscriptionCardActivity::Busy,
            ),
            ImportedSubscriptionState::Ready(kind) => (
                if language == Language::English {
                    format!(
                        "{} · {node_count} nodes",
                        source_kind_label(*kind, language)
                    )
                } else {
                    format!(
                        "{} · {node_count} 个节点",
                        source_kind_label(*kind, language)
                    )
                },
                SubscriptionCardActivity::Idle { healthy: true },
            ),
            ImportedSubscriptionState::Unavailable(_, _)
            | ImportedSubscriptionState::StoreError(_) => (
                language.text("Update failed", "更新失败").to_owned(),
                SubscriptionCardActivity::Idle { healthy: false },
            ),
            ImportedSubscriptionState::Removing(_) => (
                language.text("Removing…", "正在移除…").to_owned(),
                SubscriptionCardActivity::Busy,
            ),
        };
        let controls_enabled = !activity.is_busy()
            && !matches!(
                self.subscription_feedback,
                SubscriptionFeedback::Importing(_)
            )
            && !self.source_refresh_busy();
        SubscriptionCardPresentation {
            state,
            activity,
            controls_enabled,
            updated: source_update_label(
                subscription.last_successful_update_unix_secs,
                now,
                language,
            ),
        }
    }

    fn imported_subscription_card(
        subscription: &super::ImportedSubscription,
        presentation: &SubscriptionCardPresentation,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let card_edit_id = subscription.id.clone();
        let controls_enabled = presentation.controls_enabled;
        div()
            .id(format!("subscription-card-{card_edit_id}"))
            .role(Role::Button)
            .aria_label(language.text("Edit this subscription", "编辑这个订阅"))
            .tab_stop(controls_enabled)
            .focusable()
            .when(controls_enabled, gpui::Styled::cursor_pointer)
            .mt_2()
            .px_3()
            .py_2()
            .rounded(Radius::Row.px())
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_low)
            .child(Self::subscription_card_header(
                subscription,
                presentation,
                language,
                theme,
                cx,
            ))
            .child(
                div()
                    .mt_1()
                    .ml_7()
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Metadata.size())
                    .text_color(theme.text_tertiary)
                    .child(subscription.source.expose_to(str::to_owned)),
            )
            .child(Self::subscription_card_actions(
                subscription,
                presentation,
                language,
                theme,
                cx,
            ))
            .on_click(cx.listener(move |this, _, window, cx| {
                if controls_enabled {
                    this.open_subscription_editor(card_edit_id.clone(), cx);
                    this.open_proxy_source_dialog(window, cx);
                }
            }))
    }

    fn subscription_card_header(
        subscription: &super::ImportedSubscription,
        presentation: &SubscriptionCardPresentation,
        _language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let toggle_id = subscription.id.clone();
        let enabled = subscription.enabled;
        let controls_enabled = presentation.controls_enabled;
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                Checkbox::new(format!("subscription-enabled-{toggle_id}"))
                    .label("")
                    .checked(enabled)
                    .disabled(!controls_enabled)
                    .tab_stop(controls_enabled)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if controls_enabled {
                            this.set_subscription_enabled(toggle_id.clone(), !enabled, cx);
                        }
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .min_w(px(0.0))
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(TextRole::Label.size())
                            .font_weight(TextRole::Label.weight())
                            .text_color(if !enabled {
                                theme.text_secondary
                            } else if presentation.activity.is_healthy() {
                                theme.text_primary
                            } else {
                                theme.status_error
                            })
                            .child(subscription.name.clone()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(TextRole::Metadata.size())
                            .text_color(theme.text_tertiary)
                            .child(presentation.state.clone()),
                    ),
            )
            .when(presentation.activity.is_busy(), |row| {
                row.child(Self::benchmark_latency_spinner(
                    format!("source-refresh-{}", subscription.id),
                    theme,
                ))
            })
    }

    fn subscription_card_actions(
        subscription: &super::ImportedSubscription,
        presentation: &SubscriptionCardPresentation,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let refresh_id = subscription.id.clone();
        let remove_id = subscription.id.clone();
        let refresh_enabled = presentation.controls_enabled && subscription.enabled;
        let controls_enabled = presentation.controls_enabled;
        let busy = presentation.activity.is_busy();
        div()
            .mt_1()
            .ml_7()
            .flex()
            .items_center()
            .gap_2()
            .text_size(TextRole::Metadata.size())
            .text_color(theme.text_secondary)
            .child(refresh_interval_label(
                subscription.refresh_interval,
                language,
            ))
            .child("·")
            .child(presentation.updated.clone())
            .child(div().flex_1())
            .child(
                action_button(
                    format!("subscription-refresh-{refresh_id}"),
                    if busy {
                        language.text("Updating…", "更新中…")
                    } else {
                        language.text("Update now", "立即更新")
                    },
                    ActionRole::Quiet,
                    ControlSize::Compact,
                )
                .accessibility_label(
                    language.text("Update this subscription now", "立即更新这个订阅"),
                )
                .disabled(!refresh_enabled)
                .loading(busy)
                .when(refresh_enabled, gpui::Styled::cursor_pointer)
                .px_3()
                .border_1()
                .border_color(theme.outline_subtle)
                .bg(theme.surface_high)
                .text_color(theme.action_primary)
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    if refresh_enabled {
                        this.refresh_imported_subscription(refresh_id.clone(), cx);
                    }
                })),
            )
            .when(controls_enabled, |row| {
                row.child(
                    action_button(
                        format!("remove-{remove_id}"),
                        language.text("Remove", "移除"),
                        ActionRole::Quiet,
                        ControlSize::Compact,
                    )
                    .accessibility_label(language.text("Remove this subscription", "移除这个订阅"))
                    .cursor_pointer()
                    .px_3()
                    .text_color(theme.status_error)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.remove_imported_subscription(remove_id.clone(), cx);
                    })),
                )
            })
    }

    fn saved_single_node_cards(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let language = self.language();
        let controls_enabled = !matches!(
            self.subscription_feedback,
            SubscriptionFeedback::Importing(_)
        ) && !self.source_refresh_busy();
        div().children(self.saved_single_nodes.iter().map(|saved| {
            Self::saved_single_node_card(saved, controls_enabled, language, theme, cx)
        }))
    }

    fn saved_single_node_card(
        saved: &mihomo::StoredSingleNode,
        controls_enabled: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let card_edit_id = saved.id.clone();
        let node = saved.source.preview();
        div()
            .id(format!("single-node-card-{card_edit_id}"))
            .role(Role::Button)
            .aria_label(language.text("Edit this single-node source", "编辑这个单节点来源"))
            .tab_stop(controls_enabled)
            .focusable()
            .when(controls_enabled, gpui::Styled::cursor_pointer)
            .mt_2()
            .px_3()
            .py_2()
            .rounded(Radius::Row.px())
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_low)
            .child(Self::saved_single_node_header(
                saved,
                node,
                controls_enabled,
                theme,
                cx,
            ))
            .child(
                div()
                    .mt_1()
                    .ml_7()
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Metadata.size())
                    .text_color(theme.text_tertiary)
                    .child(saved.source.expose_to(str::to_owned)),
            )
            .child(Self::saved_single_node_actions(
                saved,
                node,
                controls_enabled,
                language,
                theme,
                cx,
            ))
            .on_click(cx.listener(move |this, _, window, cx| {
                if controls_enabled {
                    this.open_single_node_editor(card_edit_id.clone(), cx);
                    this.open_proxy_source_dialog(window, cx);
                }
            }))
    }

    fn saved_single_node_header(
        saved: &mihomo::StoredSingleNode,
        node: &crate::subscription::SourceNodePreview,
        controls_enabled: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let toggle_id = saved.id.clone();
        let enabled = saved.enabled;
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                Checkbox::new(format!("single-node-enabled-{toggle_id}"))
                    .label("")
                    .checked(enabled)
                    .disabled(!controls_enabled)
                    .tab_stop(controls_enabled)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if controls_enabled {
                            this.set_single_node_enabled(toggle_id.clone(), !enabled, cx);
                        }
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .min_w(px(0.0))
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(TextRole::Label.size())
                            .font_weight(TextRole::Label.weight())
                            .text_color(if enabled {
                                theme.text_primary
                            } else {
                                theme.text_secondary
                            })
                            .child(saved.name.clone()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(TextRole::Metadata.size())
                            .text_color(theme.text_tertiary)
                            .child(node.protocol),
                    ),
            )
    }

    fn saved_single_node_actions(
        saved: &mihomo::StoredSingleNode,
        node: &crate::subscription::SourceNodePreview,
        controls_enabled: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let remove_id = saved.id.clone();
        div()
            .mt_1()
            .ml_7()
            .flex()
            .items_center()
            .gap_2()
            .text_size(TextRole::Metadata.size())
            .text_color(theme.text_secondary)
            .child(format!("{} · {}", node.endpoint, node.detail))
            .child(div().flex_1())
            .when(controls_enabled, |row| {
                row.child(
                    action_button(
                        format!("remove-single-node-{remove_id}"),
                        language.text("Remove", "移除"),
                        ActionRole::Quiet,
                        ControlSize::Compact,
                    )
                    .accessibility_label(
                        language.text("Remove single-node source", "移除单节点来源"),
                    )
                    .cursor_pointer()
                    .px_3()
                    .text_color(theme.status_error)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.remove_saved_single_node(remove_id.clone(), cx);
                    })),
                )
            })
    }

    fn set_single_node_enabled(&mut self, id: String, enabled: bool, cx: &mut Context<Self>) {
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            return;
        };
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    super::mutate_saved_sources(&runtime, &store_dir, || {
                        mihomo::update_single_node_source_enabled_in(&store_dir, &id, enabled)
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(transaction) => {
                        let language = this.language();
                        transaction.apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        if let Some(stored) = transaction.value {
                            if let Some(existing) = this
                                .saved_single_nodes
                                .iter_mut()
                                .find(|existing| existing.id == stored.id)
                            {
                                *existing = stored;
                            }
                            this.status = format!(
                                "{}{}",
                                language.text("Single-node source updated", "单节点来源已更新"),
                                transaction.apply.status_suffix(language)
                            );
                        } else {
                            this.status = format!(
                                "{}{}",
                                language.text("Could not update source", "无法更新来源"),
                                transaction.apply.status_suffix_after_rollback_attempt(
                                    language,
                                    transaction.rollback_error.as_ref(),
                                )
                            );
                        }
                    }
                    Err(error) => {
                        this.status = format!(
                            "{}: {error}",
                            this.language()
                                .text("Could not update source", "无法更新来源")
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn remove_saved_single_node(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            return;
        };
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    super::mutate_saved_sources(&runtime, &store_dir, || {
                        mihomo::remove_single_node_source_in(&store_dir, &id).map(|()| id.clone())
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(transaction) => {
                        let language = this.language();
                        transaction.apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        if let Some(deleted_id) = transaction.value {
                            this.saved_single_nodes.retain(|node| node.id != deleted_id);
                            this.status = format!(
                                "{}{}",
                                language.text("Single-node source removed", "单节点来源已移除"),
                                transaction.apply.status_suffix(language)
                            );
                        } else {
                            this.status = format!(
                                "{}{}",
                                language.text("Failed to remove source", "移除来源失败"),
                                transaction.apply.status_suffix_after_rollback_attempt(
                                    language,
                                    transaction.rollback_error.as_ref(),
                                )
                            );
                        }
                    }
                    Err(error) => {
                        this.status = format!(
                            "{}: {error}",
                            this.language()
                                .text("Failed to remove source", "移除来源失败")
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
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

    #[allow(clippy::too_many_lines)]
    fn rule_source_manager(
        &self,
        _input: Entity<SubscriptionTextInput>,
        busy: bool,
        theme: Theme,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let language = self.language();
        let add_action = action_button(
            "configuration-add-rule-source",
            language.text("Add source", "添加来源"),
            ActionRole::Primary,
            ControlSize::Compact,
        )
        .bg(theme.action_primary)
        .text_color(theme.action_on_primary)
        .on_click(cx.listener(move |this, _, window, cx| {
            this.open_new_qx_rule_editor(cx);
            this.open_qx_rule_source_dialog(window, cx);
        }));
        let mut panel = panel_surface("configuration-rule-sources", compact, theme)
            .child(section_heading(
                language.text("Rule sources", "规则来源"),
                language.count(CountNoun::Source, self.qx_rule_sources.len()),
                Some(add_action.into_any_element()),
                theme,
            ))
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
                            .child(language.text("Saved", "已保存")),
                    ),
            );

        if self.qx_rule_sources.is_empty() {
            panel = panel.child(
                empty_state(
                    language.text("No rule sources", "暂无规则源"),
                    language.text("Add a remote QX rule set.", "添加一个远程 QX 规则集。"),
                    Some(
                        action_button(
                            "configuration-empty-add-rule-source",
                            language.text("Add source", "添加来源"),
                            ActionRole::Primary,
                            ControlSize::Compact,
                        )
                        .bg(theme.action_primary)
                        .text_color(theme.action_on_primary)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_new_qx_rule_editor(cx);
                            this.open_qx_rule_source_dialog(window, cx);
                        }))
                        .into_any_element(),
                    ),
                    theme,
                )
                .mt(Space::Md.px()),
            );
        }
        for (index, source) in self.qx_rule_sources.iter().enumerate() {
            panel = panel.child(self.rule_source_card(index, source, busy, theme, cx));
        }
        panel
    }

    fn open_new_qx_rule_editor(&mut self, cx: &mut Context<Self>) {
        self.qx_rule_editor_source_id = None;
        self.qx_rule_editor_refresh_interval = RemoteSourceRefreshInterval::Manual;
        self.qx_rule_editor_popover = super::QxRuleEditorPopover::None;
        self.qx_rule_feedback = QxRuleImportFeedback::Idle;
        if !self.qx_rule_targets().contains(&self.qx_rule_target_policy)
            && let Some(target) = self.qx_rule_targets().into_iter().next()
        {
            self.qx_rule_target_policy = target;
        }
        if let Some(input) = self.qx_rule_input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        cx.notify();
    }

    fn open_qx_rule_editor(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(source) = self.qx_rule_sources.iter().find(|source| source.id == id) else {
            return;
        };
        let url = source.source.expose_to(str::to_owned);
        let target = self.effective_rule_target(source.target_policy.as_str(), self.language());
        self.qx_rule_editor_source_id = Some(id);
        self.qx_rule_editor_refresh_interval = source.refresh_interval;
        self.qx_rule_editor_popover = super::QxRuleEditorPopover::None;
        self.qx_rule_target_policy = target;
        self.qx_rule_feedback = QxRuleImportFeedback::Idle;
        if let Some(input) = self.qx_rule_input.as_ref() {
            input.update(cx, |input, cx| input.set_value_without_event(url, cx));
        }
        cx.notify();
    }

    fn open_qx_rule_source_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let app = cx.entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            app.update(cx, |this, cx| {
                this.qx_rule_source_editor_modal(dialog, this.theme(), this.language(), window, cx)
            })
        });
        if let Some(input) = self.qx_rule_input.as_ref() {
            input.focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    fn close_qx_rule_editor(&mut self, cx: &mut Context<Self>) {
        self.qx_rule_editor_source_id = None;
        self.qx_rule_editor_popover = super::QxRuleEditorPopover::None;
        self.qx_rule_feedback = QxRuleImportFeedback::Idle;
        if let Some(input) = self.qx_rule_input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        cx.notify();
    }

    #[allow(clippy::too_many_lines)]
    fn qx_rule_source_editor_modal(
        &self,
        dialog: Dialog,
        theme: Theme,
        language: Language,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Dialog {
        let input = self
            .qx_rule_input
            .as_ref()
            .expect("QX rule input is initialized before rendering")
            .clone();
        let save_input = input.clone();
        let editing = self.qx_rule_editor_source_id.is_some();
        let busy = self.qx_rule_feedback == QxRuleImportFeedback::Importing;
        let app = cx.entity();
        let viewport = window.viewport_size();
        let dialog_width = (viewport.width.as_f32() - 32.0).clamp(300.0, 620.0);

        let mut target_menu = div().p_1();
        for target in self.qx_rule_targets() {
            let selected = target == self.qx_rule_target_policy;
            target_menu = target_menu.child(
                div()
                    .id(format!("qx-rule-editor-target-{target}"))
                    .role(Role::Button)
                    .aria_label(target.clone())
                    .tab_stop(true)
                    .focusable()
                    .cursor_pointer()
                    .px_3()
                    .py_2()
                    .rounded(Radius::Control.px())
                    .bg(if selected {
                        theme.action_soft
                    } else {
                        theme.surface_high
                    })
                    .text_size(TextRole::Label.size())
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
                    .child(target.clone())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.qx_rule_target_policy.clone_from(&target);
                        this.qx_rule_editor_popover = super::QxRuleEditorPopover::None;
                        this.qx_rule_feedback = QxRuleImportFeedback::Idle;
                        cx.notify();
                    })),
            );
        }
        let target_trigger = Button::new("qx-rule-editor-target")
            .accessibility_label(language.text("Choose target policy", "选择目标策略"))
            .dropdown_caret(true)
            .with_variant(ButtonVariant::Default)
            .with_size(ControlSize::Standard.component_size())
            .h(ControlSize::Standard.height())
            .w_full()
            .child(
                div()
                    .w_full()
                    .text_size(TextRole::Label.size())
                    .font_weight(TextRole::Label.weight())
                    .child(self.qx_rule_target_policy.clone()),
            )
            .disabled(busy);
        let target_app = app.clone();
        let target_select = crate::components::anchored_popover(
            "qx-rule-editor-target-popover",
            target_trigger,
            target_menu,
            (dialog_width - 40.0).max(240.0),
            320.0,
        )
        .open(self.qx_rule_editor_popover == super::QxRuleEditorPopover::Target)
        .on_open_change(move |open, _, cx| {
            target_app.update(cx, |this, cx| {
                this.qx_rule_editor_popover = if *open {
                    super::QxRuleEditorPopover::Target
                } else {
                    super::QxRuleEditorPopover::None
                };
                cx.notify();
            });
        });

        let mut interval_menu = div().p_1();
        for interval in [
            RemoteSourceRefreshInterval::Manual,
            RemoteSourceRefreshInterval::Hourly,
            RemoteSourceRefreshInterval::SixHours,
            RemoteSourceRefreshInterval::TwelveHours,
            RemoteSourceRefreshInterval::Daily,
        ] {
            let selected = interval == self.qx_rule_editor_refresh_interval;
            interval_menu = interval_menu.child(
                div()
                    .id(format!("qx-rule-editor-interval-{interval:?}"))
                    .role(Role::Button)
                    .aria_label(refresh_interval_label(interval, language))
                    .tab_stop(true)
                    .focusable()
                    .cursor_pointer()
                    .px_3()
                    .py_2()
                    .rounded(Radius::Control.px())
                    .bg(if selected {
                        theme.action_soft
                    } else {
                        theme.surface_high
                    })
                    .text_size(TextRole::Label.size())
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
                        this.qx_rule_editor_refresh_interval = interval;
                        this.qx_rule_editor_popover = super::QxRuleEditorPopover::None;
                        cx.notify();
                    })),
            );
        }
        let interval_trigger = Button::new("qx-rule-editor-refresh-interval")
            .accessibility_label(language.text("Choose rule update interval", "选择规则更新间隔"))
            .dropdown_caret(true)
            .with_variant(ButtonVariant::Default)
            .with_size(ControlSize::Standard.component_size())
            .h(ControlSize::Standard.height())
            .w_full()
            .child(
                div()
                    .w_full()
                    .text_size(TextRole::Label.size())
                    .font_weight(TextRole::Label.weight())
                    .child(refresh_interval_label(
                        self.qx_rule_editor_refresh_interval,
                        language,
                    )),
            )
            .disabled(busy);
        let interval_app = app.clone();
        let interval_select = crate::components::anchored_popover(
            "qx-rule-editor-refresh-popover",
            interval_trigger,
            interval_menu,
            (dialog_width - 40.0).max(240.0),
            280.0,
        )
        .open(self.qx_rule_editor_popover == super::QxRuleEditorPopover::Interval)
        .on_open_change(move |open, _, cx| {
            interval_app.update(cx, |this, cx| {
                this.qx_rule_editor_popover = if *open {
                    super::QxRuleEditorPopover::Interval
                } else {
                    super::QxRuleEditorPopover::None
                };
                cx.notify();
            });
        });

        let body = div()
            .id("qx-rule-source-modal-body")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .px_5()
            .py_4()
            .child(field_label(language.text("Rule URL", "规则 URL"), theme))
            .child(input)
            .child(field_label(language.text("Target policy", "目标策略"), theme).mt_4())
            .child(target_select)
            .child(field_label(language.text("Update interval", "更新间隔"), theme).mt_4())
            .child(interval_select)
            .child(self.qx_rule_import_feedback(theme, language));

        let footer = div()
            .flex_shrink_0()
            .px_5()
            .py_4()
            .border_t_1()
            .border_color(theme.outline_subtle)
            .flex()
            .items_center()
            .justify_end()
            .gap_2()
            .child(
                style_action_button(
                    Button::new("cancel-qx-rule-source").label(language.message(Message::Cancel)),
                    ActionRole::Secondary,
                    ControlSize::Standard,
                )
                .px(Space::Lg.px())
                .cursor_pointer()
                .border_color(theme.outline_subtle)
                .bg(theme.surface_high)
                .text_color(theme.text_primary)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.close_qx_rule_editor(cx);
                    window.close_dialog(cx);
                })),
            )
            .child(
                style_action_button(
                    Button::new("save-qx-rule-source")
                        .label(if busy {
                            language.text("Processing…", "正在处理…")
                        } else if editing {
                            language.message(Message::SaveChanges)
                        } else {
                            language.text("Add source", "添加来源")
                        })
                        .loading(busy),
                    ActionRole::Primary,
                    ControlSize::Standard,
                )
                .px(Space::Lg.px())
                .when(!busy, gpui::Styled::cursor_pointer)
                .bg(if busy {
                    theme.action_soft
                } else {
                    theme.action_primary
                })
                .text_color(if busy {
                    theme.action_primary
                } else {
                    theme.action_on_primary
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    if !busy && this.submit_qx_rule_import(&save_input, cx) {
                        window.close_dialog(cx);
                    }
                })),
            );

        dialog
            .width(px(dialog_width))
            .max_h(px((viewport.height.as_f32() - 32.0).max(320.0)))
            .margin_top(px(((viewport.height.as_f32() - 440.0) / 2.0).max(16.0)))
            .overlay(true)
            .overlay_closable(true)
            .keyboard(true)
            .close_button(false)
            .p_0()
            .rounded_md()
            .bg(theme.surface_high)
            .overflow_hidden()
            .title(
                div()
                    .px_5()
                    .py_4()
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .child(
                        div()
                            .text_size(px(17.0))
                            .font_weight(TextRole::SectionTitle.weight())
                            .child(if editing {
                                language.text("Edit rule source", "编辑规则来源")
                            } else {
                                language.text("Add rule source", "添加规则来源")
                            }),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(TextRole::Metadata.size())
                            .text_color(theme.text_secondary)
                            .child(language.text(
                                "The target policy is used by every rule in this source.",
                                "此来源中的全部规则都会使用所选目标策略。",
                            )),
                    ),
            )
            .child(body)
            .footer(footer)
            .on_close(move |_, _, cx| {
                app.update(cx, ManisApp::close_qx_rule_editor);
            })
    }

    #[allow(clippy::too_many_lines)]
    fn rule_source_card(
        &self,
        index: usize,
        source: &crate::mihomo::StoredQxRuleSource,
        busy: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let language = self.language();
        let id = source.id.clone();
        let toggle_id = id.clone();
        let refresh_id = id.clone();
        let remove_id = id.clone();
        let edit_id = id.clone();
        let refresh_state = self.qx_rule_source_refreshes.get(&source.id);
        let refreshing = refresh_state.is_some_and(QxRuleSourceRefreshState::is_refreshing);
        let duplicate = matches!(
            &self.qx_rule_feedback,
            QxRuleImportFeedback::AlreadyExists { source_id, .. } if source_id == &source.id
        );
        let controls_enabled = !busy && !self.source_refresh_busy();
        let enabled = source.enabled;
        let refresh_enabled = controls_enabled && enabled;
        let name = source.source.subscription_name().unwrap_or_else(|| {
            if language == Language::English {
                format!("Rule source {}", index + 1)
            } else {
                format!("规则源 {}", index + 1)
            }
        });
        let last_update = source_update_label(
            source.last_successful_update_unix_secs,
            mihomo::current_unix_secs(),
            language,
        );
        let refresh_error = match refresh_state {
            Some(QxRuleSourceRefreshState::Failed { message, .. }) => Some(message.clone()),
            _ => None,
        };
        let target_policy = self.effective_rule_target(source.target_policy.as_str(), language);
        div()
            .id(format!("qx-rule-source-card-{id}"))
            .role(Role::Button)
            .aria_label(language.text("Edit this rule source", "编辑这个规则来源"))
            .tab_stop(controls_enabled)
            .focusable()
            .when(controls_enabled, gpui::Styled::cursor_pointer)
            .mt(Space::Sm.px())
            .p(Space::Md.px())
            .rounded(Radius::Row.px())
            .border_1()
            .border_color(if duplicate {
                theme.status_warning
            } else {
                theme.outline_subtle
            })
            .bg(theme.surface_low)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Checkbox::new(format!("qx-rule-enabled-{toggle_id}"))
                            .label("")
                            .checked(enabled)
                            .disabled(!controls_enabled)
                            .tab_stop(controls_enabled)
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                if controls_enabled {
                                    this.set_qx_rule_source_enabled(
                                        toggle_id.clone(),
                                        !enabled,
                                        cx,
                                    );
                                }
                            })),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(TextRole::Label.weight())
                            .text_color(if enabled {
                                theme.text_primary
                            } else {
                                theme.text_secondary
                            })
                            .child(name),
                    )
                    .when(refreshing, |header| {
                        header.child(Self::benchmark_latency_spinner(
                            format!("qx-rule-refresh-{id}"),
                            theme,
                        ))
                    })
                    .when(duplicate, |header| {
                        header.child(
                            div()
                                .flex_shrink_0()
                                .text_size(TextRole::Metadata.size())
                                .line_height(TextRole::Metadata.line_height())
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.status_warning)
                                .child(language.text("Already added", "已添加")),
                        )
                    })
                    .when(!enabled, |header| {
                        header.child(
                            div()
                                .flex_shrink_0()
                                .text_size(TextRole::Metadata.size())
                                .text_color(theme.text_tertiary)
                                .child(language.text("Disabled", "未启用")),
                        )
                    }),
            )
            .child(
                div()
                    .mt_1()
                    .ml_7()
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Metadata.size())
                    .text_color(theme.text_tertiary)
                    .child(source.source.expose_to(str::to_owned)),
            )
            .when_some(refresh_error, |card, error| {
                card.child(
                    div()
                        .mt_1()
                        .ml_7()
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .text_color(theme.route_trace)
                        .child(format!(
                            "{}: {error}",
                            language.text("Last update failed", "上次更新失败")
                        )),
                )
            })
            .child(
                div()
                    .mt_1()
                    .ml_7()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(TextRole::Metadata.size())
                    .text_color(theme.text_secondary)
                    .child(if language == Language::English {
                        format!(
                            "{} rules · {} skipped",
                            source.rule_count, source.diagnostic_count
                        )
                    } else {
                        format!(
                            "{} 条 · 跳过 {} 条",
                            source.rule_count, source.diagnostic_count
                        )
                    })
                    .child("·")
                    .child(format!(
                        "{} {target_policy}",
                        language.text("Target", "目标")
                    ))
                    .child("·")
                    .child(refresh_interval_label(source.refresh_interval, language))
                    .child("·")
                    .child(last_update)
                    .child(div().flex_1())
                    .child(
                        action_button(
                            format!("qx-rule-refresh-{refresh_id}"),
                            if refreshing {
                                language.text("Updating…", "更新中…")
                            } else {
                                language.text("Update now", "立即更新")
                            },
                            ActionRole::Quiet,
                            ControlSize::Compact,
                        )
                        .accessibility_label(
                            language
                                .text("Update this remote QX rule now", "立即更新这份远程 QX 规则"),
                        )
                        .disabled(!refresh_enabled)
                        .loading(refreshing)
                        .when(refresh_enabled, gpui::Styled::cursor_pointer)
                        .px_3()
                        .border_1()
                        .border_color(theme.outline_subtle)
                        .bg(theme.surface_high)
                        .text_color(theme.action_primary)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if refresh_enabled {
                                this.refresh_qx_rule_source(refresh_id.clone(), cx);
                            }
                        })),
                    )
                    .child(
                        action_button(
                            format!("qx-rule-remove-{index}"),
                            language.text("Remove", "移除"),
                            ActionRole::Quiet,
                            ControlSize::Compact,
                        )
                        .accessibility_label(
                            language.text("Delete this remote QX rule", "删除这份远程 QX 规则"),
                        )
                        .disabled(!controls_enabled)
                        .when(controls_enabled, gpui::Styled::cursor_pointer)
                        .px_3()
                        .text_color(theme.status_error)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if controls_enabled {
                                this.remove_qx_rule_source(remove_id.clone(), cx);
                            }
                        })),
                    ),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                if controls_enabled {
                    this.open_qx_rule_editor(edit_id.clone(), cx);
                    this.open_qx_rule_source_dialog(window, cx);
                }
            }))
    }

    fn qx_rule_source_target_menu(
        &self,
        source_id: &str,
        selected_target: &str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut choices = div();
        for target in self.qx_rule_targets() {
            let selected = target == selected_target;
            let target_id = target.clone();
            let source_id = source_id.to_owned();
            choices = choices.child(
                Button::new(format!("qx-rule-source-target-{source_id}-{target_id}"))
                    .accessibility_label(format!("Target {target}"))
                    .selected(selected)
                    .with_variant(ButtonVariant::Text)
                    .with_size(ControlSize::Compact.component_size())
                    .w_full()
                    .min_h(ControlSize::Standard.min_pointer_target())
                    .px(Space::Md.px())
                    .py(Space::Sm.px())
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .bg(if selected {
                        theme.action_soft
                    } else {
                        theme.surface_high
                    })
                    .text_color(if selected {
                        theme.action_primary
                    } else {
                        theme.text_primary
                    })
                    .child(
                        div()
                            .w_full()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(TextRole::Label.weight())
                            .child(target),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.update_qx_rule_source_target(source_id.clone(), target_id.clone(), cx);
                    })),
            );
        }
        choices
    }

    fn qx_rule_source_target_select(
        &self,
        source: &crate::mihomo::StoredQxRuleSource,
        enabled: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let language = self.language();
        let source_id = source.id.clone();
        let selected_target = self.effective_rule_target(source.target_policy.as_str(), language);
        let open = self.qx_rule_target_popover.as_deref() == Some(source.id.as_str());
        let updating = self.qx_rule_source_target_updates.contains_key(&source.id);
        let menu = self.qx_rule_source_target_menu(&source.id, &selected_target, theme, cx);
        let display_value = if updating {
            language.text("Saving…", "保存中…").to_owned()
        } else {
            format!(
                "{} · {selected_target}",
                language.message(Message::PolicyGroup)
            )
        };
        let trigger = Button::new(format!("qx-rule-target-select-{}", source.id))
            .accessibility_label(language.text(
                "Change target policy for this rule source",
                "修改这个规则源的目标策略",
            ))
            .dropdown_caret(true)
            .with_variant(ButtonVariant::Default)
            .with_size(ControlSize::Compact.component_size())
            .h(ControlSize::Compact.height())
            .child(
                div()
                    .min_w(px(0.0))
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Label.size())
                    .line_height(TextRole::Label.line_height())
                    .font_weight(TextRole::Label.weight())
                    .child(display_value),
            )
            .disabled(!enabled);
        let app = cx.entity();
        crate::components::anchored_popover(
            format!("qx-rule-target-popover-{}", source.id),
            trigger,
            menu,
            240.0,
            320.0,
        )
        .open(open)
        .on_open_change(move |open, _, cx| {
            app.update(cx, |this, cx| {
                this.qx_rule_target_popover = open.then(|| source_id.clone());
                cx.notify();
            });
        })
        .into_any_element()
    }

    fn open_manual_rule_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.manual_rule_editor_state.is_open() {
            if let Some(condition) = self.manual_rule_conditions.first() {
                condition.input.focus_handle(cx).focus(window, cx);
            }
            return;
        }
        self.manual_rule_editor_state = super::ManualRuleEditorState::Creating;
        self.manual_rule_popover = None;
        self.manual_rule_error = None;
        self.manual_rule_condition_count = 1;
        for condition in &self.manual_rule_conditions {
            condition
                .input
                .update(cx, SubscriptionTextInput::clear_without_event);
        }
        for (index, condition) in self.manual_rule_conditions.iter_mut().enumerate() {
            condition.kind = if index == 1 {
                crate::manual_rule::ManualRuleKind::DstPort
            } else {
                crate::manual_rule::ManualRuleKind::default()
            };
        }
        if let Some(target) = self.manual_rule_targets().first() {
            self.manual_rule_target.clone_from(target);
        }
        self.open_manual_rule_dialog(window, cx);
    }

    fn open_manual_rule_editor_for_edit(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.manual_rule_editor_state.is_open() {
            return;
        }
        let Some(rule) = self.manual_rules.get(index).cloned() else {
            return;
        };
        self.manual_rule_editor_state = super::ManualRuleEditorState::Editing(index);
        self.manual_rule_popover = None;
        self.manual_rule_error = None;
        self.manual_rule_condition_count = if rule.is_final() {
            1
        } else {
            rule.conditions().len()
        };
        for (condition_index, editor) in self.manual_rule_conditions.iter_mut().enumerate() {
            if rule.is_final() && condition_index == 0 {
                editor.kind = crate::manual_rule::ManualRuleKind::Final;
                editor
                    .input
                    .update(cx, SubscriptionTextInput::clear_without_event);
            } else if let Some(condition) = rule.conditions().get(condition_index) {
                editor.kind = condition.kind();
                editor.input.update(cx, |input, cx| {
                    input.set_value_without_event(condition.parameter().to_owned(), cx);
                });
            } else {
                editor
                    .input
                    .update(cx, SubscriptionTextInput::clear_without_event);
            }
        }
        let targets = self.manual_rule_targets();
        self.manual_rule_target = if targets.iter().any(|target| target == rule.target()) {
            rule.target().to_owned()
        } else if rule.target() == "Proxy" {
            self.managed_policy_groups
                .first()
                .map_or_else(|| "DIRECT".to_owned(), |group| group.name.clone())
        } else {
            targets
                .first()
                .cloned()
                .unwrap_or_else(|| "DIRECT".to_owned())
        };
        self.open_manual_rule_dialog(window, cx);
    }

    fn open_manual_rule_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let app = cx.entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            app.update(cx, |this, cx| {
                let width = window.viewport_size().width.as_f32();
                let size_class = WindowSizeClass::for_width(width);
                let theme = this.theme();
                this.ensure_manual_rule_input(theme, window, cx);
                this.manual_rule_editor_modal(
                    dialog,
                    theme,
                    this.language(),
                    size_class == WindowSizeClass::Compact,
                    window,
                    cx,
                )
            })
        });
        if let Some(condition) = self.manual_rule_conditions.first() {
            condition.input.focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    fn reset_manual_rule_editor_state(&mut self) {
        self.manual_rule_editor_state = super::ManualRuleEditorState::Closed;
        self.manual_rule_popover = None;
        self.manual_rule_error = None;
    }

    fn close_manual_rule_editor(&mut self, cx: &mut Context<Self>) {
        self.reset_manual_rule_editor_state();
        cx.notify();
    }

    pub(super) fn ensure_manual_rule_input(
        &mut self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self
            .manual_rule_targets()
            .contains(&self.manual_rule_target)
        {
            self.manual_rule_target = self.manual_rule_targets().remove(0);
        }
        if !self.manual_rule_conditions.is_empty() {
            for condition in &self.manual_rule_conditions {
                let placeholder = manual_rule_placeholder(condition.kind, self.language());
                condition.input.update(cx, |input, cx| {
                    input.set_theme(theme, self.dark, cx);
                    input.set_placeholder(placeholder, cx);
                });
            }
            return;
        }
        self.manual_rule_conditions = (0..crate::manual_rule::MAX_CONDITIONS)
            .map(|index| {
                let kind = if index == 1 {
                    crate::manual_rule::ManualRuleKind::DstPort
                } else {
                    crate::manual_rule::ManualRuleKind::default()
                };
                let placeholder = manual_rule_placeholder(kind, self.language());
                let input = cx.new(|cx| {
                    SubscriptionTextInput::new_field(
                        TextInputSpec::new(
                            format!("manual-rule-parameter-{index}"),
                            placeholder,
                            MAX_MANUAL_RULE_INPUT_BYTES,
                            theme,
                            self.dark,
                        ),
                        window,
                        cx,
                    )
                });
                super::ManualRuleConditionEditor { kind, input }
            })
            .collect();
        let Some(store_dir) = self.subscription_store_dir.as_ref() else {
            return;
        };
        match crate::manual_rule::load_manual_rules_in(store_dir) {
            Ok(rules) => {
                self.manual_rules = rules;
                self.sync_routing_rule_group_order();
            }
            Err(error) => {
                self.status = format!(
                    "{}{error}",
                    self.language()
                        .text("Could not read manual rules: ", "无法读取手动分流规则：")
                );
            }
        }
        cx.notify();
    }

    fn manual_rule_targets(&self) -> Vec<String> {
        let mut targets = self
            .managed_policy_groups
            .iter()
            .map(|group| group.name.clone())
            .collect::<Vec<_>>();
        targets.extend(["DIRECT".to_owned(), "REJECT".to_owned()]);
        targets
    }

    fn set_manual_rule_kind(
        &mut self,
        condition_index: usize,
        kind: crate::manual_rule::ManualRuleKind,
        cx: &mut Context<Self>,
    ) {
        if !kind.supported_by(self.runtime.kind()) {
            self.manual_rule_error = Some(crate::manual_rule::ManualRuleError::UnsupportedByKernel);
            self.manual_rule_popover = None;
            cx.notify();
            return;
        }
        let Some(condition) = self.manual_rule_conditions.get_mut(condition_index) else {
            return;
        };
        condition.kind = kind;
        if kind == crate::manual_rule::ManualRuleKind::Final {
            self.manual_rule_condition_count = 1;
            for condition in &self.manual_rule_conditions {
                condition
                    .input
                    .update(cx, SubscriptionTextInput::clear_without_event);
            }
        }
        self.manual_rule_error = None;
        self.manual_rule_popover = None;
        let placeholder = manual_rule_placeholder(kind, self.language());
        if let Some(condition) = self.manual_rule_conditions.get(condition_index) {
            condition
                .input
                .update(cx, |input, cx| input.set_placeholder(placeholder, cx));
        }
        cx.notify();
    }

    fn add_manual_rule_condition(&mut self, cx: &mut Context<Self>) {
        if self.manual_rule_condition_count >= crate::manual_rule::MAX_CONDITIONS
            || self
                .manual_rule_conditions
                .first()
                .map(|condition| condition.kind)
                == Some(crate::manual_rule::ManualRuleKind::Final)
        {
            return;
        }
        let index = self.manual_rule_condition_count;
        self.manual_rule_condition_count += 1;
        self.manual_rule_popover = None;
        self.manual_rule_error = None;
        if let Some(condition) = self.manual_rule_conditions.get(index) {
            condition
                .input
                .update(cx, SubscriptionTextInput::clear_without_event);
        }
        cx.notify();
    }

    fn remove_manual_rule_condition(&mut self, index: usize, cx: &mut Context<Self>) {
        if index == 0 || index >= self.manual_rule_condition_count {
            return;
        }
        for current in index..self.manual_rule_condition_count - 1 {
            let next_kind = self.manual_rule_conditions[current + 1].kind;
            let value = self.manual_rule_conditions[current + 1]
                .input
                .read(cx)
                .value()
                .to_owned();
            self.manual_rule_conditions[current].kind = next_kind;
            self.manual_rule_conditions[current]
                .input
                .update(cx, |input, cx| input.set_value_without_event(value, cx));
        }
        self.manual_rule_condition_count -= 1;
        if let Some(condition) = self
            .manual_rule_conditions
            .get(self.manual_rule_condition_count)
        {
            condition
                .input
                .update(cx, SubscriptionTextInput::clear_without_event);
        }
        self.manual_rule_popover = None;
        self.manual_rule_error = None;
        cx.notify();
    }

    fn apply_manual_rule_edit(
        &mut self,
        editing_index: Option<usize>,
        rule: crate::manual_rule::ManualRule,
    ) -> Result<(), crate::manual_rule::ManualRuleEditError> {
        if let Some(index) = editing_index {
            crate::manual_rule::replace_manual_rule(&mut self.manual_rules, index, rule)?;
        } else {
            if rule.is_final()
                && self
                    .manual_rules
                    .iter()
                    .any(crate::manual_rule::ManualRule::is_final)
            {
                return Err(crate::manual_rule::ManualRuleEditError::FinalAlreadyExists);
            }
            if self
                .manual_rules
                .iter()
                .any(|existing| existing.same_definition(&rule))
            {
                return Err(crate::manual_rule::ManualRuleEditError::Duplicate);
            }
            self.manual_rules.push(rule);
        }
        Ok(())
    }

    fn submit_manual_rule(&mut self, cx: &mut Context<Self>) -> bool {
        if self.manual_rule_editor_state == super::ManualRuleEditorState::Closed {
            return false;
        }
        if self.manual_rule_conditions[..self.manual_rule_condition_count]
            .iter()
            .any(|condition| !condition.kind.supported_by(self.runtime.kind()))
        {
            self.manual_rule_error = Some(crate::manual_rule::ManualRuleError::UnsupportedByKernel);
            cx.notify();
            return false;
        }
        let conditions = self.manual_rule_conditions[..self.manual_rule_condition_count]
            .iter()
            .map(|condition| (condition.kind, condition.input.read(cx).value().to_owned()))
            .collect::<Vec<_>>();
        let condition_count = conditions.len();
        let rule = match crate::manual_rule::ManualRule::parse_conditions(
            conditions,
            &self.manual_rule_target,
        ) {
            Ok(rule) => rule,
            Err(error) => {
                self.manual_rule_error = Some(error);
                cx.notify();
                return false;
            }
        };
        let editing_index = self.manual_rule_editor_state.editing_index();
        let previous_rules = self.manual_rules.clone();
        match self.apply_manual_rule_edit(editing_index, rule) {
            Ok(()) => {}
            Err(crate::manual_rule::ManualRuleEditError::Duplicate) => {
                self.manual_rule_error = Some(crate::manual_rule::ManualRuleError::Duplicate);
                cx.notify();
                return false;
            }
            Err(crate::manual_rule::ManualRuleEditError::FinalAlreadyExists) => {
                self.manual_rule_error =
                    Some(crate::manual_rule::ManualRuleError::FinalAlreadyExists);
                cx.notify();
                return false;
            }
            Err(crate::manual_rule::ManualRuleEditError::Missing) => {
                self.reset_manual_rule_editor_state();
                cx.notify();
                return false;
            }
        }
        if let Some(index) = self
            .manual_rules
            .iter()
            .position(crate::manual_rule::ManualRule::is_final)
        {
            let final_rule = self.manual_rules.remove(index);
            self.manual_rules.push(final_rule);
        }
        let completion = self
            .language()
            .text("Manual rules updated", "手动分流规则已更新")
            .to_owned();
        if !self.persist_manual_rules(completion, previous_rules.clone(), cx) {
            self.manual_rules = previous_rules;
            return false;
        }
        self.manual_rule_error = None;
        self.reset_manual_rule_editor_state();
        record_event(
            LogLevel::Info,
            if editing_index.is_some() {
                "routing.manual_rule.updated"
            } else {
                "routing.manual_rule.added"
            },
            format!(
                "conditions={} target={} total={}",
                condition_count,
                self.manual_rule_target,
                self.manual_rules.len()
            ),
        );
        true
    }

    fn remove_manual_rule(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.manual_rules.len() {
            return;
        }
        let previous_rules = self.manual_rules.clone();
        let removed = self.manual_rules.remove(index);
        let completion = self
            .language()
            .text("Manual rule removed", "手动规则已删除")
            .to_owned();
        if !self.persist_manual_rules(completion, previous_rules.clone(), cx) {
            self.manual_rules = previous_rules;
            return;
        }
        self.manual_rule_error = None;
        record_event(
            LogLevel::Info,
            "routing.manual_rule.removed",
            format!(
                "conditions={} total={}",
                removed.conditions().len(),
                self.manual_rules.len()
            ),
        );
    }

    fn set_manual_rule_enabled(&mut self, index: usize, enabled: bool, cx: &mut Context<Self>) {
        let previous_rules = self.manual_rules.clone();
        let Some(rule) = self.manual_rules.get_mut(index) else {
            return;
        };
        if rule.is_enabled() == enabled {
            return;
        }
        rule.set_enabled(enabled);
        let completion = self
            .language()
            .text(
                if enabled {
                    "Manual rule enabled"
                } else {
                    "Manual rule disabled"
                },
                if enabled {
                    "手动规则已启用"
                } else {
                    "手动规则已禁用"
                },
            )
            .to_owned();
        if !self.persist_manual_rules(completion, previous_rules.clone(), cx) {
            self.manual_rules = previous_rules;
            return;
        }
        record_event(
            LogLevel::Info,
            if enabled {
                "routing.manual_rule.enabled"
            } else {
                "routing.manual_rule.disabled"
            },
            format!("index={index} total={}", self.manual_rules.len()),
        );
        cx.notify();
    }

    fn persist_manual_rules(
        &mut self,
        completion: String,
        previous_rules: Vec<crate::manual_rule::ManualRule>,
        cx: &mut Context<Self>,
    ) -> bool {
        let language = self.language();
        if self.routing_apply_state.is_busy() {
            language
                .message(Message::RoutingApplyBusy)
                .clone_into(&mut self.status);
            cx.notify();
            return false;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            language
                .message(Message::ManualRulesLocationUnavailable)
                .clone_into(&mut self.status);
            cx.notify();
            return false;
        };
        let store_snapshot = match mihomo::SubscriptionStoreSnapshot::capture(&store_dir) {
            Ok(snapshot) => snapshot,
            Err(_error) => {
                language
                    .message(Message::StoreTransactionUnavailable)
                    .clone_into(&mut self.status);
                cx.notify();
                return false;
            }
        };
        let previous_order = self.routing_rule_group_order.clone();
        self.sync_routing_rule_group_order();
        if mihomo::save_routing_rule_group_order_in(&store_dir, &self.routing_rule_group_order)
            .is_err()
        {
            self.routing_rule_group_order = previous_order;
            let _ = store_snapshot.restore(&store_dir);
            language
                .message(Message::RuleGroupOrderSaveFailed)
                .clone_into(&mut self.status);
            cx.notify();
            return false;
        }
        if let Err(error) = crate::manual_rule::save_manual_rules_in(&store_dir, &self.manual_rules)
        {
            let _ = store_snapshot.clone().restore(&store_dir);
            self.status = format!(
                "{}{error}",
                language.message(Message::ManualRulesSaveFailed)
            );
            cx.notify();
            return false;
        }
        self.start_routing_runtime_apply(
            store_dir,
            completion,
            super::RoutingApplyRollback {
                manual_rules: previous_rules,
                group_order: previous_order,
                store_snapshot,
            },
            cx,
        );
        true
    }

    fn start_routing_runtime_apply(
        &mut self,
        store_dir: std::path::PathBuf,
        completion: String,
        rollback: super::RoutingApplyRollback,
        cx: &mut Context<Self>,
    ) {
        let started = self.routing_apply_state.begin();
        debug_assert!(started, "routing apply must be idle before spawning");
        if !started {
            return;
        }
        self.status = format!(
            "{} · {}",
            completion,
            self.language().message(Message::ApplyingChanges)
        );
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        let disk_rollback = rollback.clone();
        cx.spawn(async move |this, cx| {
            let (apply, rollback_error) = executor
                .spawn(async move {
                    let apply =
                        SourceRuntimeApply::from_result(runtime.apply_saved_sources(&store_dir));
                    let rollback_error = if apply.requires_source_rollback() {
                        disk_rollback
                            .store_snapshot
                            .restore(&store_dir)
                            .map_err(|error| error.to_string())
                            .err()
                    } else {
                        None
                    };
                    (apply, rollback_error)
                })
                .await;
            this.update(cx, |this, cx| {
                this.routing_apply_state.finish();
                if apply.requires_source_rollback() {
                    this.manual_rules = rollback.manual_rules;
                    this.routing_rule_group_order = rollback.group_order;
                }
                apply.reconcile_proxy_mode(&mut this.proxy_mode);
                this.status = if let Some(rollback_error) = rollback_error {
                    format!(
                        "{}{} · {}{rollback_error}",
                        completion,
                        apply.status_suffix(this.language()),
                        this.language().text(
                            "could not restore the previous saved rules: ",
                            "无法恢复先前保存的规则：",
                        )
                    )
                } else {
                    format!(
                        "{}{}",
                        completion,
                        if apply.requires_source_rollback() {
                            apply.status_suffix_after_source_rollback(this.language())
                        } else {
                            apply.status_suffix(this.language())
                        }
                    )
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn sync_routing_rule_group_order(&mut self) {
        self.routing_rule_group_order = mihomo::normalized_routing_rule_group_order(
            &self.routing_rule_group_order,
            !self.manual_rules.is_empty(),
            &self.qx_rule_sources,
        );
    }

    fn move_routing_rule_group(&mut self, group_id: &str, direction: i8, cx: &mut Context<Self>) {
        if self.routing_apply_state.is_busy() {
            self.language()
                .message(Message::RoutingApplyBusy)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        self.sync_routing_rule_group_order();
        let previous = self.routing_rule_group_order.clone();
        if !mihomo::move_routing_rule_group(&mut self.routing_rule_group_order, group_id, direction)
        {
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            self.routing_rule_group_order = previous;
            return;
        };
        let store_snapshot = match mihomo::SubscriptionStoreSnapshot::capture(&store_dir) {
            Ok(snapshot) => snapshot,
            Err(_error) => {
                self.routing_rule_group_order = previous;
                self.language()
                    .message(Message::StoreTransactionUnavailable)
                    .clone_into(&mut self.status);
                cx.notify();
                return;
            }
        };
        if mihomo::save_routing_rule_group_order_in(&store_dir, &self.routing_rule_group_order)
            .is_err()
        {
            self.routing_rule_group_order = previous;
            self.language()
                .message(Message::RuleGroupOrderSaveFailed)
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let language = self.language();
        let completion = if direction < 0 {
            language.text("Rule group moved up", "规则分组已上移")
        } else {
            language.text("Rule group moved down", "规则分组已下移")
        }
        .to_owned();
        self.start_routing_runtime_apply(
            store_dir,
            completion,
            super::RoutingApplyRollback {
                manual_rules: self.manual_rules.clone(),
                group_order: previous,
                store_snapshot,
            },
            cx,
        );
    }

    fn manual_rule_kind_menu(
        &self,
        condition_index: usize,
        selected_kind: crate::manual_rule::ManualRuleKind,
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let kernel = self.runtime.kind();
        let editing_index = self.manual_rule_editor_state.editing_index();
        let final_available = !self
            .manual_rules
            .iter()
            .enumerate()
            .any(|(index, rule)| rule.is_final() && Some(index) != editing_index);
        let mut choices = div().id("manual-rule-kind-choices");
        for kind in crate::manual_rule::ManualRuleKind::ALL {
            if kind == crate::manual_rule::ManualRuleKind::Final && condition_index > 0 {
                continue;
            }
            let supported = kind.supported_by(kernel)
                && (kind != crate::manual_rule::ManualRuleKind::Final || final_available);
            let selected = selected_kind == kind;
            let detail = if supported {
                manual_rule_kind_detail(kind, language)
            } else if kind == crate::manual_rule::ManualRuleKind::Final {
                language.text("Already configured", "已经配置")
            } else if kind == crate::manual_rule::ManualRuleKind::UserAgent {
                language.text("No exact kernel equivalent", "内核无精确等价规则")
            } else {
                language.text("Available with Mihomo", "仅 Mihomo 可用")
            };
            choices = choices.child(
                div()
                    .id(format!(
                        "manual-rule-kind-{condition_index}-{}",
                        kind.storage_key()
                    ))
                    .role(Role::Button)
                    .aria_label(kind.display_label())
                    .tab_stop(supported)
                    .focusable()
                    .when(supported, gpui::Styled::cursor_pointer)
                    .min_h(px(36.0))
                    .px(Space::Md.px())
                    .py(Space::Xs.px())
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .bg(if selected {
                        theme.action_soft
                    } else {
                        theme.surface_high
                    })
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(TextRole::Label.weight())
                            .text_color(if supported {
                                theme.text_primary
                            } else {
                                theme.text_tertiary
                            })
                            .child(kind.display_label()),
                    )
                    .child(
                        div()
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .text_color(theme.text_tertiary)
                            .child(detail),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if supported {
                            this.set_manual_rule_kind(condition_index, kind, cx);
                        }
                    })),
            );
        }
        choices
    }

    fn manual_rule_target_menu(&self, theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let mut choices = div().id("manual-rule-target-choices");
        for target in self.manual_rule_targets() {
            let selected = self.manual_rule_target == target;
            let row_target = target.clone();
            choices = choices.child(
                div()
                    .id(format!("manual-rule-target-{target}"))
                    .role(Role::Button)
                    .aria_label(format!("Target {target}"))
                    .tab_stop(true)
                    .focusable()
                    .cursor_pointer()
                    .min_h(ControlSize::Standard.min_pointer_target())
                    .px(Space::Md.px())
                    .py(Space::Sm.px())
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .bg(if selected {
                        theme.action_soft
                    } else {
                        theme.surface_high
                    })
                    .text_size(TextRole::Label.size())
                    .line_height(TextRole::Label.line_height())
                    .font_weight(TextRole::Label.weight())
                    .text_color(if selected {
                        theme.action_primary
                    } else {
                        theme.text_primary
                    })
                    .child(target)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.manual_rule_target.clone_from(&row_target);
                        this.manual_rule_error = None;
                        this.manual_rule_popover = None;
                        cx.notify();
                    })),
            );
        }
        choices
    }

    fn manual_rule_select(
        id: &str,
        label: &'static str,
        value: String,
        menu: impl gpui::IntoElement,
        open: bool,
        width: f32,
        on_open_change: impl Fn(&bool, &mut gpui::Window, &mut gpui::App) + 'static,
    ) -> AnyElement {
        let trigger = Button::new(id.to_owned())
            .accessibility_label(label)
            .dropdown_caret(true)
            .with_variant(ButtonVariant::Default)
            .with_size(ControlSize::Standard.component_size())
            .h(ControlSize::Standard.height())
            .w_full()
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Label.size())
                    .line_height(TextRole::Label.line_height())
                    .font_weight(TextRole::Label.weight())
                    .child(value),
            );

        crate::components::anchored_popover(format!("{id}-popover"), trigger, menu, width, 360.0)
            .open(open)
            .on_open_change(on_open_change)
            .into_any_element()
    }

    fn manual_rule_condition_editor(
        &self,
        condition_index: usize,
        kind: crate::manual_rule::ManualRuleKind,
        theme: Theme,
        language: Language,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let input = self
            .manual_rule_conditions
            .get(condition_index)
            .expect("manual rule condition input is initialized")
            .input
            .clone();
        let kind_width = if compact { 260.0 } else { 240.0 };
        let kind_popover = ManualRulePopover::Kind(condition_index);
        let kind_open = self.manual_rule_popover == Some(kind_popover);
        let kind_menu = self.manual_rule_kind_menu(condition_index, kind, theme, language, cx);
        let select_id = format!("manual-rule-kind-select-{condition_index}");
        let label = language.text("Choose condition type", "选择条件类型");
        let app = cx.entity();
        let kind_select = Self::manual_rule_select(
            &select_id,
            label,
            kind.display_label().to_owned(),
            kind_menu,
            kind_open,
            kind_width,
            move |open, _, cx| {
                app.update(cx, |this, cx| {
                    this.manual_rule_popover = open.then_some(kind_popover);
                    cx.notify();
                });
            },
        );
        let is_final = kind == crate::manual_rule::ManualRuleKind::Final;
        let mut row = div()
            .mt_3()
            .child(div().child(field_label(
                if condition_index == 0 {
                    language.text("Condition 1", "条件 1").to_owned()
                } else if language == Language::English {
                    format!("AND · Condition {}", condition_index + 1)
                } else {
                    format!("并且 · 条件 {}", condition_index + 1)
                },
                theme,
            )))
            .child(
                div()
                    .flex()
                    .when(compact, gpui::Styled::flex_col)
                    .items_stretch()
                    .gap_2()
                    .child(
                        div()
                            .when(compact, gpui::Styled::w_full)
                            .when(!compact, |item| item.w(px(220.0)))
                            .flex_shrink_0()
                            .child(kind_select),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .when(!is_final, |field| field.child(input))
                            .when(is_final, |field| {
                                field.child(
                                    div()
                                        .h(ControlSize::Standard.height())
                                        .px(Space::Md.px())
                                        .flex()
                                        .items_center()
                                        .rounded(Radius::Control.px())
                                        .bg(theme.surface_low)
                                        .text_size(TextRole::Body.size())
                                        .line_height(TextRole::Body.line_height())
                                        .text_color(theme.text_secondary)
                                        .child(language.text(
                                            "Matches only after every rule above misses",
                                            "仅在上方所有规则均未命中时生效",
                                        )),
                                )
                            }),
                    ),
            );
        if condition_index > 0 {
            row = row.child(
                Button::new(format!("remove-manual-rule-condition-{condition_index}"))
                    .accessibility_label(language.text("Remove this condition", "移除这个条件"))
                    .label(language.text("Remove condition", "移除条件"))
                    .text()
                    .with_size(ControlSize::Compact.component_size())
                    .h(ControlSize::Compact.height())
                    .mt(Space::Sm.px())
                    .cursor_pointer()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.status_error)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.remove_manual_rule_condition(condition_index, cx);
                    })),
            );
        }
        row
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn manual_rule_editor_modal(
        &self,
        dialog: Dialog,
        theme: Theme,
        language: Language,
        compact: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Dialog {
        let target_width = if compact { 260.0 } else { 240.0 };
        let target_open = self.manual_rule_popover == Some(ManualRulePopover::Target);
        let target_menu = self.manual_rule_target_menu(theme, cx);
        let app = cx.entity();
        let target = Self::manual_rule_select(
            "manual-rule-target-select",
            language.text("Choose target policy", "选择目标策略"),
            self.manual_rule_target.clone(),
            target_menu,
            target_open,
            target_width,
            move |open, _, cx| {
                app.update(cx, |this, cx| {
                    this.manual_rule_popover = open.then_some(ManualRulePopover::Target);
                    cx.notify();
                });
            },
        );

        let editing = self.manual_rule_editor_state.editing_index().is_some();
        let final_selected = self
            .manual_rule_conditions
            .first()
            .map(|condition| condition.kind)
            == Some(crate::manual_rule::ManualRuleKind::Final);
        let mut conditions = div();
        for condition_index in 0..self.manual_rule_condition_count {
            conditions = conditions.child(self.manual_rule_condition_editor(
                condition_index,
                self.manual_rule_conditions[condition_index].kind,
                theme,
                language,
                compact,
                cx,
            ));
        }
        if !final_selected && self.manual_rule_condition_count < crate::manual_rule::MAX_CONDITIONS
        {
            conditions = conditions.child(
                Button::new("add-manual-rule-condition")
                    .accessibility_label(language.text("Add an AND condition", "添加并且条件"))
                    .label(language.text("+ Add AND condition", "+ 添加“并且”条件"))
                    .with_variant(ButtonVariant::Default)
                    .with_size(ControlSize::Standard.component_size())
                    .h(ControlSize::Standard.height())
                    .mt(Space::Md.px())
                    .px(Space::Md.px())
                    .py(Space::Sm.px())
                    .cursor_pointer()
                    .border_color(theme.outline_strong)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.action_primary)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.add_manual_rule_condition(cx);
                    })),
            );
        }

        let target_field = div()
            .mt_4()
            .child(field_label(
                language.text("Policy group after match", "命中后的策略组"),
                theme,
            ))
            .child(target);

        let footer = div()
            .flex_shrink_0()
            .px_5()
            .py_3()
            .border_t_1()
            .border_color(theme.outline_subtle)
            .flex()
            .items_center()
            .justify_end()
            .gap_2()
            .child(
                style_action_button(
                    Button::new("cancel-manual-rule")
                        .accessibility_label(language.text("Cancel editing rule", "取消编辑规则"))
                        .label(language.message(Message::Cancel)),
                    ActionRole::Secondary,
                    ControlSize::Standard,
                )
                .px(Space::Lg.px())
                .cursor_pointer()
                .border_color(theme.outline_subtle)
                .bg(theme.surface_high)
                .text_color(theme.text_primary)
                .font_weight(FontWeight::SEMIBOLD)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.close_manual_rule_editor(cx);
                    window.close_dialog(cx);
                })),
            )
            .child(
                style_action_button(
                    Button::new("save-manual-rule")
                        .accessibility_label(if editing {
                            language.text("Save manual rule changes", "保存手动规则修改")
                        } else {
                            language.text("Add manual rule", "添加手动规则")
                        })
                        .label(if editing {
                            language.message(Message::SaveChanges)
                        } else {
                            language.message(Message::AddRule)
                        })
                        .disabled(self.routing_apply_state.is_busy()),
                    ActionRole::Primary,
                    ControlSize::Standard,
                )
                .px(Space::Lg.px())
                .cursor_pointer()
                .bg(theme.action_primary)
                .text_color(theme.action_on_primary)
                .font_weight(FontWeight::SEMIBOLD)
                .on_click(cx.listener(|this, _, window, cx| {
                    if this.submit_manual_rule(cx) {
                        window.close_dialog(cx);
                    }
                })),
            );

        let viewport = window.viewport_size();
        let dialog_width = (viewport.width.as_f32() - 32.0).clamp(280.0, 720.0);
        let estimated_height = if compact {
            520.0
        } else {
            match self.manual_rule_condition_count {
                0 | 1 => 368.0,
                2 => 458.0,
                3 => 548.0,
                _ => 638.0,
            }
        };
        let margin_top = ((viewport.height.as_f32() - estimated_height) / 2.0).max(16.0);
        let app = cx.entity();

        dialog
            .width(px(dialog_width))
            .max_h(px((viewport.height.as_f32() - 32.0).max(320.0)))
            .margin_top(px(margin_top))
            .overlay(true)
            .overlay_closable(true)
            .keyboard(true)
            .close_button(false)
            .p_0()
            .rounded_md()
            .bg(theme.surface_high)
            .overflow_hidden()
            .title(
                div()
                    .id("manual-rule-modal-header")
                    .flex_shrink_0()
                    .px_5()
                    .py_4()
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .child(
                        div()
                            .text_size(px(17.0))
                            .line_height(TextRole::SectionTitle.line_height())
                            .font_weight(TextRole::SectionTitle.weight())
                            .child(if editing {
                                language.text("Edit routing rule", "编辑分流规则")
                            } else {
                                language.text("Add routing rule", "添加分流规则")
                            }),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .font_weight(FontWeight::NORMAL)
                            .text_color(theme.text_secondary)
                            .child(if final_selected {
                                language.text(
                                    "FINAL is always evaluated last and handles unmatched traffic.",
                                    "FINAL 始终最后匹配，用于处理此前未命中的流量。",
                                )
                            } else {
                                language.text(
                                    "All conditions must match. Group order determines rule priority.",
                                    "同一条规则中的条件必须全部命中；分组顺序决定规则优先级。",
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .id("manual-rule-modal-body")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .px_5()
                    .py_4()
                    .child(conditions)
                    .child(target_field)
                    .when_some(self.manual_rule_error, |body, error| {
                        body.child(
                            div()
                                .mt_3()
                                .p_3()
                                .rounded_md()
                                .bg(theme.surface_low)
                                .text_size(TextRole::Body.size())
                                .line_height(TextRole::Body.line_height())
                                .text_color(theme.status_error)
                                .child(manual_rule_error_label(error, language)),
                        )
                    }),
            )
            .footer(footer)
            .on_close(move |_, _, cx| {
                app.update(cx, ManisApp::close_manual_rule_editor);
            })
    }

    fn manual_routing_rule_row(
        &self,
        order: usize,
        index: usize,
        rule: &crate::manual_rule::ManualRule,
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let enabled = rule.is_enabled();
        let target = self.effective_rule_target(rule.target(), language);
        let matchers = Self::manual_rule_matchers(rule, enabled, theme, language);
        let edit_label = if language == Language::English {
            format!("Manual rule {order}. Enter edits, Space toggles, Delete removes the rule")
        } else {
            format!("第 {order} 条手动规则。回车编辑，空格启用或禁用，Delete 删除")
        };
        let row = div()
            .id(format!("manual-routing-rule-{index}"))
            .mt_1()
            .min_h(px(44.0))
            .px(Space::Md.px())
            .py(Space::Sm.px())
            .rounded(Radius::Row.px())
            .bg(if enabled {
                theme.surface_low
            } else {
                theme.surface_base
            })
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .cursor_pointer()
            .role(Role::Button)
            .tab_stop(true)
            .focusable()
            .aria_label(edit_label)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_manual_rule_editor_for_edit(index, window, cx);
            }))
            .on_key_down(cx.listener(move |this, event, window, cx| {
                let Some(action) = manual_rule_keyboard_action(event) else {
                    return;
                };
                cx.stop_propagation();
                match action {
                    ManualRuleKeyboardAction::Edit => {
                        this.open_manual_rule_editor_for_edit(index, window, cx);
                    }
                    ManualRuleKeyboardAction::Toggle => {
                        this.set_manual_rule_enabled(index, !enabled, cx);
                    }
                    ManualRuleKeyboardAction::Delete => {
                        this.remove_manual_rule(index, cx);
                    }
                }
            }))
            .child(
                div()
                    .w(px(42.0))
                    .flex_shrink_0()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_tertiary)
                    .child(format!("#{order:03}")),
            )
            .child(matchers)
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(TextRole::Data.size())
                    .line_height(TextRole::Data.line_height())
                    .font_weight(TextRole::Data.weight())
                    .text_color(if enabled {
                        theme.action_primary
                    } else {
                        theme.text_tertiary
                    })
                    .child(target),
            )
            .when(!enabled, |row| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .px_2()
                        .py_1()
                        .rounded(Radius::Control.px())
                        .bg(theme.surface_chrome)
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_tertiary)
                        .child(language.text("Disabled", "已禁用")),
                )
            });
        Self::manual_rule_context_menu(row, index, enabled, language, cx.entity())
    }

    fn manual_rule_context_menu(
        row: Stateful<Div>,
        index: usize,
        enabled: bool,
        language: Language,
        app: Entity<Self>,
    ) -> AnyElement {
        let toggle_label = if enabled {
            language.message(Message::Disable)
        } else {
            language.message(Message::Enable)
        };
        row.context_menu(move |menu, _, _| {
            let toggle_app = app.clone();
            let delete_app = app.clone();
            menu.item(PopupMenuItem::new(toggle_label).on_click(move |_, _, cx| {
                toggle_app.update(cx, |this, cx| {
                    this.set_manual_rule_enabled(index, !enabled, cx);
                });
            }))
            .separator()
            .item(
                PopupMenuItem::new(language.message(Message::Delete)).on_click(move |_, _, cx| {
                    delete_app.update(cx, |this, cx| {
                        this.remove_manual_rule(index, cx);
                    });
                }),
            )
        })
        .into_any_element()
    }

    fn manual_rule_matchers(
        rule: &crate::manual_rule::ManualRule,
        enabled: bool,
        theme: Theme,
        language: Language,
    ) -> Div {
        let primary_text = if enabled {
            theme.text_primary
        } else {
            theme.text_tertiary
        };
        let secondary_text = if enabled {
            theme.text_secondary
        } else {
            theme.text_tertiary
        };
        if rule.is_final() {
            return div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(Space::Sm.px())
                .child(
                    div()
                        .px(Space::Sm.px())
                        .py(Space::Xs.px())
                        .rounded(Radius::Control.px())
                        .bg(theme.surface_high)
                        .text_size(TextRole::Label.size())
                        .line_height(TextRole::Label.line_height())
                        .font_weight(TextRole::Label.weight())
                        .text_color(primary_text)
                        .child("FINAL"),
                )
                .child(
                    div()
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .text_color(secondary_text)
                        .child(language.text("Fallback · always last", "兜底规则 · 始终最后")),
                );
        }
        let mut matchers = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1();
        for (condition_index, condition) in rule.conditions().iter().enumerate() {
            if condition_index > 0 {
                matchers = matchers.child(
                    div()
                        .mx_1()
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_tertiary)
                        .child(language.text("AND", "并且")),
                );
            }
            matchers = matchers.child(
                div()
                    .flex()
                    .items_center()
                    .gap(Space::Sm.px())
                    .px(Space::Sm.px())
                    .py(Space::Xs.px())
                    .rounded(Radius::Control.px())
                    .bg(theme.surface_high)
                    .child(
                        div()
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(secondary_text)
                            .child(condition.kind().display_label()),
                    )
                    .child(
                        div()
                            .text_size(TextRole::Data.size())
                            .line_height(TextRole::Data.line_height())
                            .text_color(primary_text)
                            .child(condition.parameter().to_owned()),
                    ),
            );
        }
        matchers
    }

    fn rule_group_order_controls(
        &self,
        group_id: &str,
        group_name: &str,
        position: (usize, usize),
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> Div {
        let (position, group_count) = position;
        let up_id = group_id.to_owned();
        let down_id = group_id.to_owned();
        div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(Space::Xs.px())
            .child(
                Button::new(format!("move-rule-group-up-{group_id}"))
                    .accessibility_label(if language == Language::English {
                        format!("Move {group_name} up")
                    } else {
                        format!("上移{group_name}")
                    })
                    .icon(IconName::ArrowUp)
                    .text()
                    .with_size(ControlSize::Icon.component_size())
                    .text_color(theme.text_secondary)
                    .disabled(position == 0 || self.routing_apply_state.is_busy())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.move_routing_rule_group(&up_id, -1, cx);
                    })),
            )
            .child(
                Button::new(format!("move-rule-group-down-{group_id}"))
                    .accessibility_label(if language == Language::English {
                        format!("Move {group_name} down")
                    } else {
                        format!("下移{group_name}")
                    })
                    .icon(IconName::ArrowDown)
                    .text()
                    .with_size(ControlSize::Icon.component_size())
                    .text_color(theme.text_secondary)
                    .disabled(position + 1 >= group_count || self.routing_apply_state.is_busy())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.move_routing_rule_group(&down_id, 1, cx);
                    })),
            )
    }

    #[allow(clippy::too_many_lines)]
    fn active_rules_panel(
        &self,
        theme: Theme,
        language: Language,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let remote_count = self
            .qx_rule_sources
            .iter()
            .filter(|source| source.enabled)
            .map(|source| source.rule_count)
            .sum::<usize>();
        let disabled_remote_count = self
            .qx_rule_sources
            .iter()
            .filter(|source| !source.enabled)
            .map(|source| source.rule_count)
            .sum::<usize>();
        let enabled_manual_count = self
            .manual_rules
            .iter()
            .filter(|rule| rule.is_enabled())
            .count();
        let disabled_manual_count = self.manual_rules.len() - enabled_manual_count;
        let disabled_count = disabled_manual_count + disabled_remote_count;
        let active_count = enabled_manual_count + remote_count;
        let mut list = div()
            .id("active-routing-rules")
            .w_full()
            .flex_1()
            .min_w(px(0.0))
            .p(if compact {
                Space::Md.px()
            } else {
                Space::Lg.px()
            })
            .rounded(Radius::Pane.px())
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(Space::Md.px())
                    .child(section_heading(
                        language.text("Active rules", "生效规则"),
                        language.text(
                            "Groups match from top to bottom; use the arrows to change priority.",
                            "分组从上到下匹配；使用箭头调整优先级。",
                        ),
                        None,
                        theme,
                    ))
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap(Space::Sm.px())
                            .child(status_badge(
                                if disabled_count == 0 {
                                    language.count(CountNoun::Rule, active_count)
                                } else if language == Language::English {
                                    format!("{active_count} active · {disabled_count} disabled")
                                } else {
                                    format!("{active_count} 条生效 · {disabled_count} 条已禁用")
                                },
                                StatusTone::Route,
                                theme,
                            ))
                            .child(
                                action_button(
                                    "open-manual-rule-editor",
                                    language.message(Message::AddRule),
                                    ActionRole::Primary,
                                    ControlSize::Compact,
                                )
                                .accessibility_label(
                                    language.text("Add routing rule", "添加分流规则"),
                                )
                                .cursor_pointer()
                                .bg(theme.action_primary)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.action_on_primary)
                                .on_click(cx.listener(
                                    |this, _, window, cx| {
                                        this.open_manual_rule_editor(window, cx);
                                    },
                                )),
                            ),
                    ),
            );

        let group_order = mihomo::normalized_routing_rule_group_order(
            &self.routing_rule_group_order,
            !self.manual_rules.is_empty(),
            &self.qx_rule_sources,
        );
        let group_count = group_order.len();
        let mut order = 1;
        for (group_position, group_id) in group_order.iter().enumerate() {
            if group_id == mihomo::MANUAL_ROUTING_RULE_GROUP_ID {
                let open = rule_group_is_open(&self.node_workspace, MANUAL_RULES_EXPANSION_KEY);
                let group_name = language.text("Manual rules", "手动规则");
                let detail = if disabled_manual_count == 0 && language == Language::English {
                    format!(
                        "{} · Saved locally",
                        language.count(CountNoun::Rule, self.manual_rules.len())
                    )
                } else if disabled_manual_count == 0 {
                    format!(
                        "{} · 本地保存",
                        language.count(CountNoun::Rule, self.manual_rules.len())
                    )
                } else if language == Language::English {
                    format!(
                        "{} · {disabled_manual_count} disabled · Saved locally",
                        language.count(CountNoun::Rule, self.manual_rules.len())
                    )
                } else {
                    format!(
                        "{} · {disabled_manual_count} 条已禁用 · 本地保存",
                        language.count(CountNoun::Rule, self.manual_rules.len())
                    )
                };
                let title_detail = div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(TextRole::Label.weight())
                            .child(group_name),
                    )
                    .child(
                        div()
                            .mt(Space::Xs.px())
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .text_color(theme.text_tertiary)
                            .child(detail),
                    )
                    .child(
                        div()
                            .mt(Space::Xs.px())
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .text_color(theme.text_secondary)
                            .child(language.text(
                                "Click a rule to edit; right-click to disable or delete.",
                                "点击规则编辑；右键可禁用或删除。",
                            )),
                    );
                let title = div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(Space::Sm.px())
                    .child(title_detail)
                    .child(self.rule_group_order_controls(
                        group_id,
                        group_name,
                        (group_position, group_count),
                        theme,
                        language,
                        cx,
                    ));
                let mut rules = div()
                    .px(if compact {
                        Space::Sm.px()
                    } else {
                        Space::Md.px()
                    })
                    .pb(Space::Md.px())
                    .border_t_1()
                    .border_color(theme.outline_subtle)
                    .bg(theme.surface_high);
                for (index, rule) in self.manual_rules.iter().enumerate() {
                    rules = rules.child(
                        self.manual_routing_rule_row(order, index, rule, theme, language, cx),
                    );
                    order += 1;
                }
                let manual_group = Accordion::new("routing-manual-rules")
                    .bordered(false)
                    .with_size(Size::Large)
                    .mt(Space::Lg.px())
                    .rounded(Radius::Pane.px())
                    .overflow_hidden()
                    .border_1()
                    .border_color(theme.outline_subtle)
                    .bg(theme.surface_low)
                    .item(|item| {
                        item.open(open)
                            .rounded(Radius::Pane.px())
                            .overflow_hidden()
                            .title_style(accordion_title_style(compact))
                            .content_style(accordion_content_style())
                            .bg(theme.surface_low)
                            .title(title)
                            .child(rules)
                    })
                    .on_toggle_click(cx.listener(|this, open_indices: &[usize], _, cx| {
                        let should_collapse = !open_indices.contains(&0);
                        if this
                            .node_workspace
                            .is_group_collapsed(MANUAL_RULES_EXPANSION_KEY)
                            != should_collapse
                        {
                            this.node_workspace.toggle_group(MANUAL_RULES_EXPANSION_KEY);
                            this.persist_node_workspace();
                            cx.notify();
                        }
                    }));
                list = list.child(manual_group);
                continue;
            }
            let Some((source_index, source)) = self
                .qx_rule_sources
                .iter()
                .enumerate()
                .find(|(_, source)| source.id == *group_id)
            else {
                continue;
            };
            let parsed = QxRuleList::parse(&source.content);
            let rule_count = parsed.rules.len();
            let expansion_key = rule_source_expansion_key(&source.id);
            let open = rule_group_is_open(&self.node_workspace, &expansion_key);
            let name = source.source.subscription_name().unwrap_or_else(|| {
                if language == Language::English {
                    format!("Rule source {}", source_index + 1)
                } else {
                    format!("规则源 {}", source_index + 1)
                }
            });
            let update = source_update_label(
                source.last_successful_update_unix_secs,
                mihomo::current_unix_secs(),
                language,
            );
            let target_policy = self.effective_rule_target(source.target_policy.as_str(), language);
            let detail = if !source.enabled && language == Language::English {
                format!("{rule_count} rules · Disabled")
            } else if !source.enabled {
                format!("{rule_count} 条规则 · 已停用")
            } else if language == Language::English {
                format!("{rule_count} rules · Target {target_policy} · {update}")
            } else {
                format!("{rule_count} 条规则 · 目标 {target_policy} · {update}")
            };
            let toggle_key = expansion_key.clone();
            let title_detail = div()
                .flex_1()
                .min_w(px(0.0))
                .child(
                    div()
                        .overflow_x_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(TextRole::Label.size())
                        .line_height(TextRole::Label.line_height())
                        .font_weight(TextRole::Label.weight())
                        .text_color(if source.enabled {
                            theme.text_primary
                        } else {
                            theme.text_tertiary
                        })
                        .child(name.clone()),
                )
                .child(
                    div()
                        .mt(Space::Xs.px())
                        .overflow_x_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .text_color(theme.text_tertiary)
                        .child(detail),
                );
            let title = div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .gap(Space::Sm.px())
                .child(title_detail)
                .child(self.qx_rule_source_target_select(
                    source,
                    source.enabled && !self.source_refresh_busy(),
                    theme,
                    cx,
                ))
                .child(self.rule_group_order_controls(
                    group_id,
                    &name,
                    (group_position, group_count),
                    theme,
                    language,
                    cx,
                ));
            let mut rules = div()
                .px(if compact {
                    Space::Sm.px()
                } else {
                    Space::Md.px()
                })
                .pb(Space::Md.px())
                .border_t_1()
                .border_color(theme.outline_subtle)
                .bg(theme.surface_high);
            for rule in parsed.rules {
                rules = rules.child(Self::routing_rule_row(
                    order,
                    Self::qx_rule_kind_label(rule.kind),
                    &rule.value,
                    &target_policy,
                    theme,
                ));
                order += 1;
            }
            let source_group = Accordion::new(format!("routing-rule-source-{}", source.id))
                .bordered(false)
                .with_size(Size::Large)
                .mt(Space::Lg.px())
                .rounded(Radius::Pane.px())
                .overflow_hidden()
                .border_1()
                .border_color(theme.outline_subtle)
                .bg(theme.surface_low)
                .item(|item| {
                    item.open(open)
                        .rounded(Radius::Pane.px())
                        .overflow_hidden()
                        .title_style(accordion_title_style(compact))
                        .content_style(accordion_content_style())
                        .bg(theme.surface_low)
                        .title(title)
                        .child(rules)
                })
                .on_toggle_click(cx.listener(move |this, open_indices: &[usize], _, cx| {
                    let should_collapse = !open_indices.contains(&0);
                    if this.node_workspace.is_group_collapsed(&toggle_key) != should_collapse {
                        this.node_workspace.toggle_group(&toggle_key);
                        this.persist_node_workspace();
                        this.language()
                            .text("Rule source expanded state updated", "已更新规则源展开状态")
                            .clone_into(&mut this.status);
                        cx.notify();
                    }
                }));
            list = list.child(source_group);
        }

        if group_order.is_empty() {
            list = list.child(
                empty_state(
                    language.text("No routing rules yet", "还没有分流规则"),
                    language.text(
                        "Add rules to send matching connections through a policy group. Rules are evaluated from top to bottom.",
                        "添加规则，将匹配的连接交给指定策略组。规则会按从上到下的顺序生效。",
                    ),
                    None,
                    theme,
                )
                .mt(Space::Lg.px()),
            );
        }
        list
    }

    fn qx_rule_kind_label(kind: QxRuleKind) -> &'static str {
        match kind {
            QxRuleKind::Domain => "DOMAIN",
            QxRuleKind::DomainKeyword => "DOMAIN-KEYWORD",
            QxRuleKind::DomainSuffix => "DOMAIN-SUFFIX",
        }
    }

    fn routing_rule_row(
        order: usize,
        kind: &'static str,
        value: &str,
        target: &str,
        theme: Theme,
    ) -> Div {
        div()
            .mt_1()
            .min_h(px(44.0))
            .px(Space::Md.px())
            .py(Space::Sm.px())
            .rounded(Radius::Row.px())
            .bg(theme.surface_low)
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .child(
                div()
                    .w(px(42.0))
                    .flex_shrink_0()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_tertiary)
                    .child(format!("#{order:03}")),
            )
            .child(
                div()
                    .w(px(124.0))
                    .flex_shrink_0()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_secondary)
                    .child(kind),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Data.size())
                    .line_height(TextRole::Data.line_height())
                    .child(value.to_owned()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(TextRole::Data.size())
                    .line_height(TextRole::Data.line_height())
                    .font_weight(TextRole::Data.weight())
                    .text_color(theme.action_primary)
                    .child(target.to_owned()),
            )
    }

    fn qx_rule_import_feedback(&self, theme: Theme, language: Language) -> Div {
        let (message, color) = match &self.qx_rule_feedback {
            QxRuleImportFeedback::Idle => (
                language
                    .text(
                        "HTTPS only · Up to 1 MiB · Invalid lines are counted separately",
                        "只接受 HTTPS · 最多 1 MiB · 无效行会单独计数",
                    )
                    .to_owned(),
                theme.text_secondary,
            ),
            QxRuleImportFeedback::Importing => (
                language
                    .text(
                        "Securely downloading, parsing, and writing locally…",
                        "正在安全下载、解析并写入本机…",
                    )
                    .to_owned(),
                theme.action_primary,
            ),
            QxRuleImportFeedback::Imported {
                rule_count,
                diagnostic_count,
            } => {
                let message = if language == Language::English {
                    if *diagnostic_count == 0 {
                        format!("Imported {rule_count} rules")
                    } else {
                        format!(
                            "Imported {rule_count} rules · Skipped {diagnostic_count} invalid lines"
                        )
                    }
                } else if *diagnostic_count == 0 {
                    format!("已导入 {rule_count} 条规则")
                } else {
                    format!("已导入 {rule_count} 条规则 · 跳过 {diagnostic_count} 条无效行")
                };
                (message, theme.status_success)
            }
            QxRuleImportFeedback::AlreadyExists {
                rule_count,
                target_policy,
                ..
            } => (
                if language == Language::English {
                    format!(
                        "This rule source already exists · {rule_count} rules · Target {target_policy}. Manage or update the highlighted source below."
                    )
                } else {
                    format!(
                        "该规则源已存在 · {rule_count} 条规则 · 目标 {target_policy}。请在下方管理或更新已标出的规则源。"
                    )
                },
                theme.status_warning,
            ),
            QxRuleImportFeedback::InvalidDocument => (
                language
                    .text(
                        "File downloaded, but no recognizable QX domain rules were found",
                        "文件已下载，但没有可识别的 QX 域名规则",
                    )
                    .to_owned(),
                theme.status_error,
            ),
            QxRuleImportFeedback::DownloadFailed(error) => (error.to_string(), theme.status_error),
            QxRuleImportFeedback::StoreFailed(error) => (error.to_string(), theme.status_error),
        };
        div()
            .mt_2()
            .text_size(TextRole::Body.size())
            .line_height(TextRole::Body.line_height())
            .text_color(color)
            .child(message)
    }

    fn qx_rule_targets(&self) -> Vec<String> {
        let mut targets = self
            .managed_policy_groups
            .iter()
            .map(|group| group.name.clone())
            .collect::<Vec<_>>();
        targets.push("DIRECT".to_owned());
        targets
    }

    fn effective_rule_target(&self, target: &str, language: Language) -> String {
        if target != "Proxy"
            || self
                .managed_policy_groups
                .iter()
                .any(|group| group.name == target)
        {
            return target.to_owned();
        }
        self.managed_policy_groups.first().map_or_else(
            || language.text("Global exit", "全局出口").to_owned(),
            |group| group.name.clone(),
        )
    }

    fn submit_qx_rule_import(
        &mut self,
        input: &Entity<SubscriptionTextInput>,
        cx: &mut Context<Self>,
    ) -> bool {
        let url = input.read(cx).value().trim().to_owned();
        let target = self.qx_rule_target_policy.clone();
        let editing_id = self.qx_rule_editor_source_id.clone();
        let refresh_interval = self.qx_rule_editor_refresh_interval;
        let operation_id = begin_operation(
            "configuration.rule_source.save.requested",
            format!(
                "editing={} target={target} known_sources={}",
                editing_id.is_some(),
                self.qx_rule_sources.len()
            ),
        );
        let Ok(parsed_source) = SecretUrl::parse_https(&url) else {
            self.qx_rule_feedback =
                QxRuleImportFeedback::StoreFailed(SubscriptionStoreError::InvalidSource);
            self.language()
                .text(
                    "Enter a valid HTTPS rule URL",
                    "请输入有效的 HTTPS 规则地址",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return false;
        };
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            self.qx_rule_feedback =
                QxRuleImportFeedback::StoreFailed(SubscriptionStoreError::DataDirectoryUnavailable);
            self.language()
                .text(
                    "Could not determine where to save rules",
                    "无法确定规则保存位置",
                )
                .clone_into(&mut self.status);
            record_operation(
                operation_id,
                LogLevel::Error,
                "configuration.rule_source.add.failed",
                "phase=store reason=data_directory_unavailable",
            );
            cx.notify();
            return false;
        };
        if self.reject_duplicate_qx_rule_source(
            &parsed_source,
            editing_id.as_deref(),
            operation_id,
            cx,
        ) {
            return false;
        }
        self.qx_rule_import_generation = self.qx_rule_import_generation.wrapping_add(1);
        let generation = self.qx_rule_import_generation;
        self.qx_rule_feedback = QxRuleImportFeedback::Importing;
        self.language()
            .text("Downloading and parsing QX rules", "正在下载并解析 QX 规则")
            .clone_into(&mut self.status);
        input.update(cx, |input, cx| input.set_enabled(false, cx));
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    save_qx_rule_source(
                        &runtime,
                        &store_dir,
                        QxRuleSaveRequest {
                            url,
                            target,
                            editing_id,
                            refresh_interval,
                        },
                    )
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_qx_rule_import(generation, operation_id, result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
        true
    }

    fn reject_duplicate_qx_rule_source(
        &mut self,
        parsed_source: &SecretUrl,
        editing_id: Option<&str>,
        operation_id: u64,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((source_id, rule_count, stored_target)) = self
            .qx_rule_sources
            .iter()
            .find(|source| {
                source.source == *parsed_source && editing_id != Some(source.id.as_str())
            })
            .map(|source| {
                (
                    source.id.clone(),
                    source.rule_count,
                    source.target_policy.clone(),
                )
            })
        else {
            return false;
        };
        let target_policy = self.effective_rule_target(stored_target.as_str(), self.language());
        self.qx_rule_feedback = QxRuleImportFeedback::AlreadyExists {
            source_id: source_id.clone(),
            rule_count,
            target_policy: target_policy.clone(),
        };
        self.language()
            .text(
                "Rule source already exists; no duplicate was added",
                "规则源已存在，未重复添加",
            )
            .clone_into(&mut self.status);
        record_operation(
            operation_id,
            LogLevel::Warn,
            "configuration.rule_source.add.duplicate",
            format!("existing_id={source_id} rules={rule_count} target={target_policy}"),
        );
        cx.notify();
        true
    }

    fn finish_qx_rule_import(
        &mut self,
        generation: u64,
        operation_id: u64,
        result: super::QxRuleImportResult,
        cx: &mut Context<Self>,
    ) {
        if self.qx_rule_import_generation != generation {
            return;
        }
        if let Some(input) = self.qx_rule_input.as_ref() {
            input.update(cx, |input, cx| input.set_enabled(true, cx));
        }
        match result {
            Ok(ImportQxRuleSuccess::Imported { stored, apply }) => {
                self.finish_imported_qx_rule(operation_id, stored, &apply, cx);
            }
            Ok(ImportQxRuleSuccess::AlreadyExists { stored }) => {
                self.finish_existing_qx_rule(operation_id, stored);
            }
            Ok(ImportQxRuleSuccess::RolledBack {
                apply,
                rollback_error,
            }) => {
                self.qx_rule_feedback = QxRuleImportFeedback::Idle;
                self.status = apply
                    .status_suffix_after_rollback_attempt(self.language(), rollback_error.as_ref());
            }
            Err(error) => self.finish_failed_qx_rule_import(operation_id, &error),
        }
        cx.notify();
    }

    fn finish_imported_qx_rule(
        &mut self,
        operation_id: u64,
        stored: mihomo::StoredQxRuleSource,
        apply: &SourceRuntimeApply,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        let rule_count = stored.rule_count;
        let diagnostic_count = stored.diagnostic_count;
        let stored_id = stored.id.clone();
        let target_policy = self.effective_rule_target(stored.target_policy.as_str(), language);
        if let Some(existing) = self
            .qx_rule_sources
            .iter_mut()
            .find(|source| source.id == stored_id)
        {
            *existing = stored;
        } else {
            self.qx_rule_sources.push(stored);
        }
        self.persist_routing_rule_group_order();
        self.qx_rule_source_refreshes.remove(&stored_id);
        self.qx_rule_feedback = QxRuleImportFeedback::Imported {
            rule_count,
            diagnostic_count,
        };
        if let Some(input) = self.qx_rule_input.as_ref() {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        apply.reconcile_proxy_mode(&mut self.proxy_mode);
        self.status = if language == Language::English {
            format!(
                "QX rules imported · {rule_count} active rules{}",
                apply.status_suffix(language)
            )
        } else {
            format!(
                "QX 规则已导入 · {rule_count} 条生效{}",
                apply.status_suffix(language)
            )
        };
        record_operation(
            operation_id,
            LogLevel::Info,
            "configuration.rule_source.add.succeeded",
            format!(
                "id={stored_id} rules={rule_count} skipped={diagnostic_count} target={target_policy}"
            ),
        );
    }

    fn finish_existing_qx_rule(&mut self, operation_id: u64, stored: mihomo::StoredQxRuleSource) {
        let target_policy =
            self.effective_rule_target(stored.target_policy.as_str(), self.language());
        let source_id = stored.id.clone();
        let rule_count = stored.rule_count;
        if !self
            .qx_rule_sources
            .iter()
            .any(|source| source.id == source_id)
        {
            self.qx_rule_sources.push(stored);
        }
        self.persist_routing_rule_group_order();
        self.qx_rule_feedback = QxRuleImportFeedback::AlreadyExists {
            source_id: source_id.clone(),
            rule_count,
            target_policy: target_policy.clone(),
        };
        self.language()
            .text(
                "Rule source already exists; no duplicate was added",
                "规则源已存在，未重复添加",
            )
            .clone_into(&mut self.status);
        record_operation(
            operation_id,
            LogLevel::Warn,
            "configuration.rule_source.add.duplicate",
            format!("existing_id={source_id} rules={rule_count} target={target_policy}"),
        );
    }

    fn persist_routing_rule_group_order(&mut self) {
        self.sync_routing_rule_group_order();
        if let Some(store_dir) = self.subscription_store_dir.as_ref() {
            let _ =
                mihomo::save_routing_rule_group_order_in(store_dir, &self.routing_rule_group_order);
        }
    }

    fn finish_failed_qx_rule_import(&mut self, operation_id: u64, error: &ImportQxRuleError) {
        match error {
            ImportQxRuleError::Download(error) => {
                self.status = format!(
                    "{}: {error}",
                    self.language()
                        .text("QX rule download failed", "QX 规则下载失败")
                );
                self.qx_rule_feedback = QxRuleImportFeedback::DownloadFailed(*error);
                record_operation(
                    operation_id,
                    LogLevel::Error,
                    "configuration.rule_source.add.failed",
                    "phase=download",
                );
            }
            ImportQxRuleError::InvalidDocument => {
                self.qx_rule_feedback = QxRuleImportFeedback::InvalidDocument;
                self.language()
                    .text(
                        "QX rules not imported: no recognizable domain rules",
                        "QX 规则未导入：没有可识别的域名规则",
                    )
                    .clone_into(&mut self.status);
                record_operation(
                    operation_id,
                    LogLevel::Error,
                    "configuration.rule_source.add.failed",
                    "phase=parse reason=no_recognizable_domain_rules",
                );
            }
            ImportQxRuleError::Store(error) => {
                self.status = format!(
                    "{}: {error}",
                    self.language()
                        .text("QX rule save failed", "QX 规则保存失败")
                );
                self.qx_rule_feedback = QxRuleImportFeedback::StoreFailed(*error);
                record_operation(
                    operation_id,
                    LogLevel::Error,
                    "configuration.rule_source.add.failed",
                    "phase=store",
                );
            }
        }
    }

    fn remove_qx_rule_source(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            return;
        };
        self.qx_rule_import_generation = self.qx_rule_import_generation.wrapping_add(1);
        let generation = self.qx_rule_import_generation;
        self.qx_rule_feedback = QxRuleImportFeedback::Importing;
        self.language()
            .text("Removing remote QX rules", "正在移除远程 QX 规则")
            .clone_into(&mut self.status);
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    super::mutate_saved_sources(&runtime, &store_dir, || {
                        mihomo::remove_qx_rule_source_in(&store_dir, &id).map(|()| id.clone())
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                if this.qx_rule_import_generation != generation {
                    return;
                }
                match result {
                    Ok(transaction) if transaction.value.is_some() => {
                        let id = transaction.value.expect("checked committed mutation");
                        this.qx_rule_sources.retain(|source| source.id != id);
                        this.sync_routing_rule_group_order();
                        if let Some(store_dir) = this.subscription_store_dir.as_ref() {
                            let _ = mihomo::save_routing_rule_group_order_in(
                                store_dir,
                                &this.routing_rule_group_order,
                            );
                        }
                        this.qx_rule_source_refreshes.remove(&id);
                        this.source_refresh_retry_not_before
                            .remove(&super::DueRemoteSource::QxRule(id.clone()).scheduler_key());
                        this.qx_rule_feedback = QxRuleImportFeedback::Idle;
                        let language = this.language();
                        transaction.apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        this.status = if language == Language::English {
                            format!(
                                "Remote QX rules removed{}",
                                transaction.apply.status_suffix(language)
                            )
                        } else {
                            format!(
                                "远程 QX 规则已移除{}",
                                transaction.apply.status_suffix(language)
                            )
                        };
                    }
                    Ok(transaction) => {
                        this.qx_rule_feedback = QxRuleImportFeedback::StoreFailed(
                            SubscriptionStoreError::StoreUnavailable,
                        );
                        this.status = format!(
                            "{}{}",
                            this.language()
                                .text("Remote QX rule removal failed", "远程 QX 规则移除失败"),
                            transaction
                                .apply
                                .status_suffix_after_source_rollback(this.language())
                        );
                    }
                    Err(error) => {
                        this.qx_rule_feedback = QxRuleImportFeedback::StoreFailed(error);
                        this.status = format!(
                            "{}: {error}",
                            this.language()
                                .text("Remote QX rule removal failed", "远程 QX 规则移除失败")
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn set_subscription_enabled(&mut self, id: String, enabled: bool, cx: &mut Context<Self>) {
        if self.source_refresh_busy() {
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            return;
        };
        let Some(source) = self
            .imported_subscriptions
            .iter_mut()
            .find(|source| source.id == id)
        else {
            return;
        };
        let previous_state = source.state;
        let previous_enabled = source.enabled;
        let kind = super::source_kind(&source.source);
        self.subscription_preview_generation = self.subscription_preview_generation.wrapping_add(1);
        let generation = self.subscription_preview_generation;
        source.generation = generation;
        source.state = ImportedSubscriptionState::Refreshing(kind);
        self.language()
            .text("Applying subscription state", "正在应用订阅状态")
            .clone_into(&mut self.status);
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        let task_id = id.clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    super::mutate_saved_sources(&runtime, &store_dir, || {
                        mihomo::update_subscription_source_enabled_in(&store_dir, &task_id, enabled)
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                let language = this.language();
                let mut refresh_after_enable = false;
                let Some(source) = this
                    .imported_subscriptions
                    .iter_mut()
                    .find(|source| source.id == id)
                else {
                    return;
                };
                if source.generation != generation {
                    return;
                }
                match result {
                    Ok(transaction) if transaction.value.is_some() => {
                        let stored = transaction.value.expect("checked committed mutation");
                        source.enabled = stored.enabled;
                        source.state = if stored.enabled {
                            refresh_after_enable = true;
                            ImportedSubscriptionState::Pending(kind)
                        } else {
                            ImportedSubscriptionState::None
                        };
                        transaction.apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        this.status = format!(
                            "{}{}",
                            if stored.enabled {
                                language.text("Subscription enabled", "订阅已启用")
                            } else {
                                language.text("Subscription disabled", "订阅已停用")
                            },
                            transaction.apply.status_suffix(language)
                        );
                    }
                    Ok(transaction) => {
                        source.enabled = previous_enabled;
                        source.state = previous_state;
                        this.status = format!(
                            "{}{}",
                            language
                                .text("Failed to change subscription state", "订阅状态修改失败"),
                            transaction
                                .apply
                                .status_suffix_after_source_rollback(language)
                        );
                    }
                    Err(error) => {
                        source.enabled = previous_enabled;
                        source.state = previous_state;
                        this.status = format!(
                            "{}: {error}",
                            language
                                .text("Failed to change subscription state", "订阅状态修改失败")
                        );
                    }
                }
                if refresh_after_enable {
                    this.refresh_imported_subscription(id.clone(), cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn set_qx_rule_source_enabled(&mut self, id: String, enabled: bool, cx: &mut Context<Self>) {
        if self.source_refresh_busy() {
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            return;
        };
        self.qx_rule_import_generation = self.qx_rule_import_generation.wrapping_add(1);
        let generation = self.qx_rule_import_generation;
        self.qx_rule_source_target_updates
            .insert(id.clone(), generation);
        self.language()
            .text("Applying rule source state", "正在应用规则来源状态")
            .clone_into(&mut self.status);
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        let task_id = id.clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    super::mutate_saved_sources(&runtime, &store_dir, || {
                        mihomo::update_qx_rule_source_enabled_in(&store_dir, &task_id, enabled)
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                if this.qx_rule_source_target_updates.get(&id) != Some(&generation) {
                    return;
                }
                this.qx_rule_source_target_updates.remove(&id);
                match result {
                    Ok(transaction) if transaction.value.is_some() => {
                        let stored = transaction.value.expect("checked committed mutation");
                        let language = this.language();
                        let enabled = stored.enabled;
                        if let Some(source) = this
                            .qx_rule_sources
                            .iter_mut()
                            .find(|source| source.id == id)
                        {
                            *source = stored;
                        }
                        transaction.apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        this.status = format!(
                            "{}{}",
                            if enabled {
                                language.text("Rule source enabled", "规则来源已启用")
                            } else {
                                language.text("Rule source disabled", "规则来源已停用")
                            },
                            transaction.apply.status_suffix(language)
                        );
                    }
                    Ok(transaction) => {
                        this.status = format!(
                            "{}{}",
                            this.language()
                                .text("Failed to change rule source state", "规则来源状态修改失败"),
                            transaction
                                .apply
                                .status_suffix_after_source_rollback(this.language())
                        );
                    }
                    Err(error) => {
                        this.status = format!(
                            "{}: {error}",
                            this.language()
                                .text("Failed to change rule source state", "规则来源状态修改失败")
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn update_qx_rule_source_target(&mut self, id: String, target: String, cx: &mut Context<Self>) {
        if self.source_refresh_busy() || !self.qx_rule_targets().contains(&target) {
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            self.language()
                .text(
                    "Could not determine where to save the rule source",
                    "无法确定规则源的保存位置",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        };
        let Some(source) = self.qx_rule_sources.iter().find(|source| source.id == id) else {
            return;
        };
        if self.effective_rule_target(source.target_policy.as_str(), self.language()) == target {
            self.qx_rule_target_popover = None;
            cx.notify();
            return;
        }

        self.qx_rule_import_generation = self.qx_rule_import_generation.wrapping_add(1);
        let generation = self.qx_rule_import_generation;
        self.qx_rule_source_target_updates
            .insert(id.clone(), generation);
        self.qx_rule_target_popover = None;
        self.status = format!(
            "{} {target}",
            self.language()
                .text("Saving rule source policy", "正在保存规则源策略")
        );
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        let task_id = id.clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    super::mutate_saved_sources(&runtime, &store_dir, || {
                        mihomo::update_qx_rule_source_target_in(&store_dir, &task_id, &target)
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                if this.qx_rule_source_target_updates.get(&id) != Some(&generation) {
                    return;
                }
                this.qx_rule_source_target_updates.remove(&id);
                match result {
                    Ok(transaction) if transaction.value.is_some() => {
                        let stored = transaction.value.expect("checked committed mutation");
                        let language = this.language();
                        let target = stored.target_policy.as_str().to_owned();
                        if let Some(source) = this
                            .qx_rule_sources
                            .iter_mut()
                            .find(|source| source.id == id)
                        {
                            *source = stored;
                        }
                        transaction.apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        this.status = format!(
                            "{} {target}{}",
                            language.text("Rule source policy set to", "规则源策略已设为"),
                            transaction.apply.status_suffix(language)
                        );
                        record_event(
                            LogLevel::Info,
                            "routing.rule_source.target.updated",
                            format!("source_id={id} target={target}"),
                        );
                    }
                    Ok(transaction) => {
                        this.status = format!(
                            "{}{}",
                            this.language()
                                .text("Failed to save rule source policy", "规则源策略保存失败"),
                            transaction
                                .apply
                                .status_suffix_after_source_rollback(this.language())
                        );
                    }
                    Err(error) => {
                        this.status = format!(
                            "{}: {error}",
                            this.language()
                                .text("Failed to save rule source policy", "规则源策略保存失败")
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(super) fn refresh_qx_rule_source(&mut self, id: String, cx: &mut Context<Self>) {
        if self.source_refresh_busy() {
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            return;
        };
        let Some(source) = self.qx_rule_sources.iter().find(|source| source.id == id) else {
            return;
        };
        let url = source.source.clone();
        self.qx_rule_import_generation = self.qx_rule_import_generation.wrapping_add(1);
        let generation = self.qx_rule_import_generation;
        self.qx_rule_source_refreshes.insert(
            id.clone(),
            QxRuleSourceRefreshState::Refreshing { generation },
        );
        self.language()
            .text("Updating remote QX rules", "正在更新远程 QX 规则")
            .clone_into(&mut self.status);
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        let task_id = id.clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    let content = download_qx_rule_document_secret(&url)
                        .map_err(ImportQxRuleError::Download)?;
                    let parsed = QxRuleList::parse(&content);
                    if parsed.rules.is_empty() {
                        return Err(ImportQxRuleError::InvalidDocument);
                    }
                    let transaction = super::mutate_saved_sources(&runtime, &store_dir, || {
                        mihomo::replace_qx_rule_source_content_in(
                            &store_dir,
                            &task_id,
                            &content,
                            mihomo::current_unix_secs(),
                        )
                    })
                    .map_err(ImportQxRuleError::Store)?;
                    Ok::<_, ImportQxRuleError>(transaction)
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_qx_rule_source_refresh(&id, generation, result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn finish_qx_rule_source_refresh(
        &mut self,
        id: &str,
        generation: u64,
        result: super::QxRuleRefreshResult,
        cx: &mut Context<Self>,
    ) {
        if !matches!(
            self.qx_rule_source_refreshes.get(id),
            Some(QxRuleSourceRefreshState::Refreshing { generation: active })
                if *active == generation
        ) {
            return;
        }
        match result {
            Ok(transaction) if transaction.value.is_some() => {
                self.finish_successful_qx_rule_refresh(id, transaction);
            }
            Err(error) => self.finish_failed_qx_rule_refresh(id, generation, &error),
            Ok(transaction) => {
                let message = "runtime apply failed".to_owned();
                self.qx_rule_source_refreshes.insert(
                    id.to_owned(),
                    QxRuleSourceRefreshState::Failed {
                        generation,
                        message,
                    },
                );
                self.status = format!(
                    "{}{}",
                    self.language()
                        .text("Remote QX rule update failed", "远程 QX 规则更新失败"),
                    transaction
                        .apply
                        .status_suffix_after_source_rollback(self.language())
                );
            }
        }
        cx.notify();
    }

    fn finish_successful_qx_rule_refresh(
        &mut self,
        id: &str,
        mut transaction: super::SourceMutation<mihomo::StoredQxRuleSource>,
    ) {
        let stored = transaction
            .value
            .take()
            .expect("checked committed mutation");
        let language = self.language();
        let rule_count = stored.rule_count;
        if let Some(source) = self
            .qx_rule_sources
            .iter_mut()
            .find(|source| source.id == id)
        {
            *source = stored;
        }
        self.qx_rule_source_refreshes.remove(id);
        self.source_refresh_retry_not_before
            .remove(&super::DueRemoteSource::QxRule(id.to_owned()).scheduler_key());
        transaction.apply.reconcile_proxy_mode(&mut self.proxy_mode);
        self.status = if language == Language::English {
            format!(
                "QX rules updated · {rule_count} active rules{}",
                transaction.apply.status_suffix(language)
            )
        } else {
            format!(
                "QX 规则更新完成 · {rule_count} 条生效{}",
                transaction.apply.status_suffix(language)
            )
        };
    }

    fn finish_failed_qx_rule_refresh(
        &mut self,
        id: &str,
        generation: u64,
        error: &ImportQxRuleError,
    ) {
        let message = match error {
            ImportQxRuleError::Download(error) => error.to_string(),
            ImportQxRuleError::InvalidDocument => self
                .language()
                .text("No recognizable domain rules", "没有可识别的域名规则")
                .to_owned(),
            ImportQxRuleError::Store(error) => error.to_string(),
        };
        self.qx_rule_source_refreshes.insert(
            id.to_owned(),
            QxRuleSourceRefreshState::Failed {
                generation,
                message: message.clone(),
            },
        );
        self.status = format!(
            "{}: {message}",
            self.language()
                .text("QX rule update failed", "QX 规则更新失败")
        );
    }
}

#[cfg(test)]
mod tests {
    use manis_core::NodeWorkspaceState;

    use super::{
        Language, MANUAL_RULES_EXPANSION_KEY, ManualRuleKeyboardAction,
        manual_rule_keyboard_action_for, rule_group_is_open, rule_source_expansion_key,
        source_update_label,
    };

    #[test]
    fn remote_rule_sources_start_open_and_remember_collapse() {
        let mut workspace = NodeWorkspaceState::default();
        let key = rule_source_expansion_key("qx-rule-deadbeef");

        assert!(!workspace.is_group_collapsed(&key));
        assert!(rule_group_is_open(&workspace, &key));
        workspace.toggle_group(&key);
        assert!(workspace.is_group_collapsed(&key));
        assert!(!rule_group_is_open(&workspace, &key));
        assert_eq!(key, "routing-rule-source:qx-rule-deadbeef");
    }

    #[test]
    fn manual_rules_use_the_same_collapsible_group_state() {
        let mut workspace = NodeWorkspaceState::default();

        assert!(!workspace.is_group_collapsed(MANUAL_RULES_EXPANSION_KEY));
        assert!(rule_group_is_open(&workspace, MANUAL_RULES_EXPANSION_KEY));
        workspace.toggle_group(MANUAL_RULES_EXPANSION_KEY);
        assert!(workspace.is_group_collapsed(MANUAL_RULES_EXPANSION_KEY));
        assert!(!rule_group_is_open(&workspace, MANUAL_RULES_EXPANSION_KEY));
    }

    #[test]
    fn manual_rule_rows_expose_keyboard_edit_toggle_and_delete_actions() {
        assert_eq!(
            manual_rule_keyboard_action_for("enter", false, false),
            Some(ManualRuleKeyboardAction::Edit)
        );
        assert_eq!(
            manual_rule_keyboard_action_for("space", false, false),
            Some(ManualRuleKeyboardAction::Toggle)
        );
        assert_eq!(
            manual_rule_keyboard_action_for("delete", false, false),
            Some(ManualRuleKeyboardAction::Delete)
        );
        assert_eq!(manual_rule_keyboard_action_for("enter", true, false), None);
        assert_eq!(manual_rule_keyboard_action_for("delete", false, true), None);
    }

    #[test]
    fn source_update_time_is_compact_and_handles_clock_rollback() {
        assert_eq!(
            source_update_label(0, 10_000, Language::SimplifiedChinese),
            "从未更新"
        );
        assert_eq!(
            source_update_label(9_980, 10_000, Language::SimplifiedChinese),
            "刚刚更新"
        );
        assert_eq!(
            source_update_label(6_400, 10_000, Language::SimplifiedChinese),
            "1 小时前更新"
        );
        assert_eq!(
            source_update_label(10_100, 10_000, Language::SimplifiedChinese),
            "刚刚更新"
        );
        assert_eq!(
            source_update_label(6_400, 10_000, Language::English),
            "Updated 1 hr ago"
        );
    }
}
