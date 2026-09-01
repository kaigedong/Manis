use super::{
    ActionRole, Button, ButtonVariant, ButtonVariants, Collapsible, Context, ControlSize, Div,
    FluentBuilder, FontWeight, GroupBenchmarkNodeState, GroupBenchmarkState, InteractiveElement,
    IntoElement, Language, LoadedProviderNode, ManisApp, Message, NodeIdentity, NodeSourceGroup,
    NodeWorkspaceView, ParentElement, PrimaryWorkspace, Radius, Role, SourceGroupPresentation,
    Space, Stateful, StatefulInteractiveElement, Styled, TextRole, Theme, Toggled,
    WorkspaceNodeRowContext, action_button, copy, div, empty_state, px,
};

impl ManisApp {
    pub(super) fn source_group_list(
        &self,
        groups: &[NodeSourceGroup<'_>],
        compact: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut list = div().flex_shrink_0().flex().flex_col().gap_3();
        for group in groups {
            list = list.child(self.source_group(group, compact, language, theme, cx));
        }
        list
    }

    pub(super) fn source_group(
        &self,
        group: &NodeSourceGroup<'_>,
        compact: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let presentation = self.source_group_presentation(group, language);
        let header = Self::source_group_header(group, &presentation, compact, language, theme, cx);

        let content = if presentation.total_nodes == 0 {
            div()
                .px_4()
                .py_3()
                .border_t_1()
                .border_color(theme.outline_subtle)
                .bg(theme.surface_base)
                .text_size(TextRole::Body.size())
                .line_height(TextRole::Body.line_height())
                .text_color(theme.text_secondary)
                .child(language.localized(copy::nodes::NO_NODES_IN_THIS_SOURCE))
                .into_any_element()
        } else {
            self.source_group_table(
                group,
                presentation.benchmark,
                NodeWorkspaceView {
                    compact,
                    language,
                    theme,
                },
                cx,
            )
            .into_any_element()
        };

        div()
            .flex_shrink_0()
            .rounded(Radius::Pane.px())
            .border_1()
            .border_color(theme.outline_subtle)
            .overflow_hidden()
            .child(
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
        let total_nodes = group
            .providers
            .iter()
            .map(|provider| provider.nodes.len())
            .sum::<usize>()
            + group.saved_nodes.len();
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

    pub(super) fn source_group_header(
        group: &NodeSourceGroup<'_>,
        presentation: &SourceGroupPresentation<'_>,
        compact: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
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
            .child(Self::source_group_header_content(
                group,
                presentation,
                language,
                theme,
            ))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.node_workspace.toggle_group(&trigger_group_id);
                this.persist_node_workspace();
                this.language()
                    .localized(copy::nodes::NODE_SOURCE_EXPANDED_STATE_UPDATED)
                    .clone_into(&mut this.status);
                cx.notify();
            }));
        let benchmarking = presentation.benchmark.is_running();
        let benchmark_id = group.id.clone();
        let benchmark_name = group.name.clone();
        let delay_targets = group.delay_targets();
        div()
            .id(format!("source-group-surface-{}", group.id))
            .min_h(px(58.0))
            .px(if compact { px(12.0) } else { px(16.0) })
            .py_3()
            .flex()
            .items_center()
            .gap_3()
            .rounded_tl(Radius::Pane.px())
            .rounded_tr(Radius::Pane.px())
            .when(presentation.collapsed, |header| {
                header
                    .rounded_bl(Radius::Pane.px())
                    .rounded_br(Radius::Pane.px())
            })
            .bg(theme.surface_low)
            .hover(move |header| header.bg(theme.action_soft))
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
            .child(trigger)
    }

    pub(super) fn source_group_header_content(
        group: &NodeSourceGroup<'_>,
        presentation: &SourceGroupPresentation<'_>,
        language: Language,
        theme: Theme,
    ) -> Div {
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
                            .child(presentation.detail.clone()),
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
                            .child(Self::node_count_label(presentation.total_nodes, language)),
                    )
                    .child(crate::components::disclosure_icon(
                        !presentation.collapsed,
                        theme,
                    )),
            )
    }

    pub(super) fn source_group_table(
        &self,
        group: &NodeSourceGroup<'_>,
        benchmark: &GroupBenchmarkState,
        view: NodeWorkspaceView,
        cx: &mut Context<Self>,
    ) -> Div {
        let NodeWorkspaceView {
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

    pub(in crate::app) fn node_table_header(language: Language, theme: Theme) -> Div {
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
            .child(div().flex_1().child(language.localized(copy::nodes::NODE)))
            .child(
                div()
                    .w(px(100.0))
                    .child(language.localized(copy::nodes::PROTOCOL)),
            )
            .child(
                div()
                    .w(px(72.0))
                    .text_align(gpui::TextAlign::Right)
                    .child(language.localized(copy::common::LATENCY)),
            )
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
        let selector = row_id.clone();
        div()
            .id(row_id)
            .debug_selector(move || selector.clone())
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
                theme.surface_base
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

    pub(in crate::app) fn compact_node_row_content(
        node: &LoadedProviderNode,
        latency: GroupBenchmarkNodeState,
        idle_latency: String,
        spinner_id: &str,
        language: Language,
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
                                language,
                                theme,
                            )),
                    ),
            )
    }

    pub(in crate::app) fn wide_node_row_content(
        node: &LoadedProviderNode,
        latency: GroupBenchmarkNodeState,
        idle_latency: String,
        spinner_id: &str,
        language: Language,
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
                        language,
                        theme,
                    )),
            )
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
                    .localized(copy::nodes::IMPORT_A_SUBSCRIPTION_OR_ADD_A_VLESS_NODE_NODES_WILL),
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
