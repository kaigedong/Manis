#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DIST_DIR="${MANIS_MACOS_DIST_DIR:-$ROOT_DIR/dist/macos}"
APP_DIR="$DIST_DIR/Manis.app"
mkdir -p "$DIST_DIR"
BUILD_ROOT="$(mktemp -d "$DIST_DIR/Manis.app.build.XXXXXX")"
BUILD_APP_DIR="$BUILD_ROOT/Manis.app"
cleanup_build_root() {
  if [[ -n "${BUILD_ROOT:-}" && "$BUILD_ROOT" == "$DIST_DIR"/Manis.app.build.* && -d "$BUILD_ROOT" ]]; then
    /bin/rm -R "$BUILD_ROOT"
  fi
}
trap cleanup_build_root EXIT
CONTENTS_DIR="$BUILD_APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
LAUNCH_DAEMONS_DIR="$CONTENTS_DIR/Library/LaunchDaemons"
HELPER_TOOLS_DIR="$CONTENTS_DIR/Library/PrivilegedHelperTools"
RESOURCES_DIR="$CONTENTS_DIR/Resources/mihomo"
HELPER_ID="dev.manis.app.helper"
HELPERCTL_ID="dev.manis.app.helperctl"
LOCAL_INSTALLER_ID="dev.manis.app.helper.local-installer"
CLIENT_REQUIREMENT="${MANIS_CLIENT_REQUIREMENT:-identifier \"$HELPERCTL_ID\"}"
PARENT_REQUIREMENT="${MANIS_PARENT_REQUIREMENT:-identifier \"dev.manis.app\"}"
MIHOMO_SOURCE="${MANIS_MIHOMO_BINARY:-}"
BUNDLE_VERSION="${MANIS_BUNDLE_VERSION:-0.1.0}"
BUNDLE_BUILD="${MANIS_BUNDLE_BUILD:-1}"

if [[ ! "$BUNDLE_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "MANIS_BUNDLE_VERSION must contain exactly three numeric components" >&2
  exit 1
fi
if [[ ! "$BUNDLE_BUILD" =~ ^[1-9][0-9]*$ ]]; then
  echo "MANIS_BUNDLE_BUILD must be a positive integer" >&2
  exit 1
fi

if [[ "${MANIS_ALLOW_INSECURE_LOCAL_HELPER:-0}" == "1" && -n "${MANIS_CODESIGN_IDENTITY:-}" ]]; then
  echo "MANIS_ALLOW_INSECURE_LOCAL_HELPER cannot be combined with a signed production build" >&2
  exit 1
fi
if [[ "${MANIS_ALLOW_INSECURE_LOCAL_HELPER:-0}" == "1" && -z "$MIHOMO_SOURCE" ]]; then
  echo "local TUN helper builds require MANIS_MIHOMO_BINARY" >&2
  exit 1
fi

if [[ -n "${MANIS_CODESIGN_IDENTITY:-}" ]]; then
  if [[ -z "${MANIS_CLIENT_REQUIREMENT:-}" || -z "${MANIS_PARENT_REQUIREMENT:-}" ]]; then
    echo "signed helper builds require MANIS_CLIENT_REQUIREMENT and MANIS_PARENT_REQUIREMENT" >&2
    exit 1
  fi
  case "$CLIENT_REQUIREMENT" in
    *"identifier \"$HELPERCTL_ID\""*"anchor apple generic"*"certificate leaf[subject.OU]"*) ;;
    *) echo "MANIS_CLIENT_REQUIREMENT must pin helperctl identifier, Apple anchor, and Team ID" >&2; exit 1 ;;
  esac
  case "$PARENT_REQUIREMENT" in
    *"identifier \"dev.manis.app\""*"anchor apple generic"*"certificate leaf[subject.OU]"*) ;;
    *) echo "MANIS_PARENT_REQUIREMENT must pin Manis identifier, Apple anchor, and Team ID" >&2; exit 1 ;;
  esac
fi

cargo build -p manis-ui --release --locked

mkdir -p "$MACOS_DIR" "$LAUNCH_DAEMONS_DIR" "$HELPER_TOOLS_DIR" "$RESOURCES_DIR"

cp "$ROOT_DIR/packaging/macos/Info.plist" "$CONTENTS_DIR/Info.plist"
plutil -replace CFBundleShortVersionString -string "$BUNDLE_VERSION" "$CONTENTS_DIR/Info.plist"
plutil -replace CFBundleVersion -string "$BUNDLE_BUILD" "$CONTENTS_DIR/Info.plist"
plutil -insert ManisParentCodeSigningRequirement -string "$PARENT_REQUIREMENT" "$CONTENTS_DIR/Info.plist"
if [[ "${MANIS_ALLOW_INSECURE_LOCAL_HELPER:-0}" == "1" ]]; then
  plutil -insert ManisAllowInsecureLocalHelper -bool YES "$CONTENTS_DIR/Info.plist"
