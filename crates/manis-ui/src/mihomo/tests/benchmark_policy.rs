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
fn delay_controller_timeout_exceeds_the_kernel_test_timeout() {
    let config = super::delay_controller_config(manis_mihomo::ControllerConfig::default());

    assert!(
        config.read_timeout() > Duration::from_millis(u64::from(super::GROUP_DELAY_TIMEOUT_MS))
    );
}

#[test]
fn delay_benchmark_worker_panic_is_reported() {
    let targets = [super::ProxyDelayTarget::direct("Panicking Node")];
    let mut updates = Vec::new();

    let result = super::fetch_proxy_delay_targets_bounded_with_progress_by(
        &targets,
        &mut |name, delay| updates.push((name.to_owned(), delay)),
        |target| {
            assert!(target.name() != "Panicking Node", "fixture worker panic");
            Ok(42)
        },
    );

    assert!(matches!(
        result,
        Err(super::LoadError::Runtime(message)) if message.contains("worker panicked")
    ));
    assert!(updates.is_empty());
}

#[test]
fn fixture_group_benchmark_keeps_partial_proxy_results() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let endpoint = format!("http://{}", listener.local_addr()?);
    let server = std::thread::spawn(move || -> std::io::Result<()> {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept()?;
            let mut request_line = String::new();
            BufReader::new(stream.try_clone()?).read_line(&mut request_line)?;
            let response = if request_line.contains("Working%20Node") {
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 12\r\nConnection: close\r\n\r\n{\"delay\":64}"
            } else {
                "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            };
            stream.write_all(response.as_bytes())?;
        }
        Ok(())
    });
    let runtime = super::ControllerRuntime::Fixture { endpoint };
    let delays = runtime.test_proxy_delay_targets_with_progress(
        &[
            super::ProxyDelayTarget::direct("Working Node"),
            super::ProxyDelayTarget::direct("Offline Node"),
        ],
        |_name, _delay| {},
    )?;
    server.join().map_err(|_| "fixture server panicked")??;
    assert_eq!(delays.get("Working Node"), Some(&64));
    assert!(!delays.contains_key("Offline Node"));
    Ok(())
}

#[test]
fn fixture_proxy_benchmark_reports_fast_nodes_before_slow_nodes_finish()
-> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let endpoint = format!("http://{}", listener.local_addr()?);
    let slow_gate = Arc::new((Mutex::new(false), Condvar::new()));
    let server_gate = slow_gate.clone();
    let server = std::thread::spawn(move || -> std::io::Result<()> {
        let handlers = (0..2)
                .map(|_| {
                    let (stream, _) = listener.accept()?;
                    let gate = server_gate.clone();
                    Ok(std::thread::spawn(move || -> std::io::Result<()> {
                        let mut stream = stream;
                        let mut request_line = String::new();
                        BufReader::new(stream.try_clone()?).read_line(&mut request_line)?;
                        let delay = if request_line.contains("Slow%20Node") {
                            let (lock, ready) = &*gate;
                            let mut released = lock.lock().map_err(|_| {
                                std::io::Error::other("slow fixture gate poisoned")
                            })?;
                            while !*released {
                                released = ready.wait(released).map_err(|_| {
                                    std::io::Error::other("slow fixture gate poisoned")
                                })?;
                            }
                            70
                        } else {
                            30
                        };
                        let body = format!(r#"{{"delay":{delay}}}"#);
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        stream.write_all(response.as_bytes())?;
                        Ok(())
                    }))
                })
                .collect::<std::io::Result<Vec<_>>>()?;
        for handler in handlers {
            handler
                .join()
                .map_err(|_| std::io::Error::other("fixture handler panicked"))??;
        }
        Ok(())
    });

    let runtime = super::ControllerRuntime::Fixture { endpoint };
    let mut updates = Vec::new();
    let callback_gate = slow_gate.clone();
    let delays = runtime.test_proxy_delay_targets_with_progress(
        &[
            super::ProxyDelayTarget::direct("Slow Node"),
            super::ProxyDelayTarget::direct("Fast Node"),
        ],
        |name, delay| {
            updates.push((name.to_owned(), delay));
            if name == "Fast Node" {
                let (lock, ready) = &*callback_gate;
                let mut released = lock.lock().expect("fixture callback gate poisoned");
                *released = true;
                ready.notify_all();
            }
        },
    )?;
    server.join().map_err(|_| "fixture server panicked")??;

    assert_eq!(
        updates,
        vec![
            ("Fast Node".to_owned(), Some(30)),
            ("Slow Node".to_owned(), Some(70)),
        ]
    );
    assert_eq!(delays.get("Fast Node"), Some(&30));
    assert_eq!(delays.get("Slow Node"), Some(&70));
    Ok(())
}

