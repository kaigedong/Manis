use std::collections::{BTreeMap, BTreeSet};

use gpui::{
    AnyElement, Context, Div, FontWeight, ParentElement, Role, Stateful, Styled, Toggled, div,
    prelude::*, px,
};
use gpui_component::{
    Icon, IconName, Sizable,
    button::{Button, ButtonVariant, ButtonVariants},
    checkbox::Checkbox,
    collapsible::Collapsible,
    radio::Radio,
};
use manis_core::{
    ManagedPolicyGroup, ManagedPolicyIcon, ManagedPolicyStrategy, NodeAvailabilityFilter,
    NodeIdentity, PolicyCandidateMatcher, PrimaryWorkspace, WindowSizeClass,
};

use super::{
    GroupBenchmarkNodeState, GroupBenchmarkState, ImportedSubscriptionState, ManagedPolicyDraft,
    ManisApp, PolicyCandidateMatcherKind, PolicyEditorPopover,
};
use crate::{
    components::{ActionRole, action_button, empty_state, section_heading},
    diagnostics::{UiEvent, trace_ui},
    localization::{CountNoun, Language, Message},
    mihomo::{self, LoadedProvider, LoadedProviderNode, ProxyDelayTarget, SubscriptionStoreError},
    subscription::SourceNodePreview,
    theme::{ControlSize, Radius, Space, TextRole, Theme},
};

struct NodeSourceGroup<'a> {
    id: String,
    name: String,
    detail: String,
    providers: Vec<&'a LoadedProvider>,
    runtime_provider_names: Vec<String>,
    saved_nodes: Vec<&'a SourceNodePreview>,
}

#[derive(Clone, Copy)]
struct NodeWorkspaceView {
    filter: NodeAvailabilityFilter,
    compact: bool,
    language: Language,
    theme: Theme,
}

struct WorkspaceNodeRowContext {
    row_id: String,
    source_id: String,
    compact: bool,
    language: Language,
    theme: Theme,
}

enum ManagedPolicyDraftError {
    InvalidName,
    DuplicateName,
    ReservedName,
    InvalidInterval,
    MissingFilter,
    MissingExplicitMember,
    NoCandidates,
    InvalidReferences(String),
}

impl ManagedPolicyDraftError {
    fn message(self, language: Language) -> String {
        match self {
            Self::InvalidName => language
                .text(
                    "Group name cannot be empty or contain newlines/control characters",
                    "策略组名称不能为空，也不能包含换行或控制字符",
                )
                .to_owned(),
            Self::DuplicateName => language
                .text(
                    "A policy group with this name already exists. Choose another name.",
                    "已有同名策略组，请换一个名称",
                )
                .to_owned(),
            Self::ReservedName => language
                .text(
                    "This name is reserved by the proxy kernel",
                    "该名称由代理内核保留",
                )
                .to_owned(),
            Self::InvalidInterval => language
                .text("Automatic check interval is invalid", "自动检查间隔无效")
                .to_owned(),
            Self::MissingFilter => language
                .text("Enter the node name to match", "请填写要匹配的节点名称")
                .to_owned(),
            Self::MissingExplicitMember => language
                .text(
                    "Select at least one node or policy group",
                    "请至少选择一个节点或策略组",
                )
                .to_owned(),
            Self::NoCandidates => language
                .text(
                    "The current rule does not match any imported nodes",
                    "当前规则没有匹配到任何已导入节点",
                )
                .to_owned(),
            Self::InvalidReferences(message) => message,
        }
    }
}

struct PolicyEditorPopup {
    kind: PolicyEditorPopover,
    open: bool,
    content: AnyElement,
    width: f32,
    max_height: f32,
    show_divider: bool,
}

impl PolicyEditorPopup {
    fn new(
        kind: PolicyEditorPopover,
        open: bool,
        content: impl gpui::IntoElement,
        width: f32,
        max_height: f32,
    ) -> Self {
        Self {
            kind,
            open,
            content: content.into_any_element(),
            width,
            max_height,
            show_divider: true,
        }
    }

    fn with_divider(mut self, show_divider: bool) -> Self {
        self.show_divider = show_divider;
        self
    }
}

fn subscription_provider_refs<'a>(
    preview_providers: &'a [LoadedProvider],
    runtime_providers: &'a [LoadedProvider],
    runtime_provider_name: &str,
) -> Vec<&'a LoadedProvider> {
    if preview_providers.is_empty() {
        runtime_providers
            .iter()
            .filter(|provider| provider.name == runtime_provider_name)
            .collect()
    } else {
        preview_providers.iter().collect()
    }
}

