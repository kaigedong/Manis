use gpui_component::IconName;

use super::{
    ActionRole, Context, ControlSize, Div, InteractiveElement, IntoElement, Language, ManisApp,
    Message, ParentElement, Space, StatefulInteractiveElement, Styled, Theme, Window,
    WindowSizeClass, action_button, copy, div, px,
};
use crate::components::section_header;

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
        if self.topology.rules_view == crate::app::WorkspaceViewMode::Tree {
            return div()
                .flex_1()
                .min_w(px(0.0))
                .min_h(px(0.0))
                .h_full()
                .overflow_hidden()
                .flex()
                .flex_col()
                .bg(theme.surface_base)
                .child(self.rules_tree_workspace(theme, size_class, cx));
        }
        // The pipeline header names the ordering model and owns both creation actions; the
        // command bar above already carries the workspace title and the rule counts.
        let actions = div()
            .flex()
            .items_center()
            .gap(Space::Sm.px())
            .child(
                action_button(
                    "open-local-rule-group-editor",
                    language.localized(copy::configuration::ADD_LOCAL_RULE_GROUP),
                    ActionRole::Secondary,
                    ControlSize::Compact,
                )
                .icon(IconName::Plus)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_local_rule_group_editor_new(window, cx);
                })),
            )
            .child(
                action_button(
                    "open-manual-rule-editor",
                    language.message(Message::AddRule),
                    ActionRole::Primary,
                    ControlSize::Compact,
                )
                .icon(IconName::Plus)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_manual_rule_editor(window, cx);
                })),
            )
            .into_any_element();
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
                    .px(if compact {
                        Space::Md.px()
                    } else {
                        Space::Xl.px()
                    })
                    .pt(if compact {
                        Space::Md.px()
                    } else {
                        Space::Lg.px()
                    })
                    .child(section_header(
                        pipeline_title(language),
                        language.localized(
                            copy::configuration::GROUPS_MATCH_FROM_TOP_TO_BOTTOM_DRAG_TO_REORDER,
                        ),
                        Some(actions),
                        theme,
                    )),
            )
            .child(
                self.rule_group_scroll_target(
                    div()
                        .id("routing-rules-scroll")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .p(if compact {
                            Space::Md.px()
                        } else {
                            Space::Lg.px()
                        })
                        .child(self.active_rules_panel(theme, language, compact, cx)),
                    cx,
                ),
            )
    }
}

/// The pipeline names its own ordering model, so the header is a section title rather than a
/// second page title.
fn pipeline_title(language: Language) -> &'static str {
    match language {
        Language::English => "Priority pipeline · evaluated top to bottom",
        Language::SimplifiedChinese => "优先级管道（自上而下匹配）",
    }
}
