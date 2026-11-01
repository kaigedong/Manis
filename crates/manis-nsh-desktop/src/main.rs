//! Desktop NSH host for Manis.
//!
//! This is the first desktop embedding step: it runs the same TSX bundle as
//! Android in the GTK NSH host and answers the page commands with a static
//! empty workspace. The next steps replace the static payload with the real
//! profile/engine state.

use std::cell::RefCell;
use std::rc::Rc;

use nsh_platform_linux::gtk4 as gtk;
use nsh_platform_linux::gtk4::gio::prelude::ApplicationExtManual;
use nsh_platform_linux::gtk4::prelude::*;
use nsh_platform_linux::{AppOptions, AppView, Bundle};
use serde_json::{json, Map, Value};

const BUNDLE: &str = include_str!("../../../android/app/src/main/assets/nsh/bundle.js");

fn root_data() -> Value {
    // Reuse the desktop storage loader so saved subscriptions/nodes/groups and
    // the routing mode appear in the NSH shell.
    manis_ui::nsh_root_data()
}


fn reply(message: Option<&str>) -> Value {
    let mut payload = Map::new();
    payload.insert("data".into(), root_data());
    if let Some(message) = message {
        payload.insert("message".into(), Value::String(message.into()));
    }
    Value::Object(payload)
}

fn install_handlers(host: &nsh_engine::HostChannel) {
    for command in [
        "workspace.apply",
        "workspace.discard",
        "nodes.refresh",
        "nodes.select",
        "nodes.selectGroup",
        "subscriptions.toggle",
        "subscriptions.delete",
        "subscriptions.add",
        "subscriptions.addNode",
        "subscriptions.addGroup",
        "subscriptions.refresh",
        "subscriptions.apply",
        "rules.toggle",
        "rules.delete",
        "rules.save",
        "rules.apply",
        "settings.reload",
        "settings.logs",
        "settings.copyLogs",
        "settings.copyBackup",
        "settings.importBackup",
        "settings.dns.set",
        "settings.update.check",
        "settings.update.download",
        "settings.update.install",
        "settings.update.release",
        "connection.toggle",
        "connection.mode",
        "connection.reload",
    ] {
        host.handle(command, move |call| {
            // The static spike answers immediately; later hosts move I/O off
            // the engine thread and keep the command's own error.
            let message = match command {
                "workspace.apply" | "rules.apply" | "subscriptions.apply" => Some("已应用（桌面原型）"),
                "workspace.discard" => Some("已放弃更改（桌面原型）"),
                "settings.logs" => Some("桌面原型还没有连接内核日志"),
                _ => None,
            };
            call.resolve(reply(message));
        });
    }
}

fn main() {
    let app = gtk::Application::builder()
        .application_id("dev.manis.desktop.nsh")
        .build();
    app.connect_activate(|app| {
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Manis")
            .default_width(430)
            .default_height(900)
            .build();
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
        });
        if let Some(params) = params.as_object() {
            host.set_params(params.clone());
        }
        install_handlers(&host);
        view.load(Bundle::source(BUNDLE.to_owned()).into());
        window.set_child(Some(&view.widget()));
        let keep = RefCell::new(Some(Rc::clone(&view)));
        window.connect_destroy(move |_| {
            if let Some(view) = keep.borrow_mut().take() {
                view.stop();
            }
        });
        window.present();
    });
    app.run();
}
