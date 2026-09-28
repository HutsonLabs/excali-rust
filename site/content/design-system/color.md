+++
title = "Colour"
description = "The element palette, top picks, canvas backgrounds and the dark-mode transform, from packages/common/src/colors.ts."
weight = 2
+++

## Palette

Five shades per hue (open-color indexes 0, 2, 4, 6, 8; bronze from radix 3, 5, 7, 9, 11). Default stroke uses shade 4, default background shade 1.

| Hue | 0 | 1 | 2 | 3 | 4 |
|---|---|---|---|---|---|
| gray | <span class="swatch" style="background:#f8f9fa"></span>`#f8f9fa` | <span class="swatch" style="background:#e9ecef"></span>`#e9ecef` | <span class="swatch" style="background:#ced4da"></span>`#ced4da` | <span class="swatch" style="background:#868e96"></span>`#868e96` | <span class="swatch" style="background:#343a40"></span>`#343a40` |
| red | <span class="swatch" style="background:#fff5f5"></span>`#fff5f5` | <span class="swatch" style="background:#ffc9c9"></span>`#ffc9c9` | <span class="swatch" style="background:#ff8787"></span>`#ff8787` | <span class="swatch" style="background:#fa5252"></span>`#fa5252` | <span class="swatch" style="background:#e03131"></span>`#e03131` |
| pink | <span class="swatch" style="background:#fff0f6"></span>`#fff0f6` | <span class="swatch" style="background:#fcc2d7"></span>`#fcc2d7` | <span class="swatch" style="background:#f783ac"></span>`#f783ac` | <span class="swatch" style="background:#e64980"></span>`#e64980` | <span class="swatch" style="background:#c2255c"></span>`#c2255c` |
| grape | <span class="swatch" style="background:#f8f0fc"></span>`#f8f0fc` | <span class="swatch" style="background:#eebefa"></span>`#eebefa` | <span class="swatch" style="background:#da77f2"></span>`#da77f2` | <span class="swatch" style="background:#be4bdb"></span>`#be4bdb` | <span class="swatch" style="background:#9c36b5"></span>`#9c36b5` |
| violet | <span class="swatch" style="background:#f3f0ff"></span>`#f3f0ff` | <span class="swatch" style="background:#d0bfff"></span>`#d0bfff` | <span class="swatch" style="background:#9775fa"></span>`#9775fa` | <span class="swatch" style="background:#7950f2"></span>`#7950f2` | <span class="swatch" style="background:#6741d9"></span>`#6741d9` |
| blue | <span class="swatch" style="background:#e7f5ff"></span>`#e7f5ff` | <span class="swatch" style="background:#a5d8ff"></span>`#a5d8ff` | <span class="swatch" style="background:#4dabf7"></span>`#4dabf7` | <span class="swatch" style="background:#228be6"></span>`#228be6` | <span class="swatch" style="background:#1971c2"></span>`#1971c2` |
| cyan | <span class="swatch" style="background:#e3fafc"></span>`#e3fafc` | <span class="swatch" style="background:#99e9f2"></span>`#99e9f2` | <span class="swatch" style="background:#3bc9db"></span>`#3bc9db` | <span class="swatch" style="background:#15aabf"></span>`#15aabf` | <span class="swatch" style="background:#0c8599"></span>`#0c8599` |
| teal | <span class="swatch" style="background:#e6fcf5"></span>`#e6fcf5` | <span class="swatch" style="background:#96f2d7"></span>`#96f2d7` | <span class="swatch" style="background:#38d9a9"></span>`#38d9a9` | <span class="swatch" style="background:#12b886"></span>`#12b886` | <span class="swatch" style="background:#099268"></span>`#099268` |
| green | <span class="swatch" style="background:#ebfbee"></span>`#ebfbee` | <span class="swatch" style="background:#b2f2bb"></span>`#b2f2bb` | <span class="swatch" style="background:#69db7c"></span>`#69db7c` | <span class="swatch" style="background:#40c057"></span>`#40c057` | <span class="swatch" style="background:#2f9e44"></span>`#2f9e44` |
| yellow | <span class="swatch" style="background:#fff9db"></span>`#fff9db` | <span class="swatch" style="background:#ffec99"></span>`#ffec99` | <span class="swatch" style="background:#ffd43b"></span>`#ffd43b` | <span class="swatch" style="background:#fab005"></span>`#fab005` | <span class="swatch" style="background:#f08c00"></span>`#f08c00` |
| orange | <span class="swatch" style="background:#fff4e6"></span>`#fff4e6` | <span class="swatch" style="background:#ffd8a8"></span>`#ffd8a8` | <span class="swatch" style="background:#ffa94d"></span>`#ffa94d` | <span class="swatch" style="background:#fd7e14"></span>`#fd7e14` | <span class="swatch" style="background:#e8590c"></span>`#e8590c` |
| bronze | <span class="swatch" style="background:#f8f1ee"></span>`#f8f1ee` | <span class="swatch" style="background:#eaddd7"></span>`#eaddd7` | <span class="swatch" style="background:#d2bab0"></span>`#d2bab0` | <span class="swatch" style="background:#a18072"></span>`#a18072` | <span class="swatch" style="background:#846358"></span>`#846358` |

Singletons: `transparent`, black `#1e1e1e`, white `#ffffff`.

## Top picks (five slots each)

| Target | Colours |
|---|---|
| Stroke | black, red[4] `#e03131`, green[4] `#2f9e44`, blue[4] `#1971c2`, yellow[4] `#f08c00` |
| Background | transparent, red[1] `#ffc9c9`, green[1] `#b2f2bb`, blue[1] `#a5d8ff`, yellow[1] `#ffec99` |
| Bucket fill | white, red[1], green[1], blue[1], yellow[1] |
| Sticky note | `#ffdf6b`, pink[1], green[1], blue[1], orange[1] |
| Canvas background | `#ffffff`, `#f8f9fa`, `#f5faff`, `#fffce8`, `#fdf8f6` |

## Picker grid (5 × 3)

Row 1: transparent, white, gray, black, bronze. Row 2: cyan, blue, violet, grape, pink. Row 3: green, teal, yellow, orange, red. Keys `q w e r t / a s d f g / z x c v b` select cells; `Shift+1…5` select shades; `1…5` custom colours; `i` eyedropper.

## Element defaults

stroke `#1e1e1e`, background `transparent`, fill `solid`, stroke width 2 (medium), stroke style solid, roughness 1 (artist), opacity 100.

## Dark mode

Canvas colours are not re-authored; they pass through `invert(93%) hue-rotate(180deg)` computed numerically (`colors.ts:86-122`), and the reverse function exists for the picker. UI chrome uses the dark token set instead. Contrast helpers: `isColorDark` uses YIQ with threshold 160; outline contrast threshold 240.
