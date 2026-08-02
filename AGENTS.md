# Substrate agent guide

## Current architecture

Substrate is a Rust 2021 workspace. `mind-daemon` is the sole authority for simulation state. `mind-cli` and future frontends communicate with it over the shared IPC types and client in `mind-core`; frontend code must never own or advance game state.

The local IPC protocol is newline-delimited JSON-RPC 2.0 on loopback TCP, defaulting to `127.0.0.1:7776`. Protocol version 1 supports ping, status query/subscription, unsubscribe, and graceful shutdown. Port `7777` is reserved for MCP.

Configuration uses platform-native directories and `PathBuf`. CLI options override `SUBSTRATE_*` environment variables, which override `config.toml`, which overrides defaults. Simulation state is not durable yet.

## Required workflow

Before submitting a change, run:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Build all binaries with `cargo build --workspace` before exercising `mind-cli daemon start`.

Keep daemon logic independent of rendering and transport presentation. Extend protocol DTOs in `mind-core`, handle requests in the daemon, and consume them through the shared client. Reject non-loopback endpoints unless the security model is deliberately redesigned.

When adding or materially changing a subsystem, update or add its corresponding document under `.agent/skills/`. Keep this guide and the README synchronized with commands and architecture.

## Operational commands

```sh
./target/debug/mind-cli daemon start
./target/debug/mind-cli daemon status --json
./target/debug/mind-cli daemon status --watch
./target/debug/mind-cli daemon stop
```

Use `RUST_LOG=debug` for verbose diagnostics. Background logs and `daemon.json` are written beneath the platform-local Substrate data directory. A client may remove runtime metadata only after both the recorded endpoint and configured endpoint fail a valid protocol handshake. Never kill a PID solely because it appears in metadata.

Linux is the current acceptance target. Preserve portable paths and `cfg`-gated process behavior so macOS and Windows validation can be added without changing public interfaces.