impl NodeSourceGroup<'_> {
    fn delay_targets(&self) -> Vec<ProxyDelayTarget> {
        self.providers
            .iter()
            .enumerate()
            .flat_map(|(index, provider)| {
                let runtime_name = self
                    .runtime_provider_names
                    .get(index)
                    .unwrap_or(&provider.name);
                provider.nodes.iter().map(move |node| {
                    ProxyDelayTarget::provider(runtime_name.clone(), node.name.clone())
                })
            })
            .chain(
                self.saved_nodes
                    .iter()
                    .map(|node| ProxyDelayTarget::direct(node.name.clone())),
            )
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
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

impl ManisApp {
    fn managed_policy_icon_label(icon: ManagedPolicyIcon, language: Language) -> &'static str {
        match icon {
            ManagedPolicyIcon::None => language.text("First letter", "首字圆标"),
            ManagedPolicyIcon::Bolt => language.text("Bolt", "闪电"),
            ManagedPolicyIcon::Globe => language.text("Globe", "地球"),
            ManagedPolicyIcon::Shield => language.text("Shield", "盾牌"),
            ManagedPolicyIcon::Compass => language.text("Compass", "罗盘"),
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
        language.count(CountNoun::Source, count)
    }

    fn node_count_label(count: usize, language: Language) -> String {
        language.count(CountNoun::Node, count)
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

    pub(super) fn node_workspace(
        &self,
        theme: Theme,
        size_class: WindowSizeClass,
        cx: &mut Context<Self>,
    ) -> Div {
        let compact = size_class == WindowSizeClass::Compact;
        let has_local_sources = self
            .imported_subscriptions
            .iter()
            .any(|subscription| subscription.enabled)
            || !self.saved_single_nodes.is_empty();
        let language = self.language();
        let groups = self.node_source_groups(has_local_sources, language);
        let counts = NodeCounts::from_groups(&groups);
        let filter = self.node_workspace.filter;
        let loading = self.imported_subscriptions.iter().any(|subscription| {
            subscription.enabled
                && matches!(
                    subscription.state,
                    ImportedSubscriptionState::Pending(_)
                        | ImportedSubscriptionState::Refreshing(_)
                )
        });
        let refreshing = loading
            || (!has_local_sources
                && matches!(
                    self.controller,
                    crate::mihomo::ControllerState::Connecting { .. }
                ));
        let enabled_subscriptions = self
            .imported_subscriptions
            .iter()
            .filter(|subscription| subscription.enabled)
            .collect::<Vec<_>>();
        let unavailable = !enabled_subscriptions.is_empty()
            && enabled_subscriptions.iter().all(|subscription| {
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

        let view = NodeWorkspaceView {
            filter,
            compact,
            language,
            theme,
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
                origin,
                refreshing,
                view,
                cx,
            ))
            .child(self.node_workspace_body(&groups, loading, unavailable, view, cx))
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
                .filter(|subscription| subscription.enabled)
                .enumerate()
                .map(|(index, subscription)| {
                    let name = subscription.name.clone();
                    let runtime_provider_name = format!("Subscription {}", index + 1);
                    let providers = subscription_provider_refs(
                        &subscription.providers,
                        &self.source_providers,
                        &runtime_provider_name,
                    );
                    let using_runtime_cache =
                        subscription.providers.is_empty() && !providers.is_empty();
                    let provider_count = providers.len();
                    let transport = if subscription.source.is_https() {
                        language.text("HTTPS subscription", "HTTPS 订阅")
                    } else {
                        language.text("HTTP subscription", "HTTP 订阅")
                    };
                    let state = if using_runtime_cache {
                        language.text("Using Mihomo cache", "使用 Mihomo 缓存")
                    } else {
                        match subscription.state {
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
                            ImportedSubscriptionState::None => {
                                language.text("Not loaded", "尚未读取")
                            }
                        }
                    };
                    NodeSourceGroup {
                        id: format!("subscription:{}", subscription.id),
                        name,
                        detail: format!("{transport} · {state}"),
                        providers,
                        runtime_provider_names: vec![runtime_provider_name; provider_count],
                        saved_nodes: Vec::new(),
                    }
                })
                .collect();
            if self.saved_single_nodes.iter().any(|saved| saved.enabled) {
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
                    runtime_provider_names: Vec::new(),
                    saved_nodes: self
                        .saved_single_nodes
                        .iter()
                        .filter(|saved| saved.enabled)
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
                runtime_provider_names: vec![provider.name.clone()],
                saved_nodes: Vec::new(),
            })
            .collect()
    }

    fn node_workspace_header(
        source_count: usize,
        counts: NodeCounts,
        origin: &'static str,
        refreshing: bool,
        view: NodeWorkspaceView,
        cx: &mut Context<Self>,
    ) -> Div {
        let NodeWorkspaceView {
            filter,
            compact,
            language,
            theme,
        } = view;
        let actions = div()
            .flex()
            .items_center()
            .gap(Space::Sm.px())
            .child(Self::node_refresh_button(refreshing, language, theme, cx))
            .child(Self::node_configuration_link(language, cx))
            .into_any_element();
        let detail = format!(
            "{origin} · {} · {}",
            Self::source_count_label(source_count, language),
            language.text(
                "Review exit health and global selections here",
                "在这里查看出口健康状态",
            )
        );

        div()
            .px(if compact {
                Space::Lg.px()
            } else {
                Space::Xl.px()
            })
            .pt(if compact {
                Space::Lg.px()
            } else {
                Space::Xl.px()
            })
            .pb(Space::Lg.px())
            .border_b_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .child(crate::components::page_heading(
                format!("{} · {}", language.message(Message::Nodes), counts.total),
                detail,
                Some(actions),
                theme,
            ))
            .child(Self::node_health_summary(counts, compact, language, theme))
            .child(Self::node_filter_bar(
                counts, filter, compact, language, theme, cx,
            ))
    }

    fn node_workspace_body(
        &self,
        groups: &[NodeSourceGroup<'_>],
        loading: bool,
        unavailable: bool,
        view: NodeWorkspaceView,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let NodeWorkspaceView {
            filter,
            compact,
            language,
            theme,
        } = view;
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
                        "Manis is loading nodes from your saved subscriptions.",
                        "正在从已保存的订阅中载入节点。",
                    ),
                    theme,
                ))
            })
            .when(unavailable && groups.is_empty(), |body| {
                body.child(Self::node_message_panel(
                    language.text("Nodes are temporarily unavailable", "暂时无法读取节点"),
                    language.text(
                        "Subscriptions remain stored locally. Check source details in Configuration.",
                        "订阅仍保存在本机。请前往配置页检查来源详情。",
                    ),
                    theme,
                ))
            })
            .when(!loading && !unavailable && groups.is_empty(), |body| {
                body.child(Self::node_empty_state(compact, language, theme, cx))
            })
            .when(!groups.is_empty(), |body| {
                body.child(self.source_group_list(groups, filter, compact, language, theme, cx))
            })
    }

    fn node_section_heading(title: &'static str, detail: &'static str, theme: Theme) -> Div {
        section_heading(title, detail, None, theme).mb(Space::Md.px())
    }

    pub(super) fn managed_policy_editor_workspace(
        &self,
        draft: &ManagedPolicyDraft,
        compact: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .h_full()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .bg(theme.surface_low)
            .child(Self::policy_editor_header(draft, language, theme, cx))
            .child(self.policy_editor_form(draft, compact, false, language, theme, cx))
    }

    fn policy_editor_header(
        draft: &ManagedPolicyDraft,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let title = if draft.editing_id.is_some() {
            language.text("Edit policy group", "编辑策略组")
        } else {
            language.text("New policy group", "新建策略组")
        };
        let left = Self::policy_editor_header_button(
            "policy-editor-back",
            language.message(Message::Cancel),
            false,
            cx.listener(|this, _, _, cx| {
                this.managed_policy_draft = None;
                this.managed_policy_editor_popover = None;
                this.language()
                    .text("Policy editing cancelled", "已取消编辑策略")
                    .clone_into(&mut this.status);
                cx.notify();
            }),
        );
        let right = Self::policy_editor_header_button(
            "policy-group-save",
            language.message(Message::SaveChanges),
            true,
            cx.listener(|this, _, _, cx| this.save_managed_policy(cx)),
        );

        div()
            .h(px(64.0))
            .px_4()
            .flex_shrink_0()
            .border_b_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .flex()
            .items_center()
            .child(div().w(px(112.0)).child(left))
            .child(
                div()
                    .flex_1()
                    .text_size(px(16.0))
                    .font_weight(FontWeight::BOLD)
                    .text_center()
                    .child(title),
            )
            .child(div().w(px(112.0)).flex().justify_end().child(right))
    }

    fn policy_editor_header_button(
        id: &'static str,
        label: &'static str,
        primary: bool,
        listener: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
    ) -> Button {
        action_button(
            id,
            label,
            if primary {
                ActionRole::Primary
            } else {
                ActionRole::Quiet
            },
            ControlSize::Compact,
        )
        .accessibility_label(label)
        .px_3()
        .cursor_pointer()
        .font_weight(FontWeight::SEMIBOLD)
        .on_click(listener)
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn policy_editor_form(
        &self,
        draft: &ManagedPolicyDraft,
        compact: bool,
        embedded: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let strategy = match draft.strategy {
            ManagedPolicyStrategy::Manual => "static".to_owned(),
            ManagedPolicyStrategy::LowestLatency => "url-latency-benchmark".to_owned(),
        };
        let matcher = match draft.matcher_kind {
            PolicyCandidateMatcherKind::All => language.text("All nodes", "全部节点").to_owned(),
            PolicyCandidateMatcherKind::NameContains => {
                language.text("Name contains", "名称包含").to_owned()
            }
            PolicyCandidateMatcherKind::Explicit => language
                .text("Select nodes or groups", "选择节点或策略组")
                .to_owned(),
        };
        let interval = match (draft.test_interval_secs, language) {
            (60, Language::English) => "1 min".to_owned(),
            (60, Language::SimplifiedChinese) => "1 分钟".to_owned(),
            (300, Language::English) => "5 min".to_owned(),
            (300, Language::SimplifiedChinese) => "5 分钟".to_owned(),
            (600, Language::English) => "10 min".to_owned(),
            (600, Language::SimplifiedChinese) => "10 分钟".to_owned(),
            (1_800, Language::English) => "30 min".to_owned(),
            (1_800, Language::SimplifiedChinese) => "30 分钟".to_owned(),
            (seconds, Language::English) => format!("{seconds} sec"),
            (seconds, Language::SimplifiedChinese) => format!("{seconds} 秒"),
        };
        let name_input = self.policy_group_name_input.clone();
        let filter_input = self.policy_group_filter_input.clone();
        let policy_name = self
            .policy_group_name_input
            .as_ref()
            .map_or_else(String::new, |input| input.read(cx).value().to_owned());
        let popover_width = if compact { 280.0 } else { 300.0 };
        let strategy_menu = Self::policy_strategy_menu(draft, language, theme, cx);
        let icon_menu = Self::policy_icon_menu(draft, language, theme, cx);
        let basics = div()
            .rounded(Radius::Pane.px())
            .overflow_hidden()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .child(Self::policy_editor_popup_row(
                "policy-editor-type",
                language.text("Type", "类型"),
                strategy,
                None,
                theme,
                PolicyEditorPopup::new(
                    PolicyEditorPopover::Strategy,
                    self.managed_policy_editor_popover == Some(PolicyEditorPopover::Strategy),
                    strategy_menu,
                    popover_width,
                    220.0,
                ),
                cx,
            ))
            .child(Self::policy_editor_input_row(
                language.text("Policy group name", "策略组名称"),
                true,
                name_input,
                true,
                theme,
            ))
            .child(Self::policy_editor_popup_row(
                "policy-editor-icon",
                language.text("Icon", "图标"),
                Self::managed_policy_icon_label(draft.icon, language).to_owned(),
                Some(Self::policy_icon_visual(
                    draft.icon,
                    &policy_name,
                    28.0,
                    theme,
                )),
                theme,
                PolicyEditorPopup::new(
                    PolicyEditorPopover::Icon,
                    self.managed_policy_editor_popover == Some(PolicyEditorPopover::Icon),
                    icon_menu,
                    popover_width,
                    320.0,
                )
                .with_divider(false),
                cx,
            ));

        let candidate_mode_menu = Self::policy_candidate_mode_menu(draft, language, theme, cx);
        let has_candidate_details = draft.matcher_kind != PolicyCandidateMatcherKind::All
            || draft.strategy == ManagedPolicyStrategy::LowestLatency;
        let mut nodes = div()
            .rounded(Radius::Pane.px())
            .overflow_hidden()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .child(Self::policy_editor_popup_row(
                "policy-editor-candidate-mode",
                language.text("Node scope", "节点范围"),
                matcher,
                None,
                theme,
                PolicyEditorPopup::new(
                    PolicyEditorPopover::CandidateMode,
                    self.managed_policy_editor_popover == Some(PolicyEditorPopover::CandidateMode),
                    candidate_mode_menu,
                    popover_width,
                    280.0,
                )
                .with_divider(has_candidate_details),
                cx,
            ));
        if draft.matcher_kind == PolicyCandidateMatcherKind::NameContains {
            nodes = nodes.child(Self::policy_editor_input_row(
                language.text("Node name contains", "节点名称包含"),
                false,
                filter_input,
                draft.strategy == ManagedPolicyStrategy::LowestLatency,
                theme,
            ));
        }
        if draft.matcher_kind == PolicyCandidateMatcherKind::Explicit {
            let candidate_menu = self.policy_candidate_menu(draft, language, theme, cx);
            nodes = nodes.child(Self::policy_editor_popup_row(
                "policy-editor-selected-nodes",
                language.text("Selected candidates", "已选候选项"),
                match language {
                    Language::English => format!("{} selected", draft.explicit_members.len()),
                    Language::SimplifiedChinese => {
                        format!("已选 {} 项", draft.explicit_members.len())
                    }
                },
                None,
                theme,
                PolicyEditorPopup::new(
                    PolicyEditorPopover::CandidateNodes,
                    self.managed_policy_editor_popover == Some(PolicyEditorPopover::CandidateNodes),
                    candidate_menu,
                    popover_width.max(480.0),
                    420.0,
                )
                .with_divider(draft.strategy == ManagedPolicyStrategy::LowestLatency),
                cx,
            ));
        }
        if draft.strategy == ManagedPolicyStrategy::LowestLatency {
            let interval_menu = Self::policy_interval_menu(draft, language, theme, cx);
            nodes = nodes.child(Self::policy_editor_popup_row(
                "policy-editor-interval",
                language.text("Retest interval", "重测间隔"),
                interval,
                None,
                theme,
                PolicyEditorPopup::new(
                    PolicyEditorPopover::Interval,
                    self.managed_policy_editor_popover == Some(PolicyEditorPopover::Interval),
                    interval_menu,
                    popover_width,
                    320.0,
                )
                .with_divider(false),
                cx,
            ));
        }

        div()
            .id("policy-editor-scroll")
            .when(!embedded, |form| form.flex_1().overflow_y_scroll())
            .px(if embedded {
                px(0.0)
            } else if compact {
                px(16.0)
            } else {
                px(28.0)
            })
            .py(if embedded { px(0.0) } else { px(24.0) })
            .child(
                div()
                    .w_full()
                    .max_w(px(760.0))
                    .mx_auto()
                    .child(Self::policy_editor_section_label(
                        language.text("Basic information", "基本信息"),
                        theme,
                    ))
                    .child(basics)
                    .child(
                        Self::policy_editor_section_label(
                            language.text("Candidates", "候选节点"),
                            theme,
                        )
                        .mt_6(),
                    )
                    .child(nodes)
                    .child(
                        div()
                            .mt_3()
                            .px_2()
                            .text_size(TextRole::Body.size())
                            .line_height(TextRole::Body.line_height())
                            .text_color(theme.text_tertiary)
                            .child(language.text(
                                "Routing rules point to this policy; the policy chooses one exit from this node scope.",
                                "分流规则会指向这个策略组；策略组再从这里配置的节点范围中选择出口。",
                            )),
                    ),
            )
    }

    fn policy_editor_section_label(label: &'static str, theme: Theme) -> Div {
        div()
            .mb_2()
            .px_2()
            .text_size(TextRole::Label.size())
            .line_height(TextRole::Label.line_height())
            .font_weight(TextRole::Label.weight())
            .text_color(theme.text_secondary)
            .child(label)
    }

    fn policy_editor_popup_row(
        id: &'static str,
        label: &'static str,
        value: String,
        value_icon: Option<Div>,
        theme: Theme,
        popup: PolicyEditorPopup,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let PolicyEditorPopup {
            kind,
            open,
            content,
            width,
            max_height,
            show_divider,
        } = popup;
        let app = cx.entity();
        let trigger = Button::new(id)
            .accessibility_label(format!("{label}: {value}"))
            .dropdown_caret(true)
            .with_variant(ButtonVariant::Default)
            .with_size(ControlSize::Standard.component_size())
            .h(ControlSize::Standard.height())
            .w_full()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_low)
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .gap_2()
                    .when_some(value_icon, ParentElement::child)
                    .child(
                        div()
                            .flex_1()
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(TextRole::Label.size())
                            .line_height(TextRole::Label.line_height())
                            .font_weight(TextRole::Label.weight())
                            .text_color(theme.text_secondary)
                            .child(value),
                    ),
            );
        let popover = crate::components::anchored_popover(
            format!("{id}-popover"),
            trigger,
            content,
            width,
            max_height,
        )
        .open(open)
        .on_open_change(move |open, _, cx| {
            app.update(cx, |this, cx| {
                this.managed_policy_editor_popover = open.then_some(kind);
                cx.notify();
            });
        });

        div()
            .min_h(px(64.0))
            .px_4()
            .border_color(theme.outline_subtle)
            .when(show_divider, gpui::Styled::border_b_1)
            .flex()
            .items_center()
            .gap_4()
            .child(
                div()
                    .flex_1()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(label),
            )
            .child(div().w(px(300.0)).max_w(px(300.0)).child(popover))
            .into_any_element()
    }

    fn policy_editor_input_row(
        label: &'static str,
        required: bool,
        input: Option<gpui::Entity<crate::subscription_input::SubscriptionTextInput>>,
        show_divider: bool,
        theme: Theme,
    ) -> Div {
        div()
            .min_h(px(82.0))
            .px_4()
            .py_3()
            .border_color(theme.outline_subtle)
            .when(show_divider, gpui::Styled::border_b_1)
            .child(
                div()
                    .mb_2()
                    .flex()
                    .gap_1()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(label)
                    .when(required, |label| {
                        label.child(div().text_color(theme.status_error).child("*"))
                    }),
            )
            .when_some(input, ParentElement::child)
    }

    fn policy_choice_row(
        id: String,
        title: impl Into<gpui::SharedString>,
        selected: bool,
        theme: Theme,
        listener: impl Fn(&bool, &mut gpui::Window, &mut gpui::App) + 'static,
    ) -> Radio {
        let title = title.into();
        Radio::new(id)
            .label(title)
            .checked(selected)
            .tab_stop(true)
            .cursor_pointer()
            .min_h(px(44.0))
            .px_3()
            .flex()
            .items_center()
            .border_b_1()
            .border_color(theme.outline_subtle)
            .on_click(listener)
    }

    fn policy_icon_choice_row(
        id: String,
        icon: ManagedPolicyIcon,
        title: impl Into<gpui::SharedString>,
        selected: bool,
        theme: Theme,
        listener: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
    ) -> Stateful<Div> {
        let title = title.into();
        div()
            .id(id)
            .role(Role::RadioButton)
            .aria_toggled(if selected {
                Toggled::True
            } else {
                Toggled::False
            })
            .tab_stop(true)
            .focusable()
            .cursor_pointer()
            .min_h(px(50.0))
            .px_3()
            .border_b_1()
            .border_color(theme.outline_subtle)
            .flex()
            .items_center()
            .gap_3()
            .child(Self::policy_icon_visual(icon, "A", 30.0, theme))
            .child(div().flex_1().font_weight(FontWeight::MEDIUM).child(title))
            .child(
                div()
                    .size(px(16.0))
                    .rounded_full()
                    .border_2()
                    .border_color(if selected {
                        theme.action_primary
                    } else {
                        theme.outline_strong
                    })
                    .when(selected, |radio| radio.bg(theme.action_primary)),
            )
            .on_click(listener)
    }

    fn policy_strategy_menu(
        draft: &ManagedPolicyDraft,
        _language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let mut choices = div().id("policy-strategy-choices");
        for (strategy, technical) in [
            (ManagedPolicyStrategy::Manual, "static"),
            (
                ManagedPolicyStrategy::LowestLatency,
                "url-latency-benchmark",
            ),
        ] {
            let selected = draft.strategy == strategy;
            choices = choices.child(Self::policy_choice_row(
                format!("policy-group-strategy-{}", strategy.key()),
                technical,
                selected,
                theme,
                cx.listener(move |this, _, _, cx| {
                    if let Some(draft) = this.managed_policy_draft.as_mut() {
                        draft.strategy = strategy;
                    }
                    this.managed_policy_editor_popover = None;
                    cx.notify();
                }),
            ));
        }
        choices
    }

    fn policy_icon_menu(
        draft: &ManagedPolicyDraft,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let mut choices = div().id("policy-icon-choices");
        for icon in [
            ManagedPolicyIcon::None,
            ManagedPolicyIcon::Bolt,
            ManagedPolicyIcon::Globe,
            ManagedPolicyIcon::Shield,
            ManagedPolicyIcon::Compass,
        ] {
            let selected = draft.icon == icon;
            choices = choices.child(Self::policy_icon_choice_row(
                format!("policy-group-icon-{}", icon.key()),
                icon,
                Self::managed_policy_icon_label(icon, language),
                selected,
                theme,
                cx.listener(move |this, _, _, cx| {
                    if let Some(draft) = this.managed_policy_draft.as_mut() {
                        draft.icon = icon;
                    }
                    this.managed_policy_editor_popover = None;
                    cx.notify();
                }),
            ));
        }
        choices
    }

    fn policy_candidate_mode_menu(
        draft: &ManagedPolicyDraft,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let mut choices = div().id("policy-candidate-mode-choices");
        for (matcher, title) in [
            (
                PolicyCandidateMatcherKind::All,
                language.text("All nodes", "全部节点"),
            ),
            (
                PolicyCandidateMatcherKind::NameContains,
                language.text("Name contains", "名称包含"),
            ),
            (
                PolicyCandidateMatcherKind::Explicit,
                language.text("Select nodes or groups", "选择节点或策略组"),
            ),
        ] {
            let selected = draft.matcher_kind == matcher;
            choices = choices.child(Self::policy_choice_row(
                format!("policy-group-matcher-{matcher:?}"),
                title,
                selected,
                theme,
                cx.listener(move |this, _, _, cx| {
                    if let Some(draft) = this.managed_policy_draft.as_mut() {
                        draft.matcher_kind = matcher;
                    }
                    this.managed_policy_editor_popover = (matcher
                        == PolicyCandidateMatcherKind::Explicit)
                        .then_some(PolicyEditorPopover::CandidateNodes);
                    cx.notify();
                }),
            ));
        }
        choices
    }

    fn policy_candidate_menu(
        &self,
        draft: &ManagedPolicyDraft,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let inventory = self.policy_candidate_inventory();
        let has_local_sources =
            !self.imported_subscriptions.is_empty() || !self.saved_single_nodes.is_empty();
        let source_labels = self
            .node_source_groups(has_local_sources, language)
            .into_iter()
            .map(|group| (group.id, group.name))
            .collect::<BTreeMap<_, _>>();
        let selected_count = draft.explicit_members.len();
        let mut list =
            div()
                .id("policy-group-member-picker")
                .child(Self::policy_candidate_menu_header(
                    selected_count,
                    language,
                    theme,
                    cx,
                ));
        if inventory.is_empty() {
            list = list.child(
                div()
                    .p_5()
                    .text_color(theme.text_secondary)
                    .child(language.text(
                        "Import nodes or create another policy group before making a selection.",
                        "请先导入节点或创建其他策略组，再进行选择。",
                    )),
            );
        }
        for member in inventory {
            let selected = draft.explicit_members.contains(&member);
            let member_for_click = member.clone();
            list = list.child(
                Checkbox::new(format!(
                    "policy-group-member-{}-{}",
                    member.source_id, member.node_name
                ))
                .label(member.node_name.clone())
                .checked(selected)
                .tab_stop(true)
                .cursor_pointer()
                .min_h(px(58.0))
                .px_4()
                .flex()
                .items_center()
                .border_b_1()
                .border_color(theme.outline_subtle)
                .child(
                    div().flex().items_center().gap_3().child(
                        div()
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .text_color(theme.text_tertiary)
                            .child(match member.source_id.as_str() {
                                "builtin" => language.text("Built-in", "内置").to_owned(),
                                source if source.starts_with("policy:") => {
                                    language.text("Policy group", "策略组").to_owned()
                                }
                                source => source_labels
                                    .get(source)
                                    .cloned()
                                    .unwrap_or_else(|| source.to_owned()),
                            }),
                    ),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(draft) = this.managed_policy_draft.as_mut()
                        && !draft.explicit_members.remove(&member_for_click)
                    {
                        draft.explicit_members.insert(member_for_click.clone());
                    }
                    cx.notify();
                })),
            );
        }
        list
    }

    fn policy_candidate_menu_header(
        selected_count: usize,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .h(px(48.0))
            .px_4()
            .border_b_1()
            .border_color(theme.outline_subtle)
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(match language {
                        Language::English => {
                            format!("Select candidates · {selected_count} selected")
                        }
                        Language::SimplifiedChinese => {
                            format!("选择候选项 · 已选 {selected_count} 项")
                        }
                    }),
            )
            .child(
                action_button(
                    "policy-editor-node-menu-done",
                    language.text("Done", "完成"),
                    ActionRole::Primary,
                    ControlSize::Icon,
                )
                .accessibility_label(language.text("Finish selecting candidates", "完成选择候选项"))
                .px_3()
                .cursor_pointer()
                .font_weight(FontWeight::SEMIBOLD)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.managed_policy_editor_popover = None;
                    cx.notify();
                })),
            )
    }

    fn policy_interval_menu(
        draft: &ManagedPolicyDraft,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let mut choices = div().id("policy-interval-choices");
        for (seconds, english, chinese) in [
            (60, "1 min", "1 分钟"),
            (300, "5 min", "5 分钟"),
            (600, "10 min", "10 分钟"),
            (1_800, "30 min", "30 分钟"),
        ] {
            choices = choices.child(Self::policy_choice_row(
                format!("policy-group-interval-{seconds}"),
                language.text(english, chinese),
                draft.test_interval_secs == seconds,
                theme,
                cx.listener(move |this, _, _, cx| {
                    if let Some(draft) = this.managed_policy_draft.as_mut() {
                        draft.test_interval_secs = seconds;
                    }
                    this.managed_policy_editor_popover = None;
                    cx.notify();
                }),
            ));
        }
        choices
    }

    fn node_inventory(&self) -> Vec<NodeIdentity> {
        let has_local_sources =
            !self.imported_subscriptions.is_empty() || !self.saved_single_nodes.is_empty();
        let mut inventory = Vec::new();
        let mut seen = BTreeSet::new();
        for group in self.node_source_groups(has_local_sources, self.language()) {
            for provider in group.providers {
                for node in &provider.nodes {
                    if let Ok(identity) = NodeIdentity::new(&group.id, &node.name)
                        && seen.insert(identity.clone())
                    {
                        inventory.push(identity);
                    }
                }
            }
            for node in group.saved_nodes {
                if let Ok(identity) = NodeIdentity::new(&group.id, &node.name)
                    && seen.insert(identity.clone())
                {
                    inventory.push(identity);
                }
            }
        }
        inventory
    }

    fn policy_candidate_inventory(&self) -> Vec<NodeIdentity> {
        let mut inventory = ["DIRECT", "REJECT"]
            .into_iter()
            .filter_map(|name| NodeIdentity::new("builtin", name).ok())
            .collect::<Vec<_>>();
        inventory.extend(self.node_inventory());
        let editing_id = self
            .managed_policy_draft
            .as_ref()
            .and_then(|draft| draft.editing_id.as_deref());
        for group in &self.managed_policy_groups {
            if editing_id != Some(group.id.as_str())
                && let Ok(identity) =
                    NodeIdentity::new(&format!("policy:{}", group.id), &group.name)
            {
                inventory.push(identity);
            }
        }
        inventory
    }

    fn managed_policy_candidate_count(&self, group: &ManagedPolicyGroup) -> usize {
        self.node_inventory()
            .iter()
            .filter(|node| group.matches(&node.source_id, &node.node_name))
            .count()
    }

    pub(super) fn managed_policy_candidate_names(&self, group: &ManagedPolicyGroup) -> Vec<String> {
        let mut seen = BTreeSet::new();
        self.node_inventory()
            .into_iter()
            .filter(|node| group.matches(&node.source_id, &node.node_name))
            .map(|node| node.node_name)
            .filter(|name| seen.insert(name.clone()))
            .collect()
    }

    fn start_source_group_benchmark(
        &mut self,
        id: &str,
        name: &str,
        targets: Vec<ProxyDelayTarget>,
        cx: &mut Context<Self>,
    ) {
        let key = Self::source_group_benchmark_key(id);
        if matches!(
            self.group_benchmarks.get(&key),
            Some(GroupBenchmarkState::Running { .. })
        ) {
            return;
        }
        if targets.is_empty() {
            self.language()
                .text("This source has no nodes to test", "当前来源没有可测速节点")
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        if targets.len() > MAX_GROUP_BENCHMARK_NODES {
            let language = self.language();
            format!(
                "{}; {}",
                Self::group_limit_label(targets.len(), language),
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
            language.text("Testing source", "正在测试来源"),
            Self::node_count_label(targets.len(), language)
        );
        trace_ui(UiEvent::GroupBenchmarkStarted);

        let runtime = self.runtime.clone();
        let progress =
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new()));
        self.poll_group_benchmark_progress(generation, key.clone(), progress.clone(), cx);
        let total = targets.len();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    runtime.test_proxy_delay_targets_with_progress(
                        &targets,
                        move |node_name, delay| {
                            if let Ok(mut updates) = progress.lock() {
                                updates.push_back((node_name.to_owned(), delay));
                            }
                        },
                    )
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_source_group_benchmark(&key, generation, total, result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn finish_source_group_benchmark(
        &mut self,
        key: &str,
        generation: u64,
        total: usize,
        result: Result<BTreeMap<String, u16>, mihomo::LoadError>,
        cx: &mut Context<Self>,
    ) {
        let language = self.language();
        if self.group_benchmark_active_generation != Some(generation) {
            return;
        }
        self.group_benchmark_active_generation = None;
        let Some(state) = self.group_benchmarks.get_mut(key) else {
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
                self.status = format!(
                    "{}: {}",
                    language.text("Source test completed", "来源测速完成"),
                    Self::success_fraction_label(summary.succeeded, summary.total, language)
                );
            }
            GroupBenchmarkState::Failed { .. } => {
                trace_ui(UiEvent::GroupBenchmarkFailed);
                self.status = format!(
                    "{}：{}",
                    language.text("Source test failed", "来源测速失败"),
                    failure.as_deref().unwrap_or_else(|| {
                        language.text("Mihomo did not return a result", "Mihomo 未返回结果")
                    })
                );
            }
            _ => return,
        }
        cx.notify();
    }

    pub(super) fn start_managed_policy_create(&mut self, cx: &mut Context<Self>) {
        self.managed_policy_editor_popover = None;
        self.managed_policy_draft = Some(ManagedPolicyDraft {
            editing_id: None,
            icon: ManagedPolicyIcon::None,
            strategy: ManagedPolicyStrategy::Manual,
            test_interval_secs: 600,
            matcher_kind: PolicyCandidateMatcherKind::All,
            explicit_members: BTreeSet::new(),
        });
        if let Some(input) = self.policy_group_name_input.as_ref() {
            input.update(
                cx,
                crate::subscription_input::SubscriptionTextInput::clear_without_event,
            );
        }
        if let Some(input) = self.policy_group_filter_input.as_ref() {
            input.update(
                cx,
                crate::subscription_input::SubscriptionTextInput::clear_without_event,
            );
        }
        self.language()
            .text("Creating policy group", "正在创建策略组")
            .clone_into(&mut self.status);
        cx.notify();
    }

    pub(super) fn start_managed_policy_edit(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(group) = self
            .managed_policy_groups
            .iter()
            .find(|group| group.id == id)
            .cloned()
        else {
            return;
        };
        let (matcher_kind, filter, explicit_members) = match &group.matcher {
            PolicyCandidateMatcher::All => (PolicyCandidateMatcherKind::All, "", BTreeSet::new()),
            PolicyCandidateMatcher::NameContains(value) => (
                PolicyCandidateMatcherKind::NameContains,
                value.as_str(),
                BTreeSet::new(),
            ),
            PolicyCandidateMatcher::Explicit(members) => {
                (PolicyCandidateMatcherKind::Explicit, "", members.clone())
            }
        };
        self.managed_policy_editor_popover = None;
        self.managed_policy_draft = Some(ManagedPolicyDraft {
            editing_id: Some(group.id),
            icon: group.icon,
            strategy: group.strategy,
            test_interval_secs: group.test_interval_secs,
            matcher_kind,
            explicit_members,
        });
        if let Some(input) = self.policy_group_name_input.as_ref() {
            input.update(cx, |input, cx| {
                input.set_value_without_event(group.name.clone(), cx);
            });
        }
        if let Some(input) = self.policy_group_filter_input.as_ref() {
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

    fn build_managed_policy(
        &self,
        draft: ManagedPolicyDraft,
        name: &str,
        filter: &str,
    ) -> Result<ManagedPolicyGroup, ManagedPolicyDraftError> {
        let id = draft
            .editing_id
            .clone()
            .unwrap_or_else(mihomo::new_managed_policy_id);
        let mut group =
            ManagedPolicyGroup::new(&id, name).map_err(|_| ManagedPolicyDraftError::InvalidName)?;
        if self
            .managed_policy_groups
            .iter()
            .any(|existing| existing.id != id && existing.name == name)
        {
            return Err(ManagedPolicyDraftError::DuplicateName);
        }
        if matches!(
            name,
            manis_profile::MANIS_GLOBAL_GROUP_NAME | "GLOBAL" | "DIRECT" | "REJECT"
        ) {
            return Err(ManagedPolicyDraftError::ReservedName);
        }
        group.icon = draft.icon;
        group.strategy = draft.strategy;
        group
            .set_test_interval_secs(draft.test_interval_secs)
            .map_err(|_| ManagedPolicyDraftError::InvalidInterval)?;
        let matcher = match draft.matcher_kind {
            PolicyCandidateMatcherKind::All => PolicyCandidateMatcher::All,
            PolicyCandidateMatcherKind::NameContains => {
                PolicyCandidateMatcher::name_contains(filter)
                    .map_err(|_| ManagedPolicyDraftError::MissingFilter)?
            }
            PolicyCandidateMatcherKind::Explicit if draft.explicit_members.is_empty() => {
                return Err(ManagedPolicyDraftError::MissingExplicitMember);
            }
            PolicyCandidateMatcherKind::Explicit => {
                PolicyCandidateMatcher::Explicit(draft.explicit_members)
            }
        };
        let explicit = matches!(matcher, PolicyCandidateMatcher::Explicit(_));
        group
            .set_matcher(matcher)
            .map_err(|_| ManagedPolicyDraftError::NoCandidates)?;
        if !explicit && self.managed_policy_candidate_count(&group) == 0 {
            return Err(ManagedPolicyDraftError::NoCandidates);
        }
        let mut proposed = self.managed_policy_groups.clone();
        if let Some(existing) = proposed.iter_mut().find(|existing| existing.id == group.id) {
            existing.clone_from(&group);
        } else {
            proposed.push(group.clone());
        }
        mihomo::validate_managed_policy_references(&proposed)
            .map_err(|error| ManagedPolicyDraftError::InvalidReferences(error.to_string()))?;
        Ok(group)
    }

    pub(super) fn save_managed_policy(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.managed_policy_draft.clone() else {
            return;
        };
        let name = self
            .policy_group_name_input
            .as_ref()
            .map(|input| input.read(cx).value().trim().to_owned())
            .unwrap_or_default();
        let filter = self
            .policy_group_filter_input
            .as_ref()
            .map(|input| input.read(cx).value().trim().to_owned())
            .unwrap_or_default();
        let language = self.language();
        let group = match self.build_managed_policy(draft, &name, &filter) {
            Ok(group) => group,
            Err(error) => {
                self.status = error.message(language);
                cx.notify();
                return;
            }
        };
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            language
                .text(
                    "Could not determine where to save policy groups",
                    "无法确定策略组保存位置",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        };
        let group_name = group.name.clone();
        self.status = format!(
            "{} “{}”; {}",
            language.text("Group saved", "分组已保存"),
            group_name,
            language.text("applying changes", "正在应用更改")
        );
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    super::mutate_saved_sources(&runtime, &store_dir, || {
                        mihomo::save_managed_policy_in(&store_dir, &group).map(|()| group)
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                this.finish_managed_policy_save(result, cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn finish_managed_policy_save(
        &mut self,
        result: Result<super::SourceMutation<ManagedPolicyGroup>, SubscriptionStoreError>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(transaction) => {
                let language = self.language();
                transaction.apply.reconcile_proxy_mode(&mut self.proxy_mode);
                if let Some(group) = transaction.value {
                    if let Some(existing) = self
                        .managed_policy_groups
                        .iter_mut()
                        .find(|existing| existing.id == group.id)
                    {
                        existing.clone_from(&group);
                    } else {
                        self.managed_policy_groups.push(group.clone());
                        self.managed_policy_groups
                            .sort_by(|left, right| left.id.cmp(&right.id));
                    }
                    self.group_benchmarks
                        .remove(&Self::managed_policy_benchmark_key(&group.id));
                    self.managed_policy_runtime_states.remove(&group.id);
                    self.managed_policy_draft = None;
                    self.managed_policy_editor_popover = None;
                    self.status = format!(
                        "{} “{}”{}",
                        language.text("Group saved", "分组已保存"),
                        group.name,
                        transaction.apply.status_suffix(language)
                    );
                } else {
                    self.status = format!(
                        "{}{}",
                        language.text("Failed to save policy group", "策略组保存失败"),
                        transaction.apply.status_suffix_after_rollback_attempt(
                            language,
                            transaction.rollback_error.as_ref(),
                        )
                    );
                }
            }
            Err(error) => {
                self.status = format!(
                    "{}: {error}",
                    self.language()
                        .text("Failed to save policy group", "策略组保存失败")
                );
            }
        }
        cx.notify();
    }

    pub(super) fn remove_managed_policy(&mut self, id: &str, cx: &mut Context<Self>) {
        let reference = format!("policy:{id}");
        if self.managed_policy_groups.iter().any(|group| {
            matches!(
                &group.matcher,
                PolicyCandidateMatcher::Explicit(members)
                    if members.iter().any(|member| member.source_id == reference)
            )
        }) {
            self.language()
                .text(
                    "This policy group is used by another policy group and cannot be deleted",
                    "该策略组正被其他策略组使用，无法删除",
                )
                .clone_into(&mut self.status);
            cx.notify();
            return;
        }
        let Some(store_dir) = self.subscription_store_dir.clone() else {
            return;
        };
        let Some(index) = self
            .managed_policy_groups
            .iter()
            .position(|group| group.id == id)
        else {
            return;
        };
        let language = self.language();
        let group = self.managed_policy_groups[index].clone();
        let remove_id = id.to_owned();
        self.status = format!(
            "{} “{}”; {}",
            language.text("Group deleted", "分组已删除"),
            group.name,
            language.text("applying changes", "正在应用更改")
        );
        let runtime = self.runtime.clone();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = executor
                .spawn(async move {
                    super::mutate_saved_sources(&runtime, &store_dir, || {
                        mihomo::remove_managed_policy_in(&store_dir, &remove_id)
                            .map(|()| (remove_id, group))
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(transaction) => {
                        let language = this.language();
                        transaction.apply.reconcile_proxy_mode(&mut this.proxy_mode);
                        if let Some((deleted_id, group)) = transaction.value {
                            this.managed_policy_groups
                                .retain(|candidate| candidate.id != deleted_id);
                            this.group_benchmarks
                                .remove(&Self::managed_policy_benchmark_key(&deleted_id));
                            this.managed_policy_runtime_states.remove(&deleted_id);
                            if this
                                .managed_policy_draft
                                .as_ref()
                                .and_then(|draft| draft.editing_id.as_deref())
                                == Some(deleted_id.as_str())
                            {
                                this.managed_policy_draft = None;
                                this.managed_policy_editor_popover = None;
                            }
                            this.status = format!(
                                "{} “{}”{}",
                                language.text("Group deleted", "分组已删除"),
                                group.name,
                                transaction.apply.status_suffix(language)
                            );
                        } else {
                            this.status = format!(
                                "{}{}",
                                language.text("Failed to delete policy group", "策略组删除失败"),
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
                                .text("Failed to delete policy group", "策略组删除失败")
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

    fn node_configuration_link(language: Language, cx: &mut Context<Self>) -> Button {
        action_button(
            "nodes-open-configuration",
            language.message(Message::ManageSources),
            ActionRole::Quiet,
            ControlSize::Compact,
        )
        .accessibility_label(language.text("Manage subscription sources", "管理订阅来源"))
        .px_3()
        .cursor_pointer()
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
    ) -> Button {
        action_button(
            "nodes-refresh",
            if refreshing {
                language.text("Loading…", "读取中…")
            } else {
                language.message(Message::RefreshNodes)
            },
            ActionRole::Secondary,
            ControlSize::Compact,
        )
        .accessibility_label(language.text("Refresh node health", "刷新节点健康状态"))
        .tab_stop(!refreshing)
        .px_3()
        .cursor_pointer()
        .text_color(if refreshing {
            theme.text_tertiary
        } else {
            theme.action_primary
        })
        .on_click(cx.listener(move |this, _, _, cx| {
            if refreshing {
                return;
            }
            if !this.imported_subscriptions.is_empty() {
                for subscription in this
                    .imported_subscriptions
                    .iter_mut()
                    .filter(|subscription| subscription.enabled)
                {
                    let kind = super::source_kind(&subscription.source);
                    subscription.state = ImportedSubscriptionState::Pending(kind);
                }
                this.restore_imported_subscriptions(cx);
            } else if !this.saved_single_nodes.is_empty() {
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
            .mt(Space::Lg.px())
            .flex()
            .items_center()
            .gap(if compact {
                Space::Md.px()
            } else {
                Space::Xl.px()
            })
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
                    .text_size(TextRole::SectionTitle.size())
                    .line_height(TextRole::SectionTitle.line_height())
                    .font_weight(TextRole::SectionTitle.weight())
                    .text_color(color)
                    .child(count.to_string()),
            )
            .child(
                div()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
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
                    .h(ControlSize::Icon.min_pointer_target())
                    .px_3()
                    .rounded(Radius::Control.px())
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

    fn source_group_list(
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
            list = list.child(self.source_group(group, filter, compact, language, theme, cx));
        }
        list
    }

    #[allow(clippy::too_many_lines)]
    fn source_group(
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
        let benchmark_key = Self::source_group_benchmark_key(&group.id);
        let benchmark = self
            .group_benchmarks
            .get(&benchmark_key)
            .cloned()
            .unwrap_or_default();
        let benchmarking = benchmark.is_running();
        let benchmark_id = group.id.clone();
        let benchmark_name = group.name.clone();
        let delay_targets = group.delay_targets();
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
        let trigger_group_id = group.id.clone();
        let trigger = Button::new(format!("source-group-header-{}", group.id))
            .accessibility_label(format!(
                "{} {} {}",
                action,
                language.text("node source", "节点来源"),
                group.name
            ))
            .with_variant(ButtonVariant::Ghost)
            .h_full()
            .flex_1()
            .px_0()
            .text_color(theme.text_primary)
            .child(
                div()
                    .min_w(px(0.0))
                    .w_full()
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
                                    .overflow_x_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(group.name.clone()),
                            )
                            .child(
                                div()
                                    .mt(Space::Xs.px())
                                    .text_size(TextRole::Metadata.size())
                                    .line_height(TextRole::Metadata.line_height())
                                    .text_color(theme.text_tertiary)
                                    .child(detail),
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
                                    .text_size(TextRole::Metadata.size())
                                    .line_height(TextRole::Metadata.line_height())
                                    .text_color(theme.text_secondary)
                                    .child(Self::node_count_label(counts.total, language)),
                            )
                            .child(
                                Icon::new(if collapsed {
                                    IconName::ChevronRight
                                } else {
                                    IconName::ChevronDown
                                })
                                .xsmall()
                                .text_color(theme.action_primary),
                            ),
                    ),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.node_workspace.toggle_group(&trigger_group_id);
                this.persist_node_workspace();
                this.language()
                    .text(
                        "Node source expanded state updated",
                        "已更新节点来源展开状态",
                    )
                    .clone_into(&mut this.status);
                cx.notify();
            }));
        let header = div()
            .min_h(px(58.0))
            .px(if compact { px(12.0) } else { px(16.0) })
            .py_3()
            .flex()
            .items_center()
            .gap_3()
            .bg(theme.surface_low)
            .child(Self::group_benchmark_icon(
                &benchmark_key,
                benchmarking,
                theme,
                cx.listener(move |this, _, _, cx| {
                    if !benchmarking {
                        this.start_source_group_benchmark(
                            &benchmark_id,
                            &benchmark_name,
                            delay_targets.clone(),
                            cx,
                        );
                    }
                }),
            ))
            .child(trigger);

        let content = if visible_count == 0 {
            div()
                .px_4()
                .py_3()
                .border_t_1()
                .border_color(theme.outline_subtle)
                .text_size(TextRole::Body.size())
                .line_height(TextRole::Body.line_height())
                .text_color(theme.text_secondary)
                .child(language.text(
                    "No nodes from this source match the current filter.",
                    "这个来源中没有符合当前筛选的节点。",
                ))
                .into_any_element()
        } else {
            self.source_group_table(
                group,
                &benchmark,
                NodeWorkspaceView {
                    filter,
                    compact,
                    language,
                    theme,
                },
                cx,
            )
            .into_any_element()
        };

        div()
            .rounded(Radius::Pane.px())
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .overflow_hidden()
            .child(
                Collapsible::new()
                    .open(!collapsed)
                    .child(header)
                    .content(content),
            )
    }

    fn source_group_table(
        &self,
        group: &NodeSourceGroup<'_>,
        benchmark: &GroupBenchmarkState,
        view: NodeWorkspaceView,
        cx: &mut Context<Self>,
    ) -> Div {
        let NodeWorkspaceView {
            filter,
            compact,
            language,
            theme,
        } = view;
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
                    node,
                    benchmark,
                    WorkspaceNodeRowContext {
                        row_id: format!("node-row-{}-{provider_index}-{node_index}", group.id),
                        source_id: group.id.clone(),
                        compact,
                        language,
                        theme,
                    },
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
                &loaded,
                benchmark,
                WorkspaceNodeRowContext {
                    row_id: format!(
                        "node-row-{}-{}-{node_index}",
                        group.id,
                        group.providers.len()
                    ),
                    source_id: group.id.clone(),
                    compact,
                    language,
                    theme,
                },
                cx,
            ));
        }
        table
    }

    fn node_table_header(language: Language, theme: Theme) -> Div {
        div()
            .h(ControlSize::Compact.height())
            .px_4()
            .flex()
            .items_center()
            .gap_3()
            .bg(theme.surface_low)
            .text_size(TextRole::Metadata.size())
            .line_height(TextRole::Metadata.line_height())
            .font_weight(TextRole::Metadata.weight())
            .text_color(theme.text_tertiary)
            .child(div().flex_1().child(language.text("Node", "节点")))
            .child(div().w(px(100.0)).child(language.text("Protocol", "协议")))
            .child(
                div()
                    .w(px(72.0))
                    .text_align(gpui::TextAlign::Right)
                    .child(language.text("Latency", "延迟")),
            )
    }

    fn workspace_node_row(
        &self,
        node: &LoadedProviderNode,
        benchmark: &GroupBenchmarkState,
        context: WorkspaceNodeRowContext,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let WorkspaceNodeRowContext {
            row_id,
            source_id,
            compact,
            language,
            theme,
        } = context;
        let latency = benchmark.node_state(&node.name);
        let idle_latency = node.latency_label.clone().unwrap_or_else(|| "—".to_owned());
        let spinner_id = format!("{row_id}-latency");
        let global_identity = NodeIdentity::new(&source_id, &node.name).ok();
        let global_runtime_selected = self.runtime_global_target() == Some(node.name.as_str());
        let global_selected = global_identity.as_ref().is_some_and(|identity| {
            self.global_target_identity()
                .map_or(global_runtime_selected, |selected| selected == identity)
        });
        let selection_locked = self.global_selection_busy.is_some();
        let selected_name = node.name.clone();
        let row_body = if compact {
            Self::compact_node_row_content(
                node,
                latency,
                idle_latency,
                &spinner_id,
                language,
                theme,
            )
        } else {
            Self::wide_node_row_content(node, latency, idle_latency, &spinner_id, language, theme)
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
            .child(row_body)
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
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !selection_locked {
                            this.select_global_node(selected_identity.clone(), cx);
                        }
                    }))
            })
    }

    fn compact_node_row_content(
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
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                            .text_color(theme.text_tertiary)
                            .child(node.protocol.clone()),
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
                    .w(px(100.0))
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
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
        let action = action_button(
            "nodes-empty-import",
            language.message(Message::ImportSubscription),
            ActionRole::Primary,
            ControlSize::Standard,
        )
        .accessibility_label(language.text(
            "Go to Configuration to import a subscription",
            "前往配置导入订阅",
        ))
        .cursor_pointer()
        .w(px(180.0))
        .px_4()
        .bg(theme.action_primary)
        .text_color(theme.action_on_primary)
        .border_color(theme.action_primary)
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
        .into_any_element();

        div()
            .min_h(px(if compact { 260.0 } else { 320.0 }))
            .flex()
            .items_center()
            .child(empty_state(
                language.message(Message::NoNodes),
                language.text(
                    "Import a subscription or add a VLESS node; nodes will then appear here automatically.",
                    "导入订阅或添加 VLESS 节点后，节点会自动出现在这里。",
                ),
                Some(action),
                theme,
            ))
    }

    fn node_message_panel(title: &'static str, copy: &'static str, theme: Theme) -> Div {
        empty_state(title, copy, None, theme)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::{NodeCounts, NodeSourceGroup, subscription_provider_refs};
    use crate::app::{
        GroupBenchmarkNodeState, GroupBenchmarkState, GroupBenchmarkSummary,
        ManagedPolicyRuntimeState,
    };
    use crate::mihomo::{LoadedProvider, LoadedProviderNode, ProxyDelayTarget};

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
    fn imported_group_delay_targets_use_the_runtime_provider_id() {
        let provider = LoadedProvider {
            name: "订阅预览".to_owned(),
            vehicle_type: Some("HTTP".to_owned()),
            nodes: vec![LoadedProviderNode {
                name: "HK 03".to_owned(),
                protocol: "Trojan".to_owned(),
                latency_label: None,
                alive: None,
            }],
        };
        let group = NodeSourceGroup {
            id: "subscription:fixture".to_owned(),
            name: "Fixture".to_owned(),
            detail: String::new(),
            providers: vec![&provider],
            runtime_provider_names: vec!["Subscription 1".to_owned()],
            saved_nodes: Vec::new(),
        };

        assert_eq!(
            group.delay_targets(),
            vec![ProxyDelayTarget::provider("Subscription 1", "HK 03")]
        );
    }

    #[test]
    fn imported_subscription_falls_back_to_the_matching_runtime_provider() {
        let cached = LoadedProvider {
            name: "Subscription 1".to_owned(),
            vehicle_type: Some("HTTP".to_owned()),
            nodes: vec![node(None)],
        };
        let unrelated = LoadedProvider {
            name: "Proxy".to_owned(),
            vehicle_type: Some("Compatible".to_owned()),
            nodes: vec![node(None)],
        };
        let runtime = vec![unrelated, cached.clone()];

        let restored = subscription_provider_refs(&[], &runtime, "Subscription 1");
        assert_eq!(restored, vec![&cached]);

        let preview = vec![LoadedProvider {
            name: "Fresh preview".to_owned(),
            vehicle_type: Some("HTTP".to_owned()),
            nodes: vec![node(Some(true))],
        }];
        let restored = subscription_provider_refs(&preview, &runtime, "Subscription 1");
        assert_eq!(restored, vec![&preview[0]]);
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
    fn managed_policy_runtime_rejects_unknown_selection() {
        let mut state = ManagedPolicyRuntimeState::Ready {
            generation: 4,
            current: Some("Tokyo".to_owned()),
            candidates: BTreeSet::from(["Tokyo".to_owned(), "Singapore".to_owned()]),
        };
        assert!(!state.begin_selection(5, "Unknown"));
        assert!(state.begin_selection(5, "Singapore"));
        assert!(matches!(
            state,
            ManagedPolicyRuntimeState::Selecting {
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
