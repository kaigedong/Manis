use gpui::{
    AnyElement, Context, Div, Entity, Focusable, FontWeight, ParentElement, Role, Stateful,
    StyleRefinement, Styled, Window, div, prelude::*, px,
};
use gpui_component::{
    Disableable, IconName, Selectable, Sizable, Size, WindowExt as _,
    accordion::Accordion,
    button::{Button, ButtonVariant, ButtonVariants},
    dialog::Dialog,
};
use manis_core::{KernelKind, WindowSizeClass};
use manis_profile::{QxRuleKind, SecretUrl};

use super::{
    ImportQxRuleError, ImportQxRuleSuccess, ImportedSubscriptionState, ManisApp, ManualRulePopover,
    QxRuleImportFeedback, QxRuleList, QxRuleSourceRefreshState, SourceRuntimeApply,
    SubscriptionFeedback,
};
use crate::{
    diagnostics::{LogLevel, UiEvent, begin_operation, record_event, record_operation, trace_ui},
    localization::{Language, LanguagePreference, save_language_preference_in},
    mihomo::{self, LoadedProvider, RemoteSourceRefreshInterval, SubscriptionStoreError},
    rule_source::{download_qx_rule_document, download_qx_rule_document_secret},
    subscription::{SourceKind, SubscriptionPreview, validate_subscription_preview},
    subscription_input::SubscriptionTextInput,
    theme::Theme,
};

const MAX_MANUAL_RULE_INPUT_BYTES: usize = 1_024;
const MANUAL_RULES_EXPANSION_KEY: &str = "routing-manual-rules";

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
    }
}

