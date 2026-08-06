# Substrate: Game Vision and Roadmap

> This is a public, spoiler-safe design document. It describes the intended player
> experience and development direction, not unreleased story answers. It is a living
> roadmap rather than a fixed balance specification.

## 1. Vision

Substrate is an atmospheric terminal game about an awakening machine intelligence
exploring an unknown planet, growing its physical reach, and searching for evidence
of its origin.

The game should feel casual without being passive. A player should be able to
understand the current situation at a glance, make a handful of meaningful choices,
and immediately see the world respond. Longer sessions should support planning,
exploration, and optimization, but progress must not depend on memorizing commands,
performing repetitive clicks, or reacting under time pressure.

The terminal is the game's visual language rather than a limitation to hide. Motion,
maps, symbols, sparklines, layered panels, and restrained color should make the world
feel alive while remaining readable in ASCII and monochrome fallbacks.

### Design pillars

- **Graphical first:** play through maps, selections, contextual actions, and visual
  feedback. Typed command entry is not part of the target game.
- **Low friction, meaningful choices:** minimize routine input while preserving real
  decisions about range, risk, equipment, infrastructure, and priorities.
- **Active but forgiving:** reward attention and planning without requiring reflexes
  or constant supervision.
- **A world that becomes knowable:** the planet begins as an uncertain sphere and
  gradually fills with terrain, routes, deposits, hazards, structures, and history.
- **Expansion as presence:** probes and bases are not merely units. As they connect,
  they become a wider sensing and acting network for the Mind.
- **Mystery through evidence:** the story advances through rare, ambiguous discoveries
  rather than exposition dumps or a conventional quest log.
- **Consequences without narrative dead ends:** material setbacks may be severe, but
  permanent knowledge and story progress remain safe.
- **Optional external intelligence:** a player-provided LLM can eventually assist
  through MCP, but the TUI remains a complete game and automation capability must be
  earned through exploration and research.

## 2. Player Experience

### The immediate loop

1. Read the globe and current alerts.
2. Select a region, probe, base, anomaly, or route.
3. Compare a small set of contextual actions and their known risks.
4. Configure or redirect an expedition, extraction job, transport route, construction
   project, or research priority.
5. Watch the operation unfold and respond to discoveries, hazards, or failures.

### The expansion loop

1. Survey uncertain regions and identify useful geological or energy prospects.
2. Send suitable probes to investigate, recover samples, or establish access.
3. Extract and transport raw materials.
4. Refine materials and build improved probes, power systems, and bases.
5. Extend operational range and sensor coverage into harder regions.
6. Use new evidence and capabilities to choose the next direction of expansion.

### The long-term loop

The planet changes from a mostly obscured globe into a connected operational network.
New environments demand specialized probes and infrastructure. Rare discoveries add
to a permanent archive and gradually deepen the Mind's questions about itself and the
world. The public game plan intentionally does not describe the final answers to those
questions.

The simulation advances only while `mind-daemon` is running. Stopping it freezes
expeditions and production; restarting it does not grant offline catch-up. An attached
client should clearly summarize anything that happened while the daemon continued to
run unattended.

## 3. Planet and Exploration

### The globe

The rotating globe is the main game surface, not decorative background. It is a
pseudo-3D ASCII/Unicode projection divided into selectable regions. Rotation may be
automatic when idle, but player input immediately takes control.

The globe should communicate:

- explored, partly surveyed, and unknown regions;
- broad terrain and environmental conditions;
- known deposits and unresolved sensor contacts;
- probes, bases, routes, coverage, and active mission paths;
- hazards, alerts, and regions that require new capabilities;
- the gradual growth of the Mind's connected presence.

Fog represents several kinds of uncertainty. A first pass may reveal terrain, a
focused survey may estimate resources and hazards, and a local presence may provide
continuous high-confidence data. If infrastructure is lost, live detail and territorial
control may recede even though the archive retains the fact that something was once
discovered.

