use crate::components::style_action_button;

use super::{
    ActionRole, AnyElement, Button, ButtonVariant, ButtonVariants, Context, ControlSize,
    Disableable, Div, FluentBuilder, IconName, InteractiveElement, IntoElement, LayoutMetric,
    ManisApp, Message, ParentElement, PrimaryWorkspace, ProxyMode, Radius, Role, RoutingMode,
    Selectable, Sizable, Space, Stateful, StatefulInteractiveElement, Styled, TextRole, Theme,
    UiEvent, WindowSizeClass, action_button, assets, brand, compact_proxy_mode_label, copy, div,
    img, platform_chrome_left_padding, proxy_mode_label, px, routing_mode_label, trace_ui,
};

use crate::app::presentation::{controller_status_label, format_bytes};

impl ManisApp {
    /// The desktop shell: a left status rail plus a top command bar.
    ///
    /// Navigation lives in the rail, all connection state lives in the rail's telemetry card and
    /// the command bar, and pages own only their content. No bottom status strip competes with the
    /// workspace for height.
    pub(in crate::app) fn navigation(
        &self,
        theme: Theme,
        size_class: WindowSizeClass,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let wide = size_class == WindowSizeClass::Wide;
        let width = match size_class {
            WindowSizeClass::Wide => LayoutMetric::WideNavigation.px(),
            WindowSizeClass::Medium => LayoutMetric::MediumNavigation.px(),
            WindowSizeClass::Compact => LayoutMetric::CompactNavigation.px(),
        };
        // On macOS the transparent title bar places the traffic lights over the top-left corner.
        // A wide rail keeps the brand row beside them; a narrow rail starts below them.
        let top_padding = if cfg!(target_os = "macos") && !wide {
            px(38.0)
        } else {
            px(0.0)
        };
        div()
            .id("navigation-rail")
            .debug_selector(|| "navigation-rail".to_owned())
            .w(width)
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(theme.surface_chrome)
            .border_r_1()
            .border_color(theme.outline_subtle)
            .p(Space::Sm.px())
            .pt(Space::Sm.px() + top_padding)
            .gap(Space::Xs.px())
            .child(Self::rail_brand(theme, wide))
            .child(div().h(Space::Sm.px()))
            .child(self.rail_workspaces(theme, wide, cx))
            .child(div().flex_1().min_h(px(0.0)))
            .when(wide, |rail| rail.child(self.rail_telemetry(theme)))
            .child(self.rail_tools(theme, wide, cx))
    }

