# Product

## Platform

adaptive

## Stack

Manis is a Rust + GPUI desktop application targeting Windows, macOS, and Linux. Mihomo is the managed routing core. The GPUI application owns the product model, orchestration, and presentation.

## Users

Primary users are desktop proxy users who understand the intent of “some traffic should use one route and other traffic another,” but do not want to understand Clash/Mihomo YAML, provider internals, or networking jargon. The initial user is technically curious but new to proxy software internals.

## Product Purpose

Make rule-based proxy routing understandable and controllable. Users create ordered rules, route them into named policy groups, and see which node a real connection ultimately used and why.

## Positioning

The product treats `rule → policy group → node` as the primary visible object and explains actual routing decisions. Competing clients usually expose configuration files, providers, modes, and runtime controls without making that relationship immediately legible.

## Operating Context

The app is a long-running desktop utility used from a normal window and system tray. Users frequently switch a policy’s selected node, inspect current connections, reorder rules, diagnose a failed route, and enable or disable system proxy/TUN behavior. Mouse, keyboard, trackpad, window resizing, light/dark appearance, and high-density data lists are normal operating conditions.

## Capabilities and Constraints

- Ordered rule editor with explicit fallback behavior.
- Policy groups supporting manual selection and latency testing, with fallback and load balancing when the active kernel advertises those capabilities.
- Live connection list and authoritative route explanation from the active kernel's supported controller API.
- Subscription/node import, profile management, system proxy control, and capability-gated TUN controls.
- One shared product language across Windows/macOS/Linux, with platform-specific title bar, menu, keyboard shortcut, tray, and permission conventions.
- The product configuration is structured; raw YAML is an isolated advanced escape hatch rather than the main interface.
- The implementation framework is fixed: GPUI must not be replaced by another UI framework.

## Brand Commitments

`Manis` is the confirmed product and repository name, taken from the Latin genus name for Asian pangolins. The existing teal-and-copper Signal Patch Bay palette remains the confirmed visual baseline. The abstract Shanshui mark in `assets/brand/manis-mark.svg` is the compact product mark; the detailed companion artwork is reserved for large-format documentation and release material.

## Evidence on Hand

The GPUI interface, adaptive layout, tray presence, macOS package, native review screenshots, and source brand artwork are implemented. Production user research, a signed release process, and broad Windows/Linux runtime validation do not yet exist. Example nodes, domains, latency values, and traffic data in screenshots remain illustrative rather than product claims.

## Product Principles

1. Explain the route before exposing engine terminology.
2. Make common policy changes one obvious action away.
3. Preserve expert power through progressive disclosure, not permanent visual complexity.
4. Treat compact windows as a real operating mode, not a scaled-down desktop screenshot.
5. Keep the GUI unprivileged and make risky network/system states explicit and recoverable.

## Accessibility & Inclusion

All core operations must work with keyboard and mouse; color cannot be the only carrier of routing or health state. Light, dark, high-contrast, reduced-motion, and text scaling behavior are first-class design requirements.
