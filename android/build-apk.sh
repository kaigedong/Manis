#!/usr/bin/env bash
# Builds the Manis Android debug APK: JNI bridge, bundled Mihomo core, then Gradle.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
  ANDROID_NDK_HOME="$(find "$ANDROID_HOME/ndk" -mindepth 1 -maxdepth 1 -type d | sort | tail -1)"
  export ANDROID_NDK_HOME
fi

ABI="${1:-arm64-v8a}"
JNI_DIR="$ROOT/android/app/src/main/jniLibs/$ABI"
mkdir -p "$JNI_DIR"

echo "Building manis-android for $ABI" >&2
cargo ndk -t "$ABI" -o "$ROOT/android/app/src/main/jniLibs" build -p manis-android --release

if [[ "${MANIS_SKIP_CORE_BUILD:-0}" != "1" ]]; then
  "$ROOT/android/build-core.sh" "$ABI" "$JNI_DIR/libmihomo.so"
fi

cd "$ROOT/android"
if [[ -n "${MANIS_GRADLE:-}" ]]; then
  "$MANIS_GRADLE" --no-daemon -PmanisAbi="$ABI" assembleDebug
else
  ./gradlew --no-daemon -PmanisAbi="$ABI" assembleDebug
fi
