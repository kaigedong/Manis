# Tailscale coexistence on Linux

Manis TUN and Tailscale can run at the same time. This document records what works, what is
refused on purpose, and how to verify a machine. It was verified against Tailscale 1.104.2 and
Mihomo 1.19.32 on Arch Linux; routing layout has changed across Tailscale releases, so the
verification section matters more than any single claim below.

## Why it works at all

A TUN device is not exclusive: Manis, ZeroTier and Tailscale each create their own interface.
Coexistence is decided by routing tables and DNS, not by the device:

| Resource | Manis TUN | Tailscale |
| --- | --- | --- |
| IPv4 default route | Owns it in `table 2022` | Takes it in `table 52` for an exit node (older releases: `main`) |
| Specific prefixes | Leaves non-default routes alone | Owns peer `/32`s, `100.64.0.0/10` and accepted subnet routes |
| DNS | Claims `:53` through `resolvectl domain Meta ~.` and `dns-hijack` | Owns `100.100.100.100` for MagicDNS |

Mihomo installs `not dport 53 lookup main suppress_prefixlength 0` and a TUN default in
`table 2022`. Non-DNS traffic first looks up the main table while ignoring its default route, so
every specific prefix wins; only traffic without a specific route falls through to the TUN default.
That is why ZeroTier works without configuration: it only adds a more specific subnet route and does
not touch DNS or the default route.

Current Tailscale additionally installs its own routes in `table 52` behind a higher-priority rule:

```text
5270: from all lookup 52
100.100.100.100 dev tailscale0 table 52
100.x.y.z       dev tailscale0 table 52
```

Because `5270` runs before Mihomo's `9000`-`9010` rules, tailnet traffic and MagicDNS queries never
reach the TUN. Peer addresses, accepted subnet routes and MagicDNS therefore work with no Manis
change at all.

## What Manis adds for Tailscale

DNS is the historical exception. Mihomo deliberately resolves port 53 through its own table so that
`dns-hijack` cannot be bypassed. On releases that keep the `100.100.100.100` route only in the main
table, that captured MagicDNS and answered it with fake-IP addresses. The Linux helper therefore
installs one compatibility rule while TUN is active:

```text
pref 8000: to 100.100.100.100/32 lookup main
```

On current Tailscale the rule is never reached (`5270` wins first) and costs nothing; on older
layouts it restores MagicDNS. It is installed before the resolver is repointed, removed on restore
(best effort), and does nothing when Tailscale is absent: the main-table lookup finds nothing and
routing continues to the TUN default. It is independent of which program starts first.

## Supported combinations

- Tailscale connected, no exit node: tailnet addresses, accepted subnet routes and MagicDNS all
  work while TUN is enabled.
- Tailscale with `--accept-dns=false`: works too; MagicDNS stays off by design.
- ZeroTier: works without configuration.

## Refused combinations

- **Tailscale exit node.** An exit node needs the IPv4 default route, and so does Manis TUN. Only one
  can win: on current Tailscale `table 52` would take everything before the TUN, on older releases
  the TUN rule would ignore the exit node. Either way one side silently stops working. Manis looks
  for a default route on a `tailscale*` interface in every routing table and refuses to enable TUN
  with an explicit message. Turn the exit node off first.
- **Tailscale "override local DNS".** It replaces the system resolver instead of using per-link
  split DNS, so Manis no longer sees the queries it needs for fake-IP and domain rules. Keep
  Tailscale's default split-DNS mode.

## Optional: keep the WireGuard transport direct

`tailscaled` talks to peers over UDP 41641 and to DERP over TCP 443. Those packets are ordinary
traffic to Mihomo, so they are proxied unless a rule says otherwise. This still works, but it turns
the direct peer path into a nested tunnel and degrades NAT traversal. Add a rule above the rest of
the list if that matters:

```text
DST-PORT,41641,DIRECT
```

DERP over 443 cannot be distinguished from other TLS traffic and remains proxied.

## Verifying a machine

```bash
ip rule show                     # 5270 tailscale, 9000-9010 mihomo, 8000 compatibility
ip route show table 52           # tailscale0 peer routes and 100.100.100.100
ip route show table 2022         # TUN default
ip route get 8.8.8.8             # -> Meta
ip route get 100.100.100.100     # -> tailscale0
ip route get 10.147.20.1         # -> zt* for a ZeroTier peer
resolvectl status                # Meta has ~.; tailscale0 keeps ~ts.net
getent hosts <peer>.<tailnet>.ts.net
tailscale status                 # no exit node
```

The exit-node check runs `ip -4 route show table all` and matches `default`/`0.0.0.0/0` on a
`tailscale*` device, which covers both the current `table 52` layout and the older main-table one.
IPv6 defaults are not checked; the TUN is IPv4-only.
