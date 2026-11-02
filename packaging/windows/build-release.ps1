# Builds the portable Windows NSH bundle (no installer).
#
# Manis.exe is the shared NSH host and manis-ui.exe is the headless engine.
# The managed controller transport on Windows is still unavailable, so the
# shell currently presents saved state; see packaging/windows/README.md.
param(
    [string]$OutputDirectory = (Join-Path $PSScriptRoot "../../dist/windows"),
    [string]$Configuration = "release"
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
Push-Location $root
try {
    cargo build -p manis-nsh-desktop --$Configuration --locked
    if ($LASTEXITCODE -ne 0) { throw "manis-nsh-desktop build failed" }
    cargo build -p manis-ui --$Configuration --locked
    if ($LASTEXITCODE -ne 0) { throw "manis-ui build failed" }

    $stage = Join-Path $OutputDirectory "Manis"
    if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
    New-Item -ItemType Directory -Force -Path $stage | Out-Null

    Copy-Item (Join-Path $root "target/$Configuration/manis-nsh-desktop.exe") (Join-Path $stage "Manis.exe")
    Copy-Item (Join-Path $root "target/$Configuration/manis-ui.exe") (Join-Path $stage "manis-ui.exe")
    Copy-Item (Join-Path $root "assets/brand/manis-mark.svg") (Join-Path $stage "manis-mark.svg")

    $archive = Join-Path $OutputDirectory "Manis-windows-x86_64.zip"
    if (Test-Path $archive) { Remove-Item -Force $archive }
    Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $archive
    Write-Output $archive
}
finally {
    Pop-Location
}
