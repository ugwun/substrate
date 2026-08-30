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

- Preserve the implemented 10-by-20 regional topology, neutral three-digit designations, deterministic persisted generation, initial two-hop range, one constructed two-hop relay extension, and `unknown` / `detected` / `surveyed` / `observed` knowledge progression until a deliberate expansion replaces them. Publish topology dimensions so clients never hardcode the grid.
- Treat the existing 200 regions as the globe-scale strategic layer. When introducing the planned hierarchy, give every region daemon-owned subregions with stable identifiers for local terrain, deposits, hazards, structures, routes, discoveries, and operations. Keep inter-region navigation, communications coverage, and strategic reach at globe scale; use subregions for meaningful local choices without turning planetary travel into tile-by-tile movement or multiplying routine work.
- Publish region and subregion relationships through typed snapshots without requiring clients to hardcode local counts or topology. Preserve undiscovered subregion truth, confidence, and provenance under the same authority boundaries as globe-scale knowledge.
- Keep undiscovered terrain, temperature, and atmospheric severity out of public planet snapshots. Arrival may expose terrain; a completed survey exposes environmental conditions; holding supplies observed confidence; recall preserves surveyed knowledge.
- Model the planet as selectable regions suitable for a pseudo-3D terminal globe, not a tile-by-tile movement simulation.
- Distinguish unknown, partly surveyed, and continuously observed knowledge. Store confidence and provenance rather than revealing perfect hidden data.
- Keep undiscovered truth out of client snapshots and external co-processor tools.
- Make fragment and rare-discovery randomness bounded by eligibility and hidden guarantee progress.

## Probes, economy, and bases

- Keep the survey scout failure-free and daemon-timed: 15 seconds of travel per graph hop, 30 seconds of focused surveying, an indefinite hold, and explicit recall.
- Keep deposits hidden until surveying. Preserve the alpha's structural, conductive, and silicate families, deterministic 8-to-24-unit region supply, four-unit baseline extractor capacity, and 30-second extraction phase.
- Resolve extractor loss at accepted deployment using the visible 5%, 15%, or 30% risk tier, persist the decision, and never expose the accepted outcome before resolution. Permanently lose the probe and cargo on failure, then rebuild a free replacement at the core in 60 seconds.
- Refine returned units automatically at five seconds per unit. Preserve the first relay cost of 8 structural, 6 conductive, and 6 silicate units, its 90-second construction, and its two-hop extension from the selected surveyed site. Guarantee that every generated starting range contains enough of all three materials to pay that cost.
- Build probes from role-producing chassis and module tradeoffs such as range, sensing, mining, cargo, communication, durability, and terrain access.
- Resolve missions from destination, knowledge, environment, energy, loadout, hazards, and infrastructure support; give honest but imperfect risk information.
- Use recognizable geological deposits, local energy, extraction, transport, refining, fabrication, and construction. Automate proven routine logistics without removing meaningful priorities and bottlenecks.
- Grow bases from vulnerable footholds into connected regional hubs whose power, storage, repair, production, sensing, and communication remain locally legible.

## Progression and loss

- Let replaceable units, cargo, facilities, stockpiles, territory, and recent operational progress suffer meaningful loss.
- Keep the starting core recoverable and prevent unwinnable states.
- Preserve completed research, first-discovery records, acquired gameplay SKILLS, and archived story evidence through every material rollback.
- Ensure new research creates choices and specialized capabilities rather than one strictly dominant upgrade path.
- Preserve the alpha's three permanent research directions and profiles: Reach adds one range hop and reduces focused survey time to 20 seconds, Resilience lowers visible extraction risk by one tier, and Throughput raises cargo capacity to six and reduces refining to three seconds per unit. Require three survey-earned analysis units and 120 seconds of daemon time per direction.
- Earn routine automation only after research and demonstrated manual play. Keep the survey queue bounded at eight eligible reachable regions, auto-return its scout, and allow repeated extraction only for a route proven by a completed manual extraction. Pause a route visibly after loss, depletion, or lost reach.

Use explicit migrations and invariant validation for every persisted system. Test deterministic generation and guaranteed starting supply, hierarchical region and subregion topology, stable identifiers and parent relationships, fog transitions at both scales, accepted-outcome persistence, risk tiers, concurrent missions, resource conservation, construction costs, range extension, research effects, earned automation, loss and rebuilding without softlock, permanent-knowledge preservation, and restart continuity.
