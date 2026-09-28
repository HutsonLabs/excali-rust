+++
title = "ADR-005: Authorship gate"
description = "Everything in this repository is authored by the responsible human. Tooling leaves no attribution, session link, signature or invisible mark, and a script enforces it at every step."
weight = 5
+++

**Status.** Accepted, 2026-09-28. Enforced.

## Rule

Dr. Hutson has full responsibility for everything produced here. Editors, assistants and agents are tools, not authors. No commit, file, comment or generated page may carry a tool's attribution, a session link, a footer, a signature from a tool-held key, or an invisible watermark.

## Enforcement

One script, `scripts/gates/attribution.py`, run at four points:

| Point | Command | Effect |
|---|---|---|
| `.githooks/commit-msg` | `message <file>` | strips attribution trailers and footers deterministically, then refuses the commit if anything remains |
| `.githooks/pre-commit` | `files --staged` | refuses staged files with attribution, vendor mentions outside the allowlist, or invisible code points |
| `.githooks/pre-push` | `history <range>` | refuses to push commits whose author, committer or message fail the same rules |
| `.github/workflows/gates.yml` | `files --all` and `history base..HEAD` | the same checks on every pull request, so `--no-verify` does not help |

Rules: R1 attribution trailers, R2 tool footers and session URLs, R3 vendor identities and domains, R4 any bare mention of the vendor or tool name (opt-in per line with `gate:allow-mention` or per file in `scripts/gates/attribution-allow.txt`), R5 invisible code points (zero-width, bidi controls, tag characters, variation selectors, soft hyphen, BOM anywhere but offset 0).

`scripts/bootstrap.sh` sets the repository-local identity to the responsible human, disables commit signing (hosted agent containers sign with a key registered to the tool vendor, which is an identity marker), and points `core.hooksPath` at `.githooks`.

## Evidence

- The termhut.hutsonlabs.com repository already carries a `commit-msg` hook that strips the same trailers; this gate generalises it and adds file scanning, history scanning and CI.
- Self-tests run on 2026-09-28: a planted `Co-Authored-By` trailer was stripped; a planted zero-width space in a staged file was rejected; the gate's own source is pure ASCII so it passes itself.

## Consequences

- Prose on this site refers to "agents" and "the agent CLI" rather than naming products. Quoted upstream text that names a product lives only in the evidence page, which is allowlisted for R4.
- beads runs with `BEADS_ACTOR` set to the human's name; its issue export therefore carries the human as `created_by`, and the gate scans that export too.
