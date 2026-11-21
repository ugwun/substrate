---
name: narrative-delivery
description: Implement Substrate's public story breadcrumbs safely. Use when adding or changing fragments, anomalies, recovered text, archive records, story eligibility, hidden guarantee pacing, narrative objectives, terminal story presentation, or tests and identifiers that may reveal unreleased lore.
---

# Spoiler-safe narrative delivery

## Approval and secrecy

- Treat the public repository as fully inspectable. Never commit unreleased resolution, identity answers, private source paths, suggestive internal names, encoded spoilers, or unused future content.
- Implement only creator-approved player-facing breadcrumbs. Do not invent a canonical fact while writing code, tests, fixtures, documentation, MCP resources, or achievements.
- Use neutral identifiers based on sequence or public observation, not hidden meaning.
- Keep private narrative guidance outside the repository. Do not name or link it in public files.
- Stop and request approval when an implementation requires facts beyond the approved breadcrumb.

## Delivery contract

- Represent breadcrumbs as daemon-owned eligibility, discovery, and permanent archive state.
- Make discoveries rare in presentation but prevent indefinite bad luck through progression gates and hidden guarantee thresholds.
- Preserve required ordering, never award a fragment twice as new, and retain archived evidence through material loss.
- Deliver evidence through varied media—sensor anomalies, material observations, short recovered text, diagrams, terminal art, experiments, and the Mind's uncertain interpretation—rather than exposition-only logs.
- Keep ambiguity truthful: player observations remain valid after later reveals, while incomplete context permits multiple interpretations.
- Let approved discoveries affect research or investigation when appropriate without announcing their later meaning.
- Preserve the implemented five-record campaign spine: onboarding records the first observation; three focused local surveys including a sheltered site establish the next comparison (unlockable right after the first regional survey); two research directions and a powered laboratory expose a 75-second archive experiment; three subsequent independent-region surveys provide its quiet comparative interval; the concluding observation and read-only map arrive together only after the planetary horizon is ready. Derive the five-chapter campaign headers and foreground questions strictly from these approved public observations without adding unapproved lore or stating conclusions early.
- Deliver each newly discovered observation as a durable full-screen Discovery Transmission (`presented_archive_count` cursor in save schema 11, acknowledged via `mind.acknowledge_archive_presentation`) with neutral measurement diagrams and strict visual separation between `OBSERVATION` and `CURRENT INTERPRETATION`. Protect unreviewed milestone entries during activity-log eviction and never let return-summary dismissal discard unpresented transmissions.
- Pause the archive experiment without losing progress when its laboratory becomes unavailable. Keep normal field operations concurrent, grant its visible analysis result only on completion, and never let migration, reconnect, load shedding, or material rollback duplicate or erase an archived observation.

## Client and co-processor boundary

- Expose only discovered, player-visible evidence through TUI, IPC, and MCP.
- Keep hidden eligibility counters, hidden guarantee progress, hidden interpretations, and unreleased content out of client DTOs. Player-visible progress toward already-announced campaign objectives (such as Chapter III's post-comparison regional survey count) may be shown when derived strictly from actions the player performed.
- Keep external co-processor reflections in a separate non-canonical journal; never use model output to establish lore or gate story progression.
- Keep gameplay SKILLS free of unreleased narrative information.
- Keep the current system-map reveal observational and minimal: publish only player-visible local-frame results, leave outer returns unresolved, and expose no hidden system composition or later interpretation.

Test eligibility, guarantee thresholds, ordering, deduplication, permanent archive behavior, rollback survival, hidden-state omission, disconnected/reconnected presentation, neutral serialized identifiers, and spoiler scans across code, tests, fixtures, docs, MCP descriptions, and exported SKILLS.
