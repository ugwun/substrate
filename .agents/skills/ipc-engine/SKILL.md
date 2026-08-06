---
name: ipc-engine
description: Maintain Substrate local IPC and shared client behavior. Use when changing JSON-RPC methods or DTOs, framing, protocol versions, subscriptions, notifications, connection lifecycle, daemon dispatch, error semantics, domain snapshots, or the reusable IpcClient.
---

# IPC engine

## Current wire contract

- Use loopback TCP at `127.0.0.1:7776` by default and newline-delimited UTF-8 JSON-RPC 2.0 with a 1 MiB frame limit.
- Keep protocol version 3 until a deliberate compatibility decision changes it; verify identity through `system.ping`.
- Preserve `system.ping`, `mind.get_status`, `mind.subscribe_status`, `mind.unsubscribe_status`, `mind.complete_onboarding`, `mind.trigger_diagnostic_pulse`, `daemon.shutdown`, and `mind.status_changed` until their owning phase changes them.
- Use standard protocol error codes for malformed JSON-RPC. Return well-formed game-rule rejections as typed operation results, never crashes.
- Keep subscription state connection-local, send full status snapshots, and use separate revisioned domain snapshots for large graphs or world models.

## Evolution rules

- Add typed planetary actions and DTOs in `mind-core`; do not add a generic text, natural-language, or untyped command endpoint.
- Keep authoritative rules in the engine and consume every new operation through the reusable `IpcClient`.
- Do not expose hidden world state, unreleased narrative truth, provider credentials, or transport-local state in client DTOs.
- Use revisions for cache invalidation. Do not add deltas or silently change field meaning without an explicit protocol-version decision.
- Document every breaking wire change and update all shared clients and adapters in the same change.

Test serialization, fragmented and oversized frames, malformed traffic, typed game-rule failures, reconnects, subscriptions, multiple clients, revision behavior, and shutdown ordering. Increment the protocol version before merging a breaking wire change.
