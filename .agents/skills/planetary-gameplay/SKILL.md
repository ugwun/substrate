---
name: planetary-gameplay
description: Build and maintain Substrate's planetary game systems. Use when changing world generation, regions, fog and survey confidence, probes, missions, geology, energy, extraction, logistics, research, bases, hazards, material loss, permanent knowledge, or progression beyond the circuit prototype.
---

# Planetary gameplay

## Authority and time

- Implement world generation, missions, economy, research, bases, loss, and progression in the daemon-owned engine.
- Advance all activity only while the daemon runs; do not calculate offline catch-up in clients or at load time.
- Derive TUI, IPC, and MCP views from authoritative typed snapshots and operations.
- Persist random seeds and decision-relevant outcomes so reconnects and restarts cannot reroll accepted results.

## World and exploration

- Model the planet as selectable regions suitable for a pseudo-3D terminal globe, not a tile-by-tile movement simulation.
- Distinguish unknown, partly surveyed, and continuously observed knowledge. Store confidence and provenance rather than revealing perfect hidden data.
- Keep undiscovered truth out of client snapshots and external co-processor tools.
- Make fragment and rare-discovery randomness bounded by eligibility and hidden guarantee progress.

## Probes, economy, and bases

- Build probes from role-producing chassis and module tradeoffs such as range, sensing, mining, cargo, communication, durability, and terrain access.
- Resolve missions from destination, knowledge, environment, energy, loadout, hazards, and infrastructure support; give honest but imperfect risk information.
- Use recognizable geological deposits, local energy, extraction, transport, refining, fabrication, and construction. Automate proven routine logistics without removing meaningful priorities and bottlenecks.
- Grow bases from vulnerable footholds into connected regional hubs whose power, storage, repair, production, sensing, and communication remain locally legible.

## Progression and loss

- Let replaceable units, cargo, facilities, stockpiles, territory, and recent operational progress suffer meaningful loss.
- Keep the starting core recoverable and prevent unwinnable states.
- Preserve completed research, first-discovery records, acquired gameplay SKILLS, and archived story evidence through every material rollback.
- Ensure new research creates choices and specialized capabilities rather than one strictly dominant upgrade path.

Use explicit migrations and invariant validation for every persisted system. Test deterministic generation, fog transitions, mission resolution and retry behavior, resource conservation, route automation, local power separation, destructive cascades, recovery from severe loss, permanent-knowledge preservation, and restart continuity.
