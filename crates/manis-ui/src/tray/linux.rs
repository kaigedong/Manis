//! GTK-free Linux tray. D-Bus callbacks enqueue actions; only GPUI's thread touches app state.

#[cfg(test)]
mod dbus_tests;

use std::sync::mpsc::{self, Receiver, Sender};

use ksni::{
    blocking::{Handle, TrayMethods},
    menu::{CheckmarkItem, StandardItem},
};
use manis_core::ProxyMode;

use super::{TrayProxySnapshot, manis_icon_rgba, tray_menu_label};
use crate::{
    app::ProxyModeBlock,
    localization::{Language, copy},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TrayAction {
    Show,
    Quit,
    ProxyMode(ProxyMode),
}

pub(super) struct ManisTray {
    handle: Handle<LinuxTray>,
    events: Receiver<TrayAction>,
    pub(super) synced: Option<TrayProxySnapshot>,
}

impl ManisTray {
    pub(super) fn new(language: Language) -> Result<Self, &'static str> {
        let (sender, events) = mpsc::channel();
        // Do not assume SNI is available: without a host, hiding the last window would strand the
        // application. A successful registration enables close-to-tray in the shared installer.
        let handle = LinuxTray::new(language, sender)
            .spawn()
            .map_err(|_| language.localized(copy::tray::SYSTEM_TRAY_IS_UNAVAILABLE))?;
        Ok(Self {
            handle,
            events,
            synced: None,
        })
    }

    pub(super) fn events(&self) -> Vec<TrayAction> {
        self.events.try_iter().collect()
    }

    pub(super) fn sync(&self, snapshot: TrayProxySnapshot) {
        self.handle.update(|tray| tray.snapshot = snapshot);
    }
}

impl Drop for ManisTray {
    fn drop(&mut self) {
        // Request shutdown without blocking GPUI while D-Bus tears down the service.
        self.handle.shutdown();
    }
}

struct LinuxTray {
    events: Sender<TrayAction>,
    snapshot: TrayProxySnapshot,
}

impl LinuxTray {
    fn new(language: Language, events: Sender<TrayAction>) -> Self {
        Self {
            events,
            snapshot: TrayProxySnapshot {
                language,
                active: ProxyMode::Off,
                system_block: Some(ProxyModeBlock::ControllerNotConnected),
                tun_block: Some(ProxyModeBlock::ControllerNotConnected),
            },
        }
    }

    fn send(&self, action: TrayAction) {
        let _ = self.events.send(action);
    }

    fn proxy_item(&self, mode: ProxyMode) -> CheckmarkItem<Self> {
        let block = match mode {
            ProxyMode::System => self.snapshot.system_block,
            ProxyMode::Tun => self.snapshot.tun_block,
            ProxyMode::Off => unreachable!("the tray only offers System and TUN check items"),
        };
        CheckmarkItem {
            label: tray_menu_label(self.snapshot.language, mode, block),
            enabled: block.is_none(),
            checked: self.snapshot.active == mode,
            // Never optimistically flip the mark. GPUI sends the actual state back after applying
            // the change, including failed or still-pending requests.
            activate: Box::new(move |tray| tray.send(TrayAction::ProxyMode(mode))),
            ..Default::default()
        }
    }
}

impl ksni::Tray for LinuxTray {
    fn id(&self) -> String {
        "dev.manis.app".into()
    }

