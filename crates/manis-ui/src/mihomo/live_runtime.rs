use std::collections::VecDeque;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use manis_mihomo::{
    ConnectionsState, ControllerConfig, LiveController, MihomoError, MihomoLogEntry,
};

#[cfg(unix)]
use super::unix_socket_path;
use super::{
    LIVE_CONNECTION_INTERVAL, LIVE_LOG_MAILBOX_CAPACITY, LIVE_RETRY_MAX, LoadError,
    with_controller_secret,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct KernelLogEntry {
    pub sequence: u64,
    pub level: String,
    pub payload: String,
    pub timestamp_ms: u128,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LiveStreamStatus {
    pub activity: LiveStreamPhase,
    pub logs: LiveStreamPhase,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LiveStreamPhase {
    Waiting,
    Connecting,
    Live,
    Unavailable,
    Reconnecting(usize),
    InterruptedHttp(u16),
    InvalidData,
    ControllerUnavailable,
    Retrying,
    StartFailed(String),
}

impl Default for LiveStreamStatus {
    fn default() -> Self {
        Self {
            activity: LiveStreamPhase::Waiting,
            logs: LiveStreamPhase::Waiting,
        }
    }
}

#[derive(Default)]
pub(super) struct LiveMailbox {
    pub(super) latest_connections: Option<ConnectionsState>,
    pub(super) logs: VecDeque<KernelLogEntry>,
    pub(super) status: LiveStreamStatus,
}

pub(crate) struct LiveRuntimeUpdate {
    pub connections: Option<ConnectionsState>,
    pub logs: Vec<KernelLogEntry>,
    pub status: LiveStreamStatus,
}

pub(crate) struct LiveRuntimeSession {
    cancelled: Arc<AtomicBool>,
    mailbox: Arc<Mutex<LiveMailbox>>,
}

impl LiveRuntimeSession {
    pub(crate) fn start(
        endpoint: &str,
        controller_secret: Option<&str>,
    ) -> Result<Self, LoadError> {
        let controller = live_controller(endpoint, controller_secret)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let mailbox = Arc::new(Mutex::new(LiveMailbox::default()));
        spawn_connection_stream(controller.clone(), cancelled.clone(), mailbox.clone());
        spawn_log_stream(controller, cancelled.clone(), mailbox.clone());
        Ok(Self { cancelled, mailbox })
    }

    pub(crate) fn drain(&self) -> LiveRuntimeUpdate {
        let Ok(mut mailbox) = self.mailbox.lock() else {
            return LiveRuntimeUpdate {
                connections: None,
                logs: Vec::new(),
                status: LiveStreamStatus {
                    activity: LiveStreamPhase::Unavailable,
                    logs: LiveStreamPhase::Unavailable,
                },
            };
        };
        LiveRuntimeUpdate {
            connections: mailbox.latest_connections.take(),
            logs: mailbox.logs.drain(..).collect(),
            status: mailbox.status.clone(),
        }
    }
}

impl Drop for LiveRuntimeSession {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

fn live_controller(
    endpoint: &str,
    controller_secret: Option<&str>,
) -> Result<LiveController, MihomoError> {
    #[cfg(unix)]
    if let Some(socket_path) = unix_socket_path(endpoint)? {
        return Ok(LiveController::unix_socket(
            ControllerConfig::default(),
            socket_path,
        ));
    }

    #[cfg(not(unix))]
    if endpoint.starts_with("unix://") {
        return Err(MihomoError::InvalidConfig(
            "Unix controller sockets are not supported on this platform".to_owned(),
        ));
    }

    Ok(LiveController::loopback(with_controller_secret(
        ControllerConfig::new(endpoint)?,
        controller_secret,
    )))
}

pub(super) fn spawn_connection_stream(
    controller: LiveController,
    cancelled: Arc<AtomicBool>,
    mailbox: Arc<Mutex<LiveMailbox>>,
) {
    thread::spawn(move || {
        let mut first_request = true;
        reconnect_live_stream(&cancelled, LIVE_CONNECTION_INTERVAL, |attempt| {
            if first_request || attempt > 0 {
                set_live_status(&mailbox, true, stream_phase(attempt));
            }
            first_request = false;
            let result = controller.stream_connections(
                LIVE_CONNECTION_INTERVAL,
                &cancelled,
                |connections| {
                    if let Ok(mut mailbox) = mailbox.lock() {
                        mailbox.latest_connections = Some(connections);
                        mailbox.status.activity = LiveStreamPhase::Live;
                    }
                },
            );
            if let Err(error) = &result {
                set_live_status(&mailbox, true, safe_stream_error(error));
            }
            result
        });
    });
}

fn spawn_log_stream(
    controller: LiveController,
    cancelled: Arc<AtomicBool>,
    mailbox: Arc<Mutex<LiveMailbox>>,
) {
    thread::spawn(move || {
        let mut sequence = 0_u64;
        reconnect_live_stream(&cancelled, Duration::from_millis(250), |attempt| {
            set_live_status(&mailbox, false, stream_phase(attempt));
            let result = controller.stream_logs("info", &cancelled, |entry| {
                sequence = sequence.wrapping_add(1);
                push_kernel_log(&mailbox, sequence, &entry);
            });
            if let Err(error) = &result {
                set_live_status(&mailbox, false, safe_stream_error(error));
            }
            result
        });
    });
}

fn reconnect_live_stream(
    cancelled: &AtomicBool,
    success_delay: Duration,
    mut connect: impl FnMut(usize) -> Result<(), MihomoError>,
) {
    let mut attempt = 0_usize;
    while !cancelled.load(Ordering::Relaxed) {
        let result = connect(attempt);
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        if result.is_ok() {
            attempt = 0;
        } else {
            attempt = attempt.saturating_add(1);
        }
        let shift = u32::try_from(attempt.min(5)).unwrap_or(5);
        let delay = if result.is_ok() {
            success_delay
        } else {
            Duration::from_millis(250_u64.saturating_mul(1_u64 << shift)).min(LIVE_RETRY_MAX)
        };
        let started = std::time::Instant::now();
        while started.elapsed() < delay && !cancelled.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(50));
        }
    }
}

fn stream_phase(attempt: usize) -> LiveStreamPhase {
    if attempt == 0 {
        LiveStreamPhase::Connecting
    } else {
        LiveStreamPhase::Reconnecting(attempt)
    }
}

fn safe_stream_error(error: &MihomoError) -> LiveStreamPhase {
    match error {
        MihomoError::HttpStatus { status_code, .. } => {
            LiveStreamPhase::InterruptedHttp(*status_code)
        }
        MihomoError::Json { .. } => LiveStreamPhase::InvalidData,
        MihomoError::Io(_) => LiveStreamPhase::ControllerUnavailable,
        _ => LiveStreamPhase::Retrying,
    }
}

fn set_live_status(mailbox: &Mutex<LiveMailbox>, activity: bool, status: LiveStreamPhase) {
    if let Ok(mut mailbox) = mailbox.lock() {
        if activity {
            mailbox.status.activity = status;
        } else {
            mailbox.status.logs = status;
        }
    }
}

fn push_kernel_log(mailbox: &Mutex<LiveMailbox>, sequence: u64, entry: &MihomoLogEntry) {
    let Ok(mut mailbox) = mailbox.lock() else {
        return;
    };
    if mailbox.logs.len() == LIVE_LOG_MAILBOX_CAPACITY {
        mailbox.logs.pop_front();
    }
    mailbox.logs.push_back(KernelLogEntry {
        sequence,
        level: sanitize_log_field(&entry.level, 16),
        payload: sanitize_kernel_log(&entry.payload),
        timestamp_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    });
    mailbox.status.logs = LiveStreamPhase::Live;
}

fn sanitize_log_field(value: &str, limit: usize) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(limit)
        .collect()
}

pub(super) fn sanitize_kernel_log(value: &str) -> String {
    let bounded: String = value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(2_048)
        .collect();
    let mut output = String::with_capacity(bounded.len());
    let mut remainder = bounded.as_str();
    while !remainder.is_empty() {
        let lowercase = remainder.to_ascii_lowercase();
        let next_secret = ["https://", "http://", "vless://"]
            .into_iter()
            .filter_map(|prefix| lowercase.find(prefix).map(|index| (index, prefix.len())))
            .min_by_key(|(index, _prefix)| *index);
        let Some((index, prefix_len)) = next_secret else {
            output.push_str(remainder);
            break;
        };
        output.push_str(&remainder[..index]);
        output.push_str("<redacted-url>");
        let secret = &remainder[index + prefix_len..];
        let end = secret
            .find(|character: char| character.is_whitespace() || matches!(character, '"' | '\''))
            .unwrap_or(secret.len());
        remainder = &secret[end..];
    }
    output
}
