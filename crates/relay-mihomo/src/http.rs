use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};

use crate::{ControllerConfig, MihomoError};

pub const DEFAULT_BODY_LIMIT_BYTES: usize = 16 * 1024 * 1024;
const HEADER_LIMIT_BYTES: usize = 64 * 1024;
const BODY_PREVIEW_BYTES: usize = 512;

/// Read-only transport for Mihomo controller `GET` requests.
pub trait ReadonlyTransport {
    /// Issues a `GET` request to a controller path and returns the response body.
    ///
    /// # Errors
    ///
    /// Returns an error when the request path is invalid, the transport fails, the response is
    /// malformed, the status is non-successful, or the body exceeds the configured limit.
    fn get(&self, config: &ControllerConfig, path: &str) -> Result<String, MihomoError>;
}

impl<T> ReadonlyTransport for &T
where
    T: ReadonlyTransport + ?Sized,
{
    fn get(&self, config: &ControllerConfig, path: &str) -> Result<String, MihomoError> {
        (*self).get(config, path)
    }
}

#[derive(Debug, Clone)]
pub struct StdHttpTransport {
    body_limit: usize,
}

impl StdHttpTransport {
    #[must_use]
    pub fn with_body_limit(body_limit: usize) -> Self {
        Self { body_limit }
    }
}

impl Default for StdHttpTransport {
    fn default() -> Self {
        Self {
            body_limit: DEFAULT_BODY_LIMIT_BYTES,
        }
    }
}

impl ReadonlyTransport for StdHttpTransport {
    fn get(&self, config: &ControllerConfig, path: &str) -> Result<String, MihomoError> {
        validate_path(path)?;
        let request = build_get_request(config, path)?;
        let mut addresses = (config.host(), config.port()).to_socket_addrs()?;
        let Some(address) = addresses.find(|address| address.ip().is_loopback()) else {
            return Err(MihomoError::InvalidConfig(format!(
                "controller host {} did not resolve to a loopback address",
                config.host()
            )));
        };

        let mut stream = TcpStream::connect_timeout(&address, config.connect_timeout())?;
        stream.set_read_timeout(Some(config.read_timeout()))?;
        stream.set_write_timeout(Some(config.connect_timeout()))?;
        stream.write_all(request.as_bytes())?;
        stream.flush()?;

        decode_http_response(stream, self.body_limit)
    }
}

fn validate_path(path: &str) -> Result<(), MihomoError> {
    if !path.starts_with('/')
        || path
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte == b' ')
    {
        return Err(MihomoError::InvalidRequestPath(path.to_owned()));
    }
    Ok(())
}

fn build_get_request(config: &ControllerConfig, path: &str) -> Result<String, MihomoError> {
    let mut request = format!(
        "GET {path} HTTP/1.1\r\nHost: {}\r\nAccept: application/json\r\nUser-Agent: relay-mihomo/0.1\r\nConnection: close\r\n",
        config.authority()
    );

    if let Some(secret) = config.secret() {
        if secret.bytes().any(|byte| byte.is_ascii_control()) {
            return Err(MihomoError::InvalidConfig(
                "controller secret must not contain control characters".to_owned(),
            ));
        }
        request.push_str("Authorization: Bearer ");
        request.push_str(secret);
        request.push_str("\r\n");
    }

    request.push_str("\r\n");
    Ok(request)
}

fn decode_http_response(reader: impl Read, body_limit: usize) -> Result<String, MihomoError> {
    let mut reader = BufReader::new(reader);
    let Some(status_line) = read_limited_line(&mut reader, HEADER_LIMIT_BYTES, "HTTP status line")?
    else {
        return Err(MihomoError::InvalidResponse(
            "empty HTTP response".to_owned(),
        ));
    };

    let (status_code, reason) = parse_status_line(&status_line)?;
    let headers = read_headers(&mut reader)?;
    let body = read_body(&mut reader, &headers, body_limit)?;
    let body = String::from_utf8(body)
        .map_err(|error| MihomoError::InvalidResponse(format!("body was not UTF-8: {error}")))?;

    if !(200..=299).contains(&status_code) {
        return Err(MihomoError::HttpStatus {
            status_code,
            reason,
            body_preview: preview(&body),
        });
    }

    Ok(body)
}

