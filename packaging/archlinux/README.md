# Arch Linux package

The Arch package builds Manis from the current checkout and installs:

- `/usr/bin/manis`
- a freedesktop desktop entry
- a scalable application icon
- the project license and third-party notices

The Linux GPUI backend is compiled with Wayland and X11 support. A native Wayland session is the
primary target; X11 remains available as a compatibility fallback. The package does not contain or
download Mihomo or sing-box.

Build on an up-to-date Arch Linux system:

```bash
cd packaging/archlinux
makepkg --syncdeps --cleanbuild
```

Install the resulting package with `pacman -U`. This package is experimental: system proxy and TUN
integration still require platform-specific implementation and real-device validation.
