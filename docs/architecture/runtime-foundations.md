# Runtime foundations

GPUI and gpui-component remain Manis's UI stack. This change standardizes infrastructure, not the
product model or the UI framework. Library popularity alone is not a migration criterion: prefer
maintained implementations that fit our execution model, platform requirements, and trust boundary.

## Controller HTTP: ureq, with a restricted socket adapter

Both ordinary controller calls and long-lived connection/log streams use ureq's HTTP implementation.
Manis already uses ureq for downloads; reusing it avoids introducing an additional HTTP stack or
requiring a Tokio runtime in GPUI background workers. The domain-facing transport trait remains
mockable, and the `StdHttpTransport` name is retained for source compatibility.

Manis still owns the security policy: validate paths/authentication before connecting, allow only
loopback TCP or an absolute non-symlink Unix socket, and omit bearer authentication on the Unix
socket. An agent receives exactly one already-open socket. It has no default connector fallback,
DNS lookup, environment proxy, redirect following, or connection reuse. Non-success responses use
canonical status descriptions, not server-supplied reason phrases or body previews.

The small adapter in `manis-mihomo/src/http_socket.rs` does socket I/O and cancellation, not HTTP
parsing. Ordinary bodies are bounded; stream JSON frames are separately bounded. Response headers
have a deadline, while an established idle stream can wait until cancellation. Cancellation polling
stays inside the read operation so partially decoded headers/chunks survive idle periods. HTTP body
framing, including Content-Length termination and chunk extensions, belongs to ureq. ureq does not
expose trailers here; they are discarded with the non-reusable connection, not parsed by Manis.

Tradeoff: ureq's [custom transport API](https://docs.rs/ureq/latest/ureq/unversioned/transport/index.html)
is explicitly outside its semver contract. Keep ureq exactly pinned, isolate that API in one module,
and run the TCP/Unix, malformed-response, redirect, fragmentation, deadline, and cancellation tests
before changing the pin. Revisit reqwest/hyper if the application's networking model becomes
Tokio-native; adopting another runtime solely for these local calls is not currently justified.

## Configuration: Serde wire trees

The typed `Profile` remains the source of truth and validation boundary. Private render modules map
it into ordered, typed YAML values, then encode with serde-saphyr for Mihomo.
The `preserve_order` feature preserves field insertion order; rule/group/provider sequence order is
unchanged. No hand-written JSON/YAML escaping, commas, or indentation remain in these renderers.

[serde-saphyr](https://docs.rs/serde-saphyr/0.0.29/serde_saphyr/) already exists in the GPUI dependency
graph and provides Serde serialization and explicit quoting. It is a fit-based choice, not a claim
that it is the most widely used YAML crate. The small `QuotedYaml` Serde adapter requests
double-quoted scalar values; the library performs all escaping and layout. Empty collections now
serialize as `[]`/`{}` rather than ambiguous YAML nulls. JSON whitespace and safe YAML key quoting
may change; tests should compare parsed data for semantics, and only test formatting intentionally.

Wire trees and serializer errors can contain secrets. They stay private, are never logged, and
serialization failures become fixed, redacted errors. SecretUrl does not gain a general Serialize
implementation. Private atomic writes, profile validation, unsupported-kernel rejection, rule
ordering, Linux strict-route, fake-IP persistence, and existing DNS choices remain intact.

## Diagnostics: tracing events and JSON Lines

The existing event/operation API emits structured tracing events. A dedicated
[tracing-subscriber layer](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/layer/index.html)
captures only the `manis::events` target and known typed fields. Its private dispatch does not replace
GPUI's global subscriber or collect third-party request logs. Sanitize before emission and again at
the sink; ignore arbitrary Debug fields instead of stringifying possibly secret values.

The UI ring and file receive the same entry, with operation IDs and sequence numbers assigned in
write order. New `logs/manis-events.log` records are JSON Lines; old tab-separated records are still
readable. Restored records are sanitized, malformed records are skipped, and history reads are
bounded. `MANIS_UI_TRACE` still mirrors events to stderr. Raw core logs remain a separate source
that may contain sensitive data.

The small synchronous file writer deliberately retains the existing size-based rotation (4 MiB,
one previous file), Unix directory/file permissions (0700/0600), and immediately visible writes.
tracing-appender's calendar-based rotation/nonblocking queue is not substituted without deciding
retention, dropped-event, and shutdown-flush semantics. File I/O failure does not prevent the UI
ring from receiving events. This is application storage policy, not a new logging framework.

## Deliberately separate work

Do not rewrite the privileged helper or TUN/DNS state machine as a side effect of library adoption.
Existing helper authorization, controller ownership, and DNS restoration remain unchanged. Further
lifecycle changes need their own Linux test matrix: crash/restart, suspend/resume, network changes,
external DNS changes, and helper/app shutdown ordering. The foundation tests do not substitute for
those real-system recovery tests.
