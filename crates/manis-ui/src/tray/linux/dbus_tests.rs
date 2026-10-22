//! Run only on a private bus: `dbus-run-session -- cargo test -p manis-ui --lib
//! linux_tray_dbus --locked -- --ignored`.

use std::{collections::HashMap, sync::Mutex, sync::mpsc, time::Duration};

use zbus::{Connection, OwnedVariant, Proxy, Variant, block_on, connection::Builder};

use super::{Language, ManisTray, ProxyMode, TrayAction, TrayProxySnapshot};

const TIMEOUT: Duration = Duration::from_secs(5);

struct Watcher {
    // Served interfaces must be `Sync`; `mpsc::Sender` is only `Send`.
    registrations: Mutex<mpsc::Sender<String>>,
    host_registered: bool,
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    fn register_status_notifier_item(&self, service: &str) {
        self.registrations
            .lock()
            .unwrap()
            .send(service.into())
            .unwrap();
    }

    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        self.host_registered
    }
}

fn watcher(host_registered: bool) -> (Connection, mpsc::Receiver<String>) {
    let (registrations, received) = mpsc::channel();
    // The connection lands on zbus's process-wide multi-threaded runtime, so the served object
    // keeps answering between `block_on` calls and the rest of this test stays synchronous.
    let connection = block_on(async {
        Builder::session()
            .name("org.kde.StatusNotifierWatcher")
            .serve_at(
                "/StatusNotifierWatcher",
                Watcher {
                    registrations: Mutex::new(registrations),
                    host_registered,
                },
            )
            .build()
            .await
            .unwrap()
    });
    (connection, received)
}

type MenuProperties = Vec<(i32, HashMap<String, OwnedVariant>)>;

fn menu_item(menu: &Proxy<'_>, label: &str) -> (i32, HashMap<String, OwnedVariant>) {
    let items: MenuProperties = block_on(menu.call(
        "GetGroupProperties",
        &(Vec::<i32>::new(), Vec::<&str>::new()),
    ))
    .unwrap();
    items
        .into_iter()
        .find(|(_, properties)| {
            properties
                .get("label")
                .and_then(|value| <&str>::try_from(value).ok())
                == Some(label)
        })
        .unwrap_or_else(|| panic!("missing menu item {label}"))
}

fn click(menu: &Proxy<'_>, id: i32) {
    block_on(menu.call::<_, _, ()>("Event", &(id, "clicked", Variant::from(0_i32), 0_u32)))
        .unwrap();
}

#[test]
#[ignore = "requires an isolated session bus; run with dbus-run-session"]
fn linux_tray_dbus_registration_menu_updates_actions_and_shutdown() {
    // No watcher, or a watcher without a panel, must not enable close-to-tray.
    assert!(ManisTray::new(Language::English).is_err());
    let (empty_host, _registrations) = watcher(false);
    assert!(ManisTray::new(Language::English).is_err());
    block_on(empty_host.close()).unwrap();

    let (host, registrations) = watcher(true);
    let tray = ManisTray::new(Language::English).unwrap();
    let name = registrations.recv_timeout(TIMEOUT).unwrap();
    let sni = block_on(Proxy::new(
        &host,
        name.as_str(),
        "/StatusNotifierItem",
        "org.kde.StatusNotifierItem",
    ))
    .unwrap();
    assert_eq!(
        block_on(sni.get_property::<String>("Id")).unwrap(),
        "dev.manis.app"
    );
    block_on(sni.call::<_, _, ()>("Activate", &(0_i32, 0_i32))).unwrap();
    assert_eq!(tray.events.recv_timeout(TIMEOUT).unwrap(), TrayAction::Show);

    let menu = block_on(Proxy::new(
        &host,
        name.as_str(),
        "/MenuBar",
        "com.canonical.dbusmenu",
    ))
    .unwrap();
    tray.sync(TrayProxySnapshot {
        language: Language::English,
        active: ProxyMode::Tun,
        system_block: None,
        tun_block: None,
    });
    // A sync updates the service state immediately; AboutToShow asks it to publish that state.
    block_on(menu.call::<_, _, bool>("AboutToShow", &(0_i32,))).unwrap();
    let (tun_id, tun) = menu_item(&menu, "TUN proxy");
    assert_eq!(i32::try_from(&tun["toggle-state"]).unwrap(), 1);
    let (system_id, system) = menu_item(&menu, "System proxy");
    assert_eq!(i32::try_from(&system["toggle-state"]).unwrap(), 0);
    click(&menu, system_id);
    assert_eq!(
        tray.events.recv_timeout(TIMEOUT).unwrap(),
        TrayAction::ProxyMode(ProxyMode::System)
    );
    click(&menu, tun_id);
    assert_eq!(
        tray.events.recv_timeout(TIMEOUT).unwrap(),
        TrayAction::ProxyMode(ProxyMode::Tun)
    );
    // Clicking sends a request; it does not claim the mode changed before GPUI completes it.
    let (_, tun) = menu_item(&menu, "TUN proxy");
    assert_eq!(i32::try_from(&tun["toggle-state"]).unwrap(), 1);
    for (label, action) in [
        ("Open Manis", TrayAction::Show),
        ("About Manis", TrayAction::About),
        ("Quit Manis", TrayAction::Quit),
    ] {
        click(&menu, menu_item(&menu, label).0);
        assert_eq!(tray.events.recv_timeout(TIMEOUT).unwrap(), action);
    }

    let handle = tray.handle.clone();
    drop(tray);
    // Wait for the shutdown requested by Drop, then verify the SNI endpoint is gone.
    handle.shutdown().wait();
    assert!(handle.is_closed());
    assert!(block_on(sni.call::<_, _, ()>("Activate", &(0_i32, 0_i32))).is_err());
}
