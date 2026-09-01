use std::io::Read;
#[cfg(unix)]
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use serde::de::IgnoredAny;
use ureq::http::Method;

use crate::http::{Target, body_error};
use crate::{ConnectionsState, ControllerConfig, MihomoError, MihomoLogEntry};

const STREAM_FRAME_LIMIT: usize = 2 * 1024 * 1024;

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
        cancelled: &Arc<AtomicBool>,
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
        cancelled: &Arc<AtomicBool>,
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
        cancelled: &Arc<AtomicBool>,
        receive: impl FnMut(&str) -> Result<(), MihomoError>,
    ) -> Result<(), MihomoError> {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(());
        }
        let target = match self {
            Self::LoopbackHttp(config) => Target::Loopback(config),
            #[cfg(unix)]
            Self::UnixSocket { config, path } => Target::Unix(config, path),
        };
        let result = (|| {
            let mut response =
                target.response(Method::GET, path, None, Some(Arc::clone(cancelled)))?;
            read_frames(response.body_mut().as_reader(), cancelled, receive)
        })();
        if cancelled.load(Ordering::Relaxed) {
            Ok(())
        } else {
            result
        }
    }
}

fn read_frames(
    mut reader: impl Read,
    cancelled: &AtomicBool,
    mut receive: impl FnMut(&str) -> Result<(), MihomoError>,
) -> Result<(), MihomoError> {
    let mut frames = FrameBuffer::default();
    let mut bytes = [0_u8; 8192];
    while !cancelled.load(Ordering::Relaxed) {
        match reader.read(&mut bytes).map_err(body_error)? {
            0 => return frames.finish(&mut receive),
            read => frames.push(&bytes[..read], &mut receive)?,
        }
    }
    Ok(())
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
                serde_json::Deserializer::from_slice(&self.bytes).into_iter::<IgnoredAny>();
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

#[cfg(test)]
mod tests {
    use super::{FrameBuffer, emit_frame};
    use crate::MihomoError;

    #[test]
    fn frames_emit_split_and_adjacent_json() {
        let mut buffer = FrameBuffer::default();
        let mut frames = Vec::new();
        let mut receive = |frame: &str| {
            frames.push(frame.to_owned());
            Ok(())
        };
        for bytes in [b"{\"a\":1}\n{\"b\":2".as_slice(), b"}{\"c\":3}"] {
            buffer.push(bytes, &mut receive).unwrap();
        }
        buffer.finish(&mut receive).unwrap();
        assert_eq!(frames, [r#"{"a":1}"#, r#"{"b":2}"#, r#"{"c":3}"#]);
    }

    #[test]
    fn incomplete_frames_are_bounded() {
        let mut buffer = FrameBuffer::default();
        let input = vec![b' '; super::STREAM_FRAME_LIMIT + 1];
        assert!(buffer.push(&input, &mut |_| Ok(())).is_err());
    }

    #[test]
    fn invalid_utf8_frame_is_rejected() {
        let error = emit_frame(&[0xff], &mut |_| Ok(())).expect_err("invalid UTF-8 must fail");

        assert!(matches!(
            error,
            MihomoError::InvalidResponse(message) if message == "stream frame was not UTF-8"
        ));
    }

    #[test]
    fn non_eof_json_error_is_rejected_immediately() {
        let mut buffer = FrameBuffer::default();
        let error = buffer
            .push(b"}", &mut |_| Ok(()))
            .expect_err("invalid JSON must fail");

        assert!(matches!(
            error,
            MihomoError::InvalidResponse(message) if message == "stream contained invalid JSON"
        ));
    }

    #[test]
    fn whitespace_only_final_frame_is_ignored() {
        let mut buffer = FrameBuffer {
            bytes: b" \n\t".to_vec(),
        };
        let mut called = false;

        buffer
            .finish(&mut |_| {
                called = true;
                Ok(())
            })
            .expect("whitespace should be accepted");

        assert!(!called);
    }

    #[test]
    fn incomplete_final_json_is_left_to_the_typed_receiver() {
        let mut buffer = FrameBuffer::default();
        buffer
            .push(b"{\"a\":", &mut |_| Ok(()))
            .expect("incomplete JSON should remain buffered");

        let error = buffer
            .finish(&mut |frame| {
                serde_json::from_str::<serde_json::Value>(frame)
                    .map(|_| ())
                    .map_err(|source| MihomoError::Json {
                        endpoint: "/test".to_owned(),
                        source,
                    })
            })
            .expect_err("typed parsing must reject incomplete final JSON");

        assert!(matches!(error, MihomoError::Json { .. }));
    }
}
