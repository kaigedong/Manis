use gpui::{
    Context, Div, Entity, FontWeight, ParentElement, Role, Stateful, Styled, Window, div,
    prelude::*, px,
};
use relay_core::{ConfigurationSection, WindowSizeClass};

use super::{ImportedSubscriptionState, RelayApp, SubscriptionFeedback};
use crate::{
    diagnostics::{UiEvent, trace_ui},
    mihomo::LoadedProvider,
    subscription::{
        SourceKind, SourceNodePreview, SubscriptionPreview, validate_subscription_preview,
    },
    subscription_input::SubscriptionTextInput,
    theme::Theme,
};

fn source_kind_label(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::HttpSubscription => "HTTP 订阅",
        SourceKind::HttpsSubscription => "HTTPS 订阅",
        SourceKind::VlessNode => "VLESS 节点",
    }
}

const RULE_COUNT: usize = 2;

impl RelayApp {
    pub(super) fn configuration_workspace(
        &self,
        theme: Theme,
        size_class: WindowSizeClass,
        cx: &mut Context<Self>,
    ) -> Div {
        let compact = size_class == WindowSizeClass::Compact;
        let wide = size_class == WindowSizeClass::Wide;
        let mut tabs = div().flex().gap_1();
        for (section, label) in [
            (ConfigurationSection::Sources, "订阅源"),
            (ConfigurationSection::Groups, "策略组"),
            (ConfigurationSection::Rules, "规则顺序"),
        ] {
            tabs = tabs.child(self.configuration_tab(section, label, theme, cx));
        }

        let content = if wide {
            div()
                .id("configuration-wide")
                .flex_1()
                .min_h(px(0.0))
                .p_4()
                .flex()
                .items_start()
                .gap_3()
                .child(self.source_panel(theme, cx).w(px(320.0)).flex_shrink_0())
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(Self::group_panel(theme, cx))
                        .child(self.rule_panel(theme, cx)),
                )
                .child(self.route_probe(theme, false).w(px(280.0)).flex_shrink_0())
        } else {
            let active = match self.configuration.section {
                ConfigurationSection::Sources => self.source_panel(theme, cx),
                ConfigurationSection::Groups => Self::group_panel(theme, cx),
                ConfigurationSection::Rules => self.rule_panel(theme, cx),
            };
            div()
                .id("configuration-scroll")
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .p(if compact { px(12.0) } else { px(16.0) })
                .pb_8()
                .flex()
                .flex_col()
                .gap_3()
                .child(self.configuration_flow_strip(theme, cx))
                .child(active)
                .child(self.route_probe(theme, true))
        };

