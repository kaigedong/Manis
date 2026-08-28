#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

version="$(awk '
    /^\[workspace\.package\]$/ { in_workspace_package = 1; next }
    /^\[/ && in_workspace_package { exit }
    in_workspace_package && /^version = "/ {
      value = $0
      sub(/^version = "/, "", value)
      sub(/"$/, "", value)
      print value
      exit
    }
  ' "$ROOT_DIR/Cargo.toml")"

if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "workspace package version must contain exactly three numeric components" >&2
  exit 1
fi

printf '%s\n' "$version"
