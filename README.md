# Substrate

Substrate is an atmospheric terminal game about an awakening machine intelligence exploring an unknown planet, growing its physical reach, and searching for evidence of its origin. Its simulation runs in a headless daemon and exposes local, versioned IPC and MCP interfaces to terminal and AI clients.

![Animated Substrate terminal gameplay showing the 200-region globe, a live scout mission, Overview, Research, and System workspaces](docs/assets/substrate-terminal-demo.gif)

The repository currently implements a first playable alpha: graphical onboarding; a lit, smoothly rotating 200-region globe; fog and surveys; variable geological deposits; concurrent survey and extraction probes; permanent extraction loss with guaranteed rebuilding; automatic refining; a range-extending relay; three research specializations; earned survey and extraction automation; and a concise return summary. All progression remains daemon-owned and is presented through the attach-only Substrate Dark TUI.

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
./target/debug/mind-cli play
```

`play` starts the daemon if necessary, opens the game, and leaves the simulation running
when you detach. The lower-level lifecycle commands remain available:

```sh
./target/debug/mind-cli daemon start
./target/debug/mind-cli daemon status
./target/debug/mind-cli daemon status --watch
./target/debug/mind-tui
./target/debug/mind-cli daemon stop
```

After rebuilding across an IPC protocol change, `play` reports when an older daemon
still owns the configured endpoint. Run `./target/debug/mind-cli daemon stop` once and
then retry `play`; shutdown remains compatible across protocol revisions after the
daemon answers Substrate's identity handshake.

Use `--foreground` when debugging:

```sh
./target/debug/mind-cli daemon start --foreground
```

The IPC listener defaults to `127.0.0.1:7776`. The MCP Streamable HTTP endpoint defaults to `http://127.0.0.1:7777/mcp`. Override them with `--addr` / `SUBSTRATE_ADDR` and `--mcp-addr` / `SUBSTRATE_MCP_ADDR`. Set `SUBSTRATE_CONFIG`, `SUBSTRATE_DATA_DIR`, `SUBSTRATE_DAEMON_BIN`, and `SUBSTRATE_TUI_BIN` to override the platform configuration file, local data directory, managed daemon executable, and game interface executable.

Both listeners only accept loopback addresses. Authoritative game state is atomically persisted as `game_state.json` beneath the platform-local Substrate data directory. Simulation time advances only while the daemon runs.

The current game uses save schema 5. Because the planet expanded from 24 to 200 regions, schema-1 through schema-4 alpha saves are deliberately replaced with fresh graphical onboarding and a new planet. Malformed and future-version saves still stop startup with an error.

## How to play

Substrate is designed to be learned from the interface. The objective at the top of
every workspace tells you what the Mind is currently trying to accomplish, the
selected-region inspector lists the actions available there, and unavailable actions
state their exact blocker. `Enter` performs the highlighted contextual action, while
the letter shortcuts shown in the inspector are useful once the basic loop is familiar.

### A first-session route

1. Complete Coherence by aligning the three signal gaps. There is no timer or failure
   state.
2. On **Planet**, move from the core to a reachable unknown region and deploy the survey
   scout with `Enter` or `s`.
3. Watch its journey and survey progress in that region's inspector. The scout holds at
   its destination when finished; select that region and press `r` to recall it before
   surveying somewhere else.
4. Survey several nearby regions. Each completed survey permanently records terrain,
   conditions, any deposit, and its extraction risk.
5. When a deposit appears, press `x` to review and confirm an extraction. Returned raw
   material is refined automatically at the core.
6. Gather all three material families. The first relay requires 8 structural,
   6 conductive, and 6 silicate units; build it with `b` in a suitable surveyed region
   to extend the network by two hops.
7. Survey beyond the original network range to open **Research**. Surveys supply analysis;
   each direction costs 3 analysis and takes 120 seconds of daemon time. Choose between
   Reach, Resilience, and Throughput rather than following a mandatory upgrade order.
8. Complete one research direction, activate its operational profile, and try the
   automation you have earned. `a` can queue eligible surveys, while repeat extraction
   becomes available only after you have completed that route manually once.

### Useful hints

- Leave the daemon running when you want active missions, refining, construction, or
  research to continue. Detaching with `q` is safe; stopping the daemon freezes time,
  and the game does not grant offline catch-up.
- The survey scout cannot be lost. Extraction publishes a 5%, 15%, or 30% loss risk and
  can permanently lose both probe and cargo, but the core always rebuilds a replacement
  so the save cannot become stuck.
- Knowledge is safer than machinery: completed surveys and research persist even when
  material operations go badly.
- Use **Planet** for every region-specific action. Use **Overview** to inspect global
  stores, fleet state, current work, routes, and bottlenecks; **Research** for projects
  and profiles; and **System** for connection and daemon diagnostics.
