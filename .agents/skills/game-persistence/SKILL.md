---
name: game-persistence
description: Maintain Substrate durable game state and its save lifecycle. Use when changing persisted simulation fields, save envelopes or schema versions, atomic writes, command commit behavior, checkpointing, startup loading, migrations, or persistence health reporting.
---

# Game-state persistence

## Save contract

- Store `game_state.json` beneath the platform-local Substrate data directory in a versioned JSON envelope.
- Persist only authoritative game state. Keep derived signal caches, dirty bookkeeping, diagnostics, events, client counts, runtime metadata, and wall-clock offline progress transient.
- Write a temporary file in the destination directory, flush it, and atomically replace the destination.
- Save successful mutating DSL commands before making them visible. Roll the command back when saving fails.
- Checkpoint dirty tick-generated state every five seconds and flush it during graceful shutdown.
- Keep the daemon alive with degraded health after a background checkpoint failure and retry later.

## Loading and evolution

- Create new state only when the save file is missing.
- Abort startup with a clear error for malformed JSON, unsupported schema versions, unknown fields, or graph and resource invariant failures. Never silently discard an existing save.
- Validate power and resource bounds, progression gates, reserved identifiers, ports, unique target inputs, and acyclicity after deserialization.
- Add an explicit migration before changing the schema version or persisted representation.
- Preserve atomic replacement and test restart continuity, malformed saves, future versions, and invariant corruption.
