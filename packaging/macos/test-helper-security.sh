#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/manis-helper-security.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

for mode in administrator signed; do
  flags=()
  if [[ "$mode" == administrator ]]; then flags=(-D MANIS_ADMINISTRATOR_HELPER); fi
  for source in manis-helperctl ManisPrivilegedHelper manis-local-helper-install; do
    cat "$ROOT/HelperSecurity.swift" "$ROOT/MihomoReleaseVerifier.swift" "$ROOT/$source.swift" > "$WORK/$source.swift"
    swiftc ${flags[@]+"${flags[@]}"} "$WORK/$source.swift" -o "$WORK/$source-$mode"
  done
done

# No helper is installed and no administrator prompt is invoked by this test.
if [[ "$(id -u)" != 0 ]]; then
  if "$WORK/manis-local-helper-install-administrator" reinstall /invalid/Manis.app \
    'identifier "dev.manis.app" and cdhash H"0000000000000000000000000000000000000000"' \
    "$(printf '%064d' 0)" "$(printf '%064d' 0)" "$(id -u)" > "$WORK/installer.log" 2>&1; then
    echo "installer accepted an unprivileged caller" >&2; exit 1
  fi
  grep -q 'administrator authorization is required' "$WORK/installer.log"
fi

cat > "$WORK/parent.swift" <<'SWIFT'
import Foundation
let process = Process()
process.executableURL = URL(fileURLWithPath: CommandLine.arguments[1])
process.arguments = Array(CommandLine.arguments.dropFirst(2))
try process.run()
process.waitUntilExit()
exit(process.terminationStatus)
SWIFT
swiftc "$WORK/parent.swift" -o "$WORK/parent"
for variant in approved forged; do
  app="$WORK/$variant/Manis.app"
  mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
  cp "$WORK/parent" "$app/Contents/MacOS/Manis"
  cp "$ROOT/Info.plist" "$app/Contents/Info.plist"
  printf '%s\n' "$variant" > "$app/Contents/Resources/identity.txt"
  codesign --force --options runtime --identifier dev.manis.app --sign - "$app"
done

cat "$ROOT/HelperSecurity.swift" > "$WORK/probe.swift"
cat >> "$WORK/probe.swift" <<'SWIFT'
do {
    try ManisHelperSecurity.validateParent(
        bundle: URL(fileURLWithPath: CommandLine.arguments[1]),
        requirement: CommandLine.arguments[2]
    )
    print("approved parent")
} catch { fputs("\(error)\n", stderr); exit(1) }
SWIFT
swiftc "$WORK/probe.swift" -o "$WORK/probe"
codesign --force --options runtime --identifier dev.manis.app.helperctl --sign - "$WORK/probe"
printf 'trusted core' | /usr/bin/gzip -c > "$WORK/core.gz"
python3 - "$WORK/oversized.gz" <<'PY'
import gzip
import sys
with gzip.open(sys.argv[1], 'wb') as archive:
    chunk = bytes(1024 * 1024)
    for _ in range(129):
        archive.write(chunk)
PY
cat "$ROOT/HelperSecurity.swift" "$ROOT/MihomoReleaseVerifier.swift" \
  "$ROOT/tests/HelperSecurityTests.swift" > "$WORK/tests.swift"
swiftc "$WORK/tests.swift" -o "$WORK/tests"
"$WORK/tests" "$WORK"