- Press Page Up/Page Down when the region inspector or Overview contains more detail
  than the current terminal can show. Shift-arrow keys cross the globe faster than
  ordinary arrows.
- Press `?` at any time for contextual controls. The game never requires typed commands
  for progression.

## Terminal interface

`mind-tui` attaches to an already-running daemon and never starts or stops it. If the daemon is absent or disconnects, it retains the last authoritative snapshot as stale and retries every second. Quitting restores the original terminal and leaves the daemon running.

A fresh state begins with the graphical Coherence onboarding. Use Up/Down to select one of three signal bands, Left/Right to align its gap, and Enter to observe the result. Once all three gaps agree, Enter confirms awakening. There is no timer, score, or failure state.

Global controls:

- `q` or Ctrl-C: detach without stopping the daemon
- `r`: reconnect immediately while disconnected
- `?`: show help
- `Tab` or Shift-Tab: cycle Planet, Overview, Research, and System workspaces

Planet controls:

- arrow keys: navigate one region on the 10-by-20 globe
- Shift-arrow keys: jump two latitude bands or five longitude sectors
- Enter: use the selected region's primary available action
- `s`: deploy the protected survey scout
- `x`: prepare a risky extraction from a surveyed deposit
- `b`: prepare relay construction at a surveyed site once materials are ready
- `r`: recall the scout when its active operation belongs to the selected region
- `a`: queue or clear the earned routine available in the selected region
- Page Up/Page Down: scroll the persistent region inspector
- Esc: cancel an extraction or construction confirmation

The selected region eases into view after navigation. After 30 seconds without keyboard input, the globe begins rotating without changing the selected region or authoritative world state; the next input immediately takes control.

Planet's always-visible inspector puts active work in the selected region first. It shows mission identifiers, phases, progress, remaining time, extraction exposure and capacity, local construction, core processing or rebuilding, queued survey position, extraction-route state, the latest local report, discovered facts, local structures and probes, available actions, and a separate explanation for each blocked action. A persistent header summarizes mapped coverage, gathered and refined stores, fabrication, active profile, and analysis. Risky extraction and material-consuming construction require confirmation while the globe remains visible. Overview provides deeper planet-wide detail without dispatching regional commands; Page Up and Page Down scroll its status panel. Research presents available directions, active work, permanent completions, and profiles. Reattaching after the daemon has continued running presents only unreviewed visible operations, current stores and objective, and practical next destinations. System retains daemon diagnostics and event navigation.

The starting scout can initially reach regions up to two graph hops from the core. Arrival permanently detects broad terrain; completing a focused survey records conditions, deposits, honest extraction risk, and one analysis unit. Deposits contain deterministic amounts from 8 to 24 units. The extraction probe carries four units and may be permanently lost with its cargo at the displayed 5%, 15%, or 30% risk; the core then rebuilds it in 60 seconds so progression cannot softlock. Returned material refines automatically. A relay costs 8 structural, 6 conductive, and 6 silicate units, takes 90 seconds to construct, and extends reach by two hops. Surveying beyond the original range opens research: Reach adds a network hop and faster surveys, Resilience lowers extraction risk by one tier, and Throughput adds two cargo units and faster refining. Completing any direction earns a bounded survey queue and automation for manually proven extraction routes. Mission, research, automation, and production time advance only while the daemon runs.

Unicode and the Substrate Dark truecolor theme are enabled by default. Its deep blue-black canvas, layered teal surfaces, restrained cyan focus, amber actions, and explicit semantic states remain consistent across onboarding and every workspace. Use `--ascii` or `--no-color` for explicit fallback; `TERM=dumb` enables both fallbacks automatically, and `NO_COLOR` disables color without removing the symbol, border, text, or modifier hierarchy.

## IPC contract

IPC protocol version 6 uses newline-delimited JSON-RPC 2.0 over loopback TCP. In addition to lifecycle, status, onboarding, and diagnostics, it provides separate revisioned planet query/subscription methods and typed scout, extraction, relay, research, profile, routine-automation, and visible-activity acknowledgement operations. Planet DTOs include their topology dimensions and omit undiscovered terrain, deposits, and accepted random outcomes. `mind-core` provides the shared DTOs and reusable client.

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
- `mind-cli`: one-command game launch, managed daemon lifecycle, and status inspection
- `mind-mcp`: shared MCP tools, embedded HTTP service, and stdio bridge
- `mind-tui`: attach-only graphical onboarding and Planet, Overview, Research, and System workspaces

See [`GAME_PLAN.md`](GAME_PLAN.md) for the public game vision and future roadmap. [`AGENTS.md`](AGENTS.md) defines repository conventions.
