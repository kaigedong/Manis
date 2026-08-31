use super::{
    GroupBenchmarkNodeState, GroupBenchmarkState, ManisApp, OfflinePolicyCardView,
    PolicyListCardView, PolicyNodeRowContext, PolicySelectionRequest, compact_proxy_mode_label,
    controller_status_label, policy_kind_label, policy_presentation::PolicyGroupIconView,
    proxy_mode_label, routing_mode_label, status_bar_values,
};
use crate::{
    assets, brand,
    components::{ActionRole, StatusTone, action_button, empty_state, page_heading, status_badge},
    diagnostics::{UiEvent, trace_ui},
    localization::{CountNoun, Language, Message, copy},
    mihomo::{ControllerState, LiveStreamPhase},
    theme::{ControlSize, LayoutMetric, Radius, Space, TextRole, Theme},
};
use gpui::{
    AnyElement, Context, Div, ParentElement, Role, Stateful, Styled, Toggled, div, img, prelude::*,
    px,
};
use gpui_component::{
    Disableable, IconName, Selectable, Sizable,
    button::{Button, ButtonGroup, ButtonVariant, ButtonVariants},
    status_bar::StatusBar,
};
use manis_core::{
    ManagedPolicyGroup, ManagedPolicyIcon, ManagedPolicyStrategy, PolicyGroup, PolicyGroupId,
    PolicyNode, PrimaryWorkspace, ProxyMode, RoutingMode, WindowSizeClass,
};

impl ManisApp {
    pub(super) fn chrome(
        &self,
        theme: Theme,
        size_class: WindowSizeClass,
        cx: &mut Context<Self>,
    ) -> Div {
        let compact = size_class == WindowSizeClass::Compact;
        div()
            .h(ControlSize::Standard.height() + Space::Md.px())
            .flex_shrink_0()
            .flex()
            .items_center()
            .pl(platform_chrome_left_padding())
            .pr(Space::Lg.px())
            .gap(Space::Md.px())
            .bg(theme.surface_chrome)
            .border_b_1()
            .border_color(theme.outline_subtle)
            .child(Self::chrome_brand(theme, compact))
            .child(div().flex_1())
            .child(self.theme_toggle(theme, cx))
            .child(self.proxy_control(theme, size_class != WindowSizeClass::Wide, cx))
            .child(self.routing_control(theme, size_class != WindowSizeClass::Wide, cx))
    }

    fn chrome_brand(theme: Theme, compact: bool) -> Div {
        div()
            .w(if compact {
                LayoutMetric::CompactNavigation.px()
            } else {
                LayoutMetric::WideNavigation.px()
            })
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(Space::Sm.px())
            .child(
                div()
                    .size(ControlSize::Icon.min_pointer_target() - Space::Sm.px())
                    .flex_shrink_0()
                    .rounded(Radius::Control.px() - px(2.0))
                    .overflow_hidden()
                    .child(img(assets::BRAND_MARK_PATH).size_full()),
            )
            .when(!compact, |brand| {
                brand.child(
                    div()
                        .text_size(TextRole::SectionTitle.size())
                        .line_height(TextRole::SectionTitle.line_height())
                        .font_weight(TextRole::SectionTitle.weight())
                        .text_color(theme.text_primary)
                        .child(brand::PRODUCT_NAME),
                )
            })
    }

    fn theme_toggle(&self, _theme: Theme, cx: &mut Context<Self>) -> Button {
        let language = self.language();
        let label = if self.dark {
            language.localized(copy::app::LIGHT)
        } else {
            language.localized(copy::app::DARK)
        };
        action_button(
            "theme-toggle",
            label,
            ActionRole::Secondary,
            ControlSize::Compact,
        )
        .accessibility_label(label)
        .on_click(cx.listener(|this, _, window, cx| {
            this.dark = !this.dark;
            crate::theme::sync_component_theme(this.theme(), this.dark, Some(window), cx);
            this.sync_window_inputs(window, cx);
            let language = this.language();
            if this.dark {
                trace_ui(UiEvent::ThemeDarkSelected);
                language.localized(copy::app::DARK_THEME_ENABLED)
            } else {
                trace_ui(UiEvent::ThemeLightSelected);
                language.localized(copy::app::LIGHT_THEME_ENABLED)
            }
            .clone_into(&mut this.status);
            cx.notify();
        }))
    }