#[test]
fn provider_proxy_benchmark_uses_provider_healthcheck_endpoint()
-> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let endpoint = format!("http://{}", listener.local_addr()?);
    let server = std::thread::spawn(move || -> std::io::Result<String> {
        let (mut stream, _) = listener.accept()?;
        let mut request_line = String::new();
        BufReader::new(stream.try_clone()?).read_line(&mut request_line)?;
        let body = r#"{"delay":42}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes())?;
        Ok(request_line)
    });

    let runtime = super::ControllerRuntime::Fixture { endpoint };
    let delays = runtime.test_proxy_delay_targets_with_progress(
        &[super::ProxyDelayTarget::provider("Subscription 1", "HK 01")],
        |_name, _delay| {},
    )?;
    let request_line = server.join().map_err(|_| "fixture server panicked")??;

    assert!(
        request_line
            .starts_with("GET /providers/proxies/Subscription%201/HK%2001/healthcheck?url=")
    );
    assert_eq!(delays.get("HK 01"), Some(&42));
    Ok(())
}

#[test]
fn runtime_policy_benchmark_uses_group_delay_then_reads_automatic_winner()
-> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let endpoint = format!("http://{}", listener.local_addr()?);
    let server = std::thread::spawn(move || -> std::io::Result<Vec<String>> {
        let mut requests = Vec::new();
        for _ in 0..2 {
            let (mut stream, _) = listener.accept()?;
            let mut request_line = String::new();
            BufReader::new(stream.try_clone()?).read_line(&mut request_line)?;
            let body = if request_line.contains("/group/Auto%20HK/delay?") {
                r#"{"HK-01":68,"HK-02":29,"HK-03":0,"unrelated":42}"#
            } else {
                r#"{"name":"Auto HK","type":"URLTest","now":"HK-02","all":["HK-01","HK-02"]}"#
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes())?;
            requests.push(request_line);
        }
        Ok(requests)
    });
    let runtime = super::ControllerRuntime::Fixture { endpoint };

    let result = runtime.test_policy_group_delay(
        "Auto HK",
        &[
            super::ProxyDelayTarget::direct("HK-01"),
            super::ProxyDelayTarget::direct("HK-02"),
        ],
    )?;
    let requests = server.join().map_err(|_| "fixture server panicked")??;

    assert!(requests[0].contains("GET /group/Auto%20HK/delay?"));
    assert!(requests[1].contains("GET /proxies/Auto%20HK HTTP/1.1"));
    assert_eq!(result.current.as_deref(), Some("HK-02"));
    assert_eq!(result.delays.get("HK-02"), Some(&29));
    assert_eq!(result.delays.len(), 2);
    Ok(())
}

#[test]
fn runtime_policy_benchmark_falls_back_to_partial_node_results()
-> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let endpoint = format!("http://{}", listener.local_addr()?);
    let server = std::thread::spawn(move || -> std::io::Result<()> {
        for _ in 0..4 {
            let (mut stream, _) = listener.accept()?;
            let mut request_line = String::new();
            BufReader::new(stream.try_clone()?).read_line(&mut request_line)?;
            let response = if request_line.contains("/group/Auto%20HK/delay?") {
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_owned()
            } else if request_line.contains("/proxies/HK-01/delay?") {
                "HTTP/1.1 200 OK\r\nContent-Length: 12\r\nConnection: close\r\n\r\n{\"delay\":42}"
                    .to_owned()
            } else if request_line.contains("/proxies/HK-02/delay?") {
                "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_owned()
            } else {
                let body =
                    r#"{"name":"Auto HK","type":"URLTest","now":"HK-01","all":["HK-01","HK-02"]}"#;
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
            };
            stream.write_all(response.as_bytes())?;
        }
        Ok(())
    });
    let runtime = super::ControllerRuntime::Fixture { endpoint };

    let result = runtime.test_policy_group_delay(
        "Auto HK",
        &[
            super::ProxyDelayTarget::direct("HK-01"),
            super::ProxyDelayTarget::direct("HK-02"),
        ],
    )?;
    server.join().map_err(|_| "fixture server panicked")??;

    assert_eq!(result.current.as_deref(), Some("HK-01"));
    assert_eq!(result.delays.get("HK-01"), Some(&42));
    assert!(!result.delays.contains_key("HK-02"));
    Ok(())
}

