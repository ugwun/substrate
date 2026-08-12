---
name: game-persistence
description: Maintain Substrate durable game state and save lifecycle. Use when changing persisted fields, save envelopes or versions, atomic writes, transactional command or action commits, checkpointing, startup loading, migrations, permanent knowledge, or persistence health.
---

# Game-state persistence

## Save contract

- Store `game_state.json` beneath the platform-local Substrate data directory in a versioned JSON envelope.
- Persist only authoritative gameplay state. Keep render caches, derived signals, dirty bookkeeping, diagnostics, client connections, runtime metadata, provider credentials, heartbeat liveness, and wall-clock offline progress transient.
- Persist the implemented world seed and 200-region truth, fog knowledge, survey scout, deposits, raw and refined stores, accepted extraction outcomes, extraction and rebuild progress, relay construction and placement, research, active profile, survey queue, proven and active extraction route, visible activity acknowledgement, alpha milestones, active missions, and reports. Persist later bases, story eligibility and archive records, acquired gameplay SKILLS, pulse schedules and acknowledgements, the bounded co-processor journal, and co-processor automation audit state when those systems exist.
- Preserve completed research, first-discovery records, acquired SKILLS, and archived story evidence through material rollback.
- Write a temporary file in the destination directory, flush it, and atomically replace the destination.
- Save every successful mutating engine operation before making it visible. Roll back when saving fails.
- Checkpoint dirty tick-generated state every five seconds, retry background failures with degraded health, and flush during graceful shutdown.

## Loading and evolution

- Create new state only when the save file is missing.
- Abort startup clearly for malformed JSON, unsupported future versions, unknown fields, or invariant failures; never silently discard a save.
- Validate all implemented resource, graph, world, mission, progression, archive, and automation invariants after deserialization.
- Add an explicit migration or deliberately documented reset before changing a schema version or representation. Schema 5 intentionally replaces schema-1 through schema-4 alpha saves with fresh graphical onboarding and a newly generated 200-region planet.
- Do not let migration erase permanent knowledge or turn transient time away from the daemon into progress.

Test restart continuity, transactional rollback, checkpoint recovery, malformed and future saves, every implemented invariant class, migrations from each supported version, and preservation of permanent knowledge across losses.
