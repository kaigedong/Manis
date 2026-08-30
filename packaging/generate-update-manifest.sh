#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 RELEASE_ASSET_DIRECTORY" >&2
  exit 2
fi

release_dir="$1"
version="${MANIS_PACKAGE_VERSION:-}"
commit="${MANIS_BUILD_COMMIT:-}"

if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "MANIS_PACKAGE_VERSION must contain exactly three numeric components" >&2
  exit 1
fi
if [[ ! "$commit" =~ ^[0-9a-fA-F]{40}$ ]]; then
  echo "MANIS_BUILD_COMMIT must be a 40-character Git commit" >&2
  exit 1
fi

single_asset() {
  local pattern="$1"
  local matches=()
  shopt -s nullglob
  matches=("$release_dir"/$pattern)
  shopt -u nullglob
  if [[ ${#matches[@]} -ne 1 ]]; then
    echo "expected exactly one release asset matching $pattern" >&2
    exit 1
  fi
  printf '%s\n' "${matches[0]}"
}

asset_json() {
  local platform="$1"
  local architecture="$2"
  local path="$3"
  local checksum_path="$path.sha256"
  local name checksum size

  [[ -f "$checksum_path" ]] || {
    echo "missing checksum for $path" >&2
    exit 1
  }
  name="${path##*/}"
  checksum="$(awk 'NR == 1 { print $1 }' "$checksum_path")"
  if [[ ! "$checksum" =~ ^[0-9a-fA-F]{64}$ ]]; then
    echo "invalid SHA-256 checksum for $path" >&2
    exit 1
  fi
  size="$(wc -c < "$path" | tr -d '[:space:]')"

  checksum="$(printf '%s' "$checksum" | tr '[:upper:]' '[:lower:]')"
  jq -n \
    --arg platform "$platform" \
    --arg architecture "$architecture" \
    --arg name "$name" \
    --arg sha256 "$checksum" \
    --argjson size "$size" \
    '{platform: $platform, architecture: $architecture, name: $name, sha256: $sha256, size: $size}'
}

macos_arm64="$(single_asset "Manis-$version-macos-arm64-unsigned.zip")"
macos_x86_64="$(single_asset "Manis-$version-macos-x86_64-unsigned.zip")"
linux_x86_64="$(single_asset "manis-$version-1-x86_64.pkg.tar.zst")"

manifest="$release_dir/manis-update.json"
temporary="$manifest.tmp"
trap 'rm -f "$temporary"' EXIT

commit="$(printf '%s' "$commit" | tr '[:upper:]' '[:lower:]')"
jq -n \
  --arg version "$version" \
  --arg commit "$commit" \
  --argjson macos_arm64 "$(asset_json macos aarch64 "$macos_arm64")" \
  --argjson macos_x86_64 "$(asset_json macos x86_64 "$macos_x86_64")" \
  --argjson linux_x86_64 "$(asset_json linux x86_64 "$linux_x86_64")" \
  '{schema_version: 1, version: $version, commit: $commit, assets: [$macos_arm64, $macos_x86_64, $linux_x86_64]}' \
  > "$temporary"
mv "$temporary" "$manifest"
(cd "$release_dir" && sha256sum manis-update.json > manis-update.json.sha256)
trap - EXIT
