# Substrate: Public Product Direction

> This is Substrate's public, spoiler-safe product plan. It defines the intended
> player experience, durable design decisions, and development priorities without
> describing unreleased story answers.

## 1. Purpose

Substrate's public documents have distinct responsibilities:

- `README.md` explains what is playable now, how to run it, and how its current
  controls and systems behave.
- `AGENTS.md` records current architecture, authority boundaries, implementation
  workflow, and repository policy.
- `GAME_PLAN.md` defines the product direction and the order in which the game
  should grow.

Implementation details belong here only when they express a lasting product
constraint. Exact balance values, protocol versions, save schemas, and operational
commands belong in the README, repository guidance, or owning subsystem.

## 2. Vision

Substrate is an atmospheric terminal game about an awakening machine intelligence
exploring an unknown planet, extending its physical reach, and searching for evidence
of its origin.

The game should feel casual without becoming passive. A player should be able to read
the current situation at a glance, make a small number of meaningful choices, and see
the world respond. Longer sessions should support planning, exploration, and
optimization without requiring memorized commands, repetitive input, reflexes, or
constant supervision.

The terminal is the game's visual language rather than a limitation to hide. Maps,
motion, symbols, layered panels, and restrained color should make the world feel alive
while preserving equivalent ASCII and monochrome presentations.

### Design pillars

- **Graphical first:** maps, selection, contextual actions, and visual feedback drive
  play. Typed command entry is not a progression mechanic.
- **Low friction, meaningful choices:** reduce routine input while preserving decisions
  about range, risk, equipment, infrastructure, priorities, and recovery.
- **Active but forgiving:** reward attention and planning without demanding constant
  presence or time-critical reactions.
- **A world that becomes knowable:** uncertainty gradually gives way to terrain,
  routes, resources, hazards, structures, and permanent evidence.
- **Expansion as presence:** probes and connected infrastructure extend what the Mind
  can sense, understand, and do.
- **Mystery through evidence:** rare, ambiguous observations deepen questions without
  turning the game into a sequence of exposition logs.
- **Consequences without dead ends:** material setbacks matter, while permanent
  knowledge and required progression remain recoverable.
- **Automation after understanding:** proven routines may be compressed into policies
  and priorities, but new decisions remain with the player.
- **Optional external intelligence:** a player-provided LLM may eventually assist, but
  it is never required and never receives knowledge the player has not discovered.

## 3. Player Experience

### Core loop

1. Read the globe, objective, active operations, stores, and alerts.
2. Select a region or operational concern.
3. Compare contextual actions, requirements, uncertainty, and known risk.
4. Commit a survey, extraction, route, construction, research, or recovery decision.
5. Watch the operation unfold and use its result to choose the next expansion step.

Exploration should feed production, production should extend reach, and extended reach
should reveal new operational and investigative questions. No major system should feel
like an isolated minigame or a currency added only to lengthen progression.

### Progress horizons

Several useful horizons should coexist:

- **Immediate:** visible movement, progress, feedback, and local consequences within
  seconds or a few minutes.
- **Tactical:** a small question, constraint, or operation resolves within a normal
  play interval.
- **Session:** a player can complete a coherent plan or open a meaningful new choice.
- **Chapter:** accumulated mastery changes available capabilities or the scale of play.
- **Long term:** permanent knowledge and infrastructure gradually transform the planet
  and deepen the central investigation.

Durations are playtest targets, not retention timers. If the only meaningful action is
waiting for a number to rise, the interval should be shortened or accompanied by
another useful decision.

### Long-play principles

Substrate should encourage voluntary return through competence, autonomy, curiosity,
and unfinished plans. It must not use login streaks, expiring evidence, absence
penalties, artificial action scarcity, social pressure, or fear of missing content.

A returning player should quickly understand:

- what completed or failed while the daemon remained active;
- what changed materially;
- what permanent observations were recorded;
- what the current objective is;
- which two or three actions are now useful.

Simulation advances only while the daemon is running. Detaching is safe, stopping the
daemon freezes time, and no offline catch-up is granted.

## 4. Planetary Systems

### Globe and exploration

The rotating regional globe is the main game surface, not decorative background. It
must communicate uncertainty, discovered conditions, operational range, probes,
infrastructure, active routes, mission paths, and the gradual growth of connected
presence.

Fog represents knowledge rather than a binary covering. Arrival, focused survey, and
local presence may provide different confidence levels. Losing infrastructure may
remove live detail or access, but it must not erase an observation already preserved
as permanent knowledge.

Regions are strategic areas rather than movement tiles. Selecting one should expose
its known conditions, current operations, local presence, opportunities, risks,
available actions, and exact blockers without forcing the player to change workspace.

