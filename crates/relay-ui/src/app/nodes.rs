use gpui::{Context, Div, FontWeight, ParentElement, Role, Stateful, Styled, div, prelude::*, px};
use relay_core::{NodeAvailabilityFilter, PrimaryWorkspace, WindowSizeClass};

use super::{ImportedSubscriptionState, RelayApp};
use crate::{
    mihomo::{LoadedProvider, LoadedProviderNode},
    theme::Theme,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct NodeCounts {
    total: usize,
    available: usize,
    unavailable: usize,
    untested: usize,
}

impl NodeCounts {
    fn from_providers(providers: &[LoadedProvider]) -> Self {
        let mut counts = Self::default();
        for node in providers.iter().flat_map(|provider| &provider.nodes) {
            counts.total += 1;
            match node.alive {
                Some(true) => counts.available += 1,
                Some(false) => counts.unavailable += 1,
                None => counts.untested += 1,
            }
        }
        counts
    }

    fn count_for(self, filter: NodeAvailabilityFilter) -> usize {
        match filter {
            NodeAvailabilityFilter::All => self.total,
            NodeAvailabilityFilter::Available => self.available,
            NodeAvailabilityFilter::Unavailable => self.unavailable,
            NodeAvailabilityFilter::Untested => self.untested,
        }
    }
}

impl RelayApp {
    pub(super) fn node_workspace(
        &self,
        theme: Theme,
        size_class: WindowSizeClass,
        cx: &mut Context<Self>,
    ) -> Div {
        let compact = size_class == WindowSizeClass::Compact;
        let imported = self.imported_subscription.is_some();
        let providers = if imported {
            &self.subscription_preview_providers
        } else {
            &self.source_providers
        };
        let counts = NodeCounts::from_providers(providers);
        let filter = self.node_workspace.filter;
        let visible_count = counts.count_for(filter);
        let loading = imported
            && matches!(
                self.imported_subscription_state,
                ImportedSubscriptionState::Pending(_) | ImportedSubscriptionState::Refreshing(_)
            );
        let refreshing = loading
            || (!imported
                && matches!(
                    self.controller,
                    crate::mihomo::ControllerState::Connecting { .. }
                ));
        let unavailable = imported
            && matches!(
                self.imported_subscription_state,
                ImportedSubscriptionState::Unavailable(_, _)
                    | ImportedSubscriptionState::StoreError(_)
            );
        let origin = if imported {
            "已导入订阅"
        } else if providers.is_empty() {
            "尚无节点来源"
        } else {
            "当前 Mihomo"
        };

        div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.surface_base)
            .child(Self::node_workspace_header(
                providers.len(),
                counts,
                filter,
                origin,
                refreshing,
                compact,
                theme,
                cx,
            ))
            .child(Self::node_workspace_body(
                providers,
                filter,
                visible_count,
                loading,
                unavailable,
                compact,
                theme,
                cx,
            ))
    }

    #[allow(clippy::too_many_arguments)]
    fn node_workspace_header(
        provider_count: usize,
        counts: NodeCounts,
        filter: NodeAvailabilityFilter,
        origin: &'static str,
        refreshing: bool,
        compact: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .px(if compact { px(16.0) } else { px(24.0) })
            .pt(if compact { px(16.0) } else { px(22.0) })
            .pb_4()
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
                            .min_w(px(0.0))
                            .child(
                                div()
                                    .text_size(if compact { px(20.0) } else { px(24.0) })
                                    .font_weight(FontWeight::BOLD)
                                    .child(format!("节点 · {}", counts.total)),
                            )
                            .child(
                                div()
                                    .mt_1()
                                    .text_size(px(12.0))
                                    .text_color(theme.text_secondary)
                                    .child(format!(
                                        "{origin} · {provider_count} 个来源 · 在这里查看出口健康状态"
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(Self::node_refresh_button(refreshing, theme, cx))
                            .child(Self::node_configuration_link(theme, cx)),
                    ),
            )
            .child(Self::node_health_summary(counts, compact, theme))
            .child(Self::node_filter_bar(counts, filter, compact, theme, cx))
    }

    #[allow(clippy::too_many_arguments)]
    fn node_workspace_body(
        providers: &[LoadedProvider],
        filter: NodeAvailabilityFilter,
        visible_count: usize,
        loading: bool,
        unavailable: bool,
        compact: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id("nodes-scroll")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .px(if compact { px(12.0) } else { px(24.0) })
            .py_4()
            .when(loading && providers.is_empty(), |body| {
                body.child(Self::node_message_panel(
                    "正在恢复节点",
                    "Relay 正在通过隔离 Mihomo 重新读取已导入订阅。",
                    theme,
                ))
            })
            .when(unavailable && providers.is_empty(), |body| {
                body.child(Self::node_message_panel(
                    "暂时无法读取节点",
                    "订阅仍安全保存在本机。请前往配置页检查来源，原链接不会显示。",
                    theme,
                ))
            })
            .when(!loading && !unavailable && providers.is_empty(), |body| {
                body.child(Self::node_empty_state(compact, theme, cx))
            })
            .when(!providers.is_empty() && visible_count == 0, |body| {
                body.child(Self::node_message_panel(
                    "这个筛选下没有节点",
                    "切换到“全部”即可重新查看完整节点列表。",
                    theme,
                ))
            })
            .when(!providers.is_empty() && visible_count > 0, |body| {
                body.child(Self::node_table(providers, filter, compact, theme))
            })
    }

    fn node_configuration_link(theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id("nodes-open-configuration")
            .role(Role::Button)
            .aria_label("管理订阅来源")
            .tab_stop(true)
            .focusable()
            .cursor_pointer()
            .h(px(34.0))
            .px_3()
            .rounded_md()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_low)
            .text_color(theme.text_primary)
            .flex()
            .items_center()
            .child("管理来源")
            .on_click(cx.listener(|this, _, _, cx| {
                this.primary_workspace = PrimaryWorkspace::Configuration;
                "已打开订阅来源配置".clone_into(&mut this.status);
                cx.notify();
            }))
    }

    fn node_refresh_button(
        refreshing: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id("nodes-refresh")
            .role(Role::Button)
            .aria_label("刷新节点健康状态")
            .tab_stop(!refreshing)
            .focusable()
            .cursor_pointer()
            .h(px(34.0))
            .px_3()
            .rounded_md()
            .border_1()
            .border_color(if refreshing {
                theme.outline_subtle
            } else {
                theme.action_primary
            })
            .bg(if refreshing {
                theme.surface_low
            } else {
                theme.action_soft
            })
            .text_color(if refreshing {
                theme.text_tertiary
            } else {
                theme.action_primary
            })
            .flex()
            .items_center()
            .child(if refreshing {
                "读取中…"
            } else {
                "刷新节点"
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                if refreshing {
                    return;
                }
                if let Some(subscription) = this.imported_subscription.as_ref() {
                    let kind = super::source_kind(subscription);
                    this.imported_subscription_state = ImportedSubscriptionState::Pending(kind);
                    this.restore_imported_subscription(cx);
                } else {
                    this.connect_mihomo(cx);
                }
            }))
    }

    fn node_health_summary(counts: NodeCounts, compact: bool, theme: Theme) -> Div {
        div()
            .mt_4()
            .py_3()
            .px(if compact { px(12.0) } else { px(16.0) })
            .rounded_md()
            .bg(theme.surface_low)
            .flex()
            .items_center()
            .gap(if compact { px(12.0) } else { px(24.0) })
            .child(Self::node_health_value(
                "可用",
                counts.available,
                theme.status_success,
                theme,
            ))
            .child(Self::node_health_value(
                "不可用",
                counts.unavailable,
                theme.text_secondary,
                theme,
            ))
            .child(Self::node_health_value(
                "未测速",
                counts.untested,
                theme.text_tertiary,
                theme,
            ))
    }

    fn node_health_value(
        label: &'static str,
        count: usize,
        color: gpui::Rgba,
        theme: Theme,
    ) -> Div {
        div()
            .flex()
            .items_baseline()
            .gap_1()
            .child(
                div()
                    .text_size(px(16.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(color)
                    .child(count.to_string()),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.text_secondary)
                    .child(label),
            )
    }

    fn node_filter_bar(
        counts: NodeCounts,
        selected: NodeAvailabilityFilter,
        compact: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let filters = [
            ("全部", NodeAvailabilityFilter::All),
            ("可用", NodeAvailabilityFilter::Available),
            ("不可用", NodeAvailabilityFilter::Unavailable),
            ("未测速", NodeAvailabilityFilter::Untested),
        ];
        div()
            .id("node-filter-bar")
            .mt_3()
            .flex()
            .items_center()
            .gap_2()
            .when(compact, gpui::StatefulInteractiveElement::overflow_x_scroll)
            .children(filters.into_iter().map(|(label, filter)| {
                let active = selected == filter;
                div()
                    .id(format!("node-filter-{label}"))
                    .role(Role::Button)
                    .aria_label(format!("筛选{label}节点"))
                    .tab_stop(true)
                    .focusable()
                    .cursor_pointer()
                    .h(px(32.0))
                    .px_3()
                    .rounded_md()
                    .border_1()
                    .border_color(if active {
                        theme.action_primary
                    } else {
                        theme.outline_subtle
                    })
                    .bg(if active {
                        theme.action_soft
                    } else {
                        theme.surface_high
                    })
                    .text_color(if active {
                        theme.action_primary
                    } else {
                        theme.text_secondary
                    })
                    .font_weight(if active {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .flex()
                    .items_center()
                    .child(format!("{label} {}", counts.count_for(filter)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.node_workspace.select_filter(filter);
                        this.status = format!("节点筛选：{label}");
                        cx.notify();
                    }))
            }))
    }

    fn node_table(
        providers: &[LoadedProvider],
        filter: NodeAvailabilityFilter,
        compact: bool,
        theme: Theme,
    ) -> Div {
        let mut table = div()
            .rounded_md()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .overflow_hidden();

        if !compact {
            table = table.child(Self::node_table_header(theme));
        }

        for (provider_index, provider) in providers.iter().enumerate() {
            for (node_index, node) in provider.nodes.iter().enumerate() {
                if !(relay_core::NodeWorkspaceState { filter }).includes(node.alive) {
                    continue;
                }
                table = table.child(Self::workspace_node_row(
                    provider_index,
                    node_index,
                    provider,
                    node,
                    compact,
                    theme,
                ));
            }
        }
        table
    }

    fn node_table_header(theme: Theme) -> Div {
        div()
            .h(px(36.0))
            .px_4()
            .flex()
            .items_center()
            .gap_3()
            .bg(theme.surface_low)
            .text_size(px(10.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.text_tertiary)
            .child(div().flex_1().child("节点"))
            .child(div().w(px(180.0)).child("来源"))
            .child(div().w(px(100.0)).child("协议"))
            .child(div().w(px(82.0)).child("状态"))
            .child(
                div()
                    .w(px(72.0))
                    .text_align(gpui::TextAlign::Right)
                    .child("延迟"),
            )
    }

    fn workspace_node_row(
        provider_index: usize,
        node_index: usize,
        provider: &LoadedProvider,
        node: &LoadedProviderNode,
        compact: bool,
        theme: Theme,
    ) -> Stateful<Div> {
        let (state, color) = match node.alive {
            Some(true) => ("可用", theme.status_success),
            Some(false) => ("不可用", theme.text_secondary),
            None => ("未测速", theme.text_tertiary),
        };
        let latency = node.latency_label.as_deref().unwrap_or("—");
        let content = if compact {
            Self::compact_node_row_content(provider, node, state, latency, color, theme)
        } else {
            Self::wide_node_row_content(provider, node, state, latency, color, theme)
        };
        div()
            .id(format!("node-row-{provider_index}-{node_index}"))
            .min_h(if compact { px(64.0) } else { px(52.0) })
            .px(if compact { px(12.0) } else { px(16.0) })
            .py_2()
            .border_t_1()
            .border_color(theme.outline_subtle)
            .child(content)
    }

    fn compact_node_row_content(
        provider: &LoadedProvider,
        node: &LoadedProviderNode,
        state: &'static str,
        latency: &str,
        color: gpui::Rgba,
        theme: Theme,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(node.name.clone()),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(10.0))
                            .text_color(theme.text_tertiary)
                            .child(format!("{} · {}", provider.name, node.protocol)),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_align(gpui::TextAlign::Right)
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(color)
                            .child(state),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(10.0))
                            .text_color(theme.text_tertiary)
                            .child(latency.to_owned()),
                    ),
            )
    }

    fn wide_node_row_content(
        provider: &LoadedProvider,
        node: &LoadedProviderNode,
        state: &'static str,
        latency: &str,
        color: gpui::Rgba,
        theme: Theme,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .gap_3()
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(node.name.clone()),
            )
            .child(
                div()
                    .w(px(180.0))
                    .text_color(theme.text_secondary)
                    .child(provider.name.clone()),
            )
            .child(
                div()
                    .w(px(100.0))
                    .text_size(px(11.0))
                    .text_color(theme.text_tertiary)
                    .child(node.protocol.clone()),
            )
            .child(
                div()
                    .w(px(82.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(color)
                    .child(state),
            )
            .child(
                div()
                    .w(px(72.0))
                    .text_align(gpui::TextAlign::Right)
                    .text_color(theme.text_secondary)
                    .child(latency.to_owned()),
            )
    }

    fn node_empty_state(compact: bool, theme: Theme, cx: &mut Context<Self>) -> Div {
        div()
            .min_h(px(if compact { 260.0 } else { 320.0 }))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .text_align(gpui::TextAlign::Center)
            .child(
                div()
                    .text_size(px(18.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("还没有可管理的节点"),
            )
            .child(
                div()
                    .mt_2()
                    .max_w(px(420.0))
                    .text_color(theme.text_secondary)
                    .child("先导入一个 HTTP/HTTPS 订阅，节点会自动出现在这个工作区。"),
            )
            .child(
                div()
                    .id("nodes-empty-import")
                    .role(Role::Button)
                    .aria_label("前往配置导入订阅")
                    .tab_stop(true)
                    .focusable()
                    .cursor_pointer()
                    .mt_4()
                    .h(px(36.0))
                    .px_4()
                    .rounded_md()
                    .bg(theme.action_primary)
                    .text_color(theme.action_on_primary)
                    .font_weight(FontWeight::SEMIBOLD)
                    .flex()
                    .items_center()
                    .child("前往配置导入")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.primary_workspace = PrimaryWorkspace::Configuration;
                        "已打开订阅来源配置".clone_into(&mut this.status);
                        cx.notify();
                    })),
            )
    }

    fn node_message_panel(title: &'static str, copy: &'static str, theme: Theme) -> Div {
        div()
            .p_4()
            .rounded_md()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
            .child(div().mt_1().text_color(theme.text_secondary).child(copy))
    }
}

#[cfg(test)]
mod tests {
    use super::NodeCounts;
    use crate::mihomo::{LoadedProvider, LoadedProviderNode};

    #[test]
    fn counts_node_availability_across_providers() {
        let providers = vec![LoadedProvider {
            name: "fixture".to_owned(),
            vehicle_type: None,
            nodes: vec![
                node(Some(true)),
                node(Some(true)),
                node(Some(false)),
                node(None),
            ],
        }];

        assert_eq!(
            NodeCounts::from_providers(&providers),
            NodeCounts {
                total: 4,
                available: 2,
                unavailable: 1,
                untested: 1,
            }
        );
    }

    fn node(alive: Option<bool>) -> LoadedProviderNode {
        LoadedProviderNode {
            name: "node".to_owned(),
            protocol: "SS".to_owned(),
            latency_label: None,
            alive,
        }
    }
}
