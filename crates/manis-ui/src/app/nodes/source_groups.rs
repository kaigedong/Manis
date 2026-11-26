use super::{
    ActionRole, Button, ButtonVariant, ButtonVariants, Collapsible, Context, ControlSize,
    Disableable, Div, FluentBuilder, FontWeight, GroupBenchmarkNodeState, GroupBenchmarkState,
    InteractiveElement, IntoElement, Language, LoadedProviderNode, ManisApp, Message, NodeIdentity,
    NodeSourceGroup, NodeWorkspaceView, ParentElement, PrimaryWorkspace, Role,
    SourceGroupPresentation, Space, Stateful, StatefulInteractiveElement, Styled, TextRole, Theme,
    Toggled, WorkspaceNodeRowContext, action_button, copy, div, empty_state, parse_latency_ms, px,
    region_code, style_action_button,
};
use crate::components::{
    StatusTone, card_surface, latency_pill_with_tooltip, mono_text, region_code_badge, status_badge,
};
use crate::theme::MONO_FAMILY;
use gpui_component::IconName;

impl ManisApp {
    pub(super) fn source_group_list(
        &self,
        groups: &[NodeSourceGroup<'_>],
        view: NodeWorkspaceView,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut list = div().flex_shrink_0().flex().flex_col().gap(Space::Md.px());
        for group in groups {
            list = list.child(self.source_group(group, view, cx));
        }
        list
    }

    /// One subscription source: an opaque card whose header carries identity, quota and actions,
    /// and whose body is the node list.
    pub(super) fn source_group(
        &self,
        group: &NodeSourceGroup<'_>,
        view: NodeWorkspaceView,
        cx: &mut Context<Self>,
    ) -> Div {
        let NodeWorkspaceView {
            language, theme, ..
        } = view;
        let presentation = self.source_group_presentation(group, language);
        let header = Self::source_group_header(group, &presentation, view, cx);
        let content = if presentation.total_nodes == 0 {
            div()
                .px(Space::Lg.px())
                .pb(Space::Lg.px())
                .pt(Space::Sm.px())
                .border_t_1()
                .border_color(theme.outline_subtle)
                .text_size(TextRole::Body.size())
                .line_height(TextRole::Body.line_height())
                .text_color(theme.text_secondary)
                .child(language.localized(copy::nodes::NO_NODES_IN_THIS_SOURCE))
                .into_any_element()
        } else {
            self.source_group_table(group, presentation.benchmark, view, cx)
                .into_any_element()
        };

        card_surface(theme).flex_shrink_0().overflow_hidden().child(
            Collapsible::new()
                .open(!presentation.collapsed)
                .child(header)
                .content(content),
        )
    }

    pub(super) fn source_group_presentation(
        &self,
        group: &NodeSourceGroup<'_>,
        language: Language,
    ) -> SourceGroupPresentation<'_> {
        let total_nodes = group.node_count();
        let benchmark_key = Self::source_group_benchmark_key(&group.id);
        let benchmark = self
            .managed_policies
            .benchmarks
            .get(&benchmark_key)
            .unwrap_or(&GroupBenchmarkState::Idle);
        let detail = match benchmark {
            GroupBenchmarkState::Idle => group.detail.clone(),
            GroupBenchmarkState::Running { .. } => format!(
                "{} · {}",
                group.detail,
                language.localized(copy::nodes::TESTING)
            ),
            GroupBenchmarkState::Complete { summary, .. } => format!(
                "{} · {} {}",
                group.detail,
                language.localized(copy::nodes::TEST),
                Self::success_fraction_label(summary.succeeded, summary.total, language)
            ),
            GroupBenchmarkState::Failed { .. } => format!(
                "{} · {}",
                group.detail,
                language.localized(copy::nodes::TEST_FAILED)
            ),
        };
        SourceGroupPresentation {
            collapsed: self.node_workspace.is_group_collapsed(&group.id),
            benchmark_key,
            benchmark,
            detail,
            total_nodes,
        }
    }

