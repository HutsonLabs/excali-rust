//! `Math.log10` as V8 computes it (`src/base/ieee754.cc`, fdlibm's
//! `e_log10.c`: `n * log10_2hi + (n * log10_2lo + ivln10 * log(x))`), which
//! the wheel zoom (`App.wheel.ts`) runs on every tick above 100%.

use std::f64::consts::{LN_2, LOG10_2};

use excali_math::js;

/// `[x, Math.log10(x)]` from Node 26 (V8).
const V8: [[f64; 2]; 24] = [
    [1.0, 0.0],
    [1.1, 0.04139268515822507],
    [1.5, 0.17609125905568124],
    [2.0, LOG10_2],
    [2.5, 0.3979400086720376],
    [3.0, 0.47712125471966244],
    [5.0, 0.6989700043360189],
    [9.99, 0.9995654882259823],
    [10.0, 1.0],
    [29.0, 1.462397997898956],
    [30.0, 1.4771212547196624],
    [1.0000001, 4.3429446044209946e-8],
    [1.058607, 0.0247347614210928],
    [7.123456789, 0.8526907942085881],
    [12345.678, 4.091514945509202],
    [0.5, -LOG10_2],
    [1e-310, -310.0],
    [1.234567, 0.09151466408626273],
    [2.333333, 0.3679767232525211],
    [4.2, 0.6232492903979004],
    [17.77, 1.2496874278053016],
    [25.123456, 1.4000793810241876],
    [1.9999999, 0.3010299739492565],
    [3.1622776601683795, 0.5],
];

/// `[x, Math.log(x)]` and `[x, Math.log10(x)]` from Node 26 on arm64
/// where the `libm` crate's `log` / the platform's `log10` answer an ulp
/// away (V8 fuses multiply-adds on arm64).
const V8_ULP: [[f64; 2]; 4] = [
    [1.125678300857544, 0.11838578806024468],
    [4.464211702346802, 1.4960926483153532],
    [4.855849266052246, 1.580184012511882],
    [23.418453752994537, 3.1535243334876215],
];
const V8_LOG10_ULP: [[f64; 2]; 6] = [
    [6.574092268943787, 0.8178357952144111],
    [11.932389080524445, 1.0767274060016119],
    [7.533736705780029, 0.8770104381097534],
    [1.125678300857544, 0.051414294490332135],
    [1.0548403561115265, 0.023186736689541684],
    [1.3397953510284424, 0.1270384664941942],
];

#[test]
fn log_is_v8s() {
    for [x, want] in V8_ULP {
        assert_eq!(js::log(x), want, "log({x})");
    }
    assert_eq!(js::log(0.5), -LN_2);
    assert_eq!(js::log(2.0), LN_2);
    assert_eq!(js::log(1e-310), -713.8013788281542);
    assert_eq!(js::log(1.0000001), 9.999999505838704e-8);
    assert_eq!(js::log(1.0), 0.0);
    assert_eq!(js::log(0.0), f64::NEG_INFINITY);
    assert!(js::log(-2.0).is_nan());
    assert_eq!(js::log(f64::INFINITY), f64::INFINITY);
    for [x, want] in V8_LOG10_ULP {
        assert_eq!(js::log10(x), want, "log10({x})");
    }
}

#[test]
fn log10_is_v8s() {
    for [x, want] in V8 {
        assert_eq!(js::log10(x), want, "log10({x})");
    }
    assert_eq!(js::log10(0.0), f64::NEG_INFINITY);
    assert_eq!(js::log10(-0.0), f64::NEG_INFINITY);
    assert!(js::log10(-1.0).is_nan());
    assert!(js::log10(f64::NAN).is_nan());
    assert_eq!(js::log10(f64::INFINITY), f64::INFINITY);
    assert!(js::log10(1.0).is_sign_positive());
}
