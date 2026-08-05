# Substrate

Substrate is an ambient sci-fi terminal RPG about an awakening robotic intelligence. Its simulation runs in a headless daemon and exposes a local, versioned IPC interface to terminal and AI clients.

This repository currently implements the Phase 1.4 playable vertical slice: daemon-owned resource generation, durable progression, an acyclic logic-circuit sandbox, a command DSL, native MCP access, and an attachable Ratatui interface.

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

Both listeners only accept loopback addresses. Authoritative game state is atomically persisted as `game_state.json` beneath the platform-local Substrate data directory after commands, every five seconds while dirty, and during graceful shutdown. Time spent with the daemon stopped does not generate power. Diagnostic history, engine events, and client state remain ephemeral.

## Terminal interface

`mind-tui` attaches to an already-running daemon and never starts or stops it. If the daemon is absent or disconnects, the dashboard retains the last snapshot as stale and retries every second. Quitting restores the original terminal and leaves the daemon running.

Global controls:

- F1: dashboard workspace
- F2: circuit assembly workspace
- `q` outside the Assembly prompt, or Ctrl-C anywhere: detach
- `d`: trigger a diagnostic pulse
- `r`: reconnect immediately
- arrow keys or `j`/`k`: scroll events
- Page Up/Page Down and Home/End: navigate event history
- `f`: toggle follow mode
- `?`: show help

The Assembly prompt is focused when you press F2; after cancelling it with Esc, press `i` to focus it again. Enter submits, Tab completes keywords and known node identifiers, Up/Down recall history, and Esc cancels editing. Power accumulates at 1.0 per second to a cap of 100.0. After 10.0 power, this sequence activates the core, fabricates the first gate, and completes the awakening interlock after another 5.0 power is available:

```text
ACTIVATE LOGIC_CORE
CREATE AND INTERLOCK
WIRE SENSOR_A -> INTERLOCK.A
WIRE SENSOR_B -> INTERLOCK.B
WIRE INTERLOCK -> AWAKENING_BUS.IN
SET SENSOR_A HIGH
SET SENSOR_B HIGH
```

Use `HELP`, `STATUS`, `INSPECT [ID]`, `UNWIRE <TARGET>.<PORT>`, `REMOVE <ID>`, or `CLEAR CIRCUIT` for exploration and editing. `CLEAR CIRCUIT` preserves progression and does not refund spent power. Completing the AND truth-table objective permanently unlocks XOR fabrication.

Unicode and color are enabled by default. Use `--ascii` or `--no-color` for explicit fallback; `TERM=dumb` enables both fallbacks automatically, and `NO_COLOR` disables color.

## MCP access

Substrate supports MCP protocol `2026-07-28` and intentionally rejects legacy initialize/session clients. The tools are:

- `get_mind_status`: operational state, latest diagnostic, and recent engine events
- `trigger_diagnostic_pulse`: records a baseline self-test against the running engine
- `execute_dsl_command`: executes the same circuit DSL used by the TUI
- `query_circuit_graph`: returns the current typed circuit graph and signal levels

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

- `mind-core`: configuration, game engine, persistence, DSL, protocol types, and reusable IPC client
- `mind-daemon`: authoritative simulation loop and local TCP JSON-RPC server
- `mind-cli`: managed daemon lifecycle and status inspection
- `mind-mcp`: shared MCP tools, embedded HTTP service, and stdio bridge
- `mind-tui`: attachable Ratatui telemetry and diagnostic dashboard

See `kickoff.md` for the product direction and `AGENTS.md` for repository conventions.
