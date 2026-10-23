#!/usr/bin/env bash
# Downloads the official Mihomo Android arm64 core into the APK's jniLibs directory.
#
# The core is bundled into the signed APK instead of being executed from app-private storage:
# Android 10+ forbids exec() of downloaded files when the application targets SDK 29 or later.
# Mihomo is GPL-3.0; the distributed APK must keep the corresponding source offer.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT="${1:-$ROOT/android/app/src/main/jniLibs/arm64-v8a/libmihomo.so}"

MANIS_MIHOMO_OS=Android \
MANIS_MIHOMO_ARCH=aarch64 \
MANIS_MIHOMO_SKIP_VERSION_CHECK=1 \
  "$ROOT/packaging/fetch-mihomo.sh" "$OUTPUT"
