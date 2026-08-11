# Substrate

Substrate is an atmospheric terminal game about an awakening machine intelligence exploring an unknown planet, growing its physical reach, and searching for evidence of its origin. Its simulation runs in a headless daemon and exposes local, versioned IPC and MCP interfaces to terminal and AI clients.

The repository currently implements the Phase 0 foundation and the first playable Phase 1 survey slice: daemon-owned state, durable graphical onboarding, a lit orthographic regional globe with smooth idle rotation, fog and survey knowledge, one timed scout deployment, diagnostic telemetry, native MCP access, managed daemon lifecycle, and an attach-only Ratatui interface.

## Prerequisites

- Stable Rust with Cargo, rustfmt, and Clippy
- Linux for the currently supported and tested runtime target

The checked-in toolchain file installs the required Rust components when using rustup.

## Build and test

```sh
cargo build --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Build the complete workspace before using managed startup so that `mind-cli` can find the adjacent `mind-daemon` executable.

## Run

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

Both listeners only accept loopback addresses. Authoritative game state is atomically persisted as `game_state.json` beneath the platform-local Substrate data directory. Simulation time advances only while the daemon runs.

The current game uses save schema 3. Schema-1 and schema-2 saves are intentionally replaced with fresh graphical onboarding, a newly generated 24-region planet, and one starter scout; malformed and future-version saves still stop startup with an error.

## Terminal interface

`mind-tui` attaches to an already-running daemon and never starts or stops it. If the daemon is absent or disconnects, it retains the last authoritative snapshot as stale and retries every second. Quitting restores the original terminal and leaves the daemon running.

A fresh state begins with the graphical Coherence onboarding. Use Up/Down to select one of three signal bands, Left/Right to align its gap, and Enter to observe the result. Once all three gaps agree, Enter confirms awakening. There is no timer, score, or failure state.

Global controls:

- `q` or Ctrl-C: detach without stopping the daemon
- `r`: reconnect immediately while disconnected
- `?`: show help
- `Tab` or Shift-Tab: cycle Planet, Operations, and System workspaces

Planet controls:

- arrow keys: navigate the regional globe
- Enter: open region details and deploy the scout when available
- Esc: close region details

The selected region eases into view after navigation. When left idle, the globe rotates without changing the selected region or authoritative world state.

Operations uses Enter to recall an outbound, surveying, or holding scout. System retains `d` for a diagnostic pulse, arrow keys or `j`/`k` for event scrolling, Page Up/Page Down and Home/End for event navigation, and `f` for follow mode.

The starting scout can reach regions up to two graph hops from the core. Arrival permanently detects broad terrain; completing the focused survey records temperature and atmospheric severity, then leaves the scout holding the region under continuous observation. Recall returns the scout to the core without erasing surveyed knowledge. Mission time advances only while the daemon runs.

Unicode and color are enabled by default. Use `--ascii` or `--no-color` for explicit fallback; `TERM=dumb` enables both fallbacks automatically, and `NO_COLOR` disables color.

## IPC contract

IPC protocol version 4 uses newline-delimited JSON-RPC 2.0 over loopback TCP. In addition to lifecycle, status, onboarding, and diagnostics, it provides separate revisioned planet query/subscription methods and typed scout deployment and recall. Planet DTOs omit undiscovered terrain and conditions. `mind-core` provides the shared DTOs and reusable client.

## MCP access

Substrate supports MCP protocol `2026-07-28` and intentionally rejects legacy initialize/session clients. The current tools are:

- `get_mind_status`: operational state, onboarding status, latest diagnostic, and recent engine events
- `trigger_diagnostic_pulse`: record a baseline self-test against the running engine

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

The stdio bridge never starts its own engine. It exits with a stderr diagnostic when the daemon is unavailable and reserves stdout exclusively for MCP frames.

## Workspace

- `mind-core`: configuration, authoritative game state, persistence, protocol types, engine, and reusable IPC client
- `mind-daemon`: simulation loop, local JSON-RPC server, persistence checkpoints, and embedded MCP HTTP lifecycle
- `mind-cli`: managed daemon lifecycle and status inspection
- `mind-mcp`: shared MCP tools, embedded HTTP service, and stdio bridge
- `mind-tui`: attach-only graphical onboarding and Planet, Operations, and System workspaces

See [`GAME_PLAN.md`](GAME_PLAN.md) for the public game vision and future roadmap. [`AGENTS.md`](AGENTS.md) defines repository conventions.
