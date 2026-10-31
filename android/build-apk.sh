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
gradle_args=(--no-daemon "-PmanisAbi=$ABI")
if [[ -n "${MANIS_PACKAGE_VERSION:-}" ]]; then
  gradle_args+=("-PmanisVersionName=${MANIS_PACKAGE_VERSION}-android")
fi
if [[ -n "${MANIS_ANDROID_VERSION_CODE:-}" ]]; then
  gradle_args+=("-PmanisVersionCode=${MANIS_ANDROID_VERSION_CODE}")
fi

if [[ -n "${MANIS_GRADLE:-}" ]]; then
  "$MANIS_GRADLE" "${gradle_args[@]}" assembleDebug
else
  ./gradlew "${gradle_args[@]}" assembleDebug
fi
