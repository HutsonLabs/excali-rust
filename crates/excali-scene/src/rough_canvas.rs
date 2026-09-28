//! roughjs's `RoughCanvas.draw` (4.6.4, `bin/canvas.js`) as display items.
//!
//! Upstream draws every roughjs shape with `rc.draw(drawable)` after setting
//! `lineJoin` and `lineCap` to `"round"` (`renderElement.ts:473-495`). The
//! method walks the drawable's op sets in order:
//!
//! - `path`: a stroke in `o.stroke` (`"none"` becomes `"transparent"`),
//!   `lineWidth = o.strokeWidth`, `setLineDash(o.strokeLineDash)` when set
//!   and `lineDashOffset = o.strokeLineDashOffset` when truthy;
//! - `fillPath`: a fill in `o.fill || ''`, `evenodd` for `curve`, `polygon`
//!   and `path` shapes and `nonzero` for the rest;
//! - `fillSketch` (`fillSketch`): a stroke in `o.fill || ''`,
//!   `lineWidth = o.fillWeight` (`o.strokeWidth / 2` when negative), with
//!   `o.fillLineDash` and a truthy `o.fillLineDashOffset`;
//!
//! each op's numbers through `+d.toFixed(o.fixedDecimalPlaceDigits)` when
//! that option is a number `>= 0` (`_drawToContext`). Every set is drawn
//! inside its own `save()`/`restore()`, so the context's cap and join carry
//! in and nothing carries out: the items are independent.

use std::fmt;

use excali_rough::{Drawable, Op, OpSetType, Options, Shape};

use crate::display::{Color, Dash, DisplayItem, FillRule, LineCap, LineJoin, Path, Stroke};

/// `Number.prototype.toFixed` threw a `RangeError`: the digit count was
/// above 100 or infinite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToFixedRangeError;

impl fmt::Display for ToFixedRangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("toFixed() digits argument must be between 0 and 100")
    }
}

impl std::error::Error for ToFixedRangeError {}

/// The display items `rc.draw(drawable)` paints on a context whose
/// `lineCap` and `lineJoin` are `cap` and `join`, one per op set, in order.
pub fn draw(
    drawable: &Drawable,
    cap: LineCap,
    join: LineJoin,
) -> Result<Vec<DisplayItem>, ToFixedRangeError> {
    let o = &drawable.options;
    let digits = fixed_digits(o)?;
    let mut items = Vec::with_capacity(drawable.sets.len());
    for set in &drawable.sets {
        let path = to_path(&set.ops, digits);
        let item = match set.kind {
            OpSetType::Path => {
                let color = if o.stroke == "none" {
                    "transparent"
                } else {
                    o.stroke.as_str()
                };
                let dash = dash(o.stroke_line_dash.as_deref(), o.stroke_line_dash_offset);
                DisplayItem::Stroke {
                    path,
                    stroke: Stroke::new(Color::new(color), o.stroke_width)
                        .with_cap(cap)
                        .with_join(join)
                        .with_dash(dash),
                }
            }
            OpSetType::FillPath => {
                let rule = match drawable.shape {
                    Shape::Curve | Shape::Polygon | Shape::Path => FillRule::EvenOdd,
                    _ => FillRule::NonZero,
                };
                DisplayItem::Fill {
                    path,
                    color: fill_color(o),
                    rule,
                }
            }
            OpSetType::FillSketch => {
                let weight = if o.fill_weight < 0.0 {
                    o.stroke_width / 2.0
                } else {
                    o.fill_weight
                };
                let dash = dash(o.fill_line_dash.as_deref(), o.fill_line_dash_offset);
                DisplayItem::Stroke {
                    path,
                    stroke: Stroke::new(fill_color(o), weight)
                        .with_cap(cap)
                        .with_join(join)
                        .with_dash(dash),
                }
            }
        };
        items.push(item);
    }
    Ok(items)
}

/// `o.fill || ''`.
fn fill_color(o: &Options) -> Color {
    Color::new(o.fill.clone().unwrap_or_default())
}

