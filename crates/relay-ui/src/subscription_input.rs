use std::ops::Range;

use gpui::{
    App, Bounds, Context, CursorStyle, Element, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Focusable, GlobalElementId, KeyBinding, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ShapedLine,
    SharedString, Style, TextRun, UTF16Selection, UnderlineStyle, Window, actions, div, fill,
    point, prelude::*, px, relative, size,
};

use crate::{subscription::MAX_SUBSCRIPTION_BYTES, theme::Theme};

actions!(
    subscription_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        Paste,
    ]
);

pub(crate) struct SubscriptionInputChanged;

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some("SubscriptionInput")),
        KeyBinding::new("delete", Delete, Some("SubscriptionInput")),
        KeyBinding::new("left", Left, Some("SubscriptionInput")),
        KeyBinding::new("right", Right, Some("SubscriptionInput")),
        KeyBinding::new("shift-left", SelectLeft, Some("SubscriptionInput")),
        KeyBinding::new("shift-right", SelectRight, Some("SubscriptionInput")),
        KeyBinding::new("home", Home, Some("SubscriptionInput")),
        KeyBinding::new("end", End, Some("SubscriptionInput")),
        KeyBinding::new("cmd-a", SelectAll, Some("SubscriptionInput")),
        KeyBinding::new("ctrl-a", SelectAll, Some("SubscriptionInput")),
        KeyBinding::new("cmd-v", Paste, Some("SubscriptionInput")),
        KeyBinding::new("ctrl-v", Paste, Some("SubscriptionInput")),
    ]);
}

pub(crate) struct SubscriptionTextInput {
    focus_handle: FocusHandle,
    content: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    last_scroll_x: Pixels,
    is_selecting: bool,
    theme: Theme,
    dark: bool,
}

impl SubscriptionTextInput {
    pub(crate) fn new(theme: Theme, dark: bool, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: "".into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            last_scroll_x: px(0.0),
            is_selecting: false,
            theme,
            dark,
        }
    }

    pub(crate) fn value(&self) -> &str {
        &self.content
    }

    pub(crate) fn input_focus_handle(&self) -> FocusHandle {
        self.focus_handle.clone()
    }

    pub(crate) fn clear(&mut self, cx: &mut Context<Self>) {
        self.content = "".into();
        self.selected_range = 0..0;
        self.selection_reversed = false;
        self.marked_range = None;
        self.last_layout = None;
        self.last_bounds = None;
        self.last_scroll_x = px(0.0);
        cx.emit(SubscriptionInputChanged);
        cx.notify();
    }

    pub(crate) fn set_theme(&mut self, theme: Theme, dark: bool, cx: &mut Context<Self>) {
        if self.dark != dark {
            self.theme = theme;
            self.dark = dark;
            cx.notify();
        }
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let previous = self.previous_boundary(self.cursor_offset());
            if previous == self.cursor_offset() {
                window.play_system_bell();
                return;
            }
            self.select_to(previous, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let next = self.next_boundary(self.cursor_offset());
            if next == self.cursor_offset() {
                window.play_system_bell();
                return;
            }
            self.select_to(next, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let single_line = text.replace(['\r', '\n'], "");
            self.replace_text_in_range(None, &single_line, window, cx);
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = true;
        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify();
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .char_indices()
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .char_indices()
            .find_map(|(index, _)| (index > offset).then_some(index))
            .unwrap_or(self.content.len())
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        let (Some(bounds), Some(line)) = (self.last_bounds, self.last_layout.as_ref()) else {
            return 0;
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }
        line.closest_index_for_x(position.x - bounds.left() + self.last_scroll_x)
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for character in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += character.len_utf16();
            utf8_offset += character.len_utf8();
        }
        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for character in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += character.len_utf8();
            utf16_offset += character.len_utf16();
        }
        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }
}

impl gpui::EventEmitter<SubscriptionInputChanged> for SubscriptionTextInput {}

