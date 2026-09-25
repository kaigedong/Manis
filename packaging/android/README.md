# Android

The Android target uses the community GPUI Mobile platform implementation from
[itsbalamurali/gpui-mobile](https://github.com/itsbalamurali/gpui-mobile), which
provides a GPUI `Platform` backend for Android and an APK example. At the time
of adoption the project had 235 GitHub stars, 18 forks, and 123 commits. The
upstream Zed issue for Android GPUI support is closed as not planned, so this
community backend is the most direct reusable route.

Manis maintains the backend in
[bobo-dong/manis-gpui-mobile](https://github.com/bobo-dong/manis-gpui-mobile).
The first fork change aligns its GPUI and renderer dependencies with the GPUI
revision already locked by Manis. The fork retains the upstream's
`GPL-3.0-or-later OR AGPL-3.0-or-later OR Apache-2.0` license choice.

## Build a debug APK

Install Android SDK Platform 35, Android NDK, JDK 17, Gradle 8.9, Rust's
`aarch64-linux-android` target, and `cargo-ndk`. Then run:

```sh
cargo install cargo-ndk
packaging/android/build-apk.sh
```

The script writes the ARM64 native library into the ignored `jniLibs` build
directory and packages it as
`packaging/android/app/build/outputs/apk/debug/app-debug.apk`.

The host is a custom `NativeActivity` that opens the GPUI app full screen. The
runtime downloads and verifies Mihomo's official Android ARM64 stable core in
the app-private directory. TUN mode requests Android VPN permission, starts a
foreground `VpnService`, gives its TUN descriptor to Mihomo as fd 3, and
restarts Mihomo when TUN mode is switched on or off. The Manis app UID is
excluded from its own VPN to prevent the Mihomo process's upstream sockets from
looping back into the tunnel.

The personal sideload build targets SDK 28 because Android 10+ blocks
`execve()` of downloaded binaries in app-private storage when the app targets
SDK 29 or later. This keeps the digest-verified in-app core updater available,
but is not a Google Play release configuration. A store build must target a
current SDK and package Mihomo in the signed APK so it updates with Manis.

The Rust, Java, and APK builds succeed, but no Android device or emulator is
currently connected. Permission handling, touchscreen/keyboard behavior, and
real traffic routing still need on-device validation; treat the debug APK as
a development preview until then.
