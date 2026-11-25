use super::super::*;
use super::card::{self, PolicyCardContent};

impl ManisApp {
    pub(in crate::app) fn empty_policy_content(&self, language: Language, theme: Theme) -> Div {
        let (title, description) = match &self.controller {
            ControllerState::Disconnected => (
                language.message(Message::NoPolicyGroups),
                language.localized(
                    copy::app::START_MIHOMO_TO_LOAD_YOUR_POLICY_GROUPS_AND_SELECTED_NODES,
                ),
            ),
            ControllerState::Connecting { .. } => (
                language.localized(copy::app::LOADING_POLICY_GROUPS),
                language
                    .localized(copy::app::MANIS_IS_LOADING_YOUR_CURRENT_GROUPS_AND_SELECTED_NODES),
            ),
            ControllerState::Failed { .. } => (
                language.localized(copy::app::POLICY_GROUPS_UNAVAILABLE),
                language
                    .localized(copy::app::MIHOMO_COULD_NOT_BE_STARTED_CHECK_LOGS_FOR_DETAILS_THEN),
            ),
            ControllerState::Connected { .. } => (
                language.localized(copy::app::NO_POLICY_GROUPS_YET),
                language.localized(copy::app::ADD_A_SOURCE_OR_CREATE_A_POLICY_GROUP_TO_CHOOSE),
            ),
        };

        empty_state(title, description, None, theme)
    }

