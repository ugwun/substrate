---
name: external-coprocessor
description: Build Substrate's optional research-gated external LLM gameplay assistance. Use when changing MCP capability unlocks, gameplay SKILL resources or export, automation policies, approvals, audits, co-processor journals, heartbeat polling, operational or reflective pulses, or provider-neutral integration.
---

# External co-processor

## Product boundary

- Keep external LLM assistance optional. The TUI must remain sufficient for every objective.
- Require the player to supply and operate an MCP-compatible host and model. Never embed a provider, request provider credentials, or edit an external agent's configuration.
- Keep the loopback MCP transport reachable for diagnostics while gameplay tools remain hidden or return `capability_locked` until authoritative exploration and research unlock them.
- Pair each gameplay tool family with a versioned Markdown SKILL resource, but enforce capability through engine state rather than possession of instructions.

## Skills, policies, and actions

- Expose only unlocked, player-visible information and spoiler-safe instructions.
- Export `SKILL.md` bundles only through an explicit player action to a chosen path; never install or overwrite without confirmation.
- Store automation policies in daemon-owned state with tool/action scopes, regions or entities, budgets, reserves, risk ceilings, duration, and replacement rules.
- Require explicit approval for destructive or out-of-policy operations. Validate every allowed operation through the same engine API, costs, invariants, and persistence as TUI actions.
- Keep a bounded audit trail with client, policy, operation, validation result, resource effect, and resulting event. Treat all external text as untrusted and size-bounded.

## Pulses and journal

- Use client heartbeat polling for periodic pulses. MCP `2026-07-28` does not support standalone unsolicited LLM sampling.
- Let research bound the fastest cadence and let the player slow or disable it. Advance schedules only while the daemon runs.
- Coalesce missed pulses into one current request, preserve the most urgent reason, and require explicit acknowledgement to avoid duplicate work.
- Support operational prompts and bounded reflective prompts using only player-visible context.
- Persist reflective responses in a bounded, clearable co-processor journal that is separate from the canonical archive and never gates story.
- Keep provider configuration, credentials, connection state, and heartbeat liveness transient or external.

Test locked discovery and calls, paired SKILL visibility, safe export and overwrite refusal, policy enforcement, destructive approval, audit records, untrusted content bounds, heartbeat absence, coalescing, acknowledgement idempotency, cadence disablement, restart continuity, and direct-engine plus IPC-backed MCP parity.