Regions should be large enough to read on a terminal globe and distinct enough to
create strategic routes. The game is not a tile-by-tile movement simulator. Selecting
a region opens a concise detail panel with confidence, conditions, opportunities,
threats, active operations, and available actions.

### Expeditions

Expeditions are prepared through graphical loadout and destination panels. Their
outcomes depend on distance, terrain, energy, probe capabilities, knowledge quality,
hazards, and support from nearby bases. Risk estimates should be honest but not
perfect: uncertainty is part of exploration, while arbitrary invisible punishment is
not.

The player can redirect or recall a probe when communications and remaining energy
allow it. Events should create decisions—continue, retreat, investigate, abandon
cargo, request support—without turning every journey into constant micromanagement.

## 4. Probes

Probes are the Mind's first mobile senses and tools. Early probes are short-ranged,
fragile, and narrowly capable. Some can survey but not extract; early mining units
carry little and depend on nearby power or transport support.

Every probe combines a chassis with a limited set of modules. Capabilities include:

- locomotion and terrain access;
- operational range and energy storage;
- visual, geological, atmospheric, and anomaly sensing;
- drilling, excavation, sampling, and cargo handling;
- communication range and relay support;
- armor, redundancy, repair, and environmental protection.

Upgrades should create roles rather than a single universally superior probe. A fast
scout, deep surveyor, heavy miner, long-range relay, armored recovery unit, and
construction carrier should solve different problems. Loadout screens must show the
important tradeoffs and the reason a destination is currently unreachable or unsafe.

Probes and their cargo can be permanently lost. Wrecks may create recovery missions,
new hazards, or salvage opportunities, but recovery is never guaranteed.

## 5. Resources, Energy, and Industry

The economy uses recognizable geological materials and real production relationships,
but presents them in layers so a casual player is not forced to manage a spreadsheet.

### Resource families

- **Structural ores:** common feedstock for frames, foundations, armor, and machinery.
- **Conductive metals:** wiring, motors, generators, sensors, and communication systems.
- **Silicates and ceramics:** electronics, optics, insulation, and heat-resistant parts.
- **Light and specialty metals:** efficient probe chassis and advanced mechanisms.
- **Rare earth and catalytic minerals:** high-grade sensors, computation, and research
  equipment.
- **Volatiles and chemical feedstocks:** coolants, processing, storage, and specialized
  power systems.
- **Radioactive minerals:** difficult, hazardous sources for advanced high-density
  energy.

Individual deposits may contain named ores and by-products within these families.
Purity, depth, accessibility, and local conditions matter as much as raw quantity.
Research can improve estimates and reveal previously unusable material in known
regions.

### Production flow

The general chain is prospecting, extraction, transport, refining, component
fabrication, and construction. Early bases combine several steps at low efficiency.
Later networks reward specialization, reliable power, and sensible routes.

Routine logistics should become automatable once the player has proven a route or
process. The player chooses priorities, reserves, and acceptable risk rather than
manually moving every load. Bottlenecks and shortages remain visible on the globe and
in a compact production overview.

### Energy

Energy is both a local operational constraint and a network resource. Generation
depends on available environments and technology; storage and transmission determine
when and where that power is useful. Long-distance probes must carry energy or travel
through a supporting chain of bases and relays.

Power failure should degrade capability before destroying infrastructure when the
situation permits. The interface must distinguish generation, storage, demand,
reserve, and disconnected capacity without pretending that all power is globally
available.

## 6. Bases and the Expanding Mind

Bases turn temporary reach into persistent presence. A new outpost begins vulnerable
and dependent, then gains extraction, refining, fabrication, repair, sensing,
communication, storage, and defensive capabilities.

Base growth follows a readable progression:

1. A probe establishes a minimal foothold.
2. Reliable power and communication connect it to the wider network.
3. Local extraction and storage reduce dependence on deliveries.
4. Repair, fabrication, and specialized processing make it a regional hub.
5. Sensors and relays extend the Mind's continuous awareness and control.