    fn proxy_control(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> AnyElement {
        let language = self.language();
        if compact {
            let next = self.proxy_mode.next();
            return action_button(
                "proxy-mode-cycle",
                compact_proxy_mode_label(language, self.proxy_mode, self.proxy_mode_busy),
                ActionRole::Secondary,
                ControlSize::Compact,
            )
            .accessibility_label(language.localized(copy::app::CHANGE_PROXY_MODE))
            .loading(self.proxy_mode_busy.is_some())
            .when(self.proxy_mode_busy.is_some(), gpui::Styled::cursor_default)
            .when(self.proxy_mode_busy.is_none(), |button| {
                button.icon(IconName::Redo2)
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.apply_proxy_mode(next, cx);
            }))
            .into_any_element();
        }

        let interactive = self.proxy_mode_busy.is_none();
        let mut modes = ButtonGroup::new("proxy-mode-options")
            .with_variant(ButtonVariant::Secondary)
            .with_size(gpui_component::Size::Small)
            .h_full();
        for mode in [ProxyMode::Off, ProxyMode::System, ProxyMode::Tun] {
            let selected = mode == self.proxy_mode;
            let pending = self.proxy_mode_busy == Some(mode);
            modes = modes.child(
                Button::new(format!("proxy-mode-{mode:?}"))
                    .map(crate::components::primary_button_interaction)
                    .debug_selector(move || format!("proxy-mode-{mode:?}"))
                    .accessibility_label(proxy_mode_label(language, mode))
                    .label(if pending {
                        match mode {
                            ProxyMode::Tun => language.localized(copy::app::PREPARING_TUN),
                            ProxyMode::System => language.localized(copy::app::ENABLING),
                            ProxyMode::Off => language.localized(copy::app::TURNING_OFF),
                        }
                    } else {
                        proxy_mode_label(language, mode)
                    })
                    .selected(selected)
                    .tab_stop(interactive)
                    .disabled(!interactive)
                    .h_full()
                    .px(Space::Md.px())
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .font_weight(if pending || selected {
                        TextRole::Label.weight()
                    } else {
                        TextRole::Metadata.weight()
                    })
                    .loading(pending)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.apply_proxy_mode(mode, cx);
                    })),
            );
        }
        div()
            .id("proxy-modes")
            .h(ControlSize::Compact.height())
            .p(px(2.0))
            .rounded(Radius::Control.px())
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_base)
            .flex()
            .items_center()
            .child(
                div()
                    .px(Space::Sm.px())
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_tertiary)
                    .child(language.localized(copy::app::PROXY)),
            )
            .child(modes)
            .into_any_element()
    }

    fn routing_control(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> AnyElement {
        let language = self.language();
        if compact {
            let next = match self.routing_mode {
                RoutingMode::Direct => RoutingMode::Global,
                RoutingMode::Global => RoutingMode::Rule,
                RoutingMode::Rule => RoutingMode::Direct,
            };
            let label = if self.routing_mode_busy.is_some() {
                language.localized(copy::app::SWITCHING)
            } else {
                match self.routing_mode {
                    RoutingMode::Direct => routing_mode_label(language, RoutingMode::Direct),
                    RoutingMode::Global => routing_mode_label(language, RoutingMode::Global),
                    RoutingMode::Rule => routing_mode_label(language, RoutingMode::Rule),
                }
            };
            return action_button(
                "routing-mode-cycle",
                label,
                ActionRole::Secondary,
                ControlSize::Compact,
            )
            .accessibility_label(language.localized(copy::app::CHANGE_ROUTING_MODE))
            .loading(self.routing_mode_busy.is_some())
            .when(
                self.routing_mode_busy.is_some(),
                gpui::Styled::cursor_default,
            )
            .when(self.routing_mode_busy.is_none(), |button| {
                button.icon(IconName::Redo2)
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.apply_routing_mode(next, cx);
            }))
            .into_any_element();
        }

        let mut modes = ButtonGroup::new("routing-mode-options")
            .with_variant(ButtonVariant::Secondary)
            .with_size(gpui_component::Size::Small)
            .h_full();
        for mode in [RoutingMode::Direct, RoutingMode::Global, RoutingMode::Rule] {
            let selected = mode == self.routing_mode;
            modes = modes.child(
                Button::new(format!("routing-mode-{mode:?}"))
                    .map(crate::components::primary_button_interaction)
                    .debug_selector(move || format!("routing-mode-{mode:?}"))
                    .accessibility_label(routing_mode_label(language, mode))
                    .label(if self.routing_mode_busy == Some(mode) {
                        language.localized(copy::app::SWITCHING)
                    } else {
                        routing_mode_label(language, mode)
                    })
                    .selected(selected)
                    .h_full()
                    .px(Space::Md.px())
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .font_weight(if selected {
                        TextRole::Label.weight()
                    } else {
                        TextRole::Metadata.weight()
                    })
                    .disabled(self.routing_mode_busy.is_some())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.apply_routing_mode(mode, cx);
                    })),
            );
        }
        div()
            .id("routing-modes")
            .h(ControlSize::Compact.height())
            .p(px(2.0))
            .rounded(Radius::Control.px())
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_base)
            .flex()
            .items_center()
            .child(
                div()
                    .px(Space::Sm.px())
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_tertiary)
                    .child(language.localized(copy::app::ROUTING)),
            )
            .child(modes)
            .into_any_element()
    }

    pub(super) fn navigation(
        &self,
        theme: Theme,
        size_class: WindowSizeClass,
        cx: &mut Context<Self>,
    ) -> Div {
        let language = self.language();
        let entries = [
            (
                language.message(Message::Nodes),
                IconName::Globe,
                PrimaryWorkspace::Nodes,
            ),
            (
                language.message(Message::RoutingRules),
                IconName::Map,
                PrimaryWorkspace::RoutingRules,
            ),
            (
                language.message(Message::NetworkActivity),
                IconName::ChartPie,
                PrimaryWorkspace::Activity,
            ),
            (
                language.message(Message::Logs),
                IconName::SquareTerminal,
                PrimaryWorkspace::Logs,
            ),
            (
                language.message(Message::Configuration),
                IconName::Settings,
                PrimaryWorkspace::Configuration,
            ),
        ];
        let show_labels = size_class == WindowSizeClass::Wide;
        let width = match size_class {
            WindowSizeClass::Wide => LayoutMetric::WideNavigation.px(),
            WindowSizeClass::Medium => LayoutMetric::MediumNavigation.px(),
            WindowSizeClass::Compact => LayoutMetric::CompactNavigation.px(),
        };
        div()
            .w(width)
            .h_full()
            .flex_shrink_0()
            .p(Space::Sm.px())
            .flex()
            .flex_col()
            .gap(Space::Xs.px())
            .bg(theme.surface_base)
            .children(entries.into_iter().map(|(label, icon, workspace)| {
                let selected = workspace == self.primary_workspace;
                div()
                    .id(format!("navigation-{workspace:?}"))
                    .debug_selector(move || format!("navigation-{workspace:?}"))
                    .role(Role::Button)
                    .aria_label(label)
                    .tab_stop(true)
                    .focusable()
                    .map(crate::components::primary_button_interaction)
                    .cursor_pointer()
                    .h(ControlSize::Standard.height())
                    .px(Space::Md.px())
                    .rounded(Radius::Row.px())
                    .flex()
                    .items_center()
                    .gap(Space::Sm.px())
                    .text_size(TextRole::Label.size())
                    .line_height(TextRole::Label.line_height())
                    .when(!show_labels, |row| {
                        row.justify_center().px_0().tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(label).build(window, cx)
                        })
                    })
                    .when(selected, |row| {
                        row.bg(theme.action_soft).text_color(theme.action_primary)
                    })
                    .when(!selected, |row| row.text_color(theme.text_secondary))
                    .hover(move |row| {
                        row.bg(if selected {
                            theme.action_soft
                        } else {
                            theme.surface_high
                        })
                        .text_color(theme.text_primary)
                    })
                    .border_1()
                    .border_color(gpui::rgba(0x0000_0000))
                    .focus_visible(move |row| row.border_color(theme.focus_ring))
                    .font_weight(if selected {
                        TextRole::Label.weight()
                    } else {
                        TextRole::Metadata.weight()
                    })
                    .child(gpui_component::Icon::new(icon).size(px(18.0)))
                    .when(show_labels, |row| row.child(label))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_primary_workspace(workspace, cx);
                    }))
            }))
    }

    fn open_primary_workspace(&mut self, workspace: PrimaryWorkspace, cx: &mut Context<Self>) {
        self.primary_workspace = workspace;
        let language = self.language();
        let (event, status) = match workspace {
            PrimaryWorkspace::Nodes => (
                UiEvent::WorkspaceNodesOpened,
                language.localized(copy::app::NODES_OPENED),
            ),
            PrimaryWorkspace::RoutingRules => (
                UiEvent::WorkspaceRoutingRulesOpened,
                language.localized(copy::app::ROUTING_RULES_OPENED),
            ),
            PrimaryWorkspace::Activity => (
                UiEvent::WorkspaceActivityOpened,
                language.localized(copy::app::NETWORK_ACTIVITY_OPENED),
            ),
            PrimaryWorkspace::Logs => (
                UiEvent::WorkspaceLogsOpened,
                language.localized(copy::app::LOGS_OPENED),
            ),
            PrimaryWorkspace::Configuration => (
                UiEvent::WorkspaceConfigurationOpened,
                language.localized(copy::app::CONFIGURATION_OPENED),
            ),
        };
        trace_ui(event);
        status.clone_into(&mut self.status);
        cx.notify();
    }

    pub(super) fn policy_section(
        &self,
        theme: Theme,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let language = self.language();
        let mut rows = div().flex_shrink_0().flex().flex_col().gap(Space::Sm.px());
        let policy_count;
        if self.catalog.is_some() {
            policy_count = self.policy_groups().count();
            for item in self.policy_groups() {
                rows = rows.child(self.policy_list_card(item, language, theme, cx));
            }
        } else {
            policy_count = self.managed_policies.groups.len();
            for policy in &self.managed_policies.groups {
                rows = rows.child(self.offline_policy_card(policy, language, theme, cx));
            }
        }
        if policy_count == 0 {
            rows = rows.child(self.empty_policy_content(language, theme));
        }
        div()
            .id("node-policies-section")
            .debug_selector(|| "node-policies-section".to_owned())
            .flex_shrink_0()
            .min_w(px(0.0))
            .border_t_1()
            .border_color(theme.outline_subtle)
            .px(if compact { px(12.0) } else { px(24.0) })
            .py(Space::Lg.px())
            .flex()
            .flex_col()
            .gap(Space::Lg.px())
            .child(page_heading(
                language.message(Message::PolicyGroups),
                format!(
                    "{} · {}",
                    language.count(CountNoun::PolicyGroup, policy_count),
                    language.localized(
                        copy::app::ROUTING_RULES_CHOOSE_POLICY_GROUPS_POLICIES_CHOOSE_EXITS,
                    ),
                ),
                Some(
                    Self::managed_policy_add_button("add-policy-group-header", language, theme, cx)
                        .into_any_element(),
                ),
                theme,
            ))
            .child(rows)
    }

    fn empty_policy_content(&self, language: Language, theme: Theme) -> Div {
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

    pub(super) fn offline_policy_card_view<'a>(
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

    fn offline_policy_card(
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

    fn policy_settings_button(
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

    pub(super) fn policy_list_card_view<'a>(
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

    fn policy_list_card(
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
        let selected_node = self.node_for_policy(view.item);
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
                current: node.id == selected_node.id,
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
            .nodes
            .iter()
            .find(|node| node.name == view.item.target)
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
        let target = copy::app::policy_identity(
            language,
            policy_kind_label(language, item.kind),
            Self::policy_candidate_display_name(&item.target),
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

    fn managed_policy_add_button(
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

    pub(super) fn connection_button(&self, _theme: Theme, cx: &mut Context<Self>) -> Button {
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

    fn node_row(
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

    pub(super) fn editable_policy_group_id(&self, policy_name: &str) -> Option<&str> {
        self.managed_policies
            .groups
            .iter()
            .find(|group| group.name == policy_name)
            .map(|group| group.id.as_str())
    }

    fn policy_candidate_table_header(language: Language, theme: Theme) -> Div {
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

    pub(super) fn live_status_issue(&self) -> Option<String> {
        if !matches!(self.controller, ControllerState::Connected { .. }) {
            return None;
        }
        let language = self.language();
        let is_issue = |phase: &LiveStreamPhase| {
            !matches!(
                phase,
                LiveStreamPhase::Waiting | LiveStreamPhase::Connecting | LiveStreamPhase::Live
            )
        };
        if self.live_status.activity == self.live_status.logs
            && is_issue(&self.live_status.activity)
        {
            return Some(copy::app::live_stream_phase(
                language,
                &self.live_status.activity,
            ));
        }
        let issues = [
            (Message::NetworkActivity, &self.live_status.activity),
            (Message::Logs, &self.live_status.logs),
        ]
        .into_iter()
        .filter(|(_, phase)| is_issue(phase))
        .map(|(source, phase)| {
            format!(
                "{}：{}",
                language.message(source),
                copy::app::live_stream_phase(language, phase)
            )
        })
        .collect::<Vec<_>>();
        (!issues.is_empty()).then(|| issues.join(" · "))
    }

    pub(super) fn status_bar(&self, theme: Theme, size_class: WindowSizeClass) -> StatusBar {
        let compact = size_class == WindowSizeClass::Compact;
        let language = self.language();
        let kernel_name = self.runtime.kind().display_name();
        let source = controller_status_label(&self.controller, kernel_name, language);
        let mut values = status_bar_values(&self.controller, language, theme);
        let issue = self.live_status_issue();
        if issue.is_some() {
            values.dot = theme.status_warning;
            values.tone = StatusTone::Warning;
        }
        let status = issue.clone().unwrap_or_else(|| self.status.clone());
        let tooltip = status.clone();

        let left = div()
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .min_w_0()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(Space::Sm.px())
                    .flex_none()
                    .child(div().size(px(8.0)).rounded_full().bg(values.dot))
                    .when(!compact || issue.is_none(), |identity| {
                        identity.child(status_badge(source, values.tone, theme))
                    }),
            )
            .when(issue.is_none(), |left| {
                left.child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(TextRole::Data.size())
                        .line_height(TextRole::Data.line_height())
                        .font_weight(TextRole::Data.weight())
                        .text_color(theme.text_secondary)
                        .child(values.endpoint),
                )
            })
            .child(
                div()
                    .id("runtime-status-message")
                    .role(Role::Status)
                    .aria_label(status.clone())
                    .min_w_0()
                    .truncate()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(if issue.is_some() {
                        theme.status_warning
                    } else {
                        theme.text_tertiary
                    })
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                    })
                    .child(status),
            );
        let right = div()
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .text_size(TextRole::Data.size())
            .line_height(TextRole::Data.line_height())
            .font_weight(TextRole::Data.weight())
            .text_color(theme.text_secondary)
            .when(!compact || issue.is_none(), |right| {
                right.child(values.download).child(values.upload)
            });

        StatusBar::new()
            .h(ControlSize::Standard.height())
            .flex_shrink_0()
            .py_0()
            .px(Space::Md.px())
            .border_t_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_chrome)
            .text_size(TextRole::Metadata.size())
            .line_height(TextRole::Metadata.line_height())
            .text_color(theme.text_secondary)
            .left(left)
            .right(right)
    }
}

fn platform_chrome_left_padding() -> gpui::Pixels {
    if cfg!(target_os = "macos") {
        // A transparent macOS title bar extends application content underneath the traffic
        // lights. Reserve their native control area before rendering the Manis brand.
        px(78.0)
    } else {
        Space::Lg.px()
    }
}
