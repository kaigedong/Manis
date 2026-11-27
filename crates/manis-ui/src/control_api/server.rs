#[cfg(unix)]
mod unix_impl {
    use std::collections::VecDeque;
    use std::fs;
    use std::future::poll_fn;
    use std::io::{BufRead as _, BufReader, Read as _, Write as _};
    use std::os::unix::fs::{FileTypeExt as _, PermissionsExt as _};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex, mpsc};
    use std::task::{Poll, Waker};
    use std::thread;
    use std::time::Duration;

    use gpui::{App, Entity, Global};

    use crate::ManisApp;
    use crate::diagnostics::{LogLevel, record_event};

    use super::super::protocol::{ControlRequest, ControlResponse, ResponseSource};

    pub(super) const CONTROL_SOCKET_NAME: &str = "control.sock";
    const MAX_REQUEST_BYTES: u64 = 1024 * 1024;
    const SOCKET_IO_TIMEOUT: Duration = Duration::from_secs(5);

    type PendingControlCall = (ControlRequest, mpsc::SyncSender<ControlResponse>);

    #[derive(Default)]
    struct QueueState {
        items: VecDeque<PendingControlCall>,
        waker: Option<Waker>,
        closed: bool,
    }

    #[derive(Clone, Default)]
    struct AsyncCallQueue {
        inner: Arc<Mutex<QueueState>>,
    }

    impl AsyncCallQueue {
        fn push(&self, item: PendingControlCall) -> bool {
            let waker = {
                let Ok(mut guard) = self.inner.lock() else {
                    return false;
                };
                if guard.closed {
                    return false;
                }
                guard.items.push_back(item);
                guard.waker.take()
            };
            if let Some(waker) = waker {
                waker.wake();
            }
            true
        }

        fn close(&self) {
            let waker = {
                let Ok(mut guard) = self.inner.lock() else {
                    return;
                };
                guard.closed = true;
                guard.waker.take()
            };
            if let Some(waker) = waker {
                waker.wake();
            }
        }

        async fn next(&self) -> Option<PendingControlCall> {
            poll_fn(|cx| {
                let Ok(mut guard) = self.inner.lock() else {
                    return Poll::Ready(None);
                };
                if let Some(item) = guard.items.pop_front() {
                    return Poll::Ready(Some(item));
                }
                if guard.closed {
                    return Poll::Ready(None);
                }
                guard.waker = Some(cx.waker().clone());
                Poll::Pending
            })
            .await
        }
    }

    struct ControlServerGuard {
        socket_path: PathBuf,
        queue: AsyncCallQueue,
    }

    impl Drop for ControlServerGuard {
        fn drop(&mut self) {
            self.queue.close();
            let _ = fs::remove_file(&self.socket_path);
        }
    }

    impl Global for ControlServerGuard {}

    pub(crate) fn socket_path_in(directory: &Path) -> PathBuf {
        directory.join(CONTROL_SOCKET_NAME)
    }

    pub(crate) fn install_in(directory: &Path, app: Entity<ManisApp>, cx: &mut App) {
        let Some(socket_path) = prepare_socket_path(directory) else {
            return;
        };
        let listener = match UnixListener::bind(&socket_path) {
            Ok(listener) => listener,
            Err(error) => {
                record_event(LogLevel::Warn, "control_api.bind_failed", error.to_string());
                return;
            }
        };
        if let Err(error) = fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600)) {
            record_event(
                LogLevel::Warn,
                "control_api.permissions_failed",
                error.to_string(),
            );
            let _ = fs::remove_file(&socket_path);
            return;
        }

        let queue = AsyncCallQueue::default();
        cx.set_global(ControlServerGuard {
            socket_path: socket_path.clone(),
            queue: queue.clone(),
        });

        let accept_queue = queue.clone();
        if let Err(error) = thread::Builder::new()
            .name("manis-control-api".to_owned())
            .spawn(move || accept_loop(&listener, &accept_queue))
        {
            record_event(
                LogLevel::Warn,
                "control_api.thread_failed",
                error.to_string(),
            );
            return;
        }

        cx.spawn(async move |cx| {
            while let Some((request, reply_tx)) = queue.next().await {
                let response = cx
                    .update(|cx| app.update(cx, |app, cx| app.handle_control_request(request, cx)));
                let _ = reply_tx.send(response);
            }
        })
        .detach();
    }

    fn prepare_socket_path(directory: &Path) -> Option<PathBuf> {
        if !directory.is_absolute() {
            return None;
        }
        if fs::create_dir_all(directory).is_err() {
            return None;
        }
        let dir_meta = fs::symlink_metadata(directory).ok()?;
        if dir_meta.file_type().is_symlink() || !dir_meta.is_dir() {
            return None;
        }
        let _ = fs::set_permissions(directory, fs::Permissions::from_mode(0o700));
        let path = socket_path_in(directory);
        if let Ok(meta) = fs::symlink_metadata(&path) {
            if meta.file_type().is_symlink() || !meta.file_type().is_socket() {
                return None;
            }
            // Remove stale socket left by an earlier crash.
            fs::remove_file(&path).ok()?;
        }
        Some(path)
    }

    fn accept_loop(listener: &UnixListener, queue: &AsyncCallQueue) {
        for stream in listener.incoming() {
            let Ok(stream) = stream else {
                continue;
            };
            let queue = queue.clone();
            let _ = thread::Builder::new()
                .name("manis-control-conn".to_owned())
                .spawn(move || handle_connection(stream, &queue));
        }
    }

    fn handle_connection(mut stream: UnixStream, queue: &AsyncCallQueue) {
        let _ = stream.set_read_timeout(Some(SOCKET_IO_TIMEOUT));
        let _ = stream.set_write_timeout(Some(SOCKET_IO_TIMEOUT));

        let mut line = String::new();
        let read_result = {
            let mut reader = BufReader::new((&stream).take(MAX_REQUEST_BYTES + 1));
            reader.read_line(&mut line)
        };
        let response = match read_result {
            Ok(0) => return,
            Ok(_) if line.len() as u64 > MAX_REQUEST_BYTES => ControlResponse::err(
                ResponseSource::LiveDaemon,
                "request_too_large",
                "control request exceeds 1 MiB limit",
            ),
            Ok(_) => match serde_json::from_str::<ControlRequest>(line.trim()) {
                Ok(request) => {
                    let (reply_tx, reply_rx) = mpsc::sync_channel(1);
                    if queue.push((request, reply_tx)) {
                        reply_rx
                            .recv_timeout(SOCKET_IO_TIMEOUT)
                            .unwrap_or_else(|_| {
                                ControlResponse::err(
                                    ResponseSource::LiveDaemon,
                                    "timeout",
                                    "timed out waiting for Manis app response",
                                )
                            })
                    } else {
                        ControlResponse::err(
                            ResponseSource::LiveDaemon,
                            "daemon_shutting_down",
                            "Manis daemon is shutting down",
                        )
                    }
                }
                Err(error) => ControlResponse::err(
                    ResponseSource::LiveDaemon,
                    "invalid_request",
                    error.to_string(),
                ),
            },
            Err(error) => {
                ControlResponse::err(ResponseSource::LiveDaemon, "io_error", error.to_string())
            }
        };

        if let Ok(mut encoded) = serde_json::to_string(&response) {
            encoded.push('\n');
            let _ = stream.write_all(encoded.as_bytes());
            let _ = stream.flush();
        }
    }

    pub(crate) fn send_request(
        directory: &Path,
        request: &ControlRequest,
    ) -> Result<ControlResponse, String> {
        let path = socket_path_in(directory);
        let meta = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if meta.file_type().is_symlink() || !meta.file_type().is_socket() {
            return Err("control socket path is not a local socket".to_owned());
        }
        let mut stream = UnixStream::connect(&path).map_err(|error| error.to_string())?;
        let _ = stream.set_read_timeout(Some(SOCKET_IO_TIMEOUT));
        let _ = stream.set_write_timeout(Some(SOCKET_IO_TIMEOUT));
        let mut payload = serde_json::to_string(request).map_err(|error| error.to_string())?;
        payload.push('\n');
        stream
            .write_all(payload.as_bytes())
            .map_err(|error| error.to_string())?;
        stream.flush().map_err(|error| error.to_string())?;

        let mut response_line = String::new();
        let mut reader = BufReader::new(stream.take(4 * 1024 * 1024));
        reader
            .read_line(&mut response_line)
            .map_err(|error| error.to_string())?;
        serde_json::from_str::<ControlResponse>(response_line.trim())
            .map_err(|error| error.to_string())
    }

    #[cfg(test)]
    pub(crate) fn spawn_test_socket_server(
        directory: &Path,
        handler: impl Fn(ControlRequest) -> ControlResponse + Send + Sync + 'static,
    ) -> Result<PathBuf, String> {
        let socket_path =
            prepare_socket_path(directory).ok_or_else(|| "invalid socket dir".to_owned())?;
        let listener = UnixListener::bind(&socket_path).map_err(|error| error.to_string())?;
        fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
        let queue = AsyncCallQueue::default();
        let accept_queue = queue.clone();
        thread::spawn(move || accept_loop(&listener, &accept_queue));
        let dispatch_queue = queue;
        thread::spawn(move || {
            loop {
                let next = {
                    let Ok(mut guard) = dispatch_queue.inner.lock() else {
                        break;
                    };
                    if let Some(item) = guard.items.pop_front() {
                        Some(item)
                    } else if guard.closed {
                        break;
                    } else {
                        None
                    }
                };
                if let Some((request, reply_tx)) = next {
                    let _ = reply_tx.send(handler(request));
                } else {
                    thread::sleep(Duration::from_millis(2));
                }
            }
        });
        Ok(socket_path)
    }
}

#[cfg(all(unix, test))]
pub(crate) use unix_impl::spawn_test_socket_server;
#[cfg(unix)]
pub(crate) use unix_impl::{install_in, send_request};

#[cfg(not(unix))]
pub(crate) fn install_in(
    _directory: &std::path::Path,
    _app: gpui::Entity<crate::ManisApp>,
    _cx: &mut gpui::App,
) {
}

#[cfg(not(unix))]
pub(crate) fn send_request(
    _directory: &std::path::Path,
    _request: &super::protocol::ControlRequest,
) -> Result<super::protocol::ControlResponse, String> {
    Err("live control socket is only supported on Unix platforms".to_owned())
}
