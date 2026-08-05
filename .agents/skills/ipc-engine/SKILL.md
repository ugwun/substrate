---
name: ipc-engine
description: Maintain Substrate local IPC and shared client behavior. Use when changing JSON-RPC methods or DTOs, framing, protocol versions, subscriptions, notifications, connection lifecycle, daemon dispatch, error semantics, or the reusable IpcClient.
---

# IPC engine

## Wire contract

- Use loopback TCP, defaulting to `127.0.0.1:7776`.
- Encode UTF-8 newline-delimited JSON-RPC 2.0 with a maximum frame size of 1 MiB.
- Keep protocol version 2 and verify it through `system.ping`.
- Support `system.ping`, `mind.get_status`, `mind.subscribe_status`, `mind.unsubscribe_status`, `mind.trigger_diagnostic_pulse`, `mind.execute_dsl`, `mind.query_circuit_graph`, and `daemon.shutdown`.
- Publish `mind.status_changed` notifications.

Use standard JSON-RPC error codes for parse errors, invalid requests, unknown methods, and invalid parameters. Return DSL parse or game-rule failures as successful RPC responses with `ok: false` and stable diagnostics; keep malformed RPC parameters as protocol errors. Never let a malformed request crash or stop the daemon.

Keep subscription state on its connection and discard it on disconnect. Send full status snapshots. Fetch circuit graph snapshots separately and include a revision for cache invalidation. Do not introduce deltas without a protocol-version decision.

## Change checklist

- Define public wire structures in `mind-core` and preserve serialization tests.
- Exercise new behavior through `IpcClient` rather than ad hoc sockets.
- Integration-test fragmented or malformed traffic, disconnects, multiple clients, and shutdown ordering where relevant.
- Document compatibility breaks and increment the protocol version before merging one.
