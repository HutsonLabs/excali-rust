+++
title = "ADR-001: Task tracking with beads"
description = "Why the task graph lives in beads inside the repository rather than in GitHub Issues or a document."
weight = 1
+++

**Status.** Accepted, 2026-09-28.

## Question

How should an autonomous, multi-agent team track work so that any agent can pick the next task, dependencies are explicit, progress is reviewable by a human on this site, and nothing depends on a hosted service being reachable from a container?

## Evidence

- beads (`bd`) describes itself as a "Distributed graph issue tracker for AI agents, powered by Dolt" with "Hash-based IDs (bd-a1b2) prevent merge collisions in multi-agent/multi-branch workflows" and `bd ready` / `bd update <id> --claim` for an atomic ready queue (README of `steveyegge/beads`, read 2026-09-28).
- Installed and exercised here: `bd version 1.3.0`; `bd init`, `bd create --id`, `bd dep add`, `bd import` (upsert, with `parent-child` and `blocks` dependencies), `bd export`, `bd ready`, `bd dep cycles` all behaved as documented on a scratch repository (command outputs in the [evidence log](../../evidence/)).
- term.hut's own plans use GitHub Issues with "One commit per issue … `Closes #N`" (`docs/plans/agentic-os.md`). That works for a single human-driven repo but has no dependency graph or ready queue, and needs network access to GitHub for every read.
- beads writes `.beads/issues.jsonl` as an interchange export ("not the source of truth or a backup", per its `bd init` help) and its `bootstrap` command rebuilds a database from that file on a fresh clone.

## Options

1. **GitHub Issues + Projects.** Familiar; no dependency semantics beyond task lists; every agent call is a network call; hard to snapshot into this site.
2. **A Markdown checklist in the repo.** Reviewable, but no claiming, no ready queue, merge conflicts on every edit.
3. **beads with the JSONL export committed.** Dependency graph, ready queue, atomic claims, offline, hash ids; the export renders into the site; a human-authored `plan/tasks.json` seeds it.

## Decision

Option 3. `plan/tasks.json` is the authored graph; `scripts/tasks/seed.py` upserts it into beads; the pre-commit hook re-exports `.beads/issues.jsonl` and regenerates the progress page; `bd bootstrap` rebuilds the database on a fresh clone. beads' own git hooks are **not** installed because its `prepare-commit-msg` hook "Add[s] agent identity trailers for forensics", which conflicts with [ADR-005](../adr-005-authorship-gate/). Metrics are turned off by `bootstrap.sh`.

## Consequences

- Agents run `bd ready`, claim, and close; humans read the [progress page](../../plan/progress/).
- The Dolt database is per-clone. Cross-clone state flows through the committed JSONL, so a claim made in one clone is visible elsewhere only after a commit lands. Acceptable for a repo where each agent works on its own branch and PR.
- `sync.remote` was removed from `.beads/config.yaml` so `bd bootstrap` does not try to clone Dolt refs that are never pushed.

## Reversal

If beads' JSONL import stops being upsert-safe, or if the team needs shared live claims across machines, mirror epics to GitHub Issues with `--external-ref` and revisit.
