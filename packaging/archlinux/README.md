# Arch Linux package

The Arch package builds Manis from the current checkout and installs:

- `/usr/bin/manis`
- `/usr/lib/manis/mihomo`, the SHA-256-verified stable first-launch seed
- a freedesktop desktop entry
- a scalable application icon
- the project license and third-party notices

The package depends on polkit for graphical administrator authorization. Manis downloads and
verifies application updates in the background; **Restart and update** then runs `pacman -U`
through `pkexec` and restarts `/usr/bin/manis` only after pacman succeeds. On the first Linux TUN
activation after install or upgrade, Manis asks Polkit to grant only `CAP_NET_ADMIN` and
`CAP_NET_RAW` to the root-owned packaged core, verifies its ownership and capabilities, switches
the managed runtime to that core, and continues the original TUN request. Ordinary proxy mode and
the GUI remain unprivileged, and no terminal setup is required. Source builds and binaries outside
`/usr/bin/manis` do not self-update.

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
