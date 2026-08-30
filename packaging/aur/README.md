# AUR package

`manis-bin` repackages the immutable Arch Linux artifact produced for each successful `main`
build. It deliberately does not consume the rolling `latest` release because those assets are
replaced on every build.

The AUR helper owns updates for this package. Manis checks the pacman owner of `/usr/bin/manis` and
does not offer its built-in `pacman -U` update path when the owner is `manis-bin`, avoiding a package
name conflict.

Render a release into an AUR checkout:

```bash
packaging/aur/render-pkgbuild.sh VERSION SHA256 /path/to/manis-bin
```

Generate `.SRCINFO` on Arch Linux:

```bash
cd /path/to/manis-bin
makepkg --printsrcinfo > .SRCINFO
```

On a non-Arch host with Docker:

```bash
docker run --rm --platform linux/amd64 -v "$PWD:/aur:ro" archlinux:base-devel bash -lc '
  useradd --create-home builder
  cp -a /aur /home/builder/package
  chown -R builder:builder /home/builder/package
  runuser -u builder -- bash -lc "cd ~/package && makepkg --printsrcinfo"
' > .SRCINFO
```

Always run a clean `makepkg` build and `namcap` before publishing the generated `PKGBUILD` and
`.SRCINFO` to `ssh://aur@aur.archlinux.org/manis-bin.git`.