    fn title(&self) -> String {
        self.snapshot
            .language
            .localized(copy::tray::MANIS_RULE_ROUTING)
            .into()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(TrayAction::Show);
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        let mut data = manis_icon_rgba();
        for pixel in data.chunks_exact_mut(4) {
            // SNI requires ARGB in network byte order, not RGBA or native-endian u32s.
            pixel.rotate_right(1);
        }
        vec![ksni::Icon {
            width: 32,
            height: 32,
            data,
        }]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: self.title(),
            ..Default::default()
        }
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        let language = self.snapshot.language;
        vec![
            StandardItem {
                label: language.localized(copy::tray::OPEN_MANIS).into(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayAction::Show)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: language
                    .localized(copy::tray::RULE_ROUTING_STATUS_IS_AVAILABLE_IN_THE_MAIN_WINDOW)
                    .into(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            self.proxy_item(ProxyMode::System).into(),
            self.proxy_item(ProxyMode::Tun).into(),
            ksni::MenuItem::Separator,
            StandardItem {
                label: language.localized(copy::tray::QUIT_MANIS).into(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayAction::Quit)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use ksni::{MenuItem, Tray};

    use super::*;

    fn fixture() -> (LinuxTray, Receiver<TrayAction>) {
        let (sender, events) = mpsc::channel();
        (LinuxTray::new(Language::English, sender), events)
    }

    #[test]
    fn linux_tray_starts_with_proxy_actions_disabled() {
        let (tray, _) = fixture();
        assert_eq!(tray.menu().len(), 7);
        for mode in [ProxyMode::System, ProxyMode::Tun] {
            let item = tray.proxy_item(mode);
            assert!(!item.enabled);
            assert!(!item.checked);
        }
    }

    #[test]
    fn linux_tray_tracks_actual_mode_blocks_and_language() {
        let (mut tray, _) = fixture();
        for language in [Language::English, Language::SimplifiedChinese] {
            for active in [ProxyMode::Off, ProxyMode::System, ProxyMode::Tun] {
                tray.snapshot = TrayProxySnapshot {
                    language,
                    active,
                    system_block: None,
                    tun_block: Some(ProxyModeBlock::Busy),
                };
                let system = tray.proxy_item(ProxyMode::System);
                let tun = tray.proxy_item(ProxyMode::Tun);
                assert!(system.enabled);
                assert!(!tun.enabled);
                assert_eq!(system.checked, active == ProxyMode::System);
                assert_eq!(tun.checked, active == ProxyMode::Tun);
                assert_eq!(system.label, language.localized(copy::common::SYSTEM_PROXY));
                assert!(
                    tun.label
                        .contains(ProxyModeBlock::Busy.tray_reason(language))
                );
                assert_eq!(
                    tray.title(),
                    language.localized(copy::tray::MANIS_RULE_ROUTING)
                );
                let MenuItem::Standard(open) = tray.menu().remove(0) else {
                    panic!("first menu item should open Manis");
                };
                assert_eq!(open.label, language.localized(copy::tray::OPEN_MANIS));
            }
        }
    }

    #[test]
    fn linux_tray_callbacks_enqueue_actions_without_changing_proxy_state() {
        let (mut tray, events) = fixture();
        tray.snapshot.system_block = None;
        tray.snapshot.tun_block = None;
        let before = tray.snapshot;
        tray.activate(0, 0);
        for item in tray.menu() {
            match item {
                MenuItem::Standard(item) if item.enabled => (item.activate)(&mut tray),
                MenuItem::Checkmark(item) => (item.activate)(&mut tray),
                _ => {}
            }
        }
        assert_eq!(tray.snapshot, before);
        assert_eq!(
            events.try_iter().collect::<Vec<_>>(),
            vec![
                TrayAction::Show,
                TrayAction::Show,
                TrayAction::ProxyMode(ProxyMode::System),
                TrayAction::ProxyMode(ProxyMode::Tun),
                TrayAction::Quit,
            ]
        );
    }

    #[test]
    fn linux_tray_icon_uses_network_order_argb() {
        let (tray, _) = fixture();
        let icons = tray.icon_pixmap();
        assert_eq!(icons.len(), 1);
        let icon = &icons[0];
        assert_eq!((icon.width, icon.height), (32, 32));
        assert_eq!(icon.data.len(), 32 * 32 * 4);
        for (argb, rgba) in icon
            .data
            .chunks_exact(4)
            .zip(manis_icon_rgba().chunks_exact(4))
        {
            assert_eq!(argb, [rgba[3], rgba[0], rgba[1], rgba[2]]);
        }
    }
}
