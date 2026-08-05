---
name: agent-guidance
description: Maintain Substrate repository instructions and skills. Use when changing AGENTS.md, creating, updating, renaming, or deleting repository skills, generating skill metadata or resources, auditing guidance consistency, or changing architecture and workflows that require agent documentation updates.
---

# Agent guidance

## Assess every substantive change

- Read `AGENTS.md`, `GAME_PLAN.md`, current code and documentation, and all matching repository skills before changing guidance.
- Separate implemented architecture from future product direction; never describe planned behavior as current.
- Check whether the task changes authority, invariants, persisted state, public interfaces, workflows, commands, tests, security, narrative policy, or reusable domain knowledge.
- Update guidance proactively in the same change rather than waiting for a follow-up request.

## Choose the correct guidance action

- Update an existing skill when it already owns the changed subsystem.
- Create a skill for a distinct subsystem or reusable non-obvious workflow with no clear owner.
- Avoid micro-skills for local details that fit an existing owner.
- Delete a skill only after its subsystem, public surface, migrations, and maintenance obligations are gone.
- Remove stale instructions instead of preserving contradictory current and obsolete rules.

## Build skill packages

- Use the available `skill-creator` tooling to initialize new `.agents/skills/<hyphenated-name>` packages and generate `agents/openai.yaml`.
- Keep `SKILL.md` concise and imperative with YAML frontmatter containing only `name` and a trigger-complete `description`.
- Add directly linked `references/`, `scripts/`, or `assets/` only when progressive disclosure or deterministic reuse justifies them.
- Keep display name, short description, and `$skill-name` default prompt synchronized with the skill.
- Never include private narrative sources, hidden canon, credentials, or machine-specific private paths.

## Synchronize and validate

- Update `AGENTS.md` when architecture summaries, authority boundaries, protocols, tools, workflows, commands, acceptance targets, security, product transitions, narrative policy, or skill routing changes.
- Update README for implemented behavior and `GAME_PLAN.md` only for deliberate product-policy changes.
- Run the skill-creator validator for every added or modified skill and scan for unfinished placeholders, stale names, malformed default prompts, and forbidden private information.
- Run `git diff --check` and the repository's required formatting, Clippy, and workspace tests before handoff.
