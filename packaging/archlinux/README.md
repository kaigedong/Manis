# Arch Linux package

The Arch package builds Manis from the current checkout and installs:

- `/usr/bin/manis`
- `/usr/lib/manis/mihomo`, the SHA-256-verified stable first-launch seed
- a freedesktop desktop entry
- a scalable application icon
- the project license and third-party notices

The Linux GPUI backend is compiled with Wayland and X11 support. A native Wayland session is the
primary target; X11 remains available as a compatibility fallback. The build fetches the official
stable Mihomo release for x86_64 and verifies its GitHub release digest before packaging. sing-box
is not bundled.

Build on an up-to-date Arch Linux system:

```bash
cd packaging/archlinux
makepkg --syncdeps --cleanbuild
```

Install the resulting package with `pacman -U`. This package is experimental: system proxy and TUN
integration still require platform-specific implementation and real-device validation.
