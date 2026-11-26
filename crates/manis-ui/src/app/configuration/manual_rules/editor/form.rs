use super::{
    ActionRole, AnyElement, Button, Context, ControlSize, Div, FluentBuilder, InteractiveElement,
    IntoElement, Language, ManisApp, ManualRulePopover, ParentElement, Radius, Space, Stateful,
    Styled, TextRole, Theme, copy, div, field_label, manual_rule_kind_detail, px,
    style_action_button,
};

impl ManisApp {
    pub(in crate::app) fn manual_rule_kind_menu(
        condition_index: usize,
        selected_kind: crate::manual_rule::ManualRuleKind,
        theme: Theme,
        language: Language,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let mut choices = crate::components::popover_menu_list("manual-rule-kind-choices");
        for kind in crate::manual_rule::ManualRuleKind::ALL {
            // FINAL is configured directly on the pipeline's terminal END fallback card.
            if kind == crate::manual_rule::ManualRuleKind::Final {
                continue;
            }
            let supported = kind.supported_by(manis_core::KernelKind::Mihomo);
            let selected = selected_kind == kind;
            let detail = if supported {
                manual_rule_kind_detail(kind, language)
            } else {
                language.localized(copy::configuration::NO_EXACT_KERNEL_EQUIVALENT)
            };
            choices = choices.child(crate::components::popover_menu_option(
                format!("manual-rule-kind-{condition_index}-{}", kind.storage_key()),
                kind.display_label(),
                Some(detail.into()),
                selected,
                supported,
                theme,
                cx.listener(move |this, _, _, cx| {
                    if supported {
                        this.set_manual_rule_kind(condition_index, kind, cx);
                    }
                }),
            ));
        }
        choices
    }
    pub(in crate::app) fn manual_rule_target_menu(
        &self,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let mut choices = crate::components::popover_menu_list("manual-rule-target-choices");
        for target in self.manual_rule_targets() {
            let selected = self.manual_rule_target == target;
            let row_target = target.clone();
            choices = choices.child(crate::components::popover_menu_option(
                format!("manual-rule-target-{target}"),
                target,
                None,
                selected,
                true,
                theme,
                cx.listener(move |this, _, _, cx| {
                    this.manual_rule_target.clone_from(&row_target);
                    this.manual_rule_error = None;
                    this.manual_rule_popover = None;
                    cx.notify();
                }),
            ));
        }
        choices
    }

    pub(in crate::app) fn manual_rule_select(
        id: &str,
        label: &'static str,
        value: String,
        menu: impl gpui::IntoElement,
        open: bool,
        width: f32,
        on_open_change: impl Fn(&bool, &mut gpui::Window, &mut gpui::App) + 'static,
    ) -> AnyElement {
        let trigger = Button::new(id.to_owned())
            .accessibility_label(label)
            .dropdown_caret(true)
            .w_full()
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(TextRole::Label.size())
                    .line_height(TextRole::Label.line_height())
                    .font_weight(TextRole::Label.weight())
                    .child(value),
            );

        crate::components::anchored_popover(
            format!("{id}-popover"),
            crate::components::style_select_trigger(trigger, ControlSize::Standard),
            menu,
            width,
            360.0,
        )
        .open(open)
        .on_open_change(on_open_change)
        .into_any_element()
    }

    pub(in crate::app) fn manual_rule_condition_editor(
        &self,
        condition_index: usize,
        kind: crate::manual_rule::ManualRuleKind,
        theme: Theme,
        language: Language,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let input = self
            .manual_rule_conditions
            .get(condition_index)
            .expect("manual rule condition input is initialized")
            .input
            .clone();
        let kind_width = if compact { 260.0 } else { 240.0 };
        let kind_popover = ManualRulePopover::Kind(condition_index);
        let kind_open = self.manual_rule_popover == Some(kind_popover);
        let kind_menu = Self::manual_rule_kind_menu(condition_index, kind, theme, language, cx);
        let select_id = format!("manual-rule-kind-select-{condition_index}");
        let label = language.localized(copy::configuration::CHOOSE_CONDITION_TYPE);
        let app = cx.entity();
        let kind_select = Self::manual_rule_select(
            &select_id,
            label,
            kind.display_label().to_owned(),
            kind_menu,
            kind_open,
            kind_width,
            move |open, _, cx| {
                app.update(cx, |this, cx| {
                    this.manual_rule_popover = open.then_some(kind_popover);
                    cx.notify();
                });
            },
        );
        let is_final = kind == crate::manual_rule::ManualRuleKind::Final;
        let mut row = div()
        .mt_3()
        .child(div().child(field_label(
            if condition_index == 0 {
                language.localized(copy::configuration::CONDITION_1).to_owned()
            } else {
                copy::configuration::condition_title(language, condition_index + 1)
            },
            theme,
        )))
        .child(
            div()
                .flex()
                .when(compact, gpui::Styled::flex_col)
                .items_stretch()
                .gap_2()
                .child(
                    div()
                        .when(compact, gpui::Styled::w_full)
                        .when(!compact, |item| item.w(px(220.0)))
                        .flex_shrink_0()
                        .child(kind_select),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .when(!is_final, |field| field.child(input))
                        .when(is_final, |field| {
                            field.child(
                                div()
                                    .h(ControlSize::Standard.height())
                                    .px(Space::Md.px())
                                    .flex()
                                    .items_center()
                                    .rounded(Radius::Control.px())
                                    .bg(theme.surface_low)
                                    .text_size(TextRole::Body.size())
                                    .line_height(TextRole::Body.line_height())
                                    .text_color(theme.text_secondary)
                                    .child(language.localized(copy::configuration::MATCHES_ONLY_AFTER_EVERY_RULE_ABOVE_MISSES)),
                            )
                        }),
                ),
        );
        if condition_index == 0 && !is_final && !self.manual_rule_prefills.is_empty() {
            row = row.child(self.manual_rule_prefill_chips(language, cx));
        }
        if condition_index > 0 {
            row = row.child(
                style_action_button(
                    Button::new(format!("remove-manual-rule-condition-{condition_index}"))
                        .accessibility_label(
                            language.localized(copy::configuration::REMOVE_THIS_CONDITION),
                        )
                        .label(language.localized(copy::configuration::REMOVE_CONDITION))
                        .mt(Space::Sm.px()),
                    ActionRole::SubtleDanger,
                    ControlSize::Compact,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.remove_manual_rule_condition(condition_index, cx);
                })),
            );
        }
        row
    }

    /// The process name and path the connection reported, as one-click chips. Each chip holds the
    /// localized type it fills in, so clicking the second one turns CONDITION 1 into that type.
    fn manual_rule_prefill_chips(&self, language: Language, cx: &mut Context<Self>) -> Div {
        let mut chips = div().flex().flex_wrap().items_center().gap_2();
        for (index, prefill) in self.manual_rule_prefills.iter().enumerate() {
            let label = format!(
                "{} {}",
                manual_rule_kind_detail(prefill.kind, language),
                prefill.value
            );
            chips = chips.child(
                style_action_button(
                    Button::new(format!("manual-rule-prefill-{index}"))
                        .debug_selector(move || format!("manual-rule-prefill-{index}"))
                        .accessibility_label(label.clone())
                        .child(
                            div()
                                .max_w(px(240.0))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(label),
                        ),
                    ActionRole::Secondary,
                    ControlSize::Compact,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.apply_manual_rule_prefill(index, cx);
                })),
            );
        }
        chips.mt(Space::Sm.px())
    }
}
