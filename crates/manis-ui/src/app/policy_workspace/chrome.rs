use super::{
    ActionRole, AnyElement, Button, ButtonGroup, ButtonVariant, ButtonVariants, Context,
    ControlSize, Disableable, Div, FluentBuilder, IconName, InteractiveElement, IntoElement,
    LayoutMetric, ManisApp, Message, ParentElement, PrimaryWorkspace, ProxyMode, Radius, Role,
    RoutingMode, Selectable, Sizable, Space, StatefulInteractiveElement, Styled, TextRole, Theme,
    UiEvent, WindowSizeClass, action_button, assets, brand, compact_proxy_mode_label, copy, div,
    img, platform_chrome_left_padding, proxy_mode_label, px, routing_mode_label, trace_ui,
};

impl ManisApp {
    pub(in crate::app) fn chrome(
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
            .when(!cfg!(target_os = "macos"), |brand| {
                brand
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

    pub(in crate::app) fn navigation(
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
}
