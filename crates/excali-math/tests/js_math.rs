//! `excali_math::js`'s transcendental functions against V8's `Math` (ex-009).
//!
//! `goldens/js-math.json` (tools/goldens/js-math.mjs) holds V8's answers,
//! as IEEE bits, for every function the port calls: special values, every
//! argument-reduction path, random arguments, and arguments where V8 on
//! arm64 (fdlibm compiled with fused multiply-adds, where the goldens are
//! generated) differs from fdlibm without them (x64 V8, the `libm` crate)
//! and from the platform's libm. The port must return the same bits on every
//! platform it runs on, so the comparison is exact: `-0` is not `0`.
//!
//! The tables below repeat a few of those arguments with the numbers Node
//! 26.10.0 (arm64) printed for them (`node -e 'console.log(Math.sin(x))'`),
//! so a failure names the value.

use std::path::Path;

use excali_math::js;
use serde_json::Value;

fn bits(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).unwrap_or_else(|e| panic!("{h}: {e}")))
}

fn call(f: &str, args: &[f64]) -> f64 {
    match (f, args) {
        ("sin", [x]) => js::sin(*x),
        ("cos", [x]) => js::cos(*x),
        ("tan", [x]) => js::tan(*x),
        ("asin", [x]) => js::asin(*x),
        ("acos", [x]) => js::acos(*x),
        ("atan", [x]) => js::atan(*x),
        ("atan2", [y, x]) => js::atan2(*y, *x),
        ("exp", [x]) => js::exp(*x),
        ("log", [x]) => js::log(*x),
        ("log2", [x]) => js::log2(*x),
        ("log10", [x]) => js::log10(*x),
        ("cbrt", [x]) => js::cbrt(*x),
        ("pow", [x, y]) => js::pow(*x, *y),
        ("hypot", [a, b]) => js::hypot(*a, *b),
        _ => panic!("no such case: {f}({args:?})"),
    }
}

