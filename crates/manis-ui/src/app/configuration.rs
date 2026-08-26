use gpui::{
    Context, Div, Entity, FontWeight, ParentElement, Role, Stateful, Styled, div, prelude::*, px,
};
use manis_core::{KernelKind, WindowSizeClass};
use manis_profile::QxRuleKind;

use super::{
    ImportQxRuleError, ImportedSubscriptionState, ManisApp, QxRuleImportFeedback, QxRuleList,
    QxRuleSourceRefreshState, SourceRuntimeApply, SubscriptionFeedback,
};
use crate::{
    diagnostics::{UiEvent, trace_ui},
    localization::{Language, LanguagePreference, save_language_preference_in},
    mihomo::{self, LoadedProvider, RemoteSourceRefreshInterval, SubscriptionStoreError},
    rule_source::{download_qx_rule_document, download_qx_rule_document_secret},
    subscription::{SourceKind, SubscriptionPreview, validate_subscription_preview},
    subscription_input::SubscriptionTextInput,
    theme::Theme,
};

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
        if let Some(input) = self.node_group_name_input.as_ref() {
            input.update(cx, |input, cx| {
                input.set_placeholder(
                    language.text("For example: Hong Kong Auto", "例如：香港自动优选"),
                    cx,
                );
            });
        }
        if let Some(input) = self.node_group_filter_input.as_ref() {
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
        &self,
        theme: Theme,
        size_class: WindowSizeClass,
        _cx: &mut Context<Self>,
    ) -> Div {
        let compact = size_class == WindowSizeClass::Compact;
        let language = self.language();
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
                        self.active_rules_panel(theme, language)
                            .max_w(px(1040.0))
                            .mx_auto(),
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
                div()
                    .id("subscription-preview")
                    .role(Role::Button)
                    .aria_label(if direct_input {
                        language.text("Save VLESS node", "保存 VLESS 节点")
                    } else {
                        language.text("Validate and import subscription", "验证并导入订阅")
                    })
                    .tab_stop(true)
                    .focusable()
                    .when(!busy, gpui::Styled::cursor_pointer)
                    .h(px(36.0))
                    .px_3()
                    .rounded_md()
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
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_1()
                    .child(if busy {
                        language.text("Processing…", "正在处理…")
                    } else if direct_input {
                        language.text("Save VLESS node", "保存 VLESS 节点")
                    } else {
                        language.text("Import subscription", "导入订阅")
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if busy {
                            return;
                        }
                        this.submit_source_import(&input, cx);
                    })),
            )
            .child(
                div()
                    .id("subscription-clear")
                    .role(Role::Button)
                    .aria_label(language.text("Clear subscription link draft", "清除订阅链接草稿"))
                    .tab_stop(true)
                    .focusable()
                    .when(!busy, gpui::Styled::cursor_pointer)
                    .h(px(36.0))
                    .px_3()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.outline_subtle)
                    .bg(theme.surface_high)
                    .text_color(theme.text_secondary)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(language.text("Clear", "清除"))
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
                                div()
                                    .id(format!("subscription-refresh-{refresh_id}"))
                                    .role(Role::Button)
                                    .aria_label(
                                        language.text(
                                            "Update this subscription now",
                                            "立即更新这个订阅",
                                        ),
                                    )
                                    .tab_stop(controls_enabled)
                                    .focusable()
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
                                    .child(if busy {
                                        language.text("Updating…", "更新中…")
                                    } else {
                                        language.text("↻ Update now", "↻ 立即更新")
                                    })
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
                                    div()
                                        .id(format!("remove-{id}"))
                                        .role(Role::Button)
                                        .aria_label(
                                            language
                                                .text("Remove this subscription", "移除这个订阅"),
                                        )
                                        .tab_stop(true)
                                        .focusable()
                                        .cursor_pointer()
                                        .px_2()
                                        .py_1()
                                        .rounded_md()
                                        .text_size(px(10.0))
                                        .text_color(theme.route_trace)
                                        .child(language.text("Remove", "移除"))
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
                                div()
                                    .id(format!("remove-{id}"))
                                    .role(Role::Button)
                                    .aria_label(
                                        language.text("Remove saved node", "移除已保存节点"),
                                    )
                                    .tab_stop(true)
                                    .focusable()
                                    .cursor_pointer()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(theme.outline_subtle)
                                    .bg(theme.surface_high)
                                    .text_size(px(10.0))
                                    .text_color(theme.text_secondary)
                                    .child(language.text("Remove", "移除"))
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
                        div()
                            .id("qx-rule-import")
                            .role(Role::Button)
                            .aria_label(language.text(
                                "Download, validate, and import QX rules",
                                "下载、校验并导入 QX 规则",
                            ))
                            .tab_stop(true)
                            .focusable()
                            .when(!busy, gpui::Styled::cursor_pointer)
                            .h(px(36.0))
                            .px_3()
                            .rounded_md()
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
                            .flex()
                            .items_center()
                            .justify_center()
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
        div()
            .mt_2()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(theme.outline_subtle)
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
                            source.rule_count,
                            source.diagnostic_count,
                            source.target_policy.as_str()
                        )
                    } else {
                        format!(
                            "{} 条 · 跳过 {} 条 · → {} · {last_update}",
                            source.rule_count,
                            source.diagnostic_count,
                            source.target_policy.as_str()
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
                        div()
                            .id(format!("qx-rule-refresh-{refresh_id}"))
                            .role(Role::Button)
                            .aria_label(
                                language.text(
                                    "Update this remote QX rule now",
                                    "立即更新这份远程 QX 规则",
                                ),
                            )
                            .tab_stop(controls_enabled)
                            .focusable()
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
                            .child(if refreshing {
                                language.text("Updating…", "更新中…")
                            } else {
                                language.text("↻ Update now", "↻ 立即更新")
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if controls_enabled {
                                    this.refresh_qx_rule_source(refresh_id.clone(), cx);
                                }
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .id(format!("qx-rule-remove-{index}"))
                            .role(Role::Button)
                            .aria_label(
                                language.text("Delete this remote QX rule", "删除这份远程 QX 规则"),
                            )
                            .tab_stop(controls_enabled)
                            .focusable()
                            .when(controls_enabled, gpui::Styled::cursor_pointer)
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .text_size(px(10.0))
                            .text_color(theme.route_trace)
                            .child(language.text("Delete", "删除"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if controls_enabled {
                                    this.remove_qx_rule_source(id.clone(), cx);
                                }
                            })),
                    ),
            )
    }

    #[allow(clippy::too_many_lines)]
    fn active_rules_panel(&self, theme: Theme, language: Language) -> Stateful<Div> {
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
                                        "Matched from top to bottom; the first hit wins.",
                                        "从上到下匹配；第一条命中后停止。",
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
                            .child(if language == Language::English {
                                format!("{} rules", remote_count + 2)
                            } else {
                                format!("{} 条", remote_count + 2)
                            }),
                    ),
            );

        let mut order = 1;
        for (source_index, source) in self.qx_rule_sources.iter().enumerate() {
            list = list.child(
                div()
                    .mt_4()
                    .mb_1()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(if language == Language::English {
                                format!("Rule source {}", source_index + 1)
                            } else {
                                format!("规则源 {}", source_index + 1)
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme.text_tertiary)
                            .child(format!("→ {}", source.target_policy.as_str())),
                    ),
            );
            let parsed = QxRuleList::parse(&source.content);
            for rule in parsed.rules {
                list = list.child(Self::routing_rule_row(
                    order,
                    Self::qx_rule_kind_label(rule.kind),
                    &rule.value,
                    source.target_policy.as_str(),
                    theme,
                ));
                order += 1;
            }
        }

        if self.qx_rule_sources.is_empty() {
            list = list.child(
                div()
                    .mt_4()
                    .p_4()
                    .rounded_md()
                    .bg(theme.surface_low)
                    .text_color(theme.text_secondary)
                    .child(language.text(
                        "After adding a rule source, DOMAIN, DOMAIN-SUFFIX, and DOMAIN-KEYWORD rules will appear here.",
                        "添加规则源后，这里会逐条显示 DOMAIN、DOMAIN-SUFFIX 和 DOMAIN-KEYWORD 规则。",
                    )),
            );
        }

        list = list
            .child(
                div()
                    .mt_5()
                    .mb_1()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(language.text("System fallback", "系统兜底")),
            )
            .child(Self::routing_rule_row(
                order,
                "GEOIP",
                "CN · no-resolve",
                "DIRECT",
                theme,
            ))
            .child(Self::routing_rule_row(
                order + 1,
                "MATCH",
                language.text("Remaining traffic", "其余流量"),
                "Proxy",
                theme,
            ));
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
        let mut targets = vec!["Proxy".to_owned(), "DIRECT".to_owned()];
        for group in &self.node_policy_groups {
            if !targets.iter().any(|target| target == &group.name) {
                targets.push(group.name.clone());
            }
        }
        targets
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
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            self.qx_rule_feedback =
                QxRuleImportFeedback::StoreFailed(SubscriptionStoreError::DataDirectoryUnavailable);
            self.language()
                .text(
                    "Could not determine where to save rules",
                    "无法确定规则保存位置",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        };
        let url = input.read(cx).value().to_owned();
        let target = self.qx_rule_target_policy.clone();
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
                    let stored =
                        mihomo::save_qx_rule_source_in(&store_dir, &url, &target, &content)
                            .map_err(ImportQxRuleError::Store)?;
                    let apply =
                        SourceRuntimeApply::from_result(runtime.apply_saved_sources(&store_dir));
                    Ok::<_, ImportQxRuleError>((stored, apply))
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
                    Ok((stored, apply)) => {
                        let language = this.language();
                        let rule_count = stored.rule_count;
                        let diagnostic_count = stored.diagnostic_count;
                        let stored_id = stored.id.clone();
                        if let Some(existing) = this
                            .qx_rule_sources
                            .iter_mut()
                            .find(|source| source.id == stored_id)
                        {
                            *existing = stored;
                        } else {
                            this.qx_rule_sources.push(stored);
                        }
                        this.qx_rule_source_refreshes.remove(&stored_id);
                        this.qx_rule_feedback = QxRuleImportFeedback::Imported {
                            rule_count,
                            diagnostic_count,
                        };
                        if let Some(input) = this.qx_rule_input.as_ref() {
                            input.update(cx, SubscriptionTextInput::clear_without_event);
                        }
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
                    }
                    Err(ImportQxRuleError::Download(error)) => {
                        this.qx_rule_feedback = QxRuleImportFeedback::DownloadFailed(error);
                        this.status = format!(
                            "{}: {error}",
                            this.language()
                                .text("QX rule download failed", "QX 规则下载失败")
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
                    }
                    Err(ImportQxRuleError::Store(error)) => {
                        this.qx_rule_feedback = QxRuleImportFeedback::StoreFailed(error);
                        this.status = format!(
                            "{}: {error}",
                            this.language()
                                .text("QX rule save failed", "QX 规则保存失败")
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
                        this.qx_rule_source_refreshes.remove(&id);
                        this.source_refresh_retry_not_before
                            .remove(&super::DueRemoteSource::QxRule(id.clone()).scheduler_key());
                        this.qx_rule_feedback = QxRuleImportFeedback::Idle;
                        let language = this.language();
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
    use super::{Language, source_update_label};

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
