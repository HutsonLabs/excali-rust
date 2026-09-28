# Working in this repository

Read `site/content/plan/agent-workflow.md` (or the published Plan → Agent workflow page). The short form:

1. `./scripts/bootstrap.sh` once per clone.
2. `bd ready` → `bd update <id> --claim` → `git switch -c <id>/<slug>`.
3. Read the issue's evidence before writing code; upstream at the pinned commit is the spec.
4. Tests with every change. No skipped tests.
5. PR body has an `## Evidence` section (paths, URLs with dates, commands and results).
6. Hooks and CI run `scripts/gates/attribution.py`. Commits are authored by the responsible human; no tool attribution, trailers, session links or invisible characters. Do not install `bd hooks`.
7. `bd close <id> --reason "..."` in the merge commit; blocked on a human → `bd note`, label `needs-human`, `bd unclaim`.

Task graph changes go in `plan/tasks.json`, then `scripts/tasks/seed.py`.
