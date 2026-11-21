# Substrate agent guide

## Current architecture

Substrate is a Rust 2021 workspace. `mind-daemon` is the sole authority for simulation state. `mind-cli`, `mind-tui`, and MCP adapters communicate with it over the shared IPC types and client in `mind-core`; frontend code must never own or advance game state.

The local IPC protocol is newline-delimited JSON-RPC 2.0 on loopback TCP, defaulting to `127.0.0.1:7776`. Protocol version 13 supports ping, status and planet query/subscription, graphical onboarding completion, atomic new-playthrough creation, typed scout and regional or local extractor operations, extractor-rig configuration, focused local surveys, relay and foothold construction, base modules, module power modes and maintenance, research and profile selection, an archive comparison experiment, earned survey and extraction automation, visible-activity acknowledgement, diagnostic pulses, unsubscribe, and graceful shutdown. Planetary state uses a separate revisioned snapshot, publishes local field dimensions and coordinates, exposes only discovered regional and local knowledge, permanent player-visible archive records and the visible archive experiment, visible network condition and concluding-map availability, and never exposes undiscovered truth, hidden narrative eligibility, accepted random outcomes, or unresolved system details.

MCP protocol `2026-07-28` is exposed through Streamable HTTP at `http://127.0.0.1:7777/mcp` and through the `mind-mcp` stdio bridge. `mind-mcp` attaches to the existing daemon over IPC and never owns simulation state. The current MCP surface contains `get_mind_status` and `trigger_diagnostic_pulse`.

`mind-tui` is an attach-only Ratatui frontend with a Solarized-derived Substrate Dark visual system, a startup/session menu, graphical Coherence onboarding, and Planet, Overview, Research, Archive, System Map, and System workspaces. The menu separates resume, detach-with-simulation-running, graceful save-and-stop, and confirmed new-playthrough creation; lifecycle and reset effects are daemon-owned typed operations, and the TUI never starts, kills, or directly edits daemon state. Planet is the default post-onboarding view and sole regional command surface. Before the foundation milestone it pairs the rotating 200-region orthographic globe with an always-visible regional inspector; afterward it can replace the globe in place with the selected region's spatial 4-by-4 local field and a local inspector while preserving parent-region context. Arrow navigation follows field coordinates; unknown, frontier, observed, occupied, and active sectors remain distinct in Unicode, ASCII, color, and monochrome. Local presentation includes only discovered conditions, deposits, power, integrity, connectivity, modules, active work, contextual actions, and visible blockers. Its header keeps mapped coverage, gathered and refined stores, fabrication, active profile, and analysis visible. The globe begins idle rotation after 30 seconds without input; Shift-arrow navigation traverses its 10-by-20 grid quickly. Overview provides scrollable planet-wide material, fleet, power, node, construction, and horizon detail without commands; Research owns direction and profile choices; Archive permanently separates discovered observations from their current player-visible interpretations and starts the available powered-laboratory comparison with Enter; System retains daemon telemetry and diagnostics. System Map remains absent until the daemon-owned final demo milestone and its ordered observation spine are complete, then opens automatically with the local star, current world, unresolved outer returns, and no system-scale action surface. Reattachment presents only unreviewed player-visible activity, current stores and objective, and useful next destinations. Local application state may cache received snapshots and events and remember selections, region/local/archive view, workspace, confirmations, inspector and Overview scroll, return-summary visibility, session-menu state, and transient camera animation, but authoritative progression always originates in daemon snapshots. Detaching must never stop the daemon; only the menu's explicit save-and-stop request may do so.

Configuration uses platform-native directories and `PathBuf`. CLI options override `SUBSTRATE_*` environment variables, which override `config.toml`, which overrides defaults. Versioned game state is atomically persisted beneath the platform-local data directory; it advances only while the daemon is running. Save schema 10 expands schema-9 six-site regions into 4-by-4 local fields while preserving established identities, knowledge, deposits, bases, projects, and active references. Schema 9 migrates schema-8 saves into the approved ordered archive progression without rewriting accepted extractor loadouts; schema-7 extraction missions gain a balanced rig and fixed accepted capacity, cycle, and recovery values; schema-6 nodes and projects also gain enabled module modes and accepted logistics support, while schema-5 planets gain deterministic local terrain and network defaults. Schema-1 through schema-4 alpha saves still reset into fresh graphical onboarding and a newly generated 200-region planet. An explicitly confirmed new playthrough atomically persists a newly generated schema-10 state before replacing current progress; a failed write leaves the current playthrough intact. `mind-cli play` starts the daemon when absent and opens the TUI without stopping the daemon on detach. An older daemon on the configured endpoint is reported as a version conflict rather than treated as absent; the TUI's explicit save-and-stop action and `mind-cli daemon stop` both request graceful shutdown after a valid Substrate identity handshake.

