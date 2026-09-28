+++
title = "ADR-004: Fonts are vendored only with a verified licence"
description = "Which font files ship with the module, the rule that no family is bundled until its licence is recorded from its source, and the fallback for families whose licence is not confirmed."
weight = 4
+++

**Status.** Accepted. Fallback rule added by owner decision of 2026-09-27 (`ex-008`). Every family verified at its source on 2026-09-28 (`ex-306`): all nine bundled families are SIL OFL 1.1 or MIT; one upstream file, Liberation Sans 1.05, is not, and is replaced by the OFL build of the same family. `scripts/gates/fonts.py` enforces the tables below in CI.

## Question

Excalidraw's look depends on Excalifont, Virgil, Nunito, Comic Shanns, Lilita One, Cascadia, Liberation Sans, Assistant and the Xiaolai CJK fallback. Which may the port ship, and what happens to text set in a family that may not?

## Evidence

- Upstream's repo contains licence notices in code for Excalifont (SIL OFL 1.1, `fonts/Excalifont/index.ts:11-117`), Xiaolai (SIL OFL 1.1, `fonts/Xiaolai/index.ts:216-232`) and Comic Shanns (MIT, `fonts/ComicShanns/index.ts:11-14,45`). For Virgil, Cascadia, Nunito, Lilita One, Liberation Sans and Assistant there is no licence file under `packages/excalidraw/fonts` ([research §1.3](../../research/ui-design-system/#13-fonts)).
- Helvetica and Segoe UI Emoji are `local:` only upstream and are never bundled (`fonts/Helvetica/index.ts` and `fonts/Emoji/index.ts`: `uri: LOCAL_FONT_PROTOCOL`).
- Upstream's own fallback chain classifies every family as monospace (Cascadia, Comic Shanns) or sans-serif (all others) in `getGenericFontFamilyFallback`, and appends that generic family and Segoe UI Emoji after the requested face (`packages/common/src/constants.ts:169-198`); Excalifont additionally falls back to Xiaolai. Excalifont is the default family (`DEFAULT_FONT_FAMILY`, `constants.ts:268`).
- term.hut ships no fonts for its viewer and maps families to local stacks (`excalidrawScene.js:42-59`).
- Owner decision (d), 2026-09-27: vendor only families with a confirmed OFL, MIT or Apache licence; map any unconfirmed family to a licensed fallback, record the gap here, and continue.
- Verification, 2026-09-28 (`ex-306`). For each family: the licence file at the family's own source, fetched that day, and the name table of every upstream font file at the pinned commit (`packages/excalidraw/fonts/*/*.woff2`, read with fontTools 4.60.2: name IDs 0 copyright, 5 version, 13 licence, 14 licence URL). The upstream Excalidraw repository is not the source for any family; its code comments are corroboration only.

## Decision

