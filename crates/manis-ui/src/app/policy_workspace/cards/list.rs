use super::super::{
    ActionRole, Button, Context, ControlSize, ControllerState, CountNoun, Div, FluentBuilder,
    GroupBenchmarkNodeState, GroupBenchmarkState, InteractiveElement, IntoElement, Language,
    ManagedPolicyIcon, ManisApp, Message, ParentElement, PolicyGroup, PolicyGroupIconView,
    PolicyListCardView, PolicyNode, PolicyNodeRowContext, PolicySelectionRequest, Space, Stateful,
    StatefulInteractiveElement, Styled, TextRole, Theme, UiEvent, action_button, copy, div,
    policy_kind_label, trace_ui,
};
use super::card::{self, PolicyCardContent, PolicyMember};

impl ManisApp {
    pub(in crate::app) fn policy_list_card_view<'a>(
        &self,
        item: &'a PolicyGroup,
    ) -> PolicyListCardView<'a> {
        let benchmark_key = Self::policy_group_benchmark_key(&item.id);
        let editable_group_id = self.editable_policy_group_id(&item.name).map(str::to_owned);
        let expanded = self.expanded_policy_group.as_ref().is_some_and(|expanded| {
            expanded == &item.id || editable_group_id.as_deref() == Some(expanded.as_str())
        });
        PolicyListCardView {
            editable_group_id,
            expanded,
            icon: self
                .managed_policies
                .groups
                .iter()
                .find(|group| group.name == item.name)
                .map_or(ManagedPolicyIcon::None, |group| group.icon),
            benchmarking: self
                .managed_policies
                .benchmarks
                .get(&benchmark_key)
                .is_some_and(GroupBenchmarkState::is_running),
            item,
            benchmark_key,
        }
    }

    pub(super) fn policy_list_card(
        &self,
        item: &PolicyGroup,
        language: Language,
        theme: Theme,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let view = self.policy_list_card_view(item);
        let benchmark = self.managed_policies.benchmarks.get(&view.benchmark_key);
        let selected_node = self.selected_node_for_policy(view.item);
        let (current_latency, current_latency_measured_at) = selected_node
            .map_or((None, None), |node| {
                self.resolved_policy_exit_latency(&node.name, node.latency_ms.map(u64::from))
            });
        let members =
            self.policy_list_members(&view, selected_node, benchmark, language, theme, cx);
        let feedback = benchmark
            .and_then(|state| {
                Self::policy_group_benchmark_feedback(language, state, view.item.nodes.len(), theme)
            })
            .map(gpui::IntoElement::into_any_element);
        card::policy_card(
            PolicyCardContent {
                card_id: format!("policy-card-{}", view.item.id.as_str()),
                toggle_id: format!("policy-{}", view.item.id.as_str()),
                name: &view.item.name,
                strategy: policy_kind_label(language, view.item.kind).to_owned(),
                meta: language.count(CountNoun::Node, view.item.nodes.len()),
                current_exit: selected_node.map_or_else(
                    || language.localized(copy::app::NO_AVAILABLE_NODES).to_owned(),
                    |node| Self::policy_candidate_display_name(&node.name).to_owned(),
                ),
                current_latency,
                current_latency_measured_at,
                has_exit: selected_node.is_some(),
                expanded: view.expanded,
                identity_action: Some(Self::policy_list_test_action(&view, language, theme, cx)),
                actions: vec![
                    Self::policy_settings_button(
                        view.item.id.as_str(),
                        view.editable_group_id.clone(),
                        language,
                        theme,
                        cx,
                    )
                    .into_any_element(),
                    crate::components::disclosure_icon(view.expanded, theme).into_any_element(),
                ],
                members,
                members_empty: (view.item.nodes.is_empty())
                    .then(|| Self::empty_policy_candidates(language, theme).into_any_element()),
                feedback,
                compact,
                language,
                theme,
            },
            Self::policy_list_toggle(&view, language, cx),
        )
    }

    /// The group's latency test: one action per card, as the design asks.
    fn policy_list_test_action(
        view: &PolicyListCardView<'_>,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let benchmarkable = Self::policy_group_benchmarkable(view.item);
        let benchmarking = view.benchmarking;
        let benchmark_id = view.item.id.clone();
        Self::policy_group_icon(
            PolicyGroupIconView {
                id: &view.benchmark_key,
                icon: view.icon,
                policy_name: &view.item.name,
                benchmarkable,
                running: benchmarking,
                language,
                theme,
            },
            cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                if benchmarkable && !benchmarking {
                    this.start_policy_group_benchmark(&benchmark_id, cx);
                }
            }),
        )
    }

    fn policy_list_members(
        &self,
        view: &PolicyListCardView<'_>,
        selected_node: Option<&PolicyNode>,
        benchmark: Option<&GroupBenchmarkState>,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        view.item
            .nodes
            .iter()
            .map(|node| {
                let (latency_override, latency_measured_at) =
                    self.resolved_policy_exit_latency(&node.name, node.latency_ms.map(u64::from));
                Self::node_row(
                    node,
                    PolicyNodeRowContext {
                        source: self.policy_node_source_label(node, language),
                        current: selected_node.is_some_and(|selected| node.id == selected.id),
                        selection: PolicySelectionRequest {
                            group_id: view.item.id.clone(),
                            group_name: view.item.name.clone(),
                            node_id: node.id.clone(),
                            node_name: node.name.clone(),
                        },
                        manually_selectable: view.item.kind.allows_manual_selection(),
                        selection_busy: self.policy_selection_busy.is_some(),
                        benchmark_state: benchmark.map_or(GroupBenchmarkNodeState::Idle, |state| {
                            state.node_state(&node.name)
                        }),
                        latency_override,
                        latency_measured_at,
                        exit_ip: self.exit_ip_label(&node.name).map(str::to_owned),
                        language,
                        theme,
                    },
                    cx,
                )
                .into_any_element()
            })
            .collect()
    }

    fn policy_list_toggle(
        view: &PolicyListCardView<'_>,
        language: Language,
        cx: &mut Context<Self>,
    ) -> impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static {
        let item_id = view.item.id.clone();
        let item_name = view.item.name.clone();
        let expanded = view.expanded;
        let action = if view.expanded {
            language.localized(copy::common::COLLAPSE)
        } else {
            language.localized(copy::common::EXPAND)
        };
        let item_target_node = view
            .item
            .target
            .as_deref()
            .and_then(|target| view.item.nodes.iter().find(|node| node.name == target))
            .map(|node| node.id.clone());
        cx.listener(move |this, _, _, cx| {
            if expanded {
                this.expanded_policy_group = None;
            } else {
                this.expanded_policy_group = Some(item_id.clone());
            }
            this.workspace.select_group(item_id.clone());
            if let Some(target) = item_target_node.clone() {
                this.workspace.select_node(target);
            }
            trace_ui(UiEvent::PolicyPreviewOpened);
            this.status = copy::app::policy_group_action(this.language(), &item_name, action);
            cx.notify();
        })
    }

    /// The stored policy group behind a runtime group name, when the user can edit it.
    pub(in crate::app) fn editable_policy_group_id(&self, policy_name: &str) -> Option<&str> {
        self.managed_policies
            .groups
            .iter()
            .find(|group| group.name == policy_name)
            .map(|group| group.id.as_str())
    }

    fn empty_policy_candidates(language: Language, theme: Theme) -> Div {
        div()
            .py(Space::Sm.px())
            .text_size(TextRole::Body.size())
            .line_height(TextRole::Body.line_height())
            .text_color(theme.text_secondary)
            .child(language.localized(copy::app::THIS_POLICY_HAS_NO_CANDIDATE_NODES))
    }

    pub(in crate::app) fn managed_policy_add_button(
        id: &'static str,
        language: Language,
        _theme: Theme,
        cx: &mut Context<Self>,
    ) -> Button {
        action_button(
            id,
            language.message(Message::AddPolicyGroup),
            ActionRole::Primary,
            ControlSize::Compact,
        )
        .debug_selector(move || id.to_owned())
        .accessibility_label(language.message(Message::AddPolicyGroup))
        .on_click(cx.listener(|this, _, window, cx| {
            this.open_managed_policy_create(window, cx);
        }))
    }

    pub(in crate::app) fn connection_button(
        &self,
        _theme: Theme,
        cx: &mut Context<Self>,
    ) -> Button {
        let connecting = matches!(self.controller, ControllerState::Connecting { .. });
        let language = self.language();
        action_button(
            "connect-mihomo",
            self.runtime
                .button_label_in(&self.controller, self.language()),
            ActionRole::Secondary,
            ControlSize::Compact,
        )
        .accessibility_label(
            if matches!(self.controller, ControllerState::Failed { .. }) {
                language.message(Message::Retry)
            } else {
                language.message(Message::ConnectMihomo)
            },
        )
        .tab_stop(!connecting)
        .loading(connecting)
        .when(connecting, gpui::Styled::cursor_default)
        .on_click(cx.listener(|this, _, _, cx| this.connect_mihomo(cx)))
    }

    /// A policy candidate as a selectable chip. The chip keeps the source and exit IP that the old
    /// table row carried in a hover tooltip, so the denser card loses no information.
    pub(super) fn node_row(
        item: &PolicyNode,
        context: PolicyNodeRowContext,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let PolicyNodeRowContext {
            source,
            selection,
            current,
            manually_selectable,
            selection_busy,
            benchmark_state,
            latency_override,
            latency_measured_at,
            exit_ip,
            language,
            theme,
        } = context;
        let latency = match benchmark_state {
            GroupBenchmarkNodeState::Measured(milliseconds) => Some(u64::from(milliseconds)),
            _ => latency_override.or_else(|| item.latency_ms.map(u64::from)),
        };
        let selectable = manually_selectable && !selection_busy;
        let tooltip = exit_ip.map_or(source.clone(), |ip| format!("{source} · IP {ip}"));
        let chip_id = format!(
            "policy-node-{}-{}",
            selection.group_id.as_str(),
            item.id.as_str()
        );
        card::member_chip(
            PolicyMember {
                id: chip_id,
                name: Self::policy_candidate_display_name(&item.name),
                latency,
                latency_measured_at,
                selected: current,
                selectable,
                language,
                theme,
            },
            cx.listener(move |this, _, _, cx| {
                if selectable {
                    this.select_policy_node(selection.clone(), cx);
                }
            }),
        )
        .tooltip(move |window, cx| {
            gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
        })
    }
}
