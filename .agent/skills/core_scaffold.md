# Core scaffold

Use this guide when changing workspace structure, configuration, or the engine lifecycle.

## Invariants

- `mind-daemon` is the source of truth; clients receive serialized snapshots.
- `mind-core` cannot depend on Ratatui, MCP presentation code, or a specific frontend.
- Advance simulation through the Tokio engine loop with skipped missed ticks. Do not calculate ticks while rendering or serving a status request.
- Use `PathBuf` and platform directories for every filesystem location.
- Keep config precedence as CLI, environment, file, defaults. Validate all effective values after merging.
- Bind only to loopback addresses. IPC defaults to port 7776 and MCP to port 7777; both must bind before daemon readiness.
- Runtime metadata is advisory. Confirm daemon identity with a versioned handshake before treating a recorded process as live.

## Extension points

Add durable gameplay state behind the core engine API, not in IPC or MCP handlers. Add commands as typed request/result structures in `mind-core`, mutate state inside the engine, and then expose them through daemon dispatch and transport adapters. Persistence must use atomic replacement or transactions and must not serialize transient client counts, diagnostic events, or process metadata as game state.

Update `AGENTS.md`, the README, tests, and this file whenever these invariants or operational commands change.
