impl ManisApp {
    fn chrome(&self, theme: Theme, size_class: WindowSizeClass, cx: &mut Context<Self>) -> Div {
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

    fn theme_toggle(&self, theme: Theme, cx: &mut Context<Self>) -> Button {
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
        .border_color(theme.outline_subtle)
        .bg(theme.surface_high)
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
            return Button::new("proxy-mode-cycle")
                .accessibility_label(language.localized(copy::app::CHANGE_PROXY_MODE))
                .label(compact_proxy_mode_label(
                    language,
                    self.proxy_mode,
                    self.proxy_mode_busy,
                ))
                .with_variant(ButtonVariant::Default)
                .with_size(ControlSize::Compact.component_size())
                .h(ControlSize::Compact.height())
                .px(Space::Md.px())
                .border_color(theme.outline_subtle)
                .bg(theme.surface_high)
                .text_color(theme.text_primary)
                .text_size(TextRole::Label.size())
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
            .with_variant(ButtonVariant::Ghost)
            .with_size(ControlSize::Icon.component_size())
            .h_full();
        for mode in [ProxyMode::Off, ProxyMode::System, ProxyMode::Tun] {
            let selected = mode == self.proxy_mode;
            let pending = self.proxy_mode_busy == Some(mode);
            modes = modes.child(
                Button::new(format!("proxy-mode-{mode:?}"))
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
                    .bg(if pending || selected {
                        theme.action_primary
                    } else {
                        theme.surface_high
                    })
                    .text_color(if pending || selected {
                        theme.action_on_primary
                    } else {
                        theme.text_secondary
                    })
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
            .bg(theme.surface_high)
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
            return Button::new("routing-mode-cycle")
                .accessibility_label(language.localized(copy::app::CHANGE_ROUTING_MODE))
                .label(label)
                .with_variant(ButtonVariant::Default)
                .with_size(ControlSize::Compact.component_size())
                .h(ControlSize::Compact.height())
                .px(Space::Md.px())
                .border_color(theme.outline_subtle)
                .bg(theme.surface_high)
                .text_color(theme.text_primary)
                .text_size(TextRole::Label.size())
                .when(self.routing_mode_busy.is_none(), |button| {
                    button.icon(IconName::Redo2)
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.apply_routing_mode(next, cx);
                }))
                .into_any_element();
        }

        let mut modes = ButtonGroup::new("routing-mode-options")
            .with_variant(ButtonVariant::Ghost)
            .with_size(ControlSize::Icon.component_size())
            .h_full();
        for mode in [RoutingMode::Direct, RoutingMode::Global, RoutingMode::Rule] {
            let selected = mode == self.routing_mode;
            modes = modes.child(
                Button::new(format!("routing-mode-{mode:?}"))
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
                    .bg(if selected {
                        theme.action_primary
                    } else {
                        theme.surface_high
                    })
                    .text_color(if selected {
                        theme.action_on_primary
                    } else {
                        theme.text_secondary
                    })
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
            .bg(theme.surface_high)
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

    fn navigation(&self, theme: Theme, size_class: WindowSizeClass, cx: &mut Context<Self>) -> Div {
        let language = self.language();
        let entries = [
            (
                language.message(Message::Nodes),
                language.message(Message::Nodes),
                PrimaryWorkspace::Nodes,
            ),
            (
                language.message(Message::PolicyGroups),
                language.localized(copy::app::GROUPS),
                PrimaryWorkspace::Policies,
            ),
            (
                language.message(Message::RoutingRules),
                language.localized(copy::app::RULES),
                PrimaryWorkspace::RoutingRules,
            ),
            (
                language.message(Message::NetworkActivity),
                language.localized(copy::app::ACTIVITY),
                PrimaryWorkspace::Activity,
            ),
            (
                language.message(Message::Logs),
                language.message(Message::Logs),
                PrimaryWorkspace::Logs,
            ),
            (
                language.message(Message::Configuration),
                language.message(Message::Configuration),
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
            .bg(theme.surface_low)
            .border_r_1()
            .border_color(theme.outline_subtle)
            .children(entries.into_iter().map(|(label, short_label, workspace)| {
                let selected = workspace == self.primary_workspace;
                div()
                    .id(format!("navigation-{workspace:?}"))
                    .role(Role::Button)
                    .aria_label(label)
                    .tab_stop(true)
                    .focusable()
                    .cursor_pointer()
                    .h(ControlSize::Standard.height())
                    .px(Space::Md.px())
                    .rounded(Radius::Row.px())
                    .flex()
                    .items_center()
                    .text_size(TextRole::Label.size())
                    .line_height(TextRole::Label.line_height())
                    .when(!show_labels, |row| {
                        row.justify_center()
                            .px_0()
                            .text_size(TextRole::Metadata.size())
                            .line_height(TextRole::Metadata.line_height())
                    })
                    .when(selected, |row| {
                        row.bg(theme.action_soft).text_color(theme.action_primary)
                    })
                    .when(!selected, |row| row.text_color(theme.text_secondary))
                    .font_weight(if selected {
                        TextRole::Label.weight()
                    } else {
                        TextRole::Metadata.weight()
                    })
                    .child(if show_labels { label } else { short_label })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_primary_workspace(workspace, cx);
                    }))
            }))
    }

    fn open_primary_workspace(&mut self, workspace: PrimaryWorkspace, cx: &mut Context<Self>) {
        self.primary_workspace = workspace;
        let language = self.language();
        let (event, status) = match workspace {
            PrimaryWorkspace::Policies => (
                UiEvent::WorkspacePoliciesOpened,
                language.localized(copy::app::POLICY_GROUPS_OPENED),
            ),
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

    fn empty_policy_workspace(&self, theme: Theme, cx: &mut Context<Self>) -> Div {
        let language = self.language();
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

        let body = div()
            .id("offline-policy-scroll")
            .flex_1()
            .overflow_y_scroll()
            .p(Space::Xl.px());
        let body = if self.managed_policies.groups.is_empty() {
            body.child(
                div()
                    .max_w(px(620.0))
                    .child(empty_state(title, description, None, theme)),
            )
        } else {
            let rows = self.managed_policies.groups.clone().into_iter().fold(
                div().flex().flex_col().gap(Space::Sm.px()),
                |rows, policy| rows.child(self.offline_policy_card(policy, language, theme, cx)),
            );
            body.child(rows)
        };

        div()
            .h_full()
            .flex_1()
            .min_w(px(0.0))
            .bg(theme.surface_low)
            .flex()
            .flex_col()
            .child(
                div()
                    .p(Space::Lg.px())
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .bg(theme.surface_high)
                    .child(page_heading(
                        language.message(Message::PolicyGroups),
                        language.localized(
                            copy::app::ROUTING_RULES_CHOOSE_POLICY_GROUPS_POLICIES_CHOOSE_EXITS,
                        ),
                        Some(
                            div()
                                .flex()
                                .items_center()
                                .gap(Space::Sm.px())
                                .child(Self::managed_policy_add_button(
                                    "add-policy-group-empty",
                                    language,
                                    theme,
                                    cx,
                                ))
                                .child(self.connection_button(theme, cx))
                                .into_any_element(),
                        ),
                        theme,
                    )),
            )
            .child(body)
    }

    fn offline_policy_card_view(&self, policy: ManagedPolicyGroup) -> OfflinePolicyCardView {
        let policy_group_id = PolicyGroupId::new(policy.id.clone());
        OfflinePolicyCardView {
            candidates: self.managed_policy_candidate_names(&policy),
            selected_name: self
                .managed_policies
                .node_selections
                .policy_target(&policy.name)
                .map(str::to_owned),
            expanded: self.expanded_policy_group.as_ref() == Some(&policy_group_id),
            benchmarking: self.managed_policies.pending_benchmark_name.as_deref()
                == Some(policy.name.as_str()),
            policy,
        }
    }

    fn offline_policy_header(
        view: &OfflinePolicyCardView,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let candidate_count = view.candidates.len();
        let benchmarkable = candidate_count > 0;
        let benchmarking = view.benchmarking;
        let benchmark_name = view.policy.name.clone();
        let toggle_id = PolicyGroupId::new(view.policy.id.clone());
        let action = if view.expanded {
            language.localized(copy::common::COLLAPSE)
        } else {
            language.localized(copy::common::EXPAND)
        };
        div()
            .id(format!("saved-policy-header-{}", view.policy.id))
            .role(Role::Button)
            .aria_label(format!("{action} {}", view.policy.name))
            .tab_stop(true)
            .focusable()
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
            .child(Self::offline_policy_summary(
                candidate_count,
                action,
                language,
                theme,
            ))
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.expanded_policy_group.as_ref() == Some(&toggle_id) {
                    this.expanded_policy_group = None;
                } else {
                    this.expanded_policy_group = Some(toggle_id.clone());
                }
                cx.notify();
            }))
    }

    fn offline_policy_identity(
        view: &OfflinePolicyCardView,
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
        action: &'static str,
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
            .child(
                div()
                    .min_w(px(36.0))
                    .text_size(TextRole::Label.size())
                    .line_height(TextRole::Label.line_height())
                    .font_weight(TextRole::Label.weight())
                    .text_color(theme.action_primary)
                    .child(action),
            )
    }

    fn offline_policy_card(
        &self,
        policy: ManagedPolicyGroup,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let view = self.offline_policy_card_view(policy);
        let mut card = div()
            .rounded(Radius::Pane.px())
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
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
            for candidate in &view.candidates {
                card = card.child(Self::saved_policy_candidate_row(
                    candidate.clone(),
                    view.selected_name.as_deref() == Some(candidate.as_str()),
                    theme,
                ));
            }
        }
        card.child(Self::offline_policy_actions(
            &view.policy,
            language,
            theme,
            cx,
        ))
    }

    fn offline_policy_actions(
        policy: &ManagedPolicyGroup,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let edit_id = policy.id.clone();
        let remove_id = policy.id.clone();
        div()
            .px_4()
            .py(Space::Md.px())
            .border_t_1()
            .border_color(theme.outline_subtle)
            .flex()
            .justify_end()
            .gap(Space::Sm.px())
            .child(
                action_button(
                    format!("edit-offline-policy-{}", policy.id),
                    language.localized(copy::app::EDIT),
                    ActionRole::Secondary,
                    ControlSize::Compact,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.start_managed_policy_edit(&edit_id, cx);
                })),
            )
            .child(
                action_button(
                    format!("remove-offline-policy-{}", policy.id),
                    language.message(Message::Delete),
                    ActionRole::Danger,
                    ControlSize::Compact,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.remove_managed_policy(&remove_id, cx);
                })),
            )
    }

    fn saved_policy_candidate_row(name: String, current: bool, theme: Theme) -> Div {
        div()
            .min_h(px(48.0))
            .px(Space::Lg.px())
            .border_t_1()
            .border_color(theme.outline_subtle)
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .bg(if current {
                theme.action_soft
            } else {
                theme.surface_high
            })
            .child(
                div()
                    .size(px(10.0))
                    .rounded_full()
                    .border_1()
                    .border_color(if current {
                        theme.action_primary
                    } else {
                        theme.outline_strong
                    })
                    .when(current, |dot| dot.bg(theme.action_primary)),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Body.size())
                    .line_height(TextRole::Body.line_height())
                    .text_color(theme.text_primary)
                    .child(name),
            )
    }

    fn policy_list(&self, theme: Theme, width: Option<f32>, cx: &mut Context<Self>) -> Div {
        let language = self.language();
        let compact = width.is_none();
        let policy_count = self.policy_groups().count();
        let rows = self.policy_groups().cloned().fold(
            div()
                .id("policy-scroll")
                .flex_1()
                .overflow_y_scroll()
                .p(Space::Md.px())
                .flex()
                .flex_col()
                .gap(Space::Sm.px()),
            |rows, item| rows.child(self.policy_list_card(item, compact, language, theme, cx)),
        );

        div()
            .when_some(width, |list, width| list.w(px(width)).flex_shrink_0())
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.surface_low)
            .border_r_1()
            .border_color(theme.outline_subtle)
            .child(
                div()
                    .p(Space::Lg.px())
                    .border_b_1()
                    .border_color(theme.outline_subtle)
                    .child(page_heading(
                        language.message(Message::PolicyGroups),
                        format!(
                            "{} · {}",
                            language.count(CountNoun::PolicyGroup, policy_count),
                            language.localized(
                                copy::app::RULES_TARGET_A_POLICY_OPEN_ONE_TO_CONFIGURE_ITS_EXIT
                            )
                        ),
                        Some(
                            div()
                                .flex()
                                .items_center()
                                .gap(Space::Sm.px())
                                .child(Self::managed_policy_add_button(
                                    "add-policy-group-header",
                                    language,
                                    theme,
                                    cx,
                                ))
                                .child(self.connection_button(theme, cx))
                                .into_any_element(),
                        ),
                        theme,
                    )),
            )
            .child(rows)
    }

    fn policy_list_card_view(&self, item: PolicyGroup) -> PolicyListCardView {
        let benchmark_key = Self::policy_group_benchmark_key(&item.id);
        PolicyListCardView {
            selected: self.workspace.selected_group.as_ref() == Some(&item.id),
            expanded: self.expanded_policy_group.as_ref() == Some(&item.id),
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
        item: PolicyGroup,
        compact: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let view = self.policy_list_card_view(item);
        let mut card = div()
            .rounded(Radius::Pane.px())
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_high)
            .overflow_hidden()
            .child(Self::policy_list_header(
                &view, compact, language, theme, cx,
            ));
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
        for node in view.item.nodes.iter().cloned() {
            let benchmark_state = self
                .managed_policies
                .benchmarks
                .get(&view.benchmark_key)
                .map_or(GroupBenchmarkNodeState::Idle, |state| {
                    state.node_state(&node.name)
                });
            let current = node.name == view.item.target;
            card = card.child(Self::policy_list_candidate_row(
                node,
                PolicyCandidateRowContext {
                    policy_id: view.item.id.clone(),
                    policy_name: view.item.name.clone(),
                    current,
                    manually_selectable: view.item.kind.allows_manual_selection(),
                    selection_busy: self.policy_selection_busy.is_some(),
                    benchmark_state,
                    language,
                    theme,
                },
                cx,
            ));
        }
        card
    }

    fn policy_list_header(
        view: &PolicyListCardView,
        compact: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let benchmarkable = Self::policy_group_benchmarkable(&view.item);
        let benchmarking = view.benchmarking;
        let benchmark_id = view.item.id.clone();
        let item_id = view.item.id.clone();
        let item_name = view.item.name.clone();
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
            .role(Role::Button)
            .aria_label(format!("{action} {}", view.item.name))
            .tab_stop(true)
            .focusable()
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
            .bg(if view.selected || view.expanded {
                theme.surface_high
            } else {
                theme.surface_low
            })
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
            .child(Self::policy_list_identity(&view.item, language, theme))
            .child(Self::policy_list_summary(
                view.item.nodes.len(),
                action,
                language,
                theme,
            ))
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.expanded_policy_group.as_ref() == Some(&item_id) {
                    this.expanded_policy_group = None;
                } else {
                    this.expanded_policy_group = Some(item_id.clone());
                }
                this.policy_detail_tab = PolicyDetailTab::Nodes;
                this.workspace.select_group(item_id.clone());
                if compact {
                    this.workspace.navigate_back();
                }
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
            &item.target,
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
        action: &'static str,
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
            .child(
                div()
                    .min_w(px(36.0))
                    .text_size(TextRole::Label.size())
                    .line_height(TextRole::Label.line_height())
                    .font_weight(TextRole::Label.weight())
                    .text_color(theme.action_primary)
                    .child(action),
            )
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

    fn policy_list_candidate_row(
        node: PolicyNode,
        row_context: PolicyCandidateRowContext,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let PolicyCandidateRowContext {
            policy_id,
            policy_name,
            current,
            manually_selectable,
            selection_busy,
            benchmark_state,
            language,
            theme,
        } = row_context;
        let node_id = node.id.clone();
        let node_name = node.name.clone();
        let idle_latency = node
            .latency_ms
            .map_or_else(|| "—".to_owned(), |latency| format!("{latency} ms"));
        div()
            .id(format!("policy-list-node-{}", node.id.as_str()))
            .tab_stop(manually_selectable && !selection_busy)
            .min_h(px(48.0))
            .px(Space::Lg.px())
            .border_t_1()
            .border_color(theme.outline_subtle)
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .bg(if current {
                theme.action_soft
            } else {
                theme.surface_high
            })
            .child(
                div()
                    .size(px(10.0))
                    .rounded_full()
                    .border_1()
                    .border_color(if current {
                        theme.action_primary
                    } else {
                        theme.outline_strong
                    })
                    .when(current, |dot| dot.bg(theme.action_primary)),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Body.size())
                    .line_height(TextRole::Body.line_height())
                    .text_color(if manually_selectable {
                        theme.text_primary
                    } else {
                        theme.text_secondary
                    })
                    .child(node.name),
            )
            .child(Self::benchmark_latency_content(
                benchmark_state,
                idle_latency,
                &format!("policy-list-node-{}-spinner", node.id.as_str()),
                language,
                theme,
            ))
            .when(manually_selectable, |row| {
                row.role(Role::RadioButton)
                    .aria_toggled(if current {
                        Toggled::True
                    } else {
                        Toggled::False
                    })
                    .focusable()
                    .when(!selection_busy, gpui::Styled::cursor_pointer)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !selection_busy {
                            this.select_policy_node(
                                PolicySelectionRequest {
                                    group_id: policy_id.clone(),
                                    group_name: policy_name.clone(),
                                    node_id: node_id.clone(),
                                    node_name: node_name.clone(),
                                },
                                cx,
                            );
                        }
                    }))
            })
    }

    fn managed_policy_add_button(
        id: &'static str,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Button {
        action_button(
            id,
            language.message(Message::AddPolicyGroup),
            ActionRole::Primary,
            ControlSize::Compact,
        )
        .accessibility_label(language.message(Message::AddPolicyGroup))
        .cursor_pointer()
        .bg(theme.action_primary)
        .text_color(theme.action_on_primary)
        .font_weight(FontWeight::SEMIBOLD)
        .on_click(cx.listener(|this, _, _, cx| {
            this.workspace.compact_navigation = CompactNavigation::GroupDetail;
            this.start_managed_policy_create(cx);
        }))
    }

    fn connection_button(&self, theme: Theme, cx: &mut Context<Self>) -> Button {
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
        .px_3()
        .cursor_pointer()
        .border_color(if connecting {
            theme.outline_subtle
        } else {
            theme.action_primary
        })
        .bg(if connecting {
            theme.surface_high
        } else {
            theme.action_soft
        })
        .text_color(if connecting {
            theme.text_tertiary
        } else {
            theme.action_primary
        })
        .on_click(cx.listener(|this, _, _, cx| this.connect_mihomo(cx)))
    }

    fn node_row(
        item: PolicyNode,
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
        let PolicySelectionRequest {
            group_id: policy_id,
            group_name: policy_name,
            node_id,
            node_name,
        } = selection;
        let detail = if item.detail.trim().is_empty() {
            language.localized(copy::app::UNKNOWN_TYPE).to_owned()
        } else {
            item.detail.clone()
        };
        let idle_latency = item
            .latency_ms
            .map_or_else(|| "—".to_owned(), |latency| format!("{latency} ms"));
        let spinner_id = format!("policy-node-{}-latency", item.id.as_str());
        let leading =
            Self::policy_node_leading(item.kind, manually_selectable, current, language, theme);
        let description = Self::policy_node_description(
            item.name,
            detail,
            current,
            manually_selectable,
            language,
            theme,
        );
        let source = Self::policy_node_source(source, manually_selectable, theme);
        div()
            .id(format!("node-{}", item.id.as_str()))
            .tab_stop(manually_selectable && !selection_busy)
            .min_h(px(64.0))
            .px(Space::Md.px())
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .rounded(Radius::Row.px())
            .bg(if manually_selectable && current {
                theme.action_soft
            } else {
                theme.surface_low
            })
            .child(leading)
            .child(description)
            .child(source)
            .child(
                div()
                    .w(px(64.0))
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
                row.role(Role::RadioButton)
                    .aria_toggled(if current {
                        Toggled::True
                    } else {
                        Toggled::False
                    })
                    .focusable()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !selection_busy {
                            this.select_policy_node(
                                PolicySelectionRequest {
                                    group_id: policy_id.clone(),
                                    group_name: policy_name.clone(),
                                    node_id: node_id.clone(),
                                    node_name: node_name.clone(),
                                },
                                cx,
                            );
                        }
                    }))
            })
    }

    fn policy_node_source(source: String, manually_selectable: bool, theme: Theme) -> Div {
        div()
            .w(px(100.0))
            .overflow_x_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .text_size(TextRole::Metadata.size())
            .line_height(TextRole::Metadata.line_height())
            .text_color(if manually_selectable {
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
                    .text_color(if manually_selectable {
                        theme.text_primary
                    } else {
                        theme.text_secondary
                    })
                    .child(name)
                    .when(current && !manually_selectable, |name| {
                        name.child(div().child(status_badge(
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

    fn policy_node_leading(
        kind: manis_core::PolicyCandidateKind,
        manually_selectable: bool,
        current: bool,
        language: Language,
        theme: Theme,
    ) -> Div {
        if manually_selectable {
            return div()
                .size(px(18.0))
                .rounded_full()
                .border_2()
                .border_color(if current {
                    theme.action_primary
                } else {
                    theme.outline_strong
                })
                .when(current, |dot| dot.bg(theme.action_primary));
        }
        div()
            .size(px(22.0))
            .rounded(Radius::Control.px())
            .bg(theme.surface_high)
            .text_size(TextRole::Metadata.size())
            .line_height(TextRole::Metadata.line_height())
            .font_weight(TextRole::Label.weight())
            .text_color(theme.text_tertiary)
            .flex()
            .items_center()
            .justify_center()
            .child(if kind == manis_core::PolicyCandidateKind::PolicyGroup {
                language.localized(copy::app::G)
            } else {
                language.localized(copy::app::N)
            })
    }

    fn editable_policy_group_id(&self, policy_name: &str) -> Option<&str> {
        self.managed_policies
            .groups
            .iter()
            .find(|group| group.name == policy_name)
            .map(|group| group.id.as_str())
    }

    fn policy_detail_tabs(
        &self,
        editable_group_id: Option<String>,
        language: Language,
        cx: &mut Context<Self>,
    ) -> TabBar {
        let app = cx.entity();
        TabBar::new("policy-detail-tabs")
            .underline()
            .selected_index(self.policy_detail_tab.index())
            .child(
                Tab::new()
                    .label(language.message(Message::Nodes))
                    .aria_label(language.message(Message::Nodes)),
            )
            .child(
                Tab::new()
                    .label(language.message(Message::Settings))
                    .aria_label(language.message(Message::Settings)),
            )
            .on_click(move |index, _, cx| {
                let tab = PolicyDetailTab::from_index(*index);
                app.update(cx, |this, cx| {
                    this.policy_detail_tab = tab;
                    if tab == PolicyDetailTab::Settings {
                        if let Some(group_id) = editable_group_id.as_deref() {
                            let already_editing = this
                                .managed_policies
                                .draft
                                .as_ref()
                                .and_then(|draft| draft.editing_id.as_deref())
                                == Some(group_id);
                            if !already_editing {
                                this.start_managed_policy_edit(group_id, cx);
                                return;
                            }
                        } else {
                            this.language()
                                .localized(copy::app::THIS_RUNTIME_POLICY_IS_READ_ONLY_IN_MANIS)
                                .clone_into(&mut this.status);
                        }
                    }
                    cx.notify();
                });
            })
    }

    fn detail(&self, theme: Theme, compact: bool, cx: &mut Context<Self>) -> Div {
        let language = self.language();
        let Some(view) = self.policy_detail_view() else {
            return div().h_full().flex_1().bg(theme.surface_base);
        };
        let body = match self.policy_detail_tab {
            PolicyDetailTab::Nodes => self.policy_nodes_detail(&view, language, theme, cx),
            PolicyDetailTab::Settings => {
                self.policy_settings_detail(&view, compact, language, theme, cx)
            }
        };
        div()
            .h_full()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .bg(theme.surface_base)
            .child(self.policy_detail_header(&view, compact, language, theme, cx))
            .child(body)
    }

    fn policy_detail_view(&self) -> Option<PolicyDetailView> {
        let policy = self.selected_policy()?.clone();
        let selected_node_id = self.selected_node()?.id.clone();
        let benchmark_key = Self::policy_group_benchmark_key(&policy.id);
        Some(PolicyDetailView {
            benchmarkable: Self::policy_group_benchmarkable(&policy),
            benchmarking: self
                .managed_policies
                .benchmarks
                .get(&benchmark_key)
                .is_some_and(GroupBenchmarkState::is_running),
            editable_group_id: self
                .editable_policy_group_id(&policy.name)
                .map(str::to_owned),
            display_icon: self
                .managed_policies
                .groups
                .iter()
                .find(|group| group.name == policy.name)
                .map_or(ManagedPolicyIcon::None, |group| group.icon),
            policy,
            selected_node_id,
            benchmark_key,
        })
    }

    fn policy_detail_body() -> Stateful<Div> {
        div()
            .id("detail-scroll")
            .flex_1()
            .overflow_y_scroll()
            .p(Space::Lg.px())
            .flex()
            .flex_col()
            .gap(Space::Md.px())
    }

    fn policy_nodes_detail(
        &self,
        view: &PolicyDetailView,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let mut body = Self::policy_detail_body().child(section_heading(
            language.localized(copy::app::CANDIDATE_NODES),
            Self::policy_kind_guidance(view.policy.kind, language),
            None,
            theme,
        ));
        if view.policy.kind.is_automatic() {
            body = body.child(Self::automatic_policy_notice(language, theme));
        }
        if let Some(feedback) = self
            .managed_policies
            .benchmarks
            .get(&view.benchmark_key)
            .and_then(|state| {
                Self::policy_group_benchmark_feedback(
                    language,
                    state,
                    view.policy.nodes.len(),
                    theme,
                )
            })
        {
            body = body.child(feedback);
        }
        body = body.child(Self::policy_candidate_table_header(language, theme));
        for item in view.policy.nodes.iter().cloned() {
            let source = self.policy_node_source_label(&item, language);
            let benchmark_state = self
                .managed_policies
                .benchmarks
                .get(&view.benchmark_key)
                .map_or(GroupBenchmarkNodeState::Idle, |state| {
                    state.node_state(&item.name)
                });
            let selection = PolicySelectionRequest {
                group_id: view.policy.id.clone(),
                group_name: view.policy.name.clone(),
                node_id: item.id.clone(),
                node_name: item.name.clone(),
            };
            let current = selection.node_id == view.selected_node_id;
            body = body.child(Self::node_row(
                item,
                PolicyNodeRowContext {
                    source,
                    selection,
                    current,
                    manually_selectable: view.policy.kind.allows_manual_selection(),
                    selection_busy: self.policy_selection_busy.is_some(),
                    benchmark_state,
                    language,
                    theme,
                },
                cx,
            ));
        }
        body
    }

    fn policy_kind_guidance(kind: manis_core::PolicyGroupKind, language: Language) -> &'static str {
        match kind {
            manis_core::PolicyGroupKind::Selector => {
                language.localized(copy::app::CHOOSE_THE_EXIT_USED_WHEN_A_ROUTING_RULE_TARGETS_THIS)
            }
            manis_core::PolicyGroupKind::UrlTest => language.localized(
                copy::app::MIHOMO_MEASURES_THE_CONFIGURED_URL_ON_SCHEDULE_CANDIDATES_ARE_AUTOMATIC,
            ),
            manis_core::PolicyGroupKind::Fallback => language.localized(
                copy::app::MIHOMO_CHECKS_CANDIDATES_ON_SCHEDULE_AND_FAILS_OVER_AUTOMATICALLY,
            ),
            manis_core::PolicyGroupKind::LoadBalance => language.localized(
                copy::app::MIHOMO_DISTRIBUTES_CONNECTIONS_ACROSS_CANDIDATES_AUTOMATICALLY,
            ),
            manis_core::PolicyGroupKind::Direct => {
                language.localized(copy::app::DIRECT_POLICIES_HAVE_NO_SELECTABLE_EXIT)
            }
        }
    }

    fn automatic_policy_notice(language: Language, theme: Theme) -> Div {
        div()
            .p(Space::Md.px())
            .rounded(Radius::Row.px())
            .bg(theme.surface_low)
            .text_size(TextRole::Metadata.size())
            .line_height(TextRole::Metadata.line_height())
            .text_color(theme.text_secondary)
            .child(language.localized(copy::app::AUTOMATIC_POLICY_MANIS_SHOWS_CANDIDATES_FOR_INSPECTION_MIHOMO_SELECTS_THE))
    }

    fn policy_candidate_table_header(language: Language, theme: Theme) -> Div {
        div()
            .mt(Space::Sm.px())
            .px(Space::Md.px())
            .flex()
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
                    .child(language.localized(copy::app::SOURCE)),
            )
            .child(
                div()
                    .w(px(64.0))
                    .child(language.localized(copy::common::LATENCY)),
            )
    }

    fn policy_settings_detail(
        &self,
        view: &PolicyDetailView,
        compact: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let body = Self::policy_detail_body();
        let Some(group_id) = view.editable_group_id.as_deref() else {
            return body
                .child(section_heading(
                    language.localized(copy::app::RUNTIME_POLICY),
                    language.localized(
                        copy::app::THIS_POLICY_COMES_FROM_THE_ACTIVE_KERNEL_CONFIGURATION_AND_IS,
                    ),
                    Some(
                        status_badge(
                            language.localized(copy::app::READ_ONLY),
                            StatusTone::Neutral,
                            theme,
                        )
                        .into_any_element(),
                    ),
                    theme,
                ))
                .child(
                    Self::managed_policy_add_button(
                        "add-policy-group-readonly",
                        language,
                        theme,
                        cx,
                    )
                    .mt(Space::Sm.px()),
                );
        };
        let active_draft = self
            .managed_policies
            .draft
            .as_ref()
            .filter(|draft| draft.editing_id.as_deref() == Some(group_id));
        if let Some(draft) = active_draft {
            self.editing_policy_settings(body, draft, compact, language, theme, cx)
        } else {
            Self::saved_policy_settings(body, group_id, language, theme, cx)
        }
    }

    fn editing_policy_settings(
        &self,
        body: Stateful<Div>,
        draft: &ManagedPolicyDraft,
        compact: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let remove_id = draft
            .editing_id
            .clone()
            .expect("editing policy settings require a managed policy id");
        let actions = div()
            .flex()
            .items_center()
            .gap(Space::Sm.px())
            .child(
                action_button(
                    "remove-managed-policy-editing",
                    language.localized(copy::app::DELETE_POLICY_GROUP),
                    ActionRole::Danger,
                    ControlSize::Compact,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.remove_managed_policy(&remove_id, cx);
                })),
            )
            .child(
                action_button(
                    "cancel-managed-policy-edit",
                    language.message(Message::Cancel),
                    ActionRole::Quiet,
                    ControlSize::Compact,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.managed_policies.draft = None;
                    this.managed_policies.editor_popover = None;
                    cx.notify();
                })),
            )
            .child(
                action_button(
                    "save-managed-policy-edit",
                    language.message(Message::SaveChanges),
                    ActionRole::Primary,
                    ControlSize::Compact,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.save_managed_policy(cx);
                })),
            )
            .into_any_element();
        body.child(Self::managed_policy_settings_heading(
            language,
            Some(actions),
            theme,
        ))
        .child(self.policy_editor_form(draft, compact, true, language, theme, cx))
    }

    fn saved_policy_settings(
        body: Stateful<Div>,
        group_id: &str,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let edit_id = group_id.to_owned();
        let remove_id = group_id.to_owned();
        body.child(Self::managed_policy_settings_heading(language, None, theme))
            .child(
                div()
                    .mt(Space::Sm.px())
                    .flex()
                    .gap(Space::Sm.px())
                    .child(
                        action_button(
                            "edit-managed-policy",
                            language.localized(copy::common::EDIT_POLICY_GROUP),
                            ActionRole::Secondary,
                            ControlSize::Compact,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.start_managed_policy_edit(&edit_id, cx);
                        })),
                    )
                    .child(
                        action_button(
                            "remove-managed-policy",
                            language.localized(copy::app::DELETE_POLICY_GROUP),
                            ActionRole::Danger,
                            ControlSize::Compact,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.remove_managed_policy(&remove_id, cx);
                        })),
                    ),
            )
    }

    fn managed_policy_settings_heading(
        language: Language,
        actions: Option<AnyElement>,
        theme: Theme,
    ) -> Div {
        section_heading(
            language.localized(copy::app::MANAGED_POLICY_SETTINGS),
            language.localized(
                copy::app::SAVED_IN_MANIS_AND_APPLIED_TO_THE_MANAGED_MIHOMO_CONFIGURATION,
            ),
            actions,
            theme,
        )
    }

    fn policy_detail_header(
        &self,
        view: &PolicyDetailView,
        compact: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .p(Space::Lg.px())
            .border_b_1()
            .border_color(theme.outline_subtle)
            .child(Self::policy_detail_title(
                view, compact, language, theme, cx,
            ))
            .child(
                div()
                    .mt(Space::Lg.px())
                    .font_weight(TextRole::Label.weight())
                    .child(self.policy_detail_tabs(view.editable_group_id.clone(), language, cx)),
            )
    }

    fn policy_detail_title(
        view: &PolicyDetailView,
        compact: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let benchmark_id = view.policy.id.clone();
        let benchmarkable = view.benchmarkable;
        let benchmarking = view.benchmarking;
        div()
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .when(compact, |header| {
                header.child(
                    Button::new("compact-back")
                        .accessibility_label(language.localized(copy::app::BACK_TO_POLICY_GROUPS))
                        .label(language.localized(copy::app::BACK))
                        .icon(IconName::ArrowLeft)
                        .with_size(ControlSize::Compact.component_size())
                        .h(ControlSize::Compact.height())
                        .with_variant(ButtonVariant::Text)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.workspace.navigate_back();
                            cx.notify();
                        })),
                )
            })
            .child(Self::policy_group_icon(
                PolicyGroupIconView {
                    id: &view.benchmark_key,
                    icon: view.display_icon,
                    policy_name: &view.policy.name,
                    benchmarkable,
                    running: benchmarking,
                    language,
                    theme,
                },
                cx.listener(move |this, _, _, cx| {
                    if benchmarkable && !benchmarking {
                        this.start_policy_group_benchmark(&benchmark_id, cx);
                    }
                }),
            ))
            .child(Self::policy_detail_identity(&view.policy, language, theme))
    }

    fn policy_detail_identity(policy: &PolicyGroup, language: Language, theme: Theme) -> Div {
        div()
            .min_w(px(0.0))
            .flex_1()
            .child(
                div()
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::PageTitle.size())
                    .line_height(TextRole::PageTitle.line_height())
                    .font_weight(TextRole::PageTitle.weight())
                    .child(policy.name.clone()),
            )
            .child(
                div()
                    .mt(Space::Xs.px())
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_secondary)
                    .child(format!(
                        "{} · {}",
                        policy_kind_label(language, policy.kind),
                        language.count(CountNoun::Node, policy.nodes.len())
                    )),
            )
    }

    fn status_bar(&self, theme: Theme, cx: &mut Context<Self>) -> StatusBar {
        let language = self.language();
        let kernel_name = self.runtime.kind().display_name();
        let source = controller_status_label(&self.controller, kernel_name, language);
        let values = status_bar_values(&self.controller, language, theme);

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
                    .child(status_badge(source, values.tone, theme)),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(TextRole::Data.size())
                    .line_height(TextRole::Data.line_height())
                    .font_weight(TextRole::Data.weight())
                    .text_color(theme.text_secondary)
                    .child(values.endpoint),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_tertiary)
                    .child(self.status.clone()),
            );
        let right = div()
            .flex()
            .items_center()
            .gap(Space::Md.px())
            .text_size(TextRole::Data.size())
            .line_height(TextRole::Data.line_height())
            .font_weight(TextRole::Data.weight())
            .text_color(theme.text_secondary)
            .when_some(
                match &self.app_update_state {
                    AppUpdateState::Ready(staged) => Some(staged.version.clone()),
                    _ => None,
                },
                |right, version| {
                    right.child(
                        style_action_button(
                            Button::new("status-bar-restart-update")
                                .accessibility_label(
                                    language.localized(copy::app_update::RESTART_AND_UPDATE),
                                )
                                .label(format!(
                                    "{} · {version}",
                                    language.localized(copy::app_update::RESTART_AND_UPDATE)
                                ))
                                .icon(IconName::Redo2),
                            ActionRole::Primary,
                            ControlSize::Compact,
                        )
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.restart_with_app_update(cx);
                        })),
                    )
                },
            )
            .child(values.download)
            .child(values.upload);

        StatusBar::new()
            .h(ControlSize::Icon.min_pointer_target())
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
