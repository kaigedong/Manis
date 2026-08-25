use std::collections::BTreeSet;
use std::time::Duration;

use gpui::{
    Animation, AnimationExt as _, Context, Div, FontWeight, ParentElement, Role, Stateful, Styled,
    div, prelude::*, px,
};
use relay_core::{
    NodeAvailabilityFilter, NodeGroupIcon, NodeGroupMatcher, NodeGroupStrategy, NodeIdentity,
    NodePolicyGroup, PrimaryWorkspace, WindowSizeClass,
};

use super::{
    GroupBenchmarkState, ImportedSubscriptionState, NodeGroupDraft, NodeGroupMatcherKind,
    NodeGroupRuntimeState, RelayApp, SourceRuntimeApply,
};
use crate::{
    diagnostics::{UiEvent, trace_ui},
    mihomo::{self, LoadedProvider, LoadedProviderNode},
    subscription::SourceNodePreview,
    theme::Theme,
};

struct NodeSourceGroup<'a> {
    id: String,
    name: String,
    detail: String,
    providers: Vec<&'a LoadedProvider>,
    saved_nodes: Vec<&'a SourceNodePreview>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NodeGroupMemberView {
    identity: NodeIdentity,
    source_name: String,
    protocol: String,
    latency_label: Option<String>,
    alive: Option<bool>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SourceNodeLatencyState {
    Idle(Option<String>),
    Running,
    Measured(u16),
    Failed,
}

impl SourceNodeLatencyState {
    fn from_benchmark(node: &LoadedProviderNode, benchmark: &GroupBenchmarkState) -> Self {
        match benchmark {
            GroupBenchmarkState::Idle => Self::Idle(node.latency_label.clone()),
            GroupBenchmarkState::Running { .. } => Self::Running,
            GroupBenchmarkState::Complete { delays, .. } => match delays.get(&node.name).copied() {
                Some(delay) if delay > 0 => Self::Measured(delay),
                Some(_) | None => Self::Failed,
            },
            GroupBenchmarkState::Failed { .. } => Self::Failed,
        }
    }
}

const MAX_GROUP_BENCHMARK_NODES: usize = 512;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct NodeCounts {
    total: usize,
    available: usize,
    unavailable: usize,
    untested: usize,
}

impl NodeCounts {
    #[cfg(test)]
    fn from_providers(providers: &[LoadedProvider]) -> Self {
        let mut counts = Self::default();
        for provider in providers {
            counts.add_provider(provider);
        }
        counts
    }

    fn from_provider_refs(providers: &[&LoadedProvider]) -> Self {
        let mut counts = Self::default();
        for provider in providers {
            counts.add_provider(provider);
        }
        counts
    }

    fn from_groups(groups: &[NodeSourceGroup<'_>]) -> Self {
        let mut counts = Self::default();
        for group in groups {
            for provider in &group.providers {
                counts.add_provider(provider);
            }
            counts.total += group.saved_nodes.len();
            counts.untested += group.saved_nodes.len();
        }
        counts
    }

    fn add_provider(&mut self, provider: &LoadedProvider) {
        for node in &provider.nodes {
            self.total += 1;
            match node.alive {
                Some(true) => self.available += 1,
                Some(false) => self.unavailable += 1,
                None => self.untested += 1,
            }
        }
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
        let has_local_sources =
            !self.imported_subscriptions.is_empty() || !self.saved_vless_nodes.is_empty();
        let groups = self.node_source_groups(has_local_sources);
        let counts = NodeCounts::from_groups(&groups);
        let filter = self.node_workspace.filter;
        let loading = self.imported_subscriptions.iter().any(|subscription| {
            matches!(
                subscription.state,
                ImportedSubscriptionState::Pending(_) | ImportedSubscriptionState::Refreshing(_)
            )
        });
        let refreshing = loading
            || (!has_local_sources
                && matches!(
                    self.controller,
                    crate::mihomo::ControllerState::Connecting { .. }
                ));
        let unavailable = !self.imported_subscriptions.is_empty()
            && self.imported_subscriptions.iter().all(|subscription| {
                matches!(
                    subscription.state,
                    ImportedSubscriptionState::Unavailable(_, _)
                        | ImportedSubscriptionState::StoreError(_)
                )
            });
        let origin = if has_local_sources {
            "本机来源"
        } else if self.source_providers.is_empty() {
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
                groups.len(),
                counts,
                filter,
                origin,
                refreshing,
                compact,
                theme,
                cx,
            ))
            .child(self.node_workspace_body(
                &groups,
                filter,
                loading,
                unavailable,
                size_class,
                compact,
                theme,
                cx,
            ))
    }

    fn node_source_groups(&self, has_local_sources: bool) -> Vec<NodeSourceGroup<'_>> {
        if has_local_sources {
            let mut groups: Vec<_> = self
                .imported_subscriptions
                .iter()
                .enumerate()
                .map(|(index, subscription)| {
                    let name = subscription
                        .source
                        .subscription_name()
                        .unwrap_or_else(|| format!("订阅 {}", index + 1));
                    let transport = if subscription.source.is_https() {
                        "HTTPS 订阅"
                    } else {
                        "HTTP 订阅"
                    };
                    let state = match subscription.state {
                        ImportedSubscriptionState::Pending(_)
                        | ImportedSubscriptionState::Refreshing(_) => "正在恢复",
                        ImportedSubscriptionState::Ready(_) => "重启后自动恢复",
                        ImportedSubscriptionState::Unavailable(_, _)
                        | ImportedSubscriptionState::StoreError(_) => "当前不可用",
                        ImportedSubscriptionState::Removing(_) => "正在移除",
                        ImportedSubscriptionState::None => "尚未读取",
                    };
                    NodeSourceGroup {
                        id: format!("subscription:{}", subscription.id),
                        name,
                        detail: format!("{transport} · {state}"),
                        providers: subscription.providers.iter().collect(),
                        saved_nodes: Vec::new(),
                    }
                })
                .collect();
            if !self.saved_vless_nodes.is_empty() {
                groups.push(NodeSourceGroup {
                    id: "saved".to_owned(),
                    name: "已保存".to_owned(),
                    detail: "单独添加的 VLESS 节点 · 私有本机存储".to_owned(),
                    providers: Vec::new(),
                    saved_nodes: self
                        .saved_vless_nodes
                        .iter()
                        .map(|saved| saved.source.preview())
                        .collect(),
                });
            }
            return groups;
        }

