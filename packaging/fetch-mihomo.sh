#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: packaging/fetch-mihomo.sh OUTPUT" >&2
  exit 2
fi

OUTPUT="$1"
OS_NAME="${MANIS_MIHOMO_OS:-$(uname -s)}"
ARCH_NAME="${MANIS_MIHOMO_ARCH:-$(uname -m)}"
API_URL="${MANIS_MIHOMO_RELEASE_API:-https://api.github.com/repos/MetaCubeX/mihomo/releases/latest}"
WORK_DIR="$(mktemp -d)"
cleanup() {
  if [[ -d "$WORK_DIR" ]]; then
    /bin/rm -R "$WORK_DIR"
  fi
}
trap cleanup EXIT

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    echo "no SHA-256 utility is available" >&2
    return 1
  fi
}

case "$OS_NAME:$ARCH_NAME" in
  Darwin:arm64) asset_stem="mihomo-darwin-arm64-go122"; archive_kind="gz" ;;
  Darwin:x86_64) asset_stem="mihomo-darwin-amd64-v2-go122"; archive_kind="gz" ;;
  Linux:x86_64) asset_stem="mihomo-linux-amd64-v2"; archive_kind="gz" ;;
  MINGW*:x86_64|MSYS*:x86_64) asset_stem="mihomo-windows-amd64-v2"; archive_kind="zip" ;;
  *)
    echo "unsupported Mihomo packaging target: $OS_NAME $ARCH_NAME" >&2
    exit 1
    ;;
esac

METADATA="$WORK_DIR/release.json"
metadata_headers=(
  -H "Accept: application/vnd.github+json"
  -H "User-Agent: Manis-packager"
)
if [[ -n "${MANIS_GITHUB_TOKEN:-}" ]]; then
  metadata_headers+=(-H "Authorization: Bearer $MANIS_GITHUB_TOKEN")
fi
echo "Resolving the latest Mihomo release" >&2
curl --fail --silent --show-error --location \
  --retry 5 --retry-all-errors --retry-delay 5 --retry-max-time 120 \
  --max-filesize 1048576 \
  "${metadata_headers[@]}" \
  "$API_URL" > "$METADATA"
if (( $(wc -c < "$METADATA") > 1048576 )); then
  echo "Mihomo release metadata exceeds 1 MiB" >&2
  exit 1
fi

IFS=$'\t' read -r version asset_name download_url digest < <(
  python3 - "$METADATA" "$asset_stem" "$archive_kind" <<'PY'
import json
import pathlib
import sys

metadata = json.loads(pathlib.Path(sys.argv[1]).read_text())
stem, archive = sys.argv[2], sys.argv[3]
version = metadata.get("tag_name", "")
expected = f"{stem}-{version}.{archive}"
matches = [asset for asset in metadata.get("assets", []) if asset.get("name") == expected]
if len(matches) != 1:
    raise SystemExit(f"release {version!r} does not contain exactly one {expected!r} asset")
asset = matches[0]
digest = asset.get("digest", "")
if not digest.startswith("sha256:") or len(digest) != 71:
    raise SystemExit(f"release asset {expected!r} has no valid SHA-256 digest")
print(version, expected, asset["browser_download_url"], digest.removeprefix("sha256:"), sep="\t")
PY
)

ARCHIVE="$WORK_DIR/$asset_name"
echo "Downloading $asset_name" >&2
curl --fail --silent --show-error --location \
  --retry 5 --retry-all-errors --retry-delay 5 --retry-max-time 120 \
  --max-filesize 67108864 \
  -H "User-Agent: Manis-packager" \
  "$download_url" > "$ARCHIVE"
if (( $(wc -c < "$ARCHIVE") > 67108864 )); then
  echo "Mihomo release asset exceeds 64 MiB" >&2
  exit 1
fi

actual_digest="$(sha256_file "$ARCHIVE")"
if [[ "$actual_digest" != "$digest" ]]; then
  echo "Mihomo asset digest mismatch for $asset_name" >&2
  exit 1
fi

mkdir -p "$(dirname "$OUTPUT")"
STAGED="$WORK_DIR/mihomo"
case "$archive_kind" in
  gz) gzip -dc "$ARCHIVE" > "$STAGED" ;;
  zip) unzip -p "$ARCHIVE" > "$STAGED" ;;
esac
if (( $(wc -c < "$STAGED") == 0 || $(wc -c < "$STAGED") > 134217728 )); then
  echo "unpacked Mihomo binary has an invalid size" >&2
  exit 1
fi
chmod 0755 "$STAGED"
if [[ "${MANIS_MIHOMO_SKIP_VERSION_CHECK:-0}" != "1" ]]; then
  version_output="$("$STAGED" -v 2>&1)"
  if [[ "$version_output" != *"$version"* ]]; then
    echo "downloaded Mihomo did not report expected version $version" >&2
    exit 1
  fi
fi
mv "$STAGED" "$OUTPUT"
echo "$version"
