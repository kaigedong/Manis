use super::{
    AnyElement, Button, Context, ControlSize, Disableable, Div, FluentBuilder, FontWeight,
    InteractiveElement, IntoElement, Language, ManagedPolicyDraft, ManagedPolicyIcon,
    ManagedPolicyStrategy, ManisApp, ParentElement, PolicyCandidateMatcherKind,
    PolicyEditorPopover, PolicyEditorPopup, Radius, Role, Stateful, StatefulInteractiveElement,
    Styled, TextRole, Theme, Toggled, copy, div, px,
};

impl ManisApp {
    pub(in crate::app) fn policy_editor_form(
        &self,
        draft: &ManagedPolicyDraft,
        compact: bool,
        embedded: bool,
        language: Language,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let popover_width = if compact { 280.0 } else { 300.0 };
        let basics = self.policy_editor_basics(draft, language, theme, popover_width, cx);
        let nodes = self.policy_editor_candidates(draft, language, theme, popover_width, cx);

        div()
            .id("policy-editor-scroll")
            .when(!embedded, |form| {
                form.flex_1().min_h_0().overflow_y_scroll()
            })
            .px(if embedded {
                px(0.0)
            } else if compact {
                px(16.0)
            } else {
                px(28.0)
            })
            .py(if embedded { px(0.0) } else { px(24.0) })
            .child(
                div()
                    .w_full()
                    .max_w(px(760.0))
                    .mx_auto()
                    .child(Self::policy_editor_section_label(
                        language.localized(copy::nodes::BASIC_INFORMATION),
                        theme,
                    ))
                    .child(basics)
                    .child(
                        Self::policy_editor_section_label(
                            language.localized(copy::nodes::CANDIDATES),
                            theme,
                        )
                        .mt_6(),
                    )
                    .child(nodes),
            )
    }

