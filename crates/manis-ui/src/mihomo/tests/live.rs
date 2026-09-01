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
fn kernel_log_sanitizer_redacts_urls_and_bounds_dynamic_payloads() {
    let input = format!(
        "provider https://example.invalid/client?token=fixture-secret failed; node vless://uuid@host:443 {}",
        "x".repeat(3_000)
    );
    let sanitized = super::sanitize_kernel_log(&input);

    assert!(sanitized.contains("<redacted-url>"));
    assert!(!sanitized.contains("fixture-secret"));
    assert!(!sanitized.contains("vless://"));
    assert!(sanitized.chars().count() <= 2_048);

    let uppercase = super::sanitize_kernel_log(
        "provider HTTPS://example.invalid/path?token=uppercase-secret failed",
    );
    assert_eq!(
        uppercase, "provider <redacted-url> failed",
        "URI schemes are case-insensitive"
    );
}

#[test]
fn successful_activity_snapshots_use_the_poll_interval_without_reconnecting() {
    use std::sync::{Mutex, mpsc};
    use std::time::Instant;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config =
        super::ControllerConfig::new(format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let (times_tx, times_rx) = mpsc::channel();
    let server = std::thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                    break;
                }
            }
            let body = r#"{"downloadTotal":0,"uploadTotal":0,"connections":[]}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            times_tx.send(Instant::now()).unwrap();
        }
    });
    let cancelled = Arc::new(AtomicBool::new(false));
    let mailbox = Arc::new(Mutex::new(super::LiveMailbox::default()));
    super::spawn_connection_stream(
        super::LiveController::loopback(config),
        cancelled.clone(),
        mailbox.clone(),
    );
    let first = times_rx.recv_timeout(Duration::from_secs(3));
    std::thread::sleep(Duration::from_millis(500));
    let phase = mailbox.lock().unwrap().status.activity.clone();
    let second = times_rx.recv_timeout(Duration::from_secs(3));
    cancelled.store(true, Ordering::Relaxed);
    server.join().unwrap();
    assert_eq!(phase, super::LiveStreamPhase::Live);
    assert!(
        second.unwrap().duration_since(first.unwrap()) >= super::LIVE_CONNECTION_INTERVAL,
        "successful finite snapshots must not trigger the fast reconnect loop"
    );
}
