# Agent Integration (CLI & MCP)

Manis ships a built-in headless CLI and a stdio Model Context Protocol (MCP) server inside the same `manis` / `manis-ui` binary.

- With **no arguments**, `manis` launches the GPUI desktop application and starts a same-user (`0o600`) local Unix domain socket at `<data_dir>/control.sock`.
- With a **subcommand**, `manis` talks directly to the running desktop app over `control.sock` (reflecting changes in the GUI immediately) and falls back cleanly to the local `config.toml` store when the desktop app is offline.
- **Secret redaction by default**: all CLI summaries, Control API responses, and MCP tools omit raw subscription URLs/tokens, node passwords/UUIDs, and Mihomo controller secrets.

## CLI Quick Reference

Add `--json` to any command for deterministic machine-readable output:

```bash
# Runtime status (app/core version, controller state, proxy/routing mode, traffic)
manis status --json

# Policy groups, strategies, current exits, latencies, and candidates
manis policies --json

# Live active connections (requires running desktop app)
manis connections --process agy --limit 20 --json

# Ordered manual rules, local rule groups, remote rule sources, and fallback
manis rules --json

# Deterministic route trace: matching rule -> policy group -> resolved exit
manis explain api.openai.com --process agy --port 443 --json

# Switch a manual policy group (persisted to config.toml + applied live)
manis select "🤖 AI Platforms" "233boy-tcp-66.98.118.202" --json

# Switch routing mode (rule | global | direct)
manis routing-mode rule --json

# Redacted configuration summary (no subscription tokens or node credentials)
manis config show --json

# Dry-run validate a JSONC configuration file or stdin
manis config validate ./my-config.jsonc --json
```

On macOS installed builds, the binary is located at:
`/Applications/Manis.app/Contents/MacOS/Manis`

## MCP Server (`manis mcp`)

Register `manis mcp` in any MCP-compatible AI client (Claude Code, Claude Desktop, Codex, Cursor, etc.):

```json
{
  "mcpServers": {
    "manis": {
      "command": "/Applications/Manis.app/Contents/MacOS/Manis",
      "args": ["mcp"]
    }
  }
}
```

### Exposed Tools

| Tool | Mode | Purpose |
|---|---|---|
| `manis_status` | Read-only | App/core version, controller status, proxy & routing mode, connection count |
| `manis_list_policies` | Read-only | Policy groups, current exits, saved selections, latencies, candidates |
| `manis_list_connections` | Read-only | Live Mihomo connections filtered by host, process, or policy chain |
| `manis_list_rules` | Read-only | Ordered manual rules, local groups, remote rule sources, fallback |
| `manis_explain_route` | Read-only | Deterministic `rule → policy group → exit` trace + matching live connections |
| `manis_select_policy` | Write | Validated manual policy switch (persisted + applied live) |
| `manis_set_routing_mode` | Write | Switch routing mode (`rule` / `global` / `direct`) |
| `manis_config_summary` | Read-only | Redacted configuration overview (no secrets) |
| `manis_validate_config` | Read-only | Dry-run JSONC config validation + summary |
