#![allow(unused_imports)]

use super::*;
use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use manis_engine::ControllerEndpoint;

#[test]
#[ignore = "requires a locally installed sing-box executable"]
fn managed_sing_box_clash_api_loads_a_manis_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let binary = super::discover_sing_box_binary()?;
    let root = test_temp_dir("manis-sing-box-runtime");
    let data_dir = root.join("runtime");
    let vless = manis_profile::VlessProxy::parse_share_link(
        "vless://00000000-0000-4000-8000-000000000000@198.51.100.7:443?security=reality&encryption=none&pbk=Qs24XU-ibEZ3LWDjGBITKdQvualLy0pi_PI0qoF79A8&fp=chrome&type=tcp&flow=xtls-rprx-vision&sni=cdn.example.invalid#Reality%20TCP",
    )?;
    let mut profile = manis_profile::Profile::qx_sources(Vec::new(), vec![vless], 17_890)?;
    profile.rules = vec![manis_profile::Rule::Match {
        policy: manis_profile::PolicyRef::Group(manis_profile::Name::parse("Proxy")?),
    }];
    let address = TcpListener::bind("127.0.0.1:0")?.local_addr()?;
    let controller = ControllerEndpoint::Tcp(address);
    let secret = "fixture-controller-secret".to_owned();
    let spec = super::ManagedGeneratedProfile {
        kernel: manis_core::KernelKind::SingBox,
        binary,
        data_dir: data_dir.clone(),
        controller: controller.clone(),
        expected_mixed_port: None,
        profile_store_dir: None,
        controller_secret: Some(secret.clone()),
    };
    let rendered = super::render_generated_profile(&spec, &profile)?;
    let config_file = manis_profile::write_private_atomic(
        &data_dir,
        super::SING_BOX_PROFILE_FILE,
        rendered.as_bytes(),
    )?;
    let config = super::managed_engine_config(&spec, config_file);
    let mut manager = manis_engine::EngineManager::new(
        config,
        manis_engine::ReadinessPolicy::default(),
        super::readiness_probe(&spec),
    );
    let endpoint = manager.start()?.uri();

    super::set_routing_mode(&endpoint, manis_core::RoutingMode::Direct, Some(&secret))?;
    let runtime = super::fetch_sing_box_snapshot(&endpoint, Some(&secret))?;
    assert_eq!(runtime.runtime.mode, manis_core::RoutingMode::Direct);
    super::put_policy_group_selection(&endpoint, "GLOBAL", "Reality TCP", Some(&secret))?;
    let global = super::fetch_policy_group(&endpoint, "GLOBAL", Some(&secret))?;
    assert_eq!(global.current.as_deref(), Some("Reality TCP"));
    super::put_policy_group_selection(&endpoint, "Proxy", "Auto", Some(&secret))?;
    let selected = super::fetch_policy_group(&endpoint, "Proxy", Some(&secret))?;
    assert_eq!(selected.current.as_deref(), Some("Auto"));
    let snapshot = super::load_sing_box(&endpoint, Some(&secret));
    manager.stop()?;
    fs::remove_dir_all(root)?;
    snapshot?;
    Ok(())
}

#[test]
fn parses_absolute_unix_controller_endpoint() -> Result<(), Box<dyn std::error::Error>> {
    let path = super::unix_socket_path("unix:///tmp/verge/mihomo.sock")?
        .ok_or("expected a Unix socket path")?;
    assert_eq!(path, Path::new("/tmp/verge/mihomo.sock"));
    assert!(super::unix_socket_path("http://127.0.0.1:9090")?.is_none());
    Ok(())
}

#[test]
fn rejects_relative_unix_controller_endpoint() {
    assert!(super::unix_socket_path("unix://relative.sock").is_err());
    assert!(super::unix_socket_path("unix://").is_err());
}

