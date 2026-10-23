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
android/build-apk.sh            # arm64 phone
android/build-apk.sh x86_64     # emulator
```

The script builds the JNI library into `app/src/main/jniLibs/<abi>/`, compiles the Mihomo core with
`android/build-core.sh` (Go 1.24+, pinned tag + patch, NDK clang for `amd64`), then runs
`assembleDebug`. Output: `android/app/build/outputs/apk/debug/app-debug.apk`.

`android/debug.keystore` is a committed debug-only signing key so local and CI builds share one
signature and install over each other. It must never sign a release.

`jniLibs/` and `android/.build/` are generated and ignored by git; rebuild them for every clean
checkout.

## Why the core is compiled locally

The official Android release asset cannot start a TUN inside a normal application: it builds
per-package rules unconditionally and aborts when `/data/system/packages.xml` is unreadable. The
patch in `android/patches/` makes that list best effort, which is correct here because the app does
not use package filters. The patch, upstream tag and build command are the GPL-3.0 source offer for
the bundled binary.

## Current scope

- `minSdk 26`, `targetSdk 35`; arm64 for phones, x86_64 for the emulator.
- One subscription URL plus a node picker backed by the controller: the VPN card shows the selected
  node, delays come from the health-check history, and tapping a node updates
  `__MANIS_GLOBAL__`. Policy-group editing is not implemented.
- Per-app routing, subscription previews and the desktop's diagnostics view are not implemented.
- The controller listens on loopback with a secret generated per launch.

## CI artifact

`.github/workflows/android.yml` builds the arm64 debug APK on every Android-related change and
uploads it as the `manis-android-arm64-debug` artifact for 30 days. A `workflow_dispatch` run builds
it on demand. Download the artifact, unzip it, and install `app-debug.apk`; it is debug-signed, so
uninstall a previous build signed with a different key first.

## Verified on the emulator

Android 35 `sdk_gphone64_x86_64`: consent dialog, foreground `systemExempted` service, `tun0` with
`172.19.0.1`, `tun0(fd=3)` gVisor listener, fake-IP answers for `example.com`, ICMP into the stack,
and a clean teardown (service, `tun0` and core process all gone). Proxying itself still needs a real
subscription; the test URL returns 404 on purpose.

## Licensing

Mihomo is GPL-3.0. The APK distributes it, so a release must include the license, copyright notices
and the corresponding source location. `THIRD_PARTY_NOTICES.md` documents the obligation; the panic
free build itself remains Apache-2.0.
