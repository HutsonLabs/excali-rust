//! Display items in the raster fixture vocabulary
//! (`crates/excali-raster/tests/fixtures/README.md`), for the tests that
//! write the port's output as raster fixtures (`raster_fixture.rs`,
//! `image_elements.rs`), and the comparison they check the files with.

use excali_scene::display::{Color, DisplayItem, FillRule, ImageFilter, Path, PathCommand, Rect};
use serde_json::{json, Value};

pub fn path_json(p: &Path) -> Value {
    Value::Array(
        p.commands
            .iter()
            .map(|c| match *c {
                PathCommand::MoveTo(x, y) => json!(["M", x, y]),
                PathCommand::LineTo(x, y) => json!(["L", x, y]),
                PathCommand::QuadTo(a, b, x, y) => json!(["Q", a, b, x, y]),
                PathCommand::CubicTo(a, b, c, d, x, y) => json!(["C", a, b, c, d, x, y]),
                PathCommand::Arc {
                    cx,
                    cy,
                    radius,
                    start,
                    end,
                    anticlockwise,
                } => json!(["A", cx, cy, radius, start, end, anticlockwise]),
                PathCommand::Close => json!(["Z"]),
            })
            .collect(),
    )
}

fn color_json(c: &Color) -> Value {
    Value::String(c.as_str().to_owned())
}

fn rect_json(r: &Rect) -> Value {
    json!([r.x, r.y, r.width, r.height])
}

/// A display item in the raster fixture vocabulary. Text has no fixture
/// form (glyphs come from the font pipeline).
pub fn item_json(item: &DisplayItem) -> Value {
    match item {
        DisplayItem::Fill { path, color, rule } => {
            let mut v = json!({"type": "fill", "color": color_json(color)});
            if *rule == FillRule::EvenOdd {
                v["rule"] = json!("evenodd");
            }
            v["path"] = path_json(path);
            v
        }
        DisplayItem::FillRect { rect, color } => {
            json!({"type": "fillRect", "color": color_json(color), "rect": rect_json(rect)})
        }
        DisplayItem::Stroke { path, stroke } => {
            let mut v = json!({
                "type": "stroke",
                "color": color_json(&stroke.color),
                "width": stroke.width,
                "cap": stroke.cap.as_css(),
                "join": stroke.join.as_css(),
            });
            if stroke.miter_limit != 10.0 {
                v["miterLimit"] = json!(stroke.miter_limit);
            }
            if let Some(dash) = &stroke.dash {
                v["dash"] = json!(dash.segments());
                if dash.offset() != 0.0 {
                    v["dashOffset"] = json!(dash.offset());
                }
            }
            v["path"] = path_json(path);
            v
        }
        DisplayItem::Image(image) => {
            let mut v = json!({"type": "image", "id": image.id});
            if let Some(source) = &image.source {
                v["source"] = rect_json(source);
            }
            v["dest"] = rect_json(&image.dest);
            if !image.smoothing {
                v["smoothing"] = json!(false);
            }
            match image.filter {
                None => {}
                Some(ImageFilter::DarkTheme) => v["filter"] = json!("dark"),
            }
            v
        }
        DisplayItem::Group(g) => {
            let t = g.transform;
            let mut v = json!({"type": "group"});
            if !t.is_identity() {
                v["transform"] = json!([t.a, t.b, t.c, t.d, t.e, t.f]);
            }
            if g.opacity != 1.0 {
                v["opacity"] = json!(g.opacity);
            }
            if let Some(clip) = &g.clip {
                let mut c = json!({"path": path_json(&clip.path)});
                if clip.rule == FillRule::EvenOdd {
                    c["rule"] = json!("evenodd");
                }
                v["clip"] = c;
            }
            v["items"] = Value::Array(g.items.iter().map(item_json).collect());
            v
        }
        DisplayItem::Text(_) => panic!("raster fixtures have no text"),
        DisplayItem::Blit(_) => panic!("raster fixtures have no blits"),
    }
}

/// Whether two fixture values agree: the same structure and strings, and
/// numbers within 16 ulps. The geometry goes through the platform's `sin`
/// and `cos` (rough.js's ellipse, the rotation), which macOS and glibc can
/// round differently in the last bit; the files are written on arm64 macOS.
pub fn close(a: &Value, b: &Value, at: &str) -> Result<(), String> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            let tolerance = 16.0 * f64::EPSILON * x.abs().max(y.abs()).max(1.0);
            if (x - y).abs() <= tolerance {
                Ok(())
            } else {
                Err(format!("{at}: {x} != {y}"))
            }
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => x
            .iter()
            .zip(y)
            .enumerate()
            .try_for_each(|(i, (x, y))| close(x, y, &format!("{at}[{i}]"))),
        (Value::Object(x), Value::Object(y)) if x.len() == y.len() => {
            x.iter().try_for_each(|(k, v)| match y.get(k) {
                Some(w) => close(v, w, &format!("{at}.{k}")),
                None => Err(format!("{at}.{k} missing")),
            })
        }
        _ if a == b => Ok(()),
        _ => Err(format!("{at}: {a} != {b}")),
    }
}
