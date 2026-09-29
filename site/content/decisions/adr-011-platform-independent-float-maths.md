+++
title = "ADR-011: Platform-independent float maths"
description = "Every transcendental goes through excali_math::js, a port of V8's fdlibm that returns arm64 V8's doubles on every platform; clippy rejects the std methods."
weight = 11
+++

**Status.** Accepted, 2026-09-28 (`ex-009`).

## Question

Upstream computes `Math.sin`, `Math.cos`, `Math.atan2`, `Math.pow` and the other transcendentals in V8. The port called Rust's `f64` methods, which call the platform's C library. Do the two give the same doubles, and if not, which function does the port use, on which platforms?

## Evidence

- `ex-513` found a rotated bound text at `y = 247.5924092061952` on Linux where upstream's test expects `247.59240920619527` (`packages/excalidraw/tests/history.test.tsx:4396`, a container rotated by 90 radians). glibc's `cos(90)` is `-0.4480736161291701`; Node 26.10.0 prints `Math.cos(90) = -0.4480736161291702` (run 2026-09-28, glibc in `rust:1-bookworm`).
- V8 14.6.202.34 (the V8 of Node 26.10.0) computes `Math.sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `exp`, `log`, `log2`, `log10` and `cbrt` with its own copy of fdlibm, `src/base/ieee754.cc`. V8's GN build defaults `v8_use_libm_trig_functions = is_clang` (`gni/v8.gni:217`), which swaps `sin` and `cos` for glibc-derived routines (`third_party/glibc`), but Node builds V8 with gyp and `tools/v8_gypfiles/features.gypi` at v26.10.0 does not set it, so in Node `sin` and `cos` are fdlibm's too. `Math.hypot` is V8's own Torque (`src/builtins/math.tq`, `MathHypot`). `Math.pow` is `math::pow` (`src/numbers/ieee754.cc`): the ES special cases, `x ** 2 = x * x`, `x ** 0.5 = sqrt(x + 0)`, then the platform's `std::pow` (flag `use_std_math_pow`).
- clang contracts `a * b + c` inside one expression into a fused multiply-add wherever the target has one (`-ffp-contract=on` is its default). arm64 has FMA; x86-64's baseline does not. V8's `ieee754.cc` built with clang and `-ffp-contract=on` on macOS arm64 returns exactly what Node 26.10.0 prints for 400,000 random arguments per function; built with `-ffp-contract=off` it differs on 0.56% of `sin` arguments, 0.48% of `cos`, 1.1% of `tan` (run 2026-09-28). `tools/goldens/README.md` already records that the goldens are the arm64 output and that x86_64 V8 differs in the last bits.
- On the same 400,000 arguments, macOS arm64's libm differs from Node on 4.3% of `sin`, 4.6% of `cos`, 41% of `tan`, 22% of `acos`, 18% of `atan2`; the `libm` crate 0.2.16 (musl's fdlibm, no fused multiply-adds, other `log`, `exp` and `cbrt` algorithms) on 1.4% of `sin`, 8% of `cbrt`, 20% of `tanh`.
- `Math.pow` has no single answer: Node 26.10.0 on macOS arm64 prints `0.25570661814708906 ** 3 = 0.016719600859406776`, one ulp above the exact cube rounded (`0.01671960085940678`); glibc's `pow` is a different function again. `tools/goldens/math.mjs` already leaves out the three `math.json` cases where macOS and glibc disagree (`PLATFORM_DEPENDENT_CASES`).
- `pxfm` 0.1.30 (crates.io, BSD-3-Clause OR Apache-2.0, no dependencies; read 2026-09-28) ports CORE-MATH's correctly rounded `pow` to Rust. It agrees with macOS Node on all but 380 of 400,000 random arguments, and its answers are identical built for arm64 (FMA code path) and for wasm32 (no FMA) on 3,000,000 random arguments.

## Options

1. **Keep the platform's libm.** Every golden that goes through trigonometry needs a tolerance, and the port's exported numbers depend on the OS it runs on.
2. **The `libm` crate** (what `excali_math::js::{sin, cos, atan2}` used). The same everywhere, but not V8: musl's fdlibm without fused multiply-adds, and musl's own `log`, `exp`, `cbrt`.
3. **Port V8's `ieee754.cc` as arm64 V8 runs it**, every contracted expression written as `f64::mul_add` (correctly rounded on every target; a software routine where there is no FMA instruction, as on wasm32). The same doubles as the goldens, on every platform.

## Decision

Option 3, with a correctly rounded `pow`:

1. `excali_math::js` has `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `exp`, `log`, `log2`, `log10`, `cbrt` (ported from V8's `ieee754.cc` into `crates/excali-math/src/js/ieee754.rs`, fused multiply-adds where clang puts them on arm64, in the operand order clang's `tryEmitFMulAdd` picks), `hypot` (V8's Torque), and `pow` (V8's special cases, then `pxfm::f_pow`, the correctly rounded power, pinned `=0.1.30`). For code ported from C++ that calls the `float` functions (Skia in `excali-raster`), `sin_f32`, `cos_f32`, `acos_f32` and `pow_f32` round the double functions once.
2. The workspace `clippy.toml` disallows the `f64` and `f32` methods `sin`, `cos`, `tan`, `sin_cos`, `asin`, `acos`, `atan`, `atan2`, `exp`, `ln`, `log`, `log2`, `log10`, `powf`, `hypot`, `cbrt`, and `exp2`, `exp_m1`, `ln_1p`, `sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh` (not ported yet: port V8's version first), each with the reason "use excali_math::js (V8/fdlibm-exact on every platform)". Clippy reads the file from every package under the repository, so `cargo clippy -- -D warnings` rejects a new call in every crate and every `tools/` harness. `sqrt`, `floor`, `ceil`, `round`, `abs`, `mul_add` and `powi` stay allowed: they are correctly rounded, or exact, everywhere.
3. `goldens/js-math.json` (`tools/goldens/js-math.mjs`) records V8's bits for special values, every argument-reduction path, random arguments and the arguments where fdlibm without FMA or a platform libm is an ulp away; `crates/excali-math/tests/js_math.rs` checks every one exactly. The golden harness (`excali_rough::goldens`), `math.json`, the freehand, laser pointer and text sizing tests compare trigonometric results exactly.
4. The rust workflow runs the workspace tests and the golden harness on Linux x86-64 (`check`), Linux arm64 (`goldens`) and macOS arm64 (`test-macos`).

## Consequences

- The port's numbers no longer depend on the OS or the architecture; `excali-core` and `excali-math` stay wasm32-clean (the wasm32 build gate, and the same answers from a wasm32-wasip1 build of the comparison as from the native one).
- The reference is V8 on arm64. V8 on x86-64 (Chrome on most Windows machines) differs from it in the last bit on about 0.5% of `sin`/`cos` arguments, as it differs from the goldens; so does V8 in Chrome, built with GN and so with the glibc-derived `sin`/`cos`. The goldens were always arm64 Node, so nothing that passed before changes.
- `Math.pow` stays a known approximation of upstream: correctly rounded where upstream is the platform's `pow`. Goldens that go through `pow` keep excluding the cases where platforms disagree.
- `f64::mul_add` is a software routine on targets without FMA (wasm32, x86-64 without `+fma`): slower than a hardware instruction, but a handful per call.

## What would reverse it

Upstream moving to a different `Math` implementation (for example Node enabling `v8_use_libm_trig_functions`, which would make `sin`/`cos` glibc's correctly rounded routines): regenerate `goldens/js-math.json`, see which cases change, and port that code instead. A need for the platform's own libm (none known) would need a separate, non-`js` API.
