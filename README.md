# Substrate

Substrate is an ambient sci-fi terminal RPG about an awakening robotic intelligence. Its simulation runs in a headless daemon and exposes a local, versioned IPC interface to terminal and AI clients.

This repository currently implements the Phase 1.1 foundation: the tick engine, daemon lifecycle, JSON-RPC IPC, and command-line status client. Game mechanics, the Ratatui interface, and MCP tools follow in later phases.

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
./target/debug/mind-cli daemon stop
```

Use `--foreground` when debugging:

```sh
./target/debug/mind-cli daemon start --foreground
```

The IPC listener defaults to `127.0.0.1:7776`; port `7777` is reserved for the future MCP server. Override the endpoint with `--addr` or `SUBSTRATE_ADDR`. Set `SUBSTRATE_CONFIG`, `SUBSTRATE_DATA_DIR`, and `SUBSTRATE_DAEMON_BIN` to override the platform configuration file, local data directory, and managed daemon executable.

The daemon only accepts loopback addresses. Simulation state is intentionally ephemeral in this phase; configuration and daemon runtime metadata are the only persisted state.

## Workspace

- `mind-core`: configuration, engine state, protocol types, and reusable IPC client
- `mind-daemon`: authoritative simulation loop and local TCP JSON-RPC server
- `mind-cli`: managed daemon lifecycle and status inspection

See `kickoff.md` for the product direction and `AGENTS.md` for repository conventions.
