use std::time::Duration;

use gpui::{App, AppContext, Bounds, Global, QuitMode, WindowBounds, WindowOptions, px, size};
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem},
};

use crate::{
    RelayApp,
    localization::{Language, Localizer},
    mihomo,
};

const SHOW_MENU_ID: &str = "relay.tray.show";
const QUIT_MENU_ID: &str = "relay.tray.quit";
const TRAY_EVENT_INTERVAL: Duration = Duration::from_millis(100);

struct RelayTray {
    _icon: TrayIcon,
    show_id: MenuId,
    quit_id: MenuId,
}

impl Global for RelayTray {}

/// Opens the main Relay window, or activates it if it is already open.
pub fn show_or_open_window(cx: &mut App) {
    if let Some(window) = cx
        .windows()
        .into_iter()
        .find(|window| window.downcast::<RelayApp>().is_some())
    {
        let _ = window.update(cx, |_, window, _| window.activate_window());
        cx.activate(true);
        return;
    }

    if open_window(cx).is_ok() {
        cx.activate(true);
    }
}

/// Opens a new Relay window.
///
/// # Errors
/// Returns the GPUI platform error when the native window cannot be created.
pub fn open_window(cx: &mut App) -> gpui::Result<()> {
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
    )?;
    Ok(())
}

/// Installs the native status icon and its menu.
///
/// The icon is created after GPUI's platform event loop has started, which is required by
/// `tray-icon` on macOS and by the native event-loop integrations on Windows and Linux.
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
    gtk::init().map_err(|_error| {
        language.text(
            "Could not initialize the Linux GTK tray event loop",
            "无法初始化 Linux GTK 托盘事件循环",
        )
    })?;

    let show = MenuItem::with_id(
        SHOW_MENU_ID,
        language.text("Open Relay", "打开 Relay"),
        true,
        None,
    );
    let status = MenuItem::new(
        language.text(
            "Rule routing · status is available in the main window",
            "规则路由 · 状态请在主窗口查看",
        ),
        false,
        None,
    );
    let separator = PredefinedMenuItem::separator();
    let quit = MenuItem::with_id(
        QUIT_MENU_ID,
        language.text("Quit Relay", "退出 Relay"),
        true,
        None,
    );
    let menu = Menu::with_items(&[&show, &status, &separator, &quit]).map_err(|_error| {
        language.text(
            "Could not create the system tray menu",
            "无法创建系统托盘菜单",
        )
    })?;
    let icon = Icon::from_rgba(relay_icon_rgba(), 32, 32).map_err(|_error| {
        language.text(
            "Could not create the system tray icon",
            "无法创建系统托盘图标",
        )
    })?;
    let tray = TrayIconBuilder::new()
        .with_id("relay.status")
        .with_tooltip(language.text("Relay · rule routing", "Relay · 规则路由"))
        .with_icon(icon)
        .with_icon_as_template(cfg!(target_os = "macos"))
        .with_menu(Box::new(menu))
        .build()
        .map_err(|_error| language.text("System tray is unavailable", "系统托盘不可用"))?;

    cx.set_global(RelayTray {
        _icon: tray,
        show_id: show.id().clone(),
        quit_id: quit.id().clone(),
    });
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

fn drain_menu_events(cx: &mut App) -> bool {
    #[cfg(target_os = "linux")]
    {
        let gtk_events = gtk::glib::MainContext::default();
        while gtk_events.pending() {
            gtk_events.iteration(false);
        }
    }

    let (show_id, quit_id) = {
        let tray = cx.global::<RelayTray>();
        (tray.show_id.clone(), tray.quit_id.clone())
    };
    let mut should_quit = false;
    while let Ok(event) = MenuEvent::receiver().try_recv() {
        if event.id == show_id {
            show_or_open_window(cx);
        } else if event.id == quit_id {
            should_quit = true;
        }
    }
    if should_quit {
        cx.quit();
    }
    should_quit
}

fn relay_icon_rgba() -> Vec<u8> {
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
    use super::relay_icon_rgba;

    #[test]
    fn tray_icon_is_rgba_and_contains_transparency() {
        let icon = relay_icon_rgba();
        assert_eq!(icon.len(), 32 * 32 * 4);
        assert!(icon.iter().skip(3).step_by(4).any(|alpha| *alpha == 0));
        assert!(icon.iter().skip(3).step_by(4).any(|alpha| *alpha == 255));
    }
}
