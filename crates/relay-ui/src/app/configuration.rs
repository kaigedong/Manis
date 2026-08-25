use gpui::{
    Context, Div, Entity, FontWeight, ParentElement, Role, Stateful, Styled, div, prelude::*, px,
};
use relay_core::WindowSizeClass;
use relay_profile::QxRuleKind;

use super::{
    ImportQxRuleError, ImportedSubscriptionState, QxRuleImportFeedback, QxRuleList, RelayApp,
    SourceRuntimeApply, SubscriptionFeedback,
};
use crate::{
    diagnostics::{UiEvent, trace_ui},
    mihomo::{self, LoadedProvider, SubscriptionStoreError},
    rule_source::download_qx_rule_document,
    subscription::{SourceKind, SubscriptionPreview, validate_subscription_preview},
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

impl RelayApp {
    pub(super) fn configuration_workspace(
        &self,
        theme: Theme,
        size_class: WindowSizeClass,
        cx: &mut Context<Self>,
    ) -> Div {
        let compact = size_class == WindowSizeClass::Compact;
        let rule_input = self
            .qx_rule_input
            .as_ref()
            .expect("QX rule input is initialized before rendering")
            .clone();
        let rule_busy = self.qx_rule_feedback == QxRuleImportFeedback::Importing;
        div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.surface_base)
            .child(Self::workspace_header(
                "配置",
                "只管理来源；节点与最终生效规则在各自页面查看",
                "本机私有",
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
                            .child(self.source_panel(theme, cx))
                            .child(
                                self.rule_source_manager(rule_input, rule_busy, theme, cx)
                                    .mt_4(),
                            ),
                    ),
            )
    }

    pub(super) fn routing_rules_workspace(
        &self,
        theme: Theme,
        size_class: WindowSizeClass,
        _cx: &mut Context<Self>,
    ) -> Div {
        let compact = size_class == WindowSizeClass::Compact;
        div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.surface_base)
            .child(Self::workspace_header(
                "分流规则",
                "查看最终参与匹配的有序规则；来源请前往配置页管理",
                "从上到下匹配",
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
                    .child(self.active_rules_panel(theme).max_w(px(1040.0)).mx_auto()),
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
    fn source_panel(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
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
                    .text_size(px(17.0))
                    .font_weight(FontWeight::BOLD)
                    .child("添加代理来源"),
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
            .child(
                div()
                    .mt_5()
                    .pt_4()
                    .border_t_1()
                    .border_color(theme.outline_subtle)
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("已保存来源"))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.text_tertiary)
                            .child(format!("{saved_source_count} 个")),
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
                        .child("还没有代理来源。添加后可在节点页查看其中的节点。"),
                )
            })
            .child(self.imported_subscription_cards(theme, cx))
            .child(self.saved_vless_cards(theme, cx))
            .child(self.source_panel_footer(theme));
        div().w_full().child(panel)
    }

    fn source_panel_footer(&self, theme: Theme) -> Div {
        let source = self.runtime.profile_source();
        div()
            .mt_3()
            .pt_3()
            .border_t_1()
            .border_color(theme.outline_subtle)
            .text_size(px(10.0))
            .text_color(theme.text_tertiary)
            .child("链接保存在本机私有目录；界面和日志不会显示完整地址。")
            .child(div().mt_1().child(format!(
                "当前应用方式 · {} · {}",
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
                "HTTPS 订阅已添加",
                "节点已由 Mihomo 实际下载并解析；可前往节点页查看",
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

    #[allow(clippy::too_many_lines)]
    fn rule_source_manager(
        &self,
        input: Entity<SubscriptionTextInput>,
        busy: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
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
                    .child("规则订阅"),
            )
            .child(
                div()
                    .mt_1()
                    .text_size(px(11.0))
                    .text_color(theme.text_secondary)
                    .child("添加 QX 规则地址，并选择命中后使用的目标策略。"),
            )
            .child(
                div()
                    .mt_4()
                    .mb_1()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
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
                            .id("qx-rule-target-policy")
                            .role(Role::Button)
                            .aria_label("切换 QX 规则目标策略")
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
                            .child(format!("目标 · {}", self.qx_rule_target_policy))
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
                            .child(if busy {
                                "处理中…"
                            } else {
                                "添加规则源"
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !busy {
                                    this.submit_qx_rule_import(&input, cx);
                                }
                            })),
                    ),
            )
            .child(self.qx_rule_import_feedback(theme))
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
                            .child("已添加规则源"),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.text_tertiary)
                            .child(format!("{} 个", self.qx_rule_sources.len())),
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
                    .child("还没有规则订阅源。添加后，分流规则页会显示实际参与匹配的规则。"),
            );
        }
        for (index, source) in self.qx_rule_sources.iter().enumerate() {
            panel = panel.child(Self::rule_source_card(index, source, busy, theme, cx));
        }
        panel.child(
            div()
                .mt_3()
                .text_size(px(10.0))
                .text_color(theme.text_tertiary)
                .child("规则地址和正文仅保存在本机私有目录；日志不会记录链接。"),
        )
    }

    fn rule_source_card(
        index: usize,
        source: &crate::mihomo::StoredQxRuleSource,
        busy: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let id = source.id.clone();
        div()
            .mt_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_low)
            .flex()
            .items_center()
            .gap_3()
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("规则源 {}", index + 1)),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(10.0))
                            .text_color(theme.text_secondary)
                            .child(format!(
                                "{} 条 · 跳过 {} 条 · → {}",
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
                    .when(!busy, gpui::Styled::cursor_pointer)
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .text_size(px(10.0))
                    .text_color(theme.text_secondary)
                    .child("删除")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !busy {
                            this.remove_qx_rule_source(id.clone(), cx);
                        }
                    })),
            )
    }

    #[allow(clippy::too_many_lines)]
    fn active_rules_panel(&self, theme: Theme) -> Stateful<Div> {
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
                                    .child("生效规则"),
                            )
                            .child(
                                div()
                                    .mt_1()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_secondary)
                                    .child("从上到下匹配；第一条命中后停止。"),
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
                            .child(format!("{} 条", remote_count + 2)),
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
                            .child(format!("规则源 {}", source_index + 1)),
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
                    .child("添加规则源后，这里会逐条显示 DOMAIN、DOMAIN-SUFFIX 和 DOMAIN-KEYWORD 规则。"),
            );
        }

        list = list
            .child(
                div()
                    .mt_5()
                    .mb_1()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("系统兜底"),
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
                "其余流量",
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
}