impl Focusable for SubscriptionTextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for SubscriptionTextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());
        let single_line = new_text.replace(['\r', '\n'], "");
        let new_len = self.content.len() - (range.end - range.start) + single_line.len();
        if new_len > MAX_SUBSCRIPTION_BYTES {
            window.play_system_bell();
            return;
        }
        self.content = format!(
            "{}{}{}",
            &self.content[..range.start],
            single_line,
            &self.content[range.end..]
        )
        .into();
        let cursor = range.start + single_line.len();
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range = None;
        cx.emit(SubscriptionInputChanged);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());
        let single_line = new_text.replace(['\r', '\n'], "");
        let new_len = self.content.len() - (range.end - range.start) + single_line.len();
        if new_len > MAX_SUBSCRIPTION_BYTES {
            window.play_system_bell();
            return;
        }
        self.content = format!(
            "{}{}{}",
            &self.content[..range.start],
            single_line,
            &self.content[range.end..]
        )
        .into();
        self.marked_range =
            (!single_line.is_empty()).then_some(range.start..range.start + single_line.len());
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|selection| self.range_from_utf16(selection))
            .map_or_else(
                || {
                    let cursor = range.start + single_line.len();
                    cursor..cursor
                },
                |selection| range.start + selection.start..range.start + selection.end,
            );
        self.selection_reversed = false;
        cx.emit(SubscriptionInputChanged);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(
                bounds.left() + line.x_for_index(range.start) - self.last_scroll_x,
                bounds.top(),
            ),
            point(
                bounds.left() + line.x_for_index(range.end) - self.last_scroll_x,
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let line = self.last_layout.as_ref()?;
        let index = line.index_for_x(position.x - bounds.left() + self.last_scroll_x)?;
        Some(self.offset_to_utf16(index))
    }
}

struct SubscriptionTextElement {
    input: Entity<SubscriptionTextInput>,
}

struct PrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
    scroll_x: Pixels,
}

impl IntoElement for SubscriptionTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SubscriptionTextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let display_text: SharedString = if content.is_empty() {
            "https://example.com/subscribe?token=…".into()
        } else {
            content.clone()
        };
        let style = window.text_style();
        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: if content.is_empty() {
                input.theme.text_tertiary.into()
            } else {
                input.theme.text_primary.into()
            },
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if let Some(marked) = input.marked_range.as_ref() {
            vec![
                TextRun {
                    len: marked.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked.end - marked.start,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len() - marked.end,
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![run]
        };
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);
        let cursor_x = line.x_for_index(input.cursor_offset());
        let visible_width = bounds.size.width - px(4.0);
        let scroll_x = (cursor_x - visible_width).max(px(0.0));
        let selected = input.selected_range.clone();
        let (selection, cursor) = if selected.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + cursor_x - scroll_x, bounds.top()),
                        size(px(2.0), bounds.size.height),
                    ),
                    input.theme.action_primary,
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() + line.x_for_index(selected.start) - scroll_x,
                            bounds.top(),
                        ),
                        point(
                            bounds.left() + line.x_for_index(selected.end) - scroll_x,
                            bounds.bottom(),
                        ),
                    ),
                    input.theme.action_soft,
                )),
                None,
            )
        };
        PrepaintState {
            line: Some(line),
            cursor,
            selection,
            scroll_x,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        let line = prepaint.line.take().expect("prepaint always shapes a line");
        line.paint(
            point(bounds.left() - prepaint.scroll_x, bounds.top()),
            window.line_height(),
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        )
        .expect("single-line subscription input should paint");
        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }
        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
            input.last_scroll_x = prepaint.scroll_x;
        });
    }
}

impl gpui::Render for SubscriptionTextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus_handle.is_focused(window);
        div()
            .id("subscription-url-input")
            .key_context("SubscriptionInput")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .h(px(38.0))
            .w_full()
            .px_3()
            .overflow_hidden()
            .rounded_md()
            .border_1()
            .border_color(if focused {
                self.theme.action_primary
            } else {
                self.theme.outline_strong
            })
            .bg(self.theme.surface_high)
            .text_size(px(12.0))
            .line_height(px(36.0))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::paste))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .child(SubscriptionTextElement { input: cx.entity() })
    }
}
