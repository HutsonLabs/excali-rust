+++
title = "Agent workflow"
description = "How an autonomous team, or a person, takes work from the tracker to a merged pull request without losing evidence or authorship."
weight = 3
+++

## Tracker

Work is tracked with **beads** (`bd`), a git-native issue tracker with first-class dependencies and a ready queue, chosen in [ADR-001](../../decisions/adr-001-task-tracking/). The database is local (embedded Dolt under `.beads/`, ignored by git); the shared, reviewable state is `.beads/issues.jsonl`, which the pre-commit hook re-exports on every commit and which this site renders on the [progress page](../progress/).

The task graph is authored in `plan/tasks.json` and pushed into the tracker with `scripts/tasks/seed.py`. Editing that file and re-running the seed is how priorities, acceptance criteria and dependencies change; the seed is an upsert, so it never deletes work someone has started.

## Setup on a fresh clone

```sh
git clone https://github.com/HutsonLabs/excali-rust
cd excali-rust
./scripts/bootstrap.sh     # identity, hooks, bd, zola
bd ready                   # what can be started now
```

`bootstrap.sh` is idempotent. It sets the repository-local git identity to the responsible human, points `core.hooksPath` at `.githooks`, installs `bd` if missing, turns its metrics off, rebuilds the local database from `issues.jsonl`, pins Zola (downloaded once per clone into the main checkout's `.tools/`, shared by every worktree, and refused unless its SHA-256 matches the digest pinned for the platform; macOS `/bin/bash` 3.2 and Linux are both supported), and checks out upstream at the pinned commit (`EXCALI_SKIP_UPSTREAM=1` skips it).

## The loop

1. **Pick.** `bd ready` lists issues with no open blockers, priority first. Take the lowest-numbered ready issue in the earliest open phase unless a label says otherwise.
2. **Claim.** `bd update <id> --claim`. The claim is atomic; a second agent gets a refusal, not a race.
3. **Branch.** `git switch -c <id>/<slug>` from `main`. One issue per branch.
4. **Read the evidence.** The issue's `design` and `acceptance_criteria` fields cite the research pages and upstream paths. Read those paths in the upstream checkout at the pinned commit before writing code. `scripts/upstream/checkout.sh` creates that checkout in the main clone's `.tools/upstream` (shared by every worktree), detaches it at `extra.upstream_commit` from `site/config.toml`, and refuses any other commit unless `PIN=<sha>` is set explicitly; `--verify` checks it is clean at the pin. If the evidence is missing, stop and file a `spike` issue rather than guessing.
5. **Do the work with tests.** Every issue ships with tests: `cargo test` for Rust, the Playwright suite for the web runtime, the golden harness for rendering. An issue whose behaviour cannot be tested says so in its close note and why.
6. **Record evidence in the PR.** The PR body has a section `## Evidence` listing the upstream files and lines, external sources (URL and date), and the commands run with their results. A reviewer must be able to check each claim without asking.
7. **Gates.** Commit hooks run the authorship gate and refresh `issues.jsonl`. CI runs the same gate, `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, and the site build, plus the site smoke suite (`tests/site`, Playwright): every page and mockup must load without console errors, failed requests or HTTP errors at 1440×900, 1024×768 and 390×844. Run it locally with `cd tests/site && npm ci && npx playwright install chromium && npm test`. A push that turns CI red is fixed on the same branch before anything else.
8. **Close.** `bd close <id> --reason "<one line>"` in the same commit that merges the PR, and add the PR URL with `bd update <id> --external-ref`.
9. **Hand back.** If blocked on a human decision, add a note (`bd note <id> "..."`) , label it `needs-human`, and unclaim (`bd unclaim <id>`). Do not park work silently.

## Rules that do not bend

- **Authorship.** Commits are authored by the responsible human. No tool attribution, no session links, no signatures from a tool's key, no invisible characters. The gate enforces this; CI enforces it again ([ADR-005](../../decisions/adr-005-authorship-gate/)).
- **Evidence before opinion.** No design note, estimate or "should" without a cited source. The [evidence log](../../evidence/) is the pattern.
- **Upstream is the spec.** When the port and upstream disagree on behaviour, upstream at the pinned commit wins, unless an ADR records the deviation.
- **Primitives first.** Build the smallest pure function, test it, then compose. `excali-core` and `excali-math` must never depend on DOM, `std::fs` or a renderer.
- **No skipped tests to get green.** A flaky test is made robust or replaced; it is never quarantined.
- **One PR per issue**, one issue per commit where practical, and `Closes` is not used with GitHub issue numbers because the tracker is beads; the beads id goes in the commit subject instead: `ex-104: port getCornerRadius`.

## Parallelism

Beads ids are hash-based, so two agents creating issues on different branches do not collide. Phases 1–4 are independent of Phase 5's DOM work once the display-list crate's interface (`ex-e2`) is stable, so the graph is deliberately wide there. When claims conflict, the earlier claim stands; the later agent takes the next ready issue.

## Human checkpoints

The tracker has milestone issues (`ex-m0` … `ex-m8`). Each is closed only by a human after reviewing the acceptance evidence. Agents may do everything up to that point.