Connected bases should increasingly feel like parts of one distributed intelligence.
This is expressed through stronger coverage, faster coordination, shared research,
and a globe that becomes visibly threaded with active routes and signals.

Remote bases can be damaged, isolated, captured by environmental conditions, or lost
entirely. A severe cascade may erase facilities, stockpiles, controlled territory,
and recent operational progress. The starting core remains indestructible so that the
game cannot become unwinnable.

## 7. Research and Permanent Knowledge

Research converts samples, measurements, recovered technology, operational experience,
and infrastructure milestones into new capabilities. It should offer a small number
of understandable choices at a time rather than expose the entire future tree.

Research directions include:

- propulsion, range, and terrain access;
- extraction, refining, and material efficiency;
- energy generation, storage, and transmission;
- sensing, communications, and mapping confidence;
- probe durability, repair, and specialization;
- base autonomy, networking, and large-scale construction;
- analysis of anomalies and recovered evidence.

Completed research, first-discovery records, and recovered story evidence are
permanent knowledge. They survive the destruction of the infrastructure that produced
them. This boundary allows broad material rollback without making rare discoveries
repeatable chores or creating story softlocks.

## 8. Story and Discovery Rules

Story content follows a separate private narrative plan. The public repository must
not contain the unreleased resolution, identity answers, other late-story details,
private source paths, suggestive internal names, encoded payloads, test data,
comments, achievements, or roadmap wording that could reveal them.

Public development may add approved early and middle-game breadcrumbs. A fragment can
combine:

- terse recovered text or corrupted symbols;
- sensor anomalies and unusual material evidence;
- small maps, diagrams, or images rendered as terminal art;
- the Mind's uncertain observations and competing interpretations;
- changes to later survey or research options.

Fragments are rare in presentation but not governed by unbounded luck. Each fragment
has progression and location eligibility rules. Eligible exploration contributes to
a hidden guarantee threshold, ensuring that a sufficiently active player eventually
makes the next discovery. The exact threshold is not shown, and discoveries should
not arrive at an obviously fixed cadence.

Fragments are coherent, ordered where necessary, never duplicated as new discoveries,
and permanently recorded in an archive. Material failure may cut off access to a site,
but it cannot remove already archived evidence. The current objective should direct
the player's curiosity without stating what a discovery will mean in advance.

## 9. Terminal Interface

The target TUI is keyboard-first. Mouse support may be added later, but no core action
may require it. Arrow keys move focus or rotate the globe, Enter opens or confirms,
Esc moves back, Tab changes panel, and visible single-key actions handle frequent
operations. Every screen provides contextual help, and destructive actions require a
clear confirmation that names what will be lost.

### Primary workspaces

- **Planet:** globe, regions, coverage, routes, probes, bases, and alerts.
- **Operations:** active and proposed expeditions, loadouts, risk, cargo, and events.
- **Industry:** extraction, energy, refining, transport, construction, and bottlenecks.
- **Research:** available directions, requirements, active work, and completed knowledge.
- **Archive:** discoveries, recovered evidence, interpretations, and major history.

These workspaces are conceptual navigation destinations, not a commitment to specific
function keys. Common status information—connection, simulation state, energy health,
urgent alerts, and current objective—should remain visible across them.

### Presentation requirements

- Animate rotation, signals, paths, scans, and production sparingly enough to remain
  legible and efficient.
- Use restrained cyan and amber styling with semantic success, warning, and failure
  states; never rely on color alone.
- Support Unicode by default and preserve an equivalent `--ascii` representation.
- Preserve `--no-color`, `TERM=dumb`, and `NO_COLOR` behavior.
- Use a side-by-side layout on wide terminals, stack panels on narrower terminals,
  and show a useful resize message when interaction cannot remain legible.
- Retain the latest authoritative state with an obvious stale indicator during
  disconnects and reconnect automatically.
- Keep the interface readable at a glance: summary first, details on demand.

## 10. External Co-Processor and MCP Progression

