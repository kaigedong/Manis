use super::*;

#[test]
fn http_decoder_accepts_close_delimited_body() -> Result<(), Box<dyn std::error::Error>> {
    let (address, server) = spawn_one_response_server("HTTP/1.1 200 OK\r\n\r\n{\"ok\":true}")?;
    let config = ControllerConfig::new(format!("http://{address}"))?;
    assert_eq!(
        StdHttpTransport::default().get(&config, "/version")?,
        r#"{"ok":true}"#
    );
    server.join().unwrap();
    Ok(())
}

#[test]
fn http_decoder_accepts_headers_within_the_configured_limit()
-> Result<(), Box<dyn std::error::Error>> {
    let (address, server) = spawn_one_response_server(&format!(
        "HTTP/1.1 200 OK\r\nX-Large: {}\r\nContent-Length: 2\r\n\r\n{{}}",
        "x".repeat(32 * 1024)
    ))?;
    let config = ControllerConfig::new(format!("http://{address}"))?;
    assert_eq!(StdHttpTransport::default().get(&config, "/version")?, "{}");
    server.join().unwrap();
    Ok(())
}

#[test]
fn chunk_trailers_are_discarded_with_the_closed_connection()
-> Result<(), Box<dyn std::error::Error>> {
    // ureq ends the body at the zero chunk; Manis neither retains trailers nor reuses this socket.
    let (address, server) = spawn_one_response_server(&format!(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{{}}\r\n0\r\nX-Trailer: {}\r\n\r\n",
        "x".repeat(64 * 1024 + 1)
    ))?;
    let config = ControllerConfig::new(format!("http://{address}"))?;
    assert_eq!(
        StdHttpTransport::with_body_limit(2).get(&config, "/version")?,
        "{}"
    );
    server.join().unwrap();
    Ok(())
}

#[test]
fn http_decoder_rejects_malformed_and_oversized_metadata() -> Result<(), Box<dyn std::error::Error>>
{
    let oversized = "x".repeat(64 * 1024 + 1);
    for (index, response) in [
        format!("HTTP/1.1 200 {oversized}\r\n\r\n"),
        format!("HTTP/1.1 200 OK\r\nX-Header: {oversized}\r\n\r\n"),
        format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{oversized}\r\n"),
        "HTTP/1.1 200 OK\r\nContent-Length: 50\r\n\r\ntruncated".to_owned(),
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n1\r\nxINVALID\r\n0\r\n\r\n"
            .to_owned(),
    ]
    .into_iter()
    .enumerate()
    {
        let (address, server) = spawn_one_response_server(&response)?;
        let config = ControllerConfig::new(format!("http://{address}"))?;
        assert!(
            StdHttpTransport::with_body_limit(1024)
                .get(&config, "/version")
                .is_err(),
            "case {index}"
        );
        server.join().unwrap();
    }
    Ok(())
}

#[test]
fn redirect_does_not_follow_or_expose_secrets() -> Result<(), Box<dyn std::error::Error>> {
    let destination = TcpListener::bind("127.0.0.1:0")?;
    destination.set_nonblocking(true)?;
    let (address, server) = spawn_one_response_server(&format!(
        "HTTP/1.1 302 secret-token\r\nLocation: http://{}/stolen\r\nContent-Length: 12\r\n\r\nsecret-token",
        destination.local_addr()?
    ))?;
    let config = ControllerConfig::new(format!("http://{address}"))?.with_secret("secret-token");
    let error = StdHttpTransport::default()
        .get(&config, "/version")
        .unwrap_err();
    assert!(matches!(
        error,
        MihomoError::HttpStatus {
            status_code: 302,
            ..
        }
    ));
    assert!(!format!("{error:?} {error}").contains("secret-token"));
    assert!(matches!(destination.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
    server.join().unwrap();
    Ok(())
}
