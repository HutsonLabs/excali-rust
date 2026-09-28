//! `Math.sin` and `Math.cos` as V8 computes them (fdlibm, `src/base/
//! ieee754.cc`): the platform's `sin`/`cos` can be one ulp away (macOS
//! libm gives `sin(4) = -0.7568024953079283` where V8 gives
//! `-0.7568024953079282`), which moves a rotated point and so an exported
//! document's size in its last digit.

use excali_math::{js, point_from, point_rotate_rads, GlobalPoint, Radians};

/// `[x, Math.cos(x), Math.sin(x)]` from Node 26 (V8).
const V8: [[f64; 3]; 18] = [
    [4.0, -0.6536436208636119, -0.7568024953079282],
    [-4.0, -0.6536436208636119, 0.7568024953079282],
    [0.6, 0.8253356149096783, 0.5646424733950354],
    [2.4, -0.7373937155412454, 0.675463180551151],
    [5.5, 0.70866977429126, -0.7055403255703919],
    [-5.5, 0.70866977429126, 0.7055403255703919],
    [-2.4, -0.7373937155412454, -0.675463180551151],
    [-0.6, 0.8253356149096783, -0.5646424733950354],
    [1e10, 0.873119622676856, -0.4875060250875107],
    [123456.789, 0.05167253271870138, -0.9986640823432246],
    [0.7, 0.7648421872844885, 0.644217687237691],
    [1.2, 0.3623577544766736, 0.9320390859672263],
    [2.2, -0.5885011172553458, 0.8084964038195901],
    [5.9, 0.9274784307440359, -0.373876664830236],
    [0.3, 0.955336489125606, 0.29552020666133955],
    [0.5, 0.8775825618903728, 0.479425538604203],
    [1.5, 0.0707372016677029, 0.9974949866040544],
    [0.9, 0.6216099682706644, 0.7833269096274834],
];

#[test]
fn sin_and_cos_are_v8s() {
    for [x, cos, sin] in V8 {
        assert_eq!(js::cos(x), cos, "cos {x}");
        assert_eq!(js::sin(x), sin, "sin {x}");
    }
    assert!(js::sin(f64::NAN).is_nan());
    assert!(js::cos(f64::INFINITY).is_nan());
    assert!(js::sin(-0.0).is_sign_negative());
    assert_eq!(js::cos(0.0), 1.0);
}

#[test]
fn rotation_uses_v8s_sin_and_cos() {
    // pointRotateRads([10, 0], [0, 0], 4): (10 cos 4, 10 sin 4) in V8
    let p: GlobalPoint =
        point_rotate_rads(point_from(10.0, 0.0), point_from(0.0, 0.0), Radians(4.0));
    assert_eq!(p.x, 10.0 * -0.6536436208636119);
    assert_eq!(p.y, 10.0 * -0.7568024953079282);
}
