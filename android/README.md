# Manis for Android

A native Android client. The UI is Kotlin + Jetpack Compose; the profile model, Mihomo configuration
renderer and core launcher stay in Rust, so Android and desktop compile the same rules and policy
groups.

## How it fits together

```text
MainActivity (Compose)
  └─ ManisVpnService ── VpnService.Builder → TUN descriptor
        └─ CoreController (Kotlin)
              ├─ Native.renderConfig(url, port, 3)   → YAML from manis-profile
              └─ Native.startCore(..., descriptor)   → Rust dup2(fd, 3), exec libmihomo.so
                                                    → controller 127.0.0.1:9090 (random secret)
```

- Android owns the interface: `172.19.0.1/30`, `fd00:1::1/126`, default routes, DNS `172.19.0.2`.
- Mihomo inherits only the descriptor (`tun.file-descriptor: 3`, `auto-route: false`) and answers
  DNS through `dns-hijack`, so fake-IP and domain rules behave as on desktop.
- The core is bundled in the APK as `libmihomo.so` and executed from `nativeLibraryDir`. It is not
  downloaded at runtime: Android 10+ forbids `exec()` of app-private files for `targetSdk >= 29`.
- The app excludes its own UID from the tunnel so the core's upstream sockets cannot loop back.

## Build

Requirements: JDK 17+, Android SDK Platform 35, Android NDK, Rust `aarch64-linux-android`, and
`cargo-ndk`.

```sh
rustup target add aarch64-linux-android
cargo install cargo-ndk
export ANDROID_HOME="$HOME/Android/Sdk"
android/build-apk.sh
```

The script builds the JNI library into `app/src/main/jniLibs/arm64-v8a/`, downloads the official
Mihomo Android arm64 core (digest verified by `packaging/fetch-mihomo.sh`), then runs
`assembleDebug`. Output: `android/app/build/outputs/apk/debug/app-debug.apk`.

`jniLibs/` is generated and ignored by git; rebuild it for every clean checkout.

## Current scope

- arm64 only; `minSdk 26`, `targetSdk 35`.
- One subscription URL, no node/group editor yet: Mihomo's `select` group starts on the first
  available node, and switching nodes happens later through the controller API.
- Per-app routing, subscription previews and the desktop's diagnostics view are not implemented.
- The controller listens on loopback with a secret generated per launch.

## Licensing

Mihomo is GPL-3.0. The APK distributes it, so a release must include the license, copyright notices
and the corresponding source location. `THIRD_PARTY_NOTICES.md` documents the obligation; the panic
free build itself remains Apache-2.0.