fn source_kind_label(kind: SourceKind, language: Language) -> &'static str {
    match kind {
        SourceKind::HttpSubscription => language.text("HTTP subscription", "HTTP 订阅"),
        SourceKind::HttpsSubscription => language.text("HTTPS subscription", "HTTPS 订阅"),
        SourceKind::VlessNode => language.text("VLESS node", "VLESS 节点"),
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

fn language_preference_detail(preference: LanguagePreference, language: Language) -> &'static str {
    match preference {
        LanguagePreference::FollowSystem => language.text(
            "Use Chinese for Chinese system locales; otherwise English.",
            "系统语言为中文时使用中文，否则使用英文。",
        ),
        LanguagePreference::English => language.text("Always use English.", "始终使用英文。"),
        LanguagePreference::SimplifiedChinese => {
            language.text("Always use Simplified Chinese.", "始终使用简体中文。")
        }
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
        div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.surface_base)
            .child(Self::workspace_header(
                language.text("Settings", "配置"),
                language.text(
                    "Manage sources only; nodes and active rules live in their own pages",
                    "只管理来源；节点与最终生效规则在各自页面查看",
                ),
                language.text("Private", "本机私有"),
                theme,
                compact,
            ))
            .child(
                div()
                    .id("configuration-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .p(if compact { px(12.0) } else { px(20.0) })
                    .pb(px(56.0))
                    .child(
                        div()
                            .w_full()
                            .max_w(px(880.0))
                            .mx_auto()
                            .child(self.language_panel(theme, compact, cx))
                            .child(self.kernel_panel(theme, compact, cx).mt_4())
                            .child(self.source_panel(theme, compact, cx).mt_4())
                            .child(
                                self.rule_source_manager(rule_input, rule_busy, theme, cx)
                                    .mt_4(),
                            ),
                    ),
            )
    }

    fn language_panel(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let language = self.language();
        let current_preference = self.language_preference();
        let current_language = language.display_name();
        div()
            .id("configuration-language")
            .w_full()
            .p(if compact { px(12.0) } else { px(16.0) })
            .rounded_md()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .min_w(px(0.0))
                            .child(
                                div()
                                    .text_size(px(17.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child(language.text("Interface language", "界面语言")),
                            )
                            .child(
                                div()
                                    .mt_1()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_secondary)
                                    .child(language.text(
                                        "Follows the operating system by default and falls back to English.",
                                        "默认跟随系统语言，检测不到时回退到英文。",
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .bg(theme.action_soft)
                            .text_size(px(10.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.action_primary)
                            .child(format!(
                                "{} · {current_language}",
                                language.text("Current", "当前")
                            )),
                    ),
            )
            .child(
                div()
                    .mt_3()
                    .grid()
                    .gap_2()
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
            .min_h(px(68.0))
            .p_3()
            .rounded_md()
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
                            .font_weight(FontWeight::SEMIBOLD)
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
                                .text_size(px(10.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.action_primary)
                                .child(language.text("Selected", "已选择")),
                        )
                    }),
            )
            .child(
                div()
                    .mt_2()
                    .text_size(px(10.0))
                    .text_color(theme.text_secondary)
                    .child(language_preference_detail(preference, language)),
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

    #[allow(clippy::too_many_lines)]
    fn kernel_panel(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let language = self.language();
        let active = self.runtime.kind();
        let capabilities = self.runtime.capabilities();
        let sing_box_installed = mihomo::sing_box_binary_available();
        let sing_box_has_sources =
            self.imported_subscriptions.is_empty() && !self.saved_vless_nodes.is_empty();
        let sing_box_reason = if !sing_box_installed {
            language.text(
                "sing-box was not found on this device",
                "本机未检测到 sing-box",
            )
        } else if !self.imported_subscriptions.is_empty() {
            language.text(
                "Clash subscriptions are present; Manis needs its native parser first",
                "当前包含 Clash 订阅，需等待 Manis 原生订阅解析器",
            )
        } else if self.saved_vless_nodes.is_empty() {
            language.text(
                "At least one saved VLESS node is required",
                "至少需要一个已保存的 VLESS 节点",
            )
        } else {
            language.text(
                "Supports manual VLESS, selectors, URL tests, and routing rules",
                "支持手动 VLESS、选择器、自动测速与分流规则",
            )
        };
        let sing_box_enabled = sing_box_installed
            && sing_box_has_sources
            && !self.kernel_switch_state.is_busy()
            && active != KernelKind::SingBox;

        div()
            .id("configuration-kernel")
            .w_full()
            .p(if compact { px(12.0) } else { px(16.0) })
            .rounded_md()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .min_w(px(0.0))
                            .child(
                                div()
                                    .text_size(px(17.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child(language.text("Runtime kernel", "运行内核")),
                            )
                            .child(
                                div()
                                    .mt_1()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_secondary)
                                    .child(language.text(
                                        "Manis compiles and validates the target config before switching; failures keep the current kernel.",
                                        "切换前先编译并校验目标配置；失败不会替换当前内核。",
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .bg(theme.action_soft)
                            .text_size(px(10.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.action_primary)
                            .child(if self.kernel_switch_state.is_busy() {
                                language.text("Validating", "正在校验")
                            } else {
                                active.display_name()
                            }),
                    ),
            )
            .child(Self::kernel_option_row(
                KernelKind::Mihomo,
                language.text(
                    "Fully supports current subscriptions, policy groups, latency tests, and Clash API.",
                    "完整兼容当前订阅、策略组、测速与 Clash API",
                ),
                !self.kernel_switch_state.is_busy() && active != KernelKind::Mihomo,
                active == KernelKind::Mihomo,
                language,
                theme,
                cx,
            ))
            .child(Self::kernel_option_row(
                KernelKind::SingBox,
                sing_box_reason,
                sing_box_enabled,
                active == KernelKind::SingBox,
                language,
                theme,
                cx,
            ))
            .child(
                div()
                    .mt_3()
                    .pt_3()
                    .border_t_1()
                    .border_color(theme.outline_subtle)
                    .text_size(px(10.0))
                    .text_color(theme.text_tertiary)
                    .child(if language == Language::English {
                        format!(
                            "Current capability · Subscriptions {} · Manual VLESS {} · URL test {} · Selection saved locally in kernel.kind",
                            if capabilities.subscription_providers { "available" } else { "not available" },
                            if capabilities.manual_vless { "available" } else { "not available" },
                            if capabilities.url_test { "available" } else { "not available" },
                        )
                    } else {
                        format!(
                            "当前能力 · 订阅{} · 手动 VLESS{} · 自动测速{} · 选择保存在本机 kernel.kind",
                            if capabilities.subscription_providers { "可用" } else { "暂不可用" },
                            if capabilities.manual_vless { "可用" } else { "暂不可用" },
                            if capabilities.url_test { "可用" } else { "暂不可用" },
                        )
                    }),
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
            .mt_3()
            .pt_3()
            .border_t_1()
            .border_color(theme.outline_subtle)
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(kind.display_name()),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(11.0))
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
                    .h(px(34.0))
                    .px_3()
                    .rounded_md()
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
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
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
                language.text("Routing rules", "分流规则"),
                language.text(
                    "Inspect the ordered rules that actually participate in matching; manage sources in Settings",
                    "查看最终参与匹配的有序规则；来源请前往配置页管理",
                ),
                language.text("Top-down", "从上到下匹配"),
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
        theme: Theme,
        compact: bool,
    ) -> Div {
        div()
            .px(if compact { px(12.0) } else { px(20.0) })
            .py_3()
            .flex_shrink_0()
            .border_b_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .flex()
            .items_start()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(px(19.0))
                            .font_weight(FontWeight::BOLD)
                            .child(title),
                    )
                    .when(!compact, |header| {
                        header.child(div().mt_1().text_color(theme.text_secondary).child(detail))
                    }),
            )
            .child(
                div()
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .bg(theme.action_soft)
                    .text_size(px(10.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.action_primary)
                    .child(badge),
            )
    }

    #[allow(clippy::too_many_lines)]
    fn source_panel(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Div {
        let language = self.language();
        let input = self
            .subscription_input
            .as_ref()
            .expect("subscription input is initialized before rendering")
            .clone();
        let feedback = &self.subscription_feedback;
        let has_imported_subscription = !self.imported_subscriptions.is_empty();
        let saved_source_count = self.imported_subscriptions.len() + self.saved_vless_nodes.len();
        let direct_input = input.read(cx).value().starts_with("vless://");
        let busy = matches!(feedback, SubscriptionFeedback::Importing(_))
            || self.imported_subscriptions.iter().any(|subscription| {
                matches!(subscription.state, ImportedSubscriptionState::Removing(_))
            })
            || self.source_refresh_busy();

        let panel = div()
            .id("configuration-source")
            .w_full()
            .min_h(px(330.0))
            .p(if compact { px(12.0) } else { px(16.0) })
            .rounded_md()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .child(
                div()
                    .text_size(px(17.0))
                    .font_weight(FontWeight::BOLD)
                    .child(language.text("Add proxy source", "添加代理来源")),
            )
            .child(
                div()
                    .mt_1()
                    .text_size(px(11.0))
                    .text_color(theme.text_secondary)
                    .child(if compact {
                        language.text(
                            "HTTP/HTTPS subscription or one vless:// node",
                            "HTTP/HTTPS 订阅或单个 vless:// 节点",
                        )
                    } else {
                        language.text(
                            "Supports HTTP/HTTPS subscriptions and single vless:// node links.",
                            "支持 HTTP/HTTPS 订阅，也支持单个 vless:// 节点链接。",
                        )
                    }),
            )
            .child(
                div()
                    .mt(if compact { px(10.0) } else { px(16.0) })
                    .mb_1()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(language.text("Source address", "来源地址")),
            )
            .child(input.clone())
            .child(Self::subscription_actions(
                input,
                busy,
                direct_input,
                language,
                theme,
                cx,
            ))
            .child(Self::subscription_feedback(
                feedback,
                &self.subscription_preview_providers,
                has_imported_subscription,
                language,
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
                    .mt(if compact { px(14.0) } else { px(20.0) })
                    .pt(if compact { px(12.0) } else { px(16.0) })
                    .border_t_1()
                    .border_color(theme.outline_subtle)
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(language.text("Saved sources", "已保存来源")),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.text_tertiary)
                            .child(if language == Language::English {
                                format!("{saved_source_count} total")
                            } else {
                                format!("{saved_source_count} 个")
                            }),
                    ),
            )
            .when(saved_source_count == 0, |panel| {
                panel.child(
                    div()
                        .mt_3()
                        .p_3()
                        .rounded_md()
                        .bg(theme.surface_low)
                        .text_size(px(11.0))
                        .text_color(theme.text_secondary)
                        .child(language.text(
                            "No proxy sources yet. Nodes will appear on the Nodes page after adding one.",
                            "还没有代理来源。添加后可在节点页查看其中的节点。",
                        )),
                )
            })
            .child(self.imported_subscription_cards(theme, cx))
            .child(self.saved_vless_cards(theme, cx))
            .child(self.source_panel_footer(theme, compact));
        div().w_full().child(panel)
    }

    fn source_panel_footer(&self, theme: Theme, compact: bool) -> Div {
        let language = self.language();
        let source = self.runtime.profile_source();
        div()
            .mt_3()
            .pt_3()
            .border_t_1()
            .border_color(theme.outline_subtle)
            .text_size(px(10.0))
            .text_color(theme.text_tertiary)
            .child(language.text(
                "Links are saved in a private local directory; the UI and logs never show full addresses.",
                "链接保存在本机私有目录；界面和日志不会显示完整地址。",
            ))
            .when(!compact, |footer| {
                footer.child(if language == Language::English {
                    div().mt_1().child(format!(
                        "Current apply mode · {} · {}",
                        source.label(),
                        source.detail()
                    ))
                } else {
                    div().mt_1().child(format!(
                        "当前应用方式 · {} · {}",
                        source.label(),
                        source.detail()
                    ))
                })
            })
    }

    fn subscription_actions(
        input: Entity<SubscriptionTextInput>,
        busy: bool,
        direct_input: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let clear_input = input.clone();
        div()
            .mt_2()
            .flex()
            .gap_2()
            .child(
                Button::new("subscription-preview")
                    .accessibility_label(if direct_input {
                        language.text("Save VLESS node", "保存 VLESS 节点")
                    } else {
                        language.text("Validate and import subscription", "验证并导入订阅")
                    })
                    .label(if busy {
                        language.text("Processing…", "正在处理…")
                    } else if direct_input {
                        language.text("Save VLESS node", "保存 VLESS 节点")
                    } else {
                        language.text("Import subscription", "导入订阅")
                    })
                    .loading(busy)
                    .with_variant(ButtonVariant::Primary)
                    .with_size(px(36.0))
                    .when(!busy, gpui::Styled::cursor_pointer)
                    .h(px(36.0))
                    .px_3()
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
                    .font_weight(FontWeight::SEMIBOLD)
                    .flex_1()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if busy {
                            return;
                        }
                        this.submit_source_import(&input, cx);
                    })),
            )
            .child(
                Button::new("subscription-clear")
                    .accessibility_label(
                        language.text("Clear subscription link draft", "清除订阅链接草稿"),
                    )
                    .label(language.text("Clear", "清除"))
                    .loading(busy)
                    .with_variant(ButtonVariant::Default)
                    .with_size(px(36.0))
                    .when(!busy, gpui::Styled::cursor_pointer)
                    .h(px(36.0))
                    .px_3()
                    .border_color(theme.outline_subtle)
                    .bg(theme.surface_high)
                    .text_color(theme.text_secondary)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if busy {
                            return;
                        }
                        clear_input.update(cx, SubscriptionTextInput::clear);
                        this.subscription_feedback = SubscriptionFeedback::Idle;
                        this.language()
                            .text("Subscription link draft cleared", "已清除订阅链接草稿")
                            .clone_into(&mut this.status);
                        trace_ui(UiEvent::SubscriptionDraftCleared);
                        cx.notify();
                    })),
            )
    }

    #[allow(clippy::too_many_lines)]
    fn submit_source_import(
        &mut self,
        input: &Entity<SubscriptionTextInput>,
        cx: &mut Context<Self>,
    ) {
        let (input_value, result) = {
            let input = input.read(cx);
            (
                input.value().to_owned(),
                validate_subscription_preview(input.value()),
            )
        };
        match result {
            Ok(preview) if preview.kind == SourceKind::VlessNode => {
                let Some(store_dir) = self.subscription_store_dir.clone() else {
                    self.subscription_feedback = SubscriptionFeedback::StoreFailed(
                        SubscriptionStoreError::DataDirectoryUnavailable,
                    );
                    self.language()
                        .text(
                            "Could not determine where to save the node",
                            "无法确定节点保存位置",
                        )
                        .clone_into(&mut self.status);
                    trace_ui(UiEvent::SourceImportFailed);
                    cx.notify();
                    return;
                };
                self.subscription_preview_generation =
                    self.subscription_preview_generation.wrapping_add(1);
                let generation = self.subscription_preview_generation;
                self.subscription_feedback = SubscriptionFeedback::Importing(SourceKind::VlessNode);
                self.language()
                    .text(
                        "Saving and compiling VLESS node",
                        "正在保存并编译 VLESS 节点",
                    )
                    .clone_into(&mut self.status);
                if let Some(input) = self.subscription_input.as_ref() {
                    input.update(cx, |input, cx| input.set_enabled(false, cx));
                }
                let runtime = self.runtime.clone();
                let executor = cx.background_executor().clone();
                cx.spawn(async move |this, cx| {
                    let result = executor
                        .spawn(async move {
                            let stored = mihomo::save_vless_source_in(&store_dir, &input_value)?;
                            let apply = SourceRuntimeApply::from_result(
                                runtime.apply_saved_sources(&store_dir),
                            );
                            Ok::<_, SubscriptionStoreError>((stored, apply))
                        })
                        .await;
                    this.update(cx, |this, cx| {
                        if this.subscription_preview_generation != generation {
                            return;
                        }
                        if let Some(input) = this.subscription_input.as_ref() {
                            input.update(cx, |input, cx| input.set_enabled(true, cx));
                        }
                        match result {
                            Ok((stored, apply)) => {
                                let language = this.language();
                                if !this
                                    .saved_vless_nodes
                                    .iter()
                                    .any(|node| node.id == stored.id)
                                {
                                    this.saved_vless_nodes.push(stored);
                                }
                                this.subscription_feedback = SubscriptionFeedback::Valid(preview);
                                if let Some(input) = this.subscription_input.as_ref() {
                                    input.update(cx, SubscriptionTextInput::clear_without_event);
                                }
                                apply.reconcile_proxy_mode(&mut this.proxy_mode);
                                this.status = if language == Language::English {
                                    format!(
                                        "VLESS node saved · Added to Saved group{}",
                                        apply.status_suffix(language)
                                    )
                                } else {
                                    format!(
                                        "VLESS 节点已保存 · 已加入“已保存”分组{}",
                                        apply.status_suffix(language)
                                    )
                                };
                                trace_ui(UiEvent::SourceImportSucceeded);
                            }
                            Err(error) => {
                                this.subscription_feedback =
                                    SubscriptionFeedback::StoreFailed(error);
                                this.status = format!(
                                    "{}: {error}",
                                    this.language()
                                        .text("VLESS node save failed", "VLESS 节点保存失败")
                                );
                                trace_ui(UiEvent::SourceImportFailed);
                            }
                        }
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
                cx.notify();
            }
            Ok(preview) => {
                trace_ui(UiEvent::SourceRecognitionSucceeded);
                self.import_remote_subscription(input_value, preview.kind, cx);
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
            }
        }
    }

    fn subscription_feedback(
        feedback: &SubscriptionFeedback,
        providers: &[LoadedProvider],
        has_imported_subscription: bool,
        language: Language,
        theme: Theme,
    ) -> Div {
        match feedback {
            SubscriptionFeedback::Idle => div()
                .mt_3()
                .text_size(px(11.0))
                .text_color(theme.text_secondary)
                .child(if has_imported_subscription {
                    language.text(
                        "You can keep adding HTTP/HTTPS subscriptions or save one vless:// node.",
                        "可继续添加 HTTP/HTTPS 订阅，或保存单个 vless:// 节点",
                    )
                } else {
                    language.text(
                        "Waiting for input · HTTP/HTTPS subscription or vless:// node",
                        "等待输入 · HTTP/HTTPS 订阅或 vless:// 节点",
                    )
                }),
            SubscriptionFeedback::Importing(kind) => {
                Self::subscription_loading(*kind, language, theme)
            }
            SubscriptionFeedback::Valid(preview) => {
                Self::subscription_valid(preview, providers, language, theme)
            }
            SubscriptionFeedback::InvalidInput(error) => Self::subscription_error(
                language.text("Could not recognize source", "无法识别来源"),
                error.to_string(),
                None,
                theme,
            ),
            SubscriptionFeedback::PreviewFailed(error) => Self::subscription_error(
                language.text("Could not read subscription nodes", "无法读取订阅节点"),
                error.to_string(),
                Some(language.text(
                    "The link is still in the input; check it and retry.",
                    "链接仍保留在输入框中；检查后可再次读取。",
                )),
                theme,
            ),
            SubscriptionFeedback::StoreFailed(error) => Self::subscription_error(
                language.text(
                    "Source is valid, but could not be saved",
                    "来源有效，但无法保存",
                ),
                error.to_string(),
                Some(language.text(
                    "Existing sources are unchanged; check directory permissions and retry.",
                    "现有来源未受影响；检查目录权限后重试。",
                )),
                theme,
            ),
        }
    }

    fn subscription_loading(kind: SourceKind, language: Language, theme: Theme) -> Div {
        let title = match kind {
            SourceKind::HttpSubscription => language.text(
                "Validating and importing HTTP subscription",
                "正在验证并导入 HTTP 订阅",
            ),
            SourceKind::HttpsSubscription => language.text(
                "Validating and importing HTTPS subscription",
                "正在验证并导入 HTTPS 订阅",
            ),
            SourceKind::VlessNode => language.text("Saving VLESS node", "正在保存 VLESS 节点"),
        };
        div()
            .mt_3()
            .p_3()
            .rounded_md()
            .bg(theme.action_soft)
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.action_primary)
                    .child(title),
            )
            .child(
                div()
                    .mt_1()
                    .text_size(px(11.0))
                    .text_color(theme.text_secondary)
                    .child(language.text(
                        "An isolated Mihomo process is parsing nodes; Manis only saves after success.",
                        "隔离的 Mihomo 正在解析节点；成功后才会原子保存。",
                    )),
            )
    }

    fn subscription_valid(
        preview: &SubscriptionPreview,
        providers: &[LoadedProvider],
        language: Language,
        theme: Theme,
    ) -> Div {
        let (title, detail) = match preview.kind {
            SourceKind::HttpSubscription => (
                language.text("HTTP subscription imported", "HTTP 订阅预览完成"),
                language.text(
                    "Nodes were actually read; plain HTTP can expose subscription credentials.",
                    "节点已实际读取；HTTP 明文传输可能暴露订阅凭据",
                ),
            ),
            SourceKind::HttpsSubscription => (
                language.text("HTTPS subscription added", "HTTPS 订阅已添加"),
                language.text(
                    "Mihomo downloaded and parsed nodes; view them on the Nodes page.",
                    "节点已由 Mihomo 实际下载并解析；可前往节点页查看",
                ),
            ),
            SourceKind::VlessNode => (
                language.text("VLESS node saved", "VLESS 节点已保存"),
                language.text(
                    "Added to the Saved group on Nodes.",
                    "已加入节点页的“已保存”分组",
                ),
            ),
        };
        let node_count: usize = providers.iter().map(|provider| provider.nodes.len()).sum();

        div()
            .mt_3()
            .p_3()
            .rounded_md()
            .bg(theme.action_soft)
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.status_success)
                    .child(title),
            )
            .child(
                div()
                    .mt_1()
                    .text_size(px(11.0))
                    .text_color(theme.text_secondary)
                    .child(detail),
            )
            .when(preview.kind != SourceKind::VlessNode, |card| {
                card.child(
                    div()
                        .mt_2()
                        .text_size(px(11.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(if language == Language::English {
                            format!("{} sources · {node_count} nodes", providers.len())
                        } else {
                            format!("{} 个来源 · {node_count} 个节点", providers.len())
                        }),
                )
            })
    }

    #[allow(clippy::too_many_lines)]
    fn imported_subscription_cards(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let language = self.language();
        let mut list = div();
        let now = mihomo::current_unix_secs();
        for (index, subscription) in self.imported_subscriptions.iter().enumerate() {
            let node_count: usize = subscription
                .providers
                .iter()
                .map(|provider| provider.nodes.len())
                .sum();
            let name = subscription.source.subscription_name().unwrap_or_else(|| {
                if language == Language::English {
                    format!("Subscription {}", index + 1)
                } else {
                    format!("订阅 {}", index + 1)
                }
            });
            let (detail, busy, healthy) = match subscription.state {
                ImportedSubscriptionState::None => continue,
                ImportedSubscriptionState::Pending(_)
                | ImportedSubscriptionState::Refreshing(_) => (
                    if language == Language::English {
                        format!(
                            "Updating nodes · Last success: {}",
                            source_update_label(
                                subscription.last_successful_update_unix_secs,
                                now,
                                language
                            )
                        )
                    } else {
                        format!(
                            "正在更新节点 · 上次成功：{}",
                            source_update_label(
                                subscription.last_successful_update_unix_secs,
                                now,
                                language
                            )
                        )
                    },
                    true,
                    true,
                ),
                ImportedSubscriptionState::Ready(kind) => (
                    if language == Language::English {
                        format!(
                            "{} · {} sources · {node_count} nodes · {}",
                            source_kind_label(kind, language),
                            subscription.providers.len(),
                            source_update_label(
                                subscription.last_successful_update_unix_secs,
                                now,
                                language
                            )
                        )
                    } else {
                        format!(
                            "{} · {} 个来源 · {node_count} 个节点 · {}",
                            source_kind_label(kind, language),
                            subscription.providers.len(),
                            source_update_label(
                                subscription.last_successful_update_unix_secs,
                                now,
                                language
                            )
                        )
                    },
                    false,
                    true,
                ),
                ImportedSubscriptionState::Unavailable(kind, error) => (
                    if language == Language::English {
                        format!(
                            "{} · Update failed: {error} · {}",
                            source_kind_label(kind, language),
                            source_update_label(
                                subscription.last_successful_update_unix_secs,
                                now,
                                language
                            )
                        )
                    } else {
                        format!(
                            "{} · 更新失败：{error} · {}",
                            source_kind_label(kind, language),
                            source_update_label(
                                subscription.last_successful_update_unix_secs,
                                now,
                                language
                            )
                        )
                    },
                    false,
                    false,
                ),
                ImportedSubscriptionState::StoreError(error) => (
                    if language == Language::English {
                        format!("{error} · Local source was not deleted")
                    } else {
                        format!("{error} · 本地来源没有被删除")
                    },
                    false,
                    false,
                ),
                ImportedSubscriptionState::Removing(_) => (
                    language
                        .text(
                            "Deleting locally saved subscription",
                            "正在删除本机保存的订阅",
                        )
                        .to_owned(),
                    true,
                    true,
                ),
            };
            let removable = !busy
                && !matches!(
                    self.subscription_feedback,
                    SubscriptionFeedback::Importing(_)
                );
            let controls_enabled = removable && !self.source_refresh_busy();
            let id = subscription.id.clone();
            let refresh_id = id.clone();
            let interval_id = id.clone();
            let next_interval = subscription.refresh_interval.next();
            list = list.child(
                div()
                    .mt_3()
                    .p_3()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.outline_subtle)
                    .bg(theme.surface_low)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .overflow_x_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(if healthy {
                                        theme.status_success
                                    } else {
                                        theme.route_trace
                                    })
                                    .child(name),
                            )
                            .when(busy, |header| {
                                header.child(Self::benchmark_latency_spinner(
                                    format!("source-refresh-{id}"),
                                    theme,
                                ))
                            }),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(11.0))
                            .text_color(theme.text_secondary)
                            .child(detail),
                    )
                    .child(
                        div()
                            .mt_3()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .id(format!("subscription-interval-{interval_id}"))
                                    .role(Role::Button)
                                    .aria_label(format!(
                                        "{}{}",
                                        language.text(
                                            "Change automatic update interval, current ",
                                            "更改自动更新间隔，当前"
                                        ),
                                        refresh_interval_label(
                                            subscription.refresh_interval,
                                            language
                                        )
                                    ))
                                    .tab_stop(controls_enabled)
                                    .focusable()
                                    .when(controls_enabled, gpui::Styled::cursor_pointer)
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(theme.outline_subtle)
                                    .bg(theme.surface_high)
                                    .text_size(px(10.0))
                                    .text_color(theme.text_secondary)
                                    .child(format!(
                                        "{} · {}",
                                        language.text("Update interval", "更新间隔"),
                                        refresh_interval_label(
                                            subscription.refresh_interval,
                                            language
                                        )
                                    ))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if controls_enabled {
                                            this.set_subscription_refresh_interval(
                                                interval_id.clone(),
                                                next_interval,
                                                cx,
                                            );
                                        }
                                    })),
                            )
                            .child(
                                Button::new(format!("subscription-refresh-{refresh_id}"))
                                    .accessibility_label(
                                        language.text(
                                            "Update this subscription now",
                                            "立即更新这个订阅",
                                        ),
                                    )
                                    .label(if busy {
                                        language.text("Updating…", "更新中…")
                                    } else {
                                        language.text("Update now", "立即更新")
                                    })
                                    .icon(IconName::Redo2)
                                    .tab_stop(controls_enabled)
                                    .disabled(!controls_enabled)
                                    .loading(busy)
                                    .with_variant(ButtonVariant::Text)
                                    .when(controls_enabled, gpui::Styled::cursor_pointer)
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(theme.action_primary)
                                    .bg(theme.surface_high)
                                    .text_size(px(10.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.action_primary)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if controls_enabled {
                                            this.refresh_imported_subscription(
                                                refresh_id.clone(),
                                                cx,
                                            );
                                        }
                                    })),
                            )
                            .child(div().flex_1())
                            .when(controls_enabled, |controls| {
                                controls.child(
                                    Button::new(format!("remove-{id}"))
                                        .accessibility_label(
                                            language
                                                .text("Remove this subscription", "移除这个订阅"),
                                        )
                                        .label(language.text("Remove", "移除"))
                                        .with_variant(ButtonVariant::Text)
                                        .cursor_pointer()
                                        .px_2()
                                        .py_1()
                                        .rounded_md()
                                        .text_size(px(10.0))
                                        .text_color(theme.route_trace)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.remove_imported_subscription(id.clone(), cx);
                                        })),
                                )
                            }),
                    ),
            );
        }
        list
    }

    #[allow(clippy::too_many_lines)]
    fn saved_vless_cards(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let language = self.language();
        let mut list = div();
        for saved in &self.saved_vless_nodes {
            let id = saved.id.clone();
            let node = saved.source.preview();
            list = list.child(
                div()
                    .mt_3()
                    .p_3()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.outline_subtle)
                    .bg(theme.surface_low)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .overflow_x_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(node.name.clone()),
                            )
                            .child(
                                Button::new(format!("remove-{id}"))
                                    .accessibility_label(
                                        language.text("Remove saved node", "移除已保存节点"),
                                    )
                                    .label(language.text("Remove", "移除"))
                                    .with_variant(ButtonVariant::Text)
                                    .cursor_pointer()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(theme.outline_subtle)
                                    .bg(theme.surface_high)
                                    .text_size(px(10.0))
                                    .text_color(theme.text_secondary)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        let Some(store_dir) = this.subscription_store_dir.clone()
                                        else {
                                            this.language()
                                                .text(
                                                    "Could not determine where to save the node",
                                                    "无法确定节点保存位置",
                                                )
                                                .clone_into(&mut this.status);
                                            cx.notify();
                                            return;
                                        };
                                        let runtime = this.runtime.clone();
                                        let remove_id = id.clone();
                                        this.language()
                                            .text(
                                                "Removing saved VLESS node",
                                                "正在移除保存的 VLESS 节点",
                                            )
                                            .clone_into(&mut this.status);
                                        let executor = cx.background_executor().clone();
                                        cx.spawn(async move |this, cx| {
                                            let result = executor
                                                .spawn(async move {
                                                    mihomo::remove_vless_source_in(
                                                        &store_dir, &remove_id,
                                                    )?;
                                                    Ok::<_, SubscriptionStoreError>((
                                                        remove_id,
                                                        SourceRuntimeApply::from_result(
                                                            runtime.apply_saved_sources(&store_dir),
                                                        ),
                                                    ))
                                                })
                                                .await;
                                            this.update(cx, |this, cx| {
                                                match result {
                                                    Ok((deleted_id, apply)) => {
                                                        this.saved_vless_nodes
                                                            .retain(|node| node.id != deleted_id);
                                                        let language = this.language();
                                                        apply.reconcile_proxy_mode(
                                                            &mut this.proxy_mode,
                                                        );
                                                        this.status =
                                                            if language == Language::English {
                                                                format!(
                                                                    "Saved VLESS node removed{}",
                                                                    apply.status_suffix(language)
                                                                )
                                                            } else {
                                                                format!(
                                                                    "已移除保存的 VLESS 节点{}",
                                                                    apply.status_suffix(language)
                                                                )
                                                            };
                                                    }
                                                    Err(error) => {
                                                        this.status = format!(
                                                            "{}: {error}",
                                                            this.language().text(
                                                                "Failed to remove node",
                                                                "移除节点失败"
                                                            )
                                                        );
                                                    }
                                                }
                                                cx.notify();
                                            })
                                            .ok();
                                        })
                                        .detach();
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(11.0))
                            .text_color(theme.text_secondary)
                            .child(format!("{} · {}", node.protocol, node.detail)),
                    ),
            );
        }
        list
    }

    fn subscription_error(
        title: &'static str,
        message: String,
        recovery: Option<&'static str>,
        theme: Theme,
    ) -> Div {
        div()
            .mt_3()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(theme.outline_strong)
            .bg(theme.surface_low)
            .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
            .child(
                div()
                    .mt_1()
                    .text_size(px(11.0))
                    .text_color(theme.text_secondary)
                    .child(message),
            )
            .when_some(recovery, |card, recovery| {
                card.child(
                    div()
                        .mt_2()
                        .text_size(px(10.0))
                        .text_color(theme.text_tertiary)
                        .child(recovery),
                )
            })
    }

    #[allow(clippy::too_many_lines)]
    fn rule_source_manager(
        &self,
        input: Entity<SubscriptionTextInput>,
        busy: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let language = self.language();
        let mut panel = div()
            .w_full()
            .p_4()
            .rounded_md()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .child(
                div()
                    .text_size(px(17.0))
                    .font_weight(FontWeight::BOLD)
                    .child(language.text("Rule subscriptions", "规则订阅")),
            )
            .child(
                div()
                    .mt_1()
                    .text_size(px(11.0))
                    .text_color(theme.text_secondary)
                    .child(language.text(
                        "Add QX rule URLs and choose the target policy used after a match.",
                        "添加 QX 规则地址，并选择命中后使用的目标策略。",
                    )),
            )
            .child(
                div()
                    .mt_4()
                    .mb_1()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(language.text("HTTPS rule URL", "HTTPS 规则地址")),
            )
            .child(input.clone())
            .child(
                div()
                    .mt_2()
                    .flex()
                    .gap_2()
                    .child(
                        div()
                            .id("qx-rule-target-policy")
                            .role(Role::Button)
                            .aria_label(
                                language
                                    .text("Change QX rule target policy", "切换 QX 规则目标策略"),
                            )
                            .tab_stop(true)
                            .focusable()
                            .when(!busy, gpui::Styled::cursor_pointer)
                            .h(px(36.0))
                            .px_3()
                            .rounded_md()
                            .border_1()
                            .border_color(theme.outline_strong)
                            .bg(theme.surface_high)
                            .text_size(px(11.0))
                            .text_color(theme.action_primary)
                            .font_weight(FontWeight::SEMIBOLD)
                            .flex()
                            .items_center()
                            .child(format!(
                                "{} · {}",
                                language.text("Target", "目标"),
                                self.qx_rule_target_policy
                            ))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !busy {
                                    this.cycle_qx_rule_target();
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        Button::new("qx-rule-import")
                            .accessibility_label(language.text(
                                "Download, validate, and import QX rules",
                                "下载、校验并导入 QX 规则",
                            ))
                            .loading(busy)
                            .with_variant(ButtonVariant::Primary)
                            .with_size(px(36.0))
                            .when(!busy, gpui::Styled::cursor_pointer)
                            .h(px(36.0))
                            .px_3()
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
                            .font_weight(FontWeight::SEMIBOLD)
                            .gap_2()
                            .flex_1()
                            .when(busy, |button| {
                                button.child(Self::benchmark_latency_spinner(
                                    "qx-rule-import-spinner".to_owned(),
                                    theme,
                                ))
                            })
                            .child(if busy {
                                language.text("Processing…", "处理中…")
                            } else {
                                language.text("Add rule source", "添加规则源")
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !busy {
                                    this.submit_qx_rule_import(&input, cx);
                                }
                            })),
                    ),
            )
            .child(self.qx_rule_import_feedback(theme, language))
            .child(
                div()
                    .mt_5()
                    .pt_4()
                    .border_t_1()
                    .border_color(theme.outline_subtle)
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(language.text("Added rule sources", "已添加规则源")),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.text_tertiary)
                            .child(if language == Language::English {
                                format!("{} total", self.qx_rule_sources.len())
                            } else {
                                format!("{} 个", self.qx_rule_sources.len())
                            }),
                    ),
            );

        if self.qx_rule_sources.is_empty() {
            panel = panel.child(
                div()
                    .mt_3()
                    .p_3()
                    .rounded_md()
                    .bg(theme.surface_low)
                    .text_size(px(11.0))
                    .text_color(theme.text_secondary)
                    .child(language.text(
                        "No rule subscription sources yet. After adding one, Routing rules will show the rules that actually participate in matching.",
                        "还没有规则订阅源。添加后，分流规则页会显示实际参与匹配的规则。",
                    )),
            );
        }
        for (index, source) in self.qx_rule_sources.iter().enumerate() {
            panel = panel.child(self.rule_source_card(index, source, busy, theme, cx));
        }
        panel.child(
            div()
                .mt_3()
                .text_size(px(10.0))
                .text_color(theme.text_tertiary)
                .child(language.text(
                    "Rule URLs and content are stored only in the private local directory; logs never record links.",
                    "规则地址和正文仅保存在本机私有目录；日志不会记录链接。",
                )),
        )
    }

    #[allow(clippy::too_many_lines)]
    fn rule_source_card(
        &self,
        index: usize,
        source: &crate::mihomo::StoredQxRuleSource,
        busy: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let language = self.language();
        let id = source.id.clone();
        let refresh_id = id.clone();
        let interval_id = id.clone();
        let refresh_state = self.qx_rule_source_refreshes.get(&source.id);
        let refreshing = refresh_state.is_some_and(QxRuleSourceRefreshState::is_refreshing);
        let duplicate = matches!(
            &self.qx_rule_feedback,
            QxRuleImportFeedback::AlreadyExists { source_id, .. } if source_id == &source.id
        );
        let controls_enabled = !busy && !self.source_refresh_busy();
        let next_interval = source.refresh_interval.next();
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
            .mt_2()
            .p_3()
            .rounded_md()
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
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_weight(FontWeight::SEMIBOLD)
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
                                .text_size(px(10.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.status_warning)
                                .child(language.text("Already added", "已添加")),
                        )
                    }),
            )
            .child(
                div()
                    .mt_1()
                    .text_size(px(10.0))
                    .text_color(theme.text_secondary)
                    .child(if language == Language::English {
                        format!(
                            "{} rules · {} skipped · → {} · {last_update}",
                            source.rule_count, source.diagnostic_count, target_policy
                        )
                    } else {
                        format!(
                            "{} 条 · 跳过 {} 条 · → {} · {last_update}",
                            source.rule_count, source.diagnostic_count, target_policy
                        )
                    }),
            )
            .when_some(refresh_error, |card, error| {
                card.child(
                    div()
                        .mt_2()
                        .text_size(px(10.0))
                        .text_color(theme.route_trace)
                        .child(format!(
                            "{}: {error}",
                            language.text("Last update failed", "上次更新失败")
                        )),
                )
            })
            .child(
                div()
                    .mt_3()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(self.qx_rule_source_target_select(source, controls_enabled, theme, cx))
                    .child(
                        div()
                            .id(format!("qx-rule-interval-{interval_id}"))
                            .role(Role::Button)
                            .aria_label(format!(
                                "{}{}",
                                language.text(
                                    "Change rule automatic update interval, current ",
                                    "更改规则自动更新间隔，当前"
                                ),
                                refresh_interval_label(source.refresh_interval, language)
                            ))
                            .tab_stop(controls_enabled)
                            .focusable()
                            .when(controls_enabled, gpui::Styled::cursor_pointer)
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .border_1()
                            .border_color(theme.outline_subtle)
                            .bg(theme.surface_high)
                            .text_size(px(10.0))
                            .text_color(theme.text_secondary)
                            .child(format!(
                                "{} · {}",
                                language.text("Update interval", "更新间隔"),
                                refresh_interval_label(source.refresh_interval, language)
                            ))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if controls_enabled {
                                    this.set_qx_rule_refresh_interval(
                                        interval_id.clone(),
                                        next_interval,
                                        cx,
                                    );
                                }
                            })),
                    )
                    .child(
                        Button::new(format!("qx-rule-refresh-{refresh_id}"))
                            .accessibility_label(
                                language.text(
                                    "Update this remote QX rule now",
                                    "立即更新这份远程 QX 规则",
                                ),
                            )
                            .label(if refreshing {
                                language.text("Updating…", "更新中…")
                            } else {
                                language.text("Update now", "立即更新")
                            })
                            .icon(IconName::Redo2)
                            .tab_stop(controls_enabled)
                            .disabled(!controls_enabled)
                            .loading(refreshing)
                            .with_variant(ButtonVariant::Text)
                            .when(controls_enabled, gpui::Styled::cursor_pointer)
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .border_1()
                            .border_color(theme.action_primary)
                            .bg(theme.surface_high)
                            .text_size(px(10.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.action_primary)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if controls_enabled {
                                    this.refresh_qx_rule_source(refresh_id.clone(), cx);
                                }
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new(format!("qx-rule-remove-{index}"))
                            .accessibility_label(
                                language.text("Delete this remote QX rule", "删除这份远程 QX 规则"),
                            )
                            .label(language.text("Delete", "删除"))
                            .tab_stop(controls_enabled)
                            .disabled(!controls_enabled)
                            .with_variant(ButtonVariant::Text)
                            .when(controls_enabled, gpui::Styled::cursor_pointer)
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .text_size(px(10.0))
                            .text_color(theme.route_trace)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if controls_enabled {
                                    this.remove_qx_rule_source(id.clone(), cx);
                                }
                            })),
                    ),
            )
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
                    .label(target)
                    .selected(selected)
                    .with_variant(ButtonVariant::Text)
                    .w_full()
                    .min_h(px(40.0))
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .bg(if selected {
                        theme.action_soft
                    } else {
                        theme.surface_high
                    })
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(if selected {
                        theme.action_primary
                    } else {
                        theme.text_primary
                    })
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
        let trigger = Button::new(format!("qx-rule-target-select-{}", source.id))
            .accessibility_label(language.text(
                "Change target policy for this rule source",
                "修改这个规则源的目标策略",
            ))
            .label(if updating {
                language.text("Saving…", "保存中…").to_owned()
            } else {
                format!("{} · {selected_target}", language.text("Policy", "策略"))
            })
            .dropdown_caret(true)
            .with_variant(ButtonVariant::Default)
            .with_size(px(34.0))
            .h(px(34.0))
            .text_size(px(10.0))
            .font_weight(FontWeight::SEMIBOLD)
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
            if let Some(input) = self.manual_rule_inputs.first() {
                input.focus_handle(cx).focus(window, cx);
            }
            return;
        }
        self.manual_rule_editor_state = super::ManualRuleEditorState::Creating;
        self.manual_rule_popover = None;
        self.manual_rule_error = None;
        self.manual_rule_condition_count = 1;
        for input in &self.manual_rule_inputs {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        for (index, kind) in self.manual_rule_kinds.iter_mut().enumerate() {
            *kind = if index == 1 {
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
        self.manual_rule_condition_count = rule.conditions().len();
        for (condition_index, input) in self.manual_rule_inputs.iter().enumerate() {
            if let Some(condition) = rule.conditions().get(condition_index) {
                self.manual_rule_kinds[condition_index] = condition.kind();
                input.update(cx, |input, cx| {
                    input.set_value_without_event(condition.parameter().to_owned(), cx);
                });
            } else {
                input.update(cx, SubscriptionTextInput::clear_without_event);
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
        if let Some(input) = self.manual_rule_inputs.first() {
            input.focus_handle(cx).focus(window, cx);
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
        if !self.manual_rule_inputs.is_empty() {
            for (input, kind) in self
                .manual_rule_inputs
                .iter()
                .zip(self.manual_rule_kinds.iter().copied())
            {
                let placeholder = manual_rule_placeholder(kind, self.language());
                input.update(cx, |input, cx| {
                    input.set_theme(theme, self.dark, cx);
                    input.set_placeholder(placeholder, cx);
                });
            }
            return;
        }
        self.manual_rule_kinds = (0..crate::manual_rule::MAX_CONDITIONS)
            .map(|index| {
                if index == 1 {
                    crate::manual_rule::ManualRuleKind::DstPort
                } else {
                    crate::manual_rule::ManualRuleKind::default()
                }
            })
            .collect();
        self.manual_rule_inputs = self
            .manual_rule_kinds
            .iter()
            .copied()
            .enumerate()
            .map(|(index, kind)| {
                let placeholder = manual_rule_placeholder(kind, self.language());
                cx.new(|cx| {
                    SubscriptionTextInput::new_field(
                        format!("manual-rule-parameter-{index}"),
                        placeholder,
                        MAX_MANUAL_RULE_INPUT_BYTES,
                        theme,
                        self.dark,
                        window,
                        cx,
                    )
                })
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
        let Some(selected_kind) = self.manual_rule_kinds.get_mut(condition_index) else {
            return;
        };
        *selected_kind = kind;
        self.manual_rule_error = None;
        self.manual_rule_popover = None;
        let placeholder = manual_rule_placeholder(kind, self.language());
        if let Some(input) = self.manual_rule_inputs.get(condition_index) {
            input.update(cx, |input, cx| input.set_placeholder(placeholder, cx));
        }
        cx.notify();
    }

    fn add_manual_rule_condition(&mut self, cx: &mut Context<Self>) {
        if self.manual_rule_condition_count >= crate::manual_rule::MAX_CONDITIONS {
            return;
        }
        let index = self.manual_rule_condition_count;
        self.manual_rule_condition_count += 1;
        self.manual_rule_popover = None;
        self.manual_rule_error = None;
        if let Some(input) = self.manual_rule_inputs.get(index) {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        cx.notify();
    }

    fn remove_manual_rule_condition(&mut self, index: usize, cx: &mut Context<Self>) {
        if index == 0 || index >= self.manual_rule_condition_count {
            return;
        }
        for current in index..self.manual_rule_condition_count - 1 {
            self.manual_rule_kinds[current] = self.manual_rule_kinds[current + 1];
            let value = self.manual_rule_inputs[current + 1]
                .read(cx)
                .value()
                .to_owned();
            self.manual_rule_inputs[current]
                .update(cx, |input, cx| input.set_value_without_event(value, cx));
        }
        self.manual_rule_condition_count -= 1;
        if let Some(input) = self
            .manual_rule_inputs
            .get(self.manual_rule_condition_count)
        {
            input.update(cx, SubscriptionTextInput::clear_without_event);
        }
        self.manual_rule_popover = None;
        self.manual_rule_error = None;
        cx.notify();
    }

    fn submit_manual_rule(&mut self, cx: &mut Context<Self>) -> bool {
        if self.manual_rule_editor_state == super::ManualRuleEditorState::Closed {
            return false;
        }
        if self.manual_rule_kinds[..self.manual_rule_condition_count]
            .iter()
            .any(|kind| !kind.supported_by(self.runtime.kind()))
        {
            self.manual_rule_error = Some(crate::manual_rule::ManualRuleError::UnsupportedByKernel);
            cx.notify();
            return false;
        }
        let conditions = self.manual_rule_kinds[..self.manual_rule_condition_count]
            .iter()
            .copied()
            .zip(self.manual_rule_inputs[..self.manual_rule_condition_count].iter())
            .map(|(kind, input)| (kind, input.read(cx).value().to_owned()))
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
        let previous = if let Some(index) = editing_index {
            match crate::manual_rule::replace_manual_rule(&mut self.manual_rules, index, rule) {
                Ok(previous) => Some(previous),
                Err(crate::manual_rule::ManualRuleEditError::Duplicate) => {
                    self.manual_rule_error = Some(crate::manual_rule::ManualRuleError::Duplicate);
                    cx.notify();
                    return false;
                }
                Err(crate::manual_rule::ManualRuleEditError::Missing) => {
                    self.reset_manual_rule_editor_state();
                    cx.notify();
                    return false;
                }
            }
        } else {
            if self.manual_rules.contains(&rule) {
                self.manual_rule_error = Some(crate::manual_rule::ManualRuleError::Duplicate);
                cx.notify();
                return false;
            }
            self.manual_rules.push(rule);
            None
        };
        if !self.persist_manual_rules(cx) {
            if let Some(index) = editing_index {
                if let Some(previous) = previous {
                    let _ = crate::manual_rule::replace_manual_rule(
                        &mut self.manual_rules,
                        index,
                        previous,
                    );
                }
            } else {
                self.manual_rules.pop();
            }
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
        let removed = self.manual_rules.remove(index);
        if !self.persist_manual_rules(cx) {
            self.manual_rules.insert(index, removed);
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

    fn persist_manual_rules(&mut self, cx: &mut Context<Self>) -> bool {
        let language = self.language();
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            language
                .text(
                    "Could not determine where to save manual rules",
                    "无法确定手动分流规则的保存位置",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return false;
        };
        self.sync_routing_rule_group_order();
        if mihomo::save_routing_rule_group_order_in(&store_dir, &self.routing_rule_group_order)
            .is_err()
        {
            language
                .text(
                    "Could not save routing rule group order",
                    "无法保存分流规则分组顺序",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return false;
        }
        if let Err(error) = crate::manual_rule::save_manual_rules_in(&store_dir, &self.manual_rules)
        {
            self.status = format!(
                "{}{error}",
                language.text("Could not save manual rules: ", "无法保存手动分流规则：")
            );
            cx.notify();
            return false;
        }
        let apply = SourceRuntimeApply::from_result(self.runtime.apply_saved_sources(&store_dir));
        apply.reconcile_proxy_mode(&mut self.proxy_mode);
        self.status = format!(
            "{}{}",
            language.text("Manual rules updated", "手动分流规则已更新"),
            apply.status_suffix(language)
        );
        cx.notify();
        true
    }

    fn sync_routing_rule_group_order(&mut self) {
        self.routing_rule_group_order = mihomo::normalized_routing_rule_group_order(
            &self.routing_rule_group_order,
            !self.manual_rules.is_empty(),
            &self.qx_rule_sources,
        );
    }

    fn move_routing_rule_group(&mut self, group_id: &str, direction: i8, cx: &mut Context<Self>) {
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
        if mihomo::save_routing_rule_group_order_in(&store_dir, &self.routing_rule_group_order)
            .is_err()
        {
            self.routing_rule_group_order = previous;
            self.language()
                .text(
                    "Could not save routing rule group order",
                    "无法保存分流规则分组顺序",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let language = self.language();
        let apply = SourceRuntimeApply::from_result(self.runtime.apply_saved_sources(&store_dir));
        apply.reconcile_proxy_mode(&mut self.proxy_mode);
        self.status = format!(
            "{}{}",
            if direction < 0 {
                language.text("Rule group moved up", "规则分组已上移")
            } else {
                language.text("Rule group moved down", "规则分组已下移")
            },
            apply.status_suffix(language)
        );
        cx.notify();
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
        let mut choices = div().id("manual-rule-kind-choices");
        for kind in crate::manual_rule::ManualRuleKind::ALL {
            let supported = kind.supported_by(kernel);
            let selected = selected_kind == kind;
            let detail = if supported {
                manual_rule_kind_detail(kind, language)
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
                    .px_3()
                    .py_1()
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
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(if supported {
                                theme.text_primary
                            } else {
                                theme.text_tertiary
                            })
                            .child(kind.display_label()),
                    )
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(theme.text_tertiary)
                            .child(detail),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_manual_rule_kind(condition_index, kind, cx);
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
                    .min_h(px(40.0))
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .bg(if selected {
                        theme.action_soft
                    } else {
                        theme.surface_high
                    })
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
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
            .label(value)
            .dropdown_caret(true)
            .with_variant(ButtonVariant::Default)
            .with_size(px(38.0))
            .h(px(38.0))
            .w_full()
            .text_size(px(11.0))
            .font_weight(FontWeight::SEMIBOLD);

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
            .manual_rule_inputs
            .get(condition_index)
            .expect("manual rule condition input is initialized")
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
        let mut row = div()
            .mt_3()
            .child(
                div()
                    .mb_1()
                    .text_size(px(10.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_secondary)
                    .child(if condition_index == 0 {
                        language.text("Condition 1", "条件 1").to_owned()
                    } else if language == Language::English {
                        format!("AND · Condition {}", condition_index + 1)
                    } else {
                        format!("并且 · 条件 {}", condition_index + 1)
                    }),
            )
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
                    .child(div().flex_1().min_w(px(0.0)).child(input)),
            );
        if condition_index > 0 {
            row = row.child(
                Button::new(format!("remove-manual-rule-condition-{condition_index}"))
                    .accessibility_label(language.text("Remove this condition", "移除这个条件"))
                    .label(language.text("Remove condition", "移除条件"))
                    .text()
                    .with_size(px(30.0))
                    .mt_2()
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
        let mut conditions = div();
        for condition_index in 0..self.manual_rule_condition_count {
            conditions = conditions.child(self.manual_rule_condition_editor(
                condition_index,
                self.manual_rule_kinds[condition_index],
                theme,
                language,
                compact,
                cx,
            ));
        }
        if self.manual_rule_condition_count < crate::manual_rule::MAX_CONDITIONS {
            conditions = conditions.child(
                Button::new("add-manual-rule-condition")
                    .accessibility_label(language.text("Add an AND condition", "添加并且条件"))
                    .label(language.text("+ Add AND condition", "+ 添加“并且”条件"))
                    .with_variant(ButtonVariant::Default)
                    .with_size(px(38.0))
                    .mt_3()
                    .px_3()
                    .py_2()
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
            .child(
                div()
                    .mb_1()
                    .text_size(px(10.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_secondary)
                    .child(language.text("Policy after match", "命中后的策略")),
            )
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
                Button::new("cancel-manual-rule")
                    .accessibility_label(language.text("Cancel editing rule", "取消编辑规则"))
                    .label(language.text("Cancel", "取消"))
                    .with_variant(ButtonVariant::Default)
                    .with_size(px(38.0))
                    .h(px(38.0))
                    .px_4()
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
                Button::new("save-manual-rule")
                    .accessibility_label(if editing {
                        language.text("Save manual rule changes", "保存手动规则修改")
                    } else {
                        language.text("Add manual rule", "添加手动规则")
                    })
                    .label(if editing {
                        language.text("Save changes", "保存修改")
                    } else {
                        language.text("Add rule", "添加规则")
                    })
                    .primary()
                    .with_size(px(38.0))
                    .h(px(38.0))
                    .px_4()
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
                            .font_weight(FontWeight::BOLD)
                            .child(if editing {
                                language.text("Edit routing rule", "编辑分流规则")
                            } else {
                                language.text("Add routing rule", "添加分流规则")
                            }),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::NORMAL)
                            .text_color(theme.text_secondary)
                            .child(language.text(
                                "All conditions must match. Group order determines rule priority.",
                                "同一条规则中的条件必须全部命中；分组顺序决定规则优先级。",
                            )),
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
                                .text_size(px(11.0))
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

    fn manual_rule_actions(
        index: usize,
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_1()
            .child(
                Button::new(format!("edit-manual-rule-{index}"))
                    .accessibility_label(language.text("Edit this manual rule", "编辑这条手动规则"))
                    .label(language.text("Edit", "编辑"))
                    .text()
                    .with_size(px(30.0))
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.action_primary)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_manual_rule_editor_for_edit(index, window, cx);
                    })),
            )
            .child(
                Button::new(format!("remove-manual-rule-{index}"))
                    .accessibility_label(
                        language.text("Remove this manual rule", "移除这条手动规则"),
                    )
                    .label(language.text("Remove", "移除"))
                    .text()
                    .with_size(px(30.0))
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .text_color(theme.status_error)
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.remove_manual_rule(index, cx)),
                    ),
            )
    }

    fn manual_routing_rule_row(
        &self,
        order: usize,
        index: usize,
        rule: &crate::manual_rule::ManualRule,
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> Div {
        let target = self.effective_rule_target(rule.target(), language);
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
                        .text_size(px(9.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_tertiary)
                        .child(language.text("AND", "并且")),
                );
            }
            matchers = matchers.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .bg(theme.surface_high)
                    .child(
                        div()
                            .text_size(px(9.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_secondary)
                            .child(condition.kind().display_label()),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.text_primary)
                            .child(condition.parameter().to_owned()),
                    ),
            );
        }
        div()
            .mt_1()
            .min_h(px(44.0))
            .px_3()
            .py_2()
            .rounded_md()
            .bg(theme.surface_low)
            .flex()
            .items_center()
            .gap_3()
            .child(
                div()
                    .w(px(42.0))
                    .flex_shrink_0()
                    .text_size(px(10.0))
                    .text_color(theme.text_tertiary)
                    .child(format!("#{order:03}")),
            )
            .child(matchers)
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.action_primary)
                    .child(target),
            )
            .child(Self::manual_rule_actions(index, theme, language, cx))
    }

    fn rule_group_order_controls(
        group_id: &str,
        group_name: &str,
        position: usize,
        group_count: usize,
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> Div {
        let up_id = group_id.to_owned();
        let down_id = group_id.to_owned();
        div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_1()
            .child(
                Button::new(format!("move-rule-group-up-{group_id}"))
                    .accessibility_label(if language == Language::English {
                        format!("Move {group_name} up")
                    } else {
                        format!("上移{group_name}")
                    })
                    .icon(IconName::ArrowUp)
                    .text()
                    .with_size(px(30.0))
                    .text_color(theme.text_secondary)
                    .disabled(position == 0)
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
                    .with_size(px(30.0))
                    .text_color(theme.text_secondary)
                    .disabled(position + 1 >= group_count)
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
            .map(|source| source.rule_count)
            .sum::<usize>();
        let mut list = div()
            .id("active-routing-rules")
            .w_full()
            .flex_1()
            .min_w(px(0.0))
            .p_4()
            .rounded_md()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .child(
                                div()
                                    .text_size(px(17.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child(language.text("Active rules", "生效规则")),
                            )
                            .child(
                                div()
                                    .mt_1()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_secondary)
                                    .child(language.text(
                                        "Groups match from top to bottom; use the arrows to change priority.",
                                        "分组从上到下匹配；使用箭头调整优先级。",
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .rounded_sm()
                                    .bg(theme.action_soft)
                                    .text_size(px(10.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.action_primary)
                                    .child(if language == Language::English {
                                        format!(
                                            "{} rules",
                                            self.manual_rules.len() + remote_count
                                        )
                                    } else {
                                        format!("{} 条", self.manual_rules.len() + remote_count)
                                    }),
                            )
                            .child(
                                Button::new("open-route-test")
                                    .accessibility_label(
                                        language.text("Test routing rules", "测试分流规则"),
                                    )
                                    .label(language.text("Test rules", "测试规则"))
                                    .with_variant(ButtonVariant::Default)
                                    .with_size(px(34.0))
                                    .h(px(34.0))
                                    .px_3()
                                    .border_color(theme.outline_subtle)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_route_inspector(window, cx);
                                    })),
                            )
                            .child(
                                Button::new("open-manual-rule-editor")
                                    .accessibility_label(
                                        language.text("Add routing rule", "添加分流规则"),
                                    )
                                    .label(language.text("Add rule", "添加规则"))
                                    .primary()
                                    .with_size(px(34.0))
                                    .h(px(34.0))
                                    .px_3()
                                    .cursor_pointer()
                                    .bg(theme.action_primary)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.action_on_primary)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_manual_rule_editor(window, cx);
                                    })),
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
                let expanded = self
                    .node_workspace
                    .is_group_collapsed(MANUAL_RULES_EXPANSION_KEY);
                let group_name = language.text("Manual rules", "手动规则");
                let detail = if language == Language::English {
                    format!("{} rules · Saved locally", self.manual_rules.len())
                } else {
                    format!("{} 条规则 · 本地保存", self.manual_rules.len())
                };
                let title_detail = div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(group_name))
                    .child(
                        div()
                            .mt_1()
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(10.0))
                            .text_color(theme.text_tertiary)
                            .child(detail),
                    );
                let title = div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(title_detail)
                    .child(Self::rule_group_order_controls(
                        group_id,
                        group_name,
                        group_position,
                        group_count,
                        theme,
                        language,
                        cx,
                    ));
                let mut rules = div()
                    .px(if compact { px(8.0) } else { px(12.0) })
                    .pb_3()
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
                    .mt_4()
                    .border_t_1()
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .item(|item| {
                        item.open(expanded)
                            .title_style(accordion_title_style(compact))
                            .content_style(accordion_content_style())
                            .bg(theme.surface_low)
                            .title(title)
                            .child(rules)
                    })
                    .on_toggle_click(cx.listener(|this, open_indices: &[usize], _, cx| {
                        let should_expand = open_indices.contains(&0);
                        if this
                            .node_workspace
                            .is_group_collapsed(MANUAL_RULES_EXPANSION_KEY)
                            != should_expand
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
            let expanded = self.node_workspace.is_group_collapsed(&expansion_key);
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
            let detail = if language == Language::English {
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
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(name.clone()),
                )
                .child(
                    div()
                        .mt_1()
                        .overflow_x_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(10.0))
                        .text_color(theme.text_tertiary)
                        .child(detail),
                );
            let title = div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(title_detail)
                .child(self.qx_rule_source_target_select(
                    source,
                    !self.source_refresh_busy(),
                    theme,
                    cx,
                ))
                .child(Self::rule_group_order_controls(
                    group_id,
                    &name,
                    group_position,
                    group_count,
                    theme,
                    language,
                    cx,
                ));
            let mut rules = div()
                .px(if compact { px(8.0) } else { px(12.0) })
                .pb_3()
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
                .mt_4()
                .border_t_1()
                .border_b_1()
                .border_color(theme.outline_subtle)
                .item(|item| {
                    item.open(expanded)
                        .title_style(accordion_title_style(compact))
                        .content_style(accordion_content_style())
                        .bg(theme.surface_low)
                        .title(title)
                        .child(rules)
                })
                .on_toggle_click(cx.listener(move |this, open_indices: &[usize], _, cx| {
                    let should_expand = open_indices.contains(&0);
                    if this.node_workspace.is_group_collapsed(&toggle_key) != should_expand {
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
                div()
                    .mt_4()
                    .p_4()
                    .rounded_md()
                    .bg(theme.surface_low)
                    .text_color(theme.text_secondary)
                    .child(language.text(
                        "No active routing rules. Manis does not add locked fallback rules.",
                        "暂无生效规则；Manis 不会添加不可编辑的兜底规则。",
                    )),
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
            .px_3()
            .py_2()
            .rounded_md()
            .bg(theme.surface_low)
            .flex()
            .items_center()
            .gap_3()
            .child(
                div()
                    .w(px(42.0))
                    .flex_shrink_0()
                    .text_size(px(10.0))
                    .text_color(theme.text_tertiary)
                    .child(format!("#{order:03}")),
            )
            .child(
                div()
                    .w(px(124.0))
                    .flex_shrink_0()
                    .text_size(px(10.0))
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
                    .child(value.to_owned()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
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
            .text_size(px(11.0))
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

    fn cycle_qx_rule_target(&mut self) {
        let targets = self.qx_rule_targets();
        let next = targets
            .iter()
            .position(|target| target == &self.qx_rule_target_policy)
            .map_or(0, |index| (index + 1) % targets.len());
        self.qx_rule_target_policy.clone_from(&targets[next]);
        self.qx_rule_feedback = QxRuleImportFeedback::Idle;
        self.status = format!(
            "{} {}",
            self.language()
                .text("QX rule target switched to", "QX 规则目标已切换为"),
            self.qx_rule_target_policy
        );
    }

    #[allow(clippy::too_many_lines)]
    fn submit_qx_rule_import(
        &mut self,
        input: &Entity<SubscriptionTextInput>,
        cx: &mut Context<Self>,
    ) {
        let url = input.read(cx).value().trim().to_owned();
        let target = self.qx_rule_target_policy.clone();
        let operation_id = begin_operation(
            "configuration.rule_source.add.requested",
            format!(
                "target={target} known_sources={}",
                self.qx_rule_sources.len()
            ),
        );
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
            return;
        };
        if let Ok(source) = SecretUrl::parse_https(&url)
            && let Some(existing) = self
                .qx_rule_sources
                .iter()
                .find(|existing| existing.source == source)
        {
            let target_policy =
                self.effective_rule_target(existing.target_policy.as_str(), self.language());
            self.qx_rule_feedback = QxRuleImportFeedback::AlreadyExists {
                source_id: existing.id.clone(),
                rule_count: existing.rule_count,
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
                format!(
                    "existing_id={} rules={} target={target_policy}",
                    existing.id, existing.rule_count
                ),
            );
            cx.notify();
            return;
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
                    let content =
                        download_qx_rule_document(&url).map_err(ImportQxRuleError::Download)?;
                    let parsed = QxRuleList::parse(&content);
                    if parsed.rules.is_empty() {
                        return Err(ImportQxRuleError::InvalidDocument);
                    }
                    let saved =
                        mihomo::save_qx_rule_source_in(&store_dir, &url, &target, &content)
                            .map_err(ImportQxRuleError::Store)?;
                    Ok::<_, ImportQxRuleError>(match saved {
                        mihomo::SaveQxRuleSourceOutcome::Created(stored) => {
                            let apply = SourceRuntimeApply::from_result(
                                runtime.apply_saved_sources(&store_dir),
                            );
                            ImportQxRuleSuccess::Imported { stored, apply }
                        }
                        mihomo::SaveQxRuleSourceOutcome::Existing(stored) => {
                            ImportQxRuleSuccess::AlreadyExists { stored }
                        }
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                if this.qx_rule_import_generation != generation {
                    return;
                }
                if let Some(input) = this.qx_rule_input.as_ref() {
                    input.update(cx, |input, cx| input.set_enabled(true, cx));
                }
                match result {
                    Ok(ImportQxRuleSuccess::Imported { stored, apply }) => {
                        let language = this.language();
                        let rule_count = stored.rule_count;
                        let diagnostic_count = stored.diagnostic_count;
                        let stored_id = stored.id.clone();
                        let target_policy = this
                            .effective_rule_target(stored.target_policy.as_str(), this.language());
                        if let Some(existing) = this
                            .qx_rule_sources
                            .iter_mut()
                            .find(|source| source.id == stored_id)
                        {
                            *existing = stored;
                        } else {
                            this.qx_rule_sources.push(stored);
                        }
                        this.sync_routing_rule_group_order();
                        if let Some(store_dir) = this.subscription_store_dir.as_ref() {
                            let _ = mihomo::save_routing_rule_group_order_in(
                                store_dir,
                                &this.routing_rule_group_order,
                            );
                        }
                        this.qx_rule_source_refreshes.remove(&stored_id);
                        this.qx_rule_feedback = QxRuleImportFeedback::Imported {
                            rule_count,
                            diagnostic_count,
                        };
                        if let Some(input) = this.qx_rule_input.as_ref() {
                            input.update(cx, SubscriptionTextInput::clear_without_event);
                        }
                        apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        this.status = if language == Language::English {
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
                    Ok(ImportQxRuleSuccess::AlreadyExists { stored }) => {
                        let target_policy = this
                            .effective_rule_target(stored.target_policy.as_str(), this.language());
                        let source_id = stored.id.clone();
                        let rule_count = stored.rule_count;
                        if !this
                            .qx_rule_sources
                            .iter()
                            .any(|source| source.id == source_id)
                        {
                            this.qx_rule_sources.push(stored);
                        }
                        this.sync_routing_rule_group_order();
                        if let Some(store_dir) = this.subscription_store_dir.as_ref() {
                            let _ = mihomo::save_routing_rule_group_order_in(
                                store_dir,
                                &this.routing_rule_group_order,
                            );
                        }
                        this.qx_rule_feedback = QxRuleImportFeedback::AlreadyExists {
                            source_id: source_id.clone(),
                            rule_count,
                            target_policy: target_policy.clone(),
                        };
                        this.language()
                            .text(
                                "Rule source already exists; no duplicate was added",
                                "规则源已存在，未重复添加",
                            )
                            .clone_into(&mut this.status);
                        record_operation(
                            operation_id,
                            LogLevel::Warn,
                            "configuration.rule_source.add.duplicate",
                            format!(
                                "existing_id={source_id} rules={rule_count} target={target_policy}"
                            ),
                        );
                    }
                    Err(ImportQxRuleError::Download(error)) => {
                        this.qx_rule_feedback = QxRuleImportFeedback::DownloadFailed(error);
                        this.status = format!(
                            "{}: {error}",
                            this.language()
                                .text("QX rule download failed", "QX 规则下载失败")
                        );
                        record_operation(
                            operation_id,
                            LogLevel::Error,
                            "configuration.rule_source.add.failed",
                            format!("phase=download error={error}"),
                        );
                    }
                    Err(ImportQxRuleError::InvalidDocument) => {
                        this.qx_rule_feedback = QxRuleImportFeedback::InvalidDocument;
                        this.language()
                            .text(
                                "QX rules not imported: no recognizable domain rules",
                                "QX 规则未导入：没有可识别的域名规则",
                            )
                            .clone_into(&mut this.status);
                        record_operation(
                            operation_id,
                            LogLevel::Error,
                            "configuration.rule_source.add.failed",
                            "phase=parse reason=no_recognizable_domain_rules",
                        );
                    }
                    Err(ImportQxRuleError::Store(error)) => {
                        this.qx_rule_feedback = QxRuleImportFeedback::StoreFailed(error);
                        this.status = format!(
                            "{}: {error}",
                            this.language()
                                .text("QX rule save failed", "QX 规则保存失败")
                        );
                        record_operation(
                            operation_id,
                            LogLevel::Error,
                            "configuration.rule_source.add.failed",
                            format!("phase=store error={error}"),
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
                    mihomo::remove_qx_rule_source_in(&store_dir, &id)?;
                    let apply =
                        SourceRuntimeApply::from_result(runtime.apply_saved_sources(&store_dir));
                    Ok::<_, SubscriptionStoreError>((id, apply))
                })
                .await;
            this.update(cx, |this, cx| {
                if this.qx_rule_import_generation != generation {
                    return;
                }
                match result {
                    Ok((id, apply)) => {
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
                        apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        this.status = if language == Language::English {
                            format!("Remote QX rules removed{}", apply.status_suffix(language))
                        } else {
                            format!("远程 QX 规则已移除{}", apply.status_suffix(language))
                        };
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

    fn set_subscription_refresh_interval(
        &mut self,
        id: String,
        refresh_interval: RemoteSourceRefreshInterval,
        cx: &mut Context<Self>,
    ) {
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
        let kind = super::source_kind(&source.source);
        self.subscription_preview_generation = self.subscription_preview_generation.wrapping_add(1);
        let generation = self.subscription_preview_generation;
        source.generation = generation;
        source.state = ImportedSubscriptionState::Refreshing(kind);
        self.status = format!(
            "{}: {}",
            self.language().text(
                "Saving subscription update interval",
                "正在保存订阅更新间隔"
            ),
            refresh_interval_label(refresh_interval, self.language())
        );
        let executor = cx.background_executor().clone();
        let task_id = id.clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    mihomo::update_subscription_source_refresh_interval_in(
                        &store_dir,
                        &task_id,
                        refresh_interval,
                    )
                })
                .await;
            this.update(cx, |this, cx| {
                let language = this.language();
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
                    Ok(stored) => {
                        source.refresh_interval = stored.refresh_interval;
                        source.last_successful_update_unix_secs =
                            stored.last_successful_update_unix_secs;
                        source.state = previous_state;
                        this.status = format!(
                            "{} {}",
                            language
                                .text("Subscription update interval set to", "订阅更新间隔已设为"),
                            refresh_interval_label(stored.refresh_interval, language)
                        );
                    }
                    Err(error) => {
                        source.state = ImportedSubscriptionState::StoreError(error);
                        this.status = format!(
                            "{}: {error}",
                            this.language().text(
                                "Failed to save subscription update interval",
                                "订阅更新间隔保存失败"
                            )
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

    fn set_qx_rule_refresh_interval(
        &mut self,
        id: String,
        refresh_interval: RemoteSourceRefreshInterval,
        cx: &mut Context<Self>,
    ) {
        if self.source_refresh_busy() {
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            return;
        };
        self.qx_rule_import_generation = self.qx_rule_import_generation.wrapping_add(1);
        let generation = self.qx_rule_import_generation;
        self.qx_rule_source_refreshes.insert(
            id.clone(),
            QxRuleSourceRefreshState::Refreshing { generation },
        );
        self.status = format!(
            "{}: {}",
            self.language()
                .text("Saving rule update interval", "正在保存规则更新间隔"),
            refresh_interval_label(refresh_interval, self.language())
        );
        let executor = cx.background_executor().clone();
        let task_id = id.clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    mihomo::update_qx_rule_source_refresh_interval_in(
                        &store_dir,
                        &task_id,
                        refresh_interval,
                    )
                })
                .await;
            this.update(cx, |this, cx| {
                if !matches!(
                    this.qx_rule_source_refreshes.get(&id),
                    Some(QxRuleSourceRefreshState::Refreshing { generation: active })
                        if *active == generation
                ) {
                    return;
                }
                match result {
                    Ok(stored) => {
                        let language = this.language();
                        if let Some(source) = this
                            .qx_rule_sources
                            .iter_mut()
                            .find(|source| source.id == id)
                        {
                            *source = stored;
                        }
                        this.qx_rule_source_refreshes.remove(&id);
                        this.status = format!(
                            "{} {}",
                            language.text("Rule update interval set to", "规则更新间隔已设为"),
                            refresh_interval_label(refresh_interval, language)
                        );
                    }
                    Err(error) => {
                        this.qx_rule_source_refreshes.insert(
                            id.clone(),
                            QxRuleSourceRefreshState::Failed {
                                generation,
                                message: error.to_string(),
                            },
                        );
                        this.status = format!(
                            "{}: {error}",
                            this.language().text(
                                "Failed to save rule update interval",
                                "规则更新间隔保存失败"
                            )
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
                    let stored =
                        mihomo::update_qx_rule_source_target_in(&store_dir, &task_id, &target)?;
                    let apply =
                        SourceRuntimeApply::from_result(runtime.apply_saved_sources(&store_dir));
                    Ok::<_, SubscriptionStoreError>((stored, apply))
                })
                .await;
            this.update(cx, |this, cx| {
                if this.qx_rule_source_target_updates.get(&id) != Some(&generation) {
                    return;
                }
                this.qx_rule_source_target_updates.remove(&id);
                match result {
                    Ok((stored, apply)) => {
                        let language = this.language();
                        let target = stored.target_policy.as_str().to_owned();
                        if let Some(source) = this
                            .qx_rule_sources
                            .iter_mut()
                            .find(|source| source.id == id)
                        {
                            *source = stored;
                        }
                        apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        this.status = format!(
                            "{} {target}{}",
                            language.text("Rule source policy set to", "规则源策略已设为"),
                            apply.status_suffix(language)
                        );
                        record_event(
                            LogLevel::Info,
                            "routing.rule_source.target.updated",
                            format!("source_id={id} target={target}"),
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

    #[allow(clippy::too_many_lines)]
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
                    let stored = mihomo::replace_qx_rule_source_content_in(
                        &store_dir,
                        &task_id,
                        &content,
                        mihomo::current_unix_secs(),
                    )
                    .map_err(ImportQxRuleError::Store)?;
                    let apply =
                        SourceRuntimeApply::from_result(runtime.apply_saved_sources(&store_dir));
                    Ok::<_, ImportQxRuleError>((stored, apply))
                })
                .await;
            this.update(cx, |this, cx| {
                if !matches!(
                    this.qx_rule_source_refreshes.get(&id),
                    Some(QxRuleSourceRefreshState::Refreshing { generation: active })
                        if *active == generation
                ) {
                    return;
                }
                match result {
                    Ok((stored, apply)) => {
                        let language = this.language();
                        let rule_count = stored.rule_count;
                        if let Some(source) = this
                            .qx_rule_sources
                            .iter_mut()
                            .find(|source| source.id == id)
                        {
                            *source = stored;
                        }
                        this.qx_rule_source_refreshes.remove(&id);
                        this.source_refresh_retry_not_before
                            .remove(&super::DueRemoteSource::QxRule(id.clone()).scheduler_key());
                        apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        this.status = if language == Language::English {
                            format!(
                                "QX rules updated · {rule_count} active rules{}",
                                apply.status_suffix(language)
                            )
                        } else {
                            format!(
                                "QX 规则更新完成 · {rule_count} 条生效{}",
                                apply.status_suffix(language)
                            )
                        };
                    }
                    Err(error) => {
                        let message = match error {
                            ImportQxRuleError::Download(error) => error.to_string(),
                            ImportQxRuleError::InvalidDocument => this
                                .language()
                                .text("No recognizable domain rules", "没有可识别的域名规则")
                                .to_owned(),
                            ImportQxRuleError::Store(error) => error.to_string(),
                        };
                        this.qx_rule_source_refreshes.insert(
                            id.clone(),
                            QxRuleSourceRefreshState::Failed {
                                generation,
                                message: message.clone(),
                            },
                        );
                        this.status = format!(
                            "{}: {message}",
                            this.language()
                                .text("QX rule update failed", "QX 规则更新失败")
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
}

#[cfg(test)]
mod tests {
    use manis_core::NodeWorkspaceState;

    use super::{
        Language, MANUAL_RULES_EXPANSION_KEY, rule_source_expansion_key, source_update_label,
    };

    #[test]
    fn remote_rule_sources_start_collapsed_and_remember_expansion() {
        let mut workspace = NodeWorkspaceState::default();
        let key = rule_source_expansion_key("qx-rule-deadbeef");

        assert!(!workspace.is_group_collapsed(&key));
        workspace.toggle_group(&key);
        assert!(workspace.is_group_collapsed(&key));
        assert_eq!(key, "routing-rule-source:qx-rule-deadbeef");
    }

    #[test]
    fn manual_rules_use_the_same_collapsible_group_state() {
        let mut workspace = NodeWorkspaceState::default();

        assert!(!workspace.is_group_collapsed(MANUAL_RULES_EXPANSION_KEY));
        workspace.toggle_group(MANUAL_RULES_EXPANSION_KEY);
        assert!(workspace.is_group_collapsed(MANUAL_RULES_EXPANSION_KEY));
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