1. Ship every family whose licence is confirmed in the verification table, range-split as upstream ships them: Excalifont, Xiaolai, Comic Shanns, Virgil, Cascadia Code, Nunito, Lilita One and Assistant from upstream's files at the pinned commit, and Liberation Sans from release 2.1.5 of liberation-fonts (OFL) instead of upstream's 1.05 file (see [Licence gaps](#licence-gaps)).
2. A family is vendored only when its licence is confirmed as SIL OFL 1.1, MIT or Apache 2.0 from its source, with licence, source URL and date recorded in the verification table below before the file is added (`ex-306`).
3. **Fallback rule.** A family whose licence is not confirmed is not vendored. It maps to a licensed fallback, and the gap is recorded in [Licence gaps](#licence-gaps). Upstream's `getGenericFontFamilyFallback` (`packages/common/src/constants.ts:169-180`) has exactly two classes: monospace for Cascadia and Comic Shanns, and sans-serif as the default for every other family, Virgil (id 1) included. The fallback follows that class, with one deliberate exception:
   - monospace (Cascadia Code) → Comic Shanns (MIT), the other family upstream classes as monospace;
   - sans-serif (Nunito, Lilita One, Liberation Sans, Assistant) → the first sans-serif family whose licence is confirmed in the table; if none is, the host's local `sans-serif` stack, which ships nothing (the same local stacks term.hut uses);
   - Virgil → Excalifont (SIL OFL 1.1). This is a choice of the port, not upstream's class: upstream classes Virgil as sans-serif, but Virgil is its original hand-drawn face and Excalifont is the hand-drawn face upstream now uses by default (`DEFAULT_FONT_FAMILY = FONT_FAMILY.Excalifont`, `constants.ts:268`), so a hand-drawn drawing stays hand-drawn rather than turning into the host's sans-serif.
   An upstream file whose licence is not confirmed while its family's is (one particular build) maps to a confirmed build of the same family. The element keeps its original `fontFamily` value, so the file round-trips unchanged and the real face is used as soon as the family is confirmed. Work continues; an unconfirmed licence is not a reason to stop or to wait on a human.
4. Licence texts ship next to the font files, in the repository and in the release tarball. `scripts/gates/fonts.py check` fails CI when a font file in the tree belongs to no confirmed family, has no licence file carrying that licence's text beside it, or hashes to a file listed under Licence gaps; `fonts.py verify-upstream` checks the recorded hashes against the pinned upstream checkout.

## Consequences

Every family renders in its own face; nothing falls back to the host because of a licence. Liberation Sans is the one place the port's file differs from upstream's. Both builds have unitsPerEm 2048, hhea ascent 1854 and descent -434, OS/2 winAscent 1854 and winDescent 434, and the same advance width on 656 of the 658 code points both map. The two that differ are U+00B7 MIDDLE DOT (569 units in 1.05, 682 in 2.1.5) and U+2012 FIGURE DASH (682, 1139). 1.05 also maps U+2011 NON-BREAKING HYPHEN and the private-use U+F001, U+F002 and U+F005, which 2.1.5 does not; text using them in Liberation Sans (a `private` family upstream, not offered in the picker) measures through the fallback chain, and its width can differ from the value upstream stored with the element.

## Verification table

Checked 2026-09-28. "Vendored from" names the file the port ships; upstream paths are under `packages/excalidraw/` at the pinned commit.

| Family | Id | Licence | Source | Checked | Evidence | Vendored from |
|---|---|---|---|---|---|---|
| Virgil | 1 | SIL OFL 1.1 | <https://github.com/excalidraw/virgil/blob/main/LICENSE.md> | 2026-09-28 | LICENSE.md: "Copyright (c) 2021 - Present, Ellinor Rapp, with Reserved Font Name Virgil. This Font Software is licensed under the SIL Open Font License, Version 1.1"; repository licence OFL-1.1. Upstream `Virgil-Regular.woff2` (Version 001.001, copyright 2011 Your Own Font Foundry) carries the full OFL 1.1 text in name ID 13. | upstream `fonts/Virgil/` |
| Helvetica | 2 | local only | n/a | 2026-09-28 | `fonts/Helvetica/index.ts`: `uri: LOCAL_FONT_PROTOCOL`; no file upstream. | never vendored |
| Cascadia Code | 3 | SIL OFL 1.1 | <https://github.com/microsoft/cascadia-code/blob/v2005.15/LICENSE> | 2026-09-28 | Upstream `CascadiaCode-Regular.woff2` is Version 2005.150, release v2005.15 of 2020-05-15 (`FONTLOG.txt`). LICENSE at that tag: "Copyright (c) 2019 - Present, Microsoft Corporation, with Reserved Font Name Cascadia Code. This Font Software is licensed under the SIL Open Font License, Version 1.1". Name ID 14 is the OFL URL; name ID 13 holds Microsoft's generic "Microsoft supplied font" notice, which names no licence of its own. | upstream `fonts/Cascadia/` |
| Excalifont | 5 | SIL OFL 1.1 | <https://plus.excalidraw.com/excalifont> | 2026-09-28 | Publisher page: "Released under the OFL-1.1 license, Excalifont is freely available for both personal and commercial use", download "under OFL-1.1 license (included in the font file)". The origin name table quoted in `fonts/Excalifont/index.ts:11-117` carries the full OFL 1.1 text and `licenseURL: http://scripts.sil.org/OFL`; the subset woff2 files keep only name ID 0. There is no public source repository. | upstream `fonts/Excalifont/` |
| Nunito | 6 | SIL OFL 1.1 | <https://github.com/google/fonts/blob/main/ofl/nunito/OFL.txt> | 2026-09-28 | OFL.txt: "Copyright 2014 The Nunito Project Authors (https://github.com/googlefonts/nunito) ... licensed under the SIL Open Font License, Version 1.1". Upstream files: Version 3.602, same copyright, name ID 14 OFL URL. | upstream `fonts/Nunito/` |
| Lilita One | 7 | SIL OFL 1.1 | <https://github.com/google/fonts/blob/main/ofl/lilitaone/OFL.txt> | 2026-09-28 | OFL.txt: "Copyright (c) 2011 Juan Montoreano, with Reserved Font Name Lilita ... licensed under the SIL Open Font License, Version 1.1". Upstream files: Version 1.002, same copyright, name ID 14 OFL URL. | upstream `fonts/Lilita/` |
| Comic Shanns | 8 | MIT | <https://github.com/jesusmgg/comic-shanns-mono/blob/master/LICENSE.md> | 2026-09-28 | LICENSE.md: MIT License, "Copyright (c) 2018 Shannon Miwa, Copyright (c) 2023 Jesus Gonzalez"; repository licence MIT. The original, <https://github.com/shannpersand/comic-shanns/blob/master/LICENSE>, is MIT too. Upstream files: Comic Shanns Mono 1.3.0, full MIT text in name ID 0. | upstream `fonts/ComicShanns/` |
| Liberation Sans | 9 | SIL OFL 1.1 | <https://github.com/liberationfonts/liberation-fonts/blob/main/LICENSE> | 2026-09-28 | LICENSE (versions 2.x): "Copyright (c) 2012 Red Hat, Inc. with Reserved Font Name Liberation. This Font Software is licensed under the SIL Open Font License, Version 1.1". Release 2.1.5 of 2021-09-30, `liberation-fonts-ttf-2.1.5.tar.gz` sha256 `7191c669bf38899f73a2094ed00f7b800553364f90e2637010a69c0e268f25d0`; its `LiberationSans-Regular.ttf` (sha256 `76d04c18ea243f426b7de1f3ad208e927008f961dc5945e5aad352d0dfde8ee8`) has name ID 13 "Licensed under the SIL Open Font License, Version 1.1". Upstream's file is 1.05 and is a gap. | liberation-fonts 2.1.5 `LiberationSans-Regular.ttf` |
| Assistant | 10 | SIL OFL 1.1 | <https://github.com/google/fonts/blob/main/ofl/assistant/OFL.txt> | 2026-09-28 | OFL.txt: "Copyright 2020 The Assistant Project Authors ... Copyright 2010 The Source Sans Pro Authors, with Reserved Font Name 'Source' ... licensed under the SIL Open Font License, Version 1.1". Upstream files: Version 3.000, OFL 1.1 in name ID 13. | upstream `fonts/Assistant/` |
| Xiaolai | 100 | SIL OFL 1.1 | <https://github.com/lxgw/kose-font/blob/master/OFL.txt> | 2026-09-28 | OFL.txt: "Copyright 2020-2024 LXGW, Copyright 2014 Nozomi Seto ... licensed under the SIL Open Font License, Version 1.1", no Reserved Font Name; repository licence OFL-1.1. Upstream files: Xiaolai SC Version 3.11, modified by upstream (`fonts/Xiaolai/index.ts:216-232`), which the OFL permits. | upstream `fonts/Xiaolai/` |
| Segoe UI Emoji | 1000 | local only | n/a | 2026-09-28 | `fonts/Emoji/index.ts`: `uri: LOCAL_FONT_PROTOCOL`; no file upstream. | never vendored |

## Licence gaps

Files or families not vendored because their licence is not confirmed, and the fallback each maps to under rule 3. A row is removed when the licence is confirmed above. The sha256 lets the gate reject the file if it is ever added. Recorded 2026-09-27; revised 2026-09-28 (`ex-306`), when Virgil, Cascadia Code, Nunito, Lilita One and Assistant were confirmed and left this table.

| Family | Not vendored | sha256 | Fallback | Fallback licence |
|---|---|---|---|---|
| Liberation Sans | upstream `fonts/Liberation/LiberationSans-Regular.woff2`: Version 1.05, "Digitized data 2007 Ascender Corporation"; name ID 13 "Use of this Liberation font software is subject to the license agreement under which you accepted the Liberation font software", the GPLv2-with-font-exception terms of Liberation 1.x, which are none of OFL, MIT or Apache | `006a2b28cbbeeaec937d1b367d0949f5f48ef4b2e7b4e1d8dc7a799d2d639be8` | Liberation Sans 2.1.5 (same family, liberation-fonts release) | SIL OFL 1.1 |

Helvetica (2) and Segoe UI Emoji (1000) are `local:` only upstream and are never vendored, so they are not gaps.
