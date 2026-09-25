#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ANDROID_DIR="$ROOT/packaging/android"
JNI_LIBS="$ANDROID_DIR/app/src/main/jniLibs"

if ! command -v cargo-ndk >/dev/null 2>&1; then
    echo "cargo-ndk is required (cargo install cargo-ndk)" >&2
    exit 1
fi

rustup target add aarch64-linux-android
cargo ndk -t arm64-v8a -P 26 -o "$JNI_LIBS" build --manifest-path "$ROOT/Cargo.toml" -p manis-android --lib "$@"
"$ANDROID_DIR/gradlew" -p "$ANDROID_DIR" clean assembleDebug

echo "APK: $ANDROID_DIR/app/build/outputs/apk/debug/app-debug.apk"
