//! Android JNI boundary for Manis.
//!
//! Two responsibilities only:
//!
//! 1. Render the managed Mihomo configuration from a subscription URL, using the same typed
//!    profile and renderer as the desktop application.
//! 2. Launch the bundled Mihomo core with the `VpnService` TUN descriptor inherited as fd 3.
//!
//! Java cannot pass an arbitrary descriptor to a child process, so the `dup2` happens in the child
//! between `fork` and `exec`. This is the only unsafe surface in the crate and it contains no
//! user-controlled file paths beyond the app-private directory passed in by the Kotlin host.

// The bridge only exists for Android, which is unix. Keeping the crate empty elsewhere lets the
// workspace-wide Windows and macOS checks compile without pretending a desktop build has a JNI.
#![cfg(unix)]

use std::fs::{self, File};
use std::io;
use std::os::unix::process::CommandExt as _;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

mod model;

use jni::JNIEnv;
use jni::objects::{JObject, JString};
use jni::sys::{jint, jstring};

/// Descriptor number the generated configuration declares as `tun.file-descriptor`.
const TUN_DESCRIPTOR: i32 = 3;
/// Loopback controller shared with the Kotlin host. The secret is generated per launch so another
/// application on the device cannot drive the core through `127.0.0.1`.
const CONTROLLER_ADDRESS: &str = "127.0.0.1:9090";

static CORE: Mutex<Option<Child>> = Mutex::new(None);

fn throw(env: &mut JNIEnv<'_>, message: &str) {
    let _ = env.throw_new("java/lang/IllegalStateException", message);
}

fn string_arg(env: &mut JNIEnv<'_>, value: &JString<'_>) -> Result<String, String> {
    env.get_string(value)
        .map(Into::into)
        .map_err(|_error| "argument was not a valid UTF-8 string".to_owned())
}

/// Renders the mobile workspace document into a runtime configuration.
///
/// Returns a JSON envelope (`{"yaml": ..., "warnings": [...], "providers": n, ...}`) and throws
/// `IllegalStateException` on failure. Warnings never contain subscription credentials.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_manis_app_core_Native_renderConfig<'local>(
    mut env: JNIEnv<'local>,
    _this: JObject<'local>,
    config_json: JString<'local>,
    file_descriptor: jint,
) -> jstring {
    let rendered = (|| {
        let config = string_arg(&mut env, &config_json)?;
        model::compile(&config, file_descriptor)
    })();

    match rendered {
        Ok(envelope) => env
            .new_string(envelope)
            .map_or(std::ptr::null_mut(), |value| value.into_raw()),
        Err(message) => {
            throw(&mut env, &message);
            std::ptr::null_mut()
        }
    }
}

/// Starts the bundled core with `file_descriptor` duplicated to fd 3.
///
/// Returns the child pid on success and throws `IllegalStateException` on failure.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_manis_app_core_Native_startCore<'local>(
    mut env: JNIEnv<'local>,
    _this: JObject<'local>,
    binary: JString<'local>,
    data_directory: JString<'local>,
    config_path: JString<'local>,
    secret: JString<'local>,
    file_descriptor: jint,
) -> jint {
    let started = (|| {
        let binary = string_arg(&mut env, &binary)?;
        let data_directory = string_arg(&mut env, &data_directory)?;
        let config_path = string_arg(&mut env, &config_path)?;
        let secret = string_arg(&mut env, &secret)?;
        if file_descriptor < 0 {
            return Err("the VPN descriptor is invalid".to_owned());
        }
        if let Ok(mut core) = CORE.lock()
            && let Some(mut running) = core.take()
        {
            let _ = running.kill();
            let _ = running.wait();
        }
        fs::create_dir_all(&data_directory)
            .map_err(|error| format!("data directory could not be created: {error}"))?;
        let log = File::create(format!("{data_directory}/core.log"))
            .map_err(|error| format!("core log could not be created: {error}"))?;
        let stderr = log
            .try_clone()
            .map_err(|error| format!("core log could not be duplicated: {error}"))?;

        let mut command = Command::new(&binary);
        command
            .arg("-d")
            .arg(&data_directory)
            .arg("-f")
            .arg(&config_path)
            .arg("-ext-ctl")
            .arg(CONTROLLER_ADDRESS)
            .arg("-secret")
            .arg(&secret)
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(stderr));

        // SAFETY: the closure runs in the forked child before exec and only calls async-signal-safe
        // libc functions on values captured by copy.
        unsafe {
            command.pre_exec(move || {
                if libc::dup2(file_descriptor, TUN_DESCRIPTOR) < 0 {
                    return Err(io::Error::last_os_error());
                }
                if file_descriptor != TUN_DESCRIPTOR {
                    libc::close(file_descriptor);
                }
                Ok(())
            });
        }

        // The Java host keeps the VpnService descriptor open so the core can be restarted with the
        // same interface after a configuration change; the child only gets its own duplicate.
        let child = command
            .spawn()
            .map_err(|error| format!("core could not be started: {error}"))?;
        let pid = child.id();
        CORE.lock()
            .map_err(|_poisoned| "core state is unavailable".to_owned())?
            .replace(child);
        Ok(pid)
    })();

    match started {
        Ok(pid) => i32::try_from(pid).unwrap_or(-1),
        Err(message) => {
            throw(&mut env, &message);
            -1
        }
    }
}

/// Stops the core started by [`Java_dev_manis_app_core_Native_startCore`], if any.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_manis_app_core_Native_stopCore<'local>(
    _env: JNIEnv<'local>,
    _this: JObject<'local>,
) {
    if let Ok(mut core) = CORE.lock()
        && let Some(mut running) = core.take()
    {
        let _ = running.kill();
        let _ = running.wait();
    }
}
