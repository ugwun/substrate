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
- Treat the existing 200 regions as the globe-scale strategic layer. Preserve the deterministic daemon-owned 4-by-4 local field in every region: sixteen stable sectors with identifiers, parent links, row and column, class, energy, hazard, deposits, knowledge, projects, and base placement. Keep inter-region navigation, communications coverage, and strategic reach at globe scale; local arrows select coordinates rather than simulating an avatar or planetary tile movement.
- Let a completed regional survey detect only the five-sector landing cross. A focused survey may resolve a detected sector and must detect its orthogonally adjacent unknown sectors, producing a connected local frontier. Preserve resolved knowledge and accepted operations while the frontier expands; never reveal hidden conditions merely because a coordinate is present in a snapshot.
- Publish region and subregion relationships through typed snapshots without requiring clients to hardcode local counts or topology. Preserve undiscovered subregion truth, confidence, and provenance under the same authority boundaries as globe-scale knowledge.
- Keep undiscovered terrain, temperature, and atmospheric severity out of public planet snapshots. Arrival may expose terrain; a completed survey exposes environmental conditions; holding supplies observed confidence; recall preserves surveyed knowledge.
- Model the planet as selectable regions suitable for a pseudo-3D terminal globe, not a tile-by-tile movement simulation.
- Distinguish unknown, partly surveyed, and continuously observed knowledge. Store confidence and provenance rather than revealing perfect hidden data.
- Keep undiscovered truth out of client snapshots and external co-processor tools.
- Make fragment and rare-discovery randomness bounded by eligibility and hidden guarantee progress.

## Probes, economy, and bases

- Keep the survey scout failure-free and daemon-timed: 15 seconds of travel per graph hop, 30 seconds of focused surveying, an indefinite hold, and explicit recall.
- Keep deposits hidden until surveying. Preserve the alpha's structural, conductive, and silicate families, deterministic 8-to-24-unit region supply, four-unit balanced extractor capacity, and 30-second balanced extraction phase.
- Preserve the three reversible extractor rigs: Balanced uses baseline capacity, risk, travel, 30-second extraction, and 60-second rebuild; Cargo adds two capacity, raises risk one tier, uses 125% travel, a 35-second extraction, and a 75-second rebuild; Shielded removes one capacity, lowers risk one tier, uses 150% travel, a 40-second extraction, and a 90-second rebuild. Stack the selected rig with current profile and local support before deployment, then persist accepted capacity, risk, travel, cycle, recovery, loadout, and failure outcome so later configuration or infrastructure changes cannot rewrite an active mission.
- Resolve extractor loss at accepted deployment using the visible 5%, 15%, or 30% risk tier, persist the decision, and never expose the accepted outcome before resolution. Permanently lose the probe and cargo on failure, then rebuild a free replacement at the core using the accepted rig duration.
- Refine returned units automatically at five seconds per unit. Preserve the first relay cost of 8 structural, 6 conductive, and 6 silicate units, its 90-second construction, and its two-hop extension from the selected surveyed site. Guarantee that every generated starting range contains enough of all three materials to pay that cost.
- Build probes from role-producing chassis and module tradeoffs such as range, sensing, mining, cargo, communication, durability, and terrain access.
- Resolve missions from destination, knowledge, environment, energy, loadout, hazards, and infrastructure support; give honest but imperfect risk information.
- Use recognizable geological deposits, local energy, extraction, transport, refining, fabrication, and construction. Automate proven routine logistics without removing meaningful priorities and bottlenecks.
- Grow bases from vulnerable footholds into connected regional hubs whose power, storage, repair, production, sensing, and communication remain locally legible.
- Preserve the implemented foothold and module chain: one base per region; power before every other module; communications before workshop, laboratory, or observation; distributed laboratory support before observation; two concurrent base projects, one additional slot under Throughput, and one slot for each active connected depot up to two. Let a depot reduce accepted construction time by 25% within three regional hops. Use the public module costs from `BaseModuleKind::cost` in engine and clients instead of duplicating them.
- Make connected powered communications nodes extend field reach by three hops through a geographic chain. Count the concluding network only when it includes three connected nodes and at least one lies beyond the original core-and-relay envelope.
- Derive local power from site yield, enabled module demand, and stored reserve. Let players suspend and restore individual modules to shed load, preserve communications, or choose which support remains operational; keep a suspended mode persistent and distinguish it from an enabled but starved module. Let moderate and high site hazards weather integrity only while the daemon runs, stop destructive wear at the recoverable 20% floor, take a node offline at that floor, let a powered workshop repair routine wear and lower local extractor risk, and preserve the S4/C2, 60-second maintenance recovery that restores 35 integrity.
- Let a powered depot add four local extractor cargo units. Resolve local extraction only from surveyed site deposits, publish the effective risk, persist the accepted outcome, and keep regional route automation separate until a local route has its own demonstrated policy.

## Progression and loss

- Let replaceable units, cargo, facilities, stockpiles, territory, and recent operational progress suffer meaningful loss.
- Keep the starting core recoverable and prevent unwinnable states.
- Preserve completed research, first-discovery records, acquired gameplay SKILLS, and archived story evidence through every material rollback.
- Ensure new research creates choices and specialized capabilities rather than one strictly dominant upgrade path.
- Preserve the alpha's three permanent research directions and profiles: Reach adds one range hop and reduces focused survey time to 20 seconds, Resilience lowers visible extraction risk by one tier, and Throughput raises cargo capacity to six and reduces refining to three seconds per unit. Require three survey-earned analysis units and 120 seconds of daemon time per direction.
- Earn routine automation only after research and demonstrated manual play. Keep the survey queue bounded at eight eligible reachable regions, auto-return its scout, and allow repeated extraction only for a route proven by a completed manual extraction. Pause a route visibly after loss, depletion, or lost reach.
- Treat the first successful earned automated operation as completion of the foundation chapter and the unlock for local planetary planning. Keep the final solar-horizon milestone separate: require 18 surveyed regions, 12 surveyed sites, two research directions, proven automation, three connected bases including one beyond the foundation envelope, one powered laboratory, two powered observation arrays, and completion of the approved ordered archive observations. Publish only discovered records and that the read-only system map is available, keep the planet reviewable, and expose no system-scale action beyond the demo boundary.

Use explicit migrations and invariant validation for every persisted system. Test deterministic generation and guaranteed starting supply, complete 4-by-4 coordinates, stable identifiers and parent relationships, landing-cross and orthogonal-frontier fog transitions, old-site preservation during expansion, accepted-outcome persistence, risk tiers, concurrent missions, resource conservation, construction costs, range extension, research effects, earned automation, solar-horizon availability, loss and rebuilding without softlock, permanent-knowledge preservation, and restart continuity.
