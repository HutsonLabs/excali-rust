# excali-rust

A Rust port of the Excalidraw editor: no React, Tauri-ready, `.excalidraw` compatible, built to drop into [term.hut](https://github.com/HutsonLabs/term.hut) as a vendored module.

**Status:** planning. The review surface is the site in `site/`, published to GitHub Pages: plan, architecture, design system, mockups, decisions, research and the evidence log. No editor code exists yet; Phase 0 tasks are ready in the tracker.

## Layout

| Path | What |
|---|---|
| `site/` | Zola site (the plan). `scripts/site/zola.sh build` renders it to `site/public/`. |
| `plan/tasks.json` | The authored task graph: 9 epics, 112 tasks, 9 milestones with dependencies and acceptance criteria. |
| `.beads/` | beads tracker. `issues.jsonl` is the tracked export; the database is local and ignored. |
| `scripts/bootstrap.sh` | Idempotent setup: git identity, hooks, `bd`, Zola. Run this first. |
| `scripts/gates/attribution.py` | The authorship gate (hooks and CI). |
| `scripts/tasks/seed.py` | Upserts `plan/tasks.json` into beads and refreshes the export and the progress page. |
| `scripts/tasks/render-progress.py` | Renders the export into `site/content/plan/progress.md`. |
| `.githooks/` | `pre-commit`, `commit-msg`, `pre-push` (activated by bootstrap). |
| `.github/workflows/` | `gates.yml` (authorship gate on PRs) and `pages.yml` (site build and deploy). |

## Start

```sh
./scripts/bootstrap.sh
bd ready                      # tasks with no open blockers
./scripts/site/zola.sh serve  # http://127.0.0.1:1111
```

Working rules are on the site under Plan → Agent workflow. In one line: pick from `bd ready`, claim, branch per issue, cite evidence in the PR, pass the gates, close the issue in the merge commit.

## Licence

MIT, as upstream Excalidraw. See `LICENSE`.
