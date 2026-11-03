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
CONTENTS_RESOURCES_DIR="$CONTENTS_DIR/Resources"
MIHOMO_RESOURCES_DIR="$CONTENTS_RESOURCES_DIR/mihomo"
BRAND_MARK="$ROOT_DIR/assets/brand/manis-mark.svg"
HELPER_ID="dev.manis.app.helper"
HELPERCTL_ID="dev.manis.app.helperctl"
LOCAL_INSTALLER_ID="dev.manis.app.helper.local-installer"
CLIENT_REQUIREMENT="${MANIS_CLIENT_REQUIREMENT:-identifier \"$HELPERCTL_ID\"}"
PARENT_REQUIREMENT="${MANIS_PARENT_REQUIREMENT:-identifier \"dev.manis.app\"}"
MIHOMO_SOURCE="${MANIS_MIHOMO_BINARY:-}"
BUNDLE_VERSION="${MANIS_BUNDLE_VERSION:-0.1.0}"
BUNDLE_BUILD="${MANIS_BUNDLE_BUILD:-1}"
CARGO_BUILD_DIR="${CARGO_TARGET_DIR:-$ROOT_DIR/target}"

if [[ ! "$BUNDLE_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "MANIS_BUNDLE_VERSION must contain exactly three numeric components" >&2
  exit 1
fi
if [[ ! "$BUNDLE_BUILD" =~ ^[1-9][0-9]*$ ]]; then
  echo "MANIS_BUNDLE_BUILD must be a positive integer" >&2
  exit 1
fi

if [[ "${MANIS_ALLOW_INSECURE_LOCAL_HELPER:-0}" == "1" ]]; then
  echo "MANIS_ALLOW_INSECURE_LOCAL_HELPER is obsolete; the default build uses administrator-approved code fingerprints" >&2
  exit 1
fi
HELPER_SWIFT_FLAGS=()
HELPER_INSTALL_MODE="smappservice"
if [[ -z "${MANIS_CODESIGN_IDENTITY:-}" ]]; then
  HELPER_SWIFT_FLAGS=(-D MANIS_ADMINISTRATOR_HELPER)
  HELPER_INSTALL_MODE="administrator"
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

MANIS_BUILD_VERSION="$BUNDLE_VERSION" CARGO_TARGET_DIR="$CARGO_BUILD_DIR" cargo build -p manis-ui --release --locked

mkdir -p "$MACOS_DIR" "$LAUNCH_DAEMONS_DIR" "$HELPER_TOOLS_DIR" "$MIHOMO_RESOURCES_DIR"

cp "$ROOT_DIR/packaging/macos/Info.plist" "$CONTENTS_DIR/Info.plist"
plutil -replace CFBundleShortVersionString -string "$BUNDLE_VERSION" "$CONTENTS_DIR/Info.plist"
plutil -replace CFBundleVersion -string "$BUNDLE_BUILD" "$CONTENTS_DIR/Info.plist"
plutil -insert ManisParentCodeSigningRequirement -string "$PARENT_REQUIREMENT" "$CONTENTS_DIR/Info.plist"
plutil -insert ManisHelperInstallMode -string "$HELPER_INSTALL_MODE" "$CONTENTS_DIR/Info.plist"
cp "$CARGO_BUILD_DIR/release/manis-ui" "$MACOS_DIR/Manis"

ICONSET_DIR="$BUILD_ROOT/Manis.iconset"
mkdir -p "$ICONSET_DIR"
for icon_size in 16 32 128 256 512; do
  double_size=$((icon_size * 2))
  sips -s format png -z "$icon_size" "$icon_size" "$BRAND_MARK" \
    --out "$ICONSET_DIR/icon_${icon_size}x${icon_size}.png" >/dev/null
  sips -s format png -z "$double_size" "$double_size" "$BRAND_MARK" \
    --out "$ICONSET_DIR/icon_${icon_size}x${icon_size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET_DIR" -o "$CONTENTS_RESOURCES_DIR/Manis.icns"

# Keep the shared trust policy in one source while Swift's executable entrypoints stay standalone.
compile_helper() {
  local source="$1"
  local output="$2"
  local combined="$BUILD_ROOT/$(basename "$source")"
  cat "$ROOT_DIR/packaging/macos/HelperSecurity.swift" \
    "$ROOT_DIR/packaging/macos/MihomoReleaseVerifier.swift" \
    "$ROOT_DIR/packaging/macos/HelperProtocol.swift" "$source" > "$combined"
  swiftc ${HELPER_SWIFT_FLAGS[@]+"${HELPER_SWIFT_FLAGS[@]}"} \
    -framework CryptoKit -framework Foundation -framework ServiceManagement -framework Security \
    "$combined" -o "$output"
}
compile_helper "$ROOT_DIR/packaging/macos/ManisPrivilegedHelper.swift" "$HELPER_TOOLS_DIR/$HELPER_ID"
compile_helper "$ROOT_DIR/packaging/macos/manis-helperctl.swift" "$MACOS_DIR/manis-helperctl"
compile_helper "$ROOT_DIR/packaging/macos/manis-local-helper-install.swift" "$MACOS_DIR/manis-local-helper-install"

if [[ -z "$MIHOMO_SOURCE" ]]; then
  MIHOMO_SOURCE="$BUILD_ROOT/mihomo"
  "$ROOT_DIR/packaging/fetch-mihomo.sh" "$MIHOMO_SOURCE"
fi
if [[ ! -x "$MIHOMO_SOURCE" ]]; then
  echo "MANIS_MIHOMO_BINARY must point to an executable Mihomo binary" >&2
  exit 1
fi
cp "$MIHOMO_SOURCE" "$MIHOMO_RESOURCES_DIR/mihomo"
chmod 0755 "$MIHOMO_RESOURCES_DIR/mihomo"

PLIST_OUT="$LAUNCH_DAEMONS_DIR/dev.manis.app.helper.plist"
sed \
  -e "s#identifier \"dev.manis.app.helperctl\"#$CLIENT_REQUIREMENT#g" \
  "$ROOT_DIR/packaging/macos/dev.manis.app.helper.plist" > "$PLIST_OUT"
plutil -lint "$PLIST_OUT" >/dev/null

if [[ -n "${MANIS_CODESIGN_IDENTITY:-}" ]]; then
  codesign --force --options runtime --identifier "$HELPER_ID" --sign "$MANIS_CODESIGN_IDENTITY" "$HELPER_TOOLS_DIR/$HELPER_ID"
  codesign --force --options runtime --identifier "$HELPERCTL_ID" --sign "$MANIS_CODESIGN_IDENTITY" "$MACOS_DIR/manis-helperctl"
  codesign --force --options runtime --identifier "$LOCAL_INSTALLER_ID" --sign "$MANIS_CODESIGN_IDENTITY" "$MACOS_DIR/manis-local-helper-install"
  codesign --force --options runtime --identifier "dev.manis.app" --sign "$MANIS_CODESIGN_IDENTITY" "$BUILD_APP_DIR"
else
  codesign --force --options runtime --identifier "$HELPER_ID" --sign - "$HELPER_TOOLS_DIR/$HELPER_ID"
  codesign --force --options runtime --identifier "$HELPERCTL_ID" --sign - "$MACOS_DIR/manis-helperctl"
  codesign --force --options runtime --identifier "$LOCAL_INSTALLER_ID" --sign - "$MACOS_DIR/manis-local-helper-install"
  codesign --force --options runtime --identifier "dev.manis.app" --sign - "$BUILD_APP_DIR"
  echo "ad-hoc build: TUN will request administrator approval and pin this app version" >&2
fi

if [[ -e "$APP_DIR" ]]; then
  BACKUP_DIR="$DIST_DIR/Manis.app.previous.$(date +%Y%m%d%H%M%S)"
  mv "$APP_DIR" "$BACKUP_DIR"
  echo "previous bundle moved to $BACKUP_DIR" >&2
fi
mv "$BUILD_APP_DIR" "$APP_DIR"
cleanup_build_root
trap - EXIT
echo "$APP_DIR"
