#!/usr/bin/env bash
# Fetches the pinned NSH Android embed runtime into android/app/libs/nsh.aar.
#
# The 37 MB AAR is a build dependency, not a repository artifact. Pinning the release archive's
# SHA-256 keeps a compromised or retagged download from entering a build.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT/android/app/libs/nsh.aar"
VERSION="${NSH_VERSION:-v0.3.0}"
ARCHIVE_SHA256="${NSH_ARCHIVE_SHA256:-adc5c71e547c9901462208b50630548b3ae618b53555c581937a1d5c4e05eda6}"
URL="https://github.com/longbridge/nsh/releases/download/$VERSION/nsh-embed-android.zip"

if [[ -f "$DEST" ]]; then
  echo "NSH Android runtime already present: $DEST" >&2
  exit 0
fi

mkdir -p "$(dirname "$DEST")"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Downloading NSH $VERSION Android runtime" >&2
curl -fL --retry 3 "$URL" -o "$tmp/nsh-embed-android.zip"

if command -v sha256sum >/dev/null 2>&1; then
  printf '%s  %s\n' "$ARCHIVE_SHA256" "$tmp/nsh-embed-android.zip" | sha256sum -c -
else
  printf '%s  %s\n' "$ARCHIVE_SHA256" "$tmp/nsh-embed-android.zip" | shasum -a 256 -c -
fi

unzip -p "$tmp/nsh-embed-android.zip" nsh-embed-android/nsh.aar > "$DEST"
echo "Installed $DEST" >&2