    pub(in crate::app) fn policy_editor_basics(
        &self,
        draft: &ManagedPolicyDraft,
        language: Language,
        theme: Theme,
        popover_width: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let strategy = match draft.strategy {
            ManagedPolicyStrategy::Manual => "static".to_owned(),
            ManagedPolicyStrategy::LowestLatency => "url-latency-benchmark".to_owned(),
        };
        let busy = self.managed_policies.mutation_state.is_busy();
        let policy_name = self
            .inputs
            .policy_group_name
            .as_ref()
            .map_or_else(String::new, |input| input.read(cx).value().to_owned());
        div()
            .rounded(Radius::Pane.px())
            .overflow_hidden()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_low)
            .child(Self::policy_editor_popup_row(
                "policy-editor-type",
                language.localized(copy::nodes::TYPE),
                strategy,
                None,
                theme,
                PolicyEditorPopup::new(
                    PolicyEditorPopover::Strategy,
                    self.managed_policies.editor_popover == Some(PolicyEditorPopover::Strategy),
                    Self::policy_strategy_menu(draft, language, theme, cx),
                    popover_width,
                    220.0,
                )
                .disabled(busy),
                cx,
            ))
            .child(Self::policy_editor_input_row(
                language.localized(copy::nodes::POLICY_GROUP_NAME),
                true,
                self.inputs.policy_group_name.clone(),
                true,
                theme,
            ))
            .child(Self::policy_editor_popup_row(
                "policy-editor-icon",
                language.localized(copy::nodes::ICON),
                Self::managed_policy_icon_label(draft.icon, language).to_owned(),
                Some(Self::policy_icon_visual(
                    draft.icon,
                    &policy_name,
                    28.0,
                    theme,
                )),
                theme,
                PolicyEditorPopup::new(
                    PolicyEditorPopover::Icon,
                    self.managed_policies.editor_popover == Some(PolicyEditorPopover::Icon),
                    Self::policy_icon_menu(draft, language, theme, cx),
                    popover_width,
                    320.0,
                )
                .with_divider(false)
                .disabled(busy),
                cx,
            ))
    }

    pub(in crate::app) fn policy_editor_candidates(
        &self,
        draft: &ManagedPolicyDraft,
        language: Language,
        theme: Theme,
        popover_width: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let matcher = match draft.matcher_kind {
            PolicyCandidateMatcherKind::All => {
                language.localized(copy::nodes::ALL_NODES).to_owned()
            }
            PolicyCandidateMatcherKind::NameContains => {
                language.localized(copy::nodes::NAME_CONTAINS).to_owned()
            }
            PolicyCandidateMatcherKind::Explicit => language
                .localized(copy::nodes::SELECT_NODES_OR_GROUPS)
                .to_owned(),
        };
        let has_details = draft.matcher_kind != PolicyCandidateMatcherKind::All
            || draft.strategy == ManagedPolicyStrategy::LowestLatency;
        let busy = self.managed_policies.mutation_state.is_busy();
        let mut nodes = div()
            .rounded(Radius::Pane.px())
            .overflow_hidden()
            .border_1()
            .border_color(theme.outline_subtle)
            .bg(theme.surface_low)
            .child(Self::policy_editor_popup_row(
                "policy-editor-candidate-mode",
                language.localized(copy::nodes::NODE_SCOPE),
                matcher,
                None,
                theme,
                PolicyEditorPopup::new(
                    PolicyEditorPopover::CandidateMode,
                    self.managed_policies.editor_popover
                        == Some(PolicyEditorPopover::CandidateMode),
                    Self::policy_candidate_mode_menu(draft, language, theme, cx),
                    popover_width,
                    280.0,
                )
                .with_divider(has_details)
                .disabled(busy),
                cx,
            ));
        if draft.matcher_kind == PolicyCandidateMatcherKind::NameContains {
            nodes = nodes.child(Self::policy_editor_input_row(
                language.localized(copy::nodes::NODE_NAME_CONTAINS),
                false,
                self.inputs.policy_group_filter.clone(),
                draft.strategy == ManagedPolicyStrategy::LowestLatency,
                theme,
            ));
        }
        if draft.matcher_kind == PolicyCandidateMatcherKind::Explicit {
            nodes = nodes.child(self.policy_editor_selected_candidates(
                draft,
                language,
                theme,
                popover_width,
                cx,
            ));
        }
        if draft.strategy == ManagedPolicyStrategy::LowestLatency {
            nodes = nodes.child(self.policy_editor_interval_row(
                draft,
                language,
                theme,
                popover_width,
                cx,
            ));
            nodes = nodes.child(self.policy_editor_tolerance_row(
                draft,
                language,
                theme,
                popover_width,
                cx,
            ));
        }
        nodes
    }

    pub(in crate::app) fn policy_editor_selected_candidates(
        &self,
        draft: &ManagedPolicyDraft,
        language: Language,
        theme: Theme,
        popover_width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = copy::nodes::selected_count(language, draft.explicit_members.len());
        Self::policy_editor_popup_row(
            "policy-editor-selected-nodes",
            language.localized(copy::nodes::SELECTED_CANDIDATES),
            selected,
            None,
            theme,
            PolicyEditorPopup::new(
                PolicyEditorPopover::CandidateNodes,
                self.managed_policies.editor_popover == Some(PolicyEditorPopover::CandidateNodes),
                self.policy_candidate_menu(draft, language, theme, cx),
                popover_width.max(480.0),
                420.0,
            )
            .with_divider(draft.strategy == ManagedPolicyStrategy::LowestLatency)
            .disabled(self.managed_policies.mutation_state.is_busy()),
            cx,
        )
    }

    pub(in crate::app) fn policy_editor_tolerance_row(
        &self,
        draft: &ManagedPolicyDraft,
        language: Language,
        theme: Theme,
        popover_width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row = Self::policy_editor_popup_row(
            "policy-editor-tolerance",
            language.localized(copy::nodes::SWITCH_TOLERANCE),
            format!("{} ms", draft.switch_tolerance_ms),
            None,
            theme,
            PolicyEditorPopup::new(
                PolicyEditorPopover::Tolerance,
                self.managed_policies.editor_popover == Some(PolicyEditorPopover::Tolerance),
                Self::policy_tolerance_menu(draft, theme, cx),
                popover_width,
                320.0,
            )
            .with_divider(false)
            .disabled(self.managed_policies.mutation_state.is_busy()),
            cx,
        );
        div()
            .child(row)
            .child(
                div()
                    .px_4()
                    .pb_3()
                    .text_size(TextRole::Metadata.size())
                    .line_height(TextRole::Metadata.line_height())
                    .text_color(theme.text_secondary)
                    .child(language.localized(copy::nodes::SWITCH_TOLERANCE_HELP)),
            )
            .into_any_element()
    }

    pub(in crate::app) fn policy_editor_interval_row(
        &self,
        draft: &ManagedPolicyDraft,
        language: Language,
        theme: Theme,
        popover_width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let interval = copy::nodes::interval(language, draft.test_interval_secs);
        Self::policy_editor_popup_row(
            "policy-editor-interval",
            language.localized(copy::nodes::RETEST_INTERVAL),
            interval,
            None,
            theme,
            PolicyEditorPopup::new(
                PolicyEditorPopover::Interval,
                self.managed_policies.editor_popover == Some(PolicyEditorPopover::Interval),
                Self::policy_interval_menu(draft, language, theme, cx),
                popover_width,
                320.0,
            )
            .with_divider(true)
            .disabled(self.managed_policies.mutation_state.is_busy()),
            cx,
        )
    }

    pub(in crate::app) fn policy_editor_section_label(label: &'static str, theme: Theme) -> Div {
        div()
            .mb_2()
            .px_2()
            .text_size(TextRole::Label.size())
            .line_height(TextRole::Label.line_height())
            .font_weight(TextRole::Label.weight())
            .text_color(theme.text_secondary)
            .child(label)
    }

    fn policy_editor_popup_row(
        id: &'static str,
        label: &'static str,
        value: String,
        value_icon: Option<Div>,
        theme: Theme,
        popup: PolicyEditorPopup,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let PolicyEditorPopup {
            kind,
            open,
            content,
            width,
            max_height,
            show_divider,
            disabled,
        } = popup;
        let app = cx.entity();
        let trigger = crate::components::style_select_trigger(
            Button::new(id)
                .accessibility_label(format!("{label}: {value}"))
                .disabled(disabled)
                .dropdown_caret(true),
            ControlSize::Standard,
        )
        .when(disabled, gpui::Styled::cursor_default)
        .w_full()
        .child(
            div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap_2()
                .when_some(value_icon, ParentElement::child)
                .child(
                    div()
                        .flex_1()
                        .overflow_x_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(TextRole::Label.size())
                        .line_height(TextRole::Label.line_height())
                        .font_weight(TextRole::Label.weight())
                        .text_color(if disabled {
                            theme.text_tertiary
                        } else {
                            theme.text_primary
                        })
                        .child(value),
                ),
        );
        let popover = crate::components::anchored_popover(
            format!("{id}-popover"),
            trigger,
            content,
            width,
            max_height,
        )
        .open(open)
        .on_open_change(move |open, _, cx| {
            app.update(cx, |this, cx| {
                if this.managed_policies.mutation_state.is_busy() {
                    this.managed_policies.editor_popover = None;
                    cx.notify();
                    return;
                }
                this.managed_policies.editor_popover = open.then_some(kind);
                cx.notify();
            });
        });

        div()
            .min_h(px(52.0))
            .px_4()
            .border_color(theme.outline_subtle)
            .when(show_divider, gpui::Styled::border_b_1)
            .flex()
            .items_center()
            .gap_4()
            .child(
                div()
                    .flex_1()
                    .text_size(TextRole::Label.size())
                    .font_weight(FontWeight::MEDIUM)
                    .child(label),
            )
            .child(div().w(px(280.0)).max_w(px(280.0)).child(popover))
            .into_any_element()
    }

    pub(in crate::app) fn policy_editor_input_row(
        label: &'static str,
        required: bool,
        input: Option<gpui::Entity<crate::subscription_input::SubscriptionTextInput>>,
        show_divider: bool,
        theme: Theme,
    ) -> Div {
        div()
            .min_h(px(82.0))
            .px_4()
            .py_3()
            .border_color(theme.outline_subtle)
            .when(show_divider, gpui::Styled::border_b_1)
            .child(
                div()
                    .mb_2()
                    .flex()
                    .gap_1()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(label)
                    .when(required, |label| {
                        label.child(div().text_color(theme.status_error).child("*"))
                    }),
            )
            .when_some(input, ParentElement::child)
    }

    pub(in crate::app) fn policy_choice_row(
        id: String,
        title: impl Into<gpui::SharedString>,
        selected: bool,
        theme: Theme,
        listener: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
    ) -> Stateful<Div> {
        crate::components::popover_menu_option(id, title, None, selected, true, theme, listener)
    }

    pub(in crate::app) fn policy_icon_choice_row(
        id: String,
        icon: ManagedPolicyIcon,
        title: impl Into<gpui::SharedString>,
        selected: bool,
        theme: Theme,
        listener: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
    ) -> Stateful<Div> {
        use gpui_component::{Icon, IconName, Sizable as _};
        let title = title.into();
        div()
            .id(id)
            .role(Role::RadioButton)
            .aria_toggled(if selected {
                Toggled::True
            } else {
                Toggled::False
            })
            .tab_stop(true)
            .focusable()
            .map(crate::components::primary_button_interaction)
            .cursor_pointer()
            .min_h(px(34.0))
            .px(px(8.0))
            .py(px(4.0))
            .rounded(px(6.0))
            .bg(if selected {
                theme.action_soft
            } else {
                gpui::rgba(0x0000_0000)
            })
            .when(!selected, |row| {
                row.hover(move |style| style.bg(theme.button_hover))
                    .active(move |style| style.bg(theme.button_active))
            })
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(Self::policy_icon_visual(icon, "A", 22.0, theme))
            .child(
                div()
                    .flex_1()
                    .text_size(px(12.5))
                    .line_height(px(18.0))
                    .font_weight(if selected {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_color(if selected {
                        theme.action_primary
                    } else {
                        theme.text_primary
                    })
                    .child(title),
            )
            .when(selected, |row| {
                row.child(
                    Icon::new(IconName::Check)
                        .xsmall()
                        .flex_shrink_0()
                        .text_color(theme.action_primary),
                )
            })
            .on_click(listener)
    }
}