/// `setLineDash(list)` when the list is set (an empty array is truthy),
/// then `lineDashOffset = offset` when the offset is truthy.
fn dash(list: Option<&[f64]>, offset: Option<f64>) -> Option<Dash> {
    let list = list?;
    let offset = offset.filter(|o| *o != 0.0 && !o.is_nan()).unwrap_or(0.0);
    Dash::new(list, offset)
}

/// The digit count `_drawToContext` rounds to: `None` unless
/// `typeof fixedDecimals === 'number' && fixedDecimals >= 0`, then
/// `toFixed`'s `ToIntegerOrInfinity`, which throws above 100.
fn fixed_digits(o: &Options) -> Result<Option<usize>, ToFixedRangeError> {
    match o.fixed_decimal_place_digits {
        Some(n) if n >= 0.0 => {
            let n = n.trunc();
            if n > 100.0 {
                Err(ToFixedRangeError)
            } else {
                Ok(Some(n as usize))
            }
        }
        _ => Ok(None),
    }
}

fn to_path(ops: &[Op], digits: Option<usize>) -> Path {
    let r = |v: f64| match digits {
        Some(d) => to_fixed(v, d),
        None => v,
    };
    let mut path = Path::new();
    for op in ops {
        match *op {
            Op::Move([x, y]) => path.move_to(r(x), r(y)),
            Op::LineTo([x, y]) => path.line_to(r(x), r(y)),
            Op::BCurveTo([a, b, c, d, e, f]) => path.cubic_to(r(a), r(b), r(c), r(d), r(e), r(f)),
        };
    }
    path
}

/// `+x.toFixed(digits)` for `digits` in `0..=100`.
///
/// `toFixed` rounds the exact binary value to the nearest multiple of
/// `10^-digits`, taking the larger magnitude on a tie (Rust's `{:.N}` takes
/// the even one), keeps the sign of a negative number that rounds to zero,
/// and at `10^21` and above prints the number itself.
fn to_fixed(x: f64, digits: usize) -> f64 {
    if !x.is_finite() || x.abs() >= 1e21 {
        return x;
    }
    // Every finite double has a finite decimal expansion of at most 1074
    // fractional digits, and `{:.N}` prints it exactly.
    let exact = format!("{:.1100}", x.abs());
    let (int_part, frac) = exact.split_once('.').expect("fixed notation");
    let mut kept: Vec<u8> = format!("{int_part}{}", &frac[..digits]).into_bytes();
    if frac.as_bytes()[digits] >= b'5' {
        // Add one in the last kept place, carrying through nines.
        let mut i = kept.len();
        loop {
            if i == 0 {
                kept.insert(0, b'1');
                break;
            }
            i -= 1;
            if kept[i] == b'9' {
                kept[i] = b'0';
            } else {
                kept[i] += 1;
                break;
            }
        }
    }
    let kept = String::from_utf8(kept).expect("ascii digits");
    let (i, f) = kept.split_at(kept.len() - digits);
    let sign = if x < 0.0 { "-" } else { "" };
    let text = if digits == 0 {
        format!("{sign}{i}")
    } else {
        format!("{sign}{i}.{f}")
    };
    text.parse().expect("decimal")
}

#[cfg(test)]
mod tests {
    use super::to_fixed;

    #[test]
    fn to_fixed_matches_javascript() {
        // Values from V8: (x).toFixed(n).
        assert_eq!(to_fixed(0.5, 0), 1.0);
        assert_eq!(to_fixed(1.5, 0), 2.0);
        assert_eq!(to_fixed(2.5, 0), 3.0);
        assert_eq!(to_fixed(-2.5, 0), -3.0);
        assert_eq!(to_fixed(1.005, 2), 1.0);
        assert_eq!(to_fixed(9.995, 2), 9.99);
        assert_eq!(to_fixed(9.9951, 2), 10.0);
        assert_eq!(to_fixed(99.5, 0), 100.0);
        assert_eq!(to_fixed(0.000001, 7), 0.000001);
        assert_eq!(to_fixed(123.456, 1), 123.5);
        let neg_zero = to_fixed(-0.04, 1);
        assert_eq!(neg_zero, 0.0);
        assert!(neg_zero.is_sign_negative());
        assert!(to_fixed(f64::NAN, 2).is_nan());
        assert_eq!(to_fixed(1e21, 2), 1e21);
    }
}
