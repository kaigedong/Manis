#!/usr/bin/env bash
# Builds the bundled Mihomo core from a pinned upstream tag with one local patch.
#
# The official Android release asset cannot start a TUN inside a regular application: it builds
# Android package rules unconditionally and fails when `/data/system/packages.xml` is unreadable.
# The patch makes that package list best effort. Mihomo is GPL-3.0; distributing this binary means
# publishing the patch and the exact upstream source revision (both are recorded here and in
# THIRD_PARTY_NOTICES.md).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ABI="${1:-arm64-v8a}"
OUT="${2:-$ROOT/android/app/src/main/jniLibs/$ABI/libmihomo.so}"
TAG="${MANIS_MIHOMO_TAG:-v1.19.33}"
SOURCE="${MANIS_MIHOMO_SOURCE:-$ROOT/android/.build/mihomo-$TAG}"
PATCH="$ROOT/android/patches/mihomo-skip-android-package-manager.patch"

case "$ABI" in
  arm64-v8a) export GOARCH=arm64; TRIPLE=aarch64-linux-android ;;
  armeabi-v7a) export GOARCH=arm GOARM=7; TRIPLE=armv7a-linux-androideabi ;;
  x86_64) export GOARCH=amd64; TRIPLE=x86_64-linux-android ;;
  x86) export GOARCH=386; TRIPLE=i686-linux-android ;;
  *) echo "unsupported Android ABI: $ABI" >&2; exit 2 ;;
esac
export GOOS=android

# android/amd64 requires external linking, so build through the NDK toolchain when it is present.
NDK="${ANDROID_NDK_HOME:-${ANDROID_HOME:-$HOME/Android/Sdk}/ndk/27.2.12479018}"
TOOLCHAIN="$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin"
if [[ -x "$TOOLCHAIN/${TRIPLE}26-clang" ]]; then
  export CGO_ENABLED=1
  export CC="$TOOLCHAIN/${TRIPLE}26-clang"
  export CXX="$TOOLCHAIN/${TRIPLE}26-clang++"
  export AR="$TOOLCHAIN/llvm-ar"
  export RANLIB="$TOOLCHAIN/llvm-ranlib"
else
  export CGO_ENABLED=0
fi

if [[ ! -d "$SOURCE/.git" ]]; then
  echo "Cloning Mihomo $TAG" >&2
  git clone --depth 1 --branch "$TAG" https://github.com/MetaCubeX/mihomo.git "$SOURCE"
fi

if git -C "$SOURCE" apply --check "$PATCH" 2>/dev/null; then
  git -C "$SOURCE" apply "$PATCH"
fi

mkdir -p "$(dirname "$OUT")"
BUILD_TIME="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "Building Mihomo $TAG for $ABI" >&2
(
  cd "$SOURCE"
  go build -tags with_gvisor -trimpath \
    -ldflags "-s -w -X 'github.com/metacubex/mihomo/constant.Version=${TAG#v}-manis' -X 'github.com/metacubex/mihomo/constant.BuildTime=$BUILD_TIME'" \
    -o "$OUT" .
)
chmod 0755 "$OUT"
go version -m "$OUT" | head -3
