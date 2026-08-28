use gpui::{ElementId, IntoElement, ParentElement, Styled, prelude::*, px};
use gpui_component::{button::Button, popover::Popover};

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
        Context, InteractiveElement as _, Modifiers, ParentElement as _, Render, Styled as _,
        Window, div, px,
    };
    use gpui_component::button::Button;

    use super::anchored_popover;

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
        std::thread::sleep(Duration::from_millis(180));
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
}
