---
name: tui-attachment
description: Maintain the attach-only Ratatui frontend and daemon connection behavior. Use when changing TUI workspaces, reducers, widgets, globe or panel rendering, key handling, typed actions, reconnect logic, stale presentation, terminal restoration, responsive layouts, or ASCII and monochrome fallbacks.
---

# TUI attachment

## Permanent boundaries

- Attach to an existing daemon; never own daemon lifecycle or simulate authoritative game state.
- Render authoritative snapshots and send typed actions through one connection worker responsible for handshake, subscriptions, requests, unsubscribe, and reconnect.
- Limit local state to view focus, selections, onboarding alignment proposals, contextual UI state, bounded received caches, observed delivery rate, scroll state, help, and stale age.
- Never read daemon logs or reconstruct hidden resource, mission, world, research, or story values in widgets.
- Retry failures every second, retain the latest snapshot with an obvious stale indicator, make immediate retry available, and detach without stopping the daemon.
- Render from pure application state and restore raw mode, cursor, and screen on every exit, error, and panic path.

## Current and target interfaces

- Keep the Stage 0 Coherence onboarding graphical, untimed, and failure-free; persist only daemon-validated completion, not local band positions.
- Do not add command-entry gameplay.
- Keep Planet as the default workspace and sole regional command surface. Pair the globe with a persistent selected-region inspector that puts complete local operations first: show identifiers, phase, progress, remaining time, published risk, capacity, local construction or core processing, latest local report, discovered facts, truthful presence and mission relationships, every regional action, and an individual reason for each unavailable action.
- Keep gathered and refined stores plus fabrication status visible on Planet even while the inspector scrolls. Keep Overview deeper, aggregate, and non-commanding: show per-material stores and requirements, processing, fleet, network, active operations with risk and capacity, construction, and reports; let selection return to the associated Planet region without dispatching gameplay. Keep System as the retained telemetry and diagnostics view, and build future Research and Archive destinations without displacing the globe or reintroducing typed commands.
- Display the daemon-owned objective in every gameplay workspace. Show exact published risk tiers, and confirm risky extraction and material-consuming construction inside the inspector while leaving the globe visible.
- Let arrows select regions on Planet and entries on Overview, Enter invoke the contextual primary action or inspect an Overview entry, Esc cancel confirmations, and Tab move between workspaces; keep frequent actions visible in context.
- Require clear confirmation for destructive actions and display automation scope, approvals, and audit results when those systems exist.

## Rendering and validation

- Use the centralized Substrate Dark theme instead of widget-local color literals. Keep its Solarized-derived deep canvas, layered panel and raised surfaces, warm text hierarchy, restrained cyan focus and communication, amber actions, and distinct success, warning, and failure roles.
- Reserve bright borders for the active surface and amber borders for modal or warning surfaces. Keep inactive borders and secondary text quiet; express every state through text, symbols, borders, or modifiers as well as color.
- Fill the full terminal and panel surfaces in color mode. Never force foreground or background colors in `--no-color`, `NO_COLOR`, or `TERM=dumb` mode; preserve hierarchy there with ASCII-safe symbols and modifiers.
- Render the regional world through a true orthographic near-hemisphere projection. Use half-block foreground/background colors for square Unicode subpixels, continuous lighting and limb depth, subtle projected region boundaries, and overlays for selection, scout position, and active paths. Never sample undiscovered world truth to add visual texture.
- Keep yaw, pitch, idle rotation, easing, and animation phase transient and client-local. Begin globe rotation after 30 seconds without keyboard input, stop it on the next input, pause it outside Planet and during help or confirmation, and never change selection or advance authoritative missions.
- Preserve side-by-side wide layouts, stacked narrow layouts, a useful tiny-terminal message, Unicode defaults, and equivalent `--ascii`, `--no-color`, `TERM=dumb`, and `NO_COLOR` behavior.
- Test palette semantics and essential contrast, then exercise reducers and Ratatui `TestBackend` frames for wide, medium-footer, persistent stores, complete local operation details, stacked and scrolled inspectors, tiny, disconnected, stale, help, Unicode, fallback, globe selection, the 30-second idle threshold and interruption, workspace navigation, deep Overview details, selection and region jumps, local presence, scout deployment and recall, extraction, rebuilding, refining, relay construction, objectives, contextual actions, and confirmations.
- Finish with live PTY inspection against the real daemon. Trigger a diagnostic pulse, force disconnect/reconnect, detach, and verify daemon reachability and terminal restoration.
