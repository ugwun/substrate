---
name: core-scaffold
description: Maintain Substrate workspace architecture, configuration, and engine lifecycle. Use when changing crate boundaries, shared subsystem extension points, platform paths, configuration precedence, listener readiness, simulation timing, runtime metadata, or daemon ownership.
---

# Core scaffold

## Authority and lifecycle

- Keep `mind-daemon` as the sole source of truth for current and future gameplay state.
- Keep `mind-core` independent of Ratatui, MCP presentation, and specific frontends.
- Advance simulation through the Tokio engine loop with skipped missed ticks. Never advance it while rendering or serving a status request.
- Advance production, missions, rebuilding, construction, pulses, and all other simulated time only while the daemon runs; do not add offline catch-up.
- Advance the implemented concurrent scout, focused local survey, regional or local extraction, refining, rebuild, relay construction, research, archive experiment, base project, local energy, environmental wear, workshop upkeep, maintenance, and automation timers from the configured engine period; checkpoint tick-generated progress every five seconds and flush it during graceful shutdown.
- Use `PathBuf` and platform-native directories for every filesystem location.
- Preserve configuration precedence as CLI, environment, file, then defaults, and validate the merged result.
- Bind IPC and MCP only to loopback, bind both before readiness, and treat runtime metadata as advisory until a versioned handshake succeeds.
- Keep `mind-tui` attach-only: it must not spawn or kill the daemon, but its session menu may submit typed graceful-shutdown and new-playthrough requests. Let `mind-cli play` provide the low-friction composition: reuse a reachable daemon or start it through managed startup, launch the adjacent TUI, and leave the daemon running only when the player chooses Detach or closes the TUI without requesting shutdown.
- Treat an older daemon on the configured endpoint as live rather than absent. Report the protocol conflict before spawning, and permit graceful cross-revision shutdown only after the endpoint answers the stable Substrate ping handshake.

## Product direction

- Treat `GAME_PLAN.md` as the authoritative public future direction and current code/README as the implemented state. Keep the current demo bounded at the read-only solar-horizon map until system-scale gameplay is deliberately designed and implemented.
- Do not describe a planned system as implemented or reintroduce the retired circuit prototype or typed command gameplay.
- Place world generation, probes, missions, resources, research, bases, discoveries, automation policy, and pulse schedules behind engine APIs.
- Add typed request/result structures in `mind-core`, mutate state transactionally in the engine, and adapt the same operation through daemon, IPC, MCP, and TUI layers.

## Persistence and evolution

- Use a versioned JSON envelope and same-directory atomic replacement.
- Persist authoritative progression and permanent knowledge; keep derived render caches, connection state, diagnostics, and runtime metadata transient.
- Reject corrupt, future-version, or invariant-breaking saves rather than silently resetting progression.
- Add explicit migrations or reset transitions before changing persisted representations.
- Keep deliberate new-playthrough replacement distinct from migration resets: the daemon must persist fresh authoritative state before publication and preserve the current playthrough on failure.

Update `AGENTS.md`, README, tests, `GAME_PLAN.md` when product intent changes, and every affected repository skill when its subsystem contract changes.
