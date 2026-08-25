use gpui::{
    Context, Div, Entity, FontWeight, ParentElement, Role, Stateful, Styled, Window, div,
    prelude::*, px,
};
use relay_core::{ConfigurationSection, WindowSizeClass};

use super::{
    ImportQxRuleError, ImportedSubscriptionState, QxRuleImportFeedback, QxRuleList, RelayApp,
    SourceRuntimeApply, SubscriptionFeedback,
};
use crate::{
    diagnostics::{UiEvent, trace_ui},
    mihomo::{self, LoadedProvider, SubscriptionStoreError},
    rule_source::download_qx_rule_document,
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
                .child(if compact {
                    self.compact_route_summary(theme)
                } else {
                    self.route_probe(theme, true)
                })
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
        let rule_count = RULE_COUNT
            + self
                .qx_rule_sources
                .iter()
                .map(|source| source.rule_count)
                .sum::<usize>();
        for (section, index, label, detail) in [
            (
                ConfigurationSection::Sources,
                "01",
                "代理来源",
                "订阅 / URI".to_owned(),
            ),
            (
                ConfigurationSection::Groups,
                "02",
                "策略组",
                "3 个出口".to_owned(),
            ),
            (
                ConfigurationSection::Rules,
                "03",
                "规则",
                format!("{rule_count} 条有序"),
            ),
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
        detail: String,
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

    #[allow(clippy::too_many_lines)]
    fn source_panel(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let input = self
            .subscription_input
            .as_ref()
            .expect("subscription input is initialized before rendering")
            .clone();
        let feedback = &self.subscription_feedback;
        let has_imported_subscription = !self.imported_subscriptions.is_empty();
        let importing_remote = matches!(
            feedback,
            SubscriptionFeedback::Importing(_)
                | SubscriptionFeedback::PreviewFailed(_)
                | SubscriptionFeedback::StoreFailed(_)
        );
        let imported_providers: Vec<_> = self
            .imported_subscriptions
            .iter()
            .flat_map(|subscription| subscription.providers.iter().cloned())
            .collect();
        let displayed_providers = if importing_remote {
            self.subscription_preview_providers.clone()
        } else if has_imported_subscription {
            imported_providers
        } else {
            self.source_providers.clone()
        };
        let direct_input = input.read(cx).value().starts_with("vless://");
        let busy = matches!(feedback, SubscriptionFeedback::Importing(_))
            || self.imported_subscriptions.iter().any(|subscription| {
                matches!(subscription.state, ImportedSubscriptionState::Removing(_))
            });
        let imported_loading = self.imported_subscriptions.iter().any(|subscription| {
            matches!(
                subscription.state,
                ImportedSubscriptionState::Pending(_) | ImportedSubscriptionState::Refreshing(_)
            )
        });

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
            .when_some(self.source_store_error, |panel, error| {
                panel.child(Self::subscription_error(
                    "部分本地来源未能恢复",
                    error.to_string(),
                    Some("其余可安全读取的来源仍然保留；可检查用户数据目录权限。"),
                    theme,
                ))
            })
            .child(self.imported_subscription_cards(theme, cx))
            .child(self.saved_vless_cards(theme, cx))
            .child(Self::source_nodes(
                feedback,
                imported_loading,
                has_imported_subscription,
                &displayed_providers,
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
                "多个来源分别保存到本机私有用户数据目录 · 调试日志不记录链接"
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
                        "保存 VLESS 节点"
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
                        "保存 VLESS 节点"
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
                let Some(store_dir) = self.subscription_store_dir.clone() else {
                    self.subscription_feedback = SubscriptionFeedback::StoreFailed(
                        SubscriptionStoreError::DataDirectoryUnavailable,
                    );
                    "无法确定节点保存位置".clone_into(&mut self.status);
                    trace_ui(UiEvent::SourceImportFailed);
                    cx.notify();
                    return;
                };
                self.subscription_preview_generation =
                    self.subscription_preview_generation.wrapping_add(1);
                let generation = self.subscription_preview_generation;
                self.subscription_feedback = SubscriptionFeedback::Importing(SourceKind::VlessNode);
                "正在保存并编译 VLESS 节点".clone_into(&mut self.status);
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
                                this.status = format!(
                                    "VLESS 节点已保存 · 已加入“已保存”分组{}",
                                    apply.status_suffix()
                                );
                                trace_ui(UiEvent::SourceImportSucceeded);
                            }
                            Err(error) => {
                                this.subscription_feedback =
                                    SubscriptionFeedback::StoreFailed(error);
                                this.status = format!("VLESS 节点保存失败：{error}");
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
                    "可继续添加 HTTP/HTTPS 订阅，或保存单个 vless:// 节点"
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
                "来源有效，但无法保存",
                error.to_string(),
                Some("现有来源未受影响；检查目录权限后重试。"),
                theme,
            ),
        }
    }

    fn subscription_loading(kind: SourceKind, theme: Theme) -> Div {
        let title = match kind {
            SourceKind::HttpSubscription => "正在验证并导入 HTTP 订阅",
            SourceKind::HttpsSubscription => "正在验证并导入 HTTPS 订阅",
            SourceKind::VlessNode => "正在保存 VLESS 节点",
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
            SourceKind::VlessNode => ("VLESS 节点已保存", "已加入节点页的“已保存”分组"),
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

    #[allow(clippy::too_many_lines)]
    fn imported_subscription_cards(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let mut list = div();
        for (index, subscription) in self.imported_subscriptions.iter().enumerate() {
            let node_count: usize = subscription
                .providers
                .iter()
                .map(|provider| provider.nodes.len())
                .sum();
            let name = subscription
                .source
                .subscription_name()
                .unwrap_or_else(|| format!("订阅 {}", index + 1));
            let (detail, busy, healthy) = match subscription.state {
                ImportedSubscriptionState::None => continue,
                ImportedSubscriptionState::Pending(_)
                | ImportedSubscriptionState::Refreshing(_) => {
                    ("已安全保存 · 正在恢复节点".to_owned(), true, true)
                }
                ImportedSubscriptionState::Ready(kind) => (
                    format!(
                        "{} · {} 个来源 · {node_count} 个节点 · 重启后自动恢复",
                        source_kind_label(kind),
                        subscription.providers.len()
                    ),
                    false,
                    true,
                ),
                ImportedSubscriptionState::Unavailable(kind, error) => (
                    format!("{} · {error} · 可稍后刷新", source_kind_label(kind)),
                    false,
                    false,
                ),
                ImportedSubscriptionState::StoreError(error) => {
                    (format!("{error} · 本地来源没有被删除"), false, false)
                }
                ImportedSubscriptionState::Removing(_) => {
                    ("正在删除本机保存的订阅".to_owned(), true, true)
                }
            };
            let removable = !busy
                && !matches!(
                    self.subscription_feedback,
                    SubscriptionFeedback::Importing(_)
                );
            let id = subscription.id.clone();
            list = list.child(
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
                            .when(removable, |header| {
                                header.child(
                                    div()
                                        .id(format!("remove-{id}"))
                                        .role(Role::Button)
                                        .aria_label("移除这个订阅")
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
                                        .child("移除")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.remove_imported_subscription(id.clone(), cx);
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
                    ),
            );
        }
        list
    }

    #[allow(clippy::too_many_lines)]
    fn saved_vless_cards(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
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
                                    .aria_label("移除已保存节点")
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
                                    .child("移除")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        let Some(store_dir) = this.subscription_store_dir.clone()
                                        else {
                                            "无法确定节点保存位置".clone_into(&mut this.status);
                                            cx.notify();
                                            return;
                                        };
                                        let runtime = this.runtime.clone();
                                        let remove_id = id.clone();
                                        "正在移除保存的 VLESS 节点".clone_into(&mut this.status);
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
                                                        this.status = format!(
                                                            "已移除保存的 VLESS 节点{}",
                                                            apply.status_suffix()
                                                        );
                                                    }
                                                    Err(error) => {
                                                        this.status =
                                                            format!("移除节点失败：{error}");
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

    fn source_nodes(
        feedback: &SubscriptionFeedback,
        imported_loading: bool,
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
        let loading = matches!(feedback, SubscriptionFeedback::Importing(_)) || imported_loading;
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

    #[allow(clippy::too_many_lines)]
    fn rule_panel(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let input = self
            .qx_rule_input
            .as_ref()
            .expect("QX rule input is initialized before rendering")
            .clone();
        let busy = self.qx_rule_feedback == QxRuleImportFeedback::Importing;
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
            .child(
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
                            .items_start()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .child(
                                        div()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child("导入远程 QX 规则"),
                                    )
                                    .child(
                                        div()
                                            .mt_1()
                                            .text_size(px(11.0))
                                            .text_color(theme.text_secondary)
                                            .child(
                                                "下载后解析 DOMAIN / DOMAIN-SUFFIX / DOMAIN-KEYWORD，并统一映射到目标策略。",
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .id("qx-rule-target-policy")
                                    .role(Role::Button)
                                    .aria_label("切换 QX 规则目标策略")
                                    .tab_stop(true)
                                    .focusable()
                                    .when(!busy, gpui::Styled::cursor_pointer)
                                    .h(px(34.0))
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
                                    .child(format!("目标 · {}  ↻", self.qx_rule_target_policy))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if busy {
                                            return;
                                        }
                                        this.cycle_qx_rule_target();
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .mt_3()
                            .text_size(px(10.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_tertiary)
                            .child("HTTPS 规则地址"),
                    )
                    .child(input.clone())
                    .child(
                        div()
                            .mt_2()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .id("qx-rule-import")
                                    .role(Role::Button)
                                    .aria_label("下载、校验并导入 QX 规则")
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
                                    .child(if busy { "正在下载并解析…" } else { "导入规则" })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if busy {
                                            return;
                                        }
                                        this.submit_qx_rule_import(&input, cx);
                                    })),
                            ),
                    )
                    .child(self.qx_rule_import_feedback(theme))
                    .children(self.qx_rule_sources.iter().enumerate().map(|(index, source)| {
                        let id = source.id.clone();
                        let removing = busy;
                        div()
                            .mt_2()
                            .px_3()
                            .py_2()
                            .rounded_md()
                            .border_1()
                            .border_color(theme.outline_subtle)
                            .bg(theme.surface_high)
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .size(px(24.0))
                                    .rounded_full()
                                    .bg(theme.action_soft)
                                    .text_color(theme.action_primary)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .font_weight(FontWeight::BOLD)
                                    .child("R"),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .child(
                                        div()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(format!("远程规则 {:02}", index + 1)),
                                    )
                                    .child(
                                        div()
                                            .mt_1()
                                            .text_size(px(10.0))
                                            .text_color(theme.text_secondary)
                                            .child(format!(
                                                "{} 条规则 · {} 条跳过 · → {}",
                                                source.rule_count,
                                                source.diagnostic_count,
                                                source.target_policy.as_str()
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .id(format!("qx-rule-remove-{index}"))
                                    .role(Role::Button)
                                    .aria_label("删除这份远程 QX 规则")
                                    .tab_stop(true)
                                    .focusable()
                                    .when(!removing, gpui::Styled::cursor_pointer)
                                    .px_2()
                                    .py_1()
                                    .rounded_sm()
                                    .text_size(px(10.0))
                                    .text_color(theme.text_secondary)
                                    .child("移除")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if !removing {
                                            this.remove_qx_rule_source(id.clone(), cx);
                                        }
                                    })),
                            )
                    }))
                    .child(
                        div()
                            .mt_3()
                            .text_size(px(10.0))
                            .text_color(theme.text_tertiary)
                            .child("规则正文和地址只保存在本机私有目录；日志不会记录链接。"),
                    ),
            )
            .child(self.rule_row(0, "GEOIP", "CN · no-resolve", "DIRECT", theme, cx))
            .child(self.rule_row(1, "MATCH", "其余流量", "Proxy", theme, cx))
    }

    fn qx_rule_import_feedback(&self, theme: Theme) -> Div {
        let (message, color) = match &self.qx_rule_feedback {
            QxRuleImportFeedback::Idle => (
                "只接受 HTTPS · 最多 1 MiB · 无效行会单独计数".to_owned(),
                theme.text_secondary,
            ),
            QxRuleImportFeedback::Importing => (
                "正在安全下载、解析并写入本机…".to_owned(),
                theme.action_primary,
            ),
            QxRuleImportFeedback::Imported {
                rule_count,
                diagnostic_count,
            } => (
                if *diagnostic_count == 0 {
                    format!("已导入 {rule_count} 条规则")
                } else {
                    format!("已导入 {rule_count} 条规则 · 跳过 {diagnostic_count} 条无效行")
                },
                theme.status_success,
            ),
            QxRuleImportFeedback::InvalidDocument => (
                "文件已下载，但没有可识别的 QX 域名规则".to_owned(),
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
        self.status = format!("QX 规则目标已切换为 {}", self.qx_rule_target_policy);
    }

    fn submit_qx_rule_import(
        &mut self,
        input: &Entity<SubscriptionTextInput>,
        cx: &mut Context<Self>,
    ) {
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            self.qx_rule_feedback =
                QxRuleImportFeedback::StoreFailed(SubscriptionStoreError::DataDirectoryUnavailable);
            "无法确定规则保存位置".clone_into(&mut self.status);
            cx.notify();
            return;
        };
        let url = input.read(cx).value().to_owned();
        let target = self.qx_rule_target_policy.clone();
        self.qx_rule_import_generation = self.qx_rule_import_generation.wrapping_add(1);
        let generation = self.qx_rule_import_generation;
        self.qx_rule_feedback = QxRuleImportFeedback::Importing;
        "正在下载并解析 QX 规则".clone_into(&mut self.status);
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
                        let rule_count = stored.rule_count;
                        let diagnostic_count = stored.diagnostic_count;
                        if let Some(existing) = this
                            .qx_rule_sources
                            .iter_mut()
                            .find(|source| source.id == stored.id)
                        {
                            *existing = stored;
                        } else {
                            this.qx_rule_sources.push(stored);
                        }
                        this.qx_rule_feedback = QxRuleImportFeedback::Imported {
                            rule_count,
                            diagnostic_count,
                        };
                        if let Some(input) = this.qx_rule_input.as_ref() {
                            input.update(cx, SubscriptionTextInput::clear_without_event);
                        }
                        this.status = format!(
                            "QX 规则已导入 · {rule_count} 条生效{}",
                            apply.status_suffix()
                        );
                    }
                    Err(ImportQxRuleError::Download(error)) => {
                        this.qx_rule_feedback = QxRuleImportFeedback::DownloadFailed(error);
                        this.status = format!("QX 规则下载失败：{error}");
                    }
                    Err(ImportQxRuleError::InvalidDocument) => {
                        this.qx_rule_feedback = QxRuleImportFeedback::InvalidDocument;
                        "QX 规则未导入：没有可识别的域名规则".clone_into(&mut this.status);
                    }
                    Err(ImportQxRuleError::Store(error)) => {
                        this.qx_rule_feedback = QxRuleImportFeedback::StoreFailed(error);
                        this.status = format!("QX 规则保存失败：{error}");
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
        "正在移除远程 QX 规则".clone_into(&mut self.status);
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
                        this.qx_rule_feedback = QxRuleImportFeedback::Idle;
                        this.status = format!("远程 QX 规则已移除{}", apply.status_suffix());
                    }
                    Err(error) => {
                        this.qx_rule_feedback = QxRuleImportFeedback::StoreFailed(error);
                        this.status = format!("远程 QX 规则移除失败：{error}");
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
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

    fn compact_route_summary(&self, theme: Theme) -> Div {
        let (rule, policy) = if self.configuration.selected_rule == 0 {
            ("#01 · GEOIP, CN", "DIRECT")
        } else {
            ("#02 · MATCH", "Proxy")
        };
        div()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(theme.route_trace)
            .bg(theme.surface_high)
            .flex()
            .items_center()
            .gap_2()
            .child(div().size(px(8.0)).rounded_full().bg(theme.route_trace))
            .child(
                div()
                    .text_size(px(10.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.route_trace)
                    .child("本地配置预览"),
            )
            .child(div().text_size(px(11.0)).child(rule))
            .child(div().text_color(theme.route_trace).child("→"))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(policy))
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