fn parse_status_line(status_line: &str) -> Result<(u16, String), MihomoError> {
    let trimmed = status_line.trim_end_matches(['\r', '\n']);
    let mut parts = trimmed.splitn(3, ' ');
    let Some(version) = parts.next() else {
        return Err(MihomoError::InvalidResponse(
            "missing HTTP version".to_owned(),
        ));
    };
    if !version.starts_with("HTTP/") {
        return Err(MihomoError::InvalidResponse(format!(
            "invalid status line {trimmed}"
        )));
    }
    let Some(status) = parts.next() else {
        return Err(MihomoError::InvalidResponse(
            "missing HTTP status code".to_owned(),
        ));
    };
    let status_code = status.parse::<u16>().map_err(|_error| {
        MihomoError::InvalidResponse(format!("invalid HTTP status code {status}"))
    })?;
    let reason = match parts.next() {
        Some(reason) => reason.to_owned(),
        None => String::new(),
    };
    Ok((status_code, reason))
}

fn read_headers(reader: &mut impl BufRead) -> Result<Vec<(String, String)>, MihomoError> {
    let mut headers = Vec::new();
    let mut total = 0_usize;

    loop {
        let Some(line) = read_limited_line(reader, HEADER_LIMIT_BYTES, "HTTP header line")? else {
            return Err(MihomoError::InvalidResponse(
                "HTTP headers ended unexpectedly".to_owned(),
            ));
        };
        total = total.saturating_add(line.len());
        if total > HEADER_LIMIT_BYTES {
            return Err(MihomoError::InvalidResponse(
                "HTTP headers exceeded limit".to_owned(),
            ));
        }

        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }

        let Some((name, value)) = trimmed.split_once(':') else {
            return Err(MihomoError::InvalidResponse(format!(
                "malformed HTTP header {trimmed}"
            )));
        };
        headers.push((name.trim().to_ascii_lowercase(), value.trim().to_owned()));
    }

    Ok(headers)
}

fn read_body(
    reader: &mut impl BufRead,
    headers: &[(String, String)],
    body_limit: usize,
) -> Result<Vec<u8>, MihomoError> {
    if has_transfer_encoding(headers, "chunked") {
        return read_chunked_body(reader, body_limit);
    }

    if let Some(length) = content_length(headers)? {
        if length > body_limit {
            return Err(MihomoError::BodyTooLarge { limit: body_limit });
        }
        let mut body = vec![0_u8; length];
        reader.read_exact(&mut body)?;
        return Ok(body);
    }

    let mut body = Vec::new();
    let max_read = (body_limit as u64).saturating_add(1);
    reader.take(max_read).read_to_end(&mut body)?;
    if body.len() > body_limit {
        return Err(MihomoError::BodyTooLarge { limit: body_limit });
    }
    Ok(body)
}

fn has_transfer_encoding(headers: &[(String, String)], expected: &str) -> bool {
    headers
        .iter()
        .filter(|(name, _value)| name == "transfer-encoding")
        .flat_map(|(_name, value)| value.split(','))
        .any(|value| value.trim().eq_ignore_ascii_case(expected))
}

fn content_length(headers: &[(String, String)]) -> Result<Option<usize>, MihomoError> {
    headers
        .iter()
        .find(|(name, _value)| name == "content-length")
        .map(|(_name, value)| {
            value.parse::<usize>().map_err(|_error| {
                MihomoError::InvalidResponse(format!("invalid Content-Length {value}"))
            })
        })
        .transpose()
}

fn read_chunked_body(reader: &mut impl BufRead, body_limit: usize) -> Result<Vec<u8>, MihomoError> {
    let mut body = Vec::new();

    loop {
        let Some(line) = read_limited_line(reader, HEADER_LIMIT_BYTES, "chunk size line")? else {
            return Err(MihomoError::InvalidResponse(
                "chunked body ended before size line".to_owned(),
            ));
        };

        let size_text = line
            .trim_end_matches(['\r', '\n'])
            .split_once(';')
            .map_or(line.trim_end_matches(['\r', '\n']), |(size, _extension)| {
                size
            });
        let size = usize::from_str_radix(size_text.trim(), 16).map_err(|_error| {
            MihomoError::InvalidResponse(format!("invalid chunk size {size_text}"))
        })?;

        if size == 0 {
            consume_trailers(reader)?;
            break;
        }

        if body.len().saturating_add(size) > body_limit {
            return Err(MihomoError::BodyTooLarge { limit: body_limit });
        }

        let start = body.len();
        body.resize(start + size, 0);
        reader.read_exact(&mut body[start..])?;

        let mut crlf = [0_u8; 2];
        reader.read_exact(&mut crlf)?;
        if crlf != *b"\r\n" {
            return Err(MihomoError::InvalidResponse(
                "chunk was not terminated by CRLF".to_owned(),
            ));
        }
    }

    Ok(body)
}

