use gpui::{
    AnyElement, Div, ElementId, IntoElement, ParentElement, SharedString, Styled, div, prelude::*,
    px,
};
use gpui_component::{
    Sizable as _,
    button::{Button, ButtonVariant, ButtonVariants as _},
    dialog::Dialog,
    popover::Popover,
};

use crate::theme::{ControlSize, Radius, Space, TextRole, Theme};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ActionRole {
    Primary,
    Secondary,
    Quiet,
    Danger,
}

impl ActionRole {
    pub(crate) fn variant(self) -> ButtonVariant {
        match self {
            Self::Primary => ButtonVariant::Primary,
            Self::Secondary => ButtonVariant::Secondary,
            Self::Quiet => ButtonVariant::Ghost,
            Self::Danger => ButtonVariant::Danger,
        }
    }
}

pub(crate) fn action_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    role: ActionRole,
    size: ControlSize,
) -> Button {
    style_action_button(Button::new(id).label(label), role, size)
}

pub(crate) fn style_action_button(button: Button, role: ActionRole, size: ControlSize) -> Button {
    button
        .with_variant(role.variant())
        .with_size(size.component_size())
        .h(size.height())
        .rounded(Radius::Control.px())
}

pub(crate) fn surface_dialog(dialog: Dialog, theme: Theme) -> Dialog {
    dialog
        .p_0()
        .rounded(Radius::Pane.px())
        .border_1()
        .border_color(theme.outline_subtle)
        .bg(theme.surface_overlay)
        .overflow_hidden()
}

pub(crate) fn dialog_header_surface(theme: Theme) -> Div {
    div()
        .flex_shrink_0()
        .px_5()
        .py_4()
        .border_b_1()
        .border_color(theme.outline_subtle)
        .bg(theme.surface_overlay)
}

