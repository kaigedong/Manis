use std::collections::BTreeSet;

use gpui::{Context, Div, FontWeight, ParentElement, Role, Stateful, Styled, div, prelude::*, px};
use relay_core::{
    NodeAvailabilityFilter, NodeGroupIcon, NodeGroupMatcher, NodeGroupStrategy, NodeIdentity,
    NodePolicyGroup, PrimaryWorkspace, WindowSizeClass,
};

use super::{
    ImportedSubscriptionState, NodeGroupDraft, NodeGroupMatcherKind, RelayApp, SourceRuntimeApply,
};
use crate::{
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
        section
            .when(!self.node_policy_groups.is_empty(), |section| {
                section.child(cards)
            })
            .when_some(self.node_group_draft.as_ref(), |section, draft| {
                section.child(self.node_group_editor(draft, compact, theme, cx))
            })
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

    fn node_policy_group_card(
        &self,
        group: &NodePolicyGroup,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let matched = self.node_group_match_count(group);
        let group_id = group.id.clone();
        let remove_id = group.id.clone();
        let matcher_summary = match &group.matcher {
            NodeGroupMatcher::All => "全部节点".to_owned(),
            NodeGroupMatcher::NameContains(value) => format!("名称包含 “{value}”"),
            NodeGroupMatcher::Explicit(nodes) => format!("明确选择 {} 个节点", nodes.len()),
        };
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
                    .gap_3()
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
                                        "{} · {matcher_summary} · 匹配 {matched} 个",
                                        group.strategy.label()
                                    )),
                            ),
                    ),
            )
            .child(
                div()
                    .mt_3()
                    .flex()
                    .gap_2()
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

    fn node_group_match_count(&self, group: &NodePolicyGroup) -> usize {
        self.node_inventory()
            .iter()
            .filter(|node| group.matches(&node.source_id, &node.node_name))
            .count()
    }

    fn start_node_group_create(&mut self, cx: &mut Context<Self>) {
        self.node_group_draft = Some(NodeGroupDraft {
            editing_id: None,
            icon: NodeGroupIcon::Bolt,
            strategy: NodeGroupStrategy::Manual,
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
                cx.notify();
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
                            .child(group.detail.clone()),
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
                            .child(format!(
                                "{} 个节点 · {} 个可用",
                                counts.total, counts.available
                            )),
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
                container.child(Self::node_group_table(group, filter, compact, theme))
            })
    }

    fn node_group_table(
        group: &NodeSourceGroup<'_>,
        filter: NodeAvailabilityFilter,
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
                    &group.id,
                    provider_index,
                    node_index,
                    node,
                    &group.name,
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
                &group.id,
                group.providers.len(),
                node_index,
                &loaded,
                &group.name,
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
            .child(div().w(px(82.0)).child("状态"))
            .child(
                div()
                    .w(px(72.0))
                    .text_align(gpui::TextAlign::Right)
                    .child("延迟"),
            )
    }

    fn workspace_node_row(
        group_id: &str,
        provider_index: usize,
        node_index: usize,
        node: &LoadedProviderNode,
        source_name: &str,
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
            Self::compact_node_row_content(source_name, node, state, latency, color, theme)
        } else {
            Self::wide_node_row_content(source_name, node, state, latency, color, theme)
        };
        div()
            .id(format!("node-row-{group_id}-{provider_index}-{node_index}"))
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
                            .child(format!("{source_name} · {}", node.protocol)),
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
        source_name: &str,
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
