use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(unix)]
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

#[cfg(unix)]
use crate::http::validate_unix_socket_path;
use crate::http::{
    build_request, has_transfer_encoding, parse_status_line, read_headers, validate_path,
};
use crate::{ConnectionsState, ControllerConfig, MihomoError, MihomoLogEntry};

const STREAM_READ_TIMEOUT: Duration = Duration::from_millis(750);
const STREAM_FRAME_LIMIT: usize = 2 * 1024 * 1024;
const STREAM_LINE_LIMIT: usize = 64 * 1024;

/// A local Mihomo controller target suitable for long-lived HTTP streams.
#[derive(Clone, Debug)]
pub enum LiveController {
    LoopbackHttp(ControllerConfig),
    #[cfg(unix)]
    UnixSocket {
        config: ControllerConfig,
        path: PathBuf,
    },
}

impl LiveController {
    #[must_use]
    pub fn loopback(config: ControllerConfig) -> Self {
        Self::LoopbackHttp(config)
    }

    #[cfg(unix)]
    #[must_use]
    pub fn unix_socket(config: ControllerConfig, path: impl Into<PathBuf>) -> Self {
        Self::UnixSocket {
            config,
            path: path.into(),
        }
    }

    /// Streams complete connection snapshots until cancellation or a transport failure.
    ///
    /// # Errors
    /// Returns a bounded HTTP, I/O, or JSON error without including controller credentials.
    pub fn stream_connections(
        &self,
        interval: Duration,
        cancelled: &AtomicBool,
        mut receive: impl FnMut(ConnectionsState),
    ) -> Result<(), MihomoError> {
        let millis = interval.as_millis().clamp(100, 60_000);
        let path = format!("/connections?interval={millis}");
        self.stream_json(&path, cancelled, |frame| {
            let state = serde_json::from_str(frame).map_err(|source| MihomoError::Json {
                endpoint: "/connections".to_owned(),
                source,
            })?;
            receive(state);
            Ok(())
        })
    }

    /// Streams Mihomo kernel log entries until cancellation or a transport failure.
    ///
    /// # Errors
    /// Returns a bounded HTTP, I/O, or JSON error without including controller credentials.
    pub fn stream_logs(
        &self,
        level: &str,
        cancelled: &AtomicBool,
        mut receive: impl FnMut(MihomoLogEntry),
    ) -> Result<(), MihomoError> {
        let level = match level {
            "debug" | "info" | "warning" | "error" | "silent" => level,
            _ => {
                return Err(MihomoError::InvalidConfig(
                    "unsupported live log level".to_owned(),
                ));
            }
        };
        let path = format!("/logs?level={level}");
        self.stream_json(&path, cancelled, |frame| {
            let entry = serde_json::from_str(frame).map_err(|source| MihomoError::Json {
                endpoint: "/logs".to_owned(),
                source,
            })?;
            receive(entry);
            Ok(())
        })
    }

    fn stream_json(
        &self,
        path: &str,
        cancelled: &AtomicBool,
        receive: impl FnMut(&str) -> Result<(), MihomoError>,
    ) -> Result<(), MihomoError> {
        validate_path(path)?;
        match self {
            Self::LoopbackHttp(config) => {
                let request = build_request(config, "GET", path, None, true)?;
                let mut addresses = (config.host(), config.port()).to_socket_addrs()?;
                let Some(address) = addresses.find(|address| address.ip().is_loopback()) else {
                    return Err(MihomoError::InvalidConfig(
                        "controller did not resolve to a loopback address".to_owned(),
                    ));
                };
                let stream = TcpStream::connect_timeout(&address, config.connect_timeout())?;
                stream.set_read_timeout(Some(STREAM_READ_TIMEOUT))?;
                stream.set_write_timeout(Some(config.connect_timeout()))?;
                stream_response(stream, &request, cancelled, receive)
            }
            #[cfg(unix)]
            Self::UnixSocket {
                config,
                path: socket,
            } => {
                validate_unix_socket_path(socket)?;
                let request = build_request(config, "GET", path, None, false)?;
                let stream = UnixStream::connect(socket)?;
                stream.set_read_timeout(Some(STREAM_READ_TIMEOUT))?;
                stream.set_write_timeout(Some(config.connect_timeout()))?;
                stream_response(stream, &request, cancelled, receive)
            }
        }
    }
}

