# Builds the portable Windows desktop bundle (no installer).
#
# Manis.exe is the GPUI desktop application.
param(
    [string]$OutputDirectory = (Join-Path $PSScriptRoot "../../dist/windows"),
    [string]$Configuration = "release"
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
Push-Location $root
try {
    cargo build -p manis-ui --$Configuration --locked
    if ($LASTEXITCODE -ne 0) { throw "manis-ui build failed" }

    $stage = Join-Path $OutputDirectory "Manis"
    if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
    New-Item -ItemType Directory -Force -Path $stage | Out-Null

    Copy-Item (Join-Path $root "target/$Configuration/manis-ui.exe") (Join-Path $stage "Manis.exe")
    Copy-Item (Join-Path $root "assets/brand/manis-mark.svg") (Join-Path $stage "manis-mark.svg")

    $archive = Join-Path $OutputDirectory "Manis-windows-x86_64.zip"
    if (Test-Path $archive) { Remove-Item -Force $archive }
    Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $archive
    Write-Output $archive
}
finally {
    Pop-Location
}
