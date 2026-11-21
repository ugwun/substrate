---
name: game-persistence
description: Maintain Substrate durable game state and save lifecycle. Use when changing persisted fields, save envelopes or versions, atomic writes, transactional command or action commits, new-playthrough replacement, checkpointing, startup loading, migrations, permanent knowledge, or persistence health.
---

# Game-state persistence

## Save contract

- Store `game_state.json` beneath the platform-local Substrate data directory in a versioned JSON envelope.
- Persist only authoritative gameplay state. Keep render caches, derived signals, dirty bookkeeping, diagnostics, client connections, runtime metadata, provider credentials, heartbeat liveness, and wall-clock offline progress transient.
- Persist the implemented world seed and 200-region truth, regional and local-site knowledge, local energy, hazards and deposits, survey scout, configured extractor rig, regional or local extraction targets and accepted loadout, capacity, risk, timing, recovery and outcome, extraction and rebuild progress, raw and refined stores, relay construction and placement, research, active profile, survey queue, proven and active extraction route, foundation and solar-horizon milestones, focused local surveys, bases, enabled and suspended module modes, module and maintenance projects with accepted logistics support, power reserves, integrity and wear progress, visible activity acknowledgement, active missions, reports, discovered archive records, the active archive experiment, and its hidden comparison progress. Persist later acquired gameplay SKILLS, pulse schedules and acknowledgements, the bounded co-processor journal, and co-processor automation audit state when those systems exist.
- Preserve completed research, first-discovery records, acquired SKILLS, and archived story evidence through material rollback.
- Write a temporary file in the destination directory, flush it, and atomically replace the destination.
- Save every successful mutating engine operation before making it visible. Roll back when saving fails.
- Treat confirmed new-playthrough creation as a daemon-authoritative transaction: generate fresh state, preserve a monotonic revision for attached clients, atomically persist it, then replace in-memory state and clear prior runtime event history. Leave the current playthrough unchanged when persistence fails.
- Checkpoint dirty tick-generated state every five seconds, retry background failures with degraded health, and flush during graceful shutdown.

## Loading and evolution

- Create new state only when the save file is missing or the player submits the explicitly confirmed new-playthrough operation.
- Abort startup clearly for malformed JSON, unsupported future versions, unknown fields, or invariant failures; never silently discard a save.
- Validate all implemented resource, graph, world, mission, progression, archive, and automation invariants after deserialization.
- Add an explicit migration or deliberately documented reset before changing a schema version or representation. Schema 10 expands each schema-9 region from six established sites to a complete 4-by-4 local field, mapping the original six to fixed coordinates and preserving their identifiers, truth, knowledge, deposits, bases, projects, and active references. Schema 9 migrates schema-8 saves into the approved archive sequence without rewriting accepted extractor values; schema 8 migrated schema-7 extraction state to the balanced rig with fixed accepted mission values and preserved schema-6 default enabled module modes and accepted logistics support plus schema-5 deterministic local generation. Schema 5 intentionally replaced schema-1 through schema-4 alpha saves with fresh graphical onboarding and a newly generated 200-region planet.
- Do not let migration erase permanent knowledge or turn transient time away from the daemon into progress.

Test restart continuity, new-playthrough replacement and rollback, checkpoint recovery, malformed and future saves, every implemented invariant class, migrations from each supported version, and preservation of permanent knowledge across losses within a playthrough.