#[test]
fn every_js_math_golden_is_v8s_bits() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../goldens/js-math.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} (run node tools/goldens/generate.mjs)",
            path.display()
        )
    });
    let doc: Value = serde_json::from_str(&text).expect("js-math.json parses");
    let cases = doc["cases"].as_array().expect("cases");
    assert!(cases.len() > 2000, "{} cases", cases.len());
    let mut failures = Vec::new();
    for c in cases {
        let id = c["id"].as_str().expect("id");
        let f = c["fn"].as_str().expect("fn");
        let args: Vec<f64> = c["args"]
            .as_array()
            .expect("args")
            .iter()
            .map(|a| bits(a.as_str().expect("bits")))
            .collect();
        let got = call(f, &args);
        let ok = match c["result"].as_str().expect("result") {
            "NaN" => got.is_nan(),
            h => got.to_bits() == bits(h).to_bits(),
        };
        if !ok {
            failures.push(format!(
                "{id}: {f}({args:?}) = {got:?}, V8 gives {}",
                c["result"]
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

/// `[x, Math.f(x)]` from Node 26.10.0 on arm64, for arguments where fdlibm
/// without fused multiply-adds (or the platform's libm) is one ulp away.
#[test]
fn one_argument_functions_match_node() {
    let table: &[(&str, fn(f64) -> f64, &[[f64; 2]])] = &[
        (
            "sin",
            js::sin,
            &[
                [-2.683369236376121, -0.4423554957555586],
                [976128188384889.1, -0.4906712929454923],
                [-2.5436005339429655, -0.5629841611413305],
                [4.0, -0.7568024953079282],
            ],
        ),
        (
            "cos",
            js::cos,
            &[
                [8.308666897082428, -0.4391795761598412],
                [883782.7025707152, -0.7531498318064266],
                [-330644.38492288237, -0.37823213285329327],
                // ex-533: fdlibm without fused multiply-adds gives ...781
                [0.982953340331056, 0.5545673797180782],
            ],
        ),
        (
            "tan",
            js::tan,
            &[
                [-0.4761483457828063, -0.5157250273080588],
                [40.302980444302136, -0.5963401010252644],
                [-415076.7219674888, -0.07594273626781883],
            ],
        ),
        (
            "asin",
            js::asin,
            &[
                [0.5537755138928944, 0.5868916848448922],
                [-0.6588324731989187, -0.719265739736933],
                [0.5532374468095438, 0.5862456256266815],
            ],
        ),
        (
            "acos",
            js::acos,
            &[
                [0.6642598837672071, 0.8442930647078613],
                [0.5899561665204371, 0.939791774391891],
                [-0.00032896732219630696, 1.5711252941230265],
            ],
        ),
        (
            "atan",
            js::atan,
            &[
                [0.32674229347329486, 0.31580693598672455],
                [-3.9019987447146436, -1.3199168840756912],
                [6.489565756317702, 1.417905365297653],
            ],
        ),
        (
            "exp",
            js::exp,
            &[
                [0.23957154722891905, 1.2707045967662853],
                [177.13348595473838, 8.474137576392681e76],
                [-2.9185717221030094, 0.05401077463179142],
            ],
        ),
        (
            "log",
            js::log,
            &[
                [1.3195570763741116, 0.2772961320879545],
                [173.47423467013607, 5.156029085017046],
                [664.0850243492977, 6.498410189997719],
            ],
        ),
        (
            "log2",
            js::log2,
            &[
                [0.7331078650461749, -0.44790261150997246],
                [542.3492830207986, 9.083078463095172],
            ],
        ),
        (
            "log10",
            js::log10,
            &[
                [4.52599677549345, 0.6557142402095404],
                [334.1080110241785, 2.523886888993068],
                [499.82780128372787, 2.698820408669823],
            ],
        ),
        (
            "cbrt",
            js::cbrt,
            &[
                [592.9990451691788, 8.40139359516895],
                [7.462603531500223e166, 4.210142443692187e55],
                [2.821448927850155, 1.4130495717792673],
            ],
        ),
    ];
    for (name, f, cases) in table {
        for [x, want] in *cases {
            assert_eq!(f(*x).to_bits(), want.to_bits(), "Math.{name}({x}) = {want}");
        }
    }
}

#[test]
fn atan2_matches_node() {
    for [y, x, want] in [
        [0.05023621814968182, 0.6890742144628068, 0.07277517907501184],
        [-827.39448702809, -405.0959531509878, -2.026092896087045],
    ] {
        assert_eq!(js::atan2(y, x), want, "Math.atan2({y}, {x})");
    }
}

/// V8's `Math.pow` (src/numbers/ieee754.cc `math::pow`): the ES special
/// cases, `x ** 2` = `x * x`, `x ** 0.5` = `sqrt(x + 0)`, then C's `pow`.
#[test]
fn pow_keeps_v8s_special_cases() {
    assert!(js::pow(1.0, f64::NAN).is_nan());
    assert!(js::pow(1.0, f64::INFINITY).is_nan());
    assert!(js::pow(-1.0, f64::NEG_INFINITY).is_nan());
    assert_eq!(js::pow(f64::NAN, 0.0), 1.0);
    assert_eq!(js::pow(f64::NEG_INFINITY, 0.5), f64::INFINITY);
    assert_eq!(js::pow(-0.0, 0.5).to_bits(), 0.0f64.to_bits());
    assert!(js::pow(-4.0, 0.5).is_nan());
    assert_eq!(js::pow(-0.0, 3.0).to_bits(), (-0.0f64).to_bits());
    assert_eq!(js::pow(-0.0, -3.0), f64::NEG_INFINITY);
    for x in [0.1, -3.3, 1e155, 7.000000000000001] {
        assert_eq!(js::pow(x, 2.0).to_bits(), (x * x).to_bits(), "{x} ** 2");
    }
}

/// Past the special cases V8 calls the platform's `pow`, which is not the
/// same function everywhere (Node 26.10.0 on macOS arm64 prints
/// `0.25570661814708906 ** 3 = 0.016719600859406776`, one ulp above the
/// exact cube rounded). The port's `pow` is correctly rounded on every
/// platform: these are the exact values rounded to nearest (Python
/// `float(Fraction(x) ** n)`).
#[test]
fn pow_is_correctly_rounded() {
    for (x, y, want) in [
        (0.25570661814708906, 3.0, 0.01671960085940678),
        (0.9764517936315023, 3.0, 0.9310058770591821),
        (-595.6837189852585, 3.0, -211371870.5977843),
        (0.6275843871180324, 4.0, 0.1551274034070525),
        (10.0, 22.0, 1e22),
        (2.0, -1074.0, 5e-324),
    ] {
        assert_eq!(js::pow(x, y).to_bits(), want.to_bits(), "{x} ** {y}");
    }
}

/// The `f32` functions for ports of C++ that calls `sinf` and friends
/// (Skia's `sk_float_*`): V8's double function of the widened argument,
/// rounded once to `f32`, the same on every platform.
#[test]
fn f32_functions_round_the_double_ones() {
    for x in [0.0f32, 1.9, -2.5, 100.25, 1e-3] {
        assert_eq!(js::sin_f32(x), js::sin(f64::from(x)) as f32);
        assert_eq!(js::cos_f32(x), js::cos(f64::from(x)) as f32);
    }
    for x in [-1.0f32, -0.5, 0.0, 0.3, 1.0] {
        assert_eq!(js::acos_f32(x), js::acos(f64::from(x)) as f32);
    }
    assert_eq!(js::pow_f32(8.0, 0.333_333_3), js::pow(8.0, f64::from(0.333_333_3f32)) as f32);
}

/// Every std method with a platform-dependent result is disallowed in the
/// workspace clippy.toml, pointing at `excali_math::js`, so
/// `cargo clippy -- -D warnings` rejects a new call in any crate or tools
/// harness.
#[test]
fn clippy_disallows_the_platform_methods() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../clippy.toml");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    for ty in ["f64", "f32"] {
        for m in [
            "sin", "cos", "tan", "sin_cos", "asin", "acos", "atan", "atan2", "exp", "ln", "log",
            "log2", "log10", "powf", "hypot", "cbrt", "exp2", "exp_m1", "ln_1p", "sinh",
            "cosh", "tanh", "asinh", "acosh", "atanh",
        ] {
            let entry = format!("path = \"{ty}::{m}\"");
            assert!(text.contains(&entry), "clippy.toml has no {entry}");
        }
    }
    assert!(text.contains("use excali_math::js (V8/fdlibm-exact on every platform)"));
}
