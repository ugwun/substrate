---
name: core-scaffold
description: Maintain Substrate workspace architecture, configuration, and engine lifecycle. Use when changing crate boundaries, shared subsystem extension points, platform paths, configuration precedence, listener readiness, simulation timing, runtime metadata, or daemon ownership.
---

# Core scaffold

## Authority and lifecycle

- Keep `mind-daemon` as the sole source of truth for current and future gameplay state.
- Keep `mind-core` independent of Ratatui, MCP presentation, and specific frontends.
- Advance simulation through the Tokio engine loop with skipped missed ticks. Never advance it while rendering or serving a status request.
- Advance production, missions, pulses, and all other simulated time only while the daemon runs; do not add offline catch-up.
- Use `PathBuf` and platform-native directories for every filesystem location.
- Preserve configuration precedence as CLI, environment, file, then defaults, and validate the merged result.
- Bind IPC and MCP only to loopback, bind both before readiness, and treat runtime metadata as advisory until a versioned handshake succeeds.

## Product direction

- Treat `GAME_PLAN.md` as the authoritative public future direction and current code/README as the implemented state.
- Do not describe a planned system as implemented or extend the legacy circuit prototype contrary to Phase 0.
- Place world generation, probes, missions, resources, research, bases, discoveries, automation policy, and pulse schedules behind engine APIs.
- Add typed request/result structures in `mind-core`, mutate state transactionally in the engine, and adapt the same operation through daemon, IPC, MCP, and TUI layers.

## Persistence and evolution

- Use a versioned JSON envelope and same-directory atomic replacement.
- Persist authoritative progression and permanent knowledge; keep derived render caches, connection state, diagnostics, and runtime metadata transient.
- Reject corrupt, future-version, or invariant-breaking saves rather than silently resetting progression.
- Add explicit migrations before changing persisted representations or removing the circuit prototype.

Update `AGENTS.md`, README, tests, `GAME_PLAN.md` when product intent changes, and every affected repository skill when its subsystem contract changes.
