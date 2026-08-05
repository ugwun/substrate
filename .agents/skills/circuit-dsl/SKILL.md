---
name: circuit-dsl
description: Maintain Substrate circuit commands, solver rules, resources, and progression. Use when modifying DSL parsing or diagnostics, gates, wires, graph validation, circuit revisions, the COGNITIVE_SPARK objective, or circuit adapters in the TUI or MCP server.
---

# Circuit DSL and solver

## Authority and grammar

- Parse the line-oriented, case-insensitive DSL in `mind-core`; normalize identifiers to ASCII uppercase and retain byte spans for diagnostics.
- Support `HELP`, `STATUS`, `ACTIVATE LOGIC_CORE`, `CREATE`, `REMOVE`, `WIRE`, `UNWIRE`, `SET`, `INSPECT`, and `CLEAR CIRCUIT`.
- Parse and validate a command before mutating. Leave resources, progression, revisions, and persistence unchanged when rejecting a command.
- Keep the TUI and MCP as adapters over the same `MindEngine::execute_dsl` operation; never implement command semantics in a frontend.

## Circuit invariants

- Reserve the built-in identifiers `SENSOR_A`, `SENSOR_B`, and single-input `AWAKENING_BUS`.
- Give AND, OR, and XOR inputs A/B; give NOT input A. Allow at most one source per input, permit outputs to fan out, and consume one bandwidth per wire.
- Keep circuits combinational and acyclic. Treat unwired inputs as LOW and reject any wire that introduces a cycle.
- Cap power at 100.0 and generate exactly 1.0 per daemon-running second. Charge 10.0 for the core and 5.0 per gate. Charge 2 memory for the core, 1 memory per gate, and 1 bandwidth per wire.
- Make `CLEAR CIRCUIT` remove gates, wires, and sensor levels without refunds or progression reset.

## Awakening objective

- Make AND, OR, and NOT available before completion and keep XOR locked.
- Complete COGNITIVE_SPARK only when the current wiring makes `AWAKENING_BUS` equal `SENSOR_A AND SENSOR_B` for all four input combinations and both current sensors are HIGH.
- Keep completion permanent and unlock XOR after completion.

Test parser spans and normalization, every gate truth table, cycles, occupied inputs, capacity limits, failed-command atomicity, milestone truth-table verification, and revision behavior.
