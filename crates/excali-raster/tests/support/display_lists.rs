//! The fixture display-list vocabulary (`tests/fixtures/README.md`) read
//! into `excali_scene::display` types.
//!
//! Shared by both backends' fixture checks: excali-raster's
//! `tests/fixtures.rs` renders the lists with tiny-skia, and the Canvas 2D
//! harness (`tools/canvas2d-fixtures`, ex-502) paints the same lists in the
//! browser through `excali_canvas2d`, so the two backends draw one parse of
//! each file. It depends on `excali_scene::display` and `serde_json` only,
//! and builds for wasm32. Images are the caller's: the raster test decodes
//! them in Rust, the browser loads them as `<img>` elements.

use excali_scene::display::{
    Clip, Color, Dash, DisplayItem, DisplayList, FillRule, Group, ImageFilter, ImageItem, LineCap,
    LineJoin, Path, Rect, Stroke, Transform,
};
use serde_json::Value;

/// A number: JSON numbers, or `"NaN"`, `"Infinity"` and `"-Infinity"`,
/// which JSON cannot write and the canvas rules are about.
pub fn num(v: &Value) -> Result<f64, String> {
    match v {
        Value::Number(n) => n.as_f64().ok_or_else(|| format!("not a number: {n}")),
        Value::String(s) => match s.as_str() {
            "NaN" => Ok(f64::NAN),
            "Infinity" => Ok(f64::INFINITY),
            "-Infinity" => Ok(f64::NEG_INFINITY),
            other => Err(format!("not a number: {other:?}")),
        },
        other => Err(format!("not a number: {other}")),
    }
}

pub fn nums(v: &Value) -> Result<Vec<f64>, String> {
    v.as_array()
        .ok_or_else(|| format!("not an array: {v}"))?
        .iter()
        .map(num)
        .collect()
}

pub fn rect(v: &Value) -> Result<Rect, String> {
    let n = nums(v)?;
    if n.len() != 4 {
        return Err(format!("a rectangle is [x, y, width, height]: {v}"));
    }
    Ok(Rect::new(n[0], n[1], n[2], n[3]))
}

pub fn rule(v: Option<&Value>) -> Result<FillRule, String> {
    match v.and_then(Value::as_str) {
        None | Some("nonzero") => Ok(FillRule::NonZero),
        Some("evenodd") => Ok(FillRule::EvenOdd),
        Some(other) => Err(format!("unknown fill rule {other:?}")),
    }
}

/// A path: canvas calls `["M", x, y]`, `["L", x, y]`, `["Q", cpx, cpy, x,
/// y]`, `["C", ...6]`, `["A", cx, cy, r, start, end, anticlockwise]`,
/// `["Z"]`, and the rectangle methods `["rect", x, y, w, h]` and
/// `["roundRect", x, y, w, h, r]` through the port's `Path::rect` and
/// `Path::round_rect`.
pub fn path(v: &Value) -> Result<Path, String> {
    let mut p = Path::new();
    let calls = v
        .as_array()
        .ok_or_else(|| format!("a path is an array of calls: {v}"))?;
    for call in calls {
        let call = call
            .as_array()
            .ok_or_else(|| format!("a path call is an array: {call}"))?;
        let op = call
            .first()
            .and_then(Value::as_str)
            .ok_or("a path call starts with its name")?;
        let args = &call[1..];
        let arity = match op {
            "M" | "L" => 2,
            "Q" | "rect" => 4,
            "roundRect" | "A" => 5,
            "C" => 6,
            "Z" => 0,
            other => return Err(format!("unknown path call {other:?}")),
        };
        if args.len() < arity {
            return Err(format!(
                "{op} takes {arity} numbers: {}",
                Value::from(call.clone())
            ));
        }
        let n = args[..arity]
            .iter()
            .map(num)
            .collect::<Result<Vec<_>, _>>()?;
        match op {
            "M" => {
                p.move_to(n[0], n[1]);
            }
            "L" => {
                p.line_to(n[0], n[1]);
            }
            "Q" => {
                p.quad_to(n[0], n[1], n[2], n[3]);
            }
            "C" => {
                p.cubic_to(n[0], n[1], n[2], n[3], n[4], n[5]);
            }
            "A" => {
                let anticlockwise = args.get(5).and_then(Value::as_bool).unwrap_or(false);
                p.arc(n[0], n[1], n[2], n[3], n[4], anticlockwise);
            }
            "Z" => {
                p.close();
            }
            "rect" => p
                .commands
                .extend(Path::rect(n[0], n[1], n[2], n[3]).commands),
            _ => p
                .commands
                .extend(Path::round_rect(n[0], n[1], n[2], n[3], n[4]).commands),
        }
    }
    Ok(p)
}

