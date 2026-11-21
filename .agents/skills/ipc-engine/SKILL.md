---
name: ipc-engine
description: Maintain Substrate local IPC and shared client behavior. Use when changing JSON-RPC methods or DTOs, framing, protocol versions, subscriptions, notifications, connection lifecycle, daemon dispatch, error semantics, domain snapshots, or the reusable IpcClient.
---

# IPC engine

## Current wire contract

- Use loopback TCP at `127.0.0.1:7776` by default and newline-delimited UTF-8 JSON-RPC 2.0 with a 1 MiB frame limit.
- Keep protocol version 14 until a deliberate compatibility decision changes it; verify identity through `system.ping`.
- Keep identity probing and graceful shutdown usable across protocol revisions for lifecycle recovery. Do not treat a valid older Substrate ping as an absent daemon or clear its runtime metadata.
- Preserve `system.ping`, status query/subscription, planet query/subscription, graphical onboarding completion, atomic new-playthrough creation, typed scout, extractor-rig configuration, regional and local extractor, focused-site survey, relay, foothold, module construction and power-mode, maintenance, research, archive-experiment, profile, survey-queue, extraction-route, visible-activity acknowledgement, archive-presentation acknowledgement (`mind.acknowledge_archive_presentation`), diagnostic pulse, graceful shutdown, unsubscribe, and notification methods until their owning phase changes them.
- Use standard protocol error codes for malformed JSON-RPC. Return well-formed game-rule rejections as typed operation results, never crashes.
- Keep subscription state connection-local. Gate periodic planet broadcast checks with cheap `planet_revision()` comparison so idle daemons serialize zero snapshots, and push `planet.changed` immediately after mutating RPCs so client actions reflect without waiting for the 1 Hz broadcast tick. Initialize planet subscriptions at the current revision so a pre-subscription snapshot cannot arrive after a newer query or action. Queue notifications encountered while a shared client awaits an action response, and make reducers reject older revisions.

## Evolution rules

- Add typed lifecycle, persistence, and planetary actions and DTOs in `mind-core`; the implemented action families cover graceful shutdown, new-playthrough creation, scout deployment, regional or local extraction, focused local surveys, relay and base projects, module construction and load shedding, maintenance, research, the archive experiment, profiles, earned survey and extraction automation, and activity acknowledgement. Do not add a generic text, natural-language, or untyped command endpoint.
- Keep authoritative rules in the engine and consume every new operation through the reusable `IpcClient`.
- Publish the local field's row and column dimensions and each sector's coordinates so clients never hardcode the topology. Expose only discovered regional and local-sector knowledge, the daemon-derived five-chapter campaign snapshot and player-visible lead region, unpresented archive record IDs, the configured extractor rig and accepted visible mission statistics, visible bases, module modes, project capacity and accepted logistics support, power, integrity, connectivity, permanent discovered archive records, the currently visible archive experiment, public horizon requirements, and the player-visible availability of the concluding system map. Coordinates and stable neutral identifiers may describe an uncharted cell, but must not serialize its hidden class, conditions, resources, narrative eligibility, or accepted outcomes.
- Use revisions for cache invalidation. Do not add deltas or silently change field meaning without an explicit protocol-version decision.
- Document every breaking wire change and update all shared clients and adapters in the same change.

Test serialization, fragmented and oversized frames, malformed traffic, typed game-rule failures, reconnects, subscriptions, multiple clients, revision behavior, and shutdown ordering. Increment the protocol version before merging a breaking wire change.