### Probes and operations

Probes begin as limited mobile senses and tools. Later designs should create distinct
roles through tradeoffs in range, sensing, extraction, cargo, communications,
durability, repair, and terrain access rather than converge on one universally best
unit.

Mission outcomes should derive from destination, confidence, environment, energy,
loadout, hazards, and infrastructure support. Published risks must be honest but need
not provide perfect certainty. Accepted random outcomes must persist so reconnecting
or restarting cannot reroll them.

Failures should create decisions such as rebuilding, recovery, rerouting, or deferral.
The starting core remains a path back into play, preventing permanent softlocks.

### Resources, energy, and infrastructure

The production chain grows from prospecting and extraction through transport,
refining, fabrication, and construction. Materials should have understandable roles,
while depth comes from deposit quality, accessibility, local conditions, energy,
capacity, routing, and specialization rather than a large collection of arbitrary
currencies.

Energy is both local and networked. Generation, storage, demand, reserve, and
transmission should remain distinguishable; the interface must not imply that all
power is globally available.

Bases eventually turn temporary reach into durable regional presence. Their growth
should be readable: foothold, power and communication, local supply, repair and
production, then specialized regional support. Connected bases strengthen the Mind's
coverage and coordination without making local constraints invisible.

Routine logistics may be automated after the player proves the underlying route or
process. Automation should elevate decisions toward priorities, reserves, acceptable
risk, and intervention thresholds rather than remove strategy.

### Research, knowledge, and loss

Research consumes observations, samples, operating experience, and infrastructure
milestones to open a small number of understandable choices. New directions should
create different strategies instead of one mandatory order.

Material state and knowledge obey different loss rules:

- probes, cargo, facilities, routes, stockpiles, and recent plans may be lost;
- completed research, first discoveries, designated permanent capabilities, and
  archived evidence remain known.

This distinction permits meaningful setbacks without making rare discoveries
repeatable chores. A canonical main save must not use a prestige reset that erases
knowledge or requires the player to rediscover it.

## 5. Interface Contract

Substrate is a keyboard-first graphical TUI. Mouse support may arrive later, but no
core action may depend on it. Common actions should use selection and contextual
controls, every screen should provide help, unavailable actions should explain their
blockers, and destructive actions should require a clear confirmation.

The workspace model is:

- **Planet:** the globe and complete selected-region information; all region-specific
  actions originate here.
- **Overview:** deeper planet-wide materials, fleet, energy, construction, routes,
  bottlenecks, and reports without duplicating regional commands.
- **Research:** available directions, requirements, active work, completed knowledge,
  and operational profiles.
- **System:** connection state, daemon telemetry, diagnostics, and engine events.
- **Archive:** a future permanent record of approved discoveries and their
  player-visible context.

The interface should preserve:

- a clear objective and essential operational health across workspaces;
- summary-first presentation with details on demand;
- restrained animation for rotation, signals, paths, scans, and production;
- automatic globe rotation after inactivity without changing selection or game state;
- immediate player control on the next input;
- distinct textual and symbolic state cues rather than color alone;
- wide, narrow, ASCII, monochrome, disconnected, stale, and reconnecting states;
- terminal restoration and detach-without-shutdown behavior.

## 6. Story and Discovery

Public story work is constrained to explicitly approved player-facing observations.
The repository must never contain unreleased answers, suggestive internal names,
private source paths, encoded payloads, unused future content, or tests and
documentation that reveal more than the player can know.

Only explicitly approved player-facing observations may be implemented. Public names
must be neutral, and every client or external tool may receive only discovered,
player-visible evidence.

Approved discoveries should:

- present incomplete but truthful observations;
- preserve more than one fair interpretation at their reveal stage;
- use varied media such as sensor anomalies, material evidence, diagrams, objects,
  experiments, short recovered text, or uncertain impressions;
- connect to exploration, research, or another meaningful mechanic when appropriate;
- follow progression eligibility with bounded guarantee pacing rather than unbounded
  luck or repetitive grinding;
- remain permanently archived once discovered;
- never be established or interpreted canonically by external model output.

Material failure may block access to a site, but it cannot remove recorded story
evidence or create a narrative dead end.

## 7. External Co-Processor

A player may eventually connect a provider-neutral external LLM as an optional
co-processor. Substrate does not embed a model, select a provider, accept provider
credentials, or require outside intelligence for any objective.

External assistance should unfold through research-gated capabilities:

1. observe player-visible status and evidence;
2. compare plans without mutating the game;
3. use specialized typed operations;
4. perform routine work within explicit policies;
5. respond to optional player-controlled heartbeat pulses.

