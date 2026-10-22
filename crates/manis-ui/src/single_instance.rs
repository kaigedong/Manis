//! Prevents duplicate Manis processes and forwards a second launch to the first window.
//!
//! The first process holds an advisory lock on a file in the private data
//! directory. A second process reports that lock, leaves a small activation
//! request next to it, and exits. The first process polls for those requests
//! and reuses the existing main window.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui::{App, Global};

use crate::diagnostics::{LogLevel, record_event};

const LOCK_FILE: &str = "instance.lock";
const REQUEST_PREFIX: &str = "activate-";
const REQUEST_SUFFIX: &str = ".request";
const TEMPORARY_SUFFIX: &str = ".tmp";
const POLL_INTERVAL: Duration = Duration::from_millis(250);
const SECONDARY_RECHECK_DELAY: Duration = Duration::from_millis(100);
const SECONDARY_RECHECK_ATTEMPTS: u8 = 5;

/// The result of claiming the application-wide instance lock.
pub struct SingleInstance {
    primary: bool,
    directory: Option<PathBuf>,
    lock: Option<File>,
}

struct InstanceLock {
    _file: File,
}

impl Global for InstanceLock {}

impl SingleInstance {
    /// Whether this process should continue starting the application.
    #[must_use]
    pub const fn is_primary(&self) -> bool {
        self.primary
    }

    /// Moves the lock into the GPUI application and starts listening for
    /// activation requests from later launches.
    pub fn install(self, cx: &mut App) {
        if !self.primary {
            return;
        }
        let (Some(directory), Some(lock)) = (self.directory, self.lock) else {
            return;
        };
        cx.set_global(InstanceLock { _file: lock });
        let timer = cx.background_executor().clone();
        cx.spawn(async move |cx| {
            loop {
                timer.timer(POLL_INTERVAL).await;
                if take_activation_request(&directory) {
                    cx.update(crate::tray::show_or_open_window);
                }
            }
        })
        .detach();
    }
}

/// Claims the single-instance lock.
///
/// If another process already owns it, this writes an activation request and
/// returns a non-primary handle. Failures that occur before a lock can be
/// created are treated as unavailable rather than blocking application start.
#[must_use]
pub fn claim() -> SingleInstance {
    let Some(directory) = instance_directory() else {
        return unavailable();
    };
    claim_in(&directory)
}

fn claim_in(directory: &Path) -> SingleInstance {
    if let Err(error) = fs::create_dir_all(directory) {
        record_event(
            LogLevel::Warn,
            "instance.directory_unavailable",
            error.to_string(),
        );
        return unavailable();
    }
    let lock_path = directory.join(LOCK_FILE);
    let file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
    {
        Ok(file) => file,
        Err(error) => {
            record_event(
                LogLevel::Warn,
                "instance.lock_unavailable",
                error.to_string(),
            );
            return unavailable();
        }
    };
    match file.try_lock() {
        Ok(()) => become_primary(directory, file),
        Err(TryLockError::WouldBlock) => {
            if let Err(error) = request_activation(directory) {
                record_event(
                    LogLevel::Warn,
                    "instance.activation_request_failed",
                    error.to_string(),
                );
                eprintln!(
                    "manis_ui level=WARN event=instance.activation_request_failed message={error}"
                );
            }
            // The owner may be shutting down. Re-check briefly so relaunching
            // during quit does not silently lose the new launch.
            for _ in 0..SECONDARY_RECHECK_ATTEMPTS {
                std::thread::sleep(SECONDARY_RECHECK_DELAY);
                match file.try_lock() {
                    Ok(()) => return become_primary(directory, file),
                    Err(TryLockError::WouldBlock) => {}
                    Err(TryLockError::Error(error)) => {
                        record_event(LogLevel::Warn, "instance.lock_failed", error.to_string());
                        return unavailable();
                    }
                }
            }
            SingleInstance {
                primary: false,
                directory: None,
                lock: None,
            }
        }
        Err(TryLockError::Error(error)) => {
            record_event(LogLevel::Warn, "instance.lock_failed", error.to_string());
            unavailable()
        }
    }
}

fn become_primary(directory: &Path, file: File) -> SingleInstance {
    clear_stale_requests(directory);
    SingleInstance {
        primary: true,
        directory: Some(directory.to_owned()),
        lock: Some(file),
    }
}

fn unavailable() -> SingleInstance {
    SingleInstance {
        primary: true,
        directory: None,
        lock: None,
    }
}

fn instance_directory() -> Option<PathBuf> {
    crate::brand::data_dir().or_else(crate::brand::config_dir)
}

fn request_activation(directory: &Path) -> io::Result<()> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let name = format!(
        "{REQUEST_PREFIX}{}-{timestamp}{REQUEST_SUFFIX}",
        std::process::id()
    );
    let request = directory.join(&name);
    let temporary = directory.join(format!("{name}{TEMPORARY_SUFFIX}"));
    fs::write(&temporary, b"activate\n")?;
    fs::rename(&temporary, &request).inspect_err(|_| {
        let _ = fs::remove_file(&temporary);
    })
}

fn take_activation_request(directory: &Path) -> bool {
    let Ok(entries) = fs::read_dir(directory) else {
        return false;
    };
    let mut requested = false;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.starts_with(REQUEST_PREFIX) || !name.ends_with(REQUEST_SUFFIX) {
            continue;
        }
        if fs::remove_file(entry.path()).is_ok() {
            requested = true;
        }
    }
    requested
}

fn clear_stale_requests(directory: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with(REQUEST_PREFIX)
            && (name.ends_with(REQUEST_SUFFIX) || name.ends_with(TEMPORARY_SUFFIX))
        {
            let _ = fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use super::{
        REQUEST_PREFIX, REQUEST_SUFFIX, TEMPORARY_SUFFIX, claim_in, clear_stale_requests,
        take_activation_request,
    };

    fn fixture(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "manis-single-instance-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        ));
        fs::create_dir_all(&path).expect("fixture directory");
        path
    }

    #[test]
    fn request_files_trigger_once_and_are_cleared() {
        let directory = fixture("requests");
        fs::write(
            directory.join(format!("{REQUEST_PREFIX}test{REQUEST_SUFFIX}")),
            b"activate\n",
        )
        .expect("request");
        let temporary = directory.join(format!(
            "{REQUEST_PREFIX}test{REQUEST_SUFFIX}{TEMPORARY_SUFFIX}"
        ));
        fs::write(&temporary, b"activate\n").expect("temporary request");
        clear_stale_requests(&directory);
        assert!(!temporary.exists());
        assert!(!take_activation_request(&directory));
        fs::write(
            directory.join(format!("{REQUEST_PREFIX}test{REQUEST_SUFFIX}")),
            b"activate\n",
        )
        .expect("request");
        assert!(take_activation_request(&directory));
        assert!(!take_activation_request(&directory));
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn a_second_claim_in_the_same_process_is_not_primary() {
        let directory = fixture("lock");
        let primary = claim_in(&directory);
        assert!(primary.is_primary());
        let secondary = claim_in(&directory);
        assert!(!secondary.is_primary());
        assert!(take_activation_request(&directory));
        drop(primary);
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn a_claim_during_shutdown_takes_over_the_lock() {
        let directory = fixture("handoff");
        let primary = claim_in(&directory);
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            drop(primary);
        });
        let replacement = claim_in(&directory);
        assert!(replacement.is_primary());
        release.join().expect("release thread");
        drop(replacement);
        fs::remove_dir_all(directory).expect("cleanup");
    }
}
