use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement, ParentElement,
    SharedString, Styled, Subscription, Window, div, prelude::*,
};
use gpui_component::{
    Sizable,
    input::{Input, InputEvent, InputState},
};

use crate::{
    localization::Language,
    subscription::MAX_SUBSCRIPTION_BYTES,
    theme::{ControlSize, Radius, TextRole, Theme},
};

pub(crate) struct SubscriptionInputChanged;
pub(crate) struct SubscriptionInputSubmitted;

#[derive(Clone, Copy, Eq, PartialEq)]
enum InputAvailability {
    Enabled,
    Disabled,
}

pub(crate) struct SubscriptionTextInput {
    state: Entity<InputState>,
    _state_events: Subscription,
    element_id: SharedString,
    content: SharedString,
    placeholder: SharedString,
    max_bytes: usize,
    pending_value: Option<SharedString>,
    pending_placeholder: Option<SharedString>,
    availability: InputAvailability,
    theme: Theme,
    dark: bool,
}

impl SubscriptionTextInput {
    pub(crate) fn new_with_language(
        language: Language,
        theme: Theme,
        dark: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_field(
            "subscription-url-input",
            subscription_placeholder(language),
            MAX_SUBSCRIPTION_BYTES,
            theme,
            dark,
            window,
            cx,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_field(
        element_id: impl Into<SharedString>,
        placeholder: impl Into<SharedString>,
        max_bytes: usize,
        theme: Theme,
        dark: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = placeholder.into();
        let state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(placeholder.clone())
                .validate(move |value, _| value.len() <= max_bytes)
        });
        let state_events =
            cx.subscribe(&state, |this, state, event: &InputEvent, cx| match event {
                InputEvent::Change => {
                    this.content = state.read(cx).value();
                    cx.emit(SubscriptionInputChanged);
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => {
                    if this.availability == InputAvailability::Enabled {
                        cx.emit(SubscriptionInputSubmitted);
                    }
                }
                InputEvent::Focus | InputEvent::Blur => {}
            });

        Self {
            state,
            _state_events: state_events,
            element_id: element_id.into(),
            content: "".into(),
            placeholder,
            max_bytes,
            pending_value: None,
            pending_placeholder: None,
            availability: InputAvailability::Enabled,
            theme,
            dark,
        }
    }

    pub(crate) fn value(&self) -> &str {
        &self.content
    }

    pub(crate) fn set_value_without_event(
        &mut self,
        value: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        let value = value.into();
        self.set_content_without_event(clamp_to_byte_limit(&value, self.max_bytes), cx);
    }

    pub(crate) fn clear(&mut self, cx: &mut Context<Self>) {
        self.set_content_without_event(SharedString::default(), cx);
        cx.emit(SubscriptionInputChanged);
    }

    pub(crate) fn clear_without_event(&mut self, cx: &mut Context<Self>) {
        self.set_content_without_event(SharedString::default(), cx);
    }

    fn set_content_without_event(&mut self, value: SharedString, cx: &mut Context<Self>) {
        self.content = value;
        self.pending_value = Some(self.content.clone());
        cx.notify();
    }

    pub(crate) fn set_theme(&mut self, theme: Theme, dark: bool, cx: &mut Context<Self>) {
        if self.dark != dark {
            self.theme = theme;
            self.dark = dark;
            cx.notify();
        }
    }

    pub(crate) fn set_language(&mut self, language: Language, cx: &mut Context<Self>) {
        self.set_placeholder(subscription_placeholder(language), cx);
    }

    pub(crate) fn set_placeholder(
        &mut self,
        placeholder: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        let placeholder = placeholder.into();
        if self.placeholder != placeholder {
            self.placeholder = placeholder;
            self.pending_placeholder = Some(self.placeholder.clone());
            cx.notify();
        }
    }

    pub(crate) fn set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let availability = if enabled {
            InputAvailability::Enabled
        } else {
            InputAvailability::Disabled
        };
        if self.availability != availability {
            self.availability = availability;
            self.state.update(cx, |state, cx| {
                state.set_disabled(availability == InputAvailability::Disabled, cx);
            });
            cx.notify();
        }
    }

    fn sync_pending(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(placeholder) = self.pending_placeholder.take() {
            self.state.update(cx, |state, cx| {
                state.set_placeholder(placeholder, window, cx);
            });
        }
        if let Some(value) = self.pending_value.take() {
            self.state.update(cx, |state, cx| {
                state.set_value(value, window, cx);
            });
        }
    }
}

fn clamp_offset(content: &str, offset: usize) -> usize {
    let mut offset = offset.min(content.len());
    while !content.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn clamp_to_byte_limit(value: &str, max_bytes: usize) -> SharedString {
    let end = clamp_offset(value, max_bytes.min(value.len()));
    value[..end].into()
}

fn subscription_placeholder(language: Language) -> &'static str {
    language.text(
        "Paste an HTTP/HTTPS subscription or vless:// node link",
        "粘贴 HTTP/HTTPS 订阅或 vless:// 节点链接",
    )
}

impl EventEmitter<SubscriptionInputChanged> for SubscriptionTextInput {}
impl EventEmitter<SubscriptionInputSubmitted> for SubscriptionTextInput {}

impl Focusable for SubscriptionTextInput {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.state.read(cx).focus_handle(cx)
    }
}

