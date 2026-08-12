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

- Preserve the implemented four-by-six regional topology, neutral designations, deterministic persisted generation, initial two-hop range, one constructed two-hop relay extension, and `unknown` / `detected` / `surveyed` / `observed` knowledge progression until a deliberate expansion replaces them.
- Keep undiscovered terrain, temperature, and atmospheric severity out of public planet snapshots. Arrival may expose terrain; a completed survey exposes environmental conditions; holding supplies observed confidence; recall preserves surveyed knowledge.
- Model the planet as selectable regions suitable for a pseudo-3D terminal globe, not a tile-by-tile movement simulation.
- Distinguish unknown, partly surveyed, and continuously observed knowledge. Store confidence and provenance rather than revealing perfect hidden data.
- Keep undiscovered truth out of client snapshots and external co-processor tools.
- Make fragment and rare-discovery randomness bounded by eligibility and hidden guarantee progress.

## Probes, economy, and bases

- Keep the survey scout failure-free and daemon-timed: 15 seconds of travel per graph hop, 30 seconds of focused surveying, an indefinite hold, and explicit recall.
- Keep deposits hidden until surveying. Preserve the alpha's structural, conductive, and silicate families, deterministic region supply, four-unit extractor capacity, and 30-second extraction phase.
- Resolve extractor loss at accepted deployment using the visible 5%, 15%, or 30% risk tier, persist the decision, and never expose the accepted outcome before resolution. Permanently lose the probe and cargo on failure, then rebuild a free replacement at the core in 60 seconds.
- Refine returned units automatically at five seconds per unit. Preserve the first relay cost of 8 structural, 6 conductive, and 6 silicate units, its 90-second construction, and its two-hop extension from the selected surveyed site.
- Build probes from role-producing chassis and module tradeoffs such as range, sensing, mining, cargo, communication, durability, and terrain access.
- Resolve missions from destination, knowledge, environment, energy, loadout, hazards, and infrastructure support; give honest but imperfect risk information.
- Use recognizable geological deposits, local energy, extraction, transport, refining, fabrication, and construction. Automate proven routine logistics without removing meaningful priorities and bottlenecks.
- Grow bases from vulnerable footholds into connected regional hubs whose power, storage, repair, production, sensing, and communication remain locally legible.

## Progression and loss

- Let replaceable units, cargo, facilities, stockpiles, territory, and recent operational progress suffer meaningful loss.
- Keep the starting core recoverable and prevent unwinnable states.
- Preserve completed research, first-discovery records, acquired gameplay SKILLS, and archived story evidence through every material rollback.
- Ensure new research creates choices and specialized capabilities rather than one strictly dominant upgrade path.

Use explicit migrations and invariant validation for every persisted system. Test deterministic generation and guaranteed starting supply, fog transitions, accepted-outcome persistence, risk tiers, concurrent missions, resource conservation, construction costs, range extension, loss and rebuilding without softlock, permanent-knowledge preservation, and restart continuity.
