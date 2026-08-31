# Arch Linux package

The Arch package builds Manis from the current checkout and installs:

- `/usr/bin/manis`
- `/usr/lib/manis/mihomo`, the SHA-256-verified stable first-launch seed
- `/usr/lib/manis/manis-linux-helper`, a fixed-purpose TUN DNS helper
- a PolicyKit policy restricted to that helper
- a freedesktop desktop entry
- a scalable application icon
- the project license and third-party notices

The package depends on polkit for its narrow privileged integration. Package installation assigns
only `CAP_NET_ADMIN` and `CAP_NET_RAW` to the root-owned packaged core. The fixed-purpose helper may
only install or restore DNS routing for the managed `Meta` interface, and PolicyKit permits that
helper for the active local session without another password prompt. Ordinary proxy mode and the
GUI remain unprivileged. Manis downloads and verifies application updates in the background;
**Restart and update** runs `pacman -U` through `pkexec`, so the package upgrade itself remains the
single administrative authorization point. Source builds and binaries outside `/usr/bin/manis` do
not self-update.

The Linux GPUI backend is compiled with Wayland and X11 support. A native Wayland session is the
primary target; X11 remains available as a compatibility fallback. The build fetches the official
stable Mihomo release for x86_64 and verifies its GitHub release digest before packaging. sing-box
is not bundled.

Build on an up-to-date Arch Linux system:

```bash
cd packaging/archlinux
makepkg --syncdeps --cleanbuild
```

Install the resulting package with `pacman -U`. System proxy and TUN integration should still be
validated on the target desktop, Polkit agent, network manager, and kernel combination.