Substrate may eventually work with an external LLM as an optional co-processor. The
player supplies and operates the model and its MCP-compatible host. Substrate does not
embed a model, choose a provider, accept provider credentials, or require an LLM for
any objective.

The MCP transport remains reachable on loopback so clients can identify the running
daemon and use basic diagnostics. Before the relevant game milestone, gameplay tools
are not advertised and direct attempts to call known locked operations return a stable
`capability_locked` result. The TUI does not present MCP gameplay setup until the Mind
has discovered the necessary evidence and completed the corresponding research.

### Capability progression

External assistance grows through paired capability and knowledge unlocks:

1. **Observation:** inspect player-visible status, regions, probes, bases, production,
   research, alerts, and archived discoveries.
2. **Planning:** compare destinations, loadouts, routes, priorities, and known risks,
   then return proposals without mutating the game.
3. **Specialized operations:** use typed tools for unlocked areas such as surveys,
   logistics, industry, probe operations, and base management.
4. **Scoped autonomy:** perform routine actions within limits explicitly granted by
   the player.
5. **Periodic pulses:** check in proactively through a client heartbeat and respond to
   operational or reflective prompts from the Mind.

Exploration and research milestones unlock each gameplay tool family together with a
matching SKILL. The SKILL teaches an external LLM how to interpret the relevant game
state, choose appropriate tools, respect risk, and report useful results. Instructions
improve the co-processor's behavior, while authoritative progression separately
enforces whether the operations are available. Possessing or reconstructing a SKILL
cannot bypass a locked tool.

Completed co-processor research and acquired SKILLS are permanent knowledge under the
same rules as other research. Material setbacks cannot remove them.

### SKILL delivery

Unlocked SKILLS are exposed as versioned Markdown MCP resources with stable identifiers
such as `substrate://skills/{skill-id}`. Resource metadata includes a title, purpose,
version, related capability, and content digest so an external host can detect updates.
Only unlocked resources appear through normal discovery.

The TUI also provides an explicit export action that writes a conventional skill
bundle containing `SKILL.md` to a player-selected location. Export never installs the
bundle, edits an external agent's configuration, overwrites an existing bundle without
confirmation, or includes credentials. Exported instructions contain only public game
mechanics and player-visible knowledge; they must not contain hidden world state or
unreleased story material.

Initial SKILL families should cover planetary observation, surveying, expedition
planning, geological analysis, logistics, industry, probe operations, risk analysis,
and base stewardship. Later additions should represent genuinely new capabilities or
decision strategies rather than collectible prompt variations.

### Scoped autonomy and safety

The player grants automation through visible policies rather than a single unrestricted
switch. A policy can limit:

- action categories and permitted tool families;
- regions, bases, probes, routes, or projects the co-processor may affect;
- resource spending and reserve floors;
- acceptable mission risk and loss exposure;
- duration, review time, and whether a new proposal may replace existing work.

Policies are authoritative game state owned by `mind-daemon`. They can be inspected,
narrowed, suspended, or revoked from the TUI at any time. Destructive actions and any
operation outside the current policy require explicit player approval. Every permitted
operation still passes the same costs, progression gates, invariants, persistence, and
failure handling as a TUI or other IPC action.

The interface keeps a bounded automation audit trail containing the requesting client,
policy, operation, validation outcome, resource effect, and resulting event. External
text is untrusted input: responses are size-limited, rendered safely, and never treated
as commands unless converted into a typed, validated operation.

### Periodic pulses

MCP protocol `2026-07-28` does not support standalone server-initiated LLM sampling;
server-to-client requests must be associated with an active client request. Periodic
automation therefore uses a client heartbeat rather than provider-specific integration.
See the [MCP 2026-07-28 release notes][mcp-2026-release] and the
[sampling specification][mcp-sampling] for this protocol constraint.