impl gpui::Render for SubscriptionTextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_pending(window, cx);
        let focused = self.focus_handle(cx).is_focused(window);
        let disabled = self.availability == InputAvailability::Disabled;
        let control_height = ControlSize::Standard.height();
        let text_role = TextRole::Label;
        let mut input = Input::new(&self.state)
            .aria_label(self.placeholder.clone())
            .disabled(disabled)
            .with_size(control_height)
            .w_full()
            .px_3()
            .text_size(text_role.size())
            .line_height(text_role.line_height())
            .font_weight(text_role.weight())
            .rounded(Radius::Control.px())
            .border_1()
            .border_color(if focused {
                self.theme.action_primary
            } else {
                self.theme.outline_strong
            })
            .bg(self.theme.surface_high)
            .text_color(self.theme.text_primary);

        // `Input::h` configures multiline height. Set the single-line frame
        // through its style so it matches the other standard controls.
        input.style().size.height = Some(control_height.into());

        div()
            .id(self.element_id.clone())
            .h(control_height)
            .w_full()
            .text_size(text_role.size())
            .child(input)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gpui::{AppContext, Context, Entity, ParentElement, Render, Styled, Window, div, px};

    use super::{
        SubscriptionInputChanged, SubscriptionInputSubmitted, SubscriptionTextInput, clamp_offset,
        clamp_to_byte_limit,
    };
    use crate::theme::Theme;

    struct InputHarness {
        input: Entity<SubscriptionTextInput>,
    }

    impl Render for InputHarness {
        fn render(
            &mut self,
            _window: &mut Window,
            _cx: &mut Context<Self>,
        ) -> impl gpui::IntoElement {
            div().size_full().p(px(20.0)).child(self.input.clone())
        }
    }

    #[test]
    fn stale_selection_on_empty_placeholder_is_clamped_before_editing() {
        assert_eq!(clamp_offset("", 14), 0);
    }

    #[test]
    fn edit_ranges_are_clamped_to_utf8_boundaries() {
        assert_eq!(clamp_offset("中a", 99), 4);
        assert_eq!(clamp_offset("中a", 2), 0);
    }

    #[test]
    fn programmatic_values_are_clamped_to_utf8_boundaries() {
        assert_eq!(clamp_to_byte_limit("中a", 2).as_ref(), "");
        assert_eq!(clamp_to_byte_limit("中a", 3).as_ref(), "中");
        assert_eq!(clamp_to_byte_limit("中a", 4).as_ref(), "中a");
    }

    #[gpui::test]
    fn silent_set_and_clear_update_value_without_change_event(cx: &mut gpui::TestAppContext) {
        cx.update(gpui_component::init);

        let changed = Rc::new(Cell::new(0));
        let mut input = None;
        let changed_events = changed.clone();
        let (_, window_cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|cx| {
                SubscriptionTextInput::new_field(
                    "test-input",
                    "placeholder",
                    4,
                    Theme::light(),
                    false,
                    window,
                    cx,
                )
            });
            cx.subscribe(&entity, move |_, _, _: &SubscriptionInputChanged, _| {
                changed_events.set(changed_events.get() + 1);
            })
            .detach();
            input = Some(entity.clone());
            InputHarness { input: entity }
        });
        let input = input.expect("test input should be created");

        window_cx.update(|window, cx| {
            input.update(cx, |input, cx| input.set_value_without_event("中a", cx));
            window.draw(cx).clear(cx);
            input.read_with(cx, |input, _| assert_eq!(input.value(), "中a"));
        });
        assert_eq!(changed.get(), 0);

        window_cx.update(|window, cx| {
            input.update(cx, SubscriptionTextInput::clear_without_event);
            window.draw(cx).clear(cx);
            input.read_with(cx, |input, _| assert_eq!(input.value(), ""));
        });
        assert_eq!(changed.get(), 0);
    }

    #[gpui::test]
    fn input_state_events_are_mapped_to_wrapper_events(cx: &mut gpui::TestAppContext) {
        cx.update(gpui_component::init);

        let changed = Rc::new(Cell::new(0));
        let submitted = Rc::new(Cell::new(0));
        let mut input = None;
        let changed_events = changed.clone();
        let submitted_events = submitted.clone();
        let (_, window_cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|cx| {
                SubscriptionTextInput::new_field(
                    "test-input",
                    "placeholder",
                    128,
                    Theme::light(),
                    false,
                    window,
                    cx,
                )
            });
            cx.subscribe(&entity, move |_, _, _: &SubscriptionInputChanged, _| {
                changed_events.set(changed_events.get() + 1);
            })
            .detach();
            cx.subscribe(&entity, move |_, _, _: &SubscriptionInputSubmitted, _| {
                submitted_events.set(submitted_events.get() + 1);
            })
            .detach();
            input = Some(entity.clone());
            InputHarness { input: entity }
        });
        let input = input.expect("test input should be created");

        window_cx.update(|window, cx| {
            input.update(cx, |input, cx| {
                let state = input.state.clone();
                state.update(cx, |state, cx| {
                    state.set_value("typed", window, cx);
                    cx.emit(gpui_component::input::InputEvent::Change);
                });
            });
        });
        let _ = window_cx;
        cx.run_until_parked();
        input.read_with(cx, |input, _| assert_eq!(input.value(), "typed"));
        assert_eq!(changed.get(), 1);

        input.update(cx, |input, cx| {
            let state = input.state.clone();
            state.update(cx, |_, cx| {
                cx.emit(gpui_component::input::InputEvent::PressEnter {
                    secondary: false,
                    shift: false,
                });
            });
        });
        cx.run_until_parked();
        assert_eq!(submitted.get(), 1);
    }
}
