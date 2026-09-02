use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use manis_mihomo::{ControllerConfig, LiveController};

fn server(
    serve: impl FnOnce(TcpStream) + Send + 'static,
) -> (LiveController, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = ControllerConfig::new(format!("http://{}", listener.local_addr().unwrap()))
        .unwrap()
        .with_timeouts(Duration::from_secs(1), Duration::from_secs(2));
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        let mut byte = [0];
        while !request.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        serve(stream);
    });
    (LiveController::loopback(config), handle)
}

#[test]
fn chunked_stream_retains_partial_headers_size_lines_and_frames_across_idle_polls() {
    let (controller, handle) = server(|mut stream| {
        for part in [
            "HTTP/1.1 200 O",
            "K\r\nTransfer-Encoding: chu",
            "nked\r\n\r\n8\r",
            "\n{\"a\":1}\n\r\n6\r\n{\"b\":2\r",
            "\n2\r\n}\n\r\nE\r\n{\"c\":3}{\"d\":4}\r\n0\r\n\r\n",
        ] {
            stream.write_all(part.as_bytes()).unwrap();
            // Longer than the adapter's 100ms cancellation poll, not an HTTP failure.
            std::thread::sleep(Duration::from_millis(160));
        }
    });
    let mut count = 0;
    controller
        .stream_connections(
            Duration::from_millis(100),
            &Arc::new(AtomicBool::new(false)),
            |_| count += 1,
        )
        .unwrap();
    handle.join().unwrap();
    assert_eq!(count, 4);
}

#[test]
fn cancellation_interrupts_idle_body_and_incomplete_headers() {
    for headers in [
        "HTTP/1.1 200 O",
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n1",
    ] {
        let (ready_tx, ready_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (controller, handle) = server(move |mut stream| {
            stream.write_all(headers.as_bytes()).unwrap();
            ready_tx.send(()).unwrap();
            let _ = release_rx.recv_timeout(Duration::from_secs(3));
        });
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let (done_tx, done_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result =
                controller.stream_logs("info", &worker_cancelled, |_| panic!("no complete frame"));
            done_tx.send(result).unwrap();
        });
        ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let started = Instant::now();
        cancelled.store(true, Ordering::Relaxed);
        done_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("cancellation must not wait for EOF")
            .unwrap();
        assert!(started.elapsed() < Duration::from_secs(1));
        release_tx.send(()).unwrap();
        worker.join().unwrap();
        handle.join().unwrap();
    }
}

#[test]
fn streaming_obeys_content_length_without_waiting_for_eof() {
    let (release_tx, release_rx) = mpsc::channel();
    let (controller, handle) = server(move |mut stream| {
        let body = r#"{"type":"info","payload":"one"}{"type":"warning","payload":"two"}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let _ = release_rx.recv_timeout(Duration::from_secs(3));
    });
    let started = Instant::now();
    let mut messages = Vec::new();
    controller
        .stream_logs("info", &Arc::new(AtomicBool::new(false)), |entry| {
            messages.push(entry.payload);
        })
        .unwrap();
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(messages, ["one", "two"]);
    release_tx.send(()).unwrap();
    handle.join().unwrap();
}

#[test]
fn cancelled_stream_never_connects() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let config =
        ControllerConfig::new(format!("http://{}", listener.local_addr().unwrap())).unwrap();
    LiveController::loopback(config)
        .stream_logs("info", &Arc::new(AtomicBool::new(true)), |_| {})
        .unwrap();
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
}

#[test]
fn response_header_deadline_does_not_wait_for_idle_server_to_close() {
    let (release_tx, release_rx) = mpsc::channel();
    let (controller, handle) = server(move |mut stream| {
        stream.write_all(b"HTTP/1.1 200 O").unwrap();
        let _ = release_rx.recv_timeout(Duration::from_secs(4));
    });
    let started = Instant::now();
    let config = match controller {
        LiveController::LoopbackHttp(config) => config,
        #[cfg(unix)]
        LiveController::UnixSocket { .. } => unreachable!(),
    };
    assert!(
        manis_mihomo::MihomoClient::new(config, manis_mihomo::StdHttpTransport::default())
            .fetch_version()
            .is_err()
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "headers have a 2s deadline"
    );
    release_tx.send(()).unwrap();
    handle.join().unwrap();
}

#[test]
fn quiet_log_stream_survives_request_and_response_deadlines() {
    let (controller, handle) = server(|mut stream| {
        // Mihomo does not flush the HTTP headers until its first log entry.
        std::thread::sleep(Duration::from_millis(2200));
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
            .unwrap();
        let body = "{\"type\":\"info\",\"payload\":\"after idle\"}\n";
        for _ in 0..2 {
            std::thread::sleep(Duration::from_millis(2200));
            write!(stream, "{:X}\r\n{body}\r\n", body.len()).unwrap();
        }
        stream.write_all(b"0\r\n\r\n").unwrap();
    });
    let mut messages = Vec::new();
    let result = controller.stream_logs("info", &Arc::new(AtomicBool::new(false)), |entry| {
        messages.push(entry.payload);
    });
    assert!(
        result.is_ok(),
        "a quiet healthy stream must not time out: {result:?}"
    );
    handle.join().unwrap();
    assert_eq!(messages, ["after idle", "after idle"]);
}

#[test]
fn stream_rejects_oversized_json_frame_and_truncated_json() {
    for body in [
        format!("{{\"payload\":\"{}", "x".repeat(2 * 1024 * 1024)),
        "{\"payload\":".to_owned(),
    ] {
        let (controller, handle) = server(move |mut stream| {
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
        });
        assert!(
            controller
                .stream_logs("info", &Arc::new(AtomicBool::new(false)), |_| {})
                .is_err()
        );
        handle.join().unwrap();
    }
}
