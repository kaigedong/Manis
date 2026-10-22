# Tailscale coexistence on Linux

Manis TUN and Tailscale can run at the same time. This document records what works, what is
refused on purpose, and how to verify a machine.

## Why it works at all

A TUN device is not exclusive: Manis, ZeroTier and Tailscale each create their own interface.
Coexistence is decided by three namespaces, not by the device:

| Resource | Manis TUN | Tailscale |
| --- | --- | --- |
| IPv4 default route | Takes it in its own policy table (`table 2022`) | Takes it in `main` only for an exit node |
| Specific prefixes | Leaves `main` routes with a non-zero prefix length alone | Owns `100.64.0.0/10` plus peer and accepted subnet routes |
| DNS | Claims `:53` through `resolvectl domain Meta ~.` and `dns-hijack` | Owns `100.100.100.100` for MagicDNS |

Mihomo installs the rule `not dport 53 lookup main suppress_prefixlength 0`. Non-DNS traffic first
looks up the main table while ignoring its default route, so every specific prefix wins; only
traffic without a specific route falls through to the TUN default. That is why ZeroTier works
without any configuration: it only adds a more specific subnet route and does not touch DNS or the
default route. Tailscale's peer and subnet routes follow the same path.

## What Manis adds for Tailscale

DNS is the one exception. Mihomo deliberately resolves port 53 through its own table so that
`dns-hijack` cannot be bypassed, which also captured `100.100.100.100:53` and answered MagicDNS
names with fake-IP addresses. The Linux helper therefore installs one policy rule while TUN is
active:

```text
pref 8000: to 100.100.100.100/32 lookup main
```

The rule is installed before the resolver is repointed, removed on restore (best effort), and does
nothing when Tailscale is absent: `main` has no route for the address, the lookup falls through, and
routing continues to the TUN default. Because the rule is independent of when `tailscaled` starts,
no start order is required.

## Supported combinations

- Tailscale connected, no exit node: tailnet addresses, accepted subnet routes and MagicDNS all
  work while TUN is enabled.
- Tailscale with `--accept-dns=false`: works too; MagicDNS stays off by design.
- ZeroTier: works without configuration.

## Refused combinations

- **Tailscale exit node.** An exit node installs `0.0.0.0/0` in the main table, which the TUN
  policy rule ignores by design. The exit node would look enabled while every packet still entered
  the TUN. Manis detects the default route on a `tailscale*` interface and refuses to enable TUN
  with an explicit message instead of failing silently. Turn the exit node off first.
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
ip rule show                     # pref 8000 rule, mihomo 9000-9010 rules
ip route show table 2022         # TUN default
ip route get 8.8.8.8             # -> Meta
ip route get 100.100.100.100     # -> tailscale0 when Tailscale is up
ip route get 10.147.20.1         # -> zt* for a ZeroTier peer
resolvectl status                # Meta has ~.; tailscale0 keeps ~ts.net
tailscale status                 # no exit node
```

`/proc/net/route` is the check Manis uses for the exit-node refusal; it reports the main table and
IPv4 only, which matches the TUN itself.