fn stream_response(
    mut stream: impl Read + Write,
    request: &str,
    cancelled: &AtomicBool,
    mut receive: impl FnMut(&str) -> Result<(), MihomoError>,
) -> Result<(), MihomoError> {
    stream.write_all(request.as_bytes())?;
    stream.flush()?;
    let mut reader = BufReader::new(stream);
    let status_line = read_stream_line(&mut reader, cancelled, "HTTP status line")?
        .ok_or_else(|| MihomoError::InvalidResponse("empty HTTP response".to_owned()))?;
    let (status_code, reason) = parse_status_line(&status_line)?;
    let headers = read_headers(&mut reader)?;
    if !(200..=299).contains(&status_code) {
        return Err(MihomoError::HttpStatus {
            status_code,
            reason,
            body_preview: String::new(),
        });
    }

    let mut frames = FrameBuffer::default();
    if has_transfer_encoding(&headers, "chunked") {
        loop {
            if cancelled.load(Ordering::Relaxed) {
                return Ok(());
            }
            let Some(size_line) = read_stream_line(&mut reader, cancelled, "chunk size line")?
            else {
                return Err(MihomoError::InvalidResponse(
                    "chunked stream ended before size line".to_owned(),
                ));
            };
            let size_text = size_line
                .trim_end_matches(['\r', '\n'])
                .split_once(';')
                .map_or(size_line.trim_end_matches(['\r', '\n']), |(size, _)| size);
            let size = usize::from_str_radix(size_text.trim(), 16).map_err(|_error| {
                MihomoError::InvalidResponse("invalid stream chunk".to_owned())
            })?;
            if size == 0 {
                frames.finish(&mut receive)?;
                return Ok(());
            }
            if size > STREAM_FRAME_LIMIT {
                return Err(MihomoError::BodyTooLarge {
                    limit: STREAM_FRAME_LIMIT,
                });
            }
            let mut chunk = vec![0_u8; size];
            read_exact_cancellable(&mut reader, &mut chunk, cancelled)?;
            if cancelled.load(Ordering::Relaxed) {
                return Ok(());
            }
            let mut crlf = [0_u8; 2];
            read_exact_cancellable(&mut reader, &mut crlf, cancelled)?;
            if cancelled.load(Ordering::Relaxed) {
                return Ok(());
            }
            if crlf != *b"\r\n" {
                return Err(MihomoError::InvalidResponse(
                    "stream chunk was not terminated".to_owned(),
                ));
            }
            frames.push(&chunk, &mut receive)?;
        }
    }

    let mut buffer = [0_u8; 8192];
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(());
        }
        match reader.read(&mut buffer) {
            Ok(0) => {
                frames.finish(&mut receive)?;
                return Ok(());
            }
            Ok(read) => frames.push(&buffer[..read], &mut receive)?,
            Err(error) if is_timeout(&error) => {}
            Err(error) => return Err(error.into()),
        }
    }
}

#[derive(Default)]
struct FrameBuffer {
    bytes: Vec<u8>,
}

impl FrameBuffer {
    fn push(
        &mut self,
        bytes: &[u8],
        receive: &mut impl FnMut(&str) -> Result<(), MihomoError>,
    ) -> Result<(), MihomoError> {
        if self.bytes.len().saturating_add(bytes.len()) > STREAM_FRAME_LIMIT {
            return Err(MihomoError::BodyTooLarge {
                limit: STREAM_FRAME_LIMIT,
            });
        }
        self.bytes.extend_from_slice(bytes);
        self.emit_complete_json(receive)
    }

    fn finish(
        &mut self,
        receive: &mut impl FnMut(&str) -> Result<(), MihomoError>,
    ) -> Result<(), MihomoError> {
        let frame = std::mem::take(&mut self.bytes);
        emit_frame(&frame, receive)
    }

    fn emit_complete_json(
        &mut self,
        receive: &mut impl FnMut(&str) -> Result<(), MihomoError>,
    ) -> Result<(), MihomoError> {
        loop {
            let leading = self
                .bytes
                .iter()
                .position(|byte| !byte.is_ascii_whitespace())
                .unwrap_or(self.bytes.len());
            if leading > 0 {
                self.bytes.drain(..leading);
            }
            if self.bytes.is_empty() {
                return Ok(());
            }
            let mut stream =
                serde_json::Deserializer::from_slice(&self.bytes).into_iter::<serde_json::Value>();
            match stream.next() {
                Some(Ok(_value)) => {
                    let consumed = stream.byte_offset();
                    let frame = std::str::from_utf8(&self.bytes[..consumed]).map_err(|_error| {
                        MihomoError::InvalidResponse("stream frame was not UTF-8".to_owned())
                    })?;
                    receive(frame)?;
                    self.bytes.drain(..consumed);
                }
                Some(Err(error)) if error.is_eof() => return Ok(()),
                Some(Err(_error)) => {
                    return Err(MihomoError::InvalidResponse(
                        "stream contained invalid JSON".to_owned(),
                    ));
                }
                None => return Ok(()),
            }
        }
    }
}

