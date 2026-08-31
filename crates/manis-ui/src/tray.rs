use std::time::Duration;

use gpui::{
    App, AppContext, Bounds, Entity, Global, QuitMode, TitlebarOptions, WindowBackgroundAppearance,
    WindowBounds, WindowOptions, px, size,
};
use manis_core::ProxyMode;
#[cfg(not(target_os = "linux"))]
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem},
};

use crate::{
    ManisApp,
    app::ProxyModeBlock,
    localization::{Language, Localizer, copy},
    mihomo,
    theme::LayoutMetric,
};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux::ManisTray;

#[cfg(not(target_os = "linux"))]
const SHOW_MENU_ID: &str = "manis.tray.show";
#[cfg(not(target_os = "linux"))]
const QUIT_MENU_ID: &str = "manis.tray.quit";
#[cfg(not(target_os = "linux"))]
const SYSTEM_PROXY_MENU_ID: &str = "manis.tray.proxy.system";
#[cfg(not(target_os = "linux"))]
const TUN_PROXY_MENU_ID: &str = "manis.tray.proxy.tun";
const TRAY_EVENT_INTERVAL: Duration = Duration::from_millis(100);

#[cfg(not(target_os = "linux"))]
struct ManisTray {
    _icon: TrayIcon,
    show_id: MenuId,
    quit_id: MenuId,
    system_proxy: CheckMenuItem,
    tun_proxy: CheckMenuItem,
    synced: Option<TrayProxySnapshot>,
}

/// Everything the tray check items render, so an unchanged tick costs one comparison instead of
/// four platform menu calls.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TrayProxySnapshot {
    language: Language,
    active: ProxyMode,
    system_block: Option<ProxyModeBlock>,
    tun_block: Option<ProxyModeBlock>,
}

impl Global for ManisTray {}

/// Keeps the product state alive while the native window is closed to the status item.
///
/// A window owns only a presentation of this entity. The application-global handle continues to
/// own the managed kernel and proxy lifecycle until the user explicitly quits Manis.
struct GlobalManisApp(Entity<ManisApp>);

impl Global for GlobalManisApp {}

fn manis_app(cx: &mut App) -> Entity<ManisApp> {
    if let Some(app) = cx.try_global::<GlobalManisApp>() {
        return app.0.clone();
    }
    let app = cx.new(ManisApp::new_with_lifecycle);
    cx.set_global(GlobalManisApp(app.clone()));
    app
}

/// Opens the main Manis window, or activates it if it is already open.
pub fn show_or_open_window(cx: &mut App) {
    if let Some(window) = cx
        .windows()
        .into_iter()
        .find(|window| window.downcast::<gpui_component::Root>().is_some())
    {
        let _ = window.update(cx, |_, window, _| window.activate_window());
        cx.activate(true);
        return;
    }

    if open_window(cx).is_ok() {
        cx.activate(true);
    }
}

/// Opens a new Manis window.
///
/// # Errors
/// Returns the GPUI platform error when the native window cannot be created.
pub fn open_window(cx: &mut App) -> gpui::Result<()> {
    let window_size = size(px(1420.0), px(900.0));
    let bounds = Bounds::centered(None, window_size, cx);
    let app = manis_app(cx);
    cx.open_window(main_window_options(bounds), move |window, cx| {
        cx.new(|cx| crate::root(app, window, cx))
    })?;
    Ok(())
}

fn main_window_options(bounds: Bounds<gpui::Pixels>) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some(crate::brand::PRODUCT_NAME.into()),
            // Integrate the title bar with our opaque chrome; this does not enable blur.
            appears_transparent: true,
            ..Default::default()
        }),
        window_background: WindowBackgroundAppearance::Opaque,
        window_min_size: Some(size(
            LayoutMetric::MinWindowWidth.px(),
            LayoutMetric::MinWindowHeight.px(),
        )),
        is_resizable: true,
        focus: true,
        ..Default::default()
    }
}

/// Installs the native status icon and its menu.
///
/// The icon is created after GPUI's platform event loop has started, which is required by
/// `tray-icon` on macOS and by the native event-loop integration on Windows. Linux uses a
/// D-Bus `StatusNotifierItem` service instead of a GTK event loop.
///
/// # Errors
/// Returns a redacted message when the platform tray cannot be initialized.
pub fn install(cx: &mut App) -> Result<(), &'static str> {
    let store = mihomo::imported_subscription_store_dir().ok();
    install_with_language(cx, Localizer::load(store.as_deref()).language())
}

