//! Path data as rough.js writes it: `RoughGenerator.opsToPath(drawing,
//! fixedDecimals)` (roughjs 4.6.4 `bin/generator.js`), which `RoughSVG.draw`
//! puts in every `<path d=…>` of an exported element.

use excali_scene::display::{Path, PathCommand};

use crate::number::{fixed, js};

/// `opsToPath` of a path drawn from rough.js ops (`move`, `lineTo`,
/// `bcurveTo`): `M{x} {y} `, `L{x} {y} ` and `C{x1} {y1}, {x2} {y2}, {x}
/// {y} ` per op, trimmed, each number through `+d.toFixed(digits)` when
/// `fixed_decimals` is given. `None` when the path holds a command rough.js
/// ops never make (a quadratic curve, an arc or a close).
pub fn rough_path_data(path: &Path, fixed_decimals: Option<usize>) -> Option<String> {
    let n = |v: f64| js(fixed_decimals.map_or(v, |d| fixed(v, d)));
    let mut out = String::new();
    for command in &path.commands {
        match *command {
            PathCommand::MoveTo(x, y) => {
                out.push_str(&format!("M{} {} ", n(x), n(y)));
            }
            PathCommand::LineTo(x, y) => {
                out.push_str(&format!("L{} {} ", n(x), n(y)));
            }
            PathCommand::CubicTo(x1, y1, x2, y2, x, y) => {
                out.push_str(&format!(
                    "C{} {}, {} {}, {} {} ",
                    n(x1),
                    n(y1),
                    n(x2),
                    n(y2),
                    n(x),
                    n(y)
                ));
            }
            PathCommand::QuadTo(..) | PathCommand::Arc { .. } | PathCommand::Close => return None,
        }
    }
    Some(out.trim().to_owned())
}
