---
name: tui-attachment
description: Maintain the attach-only Ratatui frontend and daemon connection behavior. Use when changing TUI workspaces, reducers, widgets, globe or panel rendering, key handling, typed actions, reconnect logic, stale presentation, terminal restoration, responsive layouts, or ASCII and monochrome fallbacks.
---

# TUI attachment

## Permanent boundaries

- Attach to an existing daemon; never own daemon lifecycle or simulate authoritative game state.
- Render authoritative snapshots and send typed actions through one connection worker responsible for handshake, subscriptions, requests, unsubscribe, and reconnect.
- Limit local state to view focus, selections, contextual UI state, bounded received caches, observed delivery rate, scroll state, help, stale age, and legacy prompt/history state only while Assembly remains.
- Never read daemon logs or reconstruct hidden resource, mission, world, circuit, research, or story values in widgets.
- Retry failures every second, retain the latest snapshot with an obvious stale indicator, make immediate retry available, and detach without stopping the daemon.
- Render from pure application state and restore raw mode, cursor, and screen on every exit, error, and panic path.

## Current and target interfaces

- Treat Dashboard, Assembly, F1/F2, the `i` prompt, and DSL submission as legacy prototype behavior while it remains implemented.
- Do not expand command-entry gameplay. Phase 0 replaces Assembly and typed awakening with graphical onboarding.
- Build the target interface around keyboard-first Planet, Operations, Industry, Research, and Archive destinations, with a rotating selectable globe and contextual actions rather than typed commands.
- Let arrows move focus or rotate the globe, Enter open or confirm, Esc move back, and Tab move between panels; keep frequent actions visible in context.
- Require clear confirmation for destructive actions and display automation scope, approvals, and audit results when those systems exist.

## Rendering and validation

- Use restrained cyan and amber styling with semantic states, never color alone.
- Preserve side-by-side wide layouts, stacked narrow layouts, a useful tiny-terminal message, Unicode defaults, and equivalent `--ascii`, `--no-color`, `TERM=dumb`, and `NO_COLOR` behavior.
- Test reducers and Ratatui `TestBackend` frames for wide, stacked, tiny, disconnected, stale, help, Unicode, fallback, globe selection, contextual actions, and confirmations as implemented.
- Finish with live PTY inspection against the real daemon. Trigger a diagnostic pulse, force disconnect/reconnect, detach, and verify daemon reachability and terminal restoration.
