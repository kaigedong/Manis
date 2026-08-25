use gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use gpui_platform::application;
use relay_ui::{RelayApp, init};

fn main() {
    application().run(|cx: &mut App| {
        init(cx);
        let window_size = size(px(1420.0), px(900.0));
        let bounds = Bounds::centered(None, window_size, cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(640.0), px(560.0))),
                is_resizable: true,
                focus: true,
                ..Default::default()
            },
            |_, cx| cx.new(|_| RelayApp::new()),
        )
        .expect("failed to open Relay GPUI window");

        cx.activate(true);
    });
}