#[test]
fn policy_group_snapshot_deduplicates_runtime_candidates() {
    let snapshot = super::policy_group_runtime_snapshot(manis_mihomo::MihomoPolicyGroup {
        name: Some("Manis Group".to_owned()),
        proxy_type: Some("Selector".to_owned()),
        current: Some("Tokyo".to_owned()),
        all: vec!["Tokyo".to_owned(), "Tokyo".to_owned(), "Osaka".to_owned()],
    });

    assert_eq!(snapshot.current.as_deref(), Some("Tokyo"));
    assert_eq!(
        snapshot.candidates,
        ["Osaka".to_owned(), "Tokyo".to_owned()]
            .into_iter()
            .collect()
    );
    assert!(super::is_selector_proxy_type("SELECTOR"));
    assert!(!super::is_selector_proxy_type("select-or"));
    assert!(!super::is_selector_proxy_type("URLTest"));
}

#[test]
fn provider_node_global_selection_uses_the_internal_global_exit_group() {
    let global = manis_mihomo::MihomoPolicyGroup {
        name: Some("GLOBAL".to_owned()),
        proxy_type: Some("Selector".to_owned()),
        current: Some("DIRECT".to_owned()),
        all: vec!["DIRECT".to_owned(), "__MANIS_GLOBAL__".to_owned()],
    };
    let global_exit = manis_mihomo::MihomoPolicyGroup {
        name: Some("__MANIS_GLOBAL__".to_owned()),
        proxy_type: Some("Selector".to_owned()),
        current: Some("HK 01".to_owned()),
        all: vec!["HK 01".to_owned(), "HK 03".to_owned()],
    };

    assert_eq!(
        super::global_selection_route(&global, &global_exit, "HK 03"),
        Some(super::GlobalSelectionRoute::ViaGlobalExit)
    );
    assert_eq!(
        super::global_selection_route(&global, &global_exit, "DIRECT"),
        Some(super::GlobalSelectionRoute::Direct)
    );
    assert_eq!(
        super::global_selection_route(&global, &global_exit, "Missing"),
        None
    );
}

#[test]
fn global_node_selection_applies_leaf_then_internal_group() -> Result<(), Box<dyn std::error::Error>>
{
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let endpoint = format!("http://{}", listener.local_addr()?);
    let server = std::thread::spawn(move || -> std::io::Result<Vec<String>> {
        let mut requests = Vec::new();
        for index in 0..8 {
            let (mut stream, _) = listener.accept()?;
            let mut request_line = String::new();
            BufReader::new(stream.try_clone()?).read_line(&mut request_line)?;
            requests.push(request_line.trim().to_owned());
            let body = match index {
                0 | 5 => {
                    r#"{"name":"GLOBAL","type":"Selector","now":"DIRECT","all":["DIRECT","__MANIS_GLOBAL__"]}"#
                }
                1 | 2 => {
                    r#"{"name":"__MANIS_GLOBAL__","type":"Selector","now":"HK 01","all":["HK 01","HK 03"]}"#
                }
                4 => {
                    r#"{"name":"__MANIS_GLOBAL__","type":"Selector","now":"HK 03","all":["HK 01","HK 03"]}"#
                }
                7 => {
                    r#"{"name":"GLOBAL","type":"Selector","now":"__MANIS_GLOBAL__","all":["DIRECT","__MANIS_GLOBAL__"]}"#
                }
                3 | 6 => "",
                _ => unreachable!(),
            };
            let response = if body.is_empty() {
                "HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_owned()
            } else {
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
            };
            stream.write_all(response.as_bytes())?;
        }
        Ok(requests)
    });

    let snapshot = super::select_global_node_at_endpoint(&endpoint, "HK 03", None)?;
    let requests = server.join().map_err(|_| "fixture server panicked")??;

    assert_eq!(snapshot.current.as_deref(), Some("HK 03"));
    assert_eq!(
        requests,
        [
            "GET /proxies/GLOBAL HTTP/1.1",
            "GET /proxies/__MANIS_GLOBAL__ HTTP/1.1",
            "GET /proxies/__MANIS_GLOBAL__ HTTP/1.1",
            "PUT /proxies/__MANIS_GLOBAL__ HTTP/1.1",
            "GET /proxies/__MANIS_GLOBAL__ HTTP/1.1",
            "GET /proxies/GLOBAL HTTP/1.1",
            "PUT /proxies/GLOBAL HTTP/1.1",
            "GET /proxies/GLOBAL HTTP/1.1",
        ]
    );
    Ok(())
}
