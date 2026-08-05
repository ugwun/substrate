---
name: circuit-dsl
description: Maintain or retire Substrate's legacy circuit prototype. Use when changing the current DSL parser, diagnostics, gates, wires, COGNITIVE_SPARK objective, circuit persistence, or its TUI, IPC, and MCP adapters, including Phase 0 removal work.
---

# Legacy circuit DSL and solver

## Product boundary

- Treat circuits, the Assembly prompt, and COGNITIVE_SPARK as the current playable prototype, not the future product direction.
- Do not add new gates, objectives, DSL families, or a larger circuit canvas unless the user explicitly requests legacy work.
- Follow `GAME_PLAN.md`: Phase 0 removes this surface before planetary gameplay becomes the main loop.
- Preserve current behavior while the prototype remains runnable; do not partially remove a wire contract or persisted invariant.

## Current invariants

- Parse the line-oriented, case-insensitive DSL in `mind-core`; normalize identifiers to ASCII uppercase and retain byte spans for diagnostics.
- Support `HELP`, `STATUS`, `ACTIVATE LOGIC_CORE`, `CREATE`, `REMOVE`, `WIRE`, `UNWIRE`, `SET`, `INSPECT`, and `CLEAR CIRCUIT` until retirement.
- Parse and validate before mutation. Leave resources, progression, revisions, and persistence unchanged on rejection.
- Keep TUI and MCP behavior as adapters over `MindEngine::execute_dsl`; never duplicate semantics in a frontend or transport.
- Reserve `SENSOR_A`, `SENSOR_B`, and single-input `AWAKENING_BUS`; give AND, OR, and XOR inputs A/B and NOT input A.
- Keep circuits combinational and acyclic, enforce one source per target input, allow output fan-out, treat unwired inputs as LOW, and consume one bandwidth per wire.
- Cap power at 100.0 and generate 1.0 per daemon-running second. Charge 10.0 power and 2 memory for the core, 5.0 power and 1 memory per gate, and 1 bandwidth per wire.
- Keep AND, OR, and NOT initially available, XOR locked, and COGNITIVE_SPARK complete only after AWAKENING_BUS implements the full AND truth table and both sensors are currently HIGH.
- Keep completion permanent, unlock XOR afterward, and keep `CLEAR CIRCUIT` non-refundable while existing saves use these rules.

## Retirement workflow

- Remove the objective, parser, solver, graph DTOs, IPC methods, MCP tools, TUI Assembly workspace, tests, and documentation as one deliberate phase.
- Introduce an explicit save-schema migration or documented prototype-save transition; never silently reinterpret or discard an existing save.
- Replace command-based awakening with graphical onboarding before declaring the retirement playable.
- Remove this skill after no circuit code, protocol surface, persistence field, or user workflow remains.

Test legacy parser spans, truth tables, graph invariants, rejected-command atomicity, and revisions while maintained. For retirement, test save transition, removed protocol/tool discovery, graphical onboarding, and absence of stale circuit references.
