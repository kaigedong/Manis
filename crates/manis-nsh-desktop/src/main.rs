//! Desktop NSH host for Manis.
//!
//! It runs the same TSX bundle as Android in the GTK NSH host and bridges
//! saved desktop sources, the live controller snapshot and the desktop store
//! mutations. It is a development host while the GPUI window is still the
//! production desktop UI.

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("manis-nsh-desktop requires Linux and GTK 4");
}

#[cfg(target_os = "linux")]
mod desktop {
    use std::cell::RefCell;
    use std::rc::Rc;

    use nsh_platform_linux::gtk4 as gtk;
    use nsh_platform_linux::gtk4::gio::prelude::ApplicationExtManual;
    use nsh_platform_linux::gtk4::prelude::*;
    use nsh_platform_linux::{AppOptions, AppView, Bundle};

    use manis_nsh_host::{ensure_engine, install_handlers, root_data};
    use serde_json::json;

    const BUNDLE: &str = include_str!("../../../android/app/src/main/assets/nsh/bundle.js");

    #[derive(Clone, Copy)]
    enum TrayAction {
        Show,
        Quit,
    }

    struct ManisTray {
        events: std::sync::mpsc::Sender<TrayAction>,
    }

    impl ManisTray {
        fn send(&self, action: TrayAction) {
            let _ = self.events.send(action);
        }
    }

    impl ksni::Tray for ManisTray {
        fn id(&self) -> String {
            "dev.manis.app".into()
        }

        fn title(&self) -> String {
            "Manis".into()
        }

        fn icon_name(&self) -> String {
            "dev.manis.app".into()
        }

        fn activate(&mut self, _x: i32, _y: i32) {
            self.send(TrayAction::Show);
        }

        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::StandardItem;

            vec![
                StandardItem {
                    label: "打开 Manis".into(),
                    activate: Box::new(|tray: &mut Self| tray.send(TrayAction::Show)),
                    ..Default::default()
                }
                .into(),
                ksni::MenuItem::Separator,
                StandardItem {
                    label: "退出 Manis".into(),
                    activate: Box::new(|tray: &mut Self| tray.send(TrayAction::Quit)),
                    ..Default::default()
                }
                .into(),
            ]
        }
    }

    fn start_tray(
        events: std::sync::mpsc::Sender<TrayAction>,
    ) -> Option<ksni::blocking::Handle<ManisTray>> {
        use ksni::blocking::TrayMethods as _;

        ManisTray { events }.assume_sni_available(true).spawn().ok()
    }

    pub(crate) fn main() {
        if std::env::args_os().any(|argument| argument == "--version") {
            println!("Manis {}", manis_ui::version());
            return;
        }
        let engine = Rc::new(RefCell::new(ensure_engine()));
        let engine_for_activate = Rc::clone(&engine);
        let app = gtk::Application::builder()
            .application_id("dev.manis.desktop.nsh")
            .build();
        app.connect_activate(move |app| {
            let engine = Rc::clone(&engine_for_activate);
            let window = gtk::ApplicationWindow::builder()
                .application(app)
                .title("Manis")
                .default_width(430)
                .default_height(900)
                .build();
            let (tray_sender, tray_events) = std::sync::mpsc::channel();
            let tray = Rc::new(RefCell::new(start_tray(tray_sender)));
            let has_tray = tray.borrow().is_some();
            let tray_window = window.clone();
            window.connect_close_request(move |_| {
                if has_tray {
                    tray_window.set_visible(false);
                    gtk::glib::Propagation::Stop
                } else {
                    gtk::glib::Propagation::Proceed
                }
            });
            let tray_alive = Rc::clone(&tray);
            let tray_window = window.clone();
            let tray_app = app.clone();
            gtk::glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
                let _keep_tray = &tray_alive;
                while let Ok(action) = tray_events.try_recv() {
                    match action {
                        TrayAction::Show => tray_window.present(),
                        TrayAction::Quit => tray_app.quit(),
                    }
                }
                gtk::glib::ControlFlow::Continue
            });

            let view = AppView::new(AppOptions {
                app_id: "dev.manis.desktop.nsh".to_owned(),
                title: "Manis".into(),
                size: (430, 900),
                ..AppOptions::default()
            });
            let host = view.host();
            let data = root_data();
            let params = json!({
                "data": data,
                "embedded": true,
                "topInset": 0,
                "bottomInset": 0,
                "tab": std::env::var("MANIS_NSH_TAB").unwrap_or_else(|_| "connection".into()),
            });
            if let Some(params) = params.as_object() {
                host.set_params(params.clone());
            }
            install_handlers(&host);
            view.load(Bundle::source(BUNDLE.to_owned()).into());
            window.set_child(Some(&view.widget()));
            let keep = RefCell::new(Some(Rc::clone(&view)));
            let engine = Rc::clone(&engine);
            window.connect_destroy(move |_| {
                if let Some(mut engine) = engine.borrow_mut().take() {
                    let _ = engine.kill();
                }
                if let Some(view) = keep.borrow_mut().take() {
                    view.stop();
                }
            });
            window.present();
        });
        app.run();
    }
}

#[cfg(target_os = "linux")]
fn main() {
    desktop::main();
}
