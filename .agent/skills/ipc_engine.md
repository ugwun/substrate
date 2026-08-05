# IPC engine

Use this guide when changing local client/server communication.

## Wire contract

- TCP loopback, default `127.0.0.1:7776`
- UTF-8 newline-delimited JSON-RPC 2.0
- Maximum frame size: 1 MiB
- Protocol version: 2, verified through `system.ping`
- Methods: `system.ping`, `mind.get_status`, `mind.subscribe_status`, `mind.unsubscribe_status`, `mind.trigger_diagnostic_pulse`, `mind.execute_dsl`, `mind.query_circuit_graph`, `daemon.shutdown`
- Notification: `mind.status_changed`

Requests and responses use standard JSON-RPC error codes for parse errors, invalid requests, unknown methods, and invalid parameters. DSL parse or game-rule failures are successful RPC responses with `ok: false` and stable diagnostics; malformed RPC parameters remain protocol errors. A malformed request must not crash or stop the daemon. Subscription state belongs to the connection and disappears on disconnect. Status updates are full snapshots; circuit graph snapshots are fetched separately and carry a revision for cache invalidation. Do not introduce deltas without a protocol-version decision.

## Change checklist

Define public wire structures in `mind-core` and preserve serialization tests. Exercise new behavior through `IpcClient` rather than ad hoc sockets. Integration-test fragmented or malformed traffic, disconnects, multiple clients, and shutdown ordering where relevant. Document compatibility breaks and increment the protocol version before merging one.
