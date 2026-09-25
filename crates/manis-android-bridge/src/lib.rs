#![allow(unsafe_code)]

#[cfg(target_os = "android")]
mod android {
    use std::{
        os::fd::{FromRawFd, OwnedFd},
        sync::atomic::{AtomicBool, Ordering},
        sync::{Arc, Condvar, Mutex, OnceLock},
        time::Duration,
    };

    use gpui_mobile::android::jni;

    static VPN_RESULT: OnceLock<(Mutex<Option<Result<OwnedFd, String>>>, Condvar)> =
        OnceLock::new();
    static VPN_ACTIVE: AtomicBool = AtomicBool::new(false);
    static VPN_REVOKED: OnceLock<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>> = OnceLock::new();

    fn result_slot() -> &'static (Mutex<Option<Result<OwnedFd, String>>>, Condvar) {
        VPN_RESULT.get_or_init(|| (Mutex::new(None), Condvar::new()))
    }

    fn revoke_slot() -> &'static Mutex<Option<Arc<dyn Fn() + Send + Sync>>> {
        VPN_REVOKED.get_or_init(|| Mutex::new(None))
    }

    /// Install a handler for Android revoking the active VPN permission/interface.
    pub fn set_vpn_revocation_handler(handler: impl Fn() + Send + Sync + 'static) {
        if let Ok(mut slot) = revoke_slot().lock() {
            *slot = Some(Arc::new(handler));
            VPN_ACTIVE.store(true, Ordering::SeqCst);
        }
    }

    /// Ask Android for VPN permission and return the established TUN descriptor.
    pub fn request_vpn_fd() -> Result<OwnedFd, String> {
        let (result, _) = result_slot();
        *result
            .lock()
            .map_err(|_| "Android VPN state lock was poisoned")? = None;
        jni::with_env(|env| {
            let activity = jni::activity(env)?;
            env.call_method(
                &activity,
                ::jni::jni_str!("requestManisVpnPermission"),
                ::jni::jni_sig!(()),
                &[],
            )
            .map_err(|error| error.to_string())?;
            Ok(())
        })?;
        let (result, ready) = result_slot();
        let state = result
            .lock()
            .map_err(|_| "Android VPN state lock was poisoned")?;
        let (mut state, timeout) = ready
            .wait_timeout_while(state, Duration::from_secs(120), |state| state.is_none())
            .map_err(|_| "Android VPN state lock was poisoned")?;
        if timeout.timed_out() && state.is_none() {
            return Err("Android VPN permission request timed out".to_owned());
        }
        state
            .take()
            .unwrap_or_else(|| Err("Android VPN did not start".to_owned()))
    }

    /// Stop the foreground VPN service after the Mihomo child has released its TUN fd.
    pub fn stop_vpn_service() -> Result<(), String> {
        VPN_ACTIVE.store(false, Ordering::SeqCst);
        if let Ok(mut slot) = revoke_slot().lock() {
            *slot = None;
        }
        jni::with_env(|env| {
            let activity = jni::activity(env)?;
            env.call_method(
                &activity,
                ::jni::jni_str!("stopManisVpnService"),
                ::jni::jni_sig!(()),
                &[],
            )
            .map_err(|error| error.to_string())?;
            Ok(())
        })
    }

    fn publish(result: Result<OwnedFd, String>) {
        let (state, ready) = result_slot();
        if let Ok(mut state) = state.lock() {
            *state = Some(result);
            ready.notify_all();
        }
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_dev_manis_app_ManisActivity_nativeVpnStarted(
        _env: ::jni::EnvUnowned<'_>,
        _activity: ::jni::sys::jclass,
        fd: ::jni::sys::jint,
    ) {
        if fd < 0 {
            publish(Err("Android did not establish a VPN interface".to_owned()));
            return;
        }
        // SAFETY: Java detaches this live ParcelFileDescriptor immediately before this callback
        // and transfers sole ownership of its raw descriptor to this function.
        publish(Ok(unsafe { OwnedFd::from_raw_fd(fd) }));
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_dev_manis_app_ManisActivity_nativeVpnFailed(
        _env: ::jni::EnvUnowned<'_>,
        _activity: ::jni::sys::jclass,
    ) {
        if VPN_ACTIVE.swap(false, Ordering::SeqCst) {
            let handler = revoke_slot().lock().ok().and_then(|mut slot| slot.take());
            if let Some(handler) = handler {
                let _ = std::thread::Builder::new()
                    .name("manis-vpn-revoked".to_owned())
                    .spawn(move || handler());
            }
            return;
        }
        publish(Err(
            "VPN permission was denied or Android could not start the VPN service".to_owned(),
        ));
    }
}

#[cfg(target_os = "android")]
pub use android::{request_vpn_fd, set_vpn_revocation_handler, stop_vpn_service};
