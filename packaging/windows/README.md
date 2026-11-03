# Windows desktop bundle

`build-release.ps1` builds a portable directory and zip:

- `Manis.exe` — the GPUI desktop application

The managed Mihomo controller transport is not implemented on Windows yet, so
the app can read saved sources and settings but cannot start the core.

Run from a Developer PowerShell with Rust installed:

```powershell
./packaging/windows/build-release.ps1
```