    fn rail_brand(theme: Theme, wide: bool) -> Div {
        let mac_inset = if cfg!(target_os = "macos") && wide {
            platform_chrome_left_padding() - Space::Sm.px()
        } else {
            px(0.0)
        };
        div()
            .flex_shrink_0()
            .h(LayoutMetric::BrandRowHeight.px() - Space::Sm.px() * 2.0)
            .pl(mac_inset)
            .flex()
            .items_center()
            .gap(Space::Sm.px())
            .when(!wide, gpui::Styled::justify_center)
            .child(
                div()
                    .size(px(28.0))
                    .flex_shrink_0()
                    .rounded(Radius::Badge.px() + px(2.0))
                    .overflow_hidden()
                    .child(img(assets::BRAND_MARK_PATH).size_full()),
            )
            .when(wide, |brand| {
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

    fn rail_workspaces(&self, theme: Theme, wide: bool, cx: &mut Context<Self>) -> Div {
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
        ];
        div().flex().flex_col().gap(px(3.0)).children(
            entries
                .into_iter()
                .map(|(label, icon, workspace)| {
                    self.rail_workspace_row(theme, wide, label, icon, workspace, cx)
                })
                .collect::<Vec<_>>(),
        )
    }

    fn rail_workspace_row(
        &self,
        theme: Theme,
        wide: bool,
        label: &'static str,
        icon: IconName,
        workspace: PrimaryWorkspace,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let selected = workspace == self.primary_workspace;
        let badge = (workspace == PrimaryWorkspace::Activity
            && !self.active_connections.is_empty())
        .then(|| self.active_connections.len().to_string());
        div()
            .id(format!("navigation-{workspace:?}"))
            .debug_selector(move || format!("navigation-{workspace:?}"))
            .role(Role::Button)
            .aria_label(label)
            .tab_stop(true)
            .focusable()
            .map(crate::components::primary_button_interaction)
            .cursor_pointer()
            .h(px(36.0))
            .px(if wide { px(9.0) } else { px(0.0) })
            .rounded(Radius::Control.px())
            .flex()
            .items_center()
            .gap(Space::Sm.px())
            .when(!wide, |row| {
                row.justify_center().tooltip(move |window, cx| {
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
                .text_color(if selected {
                    theme.action_primary
                } else {
                    theme.text_primary
                })
            })
            .border_1()
            .border_color(gpui::rgba(0x0000_0000))
            .focus_visible(move |row| row.border_color(theme.focus_ring))
            .text_size(TextRole::Label.size())
            .line_height(TextRole::Label.line_height())
            .font_weight(if selected {
                gpui::FontWeight::SEMIBOLD
            } else {
                TextRole::Label.weight()
            })
            .when(wide, |row| {
                // The 3px accent stripe marks the active workspace at the rail edge.
                row.child(
                    div()
                        .w(px(3.0))
                        .h(px(18.0))
                        .flex_shrink_0()
                        .rounded(px(2.0))
                        .bg(if selected {
                            theme.action_primary
                        } else {
                            gpui::rgba(0x0000_0000)
                        }),
                )
            })
            .child(gpui_component::Icon::new(icon).size(px(18.0)))
            .when(wide, |row| row.child(label))
            .when_some(if wide { badge } else { None }, |row, badge| {
                row.child(
                    div()
                        .ml_auto()
                        .h(px(18.0))
                        .px(px(6.0))
                        .rounded(Radius::Badge.px())
                        .border_1()
                        .border_color(theme.outline_subtle)
                        .bg(theme.surface_elevated)
                        .font_family(crate::theme::MONO_FAMILY)
                        .text_size(px(10.5))
                        .line_height(px(16.0))
                        .text_color(theme.text_tertiary)
                        .child(badge),
                )
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_primary_workspace(workspace, cx);
            }))
    }

    /// The rail's live telemetry card: exit, mode and cumulative traffic, replacing the old
    /// full-width bottom status strip.
    fn rail_telemetry(&self, theme: Theme) -> Stateful<Div> {
        let language = self.language();
        let exit = self.managed_policies.node_selections.global().map_or_else(
            || language.localized(copy::app::NO_EXIT_SELECTED).to_owned(),
            |global| global.node_name.clone(),
        );
        let proxy_color = if self.proxy_mode == ProxyMode::Off {
            theme.text_tertiary
        } else {
            theme.status_success
        };
        let (download, upload) = match &self.controller {
            super::ControllerState::Connected {
                download_total,
                upload_total,
                ..
            } => (
                format!("↓ {}", format_bytes(*download_total)),
                format!("↑ {}", format_bytes(*upload_total)),
            ),
            _ => ("↓ —".to_owned(), "↑ —".to_owned()),
        };
        let kernel = controller_status_label(
            &self.controller,
            self.runtime.kind().display_name(),
            language,
        );
        let status_tooltip = self.status.clone();
        crate::components::card_surface(theme)
            .id("rail-telemetry")
            .debug_selector(|| "rail-telemetry".to_owned())
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(status_tooltip.clone()).build(window, cx)
            })
            .w_full()
            .flex_shrink_0()
            .p(px(11.0))
            .flex()
            .flex_col()
            .gap(px(7.0))
            .child(self.telemetry_mode_row(theme, language, proxy_color))
            .child(
                div()
                    .text_size(TextRole::Data.size())
                    .line_height(TextRole::Data.line_height())
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme.text_primary)
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(exit),
            )
            .child(Self::telemetry_traffic_row(theme, download, upload))
            .child(
                div()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_tertiary)
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(kernel),
            )
            .when_some(self.live_status_issue(), |card, issue| {
                // A live-stream problem outranks the mode colour but must stay explainable.
                card.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.0))
                        .text_size(TextRole::Metadata.size())
                        .line_height(TextRole::Metadata.line_height())
                        .text_color(theme.status_warning)
                        .child(div().size(px(5.0)).rounded_full().bg(theme.status_warning))
                        .child(issue),
                )
            })
    }

    fn telemetry_mode_row(
        &self,
        theme: Theme,
        language: super::Language,
        proxy_color: gpui::Rgba,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(Space::Sm.px())
            .text_size(TextRole::Metadata.size())
            .line_height(TextRole::Metadata.line_height())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_color(theme.text_tertiary)
                    .child(div().size(px(6.0)).rounded_full().bg(proxy_color))
                    .child(proxy_mode_label(language, self.proxy_mode)),
            )
            .child(
                div()
                    .font_family(crate::theme::MONO_FAMILY)
                    .text_color(theme.text_tertiary)
                    .child(routing_mode_label(language, self.routing_mode)),
            )
    }

    fn telemetry_traffic_row(theme: Theme, download: String, upload: String) -> Div {
        div()
            .pt(px(6.0))
            .border_t_1()
            .border_color(theme.outline_subtle)
            .flex()
            .items_center()
            .justify_between()
            .font_family(crate::theme::MONO_FAMILY)
            .text_size(TextRole::Metadata.size())
            .line_height(TextRole::Metadata.line_height())
            .text_color(theme.text_secondary)
            .child(download)
            .child(upload)
    }

    fn rail_tools(&self, theme: Theme, wide: bool, cx: &mut Context<Self>) -> Div {
        let language = self.language();
        let settings_selected = self.primary_workspace == PrimaryWorkspace::Configuration;
        let settings_row = div()
            .id("navigation-settings")
            .debug_selector(|| "navigation-settings".to_owned())
            .role(Role::Button)
            .aria_label(language.message(Message::Settings))
            .tab_stop(true)
            .focusable()
            .map(crate::components::primary_button_interaction)
            .cursor_pointer()
            .when(wide, gpui::Styled::flex_1)
            .h(px(34.0))
            .px(if wide { px(9.0) } else { px(0.0) })
            .rounded(Radius::Control.px())
            .flex()
            .items_center()
            .gap(Space::Sm.px())
            .when(!wide, gpui::Styled::justify_center)
            .when(settings_selected, |row| {
                row.bg(theme.action_soft).text_color(theme.action_primary)
            })
            .when(!settings_selected, |row| {
                row.text_color(theme.text_secondary)
            })
            .hover(move |row| row.bg(theme.surface_high).text_color(theme.text_primary))
            .border_1()
            .border_color(gpui::rgba(0x0000_0000))
            .focus_visible(move |row| row.border_color(theme.focus_ring))
            .text_size(TextRole::Label.size())
            .line_height(TextRole::Label.line_height())
            .child(gpui_component::Icon::new(IconName::Settings).size(px(17.0)))
            .when(wide, |row| row.child(language.message(Message::Settings)))
            .on_click(cx.listener(|this, _, _, cx| {
                this.open_primary_workspace(PrimaryWorkspace::Configuration, cx);
            }));

        div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(Space::Xs.px())
            .pt(Space::Sm.px())
            .border_t_1()
            .border_color(theme.outline_subtle)
            .when(!wide, gpui::Styled::flex_col)
            .child(settings_row)
            .child(self.theme_toggle(theme, cx))
    }

    fn theme_toggle(&self, _theme: Theme, cx: &mut Context<Self>) -> Button {
        let language = self.language();
        // The icon shows the theme the click switches to: sun for light, moon for dark.
        let (icon, label) = if self.dark {
            (IconName::Sun, language.localized(copy::app::LIGHT))
        } else {
            (IconName::Moon, language.localized(copy::app::DARK))
        };
        style_action_button(
            Button::new("theme-toggle").icon(icon),
            ActionRole::Secondary,
            ControlSize::Icon,
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

    /// The fixed command bar above every workspace: page identity on the left, the two
    /// segmented commands that steer all traffic on the right.
    pub(in crate::app) fn command_bar(
        &self,
        theme: Theme,
        size_class: WindowSizeClass,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let compact = size_class == WindowSizeClass::Compact;
        let (title, meta) = self.command_bar_identity();
        let view_switcher = self.workspace_view_switcher(theme, cx);
        div()
            .id("command-bar")
            .debug_selector(|| "command-bar".to_owned())
            .h(LayoutMetric::CommandBarHeight.px())
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_between()
            .gap(Space::Md.px())
            .px(if compact { px(12.0) } else { Space::Xl.px() })
            .border_b_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_base)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .min_w_0()
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(theme.text_primary)
                            .text_size(TextRole::PageTitle.size())
                            .line_height(TextRole::PageTitle.line_height())
                            .font_weight(TextRole::PageTitle.weight())
                            .child(title),
                    )
                    .when_some(view_switcher, ParentElement::child)
                    .when(!meta.is_empty(), |left| {
                        // The count pill: a compact, quiet badge next to the page title.
                        left.child(
                            div()
                                .flex_shrink_0()
                                .max_w(px(420.0))
                                .h(px(20.0))
                                .px(Space::Sm.px())
                                .rounded(Radius::Badge.px())
                                .bg(theme.surface_low)
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_color(theme.text_tertiary)
                                .text_size(TextRole::Metadata.size())
                                .line_height(px(20.0))
                                .child(meta),
                        )
                    }),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(Space::Sm.px())
                    .child(self.proxy_control(theme, compact, cx))
                    .child(self.routing_control(theme, compact, cx)),
            )
    }

    fn workspace_view_switcher(&self, theme: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        use crate::app::WorkspaceViewMode;
        use gpui_component::Icon;

        let workspace = self.primary_workspace;
        let current = self.topology.view_for(workspace)?;
        let language = self.language();

        let (tree_icon, tree_tip, list_tip) = match workspace {
            PrimaryWorkspace::Nodes => (
                gpui_component_assets::IconName::Network,
                match language {
                    super::Language::English => "Tree view (V)",
                    super::Language::SimplifiedChinese => "策略树视图 (V)",
                },
                match language {
                    super::Language::English => "Card & list view (V)",
                    super::Language::SimplifiedChinese => "列表卡片视图 (V)",
                },
            ),
            PrimaryWorkspace::RoutingRules => (
                gpui_component_assets::IconName::Workflow,
                match language {
                    super::Language::English => "Routing topology view (V)",
                    super::Language::SimplifiedChinese => "路由拓扑视图 (V)",
                },
                match language {
                    super::Language::English => "Priority pipeline view (V)",
                    super::Language::SimplifiedChinese => "优先级管道视图 (V)",
                },
            ),
            PrimaryWorkspace::Activity
            | PrimaryWorkspace::Logs
            | PrimaryWorkspace::Configuration => return None,
        };

        let active_bg = if theme.surface_elevated == theme.surface_base {
            theme.surface_high
        } else {
            theme.surface_elevated
        };

        let make_button = |id: &'static str,
                           mode: WorkspaceViewMode,
                           icon: gpui_component_assets::IconName,
                           label: &'static str| {
            let selected = current == mode;
            Button::new(id)
                .map(crate::components::primary_button_interaction)
                .debug_selector(move || id.to_owned())
                .tooltip(label)
                .accessibility_label(label)
                .selected(selected)
                .with_variant(ButtonVariant::Ghost)
                .with_size(gpui_component::Size::Small)
                .w(px(28.0))
                .h_full()
                .px_0()
                .rounded(px(6.0))
                .when(selected, |button| {
                    button.bg(active_bg).text_color(theme.action_primary)
                })
                .when(!selected, |button| button.text_color(theme.text_tertiary))
                .child(Icon::new(icon).size(px(14.0)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if this.topology.set_view_for(workspace, mode) {
                        cx.notify();
                    }
                }))
        };

        Some(
            div()
                .id("workspace-view-switch")
                .debug_selector(|| "workspace-view-switch".to_owned())
                .h(px(28.0))
                .flex_shrink_0()
                .p(px(2.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(theme.outline_subtle)
                .bg(theme.surface_low)
                .flex()
                .items_center()
                .gap(px(2.0))
                .child(make_button(
                    "view-mode-tree",
                    WorkspaceViewMode::Tree,
                    tree_icon,
                    tree_tip,
                ))
                .child(make_button(
                    "view-mode-list",
                    WorkspaceViewMode::List,
                    gpui_component_assets::IconName::LayoutList,
                    list_tip,
                ))
                .into_any_element(),
        )
    }

    fn command_bar_identity(&self) -> (String, String) {
        let language = self.language();
        match self.primary_workspace {
            PrimaryWorkspace::Nodes => {
                let policies = if self.managed_policies.groups.is_empty() {
                    self.policy_groups()
                        .filter(|group| group.name != manis_profile::MANIS_GLOBAL_GROUP_NAME)
                        .count()
                } else {
                    self.managed_policies.groups.len()
                };
                let nodes = self
                    .source_providers
                    .iter()
                    .map(|provider| provider.nodes.len())
                    .sum::<usize>()
                    + self.saved_single_nodes.len();
                (
                    language.message(Message::Nodes).to_owned(),
                    format!(
                        "{} · {}",
                        language.count(super::CountNoun::PolicyGroup, policies),
                        language.count(super::CountNoun::Node, nodes)
                    ),
                )
            }
            PrimaryWorkspace::RoutingRules => (
                language.message(Message::RoutingRules).to_owned(),
                self.active_rules_summary(language),
            ),
            PrimaryWorkspace::Activity => {
                let count =
                    language.count(super::CountNoun::Connection, self.active_connections.len());
                (
                    language.message(Message::NetworkActivity).to_owned(),
                    if self.proxy_mode == ProxyMode::Off {
                        format!("{count} · {}", proxy_mode_label(language, ProxyMode::Off))
                    } else {
                        count
                    },
                )
            }
            PrimaryWorkspace::Logs => (
                language.message(Message::Logs).to_owned(),
                language.count(
                    super::CountNoun::Log,
                    self.kernel_logs.len() + crate::diagnostics::recent_ui_logs().len(),
                ),
            ),
            PrimaryWorkspace::Configuration => (
                language.message(Message::Settings).to_owned(),
                String::new(),
            ),
        }
    }

    fn proxy_mode_button(
        &self,
        theme: Theme,
        language: super::Language,
        mode: ProxyMode,
        interactive: bool,
        cx: &mut Context<Self>,
    ) -> Button {
        let selected = mode == self.proxy_mode;
        let pending = self.proxy_mode_busy == Some(mode);
        let label = if pending {
            match mode {
                ProxyMode::Tun => language.localized(copy::app::PREPARING_TUN),
                ProxyMode::System => language.localized(copy::app::ENABLING),
                ProxyMode::Off => language.localized(copy::app::TURNING_OFF),
            }
        } else {
            proxy_mode_label(language, mode)
        };
        Button::new(format!("proxy-mode-{mode:?}"))
            .map(crate::components::primary_button_interaction)
            .debug_selector(move || format!("proxy-mode-{mode:?}"))
            .accessibility_label(proxy_mode_label(language, mode))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .when(pending || (selected && mode != ProxyMode::Off), |row| {
                        row.child(div().size(px(5.0)).rounded_full().bg(if pending {
                            theme.text_tertiary
                        } else {
                            theme.status_success
                        }))
                    })
                    .child(label),
            )
            .selected(selected)
            .tab_stop(interactive)
            .disabled(!interactive)
            .with_variant(ButtonVariant::Ghost)
            .with_size(gpui_component::Size::Small)
            .h_full()
            .px(px(9.0))
            .rounded(px(7.0))
            .text_size(TextRole::Metadata.size())
            .line_height(TextRole::Metadata.line_height())
            .font_weight(if pending || selected {
                gpui::FontWeight::SEMIBOLD
            } else {
                TextRole::Label.weight()
            })
            .when(selected, |button| {
                button
                    .bg(if theme.surface_elevated == theme.surface_base {
                        theme.surface_high
                    } else {
                        theme.surface_elevated
                    })
                    .text_color(theme.text_primary)
            })
            .when(!selected, |button| button.text_color(theme.text_secondary))
            .loading(pending)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.apply_proxy_mode(mode, cx);
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
        let mut modes = div().flex().items_center().gap(px(2.0)).h_full();
        for mode in [ProxyMode::Off, ProxyMode::System, ProxyMode::Tun] {
            modes = modes.child(self.proxy_mode_button(theme, language, mode, interactive, cx));
        }
        // The grouped track marks the two commands apart from ordinary page actions.
        div()
            .id("proxy-modes")
            .h(px(32.0))
            .flex_shrink_0()
            .p(px(2.0))
            .rounded(Radius::Control.px())
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_low)
            .flex()
            .items_center()
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
                routing_mode_label(language, self.routing_mode)
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

        let mut modes = div().flex().items_center().gap(px(2.0)).h_full();
        for mode in [RoutingMode::Direct, RoutingMode::Rule, RoutingMode::Global] {
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
                    .with_variant(ButtonVariant::Ghost)
                    .with_size(gpui_component::Size::Small)
                    .h_full()
                    .px(px(9.0))
                    .rounded(px(7.0))
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .font_weight(if selected {
                        gpui::FontWeight::SEMIBOLD
                    } else {
                        TextRole::Label.weight()
                    })
                    .when(selected, |button| {
                        button
                            .bg(if theme.surface_elevated == theme.surface_base {
                                theme.surface_high
                            } else {
                                theme.surface_elevated
                            })
                            .text_color(theme.text_primary)
                    })
                    .when(!selected, |button| button.text_color(theme.text_secondary))
                    .disabled(self.routing_mode_busy.is_some())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.apply_routing_mode(mode, cx);
                    })),
            );
        }
        div()
            .id("routing-modes")
            .h(px(32.0))
            .flex_shrink_0()
            .p(px(2.0))
            .rounded(Radius::Control.px())
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_low)
            .flex()
            .items_center()
            .child(modes)
            .into_any_element()
    }

    /// Opens the settings page in the main window, used by the sidebar and the settings shortcut.
    pub(crate) fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.open_primary_workspace(PrimaryWorkspace::Configuration, cx);
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
