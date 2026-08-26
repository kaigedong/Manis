use std::collections::BTreeSet;

use gpui::{
    Context, Div, FontWeight, ParentElement, Role, Stateful, Styled, Toggled, div, prelude::*, px,
};
use manis_core::{
    NodeAvailabilityFilter, NodeGroupIcon, NodeGroupMatcher, NodeGroupStrategy, NodeIdentity,
    NodePolicyGroup, PolicyGroupId, PrimaryWorkspace, ProxyId, WindowSizeClass,
};

use super::{
    GroupBenchmarkNodeState, GroupBenchmarkState, ImportedSubscriptionState, ManisApp,
    NodeGroupDraft, NodeGroupMatcherKind, NodeGroupRuntimeState, SourceRuntimeApply,
};
use crate::{
    diagnostics::{UiEvent, trace_ui},
    localization::Language,
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

impl ManisApp {
    fn node_group_strategy_label(strategy: NodeGroupStrategy, language: Language) -> &'static str {
        match strategy {
            NodeGroupStrategy::Manual => language.text("Manual Select", "手动选择"),
            NodeGroupStrategy::LowestLatency => language.text("Lowest Latency", "延迟优选"),
        }
    }

    fn node_group_icon_label(icon: NodeGroupIcon, language: Language) -> &'static str {
        match icon {
            NodeGroupIcon::Bolt => language.text("Bolt", "闪电"),
            NodeGroupIcon::Globe => language.text("Globe", "地球"),
            NodeGroupIcon::Shield => language.text("Shield", "盾牌"),
            NodeGroupIcon::Compass => language.text("Compass", "罗盘"),
        }
    }

    fn availability_filter_label(
        filter: NodeAvailabilityFilter,
        language: Language,
    ) -> &'static str {
        match filter {
            NodeAvailabilityFilter::All => language.text("All", "全部"),
            NodeAvailabilityFilter::Available => language.text("Available", "可用"),
            NodeAvailabilityFilter::Unavailable => language.text("Unavailable", "不可用"),
            NodeAvailabilityFilter::Untested => language.text("Untested", "未测速"),
        }
    }

    fn source_count_label(count: usize, language: Language) -> String {
        match language {
            Language::English => format!("{count} sources"),
            Language::SimplifiedChinese => format!("{count} 个来源"),
        }
    }

    fn node_count_label(count: usize, language: Language) -> String {
        match language {
            Language::English => format!("{count} nodes"),
            Language::SimplifiedChinese => format!("{count} 个节点"),
        }
    }

    fn candidate_count_label(count: usize, language: Language) -> String {
        match language {
            Language::English => format!("{count} candidate nodes"),
            Language::SimplifiedChinese => format!("{count} 个候选节点"),
        }
    }

    fn matched_count_label(count: usize, language: Language) -> String {
        match language {
            Language::English => format!("{count} matched"),
            Language::SimplifiedChinese => format!("匹配 {count} 个"),
        }
    }

    fn check_interval_seconds_label(seconds: u32, language: Language) -> String {
        match language {
            Language::English => format!("checks every {seconds}s"),
            Language::SimplifiedChinese => format!("每 {seconds} 秒检查"),
        }
    }

    fn success_fraction_label(succeeded: usize, total: usize, language: Language) -> String {
        match language {
            Language::English => format!("{succeeded}/{total} succeeded"),
            Language::SimplifiedChinese => format!("{succeeded}/{total} 成功"),
        }
    }

    fn group_limit_label(count: usize, language: Language) -> String {
        match language {
            Language::English => format!("group contains {count} nodes"),
            Language::SimplifiedChinese => format!("分组包含 {count} 个节点"),
        }
    }

    fn single_test_limit_label(limit: usize, language: Language) -> String {
        match language {
            Language::English => format!("a single test supports up to {limit}"),
            Language::SimplifiedChinese => format!("单次最多测试 {limit} 个"),
        }
    }

    fn narrow_group_limit_label(limit: usize, language: Language) -> String {
        match language {
            Language::English => {
                format!("narrow it to {limit} nodes or fewer by name or explicit selection")
            }
            Language::SimplifiedChinese => {
                format!("请先用名称或明确选择收窄到 {limit} 个以内")
            }
        }
    }

    fn source_runtime_apply_suffix(apply: &SourceRuntimeApply, language: Language) -> String {
        match apply {
            SourceRuntimeApply::Applied(mihomo::GeneratedProfileApply::NotManaged) => language
                .text(
                    " · external/existing config is active, so config was not rewritten",
                    " · 当前为外部/已有配置，未改写其配置",
                )
                .to_owned(),
            SourceRuntimeApply::Applied(mihomo::GeneratedProfileApply::Updated) => language
                .text(
                    " · written to Manis-managed config",
                    " · 已写入 Manis 托管配置",
                )
                .to_owned(),
            SourceRuntimeApply::Applied(mihomo::GeneratedProfileApply::Restarted) => language
                .text(
                    " · Manis-managed kernel reloaded safely",
                    " · Manis 托管内核已安全重载",
                )
                .to_owned(),
            SourceRuntimeApply::Failed(message) => match language {
                Language::English => {
                    format!(" · saved, but managed config apply failed: {message}")
                }
                Language::SimplifiedChinese => {
                    format!(" · 持久化已完成，但托管配置应用失败：{message}")
                }
            },
        }
    }

    pub(super) fn node_workspace(
        &self,
        theme: Theme,
        size_class: WindowSizeClass,
        cx: &mut Context<Self>,
    ) -> Div {
        let compact = size_class == WindowSizeClass::Compact;
        let has_local_sources =
            !self.imported_subscriptions.is_empty() || !self.saved_vless_nodes.is_empty();
        let language = self.language();
        let groups = self.node_source_groups(has_local_sources, language);
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
            language.text("Local sources", "本机来源")
        } else if self.source_providers.is_empty() {
            language.text("No node sources", "尚无节点来源")
        } else {
            language.text("Current Mihomo", "当前 Mihomo")
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
                language,
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
                language,
                theme,
                cx,
            ))
    }

    fn node_source_groups(
        &self,
        has_local_sources: bool,
        language: Language,
    ) -> Vec<NodeSourceGroup<'_>> {
        if has_local_sources {
            let mut groups: Vec<_> = self
                .imported_subscriptions
                .iter()
                .enumerate()
                .map(|(index, subscription)| {
                    let name = subscription.source.subscription_name().unwrap_or_else(|| {
                        format!("{} {}", language.text("Subscription", "订阅"), index + 1)
                    });
                    let transport = if subscription.source.is_https() {
                        language.text("HTTPS subscription", "HTTPS 订阅")
                    } else {
                        language.text("HTTP subscription", "HTTP 订阅")
                    };
                    let state = match subscription.state {
                        ImportedSubscriptionState::Pending(_)
                        | ImportedSubscriptionState::Refreshing(_) => {
                            language.text("Restoring", "正在恢复")
                        }
                        ImportedSubscriptionState::Ready(_) => {
                            language.text("Restores after restart", "重启后自动恢复")
                        }
                        ImportedSubscriptionState::Unavailable(_, _)
                        | ImportedSubscriptionState::StoreError(_) => {
                            language.text("Unavailable", "当前不可用")
                        }
                        ImportedSubscriptionState::Removing(_) => {
                            language.text("Removing", "正在移除")
                        }
                        ImportedSubscriptionState::None => language.text("Not loaded", "尚未读取"),
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
                    name: language.text("Saved", "已保存").to_owned(),
                    detail: language
                        .text(
                            "Individually added VLESS nodes · private local storage",
                            "单独添加的 VLESS 节点 · 私有本机存储",
                        )
                        .to_owned(),
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
                    || language.text("Mihomo source", "Mihomo 来源").to_owned(),
                    |vehicle| {
                        format!(
                            "{} · {vehicle}",
                            language.text("Mihomo source", "Mihomo 来源")
                        )
                    },
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
        language: Language,
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
                                    .child(format!(
                                        "{} · {}",
                                        language.text("Nodes", "节点"),
                                        counts.total
                                    )),
                            )
                            .child(
                                div()
                                    .mt_1()
                                    .text_size(px(12.0))
                                    .text_color(theme.text_secondary)
                                    .child(format!(
                                        "{origin} · {} · {}",
                                        Self::source_count_label(source_count, language),
                                        language.text(
                                            "Review exit health and global selections here",
                                            "在这里查看出口健康状态"
                                        )
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(Self::node_refresh_button(refreshing, language, theme, cx))
                            .child(Self::node_configuration_link(language, theme, cx)),
                    ),
            )
            .child(Self::node_health_summary(counts, compact, language, theme))
            .child(Self::node_filter_bar(
                counts, filter, compact, language, theme, cx,
            ))
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
        language: Language,
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
                language.text("Imported Nodes", "导入的节点"),
                language.text(
                    "Review imported nodes by source; choose one exit for global mode.",
                    "按来源查看已经导入的节点；可为全局模式指定一个出口。",
                ),
                theme,
            ))
            .when(loading && groups.is_empty(), |body| {
                body.child(Self::node_message_panel(
                    language.text("Restoring nodes", "正在恢复节点"),
                    language.text(
                        "Manis is reloading imported subscriptions through isolated Mihomo.",
                        "Manis 正在通过隔离 Mihomo 重新读取已导入订阅。",
                    ),
                    theme,
                ))
            })
            .when(unavailable && groups.is_empty(), |body| {
                body.child(Self::node_message_panel(
                    language.text("Nodes are temporarily unavailable", "暂时无法读取节点"),
                    language.text(
                        "Subscriptions remain safely stored locally. Check sources in Configuration; original URLs stay hidden.",
                        "订阅仍安全保存在本机。请前往配置页检查来源，原链接不会显示。",
                    ),
                    theme,
                ))
            })
            .when(!loading && !unavailable && groups.is_empty(), |body| {
                body.child(Self::node_empty_state(compact, language, theme, cx))
            })
            .when(!groups.is_empty(), |body| {
                body.child(self.node_group_list(groups, filter, compact, language, theme, cx))
            })
            .child(self.node_group_workspace_section(wide, compact, language, theme, cx))
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
        language: Language,
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
                        language.text("Node Groups", "节点分组"),
                        language.text(
                            "Organize imported nodes into routing-ready policy groups with selection strategies and match rules.",
                            "用选择策略与匹配规则，把上方节点组织成可用于规则路由的策略组。",
                        ),
                        theme,
                    ))
                    .child(Self::node_group_add_button(language, theme, cx)),
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
                            .child(language.text("No custom groups yet", "还没有自定义分组")),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_size(px(11.0))
                            .text_color(theme.text_secondary)
                            .child(language.text(
                                "Create a manual group, or let Manis automatically choose the lowest-latency node.",
                                "可以创建手动选择组，或让 Manis 自动选择最低延迟节点。",
                            )),
                    ),
            );
        }
        let mut cards = div().grid().grid_cols(if wide { 2 } else { 1 }).gap_3();
        for group in &self.node_policy_groups {
            cards = cards.child(self.node_policy_group_card(group, language, theme, cx));
        }
        if !self.node_policy_groups.is_empty() {
            section = section.child(cards);
        }
        if let Some(group) = self.selected_node_group_id.as_ref().and_then(|selected| {
            self.node_policy_groups
                .iter()
                .find(|group| group.id == *selected)
        }) {
            section =
                section.child(self.node_policy_group_detail(group, compact, language, theme, cx));
        }
        if let Some(draft) = self.node_group_draft.as_ref() {
            section = section.child(self.node_group_editor(draft, compact, language, theme, cx));
        }
        section
    }

    fn node_group_add_button(
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id("node-group-add")
            .role(Role::Button)
            .aria_label(language.text("Add node group", "添加节点分组"))
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
            .child(language.text("Add Group", "添加分组"))
            .on_click(cx.listener(|this, _, _, cx| this.start_node_group_create(cx)))
    }

    #[allow(clippy::too_many_lines)]
    fn node_policy_group_card(
        &self,
        group: &NodePolicyGroup,
        language: Language,
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
            NodeGroupMatcher::All => language.text("All nodes", "全部节点").to_owned(),
            NodeGroupMatcher::NameContains(value) => {
                format!("{} “{value}”", language.text("Name contains", "名称包含"))
            }
            NodeGroupMatcher::Explicit(nodes) => format!(
                "{} {}",
                language.text("Explicitly selected", "明确选择"),
                Self::node_count_label(nodes.len(), language)
            ),
        };
        let strategy_summary = if group.strategy == NodeGroupStrategy::LowestLatency {
            format!(
                "{} · {}",
                Self::node_group_strategy_label(group.strategy, language),
                Self::check_interval_seconds_label(group.test_interval_secs, language)
            )
        } else {
            Self::node_group_strategy_label(group.strategy, language).to_owned()
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
                    .child(Self::node_group_icon_badge(group.icon, language, theme))
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
                                        "{strategy_summary} · {matcher_summary} · {}",
                                        Self::matched_count_label(matched, language)
                                    )),
                            ),
                    ),
            )
            .child(Self::node_group_benchmark_status(
                &benchmark, matched, language, theme,
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
                            language.text("Hide Details", "收起详情")
                        } else {
                            language.text("View Details", "查看详情")
                        },
                        theme,
                        cx.listener(move |this, _, _, cx| {
                            this.toggle_node_group_detail(&detail_id, cx);
                        }),
                    ))
                    .child(Self::node_group_text_button(
                        format!("node-group-edit-{group_id}"),
                        language.text("Edit", "编辑"),
                        theme,
                        cx.listener(move |this, _, _, cx| {
                            this.start_node_group_edit(&group_id, cx);
                        }),
                    ))
                    .child(Self::node_group_text_button(
                        format!("node-group-remove-{remove_id}"),
                        language.text("Delete", "删除"),
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
        language: Language,
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
        let preferred_target = self.node_selection_preferences.policy_target(&group.name);
        let mut list = div().mt_3().border_t_1().border_color(theme.outline_subtle);
        for member in members {
            list = list.child(Self::node_group_member_row(
                group,
                member,
                &runtime_state,
                preferred_target,
                self.policy_selection_busy.is_some(),
                &benchmark,
                compact,
                language,
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
                                        "{} · {}",
                                        Self::node_group_strategy_label(group.strategy, language),
                                        Self::candidate_count_label(
                                            self.node_group_match_count(group),
                                            language,
                                        )
                                    )),
                            ),
                    )
                    .child(Self::node_group_text_button(
                        format!("node-group-detail-close-{close_id}"),
                        language.text("Close", "关闭"),
                        theme,
                        cx.listener(move |this, _, _, cx| {
                            this.toggle_node_group_detail(&close_id, cx);
                        }),
                    )),
            )
            .child(Self::node_group_runtime_banner(
                group,
                &runtime_state,
                language,
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
                        .child(language.text(
                            "This rule does not match any nodes. Edit the group rule.",
                            "当前规则没有匹配到节点，请编辑分组规则。",
                        )),
                )
            })
            .when(self.node_group_match_count(group) > 0, |detail| {
                detail.child(list)
            })
    }

    #[allow(clippy::too_many_lines)]
    fn node_group_runtime_banner(
        group: &NodePolicyGroup,
        state: &NodeGroupRuntimeState,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let (title, detail, color) = match state {
            NodeGroupRuntimeState::LocalOnly if group.strategy == NodeGroupStrategy::Manual => (
                language
                    .text("Manual policy · saved locally", "手动策略 · 本地保存")
                    .to_owned(),
                language
                    .text(
                        "Choose a node now. Manis applies the saved selection when its managed kernel connects.",
                        "现在即可选择节点；Manis 托管内核连接后会应用已保存的选择。",
                    )
                    .to_owned(),
                theme.action_primary,
            ),
            NodeGroupRuntimeState::LocalOnly => (
                language
                    .text("Automatic policy", "自动策略")
                    .to_owned(),
                language
                    .text(
                        "Mihomo selects the best candidate automatically after the managed kernel starts.",
                        "托管内核启动后由 Mihomo 自动选择最合适的候选项。",
                    )
                    .to_owned(),
                theme.text_secondary,
            ),
            NodeGroupRuntimeState::Loading { .. } => (
                language.text("Loading current exit...", "正在读取当前出口…").to_owned(),
                language
                    .text(
                        "Syncing policy group state from Manis-managed Mihomo.",
                        "正在从 Manis 托管 Mihomo 同步策略组状态。",
                    )
                    .to_owned(),
                theme.action_primary,
            ),
            NodeGroupRuntimeState::Ready { current, .. } => {
                let current = current
                    .as_deref()
                    .unwrap_or_else(|| language.text("Not selected", "尚未选择"));
                if group.strategy == NodeGroupStrategy::Manual {
                    (
                        format!("{}: {current}", language.text("Current", "当前使用")),
                        language
                            .text(
                                "Selecting another node applies immediately and Mihomo saves it.",
                                "选择其他节点后会立即应用，并由 Mihomo 保存选择。",
                            )
                            .to_owned(),
                        theme.status_success,
                    )
                } else {
                    (
                        format!("{}: {current}", language.text("Preferred", "当前优选")),
                        language
                            .text(
                                "Lowest-latency groups are tested and switched by Mihomo automatically; manual selection is disabled.",
                                "最低延迟组由 Mihomo 自动测试并切换，不支持手动指定。",
                            )
                            .to_owned(),
                        theme.status_success,
                    )
                }
            }
            NodeGroupRuntimeState::Selecting { pending, .. } => (
                format!("{}: {pending}", language.text("Switching to", "正在切换到")),
                language
                    .text(
                        "Waiting for Mihomo to confirm the new current exit.",
                        "等待 Mihomo 确认新的当前出口。",
                    )
                    .to_owned(),
                theme.action_primary,
            ),
            NodeGroupRuntimeState::Failed { .. } => (
                language
                    .text(
                        "Could not load group runtime state",
                        "无法读取分组运行状态",
                    )
                    .to_owned(),
                language
                    .text(
                        "Start or connect Manis-managed Mihomo, then retry.",
                        "请启动或连接 Manis 托管 Mihomo 后重试。",
                    )
                    .to_owned(),
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
                        language.text("Retry", "重试"),
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
        preferred_target: Option<&str>,
        selection_busy: bool,
        benchmark: &GroupBenchmarkState,
        compact: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let runtime_current = match runtime_state {
            NodeGroupRuntimeState::Ready { current, .. }
            | NodeGroupRuntimeState::Selecting { current, .. } => current.as_deref(),
            _ => None,
        };
        let selected_target = if group.strategy == NodeGroupStrategy::Manual {
            preferred_target.or(runtime_current)
        } else {
            runtime_current
        };
        let is_current = selected_target == Some(member.identity.node_name.as_str());
        let runtime_confirmed = runtime_current == Some(member.identity.node_name.as_str());
        let selecting = matches!(
            runtime_state,
            NodeGroupRuntimeState::Selecting { pending, .. }
                if pending == &member.identity.node_name
        );
        let selectable = group.strategy == NodeGroupStrategy::Manual
            && !selection_busy
            && !matches!(runtime_state, NodeGroupRuntimeState::Selecting { .. })
            && !is_current;
        let latency = benchmark.node_state(&member.identity.node_name);
        let spinner_id = format!(
            "user-group-{}-{}-latency",
            group.id, member.identity.node_name
        );
        let (health, health_color) = match member.alive {
            Some(true) => (language.text("Available", "可用"), theme.status_success),
            Some(false) => (language.text("Unavailable", "不可用"), theme.route_trace),
            None => (language.text("Untested", "未检测"), theme.text_tertiary),
        };
        let group_id = group.id.clone();
        let node_name = member.identity.node_name.clone();
        let action_label = if selecting {
            language.text("Switching...", "切换中…")
        } else if is_current && runtime_confirmed {
            language.text("Current", "当前")
        } else if is_current {
            language.text("Selected", "已选")
        } else if selectable {
            language.text("Select", "选择")
        } else if group.strategy == NodeGroupStrategy::LowestLatency {
            language.text("Auto", "自动")
        } else {
            language.text("Read-only", "只读")
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
                    .min_h(px(18.0))
                    .flex()
                    .items_center()
                    .justify_end()
                    .child(Self::benchmark_latency_content(
                        latency,
                        member
                            .latency_label
                            .clone()
                            .unwrap_or_else(|| language.text("Untested", "未测速").to_owned()),
                        &spinner_id,
                        theme,
                    )),
            )
            .child(action)
    }

    fn node_group_benchmark_status(
        state: &GroupBenchmarkState,
        matched: usize,
        language: Language,
        theme: Theme,
    ) -> Div {
        let (label, color) = match state {
            GroupBenchmarkState::Idle => (
                language.text("Not tested yet", "尚未测速").to_owned(),
                theme.text_tertiary,
            ),
            GroupBenchmarkState::Running { .. } => (
                format!(
                    "{} {}...",
                    language.text("Testing", "正在测试"),
                    Self::node_count_label(matched, language)
                ),
                theme.action_primary,
            ),
            GroupBenchmarkState::Complete { summary, .. } => {
                let label = match (summary.average_ms, summary.minimum_ms, summary.maximum_ms) {
                    (Some(average), Some(minimum), Some(maximum)) => format!(
                        "{} {average} ms · {} {minimum} ms · {} {maximum} ms · {}",
                        language.text("Avg", "平均"),
                        language.text("Min", "最低"),
                        language.text("Max", "最高"),
                        Self::success_fraction_label(summary.succeeded, summary.total, language)
                    ),
                    _ => Self::success_fraction_label(0, summary.total, language),
                };
                (label, theme.status_success)
            }
            GroupBenchmarkState::Failed { .. } => (
                language
                    .text(
                        "Test failed · check Mihomo connection and network, then retry",
                        "测速失败 · 请检查 Mihomo 连接与网络后重试",
                    )
                    .to_owned(),
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

    fn node_group_icon_badge(icon: NodeGroupIcon, language: Language, theme: Theme) -> Div {
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
            .child(Self::node_group_icon_label(icon, language))
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
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let title = if draft.editing_id.is_some() {
            language.text("Edit Node Group", "编辑节点分组")
        } else {
            language.text("Create Node Group", "创建节点分组")
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
                    .child(Self::node_group_field_label(
                        language.text("Group Name", "分组名称"),
                        theme,
                    ))
                    .when_some(name_input, ParentElement::child),
            )
            .child(
                div()
                    .mt_4()
                    .child(Self::node_group_field_label(
                        language.text("Group Icon", "分组图标"),
                        theme,
                    ))
                    .child(Self::node_group_icon_selector(draft.icon, language, theme, cx)),
            )
            .child(
                div()
                    .mt_4()
                    .child(Self::node_group_field_label(
                        language.text("Selection Strategy", "选择策略"),
                        theme,
                    ))
                    .child(Self::node_group_strategy_selector(
                        draft.strategy,
                        language,
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
                            .child(Self::node_group_field_label(
                                language.text("Automatic Recheck", "自动重新检查"),
                                theme,
                            ))
                            .child(Self::node_group_interval_selector(
                                draft.test_interval_secs,
                                language,
                                theme,
                                cx,
                            ))
                            .child(Self::node_group_helper(
                                language.text(
                                    "Each automatic group stores its own interval and writes it into Manis-managed Mihomo config.",
                                    "每个自动策略组独立保存间隔，并写入 Manis 托管的 Mihomo 配置。",
                                ),
                                theme,
                            )),
                    )
                },
            )
            .child(
                div()
                    .mt_4()
                    .child(Self::node_group_field_label(
                        language.text("Node Rule", "节点规则"),
                        theme,
                    ))
                    .child(Self::node_group_matcher_selector(
                        draft.matcher_kind,
                        language,
                        theme,
                        cx,
                    )),
            );
        match draft.matcher_kind {
            NodeGroupMatcherKind::All => {
                editor = editor.child(Self::node_group_helper(
                    language.text(
                        "All currently imported nodes will be included in this group.",
                        "当前导入的全部节点都会加入这个分组。",
                    ),
                    theme,
                ));
            }
            NodeGroupMatcherKind::NameContains => {
                editor = editor.child(
                    div()
                        .mt_3()
                        .child(Self::node_group_field_label(
                            language.text("Name Contains", "名称包含"),
                            theme,
                        ))
                        .when_some(filter_input, ParentElement::child),
                );
            }
            NodeGroupMatcherKind::Explicit => {
                editor = editor.child(self.node_group_member_picker(draft, language, theme, cx));
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
                    language.text("Cancel", "取消"),
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
                        .aria_label(language.text("Save node group", "保存节点分组"))
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
                        .child(language.text("Save Group", "保存分组"))
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
        language: Language,
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
                    .aria_label(format!(
                        "{} {}",
                        language.text("Use icon", "使用图标"),
                        Self::node_group_icon_label(icon, language)
                    ))
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
                    .child(Self::node_group_icon_label(icon, language))
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
        language: Language,
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
                    .aria_label(Self::node_group_strategy_label(strategy, language))
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
                    .child(Self::node_group_strategy_label(strategy, language))
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

    fn node_group_interval_selector(
        selected: u32,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let choices = [
            (60, language.text("1 min", "1 分钟")),
            (300, language.text("5 min", "5 分钟")),
            (600, language.text("10 min", "10 分钟")),
            (1_800, language.text("30 min", "30 分钟")),
        ];
        let mut row = div().flex().flex_wrap().gap_2();
        for (seconds, label) in choices {
            let active = selected == seconds;
            row = row.child(
                div()
                    .id(format!("node-group-interval-{seconds}"))
                    .role(Role::Button)
                    .aria_label(format!(
                        "{} {label}",
                        language.text("Automatically check every", "自动检查间隔")
                    ))
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
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let choices = [
            (
                language.text("All Nodes", "全部节点"),
                NodeGroupMatcherKind::All,
            ),
            (
                language.text("Name Contains", "名称包含"),
                NodeGroupMatcherKind::NameContains,
            ),
            (
                language.text("Explicit Selection", "明确选择"),
                NodeGroupMatcherKind::Explicit,
            ),
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
        language: Language,
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
                    .child(language.text(
                        "Import nodes above before making an explicit selection.",
                        "先在上方导入节点，再进行明确选择。",
                    )),
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
                    .aria_label(format!(
                        "{} {}",
                        language.text("Select node", "选择节点"),
                        member.node_name
                    ))
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
        for group in self.node_source_groups(has_local_sources, self.language()) {
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
        for source_group in self.node_source_groups(has_local_sources, self.language()) {
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

    pub(super) fn node_group_candidate_names(&self, group: &NodePolicyGroup) -> Vec<String> {
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
            self.language()
                .text(
                    "Group details opened; external controller remains read-only",
                    "已打开分组详情；当前外部控制器保持只读",
                )
                .clone_into(&mut self.status);
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
        let language = self.language();
        self.status = format!(
            "{} “{}”",
            language.text("Loading current exit for group", "正在读取分组当前出口"),
            group.name
        );
        let runtime = self.runtime.clone();
        let group_id = group.id.clone();
        let group_name = group.name.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move { runtime.load_node_group_runtime(&group_name) })
                .await;
            this.update(cx, |this, cx| {
                let language = this.language();
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
                        NodeGroupRuntimeState::Ready { .. } => language
                            .text(
                                "Policy group current exit synced",
                                "已同步策略组当前出口",
                            )
                            .to_owned(),
                        NodeGroupRuntimeState::LocalOnly => language
                            .text("Current controller remains read-only", "当前控制器保持只读")
                            .to_owned(),
                        NodeGroupRuntimeState::Failed { .. } => {
                            language
                                .text(
                                    "Could not load group runtime state. Start Manis-managed Mihomo and retry.",
                                    "无法读取分组运行状态，请启动 Manis 托管 Mihomo 后重试",
                                )
                                .to_owned()
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
        if group.strategy != NodeGroupStrategy::Manual {
            self.language()
                .text(
                    "This group does not support manual switching",
                    "当前分组不支持手动切换",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        if !self
            .node_group_candidate_names(&group)
            .iter()
            .any(|candidate| candidate == node_name)
        {
            self.language()
                .text(
                    "This node is not a candidate of the policy group",
                    "该节点不在当前策略组的候选项中",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        self.select_policy_node(
            PolicyGroupId::new(group.id),
            group.name,
            ProxyId::new(node_name),
            node_name.to_owned(),
            cx,
        );
    }

    #[allow(clippy::too_many_lines)]
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
            self.language()
                .text(
                    "This imported group has no nodes to test",
                    "当前导入分组没有可测速节点",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        if candidate_names.len() > MAX_GROUP_BENCHMARK_NODES {
            let language = self.language();
            format!(
                "{}; {}",
                Self::group_limit_label(candidate_names.len(), language),
                Self::single_test_limit_label(MAX_GROUP_BENCHMARK_NODES, language)
            )
            .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let Some(generation) = self.begin_group_benchmark(key.clone()) else {
            self.language()
                .text(
                    "A group test is already running. Wait for it to finish.",
                    "已有分组正在测速，请等待完成后再试",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        };
        let language = self.language();
        self.status = format!(
            "{} “{name}” · {}",
            language.text("Testing imported group", "正在测试导入分组"),
            Self::node_count_label(candidate_names.len(), language)
        );
        trace_ui(UiEvent::GroupBenchmarkStarted);

        let runtime = self.runtime.clone();
        let progress =
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new()));
        self.poll_group_benchmark_progress(generation, key.clone(), progress.clone(), cx);
        let total = candidate_names.len();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    runtime.test_proxy_delays_with_progress(
                        &candidate_names,
                        move |node_name, delay| {
                            if let Ok(mut updates) = progress.lock() {
                                updates.push_back((node_name.to_owned(), delay));
                            }
                        },
                    )
                })
                .await;
            this.update(cx, |this, cx| {
                let language = this.language();
                if this.group_benchmark_active_generation != Some(generation) {
                    return;
                }
                this.group_benchmark_active_generation = None;
                let Some(state) = this.group_benchmarks.get_mut(&key) else {
                    cx.notify();
                    return;
                };
                let failure = result.as_ref().err().map(ToString::to_string);
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
                            "{}: {}",
                            language.text("Imported group test completed", "导入分组测速完成"),
                            Self::success_fraction_label(
                                summary.succeeded,
                                summary.total,
                                language
                            )
                        );
                    }
                    GroupBenchmarkState::Failed { .. } => {
                        trace_ui(UiEvent::GroupBenchmarkFailed);
                        this.status = format!(
                            "{}：{}",
                            language.text("Imported group test failed", "导入分组测速失败"),
                            failure.as_deref().unwrap_or_else(
                                || language.text("unknown controller error", "未知控制器错误")
                            )
                        );
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
            self.language()
                .text(
                    "A group test is already running. Wait for it to finish.",
                    "已有分组正在测速，请等待完成后再试",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let candidate_names = self.node_group_candidate_names(&group);
        if candidate_names.is_empty() {
            self.language()
                .text(
                    "This group has no nodes to test. Adjust the match rule first.",
                    "当前分组没有可测速节点，请先调整匹配规则",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        if candidate_names.len() > MAX_GROUP_BENCHMARK_NODES {
            let language = self.language();
            format!(
                "{}; {}",
                Self::group_limit_label(candidate_names.len(), language),
                Self::narrow_group_limit_label(MAX_GROUP_BENCHMARK_NODES, language)
            )
            .clone_into(&mut self.status);
            cx.notify();
            return;
        }

        let benchmark_key = Self::user_group_benchmark_key(&group.id);
        let Some(generation) = self.begin_group_benchmark(benchmark_key.clone()) else {
            self.language()
                .text(
                    "A group test is already running. Wait for it to finish.",
                    "已有分组正在测速，请等待完成后再试",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        };
        let language = self.language();
        self.status = format!(
            "{} “{}” · {}",
            language.text("Testing group", "正在测试分组"),
            group.name,
            Self::node_count_label(candidate_names.len(), language)
        );
        trace_ui(UiEvent::GroupBenchmarkStarted);

        let runtime = self.runtime.clone();
        let group_id = group.id.clone();
        let group_name = group.name.clone();
        let use_group_api = group.strategy == NodeGroupStrategy::LowestLatency
            && runtime.manages_node_policy_groups();
        let refresh_after_success = group.strategy == NodeGroupStrategy::LowestLatency
            && self.selected_node_group_id.as_deref() == Some(group.id.as_str());
        let progress =
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new()));
        if !use_group_api {
            self.poll_group_benchmark_progress(
                generation,
                benchmark_key.clone(),
                progress.clone(),
                cx,
            );
        }
        let total = candidate_names.len();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    if use_group_api {
                        runtime.test_node_group_delay(&group_name, &candidate_names)
                    } else {
                        runtime.test_proxy_delays_with_progress(
                            &candidate_names,
                            move |node_name, delay| {
                                if let Ok(mut updates) = progress.lock() {
                                    updates.push_back((node_name.to_owned(), delay));
                                }
                            },
                        )
                    }
                })
                .await;
            this.update(cx, |this, cx| {
                let language = this.language();
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
                    let failure = result.as_ref().err().map(ToString::to_string);
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
                                    "{}: {}",
                                    language.text("Group test completed", "分组测速完成"),
                                    Self::success_fraction_label(
                                        summary.succeeded,
                                        summary.total,
                                        language,
                                    )
                                )
                            }
                            GroupBenchmarkState::Failed { .. } => {
                                trace_ui(UiEvent::GroupBenchmarkFailed);
                                format!(
                                    "{}：{}",
                                    language.text("Group test failed", "分组测速失败"),
                                    failure.as_deref().unwrap_or_else(|| language
                                        .text("unknown controller error", "未知控制器错误"))
                                )
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
        self.language()
            .text("Creating node group", "正在创建节点分组")
            .clone_into(&mut self.status);
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
        let language = self.language();
        self.status = format!(
            "{} “{}”",
            language.text("Editing group", "正在编辑分组"),
            group.name
        );
        cx.notify();
    }

    #[allow(clippy::too_many_lines)]
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
        let language = self.language();
        let Ok(mut group) = NodePolicyGroup::new(&id, &name) else {
            language
                .text(
                    "Group name cannot be empty or contain newlines/control characters",
                    "分组名称不能为空，也不能包含换行或控制字符",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        };
        if self
            .node_policy_groups
            .iter()
            .any(|existing| existing.id != id && existing.name == name)
        {
            language
                .text(
                    "A node group with this name already exists. Choose another name.",
                    "已有同名节点分组，请换一个名称",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        if matches!(name.as_str(), "Auto" | "Proxy") {
            language
                .text(
                    "\"Auto\" and \"Proxy\" are Manis-reserved policy group names",
                    "“Auto”和“Proxy”是 Manis 保留的策略组名称",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        group.icon = draft.icon;
        group.strategy = draft.strategy;
        if group
            .set_test_interval_secs(draft.test_interval_secs)
            .is_err()
        {
            language
                .text("Automatic check interval is invalid", "自动检查间隔无效")
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let matcher = match draft.matcher_kind {
            NodeGroupMatcherKind::All => NodeGroupMatcher::All,
            NodeGroupMatcherKind::NameContains => {
                let Ok(matcher) = NodeGroupMatcher::name_contains(&filter) else {
                    language
                        .text("Enter the node name to match", "请填写要匹配的节点名称")
                        .clone_into(&mut self.status);
                    cx.notify();
                    return;
                };
                matcher
            }
            NodeGroupMatcherKind::Explicit => {
                if draft.explicit_members.is_empty() {
                    language
                        .text("Select at least one node", "请至少选择一个节点")
                        .clone_into(&mut self.status);
                    cx.notify();
                    return;
                }
                NodeGroupMatcher::Explicit(draft.explicit_members)
            }
        };
        if group.set_matcher(matcher).is_err() || self.node_group_match_count(&group) == 0 {
            language
                .text(
                    "The current rule does not match any imported nodes",
                    "当前规则没有匹配到任何已导入节点",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            language
                .text(
                    "Could not determine where to save node groups",
                    "无法确定节点分组保存位置",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        };
        if let Err(error) = mihomo::save_node_policy_group_in(&store_dir, &group) {
            self.status = format!(
                "{}: {error}",
                language.text("Failed to save node group", "节点分组保存失败")
            );
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
        self.status = format!(
            "{} “{}”; {}",
            language.text("Group saved", "分组已保存"),
            group.name,
            language.text("applying managed config", "正在应用托管配置")
        );
        self.apply_node_policy_groups(
            store_dir,
            format!(
                "{} “{}”",
                language.text("Group saved", "分组已保存"),
                group.name
            ),
            cx,
        );
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
        let language = self.language();
        if let Err(error) = mihomo::remove_node_policy_group_in(&store_dir, id) {
            self.status = format!(
                "{}: {error}",
                language.text("Failed to delete node group", "节点分组删除失败")
            );
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
        self.status = format!(
            "{} “{}”; {}",
            language.text("Group deleted", "分组已删除"),
            group.name,
            language.text("applying managed config", "正在应用托管配置")
        );
        self.apply_node_policy_groups(
            store_dir,
            format!(
                "{} “{}”",
                language.text("Group deleted", "分组已删除"),
                group.name
            ),
            cx,
        );
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
                this.status = format!(
                    "{prefix}{}",
                    Self::source_runtime_apply_suffix(&apply, this.language())
                );
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

    fn node_configuration_link(
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id("nodes-open-configuration")
            .role(Role::Button)
            .aria_label(language.text("Manage subscription sources", "管理订阅来源"))
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
            .child(language.text("Manage Sources", "管理来源"))
            .on_click(cx.listener(|this, _, _, cx| {
                this.primary_workspace = PrimaryWorkspace::Configuration;
                this.language()
                    .text(
                        "Subscription source configuration opened",
                        "已打开订阅来源配置",
                    )
                    .clone_into(&mut this.status);
                cx.notify();
            }))
    }

    fn node_refresh_button(
        refreshing: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id("nodes-refresh")
            .role(Role::Button)
            .aria_label(language.text("Refresh node health", "刷新节点健康状态"))
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
                language.text("Loading...", "读取中…")
            } else {
                language.text("Refresh Nodes", "刷新节点")
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
                    this.language()
                        .text(
                            "Saved nodes do not need to be downloaded again",
                            "已保存节点不需要重新下载",
                        )
                        .clone_into(&mut this.status);
                    cx.notify();
                } else {
                    this.connect_mihomo(cx);
                }
            }))
    }

    fn node_health_summary(
        counts: NodeCounts,
        compact: bool,
        language: Language,
        theme: Theme,
    ) -> Div {
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
                language.text("Available", "可用"),
                counts.available,
                theme.status_success,
                theme,
            ))
            .child(Self::node_health_value(
                language.text("Unavailable", "不可用"),
                counts.unavailable,
                theme.text_secondary,
                theme,
            ))
            .child(Self::node_health_value(
                language.text("Untested", "未测速"),
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
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let filters = [
            NodeAvailabilityFilter::All,
            NodeAvailabilityFilter::Available,
            NodeAvailabilityFilter::Unavailable,
            NodeAvailabilityFilter::Untested,
        ];
        div()
            .id("node-filter-bar")
            .mt_3()
            .flex()
            .items_center()
            .gap_2()
            .when(compact, gpui::StatefulInteractiveElement::overflow_x_scroll)
            .children(filters.into_iter().map(|filter| {
                let label = Self::availability_filter_label(filter, language);
                let active = selected == filter;
                div()
                    .id(format!("node-filter-{label}"))
                    .role(Role::Button)
                    .aria_label(format!(
                        "{} {label}",
                        language.text("Filter nodes by", "筛选节点")
                    ))
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
                        let language = this.language();
                        this.status = format!(
                            "{}: {}",
                            language.text("Node filter", "节点筛选"),
                            Self::availability_filter_label(filter, language)
                        );
                        cx.notify();
                    }))
            }))
    }

    fn node_group_list(
        &self,
        groups: &[NodeSourceGroup<'_>],
        filter: NodeAvailabilityFilter,
        compact: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut list = div().flex().flex_col().gap_3();
        for group in groups {
            list = list.child(self.node_group(group, filter, compact, language, theme, cx));
        }
        list
    }

    #[allow(clippy::too_many_lines)]
    fn node_group(
        &self,
        group: &NodeSourceGroup<'_>,
        filter: NodeAvailabilityFilter,
        compact: bool,
        language: Language,
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
            GroupBenchmarkState::Running { .. } => format!(
                "{} · {}",
                group.detail,
                language.text("testing...", "正在测速…")
            ),
            GroupBenchmarkState::Complete { summary, .. } => format!(
                "{} · {} {}",
                group.detail,
                language.text("test", "测速"),
                Self::success_fraction_label(summary.succeeded, summary.total, language)
            ),
            GroupBenchmarkState::Failed { .. } => format!(
                "{} · {}",
                group.detail,
                language.text("test failed", "测速失败")
            ),
        };
        let action = if collapsed {
            language.text("Expand", "展开")
        } else {
            language.text("Collapse", "收起")
        };
        let header = div()
            .id(format!("node-group-header-{}", group.id))
            .role(Role::Button)
            .aria_label(format!(
                "{} {} {}",
                action,
                language.text("node group", "节点分组"),
                group.name
            ))
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
                            .child(Self::node_count_label(counts.total, language)),
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
                this.language()
                    .text(
                        "Node group expanded state updated",
                        "已更新节点分组展开状态",
                    )
                    .clone_into(&mut this.status);
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
                        .child(language.text(
                            "No nodes in this group match the current filter.",
                            "这个分组中没有符合当前筛选的节点。",
                        )),
                )
            })
            .when(!collapsed && visible_count > 0, |container| {
                container.child(
                    self.node_group_table(group, filter, &benchmark, compact, language, theme, cx),
                )
            })
    }

    #[allow(clippy::too_many_arguments)]
    fn node_group_table(
        &self,
        group: &NodeSourceGroup<'_>,
        filter: NodeAvailabilityFilter,
        benchmark: &GroupBenchmarkState,
        compact: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut table = div();
        if !compact {
            table = table.child(Self::node_table_header(language, theme));
        }

        for (provider_index, provider) in group.providers.iter().enumerate() {
            for (node_index, node) in provider.nodes.iter().enumerate() {
                if !filter.includes(node.alive) {
                    continue;
                }
                table = table.child(self.workspace_node_row(
                    format!("node-row-{}-{provider_index}-{node_index}", group.id),
                    node,
                    &group.id,
                    &group.name,
                    benchmark,
                    compact,
                    language,
                    theme,
                    cx,
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
            table = table.child(self.workspace_node_row(
                format!(
                    "node-row-{}-{}-{node_index}",
                    group.id,
                    group.providers.len()
                ),
                &loaded,
                &group.id,
                &group.name,
                benchmark,
                compact,
                language,
                theme,
                cx,
            ));
        }
        table
    }

    fn node_table_header(language: Language, theme: Theme) -> Div {
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
            .child(div().flex_1().child(language.text("Node", "节点")))
            .child(div().w(px(180.0)).child(language.text("Source", "来源")))
            .child(div().w(px(100.0)).child(language.text("Protocol", "协议")))
            .child(
                div()
                    .w(px(72.0))
                    .text_align(gpui::TextAlign::Right)
                    .child(language.text("Latency", "延迟")),
            )
            .child(
                div()
                    .w(px(76.0))
                    .text_align(gpui::TextAlign::Right)
                    .child(language.text("Global", "全局出口")),
            )
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn workspace_node_row(
        &self,
        row_id: String,
        node: &LoadedProviderNode,
        source_id: &str,
        source_name: &str,
        benchmark: &GroupBenchmarkState,
        compact: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let latency = benchmark.node_state(&node.name);
        let idle_latency = node.latency_label.clone().unwrap_or_else(|| "—".to_owned());
        let spinner_id = format!("{row_id}-latency");
        let global_identity = NodeIdentity::new(source_id, &node.name).ok();
        let global_selectable = global_identity.is_some();
        let global_runtime_selected = self.runtime_global_target() == Some(node.name.as_str());
        let global_selected = global_identity.as_ref().is_some_and(|identity| {
            self.global_target_identity()
                .map_or(global_runtime_selected, |selected| selected == identity)
        });
        let global_busy = self.global_selection_busy.as_deref() == Some(node.name.as_str());
        let selection_locked = self.global_selection_busy.is_some();
        let selected_name = node.name.clone();
        let content = if compact {
            Self::compact_node_row_content(
                source_name,
                node,
                latency,
                idle_latency,
                &spinner_id,
                language,
                theme,
            )
        } else {
            Self::wide_node_row_content(
                source_name,
                node,
                latency,
                idle_latency,
                &spinner_id,
                language,
                theme,
            )
        };
        div()
            .id(row_id)
            .min_h(if compact { px(64.0) } else { px(52.0) })
            .px(if compact { px(12.0) } else { px(16.0) })
            .py_2()
            .flex()
            .items_center()
            .gap_3()
            .border_t_1()
            .border_color(theme.outline_subtle)
            .bg(if global_selected {
                theme.action_soft
            } else {
                theme.surface_high
            })
            .child(content)
            .when_some(global_identity, |row, selected_identity| {
                row.role(Role::RadioButton)
                    .aria_label(format!(
                        "{} {selected_name} {}",
                        language.text("Select", "选择"),
                        language.text("as global exit", "作为全局出口")
                    ))
                    .aria_toggled(if global_selected {
                        Toggled::True
                    } else {
                        Toggled::False
                    })
                    .tab_stop(!selection_locked)
                    .focusable()
                    .cursor_pointer()
                    .child(
                        div()
                            .w(if compact { px(66.0) } else { px(76.0) })
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .text_size(px(10.0))
                            .font_weight(if global_selected {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(if global_selected {
                                theme.action_primary
                            } else {
                                theme.text_tertiary
                            })
                            .child(
                                div()
                                    .size(px(14.0))
                                    .rounded_full()
                                    .border_2()
                                    .border_color(if global_selected {
                                        theme.action_primary
                                    } else {
                                        theme.outline_strong
                                    })
                                    .when(global_selected, |dot| dot.bg(theme.action_primary)),
                            )
                            .child(if global_busy {
                                language.text("Switching", "切换中")
                            } else if global_selected
                                && global_runtime_selected
                                && self.routing_mode == manis_core::RoutingMode::Global
                            {
                                language.text("Active", "使用中")
                            } else if global_selected {
                                language.text("Selected", "已选")
                            } else {
                                language.text("Select", "选择")
                            }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !selection_locked {
                            this.select_global_node(selected_identity.clone(), cx);
                        }
                    }))
            })
            .when(!compact && !global_selectable, |row| {
                row.child(
                    div()
                        .w(px(76.0))
                        .flex_shrink_0()
                        .text_align(gpui::TextAlign::Center)
                        .text_color(theme.text_tertiary)
                        .child("—"),
                )
            })
    }

    fn compact_node_row_content(
        source_name: &str,
        node: &LoadedProviderNode,
        latency: GroupBenchmarkNodeState,
        idle_latency: String,
        spinner_id: &str,
        _language: Language,
        theme: Theme,
    ) -> Div {
        div()
            .min_w(px(0.0))
            .flex_1()
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
                            .child(Self::benchmark_latency_content(
                                latency,
                                idle_latency,
                                spinner_id,
                                theme,
                            )),
                    ),
            )
    }

    fn wide_node_row_content(
        source_name: &str,
        node: &LoadedProviderNode,
        latency: GroupBenchmarkNodeState,
        idle_latency: String,
        spinner_id: &str,
        _language: Language,
        theme: Theme,
    ) -> Div {
        div()
            .min_w(px(0.0))
            .flex_1()
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
                    .child(Self::benchmark_latency_content(
                        latency,
                        idle_latency,
                        spinner_id,
                        theme,
                    )),
            )
    }

    fn node_empty_state(
        compact: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
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
                    .child(language.text("No manageable nodes yet", "还没有可管理的节点")),
            )
            .child(
                div()
                    .mt_2()
                    .max_w(px(420.0))
                    .text_color(theme.text_secondary)
                    .child(language.text(
                        "Import an HTTP/HTTPS subscription first, and nodes will appear here automatically.",
                        "先导入一个 HTTP/HTTPS 订阅，节点会自动出现在这个工作区。",
                    )),
            )
            .child(
                div()
                    .id("nodes-empty-import")
                    .role(Role::Button)
                    .aria_label(language.text(
                        "Go to Configuration to import a subscription",
                        "前往配置导入订阅",
                    ))
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
                    .child(language.text("Import in Configuration", "前往配置导入"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.primary_workspace = PrimaryWorkspace::Configuration;
                        this.language()
                            .text(
                                "Subscription source configuration opened",
                                "已打开订阅来源配置",
                            )
                            .clone_into(&mut this.status);
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

    use super::{ManisApp, NodeCounts};
    use crate::app::{
        GroupBenchmarkNodeState, GroupBenchmarkState, GroupBenchmarkSummary, NodeGroupRuntimeState,
    };
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
        assert_eq!(
            GroupBenchmarkState::running(2).node_state("Saved Edge"),
            GroupBenchmarkNodeState::Pending,
        );
        assert_eq!(
            GroupBenchmarkState::Complete {
                generation: 2,
                summary: GroupBenchmarkSummary::default(),
                delays: BTreeMap::new(),
            }
            .node_state("Saved Edge"),
            GroupBenchmarkNodeState::Failed,
        );
        assert_eq!(
            GroupBenchmarkState::Complete {
                generation: 2,
                summary: GroupBenchmarkSummary::default(),
                delays: BTreeMap::from([("Saved Edge".to_owned(), 47)]),
            }
            .node_state("Saved Edge"),
            GroupBenchmarkNodeState::Measured(47),
        );
    }

    #[test]
    fn imported_node_latency_spinner_advances_through_eight_frames() {
        assert_eq!(ManisApp::benchmark_latency_spinner_frame(0.0), 0);
        assert_eq!(ManisApp::benchmark_latency_spinner_frame(0.124), 0);
        assert_eq!(ManisApp::benchmark_latency_spinner_frame(0.125), 1);
        assert_eq!(ManisApp::benchmark_latency_spinner_frame(0.5), 4);
        assert_eq!(ManisApp::benchmark_latency_spinner_frame(0.875), 7);
        assert_eq!(ManisApp::benchmark_latency_spinner_frame(1.0), 7);
    }

    #[test]
    fn group_benchmark_state_ignores_a_stale_completion() {
        let mut state = GroupBenchmarkState::running(7);
        let outdated = BTreeMap::from([("Tokyo".to_owned(), 90)]);
        assert!(!state.complete(6, 2, outdated));
        assert_eq!(state, GroupBenchmarkState::running(7));

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
    fn group_benchmark_exposes_each_node_result_before_completion() {
        let mut state = GroupBenchmarkState::running(7);

        assert_eq!(
            state.node_state("Tokyo"),
            crate::app::GroupBenchmarkNodeState::Pending
        );
        assert!(state.record(7, "Tokyo", Some(42)));
        assert_eq!(
            state.node_state("Tokyo"),
            crate::app::GroupBenchmarkNodeState::Measured(42)
        );
        assert!(state.record(7, "Singapore", None));
        assert_eq!(
            state.node_state("Singapore"),
            crate::app::GroupBenchmarkNodeState::Failed
        );
        assert!(!state.record(6, "Stale", Some(99)));
        assert_eq!(
            state.node_state("Stale"),
            crate::app::GroupBenchmarkNodeState::Pending
        );
    }

    #[test]
    fn benchmark_state_reports_running_only_for_active_variant() {
        assert!(GroupBenchmarkState::running(1).is_running());
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
