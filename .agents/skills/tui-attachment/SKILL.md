---
name: tui-attachment
description: Maintain the attach-only Ratatui frontend and its daemon connection behavior. Use when changing TUI workspaces, reducers, widgets, key handling, IPC connection or reconnect logic, stale-state presentation, terminal restoration, responsive layouts, or ASCII and monochrome fallbacks.
---

# TUI attachment

## Architecture boundary

- Attach `mind-tui` to an existing daemon; never let it own daemon lifecycle or simulation state.
- Read authoritative telemetry, resources, progression, health, events, and diagnostics from `StatusSnapshot`.
- Read nodes, wires, and signal levels from `CircuitGraphSnapshot` through `IpcClient`.
- Limit presentation state to bounded caches of received events, graphs, and command results; observed delivery or tick rate; scroll position; help visibility; prompt and history state; and stale age.
- Do not read the daemon log file or recreate simulation or circuit values in widgets.

## Runtime behavior

- Keep one connection worker responsible for handshake, status subscription, commands, unsubscribe, and reconnect.
- Retry transport failures every second and retain the latest snapshot with an obvious stale indicator. Make `r` request an immediate retry.
- Make F1 select Dashboard and F2 select Assembly.
- Make `i` focus the Assembly prompt; Enter execute through IPC; Tab complete; Up and Down navigate history; and Esc cancel prompt input before detaching.
- Make `q` and Ctrl-C detach without stopping the daemon.
- Send diagnostic pulses and DSL commands through the connection worker.
- Render from pure application state. Keep terminal input handling and IPC work out of widget functions.
- Restore raw mode, cursor, and the main screen on success, error, or panic.

## Rendering and validation

- Use restrained cyan and amber sci-fi telemetry with semantic green and red states by default.
- Use a side-by-side body at 100x28 and above, a stacked body below that, and a resize message below 60x18.
- Honor `--ascii`, `--no-color`, `TERM=dumb`, and `NO_COLOR`. Do not make fallback output depend on color or non-ASCII decoration.
- Test reducers directly and use Ratatui `TestBackend` for wide, stacked, tiny, disconnected, stale, help, Unicode, and fallback frames.
- Finish with a live PTY session against the real daemon. Trigger a pulse, force a disconnect and reconnect, detach, and verify that the daemon remains reachable and the terminal is restored.