## Product direction

`GAME_PLAN.md` is the authoritative public product direction. Keep current architecture descriptions accurate and distinguish the implemented Phase 0 foundation from planned planetary systems.

Phase 0 retired the circuit prototype, typed awakening, Assembly prompt, and related IPC and MCP surfaces. Do not reintroduce generic command-entry gameplay. The playable demo implements the Phase 1 survey loop, focused Phase 2 geology, Phase 3 research and earned automation, Phase 4's first planetary-network slice, and five creator-approved permanent observations from onboarding through the solar horizon. Proving automation now ends the tutorial-scale foundation chapter rather than the demo. Every region has sixteen deterministic sectors arranged in a 4-by-4 local field with class, energy, hazard, deposits, focused survey, and one-base placement tradeoffs. Regional surveys detect a five-sector landing cross; focused surveys resolve one sector and detect its orthogonal frontier, making local paths toward resources, power, or safer ground a player choice. The extractor can switch among balanced, cargo, and shielded rigs whose accepted capacity, risk, travel, cycle, and rebuild tradeoffs remain fixed for each mission. Footholds grow through power, communications, depot, workshop, laboratory, and observation modules; players may suspend and restore individual modules to shed load, preserve stored energy, or recover communications. Powered communications form three-hop geographic chains, connected depots expand concurrent construction capacity and accelerate nearby accepted projects, local support changes extraction capacity and risk, environmental strain can disconnect neglected nodes, workshops provide upkeep, and funded maintenance restores integrity. The archive sequence is earned through onboarding, focused site comparison, a powered-laboratory experiment after two research directions, and observations in three independent regions. The solar horizon additionally requires broad regional and local surveys, three connected nodes including one beyond the original relay envelope, distributed analysis and observation, and proven automation. It records the final approved observation, opens the read-only System Map, and permits no later system-scale progression. The protected survey scout remains failure-free; fully modular probe construction, transport routes, salvage, and additional creator-approved narrative discoveries remain future work.

The current 200-region topology is the globe-scale strategic layer. Each region owns a stable 4-by-4 local field for terrain class, energy, hazards, deposits, focused survey frontiers, extraction, and one permanent node, totaling 3,200 explorable subregions without turning globe navigation into tile-by-tile movement. Later expansion may deepen local transport, structures, routes, recovery, and approved discoveries without changing that hierarchy.

The target game is a keyboard-first graphical TUI centered on a rotating regional globe, probes, geological resources, energy, logistics, research, bases, permanent discoveries, and optional research-gated external LLM assistance. Its terminal-native visual direction should use maps, local scenes, tile-like terrain, compact sprites, diagrams, and restrained animation inspired by late-1980s and early-1990s pixel art wherever graphics improve atmosphere or comprehension, while preserving equivalent ASCII, monochrome, narrow, and low-capability presentations. Typed command entry must not be required for player progression. All world simulation, risk, progression, story eligibility, automation policy, and pulse timing remain daemon-owned and advance only while the daemon runs.

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

Cargo never prunes `target/`: every rebuild leaves stale binaries in `target/debug/deps` and caches in `target/debug/incremental`, and the directory reached 16 GB. Check it with `du -sh target` at the end of each substantive task and run `cargo clean` when it exceeds 5 GB; the next build recreates what it needs.

Keep daemon logic independent of rendering and transport presentation. Extend protocol DTOs in `mind-core`, handle requests in the daemon, and consume them through the shared client. MCP handlers must adapt the same engine operations rather than implementing alternate state. Reject non-loopback endpoints unless the security model is deliberately redesigned.

Follow the maintenance rules above and each matching repository skill. Use `planetary-gameplay` for the future main loop, `narrative-delivery` for public story work, and `external-coprocessor` for research-gated LLM gameplay.

## Operational commands

```sh
./target/debug/mind-cli play
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
