+++
title = "ADR-004: Fonts are vendored only with a verified licence"
description = "Which font files ship with the module, and the rule that no family is bundled until its licence is recorded from its source."
weight = 4
+++

**Status.** Accepted as a rule; per-family verification pending (`ex-306`).

## Question

Excalidraw's look depends on Excalifont, Virgil, Nunito, Comic Shanns, Lilita One, Cascadia, Liberation Sans, Assistant and the Xiaolai CJK fallback. Which may the port ship?

## Evidence

- Upstream's repo contains licence notices in code for Excalifont (SIL OFL 1.1, `fonts/Excalifont/index.ts:11-117`), Xiaolai (SIL OFL 1.1, `fonts/Xiaolai/index.ts:216-232`) and Comic Shanns (MIT, `fonts/ComicShanns/index.ts:11-14,45`). For Virgil, Cascadia, Nunito, Lilita One, Liberation Sans and Assistant there is no licence file under `packages/excalidraw/fonts` ([research §1.3](../../research/ui-design-system/#13-fonts)).
- Helvetica and Segoe UI Emoji are `local:` only upstream and are never bundled.
- term.hut ships no fonts for its viewer and maps families to local stacks (`excalidrawScene.js:42-59`).

## Decision

1. Ship Excalifont, Xiaolai and Comic Shanns from the start under their recorded licences, range-split as upstream ships them.
2. For each remaining family, task `ex-306` records licence, source URL and date on this page before the file is added. Until then the family resolves through the same local stacks term.hut uses.
3. Licence texts ship in the release tarball next to the font files.

## Consequences

Text set in an unverified family may render in a fallback face until `ex-306` closes, which is the behaviour term.hut users already have.

## Verification table

| Family | Licence | Source | Date | Verified by |
|---|---|---|---|---|
| Excalifont | SIL OFL 1.1 | upstream `fonts/Excalifont/index.ts` | 2026-09-28 | research inventory |
| Xiaolai | SIL OFL 1.1 | upstream `fonts/Xiaolai/index.ts` | 2026-09-28 | research inventory |
| Comic Shanns | MIT | upstream `fonts/ComicShanns/index.ts` | 2026-09-28 | research inventory |
| Virgil | pending | | | |
| Nunito | pending | | | |
| Lilita One | pending | | | |
| Cascadia Code | pending | | | |
| Liberation Sans | pending | | | |
| Assistant | pending | | | |
