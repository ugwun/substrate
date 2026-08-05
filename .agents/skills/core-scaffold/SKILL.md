---
name: core-scaffold
description: Maintain Substrate workspace architecture, configuration, and engine lifecycle. Use when changing crate boundaries, workspace structure, platform paths, configuration precedence, listener startup and readiness, the simulation loop, runtime metadata, or shared subsystem extension points.
---

# Core scaffold

## Invariants

- Keep `mind-daemon` as the source of truth and serialize snapshots to clients.
- Keep `mind-core` independent of Ratatui, MCP presentation code, and specific frontends.
- Advance simulation through the Tokio engine loop with skipped missed ticks. Do not calculate ticks while rendering or serving a status request.
- Use `PathBuf` and platform directories for every filesystem location.
- Preserve configuration precedence as CLI, environment, file, then defaults. Validate all effective values after merging.
- Bind only to loopback addresses. Default IPC to port 7776 and MCP to port 7777; bind both before declaring daemon readiness.
- Treat runtime metadata as advisory. Confirm daemon identity with a versioned handshake before treating a recorded process as live.

## Extension points

Keep durable gameplay state behind the core engine API rather than in IPC or MCP handlers. Add commands as typed request and result structures in `mind-core`, mutate state transactionally inside the engine, and expose them through daemon dispatch and transport adapters.

Use a versioned JSON envelope and same-directory atomic replacement for persistence. Do not serialize transient signals, client counts, diagnostic events, or process metadata as game state. Reject corrupt, future-version, or invariant-breaking saves rather than silently resetting progression.

Update `AGENTS.md`, the README, tests, and this skill whenever these invariants or operational commands change.