    /// The source header: name, vehicle and node count on the left, refresh, latency test and
    /// disclosure on the right. The identity area and the disclosure both toggle the node list;
    /// refresh and the latency test are separate actions that must not collapse the card.
    pub(super) fn source_group_header(
        group: &NodeSourceGroup<'_>,
        presentation: &SourceGroupPresentation<'_>,
        view: NodeWorkspaceView,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let NodeWorkspaceView {
            compact,
            language,
            theme,
        } = view;
        let action = if presentation.collapsed {
            language.localized(copy::common::EXPAND)
        } else {
            language.localized(copy::common::COLLAPSE)
        };
        let trigger_group_id = group.id.clone();
        let trigger = Button::new(format!("source-group-header-{}", group.id))
            .map(crate::components::primary_button_interaction)
            .debug_selector({
                let id = group.id.clone();
                move || format!("source-group-header-{id}")
            })
            .accessibility_label(format!(
                "{} {} {}",
                action,
                language.localized(copy::nodes::NODE_SOURCE),
                group.name
            ))
            .with_variant(ButtonVariant::Text)
            .h_full()
            .flex_1()
            .px_0()
            .text_color(theme.text_primary)
            .child(Self::source_group_identity(
                group,
                presentation,
                language,
                theme,
            ))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.toggle_source_group(&trigger_group_id, cx);
            }));
        let benchmarking = presentation.benchmark.is_running();
        let benchmark_id = group.id.clone();
        let benchmark_name = group.name.clone();
        let delay_targets = group.delay_targets();
        div()
            .id(format!("source-group-surface-{}", group.id))
            .min_h(px(58.0))
            .px(if compact { px(12.0) } else { Space::Lg.px() })
            .py(Space::Md.px())
            .flex()
            .items_center()
            .gap(Space::Sm.px())
            .bg(theme.surface_elevated)
            .hover(move |header| header.bg(theme.surface_low))
            .child(trigger)
            .child(Self::source_group_refresh_button(group, view, cx))
            .child(Self::group_benchmark_icon(
                &presentation.benchmark_key,
                benchmarking,
                language,
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
            .child(
                div()
                    .id(format!("source-group-disclosure-{}", group.id))
                    .role(Role::Button)
                    .aria_label(format!("{action} {}", group.name))
                    .map(crate::components::primary_button_interaction)
                    .cursor_pointer()
                    .child(crate::components::disclosure_icon(
                        !presentation.collapsed,
                        theme,
                    ))
                    .on_click(cx.listener({
                        let disclosure_group_id = group.id.clone();
                        move |this, _, _, cx| {
                            this.toggle_source_group(&disclosure_group_id, cx);
                        }
                    })),
            )
    }

    /// Expanding a source is persisted immediately, so the page survives a restart as it was left.
    fn toggle_source_group(&mut self, id: &str, cx: &mut Context<Self>) {
        self.node_workspace.toggle_group(id);
        self.persist_node_workspace();
        self.language()
            .localized(copy::nodes::NODE_SOURCE_EXPANDED_STATE_UPDATED)
            .clone_into(&mut self.status);
        cx.notify();
    }

    fn source_group_identity(
        group: &NodeSourceGroup<'_>,
        presentation: &SourceGroupPresentation<'_>,
        language: Language,
        theme: Theme,
    ) -> Div {
        div()
            .min_w(px(0.0))
            .flex_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(Space::Sm.px())
                    .child(
                        div()
                            .min_w(px(0.0))
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(TextRole::Body.size())
                            .line_height(TextRole::Body.line_height())
                            .font_weight(TextRole::Label.weight())
                            .text_color(theme.text_primary)
                            .child(group.name.clone()),
                    )
                    .when_some(group.vehicle.clone(), |row, vehicle| {
                        row.child(status_badge(vehicle, StatusTone::Neutral, theme))
                    })
                    .child(status_badge(
                        Self::node_count_label(presentation.total_nodes, language),
                        StatusTone::Neutral,
                        theme,
                    )),
            )
            .child(
                div()
                    .mt(Space::Xs.px())
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_tertiary)
                    .child(presentation.detail.clone()),
            )
            .when_some(group.quota.as_ref(), |identity, quota| {
                // A quota meter only renders for a source that actually reports traffic limits.
                identity.child(
                    div()
                        .mt(Space::Xs.px())
                        .flex()
                        .items_center()
                        .gap(Space::Sm.px())
                        .child(crate::components::usage_meter(
                            quota.used_fraction,
                            110.0,
                            theme,
                        ))
                        .child(mono_text(quota.label.clone(), theme.text_tertiary)),
                )
            })
    }

    /// The refresh control for a source that can be re-fetched; kernel-provided sources cannot,
    /// so their button stays disabled instead of pretending to refresh.
    fn source_group_refresh_button(
        group: &NodeSourceGroup<'_>,
        view: NodeWorkspaceView,
        cx: &mut Context<Self>,
    ) -> Button {
        let NodeWorkspaceView {
            language, theme, ..
        } = view;
        let subscription_id = group.subscription_id.clone();
        let refreshable = subscription_id.is_some();
        style_action_button(
            Button::new(format!("source-refresh-{}", group.id))
                .debug_selector({
                    let id = group.id.clone();
                    move || format!("source-refresh-{id}")
                })
                .accessibility_label(language.message(Message::RefreshData)),
            ActionRole::Secondary,
            ControlSize::Icon,
        )
        .flex_shrink_0()
        .when(refreshable, |button| {
            button
                .icon(IconName::Redo2)
                .text_color(theme.text_secondary)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(id) = subscription_id.clone() {
                        this.refresh_imported_subscription(id, cx);
                    }
                }))
        })
        .when(!refreshable, |button| {
            button
                .icon(IconName::Redo2)
                .text_color(theme.text_tertiary)
                .disabled(true)
                .cursor_default()
        })
    }

    pub(super) fn source_group_table(
        &self,
        group: &NodeSourceGroup<'_>,
        benchmark: &GroupBenchmarkState,
        view: NodeWorkspaceView,
        cx: &mut Context<Self>,
    ) -> Div {
        let NodeWorkspaceView { theme, .. } = view;
        let mut table = div().border_t_1().border_color(theme.outline_subtle);
        for (provider_index, provider) in group.providers.iter().enumerate() {
            for (node_index, node) in provider.nodes.iter().enumerate() {
                table = table.child(self.workspace_node_row(
                    node,
                    benchmark,
                    WorkspaceNodeRowContext {
                        row_id: format!("node-row-{}-{provider_index}-{node_index}", group.id),
                        source_id: group.id.clone(),
                        view,
                    },
                    cx,
                ));
            }
        }
        for (node_index, node) in group.saved_nodes.iter().enumerate() {
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
                    view,
                },
                cx,
            ));
        }
        table
    }

    fn resolved_source_node_latency(
        &self,
        node: &LoadedProviderNode,
        benchmark: &GroupBenchmarkState,
    ) -> (GroupBenchmarkNodeState, Option<u64>) {
        let (shared_ms, shared_measured_at) = self.resolved_node_latency(
            &node.name,
            node.latency_label.as_deref().and_then(parse_latency_ms),
        );
        let latency = if benchmark.is_running() {
            benchmark.node_state(&node.name)
        } else if let Some(ms) = shared_ms.and_then(|ms| u16::try_from(ms).ok()) {
            GroupBenchmarkNodeState::Measured(ms)
        } else {
            benchmark.node_state(&node.name)
        };
        let fallback_ts = match benchmark {
            GroupBenchmarkState::Complete {
                finished_at_epoch_secs,
                ..
            }
            | GroupBenchmarkState::Failed {
                finished_at_epoch_secs,
                ..
            } if *finished_at_epoch_secs > 0 => Some(*finished_at_epoch_secs),
            _ => None,
        };
        (latency, shared_measured_at.or(fallback_ts))
    }

    pub(super) fn workspace_node_row(
        &self,
        node: &LoadedProviderNode,
        benchmark: &GroupBenchmarkState,
        context: WorkspaceNodeRowContext,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let WorkspaceNodeRowContext {
            row_id,
            source_id,
            view,
        } = context;
        let NodeWorkspaceView {
            compact,
            language,
            theme,
        } = view;
        let (latency, latency_measured_at) = self.resolved_source_node_latency(node, benchmark);
        let idle_latency = node.latency_label.as_deref().unwrap_or("—");
        let exit_ip = self.exit_ip_label(&node.name).map(str::to_owned);
        let spinner_id = format!("{row_id}-latency");
        let global_identity = NodeIdentity::new(&source_id, &node.name).ok();
        let global_runtime_selected = self.runtime_global_target() == Some(node.name.as_str());
        let global_selected = global_identity.as_ref().is_some_and(|identity| {
            self.global_target_identity()
                .map_or(global_runtime_selected, |selected| selected == identity)
        });
        let selection_locked = self.global_selection_busy.is_some();
        let selected_name = node.name.clone();
        let row_body = Self::node_row_columns(
            node,
            latency,
            latency_measured_at,
            idle_latency,
            exit_ip,
            &spinner_id,
            global_selected,
            language,
            theme,
            compact,
        );
        let selector = row_id.clone();
        div()
            .id(row_id)
            .debug_selector(move || selector.clone())
            .min_h(if compact { px(56.0) } else { px(44.0) })
            .px(if compact { px(12.0) } else { Space::Lg.px() })
            .py(Space::Sm.px())
            .flex()
            .items_center()
            .gap(Space::Sm.px())
            .border_t_1()
            .border_color(theme.outline_subtle)
            .bg(if global_selected {
                theme.action_soft
            } else {
                theme.surface_elevated
            })
            .hover(move |row| {
                row.bg(if global_selected {
                    theme.action_soft
                } else {
                    theme.surface_low
                })
            })
            .child(row_body)
            .when_some(global_identity, |row, selected_identity| {
                row.role(Role::RadioButton)
                    .aria_label(format!(
                        "{} {selected_name} {}",
                        language.localized(copy::nodes::SELECT),
                        language.localized(copy::nodes::AS_GLOBAL_EXIT)
                    ))
                    .aria_toggled(if global_selected {
                        Toggled::True
                    } else {
                        Toggled::False
                    })
                    .tab_stop(!selection_locked)
                    .focusable()
                    .map(crate::components::primary_button_interaction)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !selection_locked {
                            this.select_global_node(selected_identity.clone(), cx);
                        }
                    }))
            })
    }

    fn node_latency_cell(
        latency: GroupBenchmarkNodeState,
        latency_measured_at: Option<u64>,
        idle_latency: &str,
        spinner_id: &str,
        language: Language,
        theme: Theme,
        compact: bool,
    ) -> Div {
        let latency_ms = match latency {
            GroupBenchmarkNodeState::Measured(milliseconds) => Some(u64::from(milliseconds)),
            _ => parse_latency_ms(idle_latency),
        };
        let cell = div()
            .w(px(if compact { 76.0 } else { 86.0 }))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_end();
        match latency {
            GroupBenchmarkNodeState::Pending => cell.child(Self::benchmark_latency_content(
                latency,
                idle_latency.to_owned(),
                spinner_id,
                language,
                theme,
            )),
            GroupBenchmarkNodeState::Failed => cell.child(
                Self::benchmark_latency_content(
                    latency,
                    idle_latency.to_owned(),
                    spinner_id,
                    language,
                    theme,
                )
                .id(format!("{spinner_id}-failed"))
                .tooltip(move |window, cx| {
                    let text = copy::app::latency_tooltip(
                        language,
                        None,
                        latency_measured_at,
                        crate::app::benchmark_timestamp(),
                    );
                    gpui_component::tooltip::Tooltip::new(text).build(window, cx)
                }),
            ),
            GroupBenchmarkNodeState::Idle | GroupBenchmarkNodeState::Measured(_) => {
                cell.child(latency_pill_with_tooltip(
                    format!("{spinner_id}-pill"),
                    latency_ms,
                    latency_measured_at,
                    language,
                    theme,
                ))
            }
        }
    }

    /// The four-column node row: `[region] name` | protocol | exit IP | signal bars and latency.
    /// A compact window drops the IP column so the name keeps its width.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::app) fn node_row_columns(
        node: &LoadedProviderNode,
        latency: GroupBenchmarkNodeState,
        latency_measured_at: Option<u64>,
        idle_latency: &str,
        exit_ip: Option<String>,
        spinner_id: &str,
        selected: bool,
        language: Language,
        theme: Theme,
        compact: bool,
    ) -> Div {
        div()
            .min_w(px(0.0))
            .flex_1()
            .flex()
            .items_center()
            .gap(Space::Sm.px())
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(region_code_badge(region_code(&node.name), theme))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(TextRole::Body.size())
                            .line_height(TextRole::Body.line_height())
                            .font_weight(if selected {
                                FontWeight::SEMIBOLD
                            } else {
                                TextRole::Label.weight()
                            })
                            .text_color(if selected {
                                theme.action_primary
                            } else {
                                theme.text_primary
                            })
                            .child(node.name.clone()),
                    )
                    .when(selected, |name| {
                        name.child(status_badge(
                            language.localized(copy::app::CURRENT),
                            StatusTone::Neutral,
                            theme,
                        ))
                    }),
            )
            .child(
                div()
                    .w(px(if compact { 82.0 } else { 96.0 }))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .child(status_badge(
                        Self::protocol_label(node, language),
                        StatusTone::Neutral,
                        theme,
                    )),
            )
            .when(!compact, |row| {
                row.child(
                    div()
                        .w(px(124.0))
                        .flex_shrink_0()
                        .overflow_x_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .font_family(MONO_FAMILY)
                        .child(Self::exit_ip_content(exit_ip, theme)),
                )
            })
            .child(Self::node_latency_cell(
                latency,
                latency_measured_at,
                idle_latency,
                spinner_id,
                language,
                theme,
                compact,
            ))
    }

    /// The protocol pill label: a node with no reported protocol still names its state.
    fn protocol_label(node: &LoadedProviderNode, language: Language) -> String {
        if node.protocol.trim().is_empty() {
            language.localized(copy::app::UNKNOWN_TYPE).to_owned()
        } else {
            node.protocol.clone()
        }
    }

    pub(in crate::app) fn node_empty_state(
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
        .accessibility_label(
            language.localized(copy::nodes::GO_TO_CONFIGURATION_TO_IMPORT_A_SUBSCRIPTION),
        )
        .w(px(180.0))
        .on_click(cx.listener(|this, _, _, cx| {
            this.primary_workspace = PrimaryWorkspace::Configuration;
            this.language()
                .localized(copy::nodes::SUBSCRIPTION_SOURCE_CONFIGURATION_OPENED)
                .clone_into(&mut this.status);
            this.scroll_to_configuration_section(
                crate::app::ConfigurationSection::ProxySources,
                cx,
            );
        }))
        .into_any_element();

        div()
            .py(if compact {
                Space::Md.px()
            } else {
                Space::Lg.px()
            })
            .flex()
            .items_center()
            .child(empty_state(
                language.message(Message::NoNodes),
                language
                    .localized(copy::nodes::IMPORT_A_SUBSCRIPTION_OR_ADD_A_SINGLE_NODE_NODES_WILL),
                Some(action),
                theme,
            ))
    }

    pub(in crate::app) fn node_message_panel(
        title: &'static str,
        copy: &'static str,
        theme: Theme,
    ) -> Div {
        empty_state(title, copy, None, theme)
    }
}