#[test]
fn policy_benchmark_fallback_keeps_subscription_provider_identity()
-> Result<(), Box<dyn std::error::Error>> {
    for (group_status, group_body) in [
        (504, r#"{"message":"test timed out"}"#),
        (200, "{}"),
        (200, r#"{"HK 01":0,"unrelated":42}"#),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let endpoint = format!("http://{}", listener.local_addr()?);
        let server = std::thread::spawn(move || -> std::io::Result<Vec<String>> {
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            let mut requests = Vec::new();
            while requests.len() < 3 && std::time::Instant::now() < deadline {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => return Err(error),
                };
                // Accepted sockets can inherit nonblocking mode on macOS.
                stream.set_nonblocking(false)?;
                stream.set_read_timeout(Some(Duration::from_secs(1)))?;
                let mut request = String::new();
                BufReader::new(stream.try_clone()?).read_line(&mut request)?;
                let (status, body) = if request.contains("/group/Auto%20HK/delay?") {
                    (group_status, group_body)
                } else if request
                    .contains("/providers/proxies/Subscription%201/HK%2001/healthcheck?")
                {
                    (200, r#"{"delay":42}"#)
                } else if request.starts_with("GET /proxies/Auto%20HK HTTP/") {
                    (200, r#"{"type":"URLTest","now":"HK 01","all":["HK 01"]}"#)
                } else {
                    (404, r#"{"message":"Resource not found"}"#)
                };
                write!(
                    stream,
                    "HTTP/1.1 {status} Result\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )?;
                requests.push(request);
            }
            Ok(requests)
        });
        let runtime = super::ControllerRuntime::Fixture { endpoint };
        let result = runtime.test_policy_group_delay(
            "Auto HK",
            &[super::ProxyDelayTarget::provider("Subscription 1", "HK 01")],
        );
        let requests = server.join().map_err(|_| "fixture server panicked")??;
        assert!(
            requests
                .iter()
                .any(|path| path
                    .contains("/providers/proxies/Subscription%201/HK%2001/healthcheck?")),
            "provider-owned fallback must not call /proxies/HK%2001/delay: {requests:?}"
        );
        assert_eq!(result?.delays.get("HK 01"), Some(&42));
    }
    Ok(())
}

#[test]
fn policy_benchmark_reports_fallback_error_and_rejects_zero_delay()
-> Result<(), Box<dyn std::error::Error>> {
    for (status, body) in [(503, ""), (200, r#"{"delay":0}"#)] {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let endpoint = format!("http://{}", listener.local_addr()?);
        let server = std::thread::spawn(move || -> std::io::Result<()> {
            for (status, body) in [(404, ""), (status, body)] {
                let (mut stream, _) = listener.accept()?;
                let mut line = String::new();
                BufReader::new(stream.try_clone()?).read_line(&mut line)?;
                write!(
                    stream,
                    "HTTP/1.1 {status} Result\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )?;
            }
            Ok(())
        });
        let runtime = super::ControllerRuntime::Fixture { endpoint };
        let result = runtime.test_policy_group_delay(
            "Auto HK",
            &[super::ProxyDelayTarget::provider("Subscription 1", "HK 01")],
        );
        server.join().map_err(|_| "fixture server panicked")??;
        if status == 503 {
            assert!(matches!(
                result,
                Err(super::LoadError::Mihomo(
                    manis_mihomo::MihomoError::HttpStatus {
                        status_code: 503,
                        ..
                    }
                ))
            ));
        } else {
            assert!(matches!(result, Err(super::LoadError::NoLatencyResults)));
        }
    }
    Ok(())
}

#[test]
fn policy_benchmark_targets_distinguish_provider_nodes_from_nested_groups() {
    use super::ProxyDelayTarget;
    use manis_core::{PolicyCandidateKind, PolicyNode, ProxyId};
    let mut candidate = PolicyNode {
        id: ProxyId::new("node"),
        name: "HK 01".to_owned(),
        kind: PolicyCandidateKind::Node,
        provider: Some("Subscription 1".to_owned()),
        detail: "Trojan".to_owned(),
        latency_ms: None,
        alive: None,
    };
    assert_eq!(
        ProxyDelayTarget::from_policy_node(&candidate),
        ProxyDelayTarget::provider("Subscription 1", "HK 01")
    );
    candidate.kind = PolicyCandidateKind::PolicyGroup;
    assert_eq!(
        ProxyDelayTarget::from_policy_node(&candidate),
        ProxyDelayTarget::direct("HK 01")
    );
    candidate.kind = PolicyCandidateKind::Node;
    candidate.provider = None;
    assert_eq!(
        ProxyDelayTarget::from_policy_node(&candidate),
        ProxyDelayTarget::direct("HK 01")
    );
}

#[test]
fn fixture_runtime_rejects_managed_policy_changes() {
    use manis_core::RoutingMode;

    let runtime = super::ControllerRuntime::Fixture {
        endpoint: "http://127.0.0.1:9".to_owned(),
    };

    assert!(
        runtime
            .select_policy_candidate("Manis Group", "Candidate")
            .is_err()
    );
    assert!(runtime.set_routing_mode(RoutingMode::Global).is_err());
    assert!(runtime.select_global_node("Candidate").is_err());
}
