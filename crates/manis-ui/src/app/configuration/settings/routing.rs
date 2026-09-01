use super::{
    ActionRole, Context, ControlSize, Div, InteractiveElement, IntoElement, ManisApp, Message,
    ParentElement, Space, StatefulInteractiveElement, Styled, Theme, Window, WindowSizeClass,
    action_button, copy, div, page_heading, px,
};

impl ManisApp {
    pub(in crate::app) fn routing_rules_workspace(
        &mut self,
        theme: Theme,
        size_class: WindowSizeClass,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let compact = size_class == WindowSizeClass::Compact;
        let language = self.language();
        self.ensure_manual_rule_input(theme, window, cx);
        div()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .flex()
        .flex_col()
        .bg(theme.surface_base)
        .child(
            div()
                .flex_shrink_0()
                .p(Space::Lg.px())
                .border_b_1()
                .border_color(theme.outline_subtle)
                .child(page_heading(
                    language.message(Message::RoutingRules),
                    format!(
                        "{} · {}",
                        self.active_rules_summary(language),
                        language.localized(copy::configuration::GROUPS_MATCH_FROM_TOP_TO_BOTTOM_USE_THE_ARROWS_TO),
                    ),
                    Some(
                        action_button(
                            "open-manual-rule-editor",
                            language.message(Message::AddRule),
                            ActionRole::Primary,
                            ControlSize::Compact,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_manual_rule_editor(window, cx);
                        }))
                        .into_any_element(),
                    ),
                    theme,
                )),
        )
        .child(
            div()
                .id("routing-rules-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .p(if compact { Space::Md.px() } else { Space::Lg.px() })
                .child(self.active_rules_panel(theme, language, compact, cx)),
        )
    }
}