fn consume_trailers(reader: &mut impl BufRead) -> Result<(), MihomoError> {
    let mut total = 0_usize;
    while let Some(line) = read_limited_line(reader, HEADER_LIMIT_BYTES, "chunk trailer line")? {
        total = total.saturating_add(line.len());
        if total > HEADER_LIMIT_BYTES {
            return Err(MihomoError::InvalidResponse(
                "chunk trailers exceeded header limit".to_owned(),
            ));
        }
        if line.trim_end_matches(['\r', '\n']).is_empty() {
            break;
        }
    }
    Ok(())
}

fn read_limited_line(
    reader: &mut impl BufRead,
    limit: usize,
    context: &str,
) -> Result<Option<String>, MihomoError> {
    let max_read = u64::try_from(limit).map_or(u64::MAX, |limit| limit.saturating_add(1));
    let mut bytes = Vec::with_capacity(limit.min(1024));
    let read = reader.take(max_read).read_until(b'\n', &mut bytes)?;
    if read == 0 {
        return Ok(None);
    }
    if bytes.len() > limit {
        return Err(MihomoError::InvalidResponse(format!(
            "{context} exceeded limit"
        )));
    }
    if !bytes.ends_with(b"\n") {
        return Err(MihomoError::InvalidResponse(format!(
            "{context} was not terminated"
        )));
    }

    String::from_utf8(bytes)
        .map(Some)
        .map_err(|error| MihomoError::InvalidResponse(format!("{context} was not UTF-8: {error}")))
}

fn preview(body: &str) -> String {
    body.chars().take(BODY_PREVIEW_BYTES).collect()
}

#[cfg(test)]
mod tests {
    use super::{build_get_request, decode_http_response};
    use crate::ControllerConfig;

    #[test]
    fn request_builder_omits_empty_secret() {
        let request = build_get_request(&ControllerConfig::default().with_secret(""), "/version")
            .expect("an empty secret should be omitted");
        assert!(!request.contains("Authorization:"));
    }

    #[test]
    fn request_builder_rejects_secret_header_injection() {
        let result = build_get_request(
            &ControllerConfig::default().with_secret("token\r\nX-Injected: yes"),
            "/version",
        );
        assert!(result.is_err());
    }

    #[test]
    fn request_path_rejects_all_whitespace_controls() {
        assert!(super::validate_path("/version\tHTTP/1.1").is_err());
    }

    #[test]
    fn response_decoder_accepts_connection_close_body() -> Result<(), Box<dyn std::error::Error>> {
        let response = b"HTTP/1.1 200 OK\r\n\r\n{\"version\":\"x\"}";
        let body = decode_http_response(&response[..], 1024)?;
        assert_eq!(body, r#"{"version":"x"}"#);
        Ok(())
    }

    #[test]
    fn response_decoder_rejects_oversized_chunk_trailers() {
        let trailer = "x".repeat(super::HEADER_LIMIT_BYTES + 1);
        let response = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\nX-Trailer: {trailer}\r\n\r\n"
        );

        let error = decode_http_response(response.as_bytes(), 1024)
            .expect_err("oversized trailer lines must be rejected");
        assert!(matches!(error, crate::MihomoError::InvalidResponse(_)));
    }

    #[test]
    fn response_decoder_rejects_every_oversized_http_line_kind() {
        let oversized = "x".repeat(super::HEADER_LIMIT_BYTES + 1);
        let responses = [
            format!("HTTP/1.1 200 {oversized}\r\n\r\n"),
            format!("HTTP/1.1 200 OK\r\nX-Header: {oversized}\r\n\r\n"),
            format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{oversized}\r\n"),
        ];

        for response in responses {
            let error = decode_http_response(response.as_bytes(), 1024)
                .expect_err("oversized HTTP lines must be rejected");
            assert!(matches!(error, crate::MihomoError::InvalidResponse(_)));
        }
    }
}
