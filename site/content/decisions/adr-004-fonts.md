+++
title = "ADR-004: Fonts are vendored only with a verified licence"
description = "Which font files ship with the module, the rule that no family is bundled until its licence is recorded from its source, and the fallback for families whose licence is not confirmed."
weight = 4
+++

**Status.** Accepted as a rule; fallback rule added by owner decision of 2026-09-27 (`ex-008`); per-family verification pending (`ex-306`).

## Question

Excalidraw's look depends on Excalifont, Virgil, Nunito, Comic Shanns, Lilita One, Cascadia, Liberation Sans, Assistant and the Xiaolai CJK fallback. Which may the port ship, and what happens to text set in a family that may not?

## Evidence

- Upstream's repo contains licence notices in code for Excalifont (SIL OFL 1.1, `fonts/Excalifont/index.ts:11-117`), Xiaolai (SIL OFL 1.1, `fonts/Xiaolai/index.ts:216-232`) and Comic Shanns (MIT, `fonts/ComicShanns/index.ts:11-14,45`). For Virgil, Cascadia, Nunito, Lilita One, Liberation Sans and Assistant there is no licence file under `packages/excalidraw/fonts` ([research §1.3](../../research/ui-design-system/#13-fonts)).
- Helvetica and Segoe UI Emoji are `local:` only upstream and are never bundled.
- Upstream's own fallback chain classifies every family as monospace (Cascadia, Comic Shanns) or sans-serif (all others) in `getGenericFontFamilyFallback`, and appends that generic family and Segoe UI Emoji after the requested face (`packages/common/src/constants.ts:169-198`); Excalifont additionally falls back to Xiaolai. Excalifont is the default family (`DEFAULT_FONT_FAMILY`, `constants.ts:268`).
- term.hut ships no fonts for its viewer and maps families to local stacks (`excalidrawScene.js:42-59`).
- Owner decision (d), 2026-09-27: vendor only families with a confirmed OFL, MIT or Apache licence; map any unconfirmed family to a licensed fallback, record the gap here, and continue.

## Decision

1. Ship Excalifont, Xiaolai and Comic Shanns from the start under their recorded licences, range-split as upstream ships them.
2. A family is vendored only when its licence is confirmed as SIL OFL 1.1, MIT or Apache 2.0 from its source, with licence, source URL and date recorded in the verification table below before the file is added (`ex-306`).
3. **Fallback rule.** A family whose licence is not confirmed is not vendored. It maps to a licensed fallback in the same class upstream's `getGenericFontFamilyFallback` uses, and the gap is recorded in [Licence gaps](#licence-gaps):
   - hand-drawn (Virgil) → Excalifont (SIL OFL 1.1), upstream's default family;
   - monospace (Cascadia Code) → Comic Shanns (MIT), the other family upstream classes as monospace;
   - sans-serif (Nunito, Lilita One, Liberation Sans, Assistant) → the first sans-serif family whose licence is confirmed in the table; until one is, the host's local `sans-serif` stack, which ships nothing (the same local stacks term.hut uses).
   The element keeps its original `fontFamily` value, so the file round-trips unchanged and the real face is used as soon as the family is confirmed. Work continues; an unconfirmed licence is not a reason to stop or to wait on a human.
4. Licence texts ship in the release tarball next to the font files.

## Consequences

Text set in an unconfirmed family renders and measures in its fallback face until `ex-306` confirms the family, which is the behaviour term.hut users already have. Width and height measured for such text can differ from the values upstream stored with the element.

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

## Licence gaps

Families not vendored because their licence is not yet confirmed, and the fallback each maps to under rule 3. A row is removed when the family's licence is confirmed above and its files are vendored. Recorded 2026-09-27.

| Family | Upstream id | Class | Fallback | Fallback licence |
|---|---|---|---|---|
| Virgil | 1 | hand-drawn | Excalifont | SIL OFL 1.1 |
| Cascadia Code | 3 | monospace | Comic Shanns | MIT |
| Nunito | 6 | sans-serif | host `sans-serif` (nothing shipped) | n/a |
| Lilita One | 7 | sans-serif | host `sans-serif` (nothing shipped) | n/a |
| Liberation Sans | 9 | sans-serif | host `sans-serif` (nothing shipped) | n/a |
| Assistant | 10 | sans-serif | host `sans-serif` (nothing shipped) | n/a |

Helvetica (2) and Segoe UI Emoji are `local:` only upstream and are never vendored, so they are not gaps.
