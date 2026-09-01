use super::super::driver::{close_window, refresh, save_screenshot};

#[cfg(target_os = "macos")]
pub(crate) fn capture_buttons(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AppContext as _, Modifiers, MouseButton, point, px, size};
    for (dark, mode) in [(false, "light"), (true, "dark")] {
        let window = cx
            .open_offscreen_window(size(px(640.0), px(400.0)), |window, cx| {
                let view = cx.new(|cx| manis_ui::button_gallery_fixture(dark, cx));
                cx.new(|cx| gpui_component::Root::new(view, window, cx))
            })?
            .into();
        refresh(cx, window)?;
        save_screenshot(cx, window, &format!("buttons-{mode}-normal.png"))?;
        let normal = cx.capture_screenshot(window)?;
        let scale = normal.width() / 640;
        let position = point(px(60.0), px(70.0));
        cx.simulate_mouse_move(window, position, None, Modifiers::none());
        refresh(cx, window)?;
        let hover = cx.capture_screenshot(window)?;
        assert_ne!(
            normal.get_pixel(30 * scale, 70 * scale),
            hover.get_pixel(30 * scale, 70 * scale),
            "{mode}: hover must change the button fill"
        );
        save_screenshot(cx, window, &format!("buttons-{mode}-hover.png"))?;
        for button in [MouseButton::Right, MouseButton::Middle] {
            cx.simulate_mouse_down(window, position, button, Modifiers::none());
            refresh(cx, window)?;
            let secondary = cx.capture_screenshot(window)?;
            // Ignore the loading spinner elsewhere in the gallery.
            for y in 48 * scale..96 * scale {
                for x in 20 * scale..140 * scale {
                    assert_eq!(
                        hover.get_pixel(x, y),
                        secondary.get_pixel(x, y),
                        "{mode}: {button:?} must not paint a focus ring or pressed state"
                    );
                }
            }
            cx.simulate_mouse_up(window, position, button, Modifiers::none());
        }
        save_screenshot(cx, window, &format!("buttons-{mode}-secondary-click.png"))?;
        cx.simulate_mouse_down(window, position, MouseButton::Left, Modifiers::none());
        refresh(cx, window)?;
        let pressed = cx.capture_screenshot(window)?;
        assert_ne!(
            hover.get_pixel(30 * scale, 70 * scale),
            pressed.get_pixel(30 * scale, 70 * scale),
            "{mode}: pressed must differ from hover"
        );
        save_screenshot(cx, window, &format!("buttons-{mode}-pressed.png"))?;
        cx.simulate_mouse_up(window, position, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(window, point(px(200.0), px(240.0)), None, Modifiers::none());
        refresh(cx, window)?;
        let disabled = cx.capture_screenshot(window)?;
        assert_eq!(
            normal.get_pixel(192 * scale, 240 * scale),
            disabled.get_pixel(192 * scale, 240 * scale),
            "{mode}: disabled buttons must not react to hover"
        );
        cx.simulate_mouse_move(window, point(px(600.0), px(380.0)), None, Modifiers::none());
        cx.simulate_keystrokes(window, "tab");
        cx.update_window(window, |_, window, cx| {
            if window.focused(cx).is_none() {
                window.focus_next(cx);
            }
            assert!(
                window.focused(cx).is_some(),
                "keyboard focus must be reachable"
            );
        })?;
        refresh(cx, window)?;
        save_screenshot(cx, window, &format!("buttons-{mode}-focus.png"))?;
        close_window(cx, window)?;
    }
    Ok(())
}
