#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DIST_DIR="${RELAY_MACOS_DIST_DIR:-$ROOT_DIR/dist/macos}"
APP_DIR="$DIST_DIR/Relay.app"
mkdir -p "$DIST_DIR"
BUILD_ROOT="$(mktemp -d "$DIST_DIR/Relay.app.build.XXXXXX")"
BUILD_APP_DIR="$BUILD_ROOT/Relay.app"
cleanup_build_root() {
  if [[ -n "${BUILD_ROOT:-}" && "$BUILD_ROOT" == "$DIST_DIR"/Relay.app.build.* && -d "$BUILD_ROOT" ]]; then
    /bin/rm -R "$BUILD_ROOT"
  fi
}
trap cleanup_build_root EXIT
CONTENTS_DIR="$BUILD_APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
LAUNCH_DAEMONS_DIR="$CONTENTS_DIR/Library/LaunchDaemons"
HELPER_TOOLS_DIR="$CONTENTS_DIR/Library/PrivilegedHelperTools"
RESOURCES_DIR="$CONTENTS_DIR/Resources/mihomo"
HELPER_ID="dev.relay.prototype.helper"
HELPERCTL_ID="dev.relay.prototype.helperctl"
CLIENT_REQUIREMENT="${RELAY_CLIENT_REQUIREMENT:-identifier \"$HELPERCTL_ID\"}"
PARENT_REQUIREMENT="${RELAY_PARENT_REQUIREMENT:-identifier \"dev.relay.prototype\"}"
MIHOMO_SOURCE="${RELAY_MIHOMO_BINARY:-}"

if [[ -n "${RELAY_CODESIGN_IDENTITY:-}" ]]; then
  if [[ -z "${RELAY_CLIENT_REQUIREMENT:-}" || -z "${RELAY_PARENT_REQUIREMENT:-}" ]]; then
    echo "signed helper builds require RELAY_CLIENT_REQUIREMENT and RELAY_PARENT_REQUIREMENT" >&2
    exit 1
  fi
  case "$CLIENT_REQUIREMENT" in
    *"identifier \"$HELPERCTL_ID\""*"anchor apple generic"*"certificate leaf[subject.OU]"*) ;;
    *) echo "RELAY_CLIENT_REQUIREMENT must pin helperctl identifier, Apple anchor, and Team ID" >&2; exit 1 ;;
  esac
  case "$PARENT_REQUIREMENT" in
    *"identifier \"dev.relay.prototype\""*"anchor apple generic"*"certificate leaf[subject.OU]"*) ;;
    *) echo "RELAY_PARENT_REQUIREMENT must pin Relay identifier, Apple anchor, and Team ID" >&2; exit 1 ;;
  esac
fi

cargo build -p relay-ui --release

mkdir -p "$MACOS_DIR" "$LAUNCH_DAEMONS_DIR" "$HELPER_TOOLS_DIR" "$RESOURCES_DIR"

cp "$ROOT_DIR/packaging/macos/Info.plist" "$CONTENTS_DIR/Info.plist"
plutil -insert RelayParentCodeSigningRequirement -string "$PARENT_REQUIREMENT" "$CONTENTS_DIR/Info.plist"
if [[ "${RELAY_ALLOW_INSECURE_LOCAL_HELPER:-0}" == "1" ]]; then
  plutil -insert RelayAllowInsecureLocalHelper -bool YES "$CONTENTS_DIR/Info.plist"
fi
cp "$ROOT_DIR/target/release/relay-ui" "$MACOS_DIR/Relay"

swiftc \
  -framework Foundation \
  -framework Security \
  "$ROOT_DIR/packaging/macos/RelayPrivilegedHelper.swift" \
  -o "$HELPER_TOOLS_DIR/$HELPER_ID"

swiftc \
  -framework Foundation \
  -framework ServiceManagement \
  -framework Security \
  "$ROOT_DIR/packaging/macos/relay-helperctl.swift" \
  -o "$MACOS_DIR/relay-helperctl"

if [[ -n "$MIHOMO_SOURCE" ]]; then
  if [[ ! -x "$MIHOMO_SOURCE" ]]; then
    echo "RELAY_MIHOMO_BINARY must point to an executable Mihomo binary" >&2
    exit 1
  fi
  cp "$MIHOMO_SOURCE" "$RESOURCES_DIR/mihomo"
  chmod 0755 "$RESOURCES_DIR/mihomo"
else
  echo "warning: RELAY_MIHOMO_BINARY not set; privileged TUN start will fail until Mihomo is bundled" >&2
fi

PLIST_OUT="$LAUNCH_DAEMONS_DIR/dev.relay.prototype.helper.plist"
sed \
  -e "s#identifier \"dev.relay.prototype.helperctl\"#$CLIENT_REQUIREMENT#g" \
  "$ROOT_DIR/packaging/macos/dev.relay.prototype.helper.plist" > "$PLIST_OUT"
if [[ "${RELAY_ALLOW_INSECURE_LOCAL_HELPER:-0}" == "1" ]]; then
  plutil -insert EnvironmentVariables.RELAY_ALLOW_INSECURE_LOCAL_HELPER -string 1 "$PLIST_OUT"
fi
plutil -lint "$PLIST_OUT" >/dev/null

if [[ -n "${RELAY_CODESIGN_IDENTITY:-}" ]]; then
  codesign --force --options runtime --identifier "$HELPER_ID" --sign "$RELAY_CODESIGN_IDENTITY" "$HELPER_TOOLS_DIR/$HELPER_ID"
  codesign --force --options runtime --identifier "$HELPERCTL_ID" --sign "$RELAY_CODESIGN_IDENTITY" "$MACOS_DIR/relay-helperctl"
  codesign --force --options runtime --identifier "dev.relay.prototype" --sign "$RELAY_CODESIGN_IDENTITY" "$BUILD_APP_DIR"
else
  codesign --force --identifier "$HELPER_ID" --sign - "$HELPER_TOOLS_DIR/$HELPER_ID" >/dev/null 2>&1 || true
  codesign --force --identifier "$HELPERCTL_ID" --sign - "$MACOS_DIR/relay-helperctl" >/dev/null 2>&1 || true
  codesign --force --identifier "dev.relay.prototype" --sign - "$BUILD_APP_DIR" >/dev/null 2>&1 || true
  echo "warning: no RELAY_CODESIGN_IDENTITY set; helper registration is expected to fail on production macOS" >&2
fi

if [[ -e "$APP_DIR" ]]; then
  BACKUP_DIR="$DIST_DIR/Relay.app.previous.$(date +%Y%m%d%H%M%S)"
  mv "$APP_DIR" "$BACKUP_DIR"
  echo "previous bundle moved to $BACKUP_DIR" >&2
fi
mv "$BUILD_APP_DIR" "$APP_DIR"
rmdir "$BUILD_ROOT"
trap - EXIT
echo "$APP_DIR"
