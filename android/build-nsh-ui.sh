#!/usr/bin/env bash
# Rebuilds the committed NSH TSX assets used by Android and the desktop host.
#
# The NSH CLI is pinned to 0.3.0 (the same release as the Android AAR). CI runs
# this and fails when the committed assets differ, so the bytecode in the APK
# is always reproducible from android/nsh-ui.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NSH="${NSH_BIN:-nsh}"

if ! command -v "$NSH" >/dev/null 2>&1; then
  echo "nsh 0.3.0 is required (cargo install nsh-cli --version 0.3.0 --locked)" >&2
  exit 1
fi

"$NSH" build "$ROOT/android/nsh-ui/App.tsx" --release \
  --out "$ROOT/android/app/src/main/assets/nsh"
