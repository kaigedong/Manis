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

use jni::{
    Env, EnvUnowned,
    errors::{ErrorPolicy, Result as JniResult},
    jni_str,
    objects::{JObject, JString},
    strings::JNIString,
    sys::{jint, jstring},
};

/// Descriptor number the generated configuration declares as `tun.file-descriptor`.
const TUN_DESCRIPTOR: i32 = 3;
/// Loopback controller shared with the Kotlin host. The secret is generated per launch so another
/// application on the device cannot drive the core through `127.0.0.1`.
const CONTROLLER_ADDRESS: &str = "127.0.0.1:9090";

static CORE: Mutex<Option<Child>> = Mutex::new(None);

/// Failure the bridge reports to the Kotlin host.
///
/// `Native.kt` documents `IllegalStateException` as the only failure signal, so JNI errors are
/// folded into the same contract instead of escaping as a different exception.
enum BridgeError {
    Message(String),
    Jni(jni::errors::Error),
}

impl From<jni::errors::Error> for BridgeError {
    fn from(error: jni::errors::Error) -> Self {
        Self::Jni(error)
    }
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(message) => formatter.write_str(message),
            Self::Jni(error) => write!(formatter, "JNI call failed: {error}"),
        }
    }
}

fn throw(env: &mut Env<'_>, message: &str) {
    if let Ok(class) = env.find_class(jni_str!("java/lang/IllegalStateException")) {
        let _ = env.throw_new(class, JNIString::new(message));
    }
}

/// Throws `IllegalStateException` and returns the JNI default value.
///
/// The default keeps the native frame complete while Java observes the pending exception; a panic
/// never unwinds across the FFI boundary.
struct ThrowIllegalStateAndDefault;

impl<T: Default> ErrorPolicy<T, BridgeError> for ThrowIllegalStateAndDefault {
    type Captures<'unowned_env_local: 'native_method, 'native_method> = ();

    fn on_error<'unowned_env_local: 'native_method, 'native_method>(
        env: &mut Env<'unowned_env_local>,
        _captures: &mut Self::Captures<'unowned_env_local, 'native_method>,
        error: BridgeError,
    ) -> JniResult<T> {
        throw(env, &error.to_string());
        Ok(T::default())
    }

    fn on_panic<'unowned_env_local: 'native_method, 'native_method>(
        env: &mut Env<'unowned_env_local>,
        _captures: &mut Self::Captures<'unowned_env_local, 'native_method>,
        _payload: Box<dyn std::any::Any + Send + 'static>,
    ) -> JniResult<T> {
        throw(env, "the native bridge panicked; see logcat for details");
        Ok(T::default())
    }
}

fn string_arg(env: &Env<'_>, value: JString<'_>) -> Result<String, BridgeError> {
    value
        .mutf8_chars(env)
        .map(String::from)
        .map_err(|_error| BridgeError::Message("argument was not a valid UTF-8 string".to_owned()))
}

/// Renders the mobile workspace document into a runtime configuration.
///
/// Returns a JSON envelope (`{"yaml": ..., "warnings": [...], "providers": n, ...}`) and throws
/// `IllegalStateException` on failure. Warnings never contain subscription credentials.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_manis_app_core_Native_renderConfig<'frame>(
    mut unowned_env: EnvUnowned<'frame>,
    _this: JObject<'frame>,
    config_json: JString<'frame>,
    file_descriptor: jint,
) -> jstring {
    unowned_env
        .with_env(|env| -> Result<jstring, BridgeError> {
            let config = string_arg(env, config_json)?;
            let envelope =
                model::compile(&config, file_descriptor).map_err(BridgeError::Message)?;
            Ok(env.new_string(envelope)?.into_raw())
        })
        .resolve::<ThrowIllegalStateAndDefault>()
}

/// Starts the bundled core with `file_descriptor` duplicated to fd 3.
///
/// Returns the child pid on success and throws `IllegalStateException` on failure.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_manis_app_core_Native_startCore<'frame>(
    mut unowned_env: EnvUnowned<'frame>,
    _this: JObject<'frame>,
    binary: JString<'frame>,
    data_directory: JString<'frame>,
    config_path: JString<'frame>,
    secret: JString<'frame>,
    file_descriptor: jint,
) -> jint {
    unowned_env
        .with_env(|env| -> Result<jint, BridgeError> {
            let binary = string_arg(env, binary)?;
            let data_directory = string_arg(env, data_directory)?;
            let config_path = string_arg(env, config_path)?;
            let secret = string_arg(env, secret)?;
            if file_descriptor < 0 {
                return Err(BridgeError::Message(
                    "the VPN descriptor is invalid".to_owned(),
                ));
            }
            if let Ok(mut core) = CORE.lock()
                && let Some(mut running) = core.take()
            {
                let _ = running.kill();
                let _ = running.wait();
            }
            fs::create_dir_all(&data_directory).map_err(|error| {
                BridgeError::Message(format!("data directory could not be created: {error}"))
            })?;
            let log = File::create(format!("{data_directory}/core.log")).map_err(|error| {
                BridgeError::Message(format!("core log could not be created: {error}"))
            })?;
            let stderr = log.try_clone().map_err(|error| {
                BridgeError::Message(format!("core log could not be duplicated: {error}"))
            })?;

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

            // SAFETY: the closure runs in the forked child before exec and only calls
            // async-signal-safe libc functions on values captured by copy.
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

            // The Java host keeps the VpnService descriptor open so the core can be restarted with
            // the same interface after a configuration change; the child only gets its own
            // duplicate.
            let child = command.spawn().map_err(|error| {
                BridgeError::Message(format!("core could not be started: {error}"))
            })?;
            let pid = child.id();
            CORE.lock()
                .map_err(|_poisoned| BridgeError::Message("core state is unavailable".to_owned()))?
                .replace(child);
            Ok(i32::try_from(pid).unwrap_or(-1))
        })
        .resolve::<ThrowIllegalStateAndDefault>()
}

/// Stops the core started by [`Java_dev_manis_app_core_Native_startCore`], if any.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_manis_app_core_Native_stopCore<'frame>(
    _unowned_env: EnvUnowned<'frame>,
    _this: JObject<'frame>,
) {
    if let Ok(mut core) = CORE.lock()
        && let Some(mut running) = core.take()
    {
        let _ = running.kill();
        let _ = running.wait();
    }
}
