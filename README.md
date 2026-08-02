# Substrate

Substrate is an ambient sci-fi terminal RPG about an awakening robotic intelligence. Its simulation runs in a headless daemon and exposes a local, versioned IPC interface to terminal and AI clients.

This repository currently implements the Phase 1.3 foundation: the tick engine, daemon lifecycle, JSON-RPC IPC, native MCP access, and an attachable Ratatui dashboard. Circuit game mechanics follow in Phase 1.4.

## Prerequisites

- Stable Rust with Cargo, rustfmt, and Clippy
- Linux for the currently supported and tested runtime target

The checked-in toolchain file installs the required Rust components when using rustup.

## Build and test

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Build the complete workspace before using managed startup so that `mind-cli` can find the adjacent `mind-daemon` executable.

## Run

Start the daemon in the background and inspect it:

```sh
cargo build --workspace
./target/debug/mind-cli daemon start
./target/debug/mind-cli daemon status
./target/debug/mind-cli daemon status --watch
./target/debug/mind-tui
./target/debug/mind-cli daemon stop
```

Use `--foreground` when debugging:

```sh
./target/debug/mind-cli daemon start --foreground
```

The IPC listener defaults to `127.0.0.1:7776`. The MCP Streamable HTTP endpoint defaults to `http://127.0.0.1:7777/mcp`. Override them with `--addr` / `SUBSTRATE_ADDR` and `--mcp-addr` / `SUBSTRATE_MCP_ADDR`. Set `SUBSTRATE_CONFIG`, `SUBSTRATE_DATA_DIR`, and `SUBSTRATE_DAEMON_BIN` to override the platform configuration file, local data directory, and managed daemon executable.

Both listeners only accept loopback addresses. Simulation state, diagnostic history, and engine events are intentionally ephemeral in this phase; configuration and daemon runtime metadata are the only persisted state.

## Terminal dashboard

`mind-tui` attaches to an already-running daemon and never starts or stops it. If the daemon is absent or disconnects, the dashboard retains the last snapshot as stale and retries every second. Quitting restores the original terminal and leaves the daemon running.

Controls:

- `q`, `Esc`, or Ctrl-C: detach
- `d`: trigger a diagnostic pulse
- `r`: reconnect immediately
- arrow keys or `j`/`k`: scroll events
- Page Up/Page Down and Home/End: navigate event history
- `f`: toggle follow mode
- `?`: show help

Unicode and color are enabled by default. Use `--ascii` or `--no-color` for explicit fallback; `TERM=dumb` enables both fallbacks automatically, and `NO_COLOR` disables color. The power panel intentionally remains offline until Phase 1.4 provides a real daemon-owned circuit power model.

## MCP access

Substrate supports MCP protocol `2026-07-28` and intentionally rejects legacy initialize/session clients. The initial tools are:

- `get_mind_status`: operational state, latest diagnostic, and recent engine events
- `trigger_diagnostic_pulse`: records a baseline self-test against the running engine

Use the embedded Streamable HTTP endpoint when your client supports remote MCP URLs:

```text
http://127.0.0.1:7777/mcp
```

For clients that launch local stdio servers, build the workspace, start the daemon separately, and configure the absolute path to `mind-mcp`:

```json
{
  "mcpServers": {
    "substrate": {
      "command": "/absolute/path/to/substrate/target/debug/mind-mcp"
    }
  }
}
```

The stdio bridge never starts its own engine. It exits with a stderr diagnostic when the daemon is unavailable, and reserves stdout exclusively for MCP frames.

## Workspace

- `mind-core`: configuration, engine state, protocol types, and reusable IPC client
- `mind-daemon`: authoritative simulation loop and local TCP JSON-RPC server
- `mind-cli`: managed daemon lifecycle and status inspection
- `mind-mcp`: shared MCP tools, embedded HTTP service, and stdio bridge
- `mind-tui`: attachable Ratatui telemetry and diagnostic dashboard

See `kickoff.md` for the product direction and `AGENTS.md` for repository conventions.