        let header = Self::configuration_header(theme, compact, wide, tabs);
        div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.surface_base)
            .child(header)
            .child(content)
    }

    fn configuration_header(theme: Theme, compact: bool, wide: bool, tabs: Div) -> Div {
        div()
            .px(if compact { px(12.0) } else { px(20.0) })
            .py_3()
            .flex_shrink_0()
            .border_b_1()
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
                            .flex_1()
                            .child(
                                div()
                                    .text_size(px(19.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child("Operate · 配置工作区"),
                            )
                            .when(!compact, |header| {
                                header.child(
                                    div().mt_1().text_color(theme.text_secondary).child(
                                        "导入来源 → 查看节点 → 编排策略；订阅源安全保存在本机",
                                    ),
                                )
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
                            .child("本机配置"),
                    ),
            )
            .when(wide, |header| header.child(div().mt_3().child(tabs)))
    }

    fn configuration_flow_strip(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let mut strip = div().flex().gap_2();
        for (section, index, label, detail) in [
            (
                ConfigurationSection::Sources,
                "01",
                "代理来源",
                "订阅 / URI",
            ),
            (ConfigurationSection::Groups, "02", "策略组", "3 个出口"),
            (ConfigurationSection::Rules, "03", "规则", "2 条有序"),
        ] {
            strip =
                strip.child(self.configuration_flow_step(section, index, label, detail, theme, cx));
        }
        strip
    }

    #[allow(clippy::too_many_arguments)]
    fn configuration_flow_step(
        &self,
        section: ConfigurationSection,
        index: &'static str,
        label: &'static str,
        detail: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let selected = self.configuration.section == section;
        div()
            .id(format!("configuration-step-{index}"))
            .role(Role::Button)
            .tab_stop(true)
            .focusable()
            .cursor_pointer()
            .min_w(px(0.0))
            .flex_1()
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
                theme.surface_high
            })
            .child(
                div()
                    .text_size(px(10.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(if selected {
                        theme.action_primary
                    } else {
                        theme.text_tertiary
                    })
                    .child(format!("{index} · {label}")),
            )
            .child(
                div()
                    .mt_1()
                    .text_size(px(11.0))
                    .text_color(theme.text_secondary)
                    .child(detail),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                let event = match section {
                    ConfigurationSection::Sources => {
                        this.configuration.select_section(section);
                        this.focus_subscription_input(window, cx);
                        "订阅输入已聚焦 · 导入成功后安全保存在本机".clone_into(&mut this.status);
                        UiEvent::SubscriptionInputFocused
                    }
                    ConfigurationSection::Groups => {
                        this.configuration.select_section(section);
                        this.status = format!("配置预览 · {label}");
                        UiEvent::ConfigurationGroupsOpened
                    }
                    ConfigurationSection::Rules => {
                        this.configuration.select_section(section);
                        this.status = format!("配置预览 · {label}");
                        UiEvent::ConfigurationRulesOpened
                    }
                };
                trace_ui(event);
                cx.notify();
            }))
    }

    fn configuration_tab(
        &self,
        section: ConfigurationSection,
        label: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let selected = self.configuration.section == section;
        div()
            .id(format!("configuration-tab-{label}"))
            .role(Role::Button)
            .tab_stop(true)
            .focusable()
            .cursor_pointer()
            .h(px(32.0))
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
            .text_color(if selected {
                theme.action_primary
            } else {
                theme.text_secondary
            })
            .font_weight(if selected {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::NORMAL
            })
            .flex()
            .items_center()
            .child(label)
            .on_click(cx.listener(move |this, _, window, cx| {
                let event = match section {
                    ConfigurationSection::Sources => {
                        this.configuration.select_section(section);
                        this.focus_subscription_input(window, cx);
                        "订阅输入已聚焦 · 导入成功后安全保存在本机".clone_into(&mut this.status);
                        UiEvent::SubscriptionInputFocused
                    }
                    ConfigurationSection::Groups => {
                        this.configuration.select_section(section);
                        this.status = format!("配置预览 · {label}");
                        UiEvent::ConfigurationGroupsOpened
                    }
                    ConfigurationSection::Rules => {
                        this.configuration.select_section(section);
                        this.status = format!("配置预览 · {label}");
                        UiEvent::ConfigurationRulesOpened
                    }
                };
                trace_ui(event);
                cx.notify();
            }))
    }

    fn focus_subscription_input(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(input) = self.subscription_input.as_ref() {
            let focus_handle = input.read(cx).input_focus_handle();
            window.focus(&focus_handle, cx);
        }
    }

    fn source_panel(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let input = self
            .subscription_input
            .as_ref()
            .expect("subscription input is initialized before rendering")
            .clone();
        let feedback = &self.subscription_feedback;
        let has_imported_subscription = self.imported_subscription.is_some();
        let importing_remote = matches!(
            feedback,
            SubscriptionFeedback::Importing(_)
                | SubscriptionFeedback::PreviewFailed(_)
                | SubscriptionFeedback::StoreFailed(_)
        );
        let displayed_providers = if has_imported_subscription || importing_remote {
            self.subscription_preview_providers.as_slice()
        } else {
            self.source_providers.as_slice()
        };
        let direct_input = input.read(cx).value().starts_with("vless://");
        let busy = matches!(feedback, SubscriptionFeedback::Importing(_))
            || matches!(
                self.imported_subscription_state,
                ImportedSubscriptionState::Removing(_)
            );

        let panel = div()
            .id("configuration-source")
            .w_full()
            .min_h(px(330.0))
            .p_4()
            .rounded_md()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .child(
                div()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_tertiary)
                    .child("01 · 代理来源"),
            )
            .child(
                div()
                    .mt_3()
                    .text_size(px(17.0))
                    .font_weight(FontWeight::BOLD)
                    .child("管理代理来源"),
            )
            .child(
                div()
                    .mt_1()
                    .text_size(px(11.0))
                    .text_color(theme.text_secondary)
                    .child("支持 HTTP/HTTPS 订阅，也支持单个 vless:// 节点链接。"),
            )
            .child(
                div()
                    .mt_4()
                    .mb_1()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("来源地址"),
            )
            .child(input.clone())
            .child(Self::subscription_actions(
                input,
                busy,
                direct_input,
                theme,
                cx,
            ))
            .child(Self::subscription_feedback(
                feedback,
                &self.subscription_preview_providers,
                has_imported_subscription,
                theme,
            ))
            .child(self.imported_subscription_card(theme, cx))
            .child(Self::source_nodes(
                feedback,
                self.imported_subscription_state,
                has_imported_subscription,
                displayed_providers,
                theme,
            ))
            .child(self.source_panel_footer(has_imported_subscription, theme));
        div().w_full().child(panel)
    }

    fn source_panel_footer(&self, has_imported_subscription: bool, theme: Theme) -> Div {
        let source = self.runtime.profile_source();
        div()
            .mt_3()
            .pt_3()
            .border_t_1()
            .border_color(theme.outline_subtle)
            .text_size(px(10.0))
            .text_color(theme.text_tertiary)
            .child(if has_imported_subscription {
                "订阅已保存到本机私有用户数据目录 · 调试日志不记录链接"
            } else {
                "导入成功后持久保存 · 调试日志不记录链接"
            })
            .child(div().mt_2().child(format!(
                "当前运行来源 · {} · {}",
                source.label(),
                source.detail()
            )))
    }

    fn subscription_actions(
        input: Entity<SubscriptionTextInput>,
        busy: bool,
        direct_input: bool,
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
                        "预览 VLESS 节点"
                    } else {
                        "验证并导入订阅"
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
                        "正在处理…"
                    } else if direct_input {
                        "预览 VLESS 节点"
                    } else {
                        "导入订阅"
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
                    .aria_label("清除订阅链接草稿")
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
                    .child("清除")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if busy {
                            return;
                        }
                        clear_input.update(cx, SubscriptionTextInput::clear);
                        this.subscription_feedback = SubscriptionFeedback::Idle;
                        "已清除订阅链接草稿".clone_into(&mut this.status);
                        trace_ui(UiEvent::SubscriptionDraftCleared);
                        cx.notify();
                    })),
            )
    }

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
                self.subscription_feedback = SubscriptionFeedback::Valid(preview);
                "VLESS 节点已识别 · 可在来源节点中查看 · 尚未保存".clone_into(&mut self.status);
                trace_ui(UiEvent::SourceRecognitionSucceeded);
                cx.notify();
            }
            Ok(preview) => {
                trace_ui(UiEvent::SourceRecognitionSucceeded);
                self.import_remote_subscription(input_value, preview.kind, cx);
            }
            Err(error) => {
                self.subscription_feedback = SubscriptionFeedback::InvalidInput(error);
                self.status = format!("来源识别失败：{error}");
                trace_ui(UiEvent::SourceRecognitionFailed);
                cx.notify();
            }
        }
    }

    fn subscription_feedback(
        feedback: &SubscriptionFeedback,
        providers: &[LoadedProvider],
        has_imported_subscription: bool,
        theme: Theme,
    ) -> Div {
        match feedback {
            SubscriptionFeedback::Idle => div()
                .mt_3()
                .text_size(px(11.0))
                .text_color(theme.text_secondary)
                .child(if has_imported_subscription {
                    "粘贴新的 HTTP/HTTPS 订阅可验证后替换；vless:// 仍提供安全预览"
                } else {
                    "等待输入 · HTTP/HTTPS 订阅或 vless:// 节点"
                }),
            SubscriptionFeedback::Importing(kind) => Self::subscription_loading(*kind, theme),
            SubscriptionFeedback::Valid(preview) => {
                Self::subscription_valid(preview, providers, theme)
            }
            SubscriptionFeedback::InvalidInput(error) => {
                Self::subscription_error("无法识别来源", error.to_string(), None, theme)
            }
            SubscriptionFeedback::PreviewFailed(error) => Self::subscription_error(
                "无法读取订阅节点",
                error.to_string(),
                Some("链接仍保留在输入框中；检查后可再次读取。"),
                theme,
            ),
            SubscriptionFeedback::StoreFailed(error) => Self::subscription_error(
                "节点有效，但无法保存订阅",
                error.to_string(),
                Some("旧的已导入订阅没有被替换；检查目录权限后重试。"),
                theme,
            ),
        }
    }

    fn subscription_loading(kind: SourceKind, theme: Theme) -> Div {
        let title = match kind {
            SourceKind::HttpSubscription => "正在验证并导入 HTTP 订阅",
            SourceKind::HttpsSubscription => "正在验证并导入 HTTPS 订阅",
            SourceKind::VlessNode => "正在解析 VLESS 节点",
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
                    .child("隔离的 Mihomo 正在解析节点；成功后才会原子保存。"),
            )
    }

    fn subscription_valid(
        preview: &SubscriptionPreview,
        providers: &[LoadedProvider],
        theme: Theme,
    ) -> Div {
        let (title, detail) = match preview.kind {
            SourceKind::HttpSubscription => (
                "HTTP 订阅预览完成",
                "节点已实际读取；HTTP 明文传输可能暴露订阅凭据",
            ),
            SourceKind::HttpsSubscription => (
                "HTTPS 订阅预览完成",
                "节点已由 Mihomo 实际下载并解析，可在下方完整浏览",
            ),
            SourceKind::VlessNode => ("VLESS 节点已识别", "已解析为 1 个可预览的直接节点"),
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
                        .child(format!("{} 个来源 · {node_count} 个节点", providers.len())),
                )
            })
    }

    fn imported_subscription_card(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let node_count: usize = self
            .subscription_preview_providers
            .iter()
            .map(|provider| provider.nodes.len())
            .sum();
        let (title, detail, busy) = match self.imported_subscription_state {
            ImportedSubscriptionState::None => return div(),
            ImportedSubscriptionState::Pending(_) | ImportedSubscriptionState::Refreshing(_) => (
                "正在恢复已导入订阅",
                "订阅已保存在本机；正在重新读取节点。".to_owned(),
                true,
            ),
            ImportedSubscriptionState::Ready(kind) => (
                "订阅已导入",
                format!(
                    "{} · {} 个来源 · {node_count} 个节点 · 重启后自动恢复",
                    source_kind_label(kind),
                    self.subscription_preview_providers.len()
                ),
                false,
            ),
            ImportedSubscriptionState::Unavailable(kind, error) => (
                "订阅已保存 · 节点刷新失败",
                format!(
                    "{} · {error}；稍后可粘贴原链接重新导入。",
                    source_kind_label(kind)
                ),
                false,
            ),
            ImportedSubscriptionState::StoreError(error) => (
                "已保存订阅不可用",
                format!("{error}；重新导入会尝试安全替换。"),
                false,
            ),
            ImportedSubscriptionState::Removing(_) => (
                "正在移除订阅",
                "正在删除本机保存的订阅来源。".to_owned(),
                true,
            ),
        };
        let operation_busy = busy
            || matches!(
                self.subscription_feedback,
                SubscriptionFeedback::Importing(_)
            );
        let removable = self.imported_subscription.is_some() && !operation_busy;

        div()
            .mt_3()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.action_soft)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.status_success)
                            .child(title),
                    )
                    .when(removable, |header| {
                        header.child(
                            div()
                                .id("remove-imported-subscription")
                                .role(Role::Button)
                                .aria_label("移除已导入订阅")
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
                                .child("移除订阅")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.remove_imported_subscription(cx);
                                })),
                        )
                    }),
            )
            .child(
                div()
                    .mt_1()
                    .text_size(px(11.0))
                    .text_color(theme.text_secondary)
                    .child(detail),
            )
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

    fn source_nodes(
        feedback: &SubscriptionFeedback,
        imported_state: ImportedSubscriptionState,
        has_imported_subscription: bool,
        providers: &[LoadedProvider],
        theme: Theme,
    ) -> Div {
        let direct_nodes = match feedback {
            SubscriptionFeedback::Valid(preview) if preview.kind == SourceKind::VlessNode => {
                Some(preview.nodes.as_slice())
            }
            _ => None,
        };
        let total = direct_nodes.map_or_else(
            || providers.iter().map(|provider| provider.nodes.len()).sum(),
            <[SourceNodePreview]>::len,
        );
        let remote_preview = has_imported_subscription
            || matches!(
                feedback,
                SubscriptionFeedback::Importing(_)
                    | SubscriptionFeedback::Valid(SubscriptionPreview {
                        kind: SourceKind::HttpSubscription | SourceKind::HttpsSubscription,
                        ..
                    })
                    | SubscriptionFeedback::PreviewFailed(_)
                    | SubscriptionFeedback::StoreFailed(_)
            );
        let loading = matches!(feedback, SubscriptionFeedback::Importing(_))
            || matches!(
                imported_state,
                ImportedSubscriptionState::Pending(_) | ImportedSubscriptionState::Refreshing(_)
            );
        let list_title = if has_imported_subscription {
            "已导入节点"
        } else if remote_preview {
            "订阅节点"
        } else if direct_nodes.is_none() && !providers.is_empty() {
            "Mihomo 当前节点"
        } else {
            "来源节点"
        };
        let mut section = div()
            .mt_3()
            .pt_3()
            .border_t_1()
            .border_color(theme.outline_subtle)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(list_title),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme.text_tertiary)
                            .child(if loading {
                                "读取中".to_owned()
                            } else {
                                format!("{total} 个")
                            }),
                    ),
            );

        if let Some(nodes) = direct_nodes {
            for node in nodes {
                section = section.child(Self::direct_node_row(node, theme));
            }
            return section;
        }

        if providers.is_empty() {
            return section.child(Self::empty_source_nodes(
                feedback,
                has_imported_subscription,
                loading,
                theme,
            ));
        }

        let mut list = div()
            .id("source-node-list")
            .mt_2()
            .max_h(px(280.0))
            .overflow_y_scroll();
        for provider in providers {
            list = list.child(Self::provider_block(provider, theme));
        }
        section.child(list)
    }

    fn empty_source_nodes(
        feedback: &SubscriptionFeedback,
        has_imported_subscription: bool,
        loading: bool,
        theme: Theme,
    ) -> Div {
        let copy = if loading {
            "正在等待 Mihomo 返回节点列表…"
        } else {
            match feedback {
                SubscriptionFeedback::PreviewFailed(_) => {
                    "没有新节点可展示；旧的已导入订阅仍然保留。"
                }
                SubscriptionFeedback::Valid(_) => "订阅没有返回可展示的代理节点。",
                _ if has_imported_subscription => "订阅已经保存，但当前没有可展示的节点。",
                _ => "导入订阅后，这里会显示它包含的全部节点。",
            }
        };
        div()
            .mt_2()
            .p_3()
            .rounded_md()
            .bg(theme.surface_low)
            .text_size(px(11.0))
            .text_color(theme.text_secondary)
            .child(copy)
    }

    fn direct_node_row(node: &SourceNodePreview, theme: Theme) -> Div {
        div()
            .mt_2()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_low)
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(node.name.clone()),
            )
            .child(
                div()
                    .mt_1()
                    .text_size(px(10.0))
                    .text_color(theme.text_secondary)
                    .child(format!(
                        "{} · {} · {}",
                        node.protocol, node.endpoint, node.detail
                    )),
            )
    }

    fn provider_block(provider: &LoadedProvider, theme: Theme) -> Div {
        let mut block = div()
            .mb_2()
            .rounded_md()
            .border_1()
            .border_color(theme.outline_subtle);
        block = block.child(
            div()
                .px_3()
                .py_2()
                .bg(theme.surface_low)
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(provider.name.clone()),
                )
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(theme.text_tertiary)
                        .child(format!("{} 个节点", provider.nodes.len())),
                ),
        );
        for node in &provider.nodes {
            let state = match (node.alive, node.latency_label.as_ref()) {
                (Some(false), _) => "不可用".to_owned(),
                (_, Some(latency)) => latency.clone(),
                _ => "未测速".to_owned(),
            };
            block = block.child(
                div()
                    .px_3()
                    .py_2()
                    .border_t_1()
                    .border_color(theme.outline_subtle)
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .child(node.name.clone())
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(theme.text_tertiary)
                                    .child(node.protocol.clone()),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(if node.alive == Some(false) {
                                theme.text_tertiary
                            } else {
                                theme.status_success
                            })
                            .child(state),
                    ),
            );
        }
        block
    }

    fn group_panel(theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
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
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_tertiary)
                            .child("02 · 策略组"),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme.text_tertiary)
                            .child("QX 风格预设"),
                    ),
            )
            .child(Self::group_row(
                "Auto",
                "URL Test",
                "订阅节点 · 自动优选",
                false,
                theme,
                cx,
            ))
            .child(Self::group_row(
                "Proxy",
                "Select",
                "Auto / DIRECT / 订阅节点",
                true,
                theme,
                cx,
            ))
            .child(Self::group_row(
                "DIRECT",
                "Builtin",
                "内置直连出口",
                false,
                theme,
                cx,
            ))
    }

    fn group_row(
        name: &'static str,
        kind: &'static str,
        detail: &'static str,
        primary: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id(format!("configuration-group-{name}"))
            .role(Role::Button)
            .tab_stop(true)
            .focusable()
            .cursor_pointer()
            .mt_2()
            .min_h(px(54.0))
            .px_3()
            .rounded_md()
            .border_1()
            .border_color(if primary {
                theme.action_primary
            } else {
                theme.outline_subtle
            })
            .bg(if primary {
                theme.action_soft
            } else {
                theme.surface_low
            })
            .flex()
            .items_center()
            .gap_3()
            .child(div().w(px(3.0)).h(px(30.0)).rounded_full().bg(if primary {
                theme.action_primary
            } else {
                theme.outline_strong
            }))
            .child(
                div()
                    .flex_1()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(name))
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(11.0))
                            .text_color(theme.text_secondary)
                            .child(detail),
                    ),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(theme.text_tertiary)
                    .child(kind),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.configuration
                    .select_section(ConfigurationSection::Groups);
                trace_ui(UiEvent::PolicyPreviewOpened);
                this.status = format!("策略组预览 · {name} · 尚未写入 Mihomo");
                cx.notify();
            }))
    }

    fn rule_panel(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
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
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_tertiary)
                            .child("03 · 规则顺序"),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme.text_tertiary)
                            .child("从上到下，首条命中"),
                    ),
            )
            .child(self.rule_row(0, "GEOIP", "CN · no-resolve", "DIRECT", theme, cx))
            .child(self.rule_row(1, "MATCH", "其余流量", "Proxy", theme, cx))
    }

    #[allow(clippy::too_many_arguments)]
    fn rule_row(
        &self,
        index: usize,
        kind: &'static str,
        payload: &'static str,
        policy: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let selected = self.configuration.selected_rule == index;
        div()
            .id(format!("configuration-rule-{index}"))
            .role(Role::Button)
            .tab_stop(true)
            .focusable()
            .cursor_pointer()
            .mt_2()
            .min_h(px(50.0))
            .px_3()
            .rounded_md()
            .border_1()
            .border_color(if selected {
                theme.route_trace
            } else {
                theme.outline_subtle
            })
            .bg(theme.surface_low)
            .flex()
            .items_center()
            .gap_3()
            .child(
                div()
                    .w(px(30.0))
                    .text_color(theme.text_tertiary)
                    .child(format!("#{:02}", index + 1)),
            )
            .child(
                div()
                    .flex_1()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(kind))
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(11.0))
                            .text_color(theme.text_secondary)
                            .child(payload),
                    ),
            )
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(if selected {
                        theme.route_trace
                    } else {
                        theme.text_primary
                    })
                    .child(policy),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.configuration.select_rule(index, RULE_COUNT);
                trace_ui(UiEvent::RulePreviewOpened);
                this.status = format!("规则 #{:02} → {policy} · 配置预览", index + 1);
                cx.notify();
            }))
    }

    fn route_probe(&self, theme: Theme, condensed: bool) -> Div {
        let (rule, policy, exit) = if self.configuration.selected_rule == 0 {
            ("#01 · GEOIP, CN", "DIRECT", "内置直连")
        } else {
            ("#02 · MATCH", "Proxy", "Auto / 订阅节点")
        };
        let stages = if condensed {
            div()
                .mt_3()
                .flex()
                .items_center()
                .gap_2()
                .child(Self::probe_chip("规则", rule, theme))
                .child(
                    div()
                        .w(px(22.0))
                        .h(px(2.0))
                        .flex_shrink_0()
                        .bg(theme.route_trace),
                )
                .child(Self::probe_chip("策略", policy, theme))
                .child(
                    div()
                        .w(px(22.0))
                        .h(px(2.0))
                        .flex_shrink_0()
                        .bg(theme.route_trace),
                )
                .child(Self::probe_chip("出口", exit, theme))
        } else {
            div()
                .child(Self::probe_stage("规则", rule, true, theme))
                .child(Self::probe_stage("策略", policy, true, theme))
                .child(Self::probe_stage("出口", exit, false, theme))
        };

        div()
            .p_4()
            .rounded_md()
            .border_1()
            .border_color(theme.route_trace)
            .bg(theme.surface_high)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.route_trace)
                            .child("ROUTE PROBE"),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .bg(theme.route_soft)
                            .text_size(px(10.0))
                            .text_color(theme.route_trace)
                            .child("本地配置预览"),
                    ),
            )
            .child(stages)
            .child(
                div()
                    .mt_3()
                    .pt_3()
                    .border_t_1()
                    .border_color(theme.outline_subtle)
                    .text_size(px(11.0))
                    .text_color(theme.text_secondary)
                    .child("铜色只表示依赖路径；这不是 Mihomo 的实时命中结果。"),
            )
    }

    fn probe_chip(label: &'static str, value: &'static str, theme: Theme) -> Div {
        div()
            .min_w(px(0.0))
            .flex_1()
            .p_2()
            .rounded_md()
            .bg(theme.route_soft)
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(theme.route_trace)
                    .child(label),
            )
            .child(div().mt_1().font_weight(FontWeight::SEMIBOLD).child(value))
    }

    fn probe_stage(label: &'static str, value: &'static str, tail: bool, theme: Theme) -> Div {
        div()
            .mt_3()
            .flex()
            .gap_3()
            .child(
                div()
                    .w(px(18.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(div().size(px(10.0)).rounded_full().bg(theme.route_trace))
                    .when(tail, |rail| {
                        rail.child(div().mt_1().w(px(2.0)).h(px(28.0)).bg(theme.route_trace))
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme.text_tertiary)
                            .child(label),
                    )
                    .child(div().mt_1().font_weight(FontWeight::SEMIBOLD).child(value)),
            )
    }
}
