use super::super::{
    ActionRole, Button, Context, ControlSize, ControllerState, CountNoun, Div, FluentBuilder,
    GroupBenchmarkNodeState, GroupBenchmarkState, InteractiveElement, Language, ManagedPolicyIcon,
    ManisApp, Message, ParentElement, PolicyGroup, PolicyGroupIconView, PolicyListCardView,
    PolicyNode, PolicyNodeRowContext, PolicySelectionRequest, Radius, Role, Space, Stateful,
    StatefulInteractiveElement, StatusTone, Styled, TextRole, Theme, Toggled, UiEvent,
    action_button, copy, div, policy_kind_label, px, status_badge, trace_ui,
};

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
        cx: &mut Context<Self>,
    ) -> Div {
        let view = self.policy_list_card_view(item);
        let card_id = format!("policy-card-{}", view.item.id.as_str());
        let mut card = div()
            .debug_selector(move || card_id.clone())
            .flex_shrink_0()
            .rounded(Radius::Pane.px())
            .border_1()
            .border_color(theme.outline_subtle)
            .overflow_hidden()
            .child(Self::policy_list_header(&view, language, theme, cx));
        if !view.expanded {
            return card;
        }
        if let Some(feedback) = self
            .managed_policies
            .benchmarks
            .get(&view.benchmark_key)
            .and_then(|state| {
                Self::policy_group_benchmark_feedback(language, state, view.item.nodes.len(), theme)
            })
        {
            card = card.child(feedback.mx_3().mb_2());
        }
        if view.item.nodes.is_empty() {
            return card.child(Self::empty_policy_candidates(language, theme));
        }
        card = card.child(Self::policy_candidate_table_header(language, theme));
        let selected_node = self.selected_node_for_policy(view.item);
        for node in &view.item.nodes {
            let benchmark_state = self
                .managed_policies
                .benchmarks
                .get(&view.benchmark_key)
                .map_or(GroupBenchmarkNodeState::Idle, |state| {
                    state.node_state(&node.name)
                });
            let context = PolicyNodeRowContext {
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
                benchmark_state,
                language,
                theme,
            };
            card = card.child(Self::node_row(node, context, cx));
        }
        card
    }

    fn policy_list_header(
        view: &PolicyListCardView<'_>,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let benchmarkable = Self::policy_group_benchmarkable(view.item);
        let benchmarking = view.benchmarking;
        let benchmark_id = view.item.id.clone();
        let item_id = view.item.id.clone();
        let item_name = view.item.name.clone();
        let expanded = view.expanded;
        let item_target_node = view
            .item
            .target
            .as_deref()
            .and_then(|target| view.item.nodes.iter().find(|node| node.name == target))
            .map(|node| node.id.clone());
        let action = if view.expanded {
            language.localized(copy::common::COLLAPSE)
        } else {
            language.localized(copy::common::EXPAND)
        };
        div()
            .id(format!("policy-{}", view.item.id.as_str()))
            .debug_selector({
                let id = view.item.id.clone();
                move || format!("policy-{}", id.as_str())
            })
            .role(Role::Button)
            .aria_label(format!("{action} {}", view.item.name))
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
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .bg(theme.surface_low)
            .hover(|header| header.bg(theme.action_soft))
            .child(Self::policy_group_icon(
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
            ))
            .child(Self::policy_list_identity(view.item, language, theme))
            .child(Self::policy_settings_button(
                view.item.id.as_str(),
                view.editable_group_id.clone(),
                language,
                theme,
                cx,
            ))
            .child(Self::policy_list_summary(
                view.item.nodes.len(),
                view.expanded,
                language,
                theme,
            ))
            .on_click(cx.listener(move |this, _, _, cx| {
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
            }))
    }

    fn policy_list_identity(item: &PolicyGroup, language: Language, theme: Theme) -> Div {
        let selected_target = item.target.as_deref().map_or_else(
            || language.localized(copy::app::NO_AVAILABLE_NODES),
            ManisApp::policy_candidate_display_name,
        );
        let target = copy::app::policy_identity(
            language,
            policy_kind_label(language, item.kind),
            selected_target,
        );
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
                    .text_color(theme.text_primary)
                    .child(item.name.clone()),
            )
            .child(
                div()
                    .mt(Space::Xs.px())
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_tertiary)
                    .child(target),
            )
    }

    fn policy_list_summary(
        node_count: usize,
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
                    .child(language.count(CountNoun::Node, node_count)),
            )
            .child(crate::components::disclosure_icon(expanded, theme))
    }

    fn empty_policy_candidates(language: Language, theme: Theme) -> Div {
        div()
            .px_4()
            .py(Space::Md.px())
            .border_t_1()
            .border_color(theme.outline_subtle)
            .text_size(TextRole::Body.size())
            .line_height(TextRole::Body.line_height())
            .text_color(theme.text_secondary)
            .child(language.localized(copy::app::THIS_POLICY_HAS_NO_CANDIDATE_NODES))
    }

    pub(super) fn managed_policy_add_button(
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
            language,
            theme,
        } = context;
        let detail = if item.name == manis_profile::MANIS_GLOBAL_GROUP_NAME {
            language
                .localized(copy::nodes::FOLLOW_HOME_SELECTION)
                .to_owned()
        } else if item.detail.trim().is_empty() {
            language.localized(copy::app::UNKNOWN_TYPE).to_owned()
        } else {
            item.detail.clone()
        };
        let idle_latency = item
            .latency_ms
            .map_or_else(|| "—".to_owned(), |latency| format!("{latency} ms"));
        let spinner_id = format!(
            "policy-node-{}-{}-latency",
            selection.group_id.as_str(),
            item.id.as_str()
        );
        let description = Self::policy_node_description(
            Self::policy_candidate_display_name(&item.name).to_owned(),
            detail,
            current,
            manually_selectable,
            language,
            theme,
        );
        let source = Self::policy_node_source(source, current, manually_selectable, theme);
        div()
            .id(format!(
                "policy-node-{}-{}",
                selection.group_id.as_str(),
                item.id.as_str()
            ))
            .tab_stop(manually_selectable && !selection_busy)
            .min_h(px(64.0))
            .px(Space::Md.px())
            .py(Space::Sm.px())
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .border_t_1()
            .border_color(theme.outline_subtle)
            .bg(if current {
                theme.action_soft
            } else {
                theme.surface_base
            })
            .child(description)
            .child(source)
            .child(
                div()
                    .w(px(64.0))
                    .flex_shrink_0()
                    .min_h(px(18.0))
                    .flex()
                    .items_center()
                    .justify_end()
                    .child(Self::benchmark_latency_content(
                        benchmark_state,
                        idle_latency,
                        &spinner_id,
                        language,
                        theme,
                    )),
            )
            .when(manually_selectable, |row| {
                row.hover(move |row| {
                    row.bg(if current {
                        theme.action_soft
                    } else {
                        theme.surface_low
                    })
                })
                .focus_visible(move |row| row.border_1().border_color(theme.focus_ring))
                .role(Role::RadioButton)
                .aria_toggled(if current {
                    Toggled::True
                } else {
                    Toggled::False
                })
                .focusable()
                .map(crate::components::primary_button_interaction)
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !selection_busy {
                        this.select_policy_node(selection.clone(), cx);
                    }
                }))
            })
    }

    fn policy_node_source(
        source: String,
        current: bool,
        manually_selectable: bool,
        theme: Theme,
    ) -> Div {
        div()
            .w(px(100.0))
            .flex_shrink_0()
            .overflow_x_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .text_size(TextRole::Metadata.size())
            .line_height(TextRole::Metadata.line_height())
            .text_color(if current || manually_selectable {
                theme.text_secondary
            } else {
                theme.text_tertiary
            })
            .child(source)
    }

    fn policy_node_description(
        name: String,
        detail: String,
        current: bool,
        manually_selectable: bool,
        language: Language,
        theme: Theme,
    ) -> Div {
        div()
            .flex_1()
            .min_w(px(0.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(Space::Sm.px())
                    .min_w(px(0.0))
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Body.size())
                    .line_height(TextRole::Body.line_height())
                    .font_weight(TextRole::Label.weight())
                    .text_color(if current || manually_selectable {
                        theme.text_primary
                    } else {
                        theme.text_secondary
                    })
                    .child(
                        div()
                            .min_w(px(0.0))
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(name),
                    )
                    .when(current, |name| {
                        name.child(div().flex_shrink_0().child(status_badge(
                            language.localized(copy::app::CURRENT),
                            StatusTone::Neutral,
                            theme,
                        )))
                    }),
            )
            .child(
                div()
                    .mt(Space::Xs.px())
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_tertiary)
                    .child(detail),
            )
    }

    pub(in crate::app) fn editable_policy_group_id(&self, policy_name: &str) -> Option<&str> {
        self.managed_policies
            .groups
            .iter()
            .find(|group| group.name == policy_name)
            .map(|group| group.id.as_str())
    }

    pub(super) fn policy_candidate_table_header(language: Language, theme: Theme) -> Div {
        div()
            .px(Space::Md.px())
            .py(Space::Sm.px())
            .bg(theme.surface_low)
            .flex()
            .gap(Space::Md.px())
            .text_size(TextRole::Metadata.size())
            .line_height(TextRole::Metadata.line_height())
            .font_weight(TextRole::Label.weight())
            .text_color(theme.text_tertiary)
            .child(
                div()
                    .flex_1()
                    .child(language.localized(copy::app::CANDIDATE_GROUP)),
            )
            .child(
                div()
                    .w(px(100.0))
                    .flex_shrink_0()
                    .child(language.localized(copy::app::SOURCE)),
            )
            .child(
                div()
                    .w(px(64.0))
                    .flex_shrink_0()
                    .text_right()
                    .child(language.localized(copy::common::LATENCY)),
            )
    }
}