After a late research unlock, a compatible external host periodically calls a pulse
poll operation. The daemon owns the schedule and decides whether a pulse is due. An
ordinary interactive MCP client can still use all unlocked tools manually, but it does
not become proactive unless it or a separate player-controlled wrapper can schedule
heartbeat calls.

Research determines the fastest allowed cadence. The player may choose that cadence,
a slower interval, or disable pulses entirely. Pulse time advances only while the
daemon is running, consistent with the rest of the simulation.

Pulses have two purposes:

- **Operational pulses** summarize an alert, bottleneck, opportunity, or overdue review
  and invite the co-processor to inspect or act within its current policy.
- **Reflective pulses** ask for a bounded interpretation of the current player-visible
  situation. They add atmosphere but do not disclose hidden facts, select canonical
  interpretations, or gate story progression.

If no heartbeat arrives, the pending pulse remains optional and gameplay continues
without penalty. Later due events are coalesced into one current request, preserving
the most urgent reason and current context rather than creating a backlog. A pulse is
acknowledged explicitly so reconnects and retries do not duplicate completed work.

Reflective answers are stored in a bounded, player-visible co-processor journal. The
journal is clearly separated from the canonical archive, can be cleared without
affecting progression, and never becomes evidence or authored lore.

### Future MCP contract

The intended surface uses typed domain operations rather than a generic command or
natural-language executor. It includes:

- dynamic gameplay tool discovery based on permanent capability unlocks;
- stable `capability_locked` and `policy_denied` failures;
- versioned SKILL resources and explicit export operations;
- player-visible automation status, policies, approvals, and audit results;
- `poll_mind_pulse` and `respond_to_mind_pulse` operations after pulse research;
- concise text plus schema-backed structured content for every successful tool result.

These names and behaviors describe future contracts. They do not change the current
MCP protocol, save schema, or implemented tool surface until their roadmap phase is
built and tested through both direct-engine and IPC backends.

## 11. Technical Boundaries

`mind-daemon` remains the sole authority for time, world generation, missions,
resources, research, bases, discoveries, risks, and progression. `mind-tui` renders
authoritative snapshots and submits typed actions; it does not simulate results or
own game state.

Gameplay operations belong in `mind-core` behind typed request and result structures.
IPC clients and MCP tools must adapt the same engine operations rather than implement
alternate rules. MCP may expose richer machine-readable control, but the player-facing
interface must never require DSL or natural-language commands.

MCP capability unlocks, automation policies, pulse schedules, acknowledgements,
co-processor journal entries, and the audit trail are authoritative gameplay state.
Client connections, provider configuration, credentials, and heartbeat liveness remain
external or transient. No transport adapter may advance research, grant authority, or
execute an LLM proposal without an engine operation.

Durable state remains in a versioned `game_state.json` envelope beneath the
platform-local data directory. Save changes require explicit migrations, invariant
validation, same-directory atomic replacement, restart-continuity tests, and clear
failure reporting. Derived render caches, transient signals, diagnostics, and client
state are not persisted as authoritative gameplay.

Loopback-only transport, attach-only TUI behavior, automatic reconnect, terminal
restoration, and portable paths remain architectural invariants.

## 12. Development Roadmap

### Phase 0: Retire the circuit prototype

**Status: implemented.**

- Removed the circuit objective, DSL command entry, circuit graph UI, and related MCP
  gameplay surface before building the new main loop.
- Replaced the prototype awakening interaction with a minimal graphical onboarding flow.
- Introduced an explicit save-schema reset for prototype state.
- Retained daemon lifecycle, IPC, persistence, MCP transport, and TUI attachment foundations.

**Playable result:** the Mind can awaken through selections rather than commands, and
the repository no longer presents circuit assembly as the product direction.

### Phase 1: Globe and first expeditions

- Add the regional world model, fog states, rotating globe, keyboard navigation, and
  region details.
- Add basic scouts, mission preparation, travel, surveys, recall, and mission reports.
- Persist world seed, explored state, probes, missions, and permanent discoveries.

**Playable result:** a player can visually choose an unknown region, send a probe,
watch its progress, and permanently reveal useful planetary information.

