# Working in this repository

Read `site/content/plan/agent-workflow.md` (or the published Plan → Agent workflow page). The short form:

1. `./scripts/bootstrap.sh` once per clone.
2. `bd ready` → `bd update <id> --claim` → `git switch -c <id>/<slug>`.
3. Read the issue's evidence before writing code; upstream at the pinned commit is the spec.
4. Tests with every change. No skipped tests.
5. PR body has an `## Evidence` section (paths, URLs with dates, commands and results).
6. Hooks and CI run `scripts/gates/attribution.py`. Commits are authored by the responsible human; no tool attribution, trailers, session links or invisible characters. Do not install `bd hooks`.
7. PRs carry code, tests and docs only: never `.beads/issues.jsonl`, `site/content/plan/progress.md` or `site/content/plan/build-log.md` (`scripts/gates/tracker_files.py` fails the PR). After the merge the integrator, from the main clone, runs `bd close <id> --reason "..."` and `bd update <id> --external-ref <PR URL>`, adds the build-log entry and pushes one commit `<id>: close in tracker, build log` to main. Blocked on a human → `bd note`, label `needs-human`, `bd unclaim`.

Task graph changes go in `plan/tasks.json`, then `scripts/tasks/seed.py`.

## This is strictly a port

No commits, pushes, PRs, issues, discussions or comments on upstream Excalidraw (`excalidraw/excalidraw`, `excalidraw-libraries` or any other excalidraw org repository). The upstream checkout under `.tools/upstream` is read-only reference: `scripts/upstream/checkout.sh` sets its push URL to `DISABLED-strictly-a-port` and `--verify` fails if that changes. Owner decision, 2026-09-27.

## Releases

Versions are calendar `YY.M.BUILD` (ADR-009); the first release is `26.9.1`, tag `v26.9.1`. Agents may create tags and GitHub releases (`gh release create`) with the ES module + WASM tarball. Do not publish to crates.io, npm or any other registry (`ex-801` is deferred by the owner).