pub(crate) fn dialog_footer_surface(theme: Theme) -> Div {
    div()
        .flex_shrink_0()
        .px_5()
        .py_3()
        .border_t_1()
        .border_color(theme.outline_subtle)
        .bg(theme.surface_overlay)
        .flex()
        .items_center()
        .justify_end()
        .gap_2()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StatusTone {
    Neutral,
    Success,
    Warning,
    Error,
    Route,
}

impl StatusTone {
    fn colors(self, theme: Theme) -> (gpui::Rgba, gpui::Rgba) {
        match self {
            Self::Neutral => (theme.text_secondary, theme.surface_low),
            Self::Success => (theme.status_success, theme.surface_low),
            Self::Warning => (theme.status_warning, theme.surface_low),
            Self::Error => (theme.status_error, theme.surface_low),
            Self::Route => (theme.route_trace, theme.route_soft),
        }
    }
}

pub(crate) fn status_badge(label: impl Into<SharedString>, tone: StatusTone, theme: Theme) -> Div {
    let (foreground, background) = tone.colors(theme);
    div()
        .flex()
        .items_center()
        .h(TextRole::Label.line_height() + Space::Sm.px())
        .px(Space::Sm.px())
        .rounded(Radius::Control.px())
        .bg(background)
        .text_color(foreground)
        .text_size(TextRole::Label.size())
        .line_height(TextRole::Label.line_height())
        .font_weight(TextRole::Label.weight())
        .child(label.into())
}

pub(crate) fn page_heading(
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    action: Option<AnyElement>,
    theme: Theme,
) -> Div {
    heading(
        title,
        detail,
        action,
        theme,
        TextRole::PageTitle,
        TextRole::Body,
    )
}

pub(crate) fn section_heading(
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    action: Option<AnyElement>,
    theme: Theme,
) -> Div {
    heading(
        title,
        detail,
        action,
        theme,
        TextRole::SectionTitle,
        TextRole::Metadata,
    )
}

fn heading(
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    action: Option<AnyElement>,
    theme: Theme,
    title_role: TextRole,
    detail_role: TextRole,
) -> Div {
    let detail = detail.into();
    div()
        .flex()
        .flex_wrap()
        .w_full()
        .items_start()
        .justify_between()
        .gap(Space::Lg.px())
        .child(
            div()
                .flex_1()
                // Actions wrap before explanatory text becomes a narrow column.
                .min_w(px(180.0))
                .child(
                    div()
                        .text_color(theme.text_primary)
                        .text_size(title_role.size())
                        .line_height(title_role.line_height())
                        .font_weight(title_role.weight())
                        .child(title.into()),
                )
                .when(!detail.as_ref().is_empty(), |this| {
                    this.child(
                        div()
                            .mt(Space::Xs.px())
                            .text_color(theme.text_secondary)
                            .text_size(detail_role.size())
                            .line_height(detail_role.line_height())
                            .child(detail),
                    )
                }),
        )
        .when_some(action, |this, action| this.flex_none().child(action))
}

pub(crate) fn empty_state(
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    action: Option<AnyElement>,
    theme: Theme,
) -> Div {
    let detail = detail.into();
    div()
        .w_full()
        .p(Space::Xl.px())
        .child(
            div()
                .text_color(theme.text_primary)
                .text_size(TextRole::SectionTitle.size())
                .line_height(TextRole::SectionTitle.line_height())
                .font_weight(TextRole::SectionTitle.weight())
                .child(title.into()),
        )
        .when(!detail.as_ref().is_empty(), |this| {
            this.child(
                div()
                    .mt(Space::Sm.px())
                    .max_w(px(520.0))
                    .text_color(theme.text_secondary)
                    .text_size(TextRole::Body.size())
                    .line_height(TextRole::Body.line_height())
                    .child(detail),
            )
        })
        .when_some(action, |this, action| {
            this.child(div().mt(Space::Lg.px()).flex().items_center().child(action))
        })
}

/// Builds a dropdown whose position is measured from its trigger bounds.
///
/// Business screens still own their option rows, while `gpui-component` owns
/// trigger interaction, focus/dismissal, window-edge collision, and popup
/// placement. Keeping that boundary here prevents screens from reintroducing
/// guessed pixel offsets.
pub(crate) fn anchored_popover(
    id: impl Into<ElementId>,
    trigger: Button,
    content: impl IntoElement,
    width: f32,
    max_height: f32,
) -> Popover {
    Popover::new(id).trigger(trigger).w(px(width)).p_0().child(
        gpui::div()
            .id("component-popover-scroll")
            .w_full()
            .max_h(px(max_height))
            .overflow_y_scroll()
            .child(content),
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gpui::{
        Context, InteractiveElement as _, IntoElement as _, Modifiers, ParentElement as _, Render,
        Styled as _, Window, div, px,
    };
    use gpui_component::button::{Button, ButtonVariant};

    use super::{
        ActionRole, StatusTone, action_button, anchored_popover, empty_state, page_heading,
        status_badge,
    };
    use crate::theme::{ControlSize, Theme};

    #[test]
    fn action_roles_map_to_component_button_variants() {
        assert_eq!(ActionRole::Primary.variant(), ButtonVariant::Primary);
        assert_eq!(ActionRole::Secondary.variant(), ButtonVariant::Secondary);
        assert_eq!(ActionRole::Quiet.variant(), ButtonVariant::Ghost);
        assert_eq!(ActionRole::Danger.variant(), ButtonVariant::Danger);
    }

    struct PopoverHarness;

    impl Render for PopoverHarness {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
            div().size_full().p(px(80.)).child(anchored_popover(
                "positioned-popover",
                Button::new("positioned-trigger")
                    .debug_selector(|| "positioned-trigger".into())
                    .label("Open")
                    .w(px(180.))
                    .h(px(38.)),
                div()
                    .debug_selector(|| "positioned-content".into())
                    .h(px(80.)),
                180.,
                240.,
            ))
        }
    }

    #[gpui::test]
    fn popup_is_anchored_directly_below_its_trigger(cx: &mut gpui::TestAppContext) {
        cx.update(gpui_component::init);
        let (_, cx) = cx.add_window_view(|_, _| PopoverHarness);
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let trigger = cx
            .debug_bounds("positioned-trigger")
            .expect("trigger should render");
        cx.simulate_click(trigger.center(), Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.executor().advance_clock(Duration::from_millis(180));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let content = cx
            .debug_bounds("positioned-content")
            .expect("popover content should render after clicking the trigger");
        let vertical_gap = content.origin.y - (trigger.origin.y + trigger.size.height);
        let horizontal_drift = content.origin.x - trigger.origin.x;

        assert!(
            (px(0.)..=px(8.)).contains(&vertical_gap),
            "popover gap should be the component spacing, got {vertical_gap:?}",
        );
        assert!(
            horizontal_drift.abs() <= px(1.),
            "popover should align with its trigger, drifted by {horizontal_drift:?}",
        );
    }

    struct FoundationHarness;

    impl Render for FoundationHarness {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
            let theme = Theme::light();
            div()
                .size_full()
                .p(px(40.0))
                .child(page_heading(
                    "Rules",
                    "Route traffic into policy groups.",
                    Some(
                        action_button(
                            "heading-action",
                            "Add",
                            ActionRole::Primary,
                            ControlSize::Standard,
                        )
                        .debug_selector(|| "heading-action".into())
                        .into_any_element(),
                    ),
                    theme,
                ))
                .child(
                    status_badge("Observed", StatusTone::Route, theme)
                        .debug_selector(|| "route-badge".into()),
                )
                .child(
                    empty_state("No traffic yet", "Connections appear here.", None, theme)
                        .debug_selector(|| "empty-state".into()),
                )
        }
    }

    #[gpui::test]
    fn shared_foundation_components_render_their_required_parts(cx: &mut gpui::TestAppContext) {
        cx.update(gpui_component::init);
        let (_, cx) = cx.add_window_view(|_, _| FoundationHarness);
        cx.update(|window, cx| window.draw(cx).clear(cx));

        assert!(cx.debug_bounds("heading-action").is_some());
        assert!(cx.debug_bounds("route-badge").is_some());
        assert!(cx.debug_bounds("empty-state").is_some());
    }
}
