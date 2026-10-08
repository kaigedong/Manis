#[cfg(target_os = "macos")]
pub(crate) fn capture_utility_windows(
    cx: &mut gpui::VisualTestAppContext,
) -> Result<(), Box<dyn std::error::Error>> {
    use super::super::driver::{close_window, refresh, save_screenshot};
    use gpui::{AppContext as _, px, size};
    use manis_ui::UtilityWindowFixture as Kind;
    for (kind, dark, width, height, name) in [
        (
            Kind::Settings,
            false,
            980.,
            760.,
            "settings-window-light.png",
        ),
        (Kind::Settings, true, 980., 760., "settings-window-dark.png"),
        (
            Kind::Settings,
            false,
            480.,
            560.,
            "settings-window-minimum.png",
        ),
        (
            Kind::ProxySource,
            false,
            720.,
            660.,
            "source-window-light.png",
        ),
        (
            Kind::ProxySource,
            true,
            480.,
            560.,
            "source-window-minimum-dark.png",
        ),
        (
            Kind::RuleSource,
            false,
            480.,
            600.,
            "rules-source-window-minimum.png",
        ),
        (
            Kind::ManualRule,
            false,
            720.,
            660.,
            "manual-rule-window-light.png",
        ),
        (
            Kind::ManualRule,
            true,
            480.,
            600.,
            "manual-rule-window-minimum-dark.png",
        ),
        (Kind::Policy, false, 720., 660., "policy-window-light.png"),
        (
            Kind::Policy,
            true,
            480.,
            600.,
            "policy-window-minimum-dark.png",
        ),
    ] {
        let window = cx.open_offscreen_window(size(px(width), px(height)), move |window, cx| {
            let app = cx.new(|_| manis_ui::ManisApp::with_fixture_controller("http://127.0.0.1:1"));
            manis_ui::utility_window_fixture_root(app, kind, dark, window, cx)
        })?;
        refresh(cx, window.into())?;
        save_screenshot(cx, window.into(), name)?;
        close_window(cx, window.into())?;
    }
    Ok(())
}
