use super::super::driver::{close_window, refresh, save_screenshot};

#[cfg(target_os = "macos")]
pub(super) fn verify_secondary_click(
    cx: &mut gpui::VisualTestAppContext,
    window: gpui::AnyWindowHandle,
    position: gpui::Point<gpui::Pixels>,
    screenshot: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{Modifiers, MouseButton};
    cx.simulate_mouse_move(window, position, None, Modifiers::none());
    refresh(cx, window)?;
    let hover = cx.capture_screenshot(window)?;
    for button in [MouseButton::Right, MouseButton::Middle] {
        cx.simulate_mouse_down(window, position, button, Modifiers::none());
        refresh(cx, window)?;
        assert_eq!(
            hover,
            cx.capture_screenshot(window)?,
            "{screenshot}: secondary press changed the page"
        );
        cx.simulate_mouse_up(window, position, button, Modifiers::none());
        refresh(cx, window)?;
        assert_eq!(
            hover,
            cx.capture_screenshot(window)?,
            "{screenshot}: secondary click changed the page"
        );
    }
    save_screenshot(cx, window, screenshot)
}

#[cfg(target_os = "macos")]
pub(super) fn manis_root(
    window: &mut gpui::Window,
    cx: &mut gpui::App,
    build: impl FnOnce(&mut gpui::Context<manis_ui::ManisApp>) -> manis_ui::ManisApp + 'static,
) -> gpui::Entity<gpui_component::Root> {
    use gpui::AppContext as _;

    let app = cx.new(build);
    cx.new(|cx| manis_ui::root(app, window, cx))
}

#[cfg(target_os = "macos")]
pub(crate) fn snapshot_hex(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.bytes() {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[cfg(target_os = "macos")]
pub(crate) fn capture(
    cx: &mut gpui::VisualTestAppContext,
    width: f32,
    height: f32,
    file_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::{AnyWindowHandle, px, size};
    use manis_ui::ManisApp;

    let window = cx.open_offscreen_window(size(px(width), px(height)), |window, cx| {
        manis_root(window, cx, |_| {
            ManisApp::with_fixture_controller("http://127.0.0.1:9090")
        })
    })?;
    let window: AnyWindowHandle = window.into();

    refresh(cx, window)?;
    save_screenshot(cx, window, file_name)?;
    close_window(cx, window)
}