/// Installs the native status icon and its menu with labels for the selected language.
///
/// # Errors
/// Returns a redacted message when the platform tray cannot be initialized.
pub(crate) fn install_with_language(cx: &mut App, language: Language) -> Result<(), &'static str> {
    #[cfg(target_os = "linux")]
    let tray = ManisTray::new(language)?;
    #[cfg(not(target_os = "linux"))]
    let tray = create_native_tray(language)?;

    cx.set_global(tray);
    // Only hide on close after a tray was successfully registered. Without a compatible desktop
    // host, install fails and the application's normal window-close behavior remains in effect.
    cx.set_quit_mode(QuitMode::Explicit);

    let timer = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        loop {
            timer.timer(TRAY_EVENT_INTERVAL).await;
            let should_quit = cx.update(drain_menu_events);
            if should_quit {
                break;
            }
        }
    })
    .detach();
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn create_native_tray(language: Language) -> Result<ManisTray, &'static str> {
    let show = MenuItem::with_id(
        SHOW_MENU_ID,
        language.localized(copy::tray::OPEN_MANIS),
        true,
        None,
    );
    let status = MenuItem::new(
        language.localized(copy::tray::RULE_ROUTING_STATUS_IS_AVAILABLE_IN_THE_MAIN_WINDOW),
        false,
        None,
    );
    // Both entries start disabled and unchecked; the event loop enables them once a controller is
    // connected, so the tray never offers a switch that would be rejected.
    let system_proxy = CheckMenuItem::with_id(
        SYSTEM_PROXY_MENU_ID,
        proxy_mode_menu_label(language, ProxyMode::System),
        false,
        false,
        None,
    );
    let tun_proxy = CheckMenuItem::with_id(
        TUN_PROXY_MENU_ID,
        proxy_mode_menu_label(language, ProxyMode::Tun),
        false,
        false,
        None,
    );
    let separator = PredefinedMenuItem::separator();
    let proxy_separator = PredefinedMenuItem::separator();
    let quit = MenuItem::with_id(
        QUIT_MENU_ID,
        language.localized(copy::tray::QUIT_MANIS),
        true,
        None,
    );
    let menu = Menu::with_items(&[
        &show,
        &status,
        &proxy_separator,
        &system_proxy,
        &tun_proxy,
        &separator,
        &quit,
    ])
    .map_err(|_error| language.localized(copy::tray::COULD_NOT_CREATE_THE_SYSTEM_TRAY_MENU))?;
    let icon = Icon::from_rgba(manis_icon_rgba(), 32, 32)
        .map_err(|_error| language.localized(copy::tray::COULD_NOT_CREATE_THE_SYSTEM_TRAY_ICON))?;
    let tray = TrayIconBuilder::new()
        .with_id("manis.status")
        .with_tooltip(language.localized(copy::tray::MANIS_RULE_ROUTING))
        .with_icon(icon)
        .with_icon_as_template(cfg!(target_os = "macos"))
        .with_menu(Box::new(menu))
        .build()
        .map_err(|_error| language.localized(copy::tray::SYSTEM_TRAY_IS_UNAVAILABLE))?;

    Ok(ManisTray {
        _icon: tray,
        show_id: show.id().clone(),
        quit_id: quit.id().clone(),
        system_proxy,
        tun_proxy,
        synced: None,
    })
}

#[cfg(not(target_os = "linux"))]
fn drain_menu_events(cx: &mut App) -> bool {
    let (show_id, quit_id, system_id, tun_id) = {
        let tray = cx.global::<ManisTray>();
        (
            tray.show_id.clone(),
            tray.quit_id.clone(),
            tray.system_proxy.id().clone(),
            tray.tun_proxy.id().clone(),
        )
    };
    let mut should_quit = false;
    while let Ok(event) = MenuEvent::receiver().try_recv() {
        if event.id == show_id {
            show_or_open_window(cx);
        } else if event.id == quit_id {
            should_quit = true;
        } else if event.id == system_id {
            request_proxy_mode(cx, ProxyMode::System);
        } else if event.id == tun_id {
            request_proxy_mode(cx, ProxyMode::Tun);
        }
    }
    if should_quit {
        cx.quit();
        return true;
    }
    sync_proxy_menu(cx);
    false
}

