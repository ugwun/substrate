# TUI attachment

Use this guide when changing the Ratatui dashboard or its daemon attachment behavior.

## Architecture boundary

- `mind-tui` attaches to an existing daemon and never owns daemon lifecycle or simulation state.
- Authoritative telemetry, health, events, and diagnostic results come from `StatusSnapshot` over `IpcClient`.
- Presentation state may include a bounded cache of already-received events, observed delivery/tick rate, scroll position, help visibility, and stale age.
- Do not read the daemon log file or recreate simulation values in widgets. Power remains explicitly unavailable until a real engine mechanic exists.

## Runtime behavior

- Keep one connection worker responsible for handshake, status subscription, commands, unsubscribe, and reconnect.
- Retry transport failures every second and retain the most recent snapshot with an obvious stale indicator. `r` requests an immediate retry.
- `q`, `Esc`, and Ctrl-C detach without stopping the daemon. Diagnostic pulse is the only mutating dashboard command in Phase 1.3.
- Render from pure application state. Keep terminal input handling and IPC work out of widget functions.
- Restore raw mode, cursor, and the main screen on success, error, or panic.

## Rendering and validation

- The default visual language is restrained cyan/amber sci-fi telemetry with semantic green and red states.
- Use a side-by-side body at 100x28 and above, a stacked body below that, and a resize message below 60x18.
- Honor `--ascii`, `--no-color`, `TERM=dumb`, and `NO_COLOR`; fallback output must not rely on color or non-ASCII decoration.
- Test reducers directly and use Ratatui `TestBackend` for wide, stacked, tiny, disconnected, stale, help, Unicode, and fallback frames.
- Finish with a live PTY session against the real daemon. Trigger a pulse, force a disconnect/reconnect, detach, and verify the daemon remains reachable and the terminal is restored.
