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
use serde_json::{Map, Value, json};

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
            let data = call.data();
            let string_field =
                |name: &str| data.get(name).and_then(Value::as_str).map(str::to_owned);
            if command == "settings.logs" {
                let mut payload = reply(None);
                payload["logs"] = json!(manis_ui::nsh_logs());
                call.resolve(payload);
                return;
            }
            let message = match command {
                "nodes.select" => string_field("name").map(|name| {
                    manis_ui::nsh_select_global_node(&name)
                        .map_or_else(|error| error, |()| "已切换".to_owned())
                }),
                "nodes.selectGroup" => match (string_field("group"), string_field("name")) {
                    (Some(group), Some(name)) => Some(
                        manis_ui::nsh_select_group_node(&group, &name)
                            .map_or_else(|error| error, |()| "已选择".to_owned()),
                    ),
                    _ => Some("缺少策略组或节点".to_owned()),
                },
                "connection.mode" => string_field("mode").map(|mode| {
                    manis_ui::nsh_set_routing_mode(&mode)
                        .map_or_else(|error| error, |()| "模式已切换".to_owned())
                }),
                "subscriptions.toggle" => {
                    let kind = string_field("kind");
                    let index = data
                        .get("index")
                        .and_then(Value::as_u64)
                        .map(|value| value as usize);
                    let enabled = data.get("enabled").and_then(Value::as_bool);
                    match (kind.as_deref(), index, enabled) {
                        (Some("subscription"), Some(index), Some(enabled)) => Some(
                            manis_ui::nsh_toggle_subscription(index, enabled)
                                .map_or_else(|error| error, |()| "订阅状态已保存".to_owned()),
                        ),
                        (Some("node"), Some(index), Some(enabled)) => Some(
                            manis_ui::nsh_toggle_node(index, enabled)
                                .map_or_else(|error| error, |()| "节点状态已保存".to_owned()),
                        ),
                        _ => Some("无法识别要修改的来源".to_owned()),
                    }
                }
                "subscriptions.delete" => {
                    let kind = string_field("kind");
                    let index = data
                        .get("index")
                        .and_then(Value::as_u64)
                        .map(|value| value as usize);
                    match (kind.as_deref(), index) {
                        (Some("subscription"), Some(index)) => Some(
                            manis_ui::nsh_delete_subscription(index)
                                .map_or_else(|error| error, |()| "订阅已删除".to_owned()),
                        ),
                        (Some("node"), Some(index)) => Some(
                            manis_ui::nsh_delete_node(index)
                                .map_or_else(|error| error, |()| "节点已删除".to_owned()),
                        ),
                        _ => Some("暂时只能删除订阅或手动节点".to_owned()),
                    }
                }
                "subscriptions.add" => match (string_field("name"), string_field("url")) {
                    (Some(name), Some(url)) => Some(
                        manis_ui::nsh_add_subscription(&name, &url)
                            .map_or_else(|error| error, |()| "订阅已保存".to_owned()),
                    ),
                    _ => Some("订阅名称和地址不能为空".to_owned()),
                },
                "subscriptions.addNode" => string_field("link").map(|link| {
                    manis_ui::nsh_add_node(&link)
                        .map_or_else(|error| error, |()| "节点已保存".to_owned())
                }),
                "workspace.apply"
                | "rules.apply"
                | "subscriptions.apply"
                | "subscriptions.refresh"
                | "settings.reload" => Some(
                    manis_ui::nsh_apply_profile()
                        .map_or_else(|error| error, |()| "配置已重新载入".to_owned()),
                ),
                "workspace.discard" => Some("已放弃更改（桌面原型）".to_owned()),
                "settings.logs" => Some("桌面原型还没有连接内核日志".to_owned()),
                _ => None,
            };
            call.resolve(reply(message.as_deref()));
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
            "tab": std::env::var("MANIS_NSH_TAB").unwrap_or_else(|_| "connection".into()),
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
