# Substrate agent guide

## Current architecture

Substrate is a Rust 2021 workspace. `mind-daemon` is the sole authority for simulation state. `mind-cli`, `mind-tui`, and MCP adapters communicate with it over the shared IPC types and client in `mind-core`; frontend code must never own or advance game state.

The local IPC protocol is newline-delimited JSON-RPC 2.0 on loopback TCP, defaulting to `127.0.0.1:7776`. Protocol version 1 supports ping, status query/subscription, diagnostic pulses, unsubscribe, and graceful shutdown.

MCP protocol `2026-07-28` is exposed through Streamable HTTP at `http://127.0.0.1:7777/mcp` and through the `mind-mcp` stdio bridge. `mind-mcp` attaches to the existing daemon over IPC and never owns simulation state. The MCP surface currently contains `get_mind_status` and `trigger_diagnostic_pulse`.

`mind-tui` is an attach-only Ratatui frontend. Its local application state may cache received events, calculate client-observed tick rate, and remember view position, but authoritative telemetry and diagnostics always originate in daemon status snapshots. Detaching must never stop the daemon.

Configuration uses platform-native directories and `PathBuf`. CLI options override `SUBSTRATE_*` environment variables, which override `config.toml`, which overrides defaults. Simulation state is not durable yet.

## Required workflow

Before submitting a change, run:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Build all binaries with `cargo build --workspace` before exercising `mind-cli daemon start`.

Keep daemon logic independent of rendering and transport presentation. Extend protocol DTOs in `mind-core`, handle requests in the daemon, and consume them through the shared client. MCP handlers must adapt the same engine operations rather than implementing alternate state. Reject non-loopback endpoints unless the security model is deliberately redesigned.

When adding or materially changing a subsystem, update or add its corresponding document under `.agent/skills/`. Keep this guide and the README synchronized with commands and architecture.

## Operational commands

```sh
./target/debug/mind-cli daemon start
./target/debug/mind-cli daemon status --json
./target/debug/mind-cli daemon status --watch
./target/debug/mind-tui
./target/debug/mind-cli daemon stop
./target/debug/mind-mcp
```

Use `RUST_LOG=debug` for verbose diagnostics. Background logs and `daemon.json` are written beneath the platform-local Substrate data directory. A client may remove runtime metadata only after both the recorded endpoint and configured endpoint fail a valid protocol handshake. Never kill a PID solely because it appears in metadata.

Keep the MCP server current-only until product policy changes: advertise only `2026-07-28`, require stateless request metadata, validate Host and Origin, and keep stdout clean in stdio mode. Add tools only when their backing mechanic is real and tested through both direct-engine and IPC backends.

Linux is the current acceptance target. Preserve portable paths and `cfg`-gated process behavior so macOS and Windows validation can be added without changing public interfaces.

For TUI changes, exercise pure state transitions and Ratatui `TestBackend` frames before live PTY inspection. Preserve automatic reconnect, stale-state visibility, ASCII/monochrome fallbacks, and terminal restoration on every exit path.
