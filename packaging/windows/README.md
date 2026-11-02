# Windows NSH bundle

`build-release.ps1` builds a portable directory and zip:

- `Manis.exe` — the shared NSH desktop host
- `manis-engine.exe` — the headless engine binary

The managed Mihomo controller transport is not implemented on Windows yet, so
the shell can read saved sources and settings but cannot start the core. The
NSH frontend, host bridge and page models are the same as Linux/macOS.

Run from a Developer PowerShell with Rust installed:

```powershell
./packaging/windows/build-release.ps1
```
