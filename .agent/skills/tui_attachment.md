# TUI attachment

Use this guide when changing the Ratatui dashboard or its daemon attachment behavior.

## Architecture boundary

- `mind-tui` attaches to an existing daemon and never owns daemon lifecycle or simulation state.
- Authoritative telemetry, resources, progression, health, events, and diagnostic results come from `StatusSnapshot`; nodes, wires, and signal levels come from `CircuitGraphSnapshot` over `IpcClient`.
- Presentation state may include bounded caches of already-received events, graphs, and command results; observed delivery/tick rate; scroll position; help visibility; prompt/history state; and stale age.
- Do not read the daemon log file or recreate simulation or circuit values in widgets.

## Runtime behavior

- Keep one connection worker responsible for handshake, status subscription, commands, unsubscribe, and reconnect.
- Retry transport failures every second and retain the most recent snapshot with an obvious stale indicator. `r` requests an immediate retry.
- F1 selects Dashboard and F2 selects Assembly. `i` focuses the Assembly prompt; Enter executes through IPC, Tab completes, Up/Down navigate history, and Esc cancels prompt input before it detaches.
- `q` and Ctrl-C detach without stopping the daemon. Diagnostic pulses and DSL commands are sent through the connection worker.
- Render from pure application state. Keep terminal input handling and IPC work out of widget functions.
- Restore raw mode, cursor, and the main screen on success, error, or panic.

## Rendering and validation

- The default visual language is restrained cyan/amber sci-fi telemetry with semantic green and red states.
- Use a side-by-side body at 100x28 and above, a stacked body below that, and a resize message below 60x18.
- Honor `--ascii`, `--no-color`, `TERM=dumb`, and `NO_COLOR`; fallback output must not rely on color or non-ASCII decoration.
- Test reducers directly and use Ratatui `TestBackend` for wide, stacked, tiny, disconnected, stale, help, Unicode, and fallback frames.
- Finish with a live PTY session against the real daemon. Trigger a pulse, force a disconnect/reconnect, detach, and verify the daemon remains reachable and the terminal is restored.
