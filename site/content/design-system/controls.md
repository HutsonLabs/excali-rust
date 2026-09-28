+++
title = "Controls"
description = "Which properties each element type exposes, and the option sets behind every control in the styles panel."
weight = 5
+++

Visibility comes from `packages/element/src/comparisons.ts` and `shapeActionPredicates.ts` ([research §8](../../research/ui-design-system/#8-element-property-controls-pactionsactionpropertiestsx)).

## Property × element type

| Property | rectangle | diamond | ellipse | arrow | line | freedraw | text | image | frame | embeddable | stickynote |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Stroke colour | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ (text colour) | | | ✓ | ✓ (text colour) |
| Background | ✓ | ✓ | ✓ | | ✓ | ✓ | | | | ✓ | ✓ |
| Fill style | ✓ | ✓ | ✓ | | ✓ | ✓ | | | | ✓ | |
| Stroke width | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ (half values) | | | | ✓ | |
| Stroke style | ✓ | ✓ | ✓ | ✓ | ✓ | | | | | ✓ | |
| Sloppiness | ✓ | ✓ | ✓ | ✓ | ✓ | | | | | ✓ | ✓ |
| Edges (roundness) | ✓ | ✓ | | | ✓ | | | ✓ | | ✓ | ✓ |
| Arrow type | | | | ✓ | | | | | | | |
| Arrowheads | | | | ✓ | | | | | | | |
| Freedraw mode | | | | | | ✓ | | | | | |
| Font family / size / align | | | | | | | ✓ | | | | ✓ |
| Vertical align | bound text only | | | | | | | | | | |
| Opacity | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Crop | | | | | | | | ✓ (single) | | | |
| Line editor | | | | ✓ (non-elbow) | ✓ | | | | | | |

Fill shows only when the background is not transparent. Layers hide while the freedraw or autoshape tool is active. Align needs a multi-selection; distribute needs more than two elements. Link needs a single element or container+text. Lock is in the context menu and Ctrl/Cmd+Shift+L, not the panel.

## Option sets

| Control | Options | Default |
|---|---|---|
| Fill | hachure (Alt-click toggles zigzag), cross-hatch, solid | solid |
| Stroke width | thin 1, medium 2, bold 4 (freedraw 0.5 / 1 / 2) | 2 |
| Stroke style | solid, dashed, dotted | solid |
| Sloppiness | architect 0, artist 1, cartoonist 2 | 1 |
| Pressure (freedraw) | constant, variable | variable |
| Edges | sharp, round | round |
| Opacity | 0–100 step 10 | 100 |
| Font size | S 16, M 20, L 28, XL 36 | 20 |
| Text align | left, center, right | left |
| Vertical align | top, middle, bottom | top |
| Arrow type | sharp, round, elbow | round |
| Arrowheads (start, end) | none, arrow, triangle, triangle_outline, circle, circle_outline, diamond, diamond_outline, bar, cardinality_one, cardinality_many, cardinality_one_or_many, cardinality_exactly_one, cardinality_zero_or_one, cardinality_zero_or_many | start none, end arrow |

## Action groups

Layers: send to back, send backward, bring forward, bring to front. Align: left, centre-H, right, distribute-H; top, centre-V, bottom, distribute-V (mirrored for RTL). Actions: duplicate, delete, group, ungroup, link, crop, line editor.

## Context menus

Canvas: paste · copy as PNG, copy as SVG, copy text · select all, unlock all · grid, object snap, arrow binding, midpoint snapping, zen mode, view mode, stats.

Element: cut, copy, paste · select all in frame, remove all from frame, wrap in frame · crop · copy as PNG/SVG/text · copy styles, paste styles · group, auto resize, unbind/bind text, wrap text in container, ungroup · add to library · z-order (desktop) · flip H/V · toggle line editor · link, copy element link · duplicate, lock · delete.
