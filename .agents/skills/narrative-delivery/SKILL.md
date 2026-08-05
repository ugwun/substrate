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

## Client and co-processor boundary

- Expose only discovered, player-visible evidence through TUI, IPC, and MCP.
- Keep eligibility counters, guarantee progress, hidden interpretations, and unreleased content out of client DTOs.
- Keep external co-processor reflections in a separate non-canonical journal; never use model output to establish lore or gate story progression.
- Keep gameplay SKILLS free of unreleased narrative information.

Test eligibility, guarantee thresholds, ordering, deduplication, permanent archive behavior, rollback survival, hidden-state omission, disconnected/reconnected presentation, neutral serialized identifiers, and spoiler scans across code, tests, fixtures, docs, MCP descriptions, and exported SKILLS.