fi
cp "$ROOT_DIR/target/release/manis-ui" "$MACOS_DIR/Manis"

swiftc \
  -framework Foundation \
  -framework Security \
  "$ROOT_DIR/packaging/macos/ManisPrivilegedHelper.swift" \
  -o "$HELPER_TOOLS_DIR/$HELPER_ID"

swiftc \
  -framework CryptoKit \
  -framework Foundation \
  -framework ServiceManagement \
  -framework Security \
  "$ROOT_DIR/packaging/macos/manis-helperctl.swift" \
  -o "$MACOS_DIR/manis-helperctl"

swiftc \
  -framework CryptoKit \
  -framework Foundation \
  "$ROOT_DIR/packaging/macos/manis-local-helper-install.swift" \
  -o "$MACOS_DIR/manis-local-helper-install"

if [[ -n "$MIHOMO_SOURCE" ]]; then
  if [[ ! -x "$MIHOMO_SOURCE" ]]; then
    echo "MANIS_MIHOMO_BINARY must point to an executable Mihomo binary" >&2
    exit 1
  fi
  cp "$MIHOMO_SOURCE" "$RESOURCES_DIR/mihomo"
  chmod 0755 "$RESOURCES_DIR/mihomo"
else
  echo "warning: MANIS_MIHOMO_BINARY not set; privileged TUN start will fail until Mihomo is bundled" >&2
fi

PLIST_OUT="$LAUNCH_DAEMONS_DIR/dev.manis.app.helper.plist"
sed \
  -e "s#identifier \"dev.manis.app.helperctl\"#$CLIENT_REQUIREMENT#g" \
  "$ROOT_DIR/packaging/macos/dev.manis.app.helper.plist" > "$PLIST_OUT"
if [[ "${MANIS_ALLOW_INSECURE_LOCAL_HELPER:-0}" == "1" ]]; then
  plutil -insert EnvironmentVariables.MANIS_ALLOW_INSECURE_LOCAL_HELPER -string 1 "$PLIST_OUT"
fi
plutil -lint "$PLIST_OUT" >/dev/null

if [[ -n "${MANIS_CODESIGN_IDENTITY:-}" ]]; then
  codesign --force --options runtime --identifier "$HELPER_ID" --sign "$MANIS_CODESIGN_IDENTITY" "$HELPER_TOOLS_DIR/$HELPER_ID"
  codesign --force --options runtime --identifier "$HELPERCTL_ID" --sign "$MANIS_CODESIGN_IDENTITY" "$MACOS_DIR/manis-helperctl"
  codesign --force --options runtime --identifier "$LOCAL_INSTALLER_ID" --sign "$MANIS_CODESIGN_IDENTITY" "$MACOS_DIR/manis-local-helper-install"
  codesign --force --options runtime --identifier "dev.manis.app" --sign "$MANIS_CODESIGN_IDENTITY" "$BUILD_APP_DIR"
else
  codesign --force --identifier "$HELPER_ID" --sign - "$HELPER_TOOLS_DIR/$HELPER_ID" >/dev/null 2>&1 || true
  codesign --force --identifier "$HELPERCTL_ID" --sign - "$MACOS_DIR/manis-helperctl" >/dev/null 2>&1 || true
  codesign --force --identifier "$LOCAL_INSTALLER_ID" --sign - "$MACOS_DIR/manis-local-helper-install" >/dev/null 2>&1 || true
  codesign --force --identifier "dev.manis.app" --sign - "$BUILD_APP_DIR" >/dev/null 2>&1 || true
  if [[ "${MANIS_ALLOW_INSECURE_LOCAL_HELPER:-0}" == "1" ]]; then
    echo "warning: built with the local administrator-installed TUN helper; do not distribute" >&2
  else
    echo "warning: no MANIS_CODESIGN_IDENTITY set; helper registration is expected to fail on production macOS" >&2
  fi
fi

if [[ -e "$APP_DIR" ]]; then
  BACKUP_DIR="$DIST_DIR/Manis.app.previous.$(date +%Y%m%d%H%M%S)"
  mv "$APP_DIR" "$BACKUP_DIR"
  echo "previous bundle moved to $BACKUP_DIR" >&2
fi
mv "$BUILD_APP_DIR" "$APP_DIR"
rmdir "$BUILD_ROOT"
trap - EXIT
echo "$APP_DIR"
