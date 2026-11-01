#!/usr/bin/env bash
# Runs the NSH UI tests headlessly against the pinned NSH 0.3.0 CLI.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NSH="${NSH_BIN:-nsh}"

if ! command -v "$NSH" >/dev/null 2>&1; then
  echo "nsh 0.3.0 is required (cargo install nsh-cli --version 0.3.0 --locked)" >&2
  exit 1
fi

"$NSH" test "$ROOT/android/nsh-ui"
