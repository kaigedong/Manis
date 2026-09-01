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
fn preview_workspace_is_private_and_removed_on_drop() -> Result<(), Box<dyn std::error::Error>> {
    let workspace = super::PreviewWorkspace::create()?;
    let path = workspace.path().to_owned();
    assert!(path.is_dir());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&path)?.permissions().mode() & 0o077, 0);
    }

    drop(workspace);
    assert!(!path.exists());
    Ok(())
}

#[test]
fn preview_errors_never_expose_subscription_input() {
    let input = "https://subscription.example.invalid/private-token";
    let error = super::preview_subscription_with_binary(input, Path::new("/missing/mihomo"))
        .expect_err("missing preview binary should fail safely");

    assert!(!error.to_string().contains("private-token"));
    assert!(!format!("{error:?}").contains("private-token"));
}

#[test]
#[ignore = "requires MANIS_MIHOMO_TEST_BINARY pointing to a local Mihomo executable"]
fn real_mihomo_previews_all_nodes_from_a_subscription() -> Result<(), Box<dyn std::error::Error>> {
    let binary = std::env::var_os("MANIS_MIHOMO_TEST_BINARY")
        .ok_or("MANIS_MIHOMO_TEST_BINARY is required")?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let subscription_url = format!("http://{}/subscription", listener.local_addr()?);
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = stop.clone();
    let server = std::thread::spawn(move || -> std::io::Result<()> {
        let body = r#"proxies:
  - name: "Fixture Alpha"
    type: ss
    server: 127.0.0.1
    port: 443
    cipher: aes-128-gcm
    password: fixture-alpha
  - name: "Fixture Beta"
    type: ss
    server: 127.0.0.1
    port: 8443
    cipher: aes-128-gcm
    password: fixture-beta
"#;
        while !server_stop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut request_line = String::new();
                    BufReader::new(stream.try_clone()?).read_line(&mut request_line)?;
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/yaml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(response.as_bytes())?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    });

    let result = super::preview_subscription_with_binary(&subscription_url, Path::new(&binary));
    let import_root = test_temp_dir("manis-real-import");
    let store = import_root.join("subscriptions");
    super::save_imported_subscription_in(&store, &subscription_url)?;
    let restored_secret = super::load_imported_subscription_in(&store)?
        .ok_or("imported subscription should exist")?;
    let restored =
        super::preview_secret_subscription_with_binary(&restored_secret, Path::new(&binary));
    stop.store(true, Ordering::Relaxed);
    server.join().map_err(|_| "fixture server panicked")??;
    let providers = result?;
    let restored_providers = restored?;

    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0].nodes.len(), 2);
    assert_eq!(providers[0].nodes[0].name, "Fixture Alpha");
    assert_eq!(providers[0].nodes[1].name, "Fixture Beta");
    assert_eq!(restored_providers, providers);
    fs::remove_dir_all(import_root)?;
    Ok(())
}
