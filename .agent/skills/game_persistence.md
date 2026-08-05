# Game-state persistence

Use this guide when changing durable simulation state or save lifecycle.

## Save contract

- Store `game_state.json` beneath the platform-local Substrate data directory in a versioned JSON envelope.
- Persist only authoritative game state. Derived signal caches, dirty bookkeeping, diagnostics, events, client counts, runtime metadata, and wall-clock offline progress are transient.
- Write a temporary file in the destination directory, flush it, and atomically replace the destination. Successful mutating DSL commands save before becoming visible; a save failure rolls the command back.
- Also checkpoint dirty tick-generated state every five seconds and flush it on graceful shutdown. Failed background checkpoints keep the daemon alive in degraded health and retry later.

## Loading and evolution

- Missing save files create a new state. Malformed JSON, unsupported schema versions, unknown fields, and graph/resource invariant failures abort startup with a clear error; never silently discard an existing save.
- Validate power/resource bounds, progression gates, reserved identifiers, ports, unique target inputs, and acyclicity after deserialization.
- Add an explicit migration before changing the schema version or persisted representation. Preserve atomic replacement and test restart continuity, malformed saves, future versions, and invariant corruption.
