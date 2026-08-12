# Substrate agent guide

## Current architecture

Substrate is a Rust 2021 workspace. `mind-daemon` is the sole authority for simulation state. `mind-cli`, `mind-tui`, and MCP adapters communicate with it over the shared IPC types and client in `mind-core`; frontend code must never own or advance game state.

The local IPC protocol is newline-delimited JSON-RPC 2.0 on loopback TCP, defaulting to `127.0.0.1:7776`. Protocol version 5 supports ping, status and planet query/subscription, graphical onboarding completion, typed scout deployment and recall, extraction deployment, relay construction, diagnostic pulses, unsubscribe, and graceful shutdown. Planetary state uses a separate revisioned snapshot and never exposes undiscovered region truth or accepted random outcomes.

MCP protocol `2026-07-28` is exposed through Streamable HTTP at `http://127.0.0.1:7777/mcp` and through the `mind-mcp` stdio bridge. `mind-mcp` attaches to the existing daemon over IPC and never owns simulation state. The current MCP surface contains `get_mind_status` and `trigger_diagnostic_pulse`.

`mind-tui` is an attach-only Ratatui frontend with a Solarized-derived Substrate Dark visual system, graphical Coherence onboarding, and Planet, Overview, and System workspaces. Planet is the default post-onboarding view and the sole regional command surface: it pairs the rotating 24-region orthographic globe with an always-visible inspector that prioritizes complete local operation progress before discovered facts, local presence, contextual actions, and block reasons. Its header keeps gathered and refined stores plus fabrication status visible. The globe begins idle rotation after 30 seconds without input. Overview provides the deeper planet-wide material, processing, fleet, network, active-operation, construction, and report breakdown; selecting an entry returns to its region on Planet without dispatching an action. System retains daemon telemetry and diagnostics. A daemon-owned objective appears throughout play. Local application state may cache received snapshots and events and remember selections, workspace, confirmations, alignment, inspector and event scroll, and transient camera-animation state, but authoritative progression always originates in daemon snapshots. Detaching must never stop the daemon.

Configuration uses platform-native directories and `PathBuf`. CLI options override `SUBSTRATE_*` environment variables, which override `config.toml`, which overrides defaults. Versioned game state is atomically persisted beneath the platform-local data directory; it advances only while the daemon is running. Save schema 4 migrates schema-3 regional saves into the first expansion economy and intentionally resets schema-1 and schema-2 saves into fresh onboarding.

## Product direction

`GAME_PLAN.md` is the authoritative public product direction. Keep current architecture descriptions accurate and distinguish the implemented Phase 0 foundation from planned planetary systems.

Phase 0 retired the circuit prototype, typed awakening, Assembly prompt, and related IPC and MCP surfaces. Do not reintroduce generic command-entry gameplay. The first playable alpha implements the Phase 1 survey loop and a focused Phase 2 expansion slice: hidden deposits, risky extraction with permanent probe and cargo loss, guaranteed core rebuilding, automatic refining, one constructed relay, extended survey reach, and a persistent completion milestone. The protected survey scout remains failure-free; broader logistics, loadouts, energy, research, bases, and narrative discoveries remain planned.

The target game is a keyboard-first graphical TUI centered on a rotating regional globe, probes, geological resources, energy, logistics, research, bases, permanent discoveries, and optional research-gated external LLM assistance. Typed command entry must not be required for player progression. All world simulation, risk, progression, story eligibility, automation policy, and pulse timing remain daemon-owned and advance only while the daemon runs.

## Narrative secrecy

This repository is public. Never commit unreleased story resolution, identity answers, private narrative paths, suggestive identifiers, encoded spoilers, or unused future content. Implement only creator-approved player-facing breadcrumbs, use neutral public names, and follow `.agents/skills/narrative-delivery/SKILL.md` for every story or archive change. Private narrative guidance must remain outside the repository and must not be named or linked from public files.

Expose only discovered, player-visible knowledge through IPC, MCP, exported gameplay SKILLS, tests, and diagnostics. External co-processor reflections are non-canonical and must never establish lore or gate progression.

## Maintaining agent guidance

Treat repository guidance as part of the implementation, not optional follow-up documentation. At the start of every substantive task, inspect `AGENTS.md` and all matching skills. Before finishing, evaluate whether the change alters architecture, authority, invariants, workflows, public interfaces, operational commands, security assumptions, product direction, or reusable implementation knowledge.

Create a new repository skill when a change introduces a distinct subsystem or a reusable, non-obvious workflow that is not clearly owned by an existing skill. Update the owning skill in the same change whenever its behavior, invariants, APIs, persisted state, commands, failure modes, or required tests change. Delete a skill only after its subsystem, public surface, migration obligations, and maintenance workflow are completely removed. Do not create micro-skills for isolated implementation details that belong in an existing owner.

Store each skill at `.agents/skills/<hyphenated-name>/SKILL.md`. Give it YAML frontmatter containing only `name` and a comprehensive trigger-oriented `description`; write the body as concise imperative guidance and keep detailed optional material in directly linked `references/`, `scripts/`, or `assets/` only when it earns its context cost. Create or refresh `agents/openai.yaml` so its display name, short description, and `$skill-name` default prompt match the skill. Use the available `skill-creator` initialization and generation tools for new packages, and run its validator against every added or modified skill.

Update `AGENTS.md` in the same change whenever the current architecture summary, authority boundaries, supported protocols or tools, required workflow, operational commands, acceptance target, security policy, product-direction transition, narrative policy, or skill-routing guidance changes. Remove stale instructions rather than leaving current and obsolete contracts side by side. Keep README descriptions synchronized with implemented behavior and `GAME_PLAN.md` synchronized with deliberate product-policy changes.

Agents must perform these guidance updates proactively without waiting for a separate user request. Repository skills and agent metadata must never contain private narrative sources, hidden canon, provider credentials, machine-specific private paths, or other information forbidden by the narrative and security policies above.

## Required workflow

Before submitting a change, run:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Build all binaries with `cargo build --workspace` before exercising `mind-cli daemon start`.

Keep daemon logic independent of rendering and transport presentation. Extend protocol DTOs in `mind-core`, handle requests in the daemon, and consume them through the shared client. MCP handlers must adapt the same engine operations rather than implementing alternate state. Reject non-loopback endpoints unless the security model is deliberately redesigned.

Follow the maintenance rules above and each matching repository skill. Use `planetary-gameplay` for the future main loop, `narrative-delivery` for public story work, and `external-coprocessor` for research-gated LLM gameplay.

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

Keep the MCP server current-only until product policy changes: advertise only `2026-07-28`, require stateless request metadata, validate Host and Origin, and keep stdout clean in stdio mode. Every tool must adapt the same engine operation used by IPC clients and be tested through both direct-engine and IPC backends.

Linux is the current acceptance target. Preserve portable paths and `cfg`-gated process behavior so macOS and Windows validation can be added without changing public interfaces.

For TUI changes, exercise pure state transitions and Ratatui `TestBackend` frames before live PTY inspection. Preserve automatic reconnect, stale-state visibility, ASCII/monochrome fallbacks, and terminal restoration on every exit path.