#[cfg(target_os = "linux")]
fn drain_menu_events(cx: &mut App) -> bool {
    let events = cx.global::<ManisTray>().events();
    for event in events {
        match event {
            linux::TrayAction::Show => show_or_open_window(cx),
            linux::TrayAction::Quit => {
                cx.quit();
                return true;
            }
            linux::TrayAction::ProxyMode(mode) => request_proxy_mode(cx, mode),
        }
    }
    sync_proxy_menu(cx);
    false
}

/// Applies the mode a tray check item stands for, clearing it when it is already active.
///
/// `muda` flips the check mark as soon as the item is clicked. The following sync pass restores
/// the mark to the mode that is actually in effect, so a switch that fails or is still running
/// never reads as applied.
fn request_proxy_mode(cx: &mut App, selected: ProxyMode) {
    let app = manis_app(cx);
    app.update(cx, |app, cx| app.toggle_proxy_mode(selected, cx));
}

/// Mirrors the live proxy mode onto the tray check items.
fn sync_proxy_menu(cx: &mut App) {
    let Some(app) = cx.try_global::<GlobalManisApp>().map(|app| app.0.clone()) else {
        return;
    };
    let snapshot = {
        let app = app.read(cx);
        let active = app.active_proxy_mode();
        TrayProxySnapshot {
            language: app.language(),
            active,
            // Each reason is read for the switch that click would actually request, so turning a
            // mode off stays available even when turning it on would not be.
            system_block: app.proxy_mode_block(active.toggled(ProxyMode::System)),
            tun_block: app.proxy_mode_block(active.toggled(ProxyMode::Tun)),
        }
    };

    let tray = cx.global_mut::<ManisTray>();
    if tray.synced == Some(snapshot) {
        return;
    }
    tray.synced = Some(snapshot);
    #[cfg(target_os = "linux")]
    tray.sync(snapshot);
    #[cfg(not(target_os = "linux"))]
    for (item, mode, block) in [
        (&tray.system_proxy, ProxyMode::System, snapshot.system_block),
        (&tray.tun_proxy, ProxyMode::Tun, snapshot.tun_block),
    ] {
        item.set_checked(snapshot.active == mode);
        item.set_enabled(block.is_none());
        item.set_text(tray_menu_label(snapshot.language, mode, block));
    }
}

fn proxy_mode_menu_label(language: Language, mode: ProxyMode) -> &'static str {
    match mode {
        ProxyMode::Off => language.localized(copy::common::OFF),
        ProxyMode::System => language.localized(copy::common::SYSTEM_PROXY),
        ProxyMode::Tun => language.localized(copy::common::TUN_PROXY),
    }
}

fn tray_menu_label(language: Language, mode: ProxyMode, block: Option<ProxyModeBlock>) -> String {
    let label = proxy_mode_menu_label(language, mode);
    match block {
        None => label.to_owned(),
        Some(block) => format!("{label} — {}", block.tray_reason(language)),
    }
}

fn manis_icon_rgba() -> Vec<u8> {
    const SIZE: u32 = 32;
    let mut rgba = vec![0; (SIZE * SIZE * 4) as usize];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = i64::from(x) - 16;
            let dy = i64::from(y) - 16;
            let radius_squared = dx * dx + dy * dy;
            let route = (8..=24).contains(&x) && (13..=17).contains(&y)
                || (20..=24).contains(&x) && (8..=17).contains(&y);
            let visible = (94..=177).contains(&radius_squared) || route;
            if visible {
                let index = ((y * SIZE + x) * 4) as usize;
                rgba[index] = 23;
                rgba[index + 1] = 108;
                rgba[index + 2] = 98;
                rgba[index + 3] = 255;
            }
        }
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::{main_window_options, manis_icon_rgba};

    #[test]
    fn main_window_uses_an_opaque_native_backdrop() {
        let options = main_window_options(gpui::Bounds::default());
        assert!(matches!(
            options.window_background,
            gpui::WindowBackgroundAppearance::Opaque
        ));
        assert!(options.titlebar.unwrap().appears_transparent);
    }

    #[test]
    fn tray_icon_is_rgba_and_contains_transparency() {
        let icon = manis_icon_rgba();
        assert_eq!(icon.len(), 32 * 32 * 4);
        assert!(icon.iter().skip(3).step_by(4).any(|alpha| *alpha == 0));
        assert!(icon.iter().skip(3).step_by(4).any(|alpha| *alpha == 255));
    }
}
