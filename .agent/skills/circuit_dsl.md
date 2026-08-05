# Circuit DSL and solver

Use this guide when changing Phase 1.4 circuit commands, graph rules, or progression.

## Authority and grammar

- Parse the line-oriented, case-insensitive DSL in `mind-core`; normalize identifiers to ASCII uppercase and retain byte spans for diagnostics.
- Supported commands are `HELP`, `STATUS`, `ACTIVATE LOGIC_CORE`, `CREATE`, `REMOVE`, `WIRE`, `UNWIRE`, `SET`, `INSPECT`, and `CLEAR CIRCUIT`.
- Parse and validate a command before mutating. A rejected command must leave resources, progression, revisions, and persistence unchanged.
- The TUI and MCP are adapters over the same `MindEngine::execute_dsl` operation; never implement command semantics in a frontend.

## Circuit invariants

- Built-ins are `SENSOR_A`, `SENSOR_B`, and single-input `AWAKENING_BUS`; their identifiers are reserved.
- AND, OR, XOR use inputs A/B; NOT uses A. Every input accepts at most one source, outputs may fan out, and each wire consumes one bandwidth.
- Circuits are combinational and acyclic. Treat unwired inputs as LOW and reject a wire that introduces a cycle.
- Power caps at 100.0 and generates at exactly 1.0 per daemon-running second. The core costs 10.0; gates cost 5.0. Core memory is 2, each gate is 1, and each wire is 1 bandwidth.
- `CLEAR CIRCUIT` removes gates, wires, and sensor levels without refunds or progression reset.

## Awakening objective

Before completion, AND/OR/NOT are available and XOR is locked. The COGNITIVE_SPARK milestone requires the current wiring to make `AWAKENING_BUS` equal `SENSOR_A AND SENSOR_B` for all four input combinations, with both current sensors HIGH. Completion is permanent and unlocks XOR.

Test parser spans and normalization, every gate truth table, cycles, occupied inputs, capacity limits, failed-command atomicity, milestone truth-table verification, and revision behavior.