Each gameplay capability should be paired with a versioned SKILL that explains its
public mechanics and safe use. Possessing instructions cannot bypass an engine-owned
unlock.

The player must be able to inspect, narrow, suspend, and revoke automation policy.
Policies constrain action categories, regions, resources, reserves, risk, duration,
and approvals. Every permitted operation follows the same costs, rules, persistence,
and failure handling as an action initiated through the TUI.

External reflections are untrusted, non-canonical, and separate from the permanent
archive. They cannot reveal hidden state, select an official interpretation, or gate
progression. If no compatible client is connected, the complete game remains playable
without penalty.

## 8. Technical Product Boundaries

- `mind-daemon` is the sole authority for simulation time, world state, risk,
  progression, persistence, automation policy, and story eligibility.
- Frontends render authoritative snapshots and submit typed actions; they never own or
  advance game state.
- IPC and MCP adapt the same engine operations. Neither transport may implement an
  alternate simulation or expose undiscovered truth.
- Gameplay uses typed domain operations, never a generic command or natural-language
  executor.
- Authoritative state is versioned and atomically persisted. Accepted outcomes survive
  reconnects and restarts.
- Transport remains local-only unless the security model is deliberately redesigned.
- The attach-only TUI reconnects automatically, marks retained snapshots as stale, and
  never stops the daemon when it detaches.
- Linux is the current acceptance target while paths and process behavior remain
  portable enough for later platform validation.

Exact protocol, schema, configuration, and operational details are current
implementation facts and therefore live in `AGENTS.md`, `README.md`, and the owning
repository skills.

## 9. Roadmap

### Current: first playable alpha

The repository has completed Phase 0's technical foundation and Phase 1's first
globe-survey loop. It also contains focused slices of Phase 2's geological economy and
Phase 3's research and loss systems:

- graphical onboarding and one-command startup;
- a persistent regional planet with fog, surveys, probes, and mission progress;
- variable deposits, risky extraction, rebuilding, refining, and initial range
  infrastructure;
- competing research directions and operational profiles;
- earned survey and extraction automation;
- local-first Planet interaction, global Overview detail, return summaries, and
  contextual guidance.

The README is the authority for the exact behavior and controls available in the
current build.

### Next: prove and deepen the alpha

The immediate priority is to make the existing 60–90 minute learning arc intuitive,
well paced, visually compelling, and resilient under real play:

1. Playtest onboarding, navigation, objectives, blockers, return summaries, and the
   path from surveying through research and automation.
2. Tune operation durations, costs, resource distribution, risk, recovery, and
   information density from observed play rather than adding rewards to mask friction.
3. Complete the geological and research slices with energy, transport, loadout
   tradeoffs, environmental requirements, damage, recovery, and salvage.
4. Expand automation only where players have demonstrated understanding and meaningful
   choices remain visible.
5. Preserve restart continuity, fallback rendering, reconnect behavior, and
   spoiler-safe data boundaries while the systems deepen.

This stage is complete when a new player can progress through the alpha without an
external guide, explain their current goal and blockers, recover from material loss,
and leave with more than one sensible next plan.

### Later: expand scale without duplicating chores

| Phase | Direction | Intended outcome |
| --- | --- | --- |
| 4 | Bases and planetary network | Temporary reach becomes durable, locally legible infrastructure with power, production, routes, failure, and recovery. |
| 5 | External co-processor | Optional research-gated tools, SKILLS, scoped policies, approvals, and audits assist without bypassing the game. |
| 6 | Breadcrumb progression | A permanent archive and approved, guarantee-paced discoveries connect investigation to planetary play. |
| 7 | Breadth and longevity | More environments, probe roles, production choices, events, visual effects, balance depth, and optional pulses support long-running saves. |

Later phases should unfold only after the preceding loop is understandable. New
complexity should reframe an established activity or open a genuinely different
decision, not duplicate planetary chores or add currencies without strategic purpose.

## 10. Definition of Success

Substrate succeeds when:

- a new player can awaken, read the globe, and launch a useful operation without a
  command manual;
- a returning player can understand what changed and choose a useful next action
  within minutes;
- a relaxed session produces meaningful progress without constant supervision;
- a dedicated player can optimize exploration, production, infrastructure, and
  automation without mandatory micromanagement;
- failures matter but do not erase permanent knowledge or make a save unwinnable;
- rare discoveries remain coherent, fair, and compelling across a long-running save;
- the interface remains visually distinctive and readable across supported terminal
  sizes and fallbacks;
- external assistance, when enabled, is useful, optional, revocable, and limited to
  the player's unlocked and visible capabilities.
