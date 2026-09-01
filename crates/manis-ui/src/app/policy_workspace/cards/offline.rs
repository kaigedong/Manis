use super::super::*;

impl ManisApp {
    pub(super) fn empty_policy_content(&self, language: Language, theme: Theme) -> Div {
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

    fn offline_policy_header(
        view: &OfflinePolicyCardView<'_>,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let candidate_count = view.candidates.len();
        let benchmarkable = candidate_count > 0;
        let benchmarking = view.benchmarking;
        let benchmark_name = view.policy.name.clone();
        let toggle_id = PolicyGroupId::new(view.policy.id.clone());
        let expanded = view.expanded;
        let action = if view.expanded {
            language.localized(copy::common::COLLAPSE)
        } else {
            language.localized(copy::common::EXPAND)
        };
        div()
            .id(format!("saved-policy-header-{}", view.policy.id))
            .debug_selector({
                let id = view.policy.id.clone();
                move || format!("saved-policy-header-{id}")
            })
            .role(Role::Button)
            .aria_label(format!("{action} {}", view.policy.name))
            .tab_stop(true)
            .focusable()
            .map(crate::components::primary_button_interaction)
            .cursor_pointer()
            .min_h(px(64.0))
            .px(Space::Lg.px())
            .py(Space::Md.px())
            .rounded_tl(Radius::Pane.px())
            .rounded_tr(Radius::Pane.px())
            .when(!view.expanded, |header| {
                header
                    .rounded_bl(Radius::Pane.px())
                    .rounded_br(Radius::Pane.px())
            })
            .bg(theme.surface_low)
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .child(Self::policy_group_icon(
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
            ))
            .child(Self::offline_policy_identity(view, language, theme))
            .child(Self::policy_settings_button(
                &view.policy.id,
                Some(view.policy.id.clone()),
                language,
                theme,
                cx,
            ))
            .child(Self::offline_policy_summary(
                candidate_count,
                view.expanded,
                language,
                theme,
            ))
            .on_click(cx.listener(move |this, _, _, cx| {
                if expanded {
                    this.expanded_policy_group = None;
                } else {
                    this.expanded_policy_group = Some(toggle_id.clone());
                }
                cx.notify();
            }))
    }

    fn offline_policy_identity(
        view: &OfflinePolicyCardView<'_>,
        language: Language,
        theme: Theme,
    ) -> Div {
        let kind = match view.policy.strategy {
            ManagedPolicyStrategy::Manual => language.localized(copy::app::MANUAL_SELECTION),
            ManagedPolicyStrategy::LowestLatency => {
                language.localized(copy::app::AUTOMATIC_SELECTION)
            }
        };
        div()
            .min_w(px(0.0))
            .flex_1()
            .child(
                div()
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Body.size())
                    .line_height(TextRole::Body.line_height())
                    .font_weight(TextRole::Label.weight())
                    .child(view.policy.name.clone()),
            )
            .child(
                div()
                    .mt(Space::Xs.px())
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_secondary)
                    .child(kind),
            )
    }

    fn offline_policy_summary(
        candidate_count: usize,
        expanded: bool,
        language: Language,
        theme: Theme,
    ) -> Div {
        div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .child(
                div()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_secondary)
                    .child(language.count(CountNoun::Node, candidate_count)),
            )
            .child(crate::components::disclosure_icon(expanded, theme))
    }

    pub(super) fn offline_policy_card(
        &self,
        policy: &ManagedPolicyGroup,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let view = self.offline_policy_card_view(policy);
        let card_id = format!("saved-policy-card-{}", view.policy.id);
        let mut card = div()
            .debug_selector(move || card_id.clone())
            .flex_shrink_0()
            .rounded(Radius::Pane.px())
            .border_1()
            .border_color(theme.outline_subtle)
            .overflow_hidden()
            .child(Self::offline_policy_header(&view, language, theme, cx));
        if !view.expanded {
            return card;
        }
        if view.candidates.is_empty() {
            card = card.child(
                div()
                    .px_4()
                    .py(Space::Md.px())
                    .border_t_1()
                    .border_color(theme.outline_subtle)
                    .text_size(TextRole::Body.size())
                    .line_height(TextRole::Body.line_height())
                    .text_color(theme.text_secondary)
                    .child(
                        language
                            .localized(copy::app::NO_IMPORTED_NODES_CURRENTLY_MATCH_THIS_POLICY),
                    ),
            );
        } else {
            card = card.child(Self::policy_candidate_table_header(language, theme));
            let benchmark_key = Self::managed_policy_benchmark_key(&view.policy.id);
            let runtime_key =
                Self::policy_group_benchmark_key(&PolicyGroupId::new(view.policy.name.clone()));
            let benchmark = self
                .managed_policies
                .benchmarks
                .get(&benchmark_key)
                .or_else(|| self.managed_policies.benchmarks.get(&runtime_key));
            for candidate in &view.candidates {
                let current = view.selected_name.as_deref() == Some(candidate.name.as_str());
                card = card.child(Self::node_row(
                    candidate,
                    PolicyNodeRowContext {
                        source: self.policy_node_source_label(candidate, language),
                        selection: PolicySelectionRequest {
                            group_id: PolicyGroupId::new(view.policy.id.clone()),
                            group_name: view.policy.name.clone(),
                            node_id: candidate.id.clone(),
                            node_name: candidate.name.clone(),
                        },
                        current,
                        manually_selectable: view.policy.strategy == ManagedPolicyStrategy::Manual,
                        selection_busy: self.policy_selection_busy.is_some(),
                        benchmark_state: benchmark.map_or(GroupBenchmarkNodeState::Idle, |state| {
                            state.node_state(&candidate.name)
                        }),
                        language,
                        theme,
                    },
                    cx,
                ));
            }
        }
        card
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