fn color(v: &Value, what: &str) -> Result<Color, String> {
    v["color"]
        .as_str()
        .map(Color::new)
        .ok_or_else(|| format!("a {what} has a colour: {v}"))
}

pub fn item(v: &Value) -> Result<DisplayItem, String> {
    let kind = v["type"]
        .as_str()
        .ok_or_else(|| format!("an item has a type: {v}"))?;
    Ok(match kind {
        "fill" => DisplayItem::Fill {
            path: path(&v["path"])?,
            color: color(v, "fill")?,
            rule: rule(v.get("rule"))?,
        },
        "fillRect" => DisplayItem::FillRect {
            rect: rect(&v["rect"])?,
            color: color(v, "fillRect")?,
        },
        "stroke" => {
            let width = match v.get("width") {
                Some(w) => num(w)?,
                None => 1.0,
            };
            let mut stroke = Stroke::new(color(v, "stroke")?, width);
            stroke.cap = match v.get("cap").and_then(Value::as_str) {
                None | Some("butt") => LineCap::Butt,
                Some("round") => LineCap::Round,
                Some("square") => LineCap::Square,
                Some(other) => return Err(format!("unknown cap {other:?}")),
            };
            stroke.join = match v.get("join").and_then(Value::as_str) {
                None | Some("miter") => LineJoin::Miter,
                Some("round") => LineJoin::Round,
                Some("bevel") => LineJoin::Bevel,
                Some(other) => return Err(format!("unknown join {other:?}")),
            };
            if let Some(limit) = v.get("miterLimit") {
                stroke.miter_limit = num(limit)?;
            }
            if let Some(dash) = v.get("dash") {
                let offset = match v.get("dashOffset") {
                    Some(o) => num(o)?,
                    None => 0.0,
                };
                stroke.dash = Dash::new(&nums(dash)?, offset);
            }
            DisplayItem::Stroke {
                path: path(&v["path"])?,
                stroke,
            }
        }
        "image" => DisplayItem::Image(ImageItem {
            id: v["id"]
                .as_str()
                .ok_or_else(|| format!("an image has an id: {v}"))?
                .to_owned(),
            source: v.get("source").map(rect).transpose()?,
            dest: rect(&v["dest"])?,
            smoothing: v.get("smoothing").and_then(Value::as_bool).unwrap_or(true),
            filter: match v.get("filter").and_then(Value::as_str) {
                None => None,
                Some("dark") => Some(ImageFilter::DarkTheme),
                Some(other) => return Err(format!("unknown image filter {other:?}")),
            },
        }),
        "group" => {
            let transform = match v.get("transform") {
                Some(t) => {
                    let m = nums(t)?;
                    if m.len() != 6 {
                        return Err(format!("a transform is [a, b, c, d, e, f]: {t}"));
                    }
                    Transform::new(m[0], m[1], m[2], m[3], m[4], m[5])
                }
                None => Transform::IDENTITY,
            };
            let clip = match v.get("clip") {
                Some(c) => Some(Clip {
                    path: path(&c["path"])?,
                    rule: rule(c.get("rule"))?,
                }),
                None => None,
            };
            DisplayItem::Group(Group {
                transform,
                opacity: match v.get("opacity") {
                    Some(o) => num(o)?,
                    None => 1.0,
                },
                clip,
                items: items(&v["items"])?,
            })
        }
        other => return Err(format!("unknown item type {other:?}")),
    })
}

pub fn items(v: &Value) -> Result<Vec<DisplayItem>, String> {
    v.as_array()
        .ok_or_else(|| format!("items is an array: {v}"))?
        .iter()
        .map(item)
        .collect()
}

/// A fixture file's canvas and list: the parts both backends read the same
/// way. `images` and `builtinImages` are left to the caller.
pub struct ListFixture {
    pub width: u32,
    pub height: u32,
    /// The device scale `bootstrapCanvas` applies before drawing.
    pub scale: f64,
    pub list: DisplayList,
}

pub fn list_fixture(v: &Value) -> Result<ListFixture, String> {
    let size = |key: &str| {
        v[key]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .filter(|&n| n > 0)
            .ok_or_else(|| format!("{key} is a positive integer: {}", v[key]))
    };
    Ok(ListFixture {
        width: size("width")?,
        height: size("height")?,
        scale: match v.get("scale") {
            Some(s) => num(s)?,
            None => 1.0,
        },
        list: items(&v["items"])?.into_iter().collect(),
    })
}