        self.source_providers
            .iter()
            .enumerate()
            .map(|(index, provider)| NodeSourceGroup {
                id: format!("mihomo:{index}"),
                name: provider.name.clone(),
                detail: provider.vehicle_type.as_ref().map_or_else(
                    || "Mihomo 来源".to_owned(),
                    |vehicle| format!("Mihomo 来源 · {vehicle}"),
                ),
                providers: vec![provider],
                saved_nodes: Vec::new(),
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn node_workspace_header(
        source_count: usize,
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
                                        "{origin} · {source_count} 个来源 · 在这里查看出口健康状态"
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
        &self,
        groups: &[NodeSourceGroup<'_>],
        filter: NodeAvailabilityFilter,
        loading: bool,
        unavailable: bool,
        size_class: WindowSizeClass,
        compact: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let wide = size_class == WindowSizeClass::Wide;
        div()
            .id("nodes-scroll")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .px(if compact { px(12.0) } else { px(24.0) })
            .py_4()
            .child(Self::node_section_heading(
                "导入的节点",
                "按来源查看已经导入的节点；这里只管理库存，不决定流量走向。",
                theme,
            ))
            .when(loading && groups.is_empty(), |body| {
                body.child(Self::node_message_panel(
                    "正在恢复节点",
                    "Relay 正在通过隔离 Mihomo 重新读取已导入订阅。",
                    theme,
                ))
            })
            .when(unavailable && groups.is_empty(), |body| {
                body.child(Self::node_message_panel(
                    "暂时无法读取节点",
                    "订阅仍安全保存在本机。请前往配置页检查来源，原链接不会显示。",
                    theme,
                ))
            })
            .when(!loading && !unavailable && groups.is_empty(), |body| {
                body.child(Self::node_empty_state(compact, theme, cx))
            })
            .when(!groups.is_empty(), |body| {
                body.child(self.node_group_list(groups, filter, compact, theme, cx))
            })
            .child(self.node_group_workspace_section(wide, compact, theme, cx))
    }

    fn node_section_heading(title: &'static str, detail: &'static str, theme: Theme) -> Div {
        div()
            .mb_3()
            .child(
                div()
                    .text_size(px(16.0))
                    .font_weight(FontWeight::BOLD)
                    .child(title),
            )
            .child(
                div()
                    .mt_1()
                    .text_size(px(11.0))
                    .text_color(theme.text_secondary)
                    .child(detail),
            )
    }

    fn node_group_workspace_section(
        &self,
        wide: bool,
        compact: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut section = div()
            .mt_6()
            .pt_5()
            .border_t_1()
            .border_color(theme.outline_subtle)
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap_3()
                    .child(Self::node_section_heading(
                        "节点分组",
                        "用选择策略与匹配规则，把上方节点组织成可用于规则路由的策略组。",
                        theme,
                    ))
                    .child(Self::node_group_add_button(theme, cx)),
            );
        if self.node_policy_groups.is_empty() && self.node_group_draft.is_none() {
            section = section.child(
                div()
                    .p_5()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.outline_subtle)
                    .bg(theme.surface_high)
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("还没有自定义分组"),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(11.0))
                            .text_color(theme.text_secondary)
                            .child("可以创建手动选择组，或让 Relay 自动选择最低延迟节点。"),
                    ),
            );
        }
        let mut cards = div().grid().grid_cols(if wide { 2 } else { 1 }).gap_3();
        for group in &self.node_policy_groups {
            cards = cards.child(self.node_policy_group_card(group, theme, cx));
        }
        if !self.node_policy_groups.is_empty() {
            section = section.child(cards);
        }
        if let Some(group) = self.selected_node_group_id.as_ref().and_then(|selected| {
            self.node_policy_groups
                .iter()
                .find(|group| group.id == *selected)
        }) {
            section = section.child(self.node_policy_group_detail(group, compact, theme, cx));
        }
        if let Some(draft) = self.node_group_draft.as_ref() {
            section = section.child(self.node_group_editor(draft, compact, theme, cx));
        }
        section
    }

    fn node_group_add_button(theme: Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id("node-group-add")
            .role(Role::Button)
            .aria_label("添加节点分组")
            .tab_stop(true)
            .focusable()
            .cursor_pointer()
            .h(px(36.0))
            .px_4()
            .rounded_md()
            .bg(theme.action_primary)
            .text_color(theme.action_on_primary)
            .font_weight(FontWeight::SEMIBOLD)
            .flex()
            .items_center()
            .child("添加分组")
            .on_click(cx.listener(|this, _, _, cx| this.start_node_group_create(cx)))
    }

    #[allow(clippy::too_many_lines)]
    fn node_policy_group_card(
        &self,
        group: &NodePolicyGroup,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let matched = self.node_group_match_count(group);
        let benchmark = self
            .group_benchmarks
            .get(&Self::user_group_benchmark_key(&group.id))
            .cloned()
            .unwrap_or_default();
        let benchmarking = benchmark.is_running();
        let selected = self.selected_node_group_id.as_deref() == Some(group.id.as_str());
        let benchmark_id = group.id.clone();
        let detail_id = group.id.clone();
        let group_id = group.id.clone();
        let remove_id = group.id.clone();
        let matcher_summary = match &group.matcher {
            NodeGroupMatcher::All => "全部节点".to_owned(),
            NodeGroupMatcher::NameContains(value) => format!("名称包含 “{value}”"),
            NodeGroupMatcher::Explicit(nodes) => format!("明确选择 {} 个节点", nodes.len()),
        };
        let strategy_summary = if group.strategy == NodeGroupStrategy::LowestLatency {
            format!(
                "{} · 每 {} 秒检查",
                group.strategy.label(),
                group.test_interval_secs
            )
        } else {
            group.strategy.label().to_owned()
        };
        div()
            .p_4()
            .rounded_md()
            .border_1()
            .border_color(if selected {
                theme.action_primary
            } else {
                theme.outline_subtle
            })
            .bg(theme.surface_high)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(Self::group_benchmark_icon(
                        &Self::user_group_benchmark_key(&group.id),
                        benchmarking,
                        theme,
                        cx.listener(move |this, _, _, cx| {
                            if !benchmarking {
                                this.start_node_group_benchmark(&benchmark_id, cx);
                            }
                        }),
                    ))
                    .child(Self::node_group_icon_badge(group.icon, theme))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(group.name.clone()),
                            )
                            .child(
                                div()
                                    .mt_1()
                                    .text_size(px(10.0))
                                    .text_color(theme.text_secondary)
                                    .child(format!(
                                        "{strategy_summary} · {matcher_summary} · 匹配 {matched} 个"
                                    )),
                            ),
                    ),
            )
            .child(Self::node_group_benchmark_status(
                &benchmark, matched, theme,
            ))
            .child(
                div()
                    .mt_3()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(Self::node_group_text_button(
                        format!("node-group-detail-{detail_id}"),
                        if selected {
                            "收起详情"
                        } else {
                            "查看详情"
                        },
                        theme,
                        cx.listener(move |this, _, _, cx| {
                            this.toggle_node_group_detail(&detail_id, cx);
                        }),
                    ))
                    .child(Self::node_group_text_button(
                        format!("node-group-edit-{group_id}"),
                        "编辑",
                        theme,
                        cx.listener(move |this, _, _, cx| {
                            this.start_node_group_edit(&group_id, cx);
                        }),
                    ))
                    .child(Self::node_group_text_button(
                        format!("node-group-remove-{remove_id}"),
                        "删除",
                        theme,
                        cx.listener(move |this, _, _, cx| {
                            this.remove_node_policy_group(&remove_id, cx);
                        }),
                    )),
            )
    }

    fn node_policy_group_detail(
        &self,
        group: &NodePolicyGroup,
        compact: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let members = self.node_group_members(group);
        let runtime_state = self
            .node_group_runtime_states
            .get(&group.id)
            .cloned()
            .unwrap_or_default();
        let benchmark = self
            .group_benchmarks
            .get(&Self::user_group_benchmark_key(&group.id))
            .cloned()
            .unwrap_or_default();
        let close_id = group.id.clone();
        let mut list = div().mt_3().border_t_1().border_color(theme.outline_subtle);
        for member in members {
            list = list.child(Self::node_group_member_row(
                group,
                member,
                &runtime_state,
                &benchmark,
                compact,
                self.runtime.manages_node_policy_groups(),
                theme,
                cx,
            ));
        }

        div()
            .mt_3()
            .p(if compact { px(14.0) } else { px(18.0) })
            .rounded_md()
            .border_1()
            .border_color(theme.action_primary)
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
                                    .text_size(px(15.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child(group.name.clone()),
                            )
                            .child(
                                div()
                                    .mt_1()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_secondary)
                                    .child(format!(
                                        "{} · {} 个候选节点",
                                        group.strategy.label(),
                                        self.node_group_match_count(group)
                                    )),
                            ),
                    )
                    .child(Self::node_group_text_button(
                        format!("node-group-detail-close-{close_id}"),
                        "关闭",
                        theme,
                        cx.listener(move |this, _, _, cx| {
                            this.toggle_node_group_detail(&close_id, cx);
                        }),
                    )),
            )
            .child(Self::node_group_runtime_banner(
                group,
                &runtime_state,
                theme,
                cx,
            ))
            .when(self.node_group_match_count(group) == 0, |detail| {
                detail.child(
                    div()
                        .mt_3()
                        .p_4()
                        .rounded_md()
                        .bg(theme.surface_low)
                        .text_size(px(11.0))
                        .text_color(theme.text_secondary)
                        .child("当前规则没有匹配到节点，请编辑分组规则。"),
                )
            })
            .when(self.node_group_match_count(group) > 0, |detail| {
                detail.child(list)
            })
    }

    fn node_group_runtime_banner(
        group: &NodePolicyGroup,
        state: &NodeGroupRuntimeState,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let (title, detail, color) = match state {
            NodeGroupRuntimeState::LocalOnly => (
                "本地候选 · 当前只读".to_owned(),
                "Relay 不会改写外部控制器或已有配置；请使用 Relay 托管配置来切换此分组。"
                    .to_owned(),
                theme.text_secondary,
            ),
            NodeGroupRuntimeState::Loading { .. } => (
                "正在读取当前出口…".to_owned(),
                "正在从 Relay 托管 Mihomo 同步策略组状态。".to_owned(),
                theme.action_primary,
            ),
            NodeGroupRuntimeState::Ready { current, .. } => {
                let current = current.as_deref().unwrap_or("尚未选择");
                if group.strategy == NodeGroupStrategy::Manual {
                    (
                        format!("当前使用：{current}"),
                        "选择其他节点后会立即应用，并由 Mihomo 保存选择。".to_owned(),
                        theme.status_success,
                    )
                } else {
                    (
                        format!("当前优选：{current}"),
                        "最低延迟组由 Mihomo 自动测试并切换，不支持手动指定。".to_owned(),
                        theme.status_success,
                    )
                }
            }
            NodeGroupRuntimeState::Selecting { pending, .. } => (
                format!("正在切换到：{pending}"),
                "等待 Mihomo 确认新的当前出口。".to_owned(),
                theme.action_primary,
            ),
            NodeGroupRuntimeState::Failed { .. } => (
                "无法读取分组运行状态".to_owned(),
                "请启动或连接 Relay 托管 Mihomo 后重试。".to_owned(),
                theme.route_trace,
            ),
        };
        let retry_id = group.id.clone();
        div()
            .mt_3()
            .p_3()
            .rounded_md()
            .bg(theme.surface_low)
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(color)
                            .child(title),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(10.0))
                            .text_color(theme.text_secondary)
                            .child(detail),
                    ),
            )
            .when(
                matches!(state, NodeGroupRuntimeState::Failed { .. }),
                |banner| {
                    banner.child(Self::node_group_text_button(
                        format!("node-group-runtime-retry-{retry_id}"),
                        "重试",
                        theme,
                        cx.listener(move |this, _, _, cx| {
                            this.refresh_node_group_runtime(&retry_id, cx);
                        }),
                    ))
                },
            )
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn node_group_member_row(
        group: &NodePolicyGroup,
        member: NodeGroupMemberView,
        runtime_state: &NodeGroupRuntimeState,
        benchmark: &GroupBenchmarkState,
        compact: bool,
        managed: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let current = match runtime_state {
            NodeGroupRuntimeState::Ready { current, .. }
            | NodeGroupRuntimeState::Selecting { current, .. } => current.as_deref(),
            _ => None,
        };
        let is_current = current == Some(member.identity.node_name.as_str());
        let selecting = matches!(
            runtime_state,
            NodeGroupRuntimeState::Selecting { pending, .. }
                if pending == &member.identity.node_name
        );
        let selectable = managed
            && group.strategy == NodeGroupStrategy::Manual
            && matches!(
                runtime_state,
                NodeGroupRuntimeState::Ready { candidates, .. }
                    if candidates.contains(&member.identity.node_name)
            )
            && !is_current;
        let delay = match benchmark {
            GroupBenchmarkState::Complete { delays, .. } => {
                match delays.get(&member.identity.node_name).copied() {
                    Some(0) => Some("失败".to_owned()),
                    Some(delay) => Some(format!("{delay} ms")),
                    None => member.latency_label.clone(),
                }
            }
            _ => member.latency_label.clone(),
        }
        .unwrap_or_else(|| "未测速".to_owned());
        let (health, health_color) = match member.alive {
            Some(true) => ("可用", theme.status_success),
            Some(false) => ("不可用", theme.route_trace),
            None => ("未检测", theme.text_tertiary),
        };
        let group_id = group.id.clone();
        let node_name = member.identity.node_name.clone();
        let action_label = if is_current {
            "当前"
        } else if selecting {
            "切换中…"
        } else if selectable {
            "选择"
        } else if group.strategy == NodeGroupStrategy::LowestLatency {
            "自动"
        } else {
            "只读"
        };
        let action = div()
            .id(format!(
                "node-group-select-{}-{}",
                group.id, member.identity.node_name
            ))
            .role(Role::Button)
            .aria_label(format!(
                "{} · {} · {action_label}",
                group.name, member.identity.node_name
            ))
            .tab_stop(selectable)
            .focusable()
            .h(px(30.0))
            .px_3()
            .rounded_md()
            .border_1()
            .border_color(if selectable {
                theme.action_primary
            } else {
                theme.outline_subtle
            })
            .bg(if is_current {
                theme.action_soft
            } else {
                theme.surface_high
            })
            .text_size(px(10.0))
            .text_color(if selectable || is_current {
                theme.action_primary
            } else {
                theme.text_tertiary
            })
            .font_weight(FontWeight::SEMIBOLD)
            .flex()
            .items_center()
            .child(action_label)
            .when(selectable, |button| {
                button
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_node_group_member(&group_id, &node_name, cx);
                    }))
            });
        let metadata = format!("{} · {}", member.source_name, member.protocol);
        div()
            .min_h(if compact { px(58.0) } else { px(52.0) })
            .py_2()
            .flex()
            .items_center()
            .gap_3()
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .child(
                        div()
                            .font_weight(if is_current {
                                FontWeight::BOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .child(member.identity.node_name),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(10.0))
                            .text_color(theme.text_secondary)
                            .child(metadata),
                    ),
            )
            .child(
                div()
                    .w(if compact { px(58.0) } else { px(72.0) })
                    .text_size(px(10.0))
                    .text_color(health_color)
                    .child(health),
            )
            .child(
                div()
                    .w(if compact { px(62.0) } else { px(76.0) })
                    .text_size(px(10.0))
                    .text_color(theme.text_secondary)
                    .child(delay),
            )
            .child(action)
    }

    fn node_group_benchmark_status(
        state: &GroupBenchmarkState,
        matched: usize,
        theme: Theme,
    ) -> Div {
        let (label, color) = match state {
            GroupBenchmarkState::Idle => ("尚未测速".to_owned(), theme.text_tertiary),
            GroupBenchmarkState::Running { .. } => {
                (format!("正在测试 {matched} 个节点…"), theme.action_primary)
            }
            GroupBenchmarkState::Complete { summary, .. } => {
                let label = match (summary.average_ms, summary.minimum_ms, summary.maximum_ms) {
                    (Some(average), Some(minimum), Some(maximum)) => format!(
                        "平均 {average} ms · 最低 {minimum} ms · 最高 {maximum} ms · {}/{} 成功",
                        summary.succeeded, summary.total
                    ),
                    _ => format!("0/{} 成功", summary.total),
                };
                (label, theme.status_success)
            }
            GroupBenchmarkState::Failed { .. } => (
                "测速失败 · 请检查 Mihomo 连接与网络后重试".to_owned(),
                theme.route_trace,
            ),
        };
        div()
            .mt_3()
            .pt_3()
            .border_t_1()
            .border_color(theme.outline_subtle)
            .text_size(px(11.0))
            .text_color(color)
            .child(label)
    }

    fn node_group_icon_badge(icon: NodeGroupIcon, theme: Theme) -> Div {
        div()
            .size(px(40.0))
            .flex_shrink_0()
            .rounded_md()
            .bg(theme.action_soft)
            .text_color(theme.action_primary)
            .text_size(px(10.0))
            .font_weight(FontWeight::BOLD)
            .flex()
            .items_center()
            .justify_center()
            .child(icon.label())
    }

    fn node_group_text_button(
        id: String,
        label: &'static str,
        theme: Theme,
        listener: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
    ) -> Stateful<Div> {
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(label)
            .tab_stop(true)
            .focusable()
            .cursor_pointer()
            .h(px(32.0))
            .px_3()
            .rounded_md()
            .border_1()
            .border_color(theme.outline_subtle)
            .text_color(theme.text_secondary)
            .flex()
            .items_center()
            .child(label)
            .on_click(listener)
    }

    #[allow(clippy::too_many_lines)]
    fn node_group_editor(
        &self,
        draft: &NodeGroupDraft,
        compact: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let title = if draft.editing_id.is_some() {
            "编辑节点分组"
        } else {
            "创建节点分组"
        };
        let name_input = self.node_group_name_input.clone();
        let filter_input = self.node_group_filter_input.clone();
        let mut editor = div()
            .mt_3()
            .p(if compact { px(14.0) } else { px(18.0) })
            .rounded_md()
            .border_1()
            .border_color(theme.action_primary)
            .bg(theme.surface_high)
            .child(
                div()
                    .text_size(px(15.0))
                    .font_weight(FontWeight::BOLD)
                    .child(title),
            )
            .child(
                div()
                    .mt_4()
                    .child(Self::node_group_field_label("分组名称", theme))
                    .when_some(name_input, ParentElement::child),
            )
            .child(
                div()
                    .mt_4()
                    .child(Self::node_group_field_label("分组图标", theme))
                    .child(Self::node_group_icon_selector(draft.icon, theme, cx)),
            )
            .child(
                div()
                    .mt_4()
                    .child(Self::node_group_field_label("选择策略", theme))
                    .child(Self::node_group_strategy_selector(
                        draft.strategy,
                        theme,
                        cx,
                    )),
            )
            .when(
                draft.strategy == NodeGroupStrategy::LowestLatency,
                |editor| {
                    editor.child(
                        div()
                            .mt_4()
                            .child(Self::node_group_field_label("自动重新检查", theme))
                            .child(Self::node_group_interval_selector(
                                draft.test_interval_secs,
                                theme,
                                cx,
                            ))
                            .child(Self::node_group_helper(
                                "每个自动策略组独立保存间隔，并写入 Relay 托管的 Mihomo 配置。",
                                theme,
                            )),
                    )
                },
            )
            .child(
                div()
                    .mt_4()
                    .child(Self::node_group_field_label("节点规则", theme))
                    .child(Self::node_group_matcher_selector(
                        draft.matcher_kind,
                        theme,
                        cx,
                    )),
            );
        match draft.matcher_kind {
            NodeGroupMatcherKind::All => {
                editor = editor.child(Self::node_group_helper(
                    "当前导入的全部节点都会加入这个分组。",
                    theme,
                ));
            }
            NodeGroupMatcherKind::NameContains => {
                editor = editor.child(
                    div()
                        .mt_3()
                        .child(Self::node_group_field_label("名称包含", theme))
                        .when_some(filter_input, ParentElement::child),
                );
            }
            NodeGroupMatcherKind::Explicit => {
                editor = editor.child(self.node_group_member_picker(draft, theme, cx));
            }
        }
        editor.child(
            div()
                .mt_5()
                .flex()
                .justify_end()
                .gap_2()
                .child(Self::node_group_text_button(
                    "node-group-cancel".to_owned(),
                    "取消",
                    theme,
                    cx.listener(|this, _, _, cx| {
                        this.node_group_draft = None;
                        cx.notify();
                    }),
                ))
                .child(
                    div()
                        .id("node-group-save")
                        .role(Role::Button)
                        .aria_label("保存节点分组")
                        .tab_stop(true)
                        .focusable()
                        .cursor_pointer()
                        .h(px(32.0))
                        .px_4()
                        .rounded_md()
                        .bg(theme.action_primary)
                        .text_color(theme.action_on_primary)
                        .font_weight(FontWeight::SEMIBOLD)
                        .flex()
                        .items_center()
                        .child("保存分组")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.save_node_policy_group(cx);
                        })),
                ),
        )
    }

    fn node_group_field_label(label: &'static str, theme: Theme) -> Div {
        div()
            .mb_2()
            .text_size(px(11.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.text_secondary)
            .child(label)
    }

    fn node_group_helper(copy: &'static str, theme: Theme) -> Div {
        div()
            .mt_3()
            .text_size(px(10.0))
            .text_color(theme.text_tertiary)
            .child(copy)
    }

    fn node_group_icon_selector(
        selected: NodeGroupIcon,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut row = div().flex().flex_wrap().gap_2();
        for icon in [
            NodeGroupIcon::Bolt,
            NodeGroupIcon::Globe,
            NodeGroupIcon::Shield,
            NodeGroupIcon::Compass,
        ] {
            let active = selected == icon;
            row = row.child(
                div()
                    .id(format!("node-group-icon-{}", icon.key()))
                    .role(Role::Button)
                    .aria_label(format!("使用{}图标", icon.label()))
                    .aria_toggled(if active {
                        gpui::Toggled::True
                    } else {
                        gpui::Toggled::False
                    })
                    .tab_stop(true)
                    .focusable()
                    .cursor_pointer()
                    .h(px(34.0))
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
                        theme.surface_low
                    })
                    .text_color(if active {
                        theme.action_primary
                    } else {
                        theme.text_secondary
                    })
                    .flex()
                    .items_center()
                    .child(icon.label())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(draft) = this.node_group_draft.as_mut() {
                            draft.icon = icon;
                            cx.notify();
                        }
                    })),
            );
        }
        row
    }

    fn node_group_strategy_selector(
        selected: NodeGroupStrategy,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut row = div().flex().flex_wrap().gap_2();
        for strategy in [NodeGroupStrategy::Manual, NodeGroupStrategy::LowestLatency] {
            let active = selected == strategy;
            row = row.child(
                div()
                    .id(format!("node-group-strategy-{}", strategy.key()))
                    .role(Role::Button)
                    .aria_label(strategy.label())
                    .aria_toggled(if active {
                        gpui::Toggled::True
                    } else {
                        gpui::Toggled::False
                    })
                    .tab_stop(true)
                    .focusable()
                    .cursor_pointer()
                    .h(px(34.0))
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
                        theme.surface_low
                    })
                    .text_color(if active {
                        theme.action_primary
                    } else {
                        theme.text_secondary
                    })
                    .flex()
                    .items_center()
                    .child(strategy.label())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(draft) = this.node_group_draft.as_mut() {
                            draft.strategy = strategy;
                            cx.notify();
                        }
                    })),
            );
        }
        row
    }

    fn node_group_interval_selector(selected: u32, theme: Theme, cx: &mut Context<Self>) -> Div {
        let choices = [
            (60, "1 分钟"),
            (300, "5 分钟"),
            (600, "10 分钟"),
            (1_800, "30 分钟"),
        ];
        let mut row = div().flex().flex_wrap().gap_2();
        for (seconds, label) in choices {
            let active = selected == seconds;
            row = row.child(
                div()
                    .id(format!("node-group-interval-{seconds}"))
                    .role(Role::Button)
                    .aria_label(format!("每{label}自动检查"))
                    .aria_toggled(if active {
                        gpui::Toggled::True
                    } else {
                        gpui::Toggled::False
                    })
                    .tab_stop(true)
                    .focusable()
                    .cursor_pointer()
                    .h(px(34.0))
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
                        theme.surface_low
                    })
                    .text_color(if active {
                        theme.action_primary
                    } else {
                        theme.text_secondary
                    })
                    .flex()
                    .items_center()
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(draft) = this.node_group_draft.as_mut() {
                            draft.test_interval_secs = seconds;
                            cx.notify();
                        }
                    })),
            );
        }
        row
    }

    fn node_group_matcher_selector(
        selected: NodeGroupMatcherKind,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let choices = [
            ("全部节点", NodeGroupMatcherKind::All),
            ("名称包含", NodeGroupMatcherKind::NameContains),
            ("明确选择", NodeGroupMatcherKind::Explicit),
        ];
        let mut row = div().flex().flex_wrap().gap_2();
        for (label, matcher) in choices {
            let active = selected == matcher;
            row = row.child(
                div()
                    .id(format!("node-group-matcher-{matcher:?}"))
                    .role(Role::Button)
                    .aria_label(label)
                    .aria_toggled(if active {
                        gpui::Toggled::True
                    } else {
                        gpui::Toggled::False
                    })
                    .tab_stop(true)
                    .focusable()
                    .cursor_pointer()
                    .h(px(34.0))
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
                        theme.surface_low
                    })
                    .text_color(if active {
                        theme.action_primary
                    } else {
                        theme.text_secondary
                    })
                    .flex()
                    .items_center()
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(draft) = this.node_group_draft.as_mut() {
                            draft.matcher_kind = matcher;
                            cx.notify();
                        }
                    })),
            );
        }
        row
    }

    fn node_group_member_picker(
        &self,
        draft: &NodeGroupDraft,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let inventory = self.node_inventory();
        let mut list = div()
            .id("node-group-member-picker")
            .mt_3()
            .max_h(px(220.0))
            .overflow_y_scroll()
            .rounded_md()
            .border_1()
            .border_color(theme.outline_subtle);
        if inventory.is_empty() {
            return list.child(
                div()
                    .p_3()
                    .text_size(px(11.0))
                    .text_color(theme.text_secondary)
                    .child("先在上方导入节点，再进行明确选择。"),
            );
        }
        for member in inventory {
            let selected = draft.explicit_members.contains(&member);
            let member_for_click = member.clone();
            list = list.child(
                div()
                    .id(format!(
                        "node-group-member-{}-{}",
                        member.source_id, member.node_name
                    ))
                    .role(Role::CheckBox)
                    .aria_label(format!("选择节点{}", member.node_name))
                    .tab_stop(true)
                    .focusable()
                    .cursor_pointer()
                    .min_h(px(40.0))
                    .px_3()
                    .border_t_1()
                    .border_color(theme.outline_subtle)
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .size(px(16.0))
                            .rounded_sm()
                            .border_1()
                            .border_color(if selected {
                                theme.action_primary
                            } else {
                                theme.outline_strong
                            })
                            .bg(if selected {
                                theme.action_primary
                            } else {
                                theme.surface_high
                            }),
                    )
                    .child(div().flex_1().child(member.node_name.clone()))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme.text_tertiary)
                            .child(member.source_id.clone()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(draft) = this.node_group_draft.as_mut() {
                            if !draft.explicit_members.remove(&member_for_click) {
                                draft.explicit_members.insert(member_for_click.clone());
                            }
                            cx.notify();
                        }
                    })),
            );
        }
        list
    }

    fn node_inventory(&self) -> Vec<NodeIdentity> {
        let has_local_sources =
            !self.imported_subscriptions.is_empty() || !self.saved_vless_nodes.is_empty();
        let mut inventory = BTreeSet::new();
        for group in self.node_source_groups(has_local_sources) {
            for provider in group.providers {
                for node in &provider.nodes {
                    if let Ok(identity) = NodeIdentity::new(&group.id, &node.name) {
                        inventory.insert(identity);
                    }
                }
            }
            for node in group.saved_nodes {
                if let Ok(identity) = NodeIdentity::new(&group.id, &node.name) {
                    inventory.insert(identity);
                }
            }
        }
        inventory.into_iter().collect()
    }

    fn node_group_members(&self, policy_group: &NodePolicyGroup) -> Vec<NodeGroupMemberView> {
        let has_local_sources =
            !self.imported_subscriptions.is_empty() || !self.saved_vless_nodes.is_empty();
        let mut members = Vec::new();
        for source_group in self.node_source_groups(has_local_sources) {
            for provider in source_group.providers {
                for node in &provider.nodes {
                    let Ok(identity) = NodeIdentity::new(&source_group.id, &node.name) else {
                        continue;
                    };
                    if policy_group.matches(&identity.source_id, &identity.node_name) {
                        members.push(NodeGroupMemberView {
                            identity,
                            source_name: source_group.name.clone(),
                            protocol: node.protocol.clone(),
                            latency_label: node.latency_label.clone(),
                            alive: node.alive,
                        });
                    }
                }
            }
            for node in source_group.saved_nodes {
                let Ok(identity) = NodeIdentity::new(&source_group.id, &node.name) else {
                    continue;
                };
                if policy_group.matches(&identity.source_id, &identity.node_name) {
                    members.push(NodeGroupMemberView {
                        identity,
                        source_name: source_group.name.clone(),
                        protocol: node.protocol.to_owned(),
                        latency_label: None,
                        alive: None,
                    });
                }
            }
        }
        members.sort_by(|left, right| left.identity.cmp(&right.identity));
        members.dedup_by(|left, right| left.identity == right.identity);
        members
    }

    fn node_group_match_count(&self, group: &NodePolicyGroup) -> usize {
        self.node_inventory()
            .iter()
            .filter(|node| group.matches(&node.source_id, &node.node_name))
            .count()
    }

    fn node_group_candidate_names(&self, group: &NodePolicyGroup) -> Vec<String> {
        self.node_inventory()
            .into_iter()
            .filter(|node| group.matches(&node.source_id, &node.node_name))
            .map(|node| node.node_name)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    fn toggle_node_group_detail(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.selected_node_group_id.as_deref() == Some(id) {
            self.selected_node_group_id = None;
            cx.notify();
            return;
        }
        if !self.node_policy_groups.iter().any(|group| group.id == id) {
            return;
        }
        self.selected_node_group_id = Some(id.to_owned());
        if self.runtime.manages_node_policy_groups() {
            self.refresh_node_group_runtime(id, cx);
        } else {
            self.node_group_runtime_states
                .insert(id.to_owned(), NodeGroupRuntimeState::LocalOnly);
            "已打开分组详情；当前外部控制器保持只读".clone_into(&mut self.status);
            cx.notify();
        }
    }

    fn refresh_node_group_runtime(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(group) = self
            .node_policy_groups
            .iter()
            .find(|group| group.id == id)
            .cloned()
        else {
            return;
        };
        if !self.runtime.manages_node_policy_groups() {
            self.node_group_runtime_states
                .insert(id.to_owned(), NodeGroupRuntimeState::LocalOnly);
            cx.notify();
            return;
        }
        self.node_group_runtime_generation = self.node_group_runtime_generation.wrapping_add(1);
        let generation = self.node_group_runtime_generation;
        self.node_group_runtime_states
            .insert(id.to_owned(), NodeGroupRuntimeState::Loading { generation });
        self.status = format!("正在读取分组“{}”的当前出口", group.name);
        let runtime = self.runtime.clone();
        let group_id = group.id.clone();
        let group_name = group.name.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move { runtime.load_node_group_runtime(&group_name) })
                .await;
            this.update(cx, |this, cx| {
                if !this
                    .node_policy_groups
                    .iter()
                    .any(|group| group.id == group_id)
                {
                    return;
                }
                let Some(state) = this.node_group_runtime_states.get_mut(&group_id) else {
                    return;
                };
                let accepted = match result {
                    Ok(Some(snapshot)) => {
                        state.complete_refresh(generation, snapshot.current, snapshot.candidates)
                    }
                    Ok(None) => {
                        *state = NodeGroupRuntimeState::LocalOnly;
                        true
                    }
                    Err(_error) => state.fail(generation),
                };
                if accepted {
                    this.status = match state {
                        NodeGroupRuntimeState::Ready { .. } => "已同步策略组当前出口".to_owned(),
                        NodeGroupRuntimeState::LocalOnly => "当前控制器保持只读".to_owned(),
                        NodeGroupRuntimeState::Failed { .. } => {
                            "无法读取分组运行状态，请启动 Relay 托管 Mihomo 后重试".to_owned()
                        }
                        _ => return,
                    };
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn select_node_group_member(
        &mut self,
        group_id: &str,
        node_name: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(group) = self
            .node_policy_groups
            .iter()
            .find(|group| group.id == group_id)
            .cloned()
        else {
            return;
        };
        if group.strategy != NodeGroupStrategy::Manual || !self.runtime.manages_node_policy_groups()
        {
            "当前分组不支持手动切换".clone_into(&mut self.status);
            cx.notify();
            return;
        }
        self.node_group_runtime_generation = self.node_group_runtime_generation.wrapping_add(1);
        let generation = self.node_group_runtime_generation;
        let Some(state) = self.node_group_runtime_states.get_mut(group_id) else {
            return;
        };
        if !state.begin_selection(generation, node_name) {
            "节点不在 Mihomo 当前候选列表中，请刷新后重试".clone_into(&mut self.status);
            cx.notify();
            return;
        }
        self.status = format!("正在将“{}”切换到“{}”", group.name, node_name);
        let runtime = self.runtime.clone();
        let selected = node_name.to_owned();
        let selected_for_request = selected.clone();
        let group_id = group.id.clone();
        let group_name = group.name.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result =
                executor
                    .spawn(async move {
                        runtime.select_node_group_node(&group_name, &selected_for_request)
                    })
                    .await;
            this.update(cx, |this, cx| {
                let Some(state) = this.node_group_runtime_states.get_mut(&group_id) else {
                    return;
                };
                let accepted = match result {
                    Ok(snapshot) => {
                        state.complete_refresh(generation, snapshot.current, snapshot.candidates)
                    }
                    Err(_error) => state.fail(generation),
                };
                if accepted {
                    this.status = match state {
                        NodeGroupRuntimeState::Ready { .. } => {
                            format!("已切换到“{selected}”；Mihomo 将保存本次选择")
                        }
                        NodeGroupRuntimeState::Failed { .. } => {
                            "切换失败，请刷新分组状态后重试".to_owned()
                        }
                        _ => return,
                    };
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn start_source_group_benchmark(
        &mut self,
        id: &str,
        name: &str,
        candidate_names: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        let key = Self::source_group_benchmark_key(id);
        if matches!(
            self.group_benchmarks.get(&key),
            Some(GroupBenchmarkState::Running { .. })
        ) {
            return;
        }
        if candidate_names.is_empty() {
            "当前导入分组没有可测速节点".clone_into(&mut self.status);
            cx.notify();
            return;
        }
        if candidate_names.len() > MAX_GROUP_BENCHMARK_NODES {
            format!(
                "分组包含 {} 个节点，单次最多测试 {} 个",
                candidate_names.len(),
                MAX_GROUP_BENCHMARK_NODES
            )
            .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let Some(generation) = self.begin_group_benchmark(key.clone()) else {
            "已有分组正在测速，请等待完成后再试".clone_into(&mut self.status);
            cx.notify();
            return;
        };
        self.status = format!(
            "正在测试导入分组“{name}”的 {} 个节点",
            candidate_names.len()
        );
        trace_ui(UiEvent::GroupBenchmarkStarted);

        let runtime = self.runtime.clone();
        let group_name = name.to_owned();
        let total = candidate_names.len();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move { runtime.test_node_group_delay(&group_name, &candidate_names) })
                .await;
            this.update(cx, |this, cx| {
                if this.group_benchmark_active_generation != Some(generation) {
                    return;
                }
                this.group_benchmark_active_generation = None;
                let Some(state) = this.group_benchmarks.get_mut(&key) else {
                    cx.notify();
                    return;
                };
                let accepted = match result {
                    Ok(delays) => state.complete(generation, total, delays),
                    Err(_error) => state.fail(generation),
                };
                if !accepted {
                    return;
                }
                match state {
                    GroupBenchmarkState::Complete { summary, .. } => {
                        trace_ui(UiEvent::GroupBenchmarkSucceeded);
                        this.status = format!(
                            "导入分组测速完成：{}/{} 个节点成功",
                            summary.succeeded, summary.total
                        );
                    }
                    GroupBenchmarkState::Failed { .. } => {
                        trace_ui(UiEvent::GroupBenchmarkFailed);
                        "导入分组测速失败，请检查 Mihomo 连接与网络后重试"
                            .clone_into(&mut this.status);
                    }
                    _ => return,
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    #[allow(clippy::too_many_lines)]
    fn start_node_group_benchmark(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(group) = self
            .node_policy_groups
            .iter()
            .find(|group| group.id == id)
            .cloned()
        else {
            return;
        };
        if matches!(
            self.group_benchmarks
                .get(&Self::user_group_benchmark_key(id)),
            Some(GroupBenchmarkState::Running { .. })
        ) {
            return;
        }
        if self.group_benchmark_active_generation.is_some() {
            "已有分组正在测速，请等待完成后再试".clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let candidate_names = self.node_group_candidate_names(&group);
        if candidate_names.is_empty() {
            "当前分组没有可测速节点，请先调整匹配规则".clone_into(&mut self.status);
            cx.notify();
            return;
        }
        if candidate_names.len() > MAX_GROUP_BENCHMARK_NODES {
            format!(
                "分组包含 {} 个节点，请先用名称或明确选择收窄到 {} 个以内",
                candidate_names.len(),
                MAX_GROUP_BENCHMARK_NODES
            )
            .clone_into(&mut self.status);
            cx.notify();
            return;
        }

        let benchmark_key = Self::user_group_benchmark_key(&group.id);
        let Some(generation) = self.begin_group_benchmark(benchmark_key.clone()) else {
            "已有分组正在测速，请等待完成后再试".clone_into(&mut self.status);
            cx.notify();
            return;
        };
        self.status = format!(
            "正在测试分组“{}”的 {} 个节点",
            group.name,
            candidate_names.len()
        );
        trace_ui(UiEvent::GroupBenchmarkStarted);

        let runtime = self.runtime.clone();
        let group_id = group.id.clone();
        let group_name = group.name.clone();
        let refresh_after_success = group.strategy == NodeGroupStrategy::LowestLatency
            && self.selected_node_group_id.as_deref() == Some(group.id.as_str());
        let total = candidate_names.len();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move { runtime.test_node_group_delay(&group_name, &candidate_names) })
                .await;
            this.update(cx, |this, cx| {
                if this.group_benchmark_active_generation != Some(generation) {
                    return;
                }
                this.group_benchmark_active_generation = None;
                if !this
                    .node_policy_groups
                    .iter()
                    .any(|group| group.id == group_id)
                {
                    cx.notify();
                    return;
                }
                let (accepted, succeeded) = {
                    let Some(state) = this.group_benchmarks.get_mut(&benchmark_key) else {
                        cx.notify();
                        return;
                    };
                    let accepted = match result {
                        Ok(delays) => state.complete(generation, total, delays),
                        Err(_error) => state.fail(generation),
                    };
                    let succeeded = matches!(state, GroupBenchmarkState::Complete { .. });
                    if accepted {
                        this.status = match state {
                            GroupBenchmarkState::Complete { summary, .. } => {
                                trace_ui(UiEvent::GroupBenchmarkSucceeded);
                                format!(
                                    "分组测速完成：{}/{} 个节点成功",
                                    summary.succeeded, summary.total
                                )
                            }
                            GroupBenchmarkState::Failed { .. } => {
                                trace_ui(UiEvent::GroupBenchmarkFailed);
                                "分组测速失败，请检查 Mihomo 连接与网络后重试".to_owned()
                            }
                            _ => return,
                        };
                    }
                    (accepted, succeeded)
                };
                if accepted && succeeded && refresh_after_success {
                    this.refresh_node_group_runtime(&group_id, cx);
                } else if accepted {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn start_node_group_create(&mut self, cx: &mut Context<Self>) {
        self.node_group_draft = Some(NodeGroupDraft {
            editing_id: None,
            icon: NodeGroupIcon::Bolt,
            strategy: NodeGroupStrategy::Manual,
            test_interval_secs: 600,
            matcher_kind: NodeGroupMatcherKind::All,
            explicit_members: BTreeSet::new(),
        });
        if let Some(input) = self.node_group_name_input.as_ref() {
            input.update(
                cx,
                crate::subscription_input::SubscriptionTextInput::clear_without_event,
            );
        }
        if let Some(input) = self.node_group_filter_input.as_ref() {
            input.update(
                cx,
                crate::subscription_input::SubscriptionTextInput::clear_without_event,
            );
        }
        "正在创建节点分组".clone_into(&mut self.status);
        cx.notify();
    }

    fn start_node_group_edit(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(group) = self
            .node_policy_groups
            .iter()
            .find(|group| group.id == id)
            .cloned()
        else {
            return;
        };
        let (matcher_kind, filter, explicit_members) = match &group.matcher {
            NodeGroupMatcher::All => (NodeGroupMatcherKind::All, "", BTreeSet::new()),
            NodeGroupMatcher::NameContains(value) => (
                NodeGroupMatcherKind::NameContains,
                value.as_str(),
                BTreeSet::new(),
            ),
            NodeGroupMatcher::Explicit(members) => {
                (NodeGroupMatcherKind::Explicit, "", members.clone())
            }
        };
        self.node_group_draft = Some(NodeGroupDraft {
            editing_id: Some(group.id),
            icon: group.icon,
            strategy: group.strategy,
            test_interval_secs: group.test_interval_secs,
            matcher_kind,
            explicit_members,
        });
        if let Some(input) = self.node_group_name_input.as_ref() {
            input.update(cx, |input, cx| {
                input.set_value_without_event(group.name.clone(), cx);
            });
        }
        if let Some(input) = self.node_group_filter_input.as_ref() {
            input.update(cx, |input, cx| {
                input.set_value_without_event(filter.to_owned(), cx);
            });
        }
        self.status = format!("正在编辑分组“{}”", group.name);
        cx.notify();
    }

    fn save_node_policy_group(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.node_group_draft.clone() else {
            return;
        };
        let name = self
            .node_group_name_input
            .as_ref()
            .map(|input| input.read(cx).value().trim().to_owned())
            .unwrap_or_default();
        let filter = self
            .node_group_filter_input
            .as_ref()
            .map(|input| input.read(cx).value().trim().to_owned())
            .unwrap_or_default();
        let id = draft
            .editing_id
            .clone()
            .unwrap_or_else(mihomo::new_node_policy_group_id);
        let Ok(mut group) = NodePolicyGroup::new(&id, &name) else {
            "分组名称不能为空，也不能包含换行或控制字符".clone_into(&mut self.status);
            cx.notify();
            return;
        };
        if self
            .node_policy_groups
            .iter()
            .any(|existing| existing.id != id && existing.name == name)
        {
            "已有同名节点分组，请换一个名称".clone_into(&mut self.status);
            cx.notify();
            return;
        }
        if matches!(name.as_str(), "Auto" | "Proxy") {
            "“Auto”和“Proxy”是 Relay 保留的策略组名称".clone_into(&mut self.status);
            cx.notify();
            return;
        }
        group.icon = draft.icon;
        group.strategy = draft.strategy;
        if group
            .set_test_interval_secs(draft.test_interval_secs)
            .is_err()
        {
            "自动检查间隔无效".clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let matcher = match draft.matcher_kind {
            NodeGroupMatcherKind::All => NodeGroupMatcher::All,
            NodeGroupMatcherKind::NameContains => {
                let Ok(matcher) = NodeGroupMatcher::name_contains(&filter) else {
                    "请填写要匹配的节点名称".clone_into(&mut self.status);
                    cx.notify();
                    return;
                };
                matcher
            }
            NodeGroupMatcherKind::Explicit => {
                if draft.explicit_members.is_empty() {
                    "请至少选择一个节点".clone_into(&mut self.status);
                    cx.notify();
                    return;
                }
                NodeGroupMatcher::Explicit(draft.explicit_members)
            }
        };
        if group.set_matcher(matcher).is_err() || self.node_group_match_count(&group) == 0 {
            "当前规则没有匹配到任何已导入节点".clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            "无法确定节点分组保存位置".clone_into(&mut self.status);
            cx.notify();
            return;
        };
        if let Err(error) = mihomo::save_node_policy_group_in(&store_dir, &group) {
            self.status = format!("节点分组保存失败：{error}");
            cx.notify();
            return;
        }
        if let Some(existing) = self
            .node_policy_groups
            .iter_mut()
            .find(|existing| existing.id == group.id)
        {
            existing.clone_from(&group);
        } else {
            self.node_policy_groups.push(group.clone());
            self.node_policy_groups
                .sort_by(|left, right| left.id.cmp(&right.id));
        }
        self.group_benchmarks
            .remove(&Self::user_group_benchmark_key(&group.id));
        self.node_group_runtime_states.remove(&group.id);
        self.node_group_draft = None;
        self.status = format!("分组“{}”已保存，正在应用托管配置", group.name);
        self.apply_node_policy_groups(store_dir, format!("分组“{}”已保存", group.name), cx);
    }

    fn remove_node_policy_group(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            return;
        };
        let Some(index) = self
            .node_policy_groups
            .iter()
            .position(|group| group.id == id)
        else {
            return;
        };
        if let Err(error) = mihomo::remove_node_policy_group_in(&store_dir, id) {
            self.status = format!("节点分组删除失败：{error}");
            cx.notify();
            return;
        }
        let group = self.node_policy_groups.remove(index);
        self.group_benchmarks
            .remove(&Self::user_group_benchmark_key(id));
        self.node_group_runtime_states.remove(id);
        if self.selected_node_group_id.as_deref() == Some(id) {
            self.selected_node_group_id = None;
        }
        if self
            .node_group_draft
            .as_ref()
            .and_then(|draft| draft.editing_id.as_deref())
            == Some(id)
        {
            self.node_group_draft = None;
        }
        self.status = format!("分组“{}”已删除，正在应用托管配置", group.name);
        self.apply_node_policy_groups(store_dir, format!("分组“{}”已删除", group.name), cx);
    }

    fn apply_node_policy_groups(
        &mut self,
        store_dir: std::path::PathBuf,
        prefix: String,
        cx: &mut Context<Self>,
    ) {
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let apply = executor
                .spawn(async move {
                    SourceRuntimeApply::from_result(runtime.apply_saved_sources(&store_dir))
                })
                .await;
            this.update(cx, |this, cx| {
                this.status = format!("{prefix}{}", apply.status_suffix());
                if let Some(selected) = this.selected_node_group_id.clone() {
                    this.refresh_node_group_runtime(&selected, cx);
                } else {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
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
                if !this.imported_subscriptions.is_empty() {
                    for subscription in &mut this.imported_subscriptions {
                        let kind = super::source_kind(&subscription.source);
                        subscription.state = ImportedSubscriptionState::Pending(kind);
                    }
                    this.restore_imported_subscriptions(cx);
                } else if !this.saved_vless_nodes.is_empty() {
                    "已保存节点不需要重新下载".clone_into(&mut this.status);
                    cx.notify();
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

    fn node_group_list(
        &self,
        groups: &[NodeSourceGroup<'_>],
        filter: NodeAvailabilityFilter,
        compact: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut list = div().flex().flex_col().gap_3();
        for group in groups {
            list = list.child(self.node_group(group, filter, compact, theme, cx));
        }
        list
    }

    #[allow(clippy::too_many_lines)]
    fn node_group(
        &self,
        group: &NodeSourceGroup<'_>,
        filter: NodeAvailabilityFilter,
        compact: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut counts = NodeCounts::from_provider_refs(&group.providers);
        counts.total += group.saved_nodes.len();
        counts.untested += group.saved_nodes.len();
        let visible_count = counts.count_for(filter);
        let collapsed = self.node_workspace.is_group_collapsed(&group.id);
        let group_id = group.id.clone();
        let benchmark_key = Self::source_group_benchmark_key(&group.id);
        let benchmark = self
            .group_benchmarks
            .get(&benchmark_key)
            .cloned()
            .unwrap_or_default();
        let benchmarking = benchmark.is_running();
        let benchmark_id = group.id.clone();
        let benchmark_name = group.name.clone();
        let candidate_names = group
            .providers
            .iter()
            .flat_map(|provider| provider.nodes.iter().map(|node| node.name.clone()))
            .chain(group.saved_nodes.iter().map(|node| node.name.clone()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let detail = match &benchmark {
            GroupBenchmarkState::Idle => group.detail.clone(),
            GroupBenchmarkState::Running { .. } => format!("{} · 正在测速…", group.detail),
            GroupBenchmarkState::Complete { summary, .. } => format!(
                "{} · 测速 {}/{} 成功",
                group.detail, summary.succeeded, summary.total
            ),
            GroupBenchmarkState::Failed { .. } => format!("{} · 测速失败", group.detail),
        };
        let action = if collapsed { "展开" } else { "收起" };
        let header = div()
            .id(format!("node-group-header-{}", group.id))
            .role(Role::Button)
            .aria_label(format!("{action}节点分组{}", group.name))
            .tab_stop(true)
            .focusable()
            .cursor_pointer()
            .min_h(px(58.0))
            .px(if compact { px(12.0) } else { px(16.0) })
            .py_3()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .bg(theme.surface_low)
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .overflow_x_hidden()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(Self::group_benchmark_icon(
                        &benchmark_key,
                        benchmarking,
                        theme,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if !benchmarking {
                                this.start_source_group_benchmark(
                                    &benchmark_id,
                                    &benchmark_name,
                                    candidate_names.clone(),
                                    cx,
                                );
                            }
                        }),
                    ))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .child(
                                div()
                                    .overflow_x_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(group.name.clone()),
                            )
                            .child(
                                div()
                                    .mt_1()
                                    .text_size(px(10.0))
                                    .text_color(theme.text_tertiary)
                                    .child(detail),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme.text_secondary)
                            .child(format!("{} 个节点", counts.total)),
                    )
                    .child(
                        div()
                            .min_w(px(32.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.action_primary)
                            .child(action),
                    ),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.node_workspace.toggle_group(&group_id);
                this.persist_node_workspace();
                "已更新节点分组展开状态".clone_into(&mut this.status);
                cx.notify();
            }));

        div()
            .rounded_md()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .overflow_hidden()
            .child(header)
            .when(!collapsed && visible_count == 0, |container| {
                container.child(
                    div()
                        .px_4()
                        .py_3()
                        .border_t_1()
                        .border_color(theme.outline_subtle)
                        .text_size(px(11.0))
                        .text_color(theme.text_secondary)
                        .child("这个分组中没有符合当前筛选的节点。"),
                )
            })
            .when(!collapsed && visible_count > 0, |container| {
                container.child(Self::node_group_table(
                    group, filter, &benchmark, compact, theme,
                ))
            })
    }

    fn node_group_table(
        group: &NodeSourceGroup<'_>,
        filter: NodeAvailabilityFilter,
        benchmark: &GroupBenchmarkState,
        compact: bool,
        theme: Theme,
    ) -> Div {
        let mut table = div();
        if !compact {
            table = table.child(Self::node_table_header(theme));
        }

        for (provider_index, provider) in group.providers.iter().enumerate() {
            for (node_index, node) in provider.nodes.iter().enumerate() {
                if !filter.includes(node.alive) {
                    continue;
                }
                table = table.child(Self::workspace_node_row(
                    format!("node-row-{}-{provider_index}-{node_index}", group.id),
                    node,
                    &group.name,
                    benchmark,
                    compact,
                    theme,
                ));
            }
        }
        for (node_index, node) in group.saved_nodes.iter().enumerate() {
            if !filter.includes(None) {
                continue;
            }
            let loaded = LoadedProviderNode {
                name: node.name.clone(),
                protocol: node.protocol.to_owned(),
                latency_label: None,
                alive: None,
            };
            table = table.child(Self::workspace_node_row(
                format!(
                    "node-row-{}-{}-{node_index}",
                    group.id,
                    group.providers.len()
                ),
                &loaded,
                &group.name,
                benchmark,
                compact,
                theme,
            ));
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
            .child(
                div()
                    .w(px(72.0))
                    .text_align(gpui::TextAlign::Right)
                    .child("延迟"),
            )
    }

    fn workspace_node_row(
        row_id: String,
        node: &LoadedProviderNode,
        source_name: &str,
        benchmark: &GroupBenchmarkState,
        compact: bool,
        theme: Theme,
    ) -> Stateful<Div> {
        let latency = SourceNodeLatencyState::from_benchmark(node, benchmark);
        let spinner_id = format!("{row_id}-latency");
        let content = if compact {
            Self::compact_node_row_content(source_name, node, &latency, &spinner_id, theme)
        } else {
            Self::wide_node_row_content(source_name, node, &latency, &spinner_id, theme)
        };
        div()
            .id(row_id)
            .min_h(if compact { px(64.0) } else { px(52.0) })
            .px(if compact { px(12.0) } else { px(16.0) })
            .py_2()
            .border_t_1()
            .border_color(theme.outline_subtle)
            .child(content)
    }

    fn compact_node_row_content(
        source_name: &str,
        node: &LoadedProviderNode,
        latency: &SourceNodeLatencyState,
        spinner_id: &str,
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
                            .child(format!("{source_name} · {}", node.protocol)),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .min_w(px(48.0))
                    .text_align(gpui::TextAlign::Right)
                    .child(
                        div()
                            .min_h(px(18.0))
                            .flex()
                            .items_center()
                            .justify_end()
                            .child(Self::source_node_latency_content(
                                latency, spinner_id, theme,
                            )),
                    ),
            )
    }

    fn wide_node_row_content(
        source_name: &str,
        node: &LoadedProviderNode,
        latency: &SourceNodeLatencyState,
        spinner_id: &str,
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
                    .child(source_name.to_owned()),
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
                    .w(px(72.0))
                    .min_h(px(18.0))
                    .flex()
                    .items_center()
                    .justify_end()
                    .child(Self::source_node_latency_content(
                        latency, spinner_id, theme,
                    )),
            )
    }

    fn source_node_latency_content(
        latency: &SourceNodeLatencyState,
        spinner_id: &str,
        theme: Theme,
    ) -> Div {
        let cell = div()
            .min_w(px(42.0))
            .flex()
            .items_center()
            .justify_end()
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(px(11.0));
        match latency {
            SourceNodeLatencyState::Running => cell.child(Self::source_node_latency_spinner(
                spinner_id.to_owned(),
                theme,
            )),
            SourceNodeLatencyState::Measured(delay) => cell
                .text_color(theme.status_success)
                .child(format!("{delay} ms")),
            SourceNodeLatencyState::Failed => cell.text_color(theme.route_trace).child("失败"),
            SourceNodeLatencyState::Idle(previous) => cell
                .text_color(theme.text_tertiary)
                .child(previous.clone().unwrap_or_else(|| "—".to_owned())),
        }
    }

    fn source_node_latency_spinner(id: String, theme: Theme) -> impl IntoElement {
        div().relative().size(px(14.0)).with_animation(
            id,
            Animation::new(Duration::from_millis(720)).repeat(),
            move |spinner, delta| {
                let active = Self::source_node_latency_spinner_frame(delta);
                (0..8).fold(spinner, |spinner, index| {
                    spinner.child(Self::source_node_latency_dot(
                        index,
                        active,
                        theme.action_primary,
                    ))
                })
            },
        )
    }

    fn source_node_latency_spinner_frame(delta: f32) -> usize {
        if delta < 0.125 {
            0
        } else if delta < 0.25 {
            1
        } else if delta < 0.375 {
            2
        } else if delta < 0.5 {
            3
        } else if delta < 0.625 {
            4
        } else if delta < 0.75 {
            5
        } else if delta < 0.875 {
            6
        } else {
            7
        }
    }

    fn source_node_latency_dot(index: usize, active: usize, color: gpui::Rgba) -> Div {
        const POSITIONS: [(f32, f32); 8] = [
            (5.5, 0.0),
            (9.5, 1.5),
            (11.0, 5.5),
            (9.5, 9.5),
            (5.5, 11.0),
            (1.5, 9.5),
            (0.0, 5.5),
            (1.5, 1.5),
        ];
        const OPACITY: [f32; 8] = [1.0, 0.82, 0.68, 0.54, 0.42, 0.32, 0.24, 0.18];
        let distance = (index + 8 - active) % 8;
        let (left, top) = POSITIONS[index];
        div()
            .absolute()
            .left(px(left))
            .top(px(top))
            .size(px(3.0))
            .rounded_full()
            .bg(color.opacity(OPACITY[distance]))
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
    use std::collections::{BTreeMap, BTreeSet};

    use super::{NodeCounts, RelayApp, SourceNodeLatencyState};
    use crate::app::{GroupBenchmarkState, GroupBenchmarkSummary, NodeGroupRuntimeState};
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

    #[test]
    fn group_benchmark_summary_counts_failures_and_latency_range() {
        let summary = GroupBenchmarkSummary::from_delays(4, [80, 0, 42]);

        assert_eq!(summary.total, 4);
        assert_eq!(summary.succeeded, 2);
        assert_eq!(summary.failed, 2);
        assert_eq!(summary.minimum_ms, Some(42));
        assert_eq!(summary.maximum_ms, Some(80));
        assert_eq!(summary.average_ms, Some(61));
    }

    #[test]
    fn imported_node_latency_uses_running_and_failure_states_without_health_labels() {
        let node = LoadedProviderNode {
            name: "Saved Edge".to_owned(),
            protocol: "VLESS".to_owned(),
            latency_label: Some("88 ms".to_owned()),
            alive: Some(true),
        };

        assert_eq!(
            SourceNodeLatencyState::from_benchmark(
                &node,
                &GroupBenchmarkState::Running { generation: 2 },
            ),
            SourceNodeLatencyState::Running,
        );
        assert_eq!(
            SourceNodeLatencyState::from_benchmark(
                &node,
                &GroupBenchmarkState::Complete {
                    generation: 2,
                    summary: GroupBenchmarkSummary::default(),
                    delays: BTreeMap::new(),
                },
            ),
            SourceNodeLatencyState::Failed,
        );
        assert_eq!(
            SourceNodeLatencyState::from_benchmark(
                &node,
                &GroupBenchmarkState::Complete {
                    generation: 2,
                    summary: GroupBenchmarkSummary::default(),
                    delays: BTreeMap::from([("Saved Edge".to_owned(), 47)]),
                },
            ),
            SourceNodeLatencyState::Measured(47),
        );
    }

    #[test]
    fn imported_node_latency_spinner_advances_through_eight_frames() {
        assert_eq!(RelayApp::source_node_latency_spinner_frame(0.0), 0);
        assert_eq!(RelayApp::source_node_latency_spinner_frame(0.124), 0);
        assert_eq!(RelayApp::source_node_latency_spinner_frame(0.125), 1);
        assert_eq!(RelayApp::source_node_latency_spinner_frame(0.5), 4);
        assert_eq!(RelayApp::source_node_latency_spinner_frame(0.875), 7);
        assert_eq!(RelayApp::source_node_latency_spinner_frame(1.0), 7);
    }

    #[test]
    fn group_benchmark_state_ignores_a_stale_completion() {
        let mut state = GroupBenchmarkState::Running { generation: 7 };
        let outdated = BTreeMap::from([("Tokyo".to_owned(), 90)]);
        assert!(!state.complete(6, 2, outdated));
        assert_eq!(state, GroupBenchmarkState::Running { generation: 7 });

        let current = BTreeMap::from([("Tokyo".to_owned(), 55), ("Singapore".to_owned(), 75)]);
        assert!(state.complete(7, 2, current));
        assert!(matches!(
            &state,
            GroupBenchmarkState::Complete {
                summary: GroupBenchmarkSummary {
                    average_ms: Some(65),
                    ..
                },
                delays,
                ..
            } if delays.get("Tokyo") == Some(&55)
        ));
    }

    #[test]
    fn benchmark_state_reports_running_only_for_active_variant() {
        assert!(GroupBenchmarkState::Running { generation: 1 }.is_running());
        assert!(!GroupBenchmarkState::Idle.is_running());
        assert!(!GroupBenchmarkState::Failed { generation: 1 }.is_running());
    }

    #[test]
    fn group_runtime_state_rejects_stale_refresh_and_unknown_selection() {
        let mut state = NodeGroupRuntimeState::Loading { generation: 4 };
        assert!(!state.complete_refresh(
            3,
            Some("Tokyo".to_owned()),
            BTreeSet::from(["Tokyo".to_owned()]),
        ));
        assert!(state.complete_refresh(
            4,
            Some("Tokyo".to_owned()),
            BTreeSet::from(["Tokyo".to_owned(), "Singapore".to_owned()]),
        ));
        assert!(!state.begin_selection(5, "Unknown"));
        assert!(state.begin_selection(5, "Singapore"));
        assert!(matches!(
            state,
            NodeGroupRuntimeState::Selecting {
                generation: 5,
                ref pending,
                ..
            } if pending == "Singapore"
        ));
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