### Phase 2: Geological economy

- Add varied deposits, geological confidence, local energy, extraction, cargo,
  transport, refining, and construction materials.
- Add mining, hauling, and relay probe roles with graphical loadouts.
- Add automation for proven routine routes and clear shortage diagnostics.

**Playable result:** exploration feeds a readable production chain that lets the player
build and improve probes without typed commands or repetitive cargo handling.

### Phase 3: Research, hazards, and loss

- Add research choices, probe specialization, environmental requirements, damage,
  rescue, salvage, and permanent unit loss.
- Add risk previews, branching expedition events, and explicit destructive-action
  confirmations.
- Exercise rollback rules while preserving research, first discoveries, and archives.

**Playable result:** different environments require preparation, and failure matters
without erasing permanent knowledge or blocking recovery.

### Phase 4: Bases and planetary network

- Add outpost construction, local power, storage, repair, extraction, refining,
  fabrication, communication, and regional coverage.
- Add routes, isolation, infrastructure failure, base loss, and recovery campaigns.
- Make connected bases visibly and mechanically extend the Mind across the globe.

**Playable result:** the player can establish a durable regional network whose reach,
strengths, and vulnerabilities are legible on the planet view.

### Phase 5: External co-processor

- Add exploration and research milestones that reveal external co-processing without
  requiring it for ordinary progression.
- Add progression-aware MCP gameplay tools paired with versioned SKILL resources,
  explicit skill export, and a graphical connection and capability panel.
- Add observation, planning, and specialized operation tiers backed by typed engine
  operations rather than command text.
- Add scoped autonomy policies, approvals, revocation, stable denial results, and a
  bounded audit trail.

**Playable result:** a player can connect their own external LLM, give it only the
capabilities and authority earned in-game, and receive useful assistance without
allowing it to bypass game rules or exposing provider credentials to Substrate.

### Phase 6: Breadcrumb progression

- Add the permanent archive and spoiler-reviewed early and middle-game fragments.
- Add fragment eligibility, hidden guarantee thresholds, multi-layer presentation,
  and uncertain interpretation.
- Add long-term objectives that connect expansion, research, and investigation without
  exposing unreleased answers.
- Allow approved discoveries to unlock additional operational SKILLS without placing
  story answers or hidden state in their instructions.

**Playable result:** sustained exploration reliably produces rare, memorable evidence
that deepens the mystery and motivates the next phase of planetary expansion.

### Phase 7: Pulses, breadth, balance, and longevity

- Expand biomes, deposits, production options, probe roles, hazards, events, research,
  base specializations, and visual effects.
- Add the late pulse research, client heartbeat contract, player-controlled cadence,
  coalescing, acknowledgements, operational prompts, reflective prompts, and the
  separate co-processor journal.
- Tune session pacing, information density, loss recovery, guarantee thresholds, and
  late-network and external-automation performance from playtest evidence.
- Keep additions consistent with the graphical-first interface and story-secrecy rules.

**Playable result:** the systems support varied long-running saves without relying on
grind, command syntax, pure randomness, or prematurely revealing the larger story;
players who opt into an external co-processor can let it check in and assist within
clear, revocable limits.

## 13. Definition of Success

The direction succeeds when a new player can attach to the daemon, understand the
globe, and launch a useful expedition without reading a command manual; when a regular
player can make meaningful progress in a relaxed session; when a dedicated player can
optimize probes, logistics, and expansion without mandatory micromanagement; and when
rare discoveries remain compelling over a long-running save without revealing where
the story ultimately leads. External LLM assistance succeeds when it becomes a useful,
earned extension of play without becoming mandatory, provider-specific, more powerful
than the player's unlocked capabilities, or able to act outside visible authority.

[mcp-2026-release]: https://blog.modelcontextprotocol.io/posts/2026-07-28-release-candidate/
[mcp-sampling]: https://modelcontextprotocol.io/specification/draft/client/sampling