    pub(in crate::app) fn offline_policy_card_view<'a>(
        &self,
        policy: &'a ManagedPolicyGroup,
    ) -> OfflinePolicyCardView<'a> {
        let policy_group_id = PolicyGroupId::new(policy.id.clone());
        OfflinePolicyCardView {
            candidates: self.managed_policy_candidate_nodes(policy),
            selected_name: self
                .managed_policies
                .node_selections
                .policy_target(&policy.name)
                .map(str::to_owned),
            expanded: self.expanded_policy_group.as_ref().is_some_and(|expanded| {
                expanded == &policy_group_id || expanded.as_str() == policy.name
            }),
            benchmarking: self.managed_policies.pending_benchmark_name.as_deref()
                == Some(policy.name.as_str()),
            policy,
        }
    }

    pub(super) fn offline_policy_card(
        &self,
        policy: &ManagedPolicyGroup,
        language: Language,
        theme: Theme,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let view = self.offline_policy_card_view(policy);
        let benchmark_key = Self::managed_policy_benchmark_key(&view.policy.id);
        let runtime_key =
            Self::policy_group_benchmark_key(&PolicyGroupId::new(view.policy.name.clone()));
        let benchmark = self
            .managed_policies
            .benchmarks
            .get(&benchmark_key)
            .or_else(|| self.managed_policies.benchmarks.get(&runtime_key));
        let selected = view
            .selected_name
            .as_deref()
            .and_then(|name| view.candidates.iter().find(|node| node.name == name));
        let members = self.offline_policy_members(&view, benchmark, language, theme, cx);
        card::policy_card(
            PolicyCardContent {
                card_id: format!("saved-policy-card-{}", view.policy.id),
                toggle_id: format!("saved-policy-header-{}", view.policy.id),
                name: &view.policy.name,
                strategy: Self::offline_policy_strategy(language, view.policy.strategy),
                meta: language.count(CountNoun::Node, view.candidates.len()),
                current_exit: selected.map_or_else(
                    || language.localized(copy::app::NO_AVAILABLE_NODES).to_owned(),
                    |node| Self::policy_candidate_display_name(&node.name).to_owned(),
                ),
                current_latency: selected.and_then(|node| node.latency_ms).map(u64::from),
                has_exit: selected.is_some(),
                expanded: view.expanded,
                identity_action: Some(Self::offline_policy_test_action(&view, language, theme, cx)),
                actions: vec![
                    Self::policy_settings_button(
                        &view.policy.id,
                        Some(view.policy.id.clone()),
                        language,
                        theme,
                        cx,
                    )
                    .into_any_element(),
                    crate::components::disclosure_icon(view.expanded, theme).into_any_element(),
                ],
                members,
                members_empty: view.candidates.is_empty().then(|| {
                    div()
                        .py(Space::Sm.px())
                        .text_size(TextRole::Body.size())
                        .line_height(TextRole::Body.line_height())
                        .text_color(theme.text_secondary)
                        .child(
                            language.localized(
                                copy::app::NO_IMPORTED_NODES_CURRENTLY_MATCH_THIS_POLICY,
                            ),
                        )
                        .into_any_element()
                }),
                feedback: None,
                compact,
                language,
                theme,
            },
            Self::offline_policy_toggle(&view, cx),
        )
    }

    /// Testing a saved group first needs the kernel, so the action starts Mihomo as well.
    fn offline_policy_test_action(
        view: &OfflinePolicyCardView<'_>,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let benchmarkable = !view.candidates.is_empty();
        let benchmarking = view.benchmarking;
        let benchmark_name = view.policy.name.clone();
        Self::policy_group_icon(
            PolicyGroupIconView {
                id: &format!("saved-{}", view.policy.id),
                icon: view.policy.icon,
                policy_name: &view.policy.name,
                benchmarkable,
                running: benchmarking,
                language,
                theme,
            },
            cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                if !benchmarking {
                    this.managed_policies.pending_benchmark_name = Some(benchmark_name.clone());
                    this.connect_mihomo(cx);
                }
            }),
        )
    }

    fn offline_policy_members(
        &self,
        view: &OfflinePolicyCardView<'_>,
        benchmark: Option<&GroupBenchmarkState>,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let manually_selectable = view.policy.strategy == ManagedPolicyStrategy::Manual;
        let selection_busy = self.policy_selection_busy.is_some();
        view.candidates
            .iter()
            .map(|candidate| {
                Self::node_row(
                    candidate,
                    PolicyNodeRowContext {
                        source: self.policy_node_source_label(candidate, language),
                        selection: PolicySelectionRequest {
                            group_id: PolicyGroupId::new(view.policy.id.clone()),
                            group_name: view.policy.name.clone(),
                            node_id: candidate.id.clone(),
                            node_name: candidate.name.clone(),
                        },
                        current: view.selected_name.as_deref() == Some(candidate.name.as_str()),
                        manually_selectable,
                        selection_busy,
                        benchmark_state: benchmark.map_or(GroupBenchmarkNodeState::Idle, |state| {
                            state.node_state(&candidate.name)
                        }),
                        exit_ip: self.exit_ip_label(&candidate.name).map(str::to_owned),
                        language,
                        theme,
                    },
                    cx,
                )
                .into_any_element()
            })
            .collect()
    }

    fn offline_policy_toggle(
        view: &OfflinePolicyCardView<'_>,
        cx: &mut Context<Self>,
    ) -> impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static {
        let expanded = view.expanded;
        let toggle_id = PolicyGroupId::new(view.policy.id.clone());
        cx.listener(move |this, _, _, cx| {
            if expanded {
                this.expanded_policy_group = None;
            } else {
                this.expanded_policy_group = Some(toggle_id.clone());
            }
            cx.notify();
        })
    }

    pub(in crate::app) fn offline_policy_strategy(
        language: Language,
        strategy: ManagedPolicyStrategy,
    ) -> String {
        match strategy {
            ManagedPolicyStrategy::Manual => language.localized(copy::app::MANUAL_SELECTION),
            ManagedPolicyStrategy::LowestLatency => {
                language.localized(copy::app::AUTOMATIC_SELECTION)
            }
        }
        .to_owned()
    }

    pub(super) fn policy_settings_button(
        row_id: &str,
        group_id: Option<String>,
        language: Language,
        _theme: Theme,
        cx: &mut Context<Self>,
    ) -> Button {
        let editable = group_id.is_some();
        let selector = format!("policy-settings-{row_id}");
        action_button(
            format!("policy-settings-{row_id}"),
            if editable {
                language.message(Message::Settings)
            } else {
                language.localized(copy::app::READ_ONLY)
            },
            ActionRole::Secondary,
            ControlSize::Compact,
        )
        .debug_selector(move || selector.clone())
        .disabled(!editable)
        .when(!editable, gpui::Styled::cursor_default)
        .accessibility_label(language.localized(if editable {
            copy::common::EDIT_POLICY_GROUP
        } else {
            copy::app::THIS_RUNTIME_POLICY_IS_READ_ONLY_IN_MANIS
        }))
        .on_click(cx.listener(move |this, _, window, cx| {
            cx.stop_propagation();
            if let Some(id) = group_id.as_deref() {
                this.open_managed_policy_settings(id, window, cx);
            }
        }))
    }
}