fn emit_frame(
    frame: &[u8],
    receive: &mut impl FnMut(&str) -> Result<(), MihomoError>,
) -> Result<(), MihomoError> {
    if frame.iter().all(u8::is_ascii_whitespace) {
        return Ok(());
    }
    let frame = std::str::from_utf8(frame)
        .map_err(|_error| MihomoError::InvalidResponse("stream frame was not UTF-8".to_owned()))?;
    receive(frame)
}

fn read_stream_line(
    reader: &mut impl BufRead,
    cancelled: &AtomicBool,
    context: &str,
) -> Result<Option<String>, MihomoError> {
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let mut bytes = Vec::new();
        match reader
            .take((STREAM_LINE_LIMIT + 1) as u64)
            .read_until(b'\n', &mut bytes)
        {
            Ok(0) => return Ok(None),
            Ok(_) if bytes.len() > STREAM_LINE_LIMIT => {
                return Err(MihomoError::InvalidResponse(format!(
                    "{context} exceeded limit"
                )));
            }
            Ok(_) if !bytes.ends_with(b"\n") => {
                return Err(MihomoError::InvalidResponse(format!(
                    "{context} was not terminated"
                )));
            }
            Ok(_) => {
                return String::from_utf8(bytes).map(Some).map_err(|_error| {
                    MihomoError::InvalidResponse(format!("{context} was not UTF-8"))
                });
            }
            Err(error) if is_timeout(&error) => {}
            Err(error) => return Err(error.into()),
        }
    }
}

fn read_exact_cancellable(
    reader: &mut impl Read,
    mut output: &mut [u8],
    cancelled: &AtomicBool,
) -> Result<(), MihomoError> {
    while !output.is_empty() {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(());
        }
        match reader.read(output) {
            Ok(0) => return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into()),
            Ok(read) => output = &mut output[read..],
            Err(error) if is_timeout(&error) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn is_timeout(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    )
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use super::stream_response;

    #[test]
    fn chunked_stream_emits_split_json_lines() {
        let response = concat!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n",
            "8\r\n{\"a\":1}\n\r\n",
            "6\r\n{\"b\":2\r\n",
            "2\r\n}\n\r\n",
            "0\r\n\r\n"
        );
        let mut frames = Vec::new();
        let request = "GET /logs HTTP/1.1\r\n\r\n";
        let stream = FixtureStream::new(response.as_bytes());

        stream_response(stream, request, &AtomicBool::new(false), |frame| {
            frames.push(frame.to_owned());
            Ok(())
        })
        .expect("fixture chunked stream should decode");

        assert_eq!(frames, vec!["{\"a\":1}", "{\"b\":2}"]);
    }

    #[test]
    fn chunked_stream_emits_adjacent_json_without_newlines() {
        let response = concat!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n",
            "E\r\n{\"a\":1}{\"b\":2}\r\n",
            "0\r\n\r\n"
        );
        let mut frames = Vec::new();

        stream_response(
            FixtureStream::new(response.as_bytes()),
            "GET /connections HTTP/1.1\r\n\r\n",
            &AtomicBool::new(false),
            |frame| {
                frames.push(frame.to_owned());
                Ok(())
            },
        )
        .expect("adjacent JSON values should be framed");

        assert_eq!(frames, vec!["{\"a\":1}", "{\"b\":2}"]);
    }

    struct FixtureStream<'a> {
        input: &'a [u8],
    }

    impl<'a> FixtureStream<'a> {
        fn new(input: &'a [u8]) -> Self {
            Self { input }
        }
    }

    impl std::io::Read for FixtureStream<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let read = buffer.len().min(self.input.len());
            buffer[..read].copy_from_slice(&self.input[..read]);
            self.input = &self.input[read..];
            Ok(read)
        }
    }

    impl std::io::Write for FixtureStream<'_> {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            Ok(buffer.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
}
