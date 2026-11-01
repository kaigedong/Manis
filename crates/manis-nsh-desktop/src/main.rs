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

    #[allow(clippy::too_many_lines)]
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
                    "connection.toggle" => {
                        data.get("enabled").and_then(Value::as_bool).map(|enabled| {
                            manis_ui::nsh_set_tun(enabled).map_or_else(
                                |error| error,
                                |()| {
                                    if enabled {
                                        "已开启 TUN".to_owned()
                                    } else {
                                        "已关闭 TUN".to_owned()
                                    }
                                },
                            )
                        })
                    }
                    "connection.mode" => string_field("mode").map(|mode| {
                        manis_ui::nsh_set_routing_mode(&mode)
                            .map_or_else(|error| error, |()| "模式已切换".to_owned())
                    }),
                    "subscriptions.toggle" => {
                        let kind = string_field("kind");
                        let index = data
                            .get("index")
                            .and_then(Value::as_u64)
                            .and_then(|value| usize::try_from(value).ok());
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
                            .and_then(|value| usize::try_from(value).ok());
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
                    "rules.save" => {
                        let index = data
                            .get("index")
                            .and_then(Value::as_u64)
                            .and_then(|value| usize::try_from(value).ok());
                        match (
                            string_field("kind"),
                            string_field("value"),
                            string_field("policy"),
                        ) {
                            (Some(kind), Some(value), Some(policy)) => Some(
                                manis_ui::nsh_save_rule(index, &kind, &value, &policy)
                                    .map_or_else(|error| error, |()| "规则已保存".to_owned()),
                            ),
                            _ => Some("规则类型、内容和策略不能为空".to_owned()),
                        }
                    }
                    "rules.toggle" => {
                        let index = data
                            .get("index")
                            .and_then(Value::as_u64)
                            .and_then(|value| usize::try_from(value).ok());
                        let enabled = data.get("enabled").and_then(Value::as_bool);
                        match (index, enabled) {
                            (Some(index), Some(enabled)) => Some(
                                manis_ui::nsh_toggle_rule(index, enabled)
                                    .map_or_else(|error| error, |()| "规则状态已保存".to_owned()),
                            ),
                            _ => Some("缺少规则索引".to_owned()),
                        }
                    }
                    "rules.delete" => {
                        let index = data
                            .get("index")
                            .and_then(Value::as_u64)
                            .and_then(|value| usize::try_from(value).ok());
                        index.map(|index| {
                            manis_ui::nsh_delete_rule(index)
                                .map_or_else(|error| error, |()| "规则已删除".to_owned())
                        })
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

    fn controller_socket() -> Option<std::path::PathBuf> {
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|home| std::path::PathBuf::from(home).join(".local/share"))
            })?;
        Some(data.join("manis/mihomo/controller.sock"))
    }

    fn controller_ready(socket: &std::path::Path) -> bool {
        use std::io::{Read, Write};
        use std::os::unix::net::UnixStream;

        let Ok(mut stream) = UnixStream::connect(socket) else {
            return false;
        };
        let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(400)));
        let _ = stream.set_write_timeout(Some(std::time::Duration::from_millis(400)));
        if stream
            .write_all(b"GET /version HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .is_err()
        {
            return false;
        }
        let mut response = String::new();
        stream.read_to_string(&mut response).is_ok() && response.contains("200")
    }

    fn ensure_engine() -> Option<std::process::Child> {
        let socket = controller_socket()?;
        if controller_ready(&socket) {
            return None;
        }

        let mut candidates = Vec::new();
        if let Some(binary) = std::env::var_os("MANIS_UI_BIN") {
            candidates.push(std::path::PathBuf::from(binary));
        }
        if let Ok(executable) = std::env::current_exe()
            && let Some(directory) = executable.parent()
        {
            candidates.push(directory.join("manis-ui"));
        }
        candidates.push(std::path::PathBuf::from("manis-ui"));

        for binary in candidates {
            let Ok(child) = std::process::Command::new(&binary)
                .arg("--headless")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            else {
                continue;
            };
            for _ in 0..50 {
                if controller_ready(&socket) {
                    return Some(child);
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            return None;
        }
        None
    }

    pub(crate) fn main() {
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
