---
name: ipc-engine
description: Maintain Substrate local IPC and shared client behavior. Use when changing JSON-RPC methods or DTOs, framing, protocol versions, subscriptions, notifications, connection lifecycle, daemon dispatch, error semantics, domain snapshots, or the reusable IpcClient.
---

# IPC engine

## Current wire contract

- Use loopback TCP at `127.0.0.1:7776` by default and newline-delimited UTF-8 JSON-RPC 2.0 with a 1 MiB frame limit.
- Keep protocol version 5 until a deliberate compatibility decision changes it; verify identity through `system.ping`.
- Preserve `system.ping`, status query/subscription, planet query/subscription, graphical onboarding completion, typed scout deployment and recall, typed extraction deployment and relay construction, diagnostic pulse, graceful shutdown, and their unsubscribe and notification methods until their owning phase changes them.
- Use standard protocol error codes for malformed JSON-RPC. Return well-formed game-rule rejections as typed operation results, never crashes.
- Keep subscription state connection-local. Send full status snapshots and separate full revisioned planet snapshots; queue notifications encountered while a shared client awaits an action response.

## Evolution rules

- Add typed planetary actions and DTOs in `mind-core`; the implemented actions are `mind.deploy_scout`, `mind.recall_scout`, `mind.deploy_extractor`, and `mind.construct_relay`. Do not add a generic text, natural-language, or untyped command endpoint.
- Keep authoritative rules in the engine and consume every new operation through the reusable `IpcClient`.
- Do not expose hidden world state, unreleased narrative truth, provider credentials, or transport-local state in client DTOs.
- Use revisions for cache invalidation. Do not add deltas or silently change field meaning without an explicit protocol-version decision.
- Document every breaking wire change and update all shared clients and adapters in the same change.

Test serialization, fragmented and oversized frames, malformed traffic, typed game-rule failures, reconnects, subscriptions, multiple clients, revision behavior, and shutdown ordering. Increment the protocol version before merging a breaking wire change.
