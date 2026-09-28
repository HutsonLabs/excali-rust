//! Recording a display list's draws and holding them to the draws upstream
//! made on the recording 2D context of `tools/goldens/lib/
//! recording-context.mjs` (the static scene and canvas export goldens).

use excali_scene::display::{
    Clip, Color, DisplayList, FillRule, ImageItem, PaintState, Painter, Path, PathCommand, Rect,
    Rgba, Stroke, TextRun, Transform,
};
use excali_scene::render_element::builtin_image;
use serde_json::Value;

// ---------------------------------------------------------------------------
// Recording the port's draws

#[derive(Debug)]
pub enum Draw {
    Fill {
        m: Transform,
        alpha: f64,
        color: Color,
        rgba: Rgba,
        rule: FillRule,
        path: Path,
    },
    FillRect {
        m: Transform,
        alpha: f64,
        color: Color,
        rgba: Rgba,
        rect: Rect,
    },
    Stroke {
        m: Transform,
        alpha: f64,
        stroke: Stroke,
        rgba: Rgba,
        path: Path,
    },
    Image {
        m: Transform,
        alpha: f64,
        image: ImageItem,
    },
    Text {
        m: Transform,
        alpha: f64,
        run: TextRun,
        rgba: Rgba,
    },
    Clip {
        m: Transform,
        clip: Clip,
    },
    Unclip,
}

#[derive(Default)]
pub struct Recorder(pub Vec<Draw>);

impl Painter for Recorder {
    fn fill(&mut self, path: &Path, color: &Color, rgba: Rgba, rule: FillRule, s: &PaintState) {
        self.0.push(Draw::Fill {
            m: s.transform,
            alpha: s.alpha,
            color: color.clone(),
            rgba,
            rule,
            path: path.clone(),
        });
    }
    fn fill_rect(&mut self, rect: &Rect, color: &Color, rgba: Rgba, s: &PaintState) {
        self.0.push(Draw::FillRect {
            m: s.transform,
            alpha: s.alpha,
            color: color.clone(),
            rgba,
            rect: *rect,
        });
    }
    fn stroke(&mut self, path: &Path, stroke: &Stroke, rgba: Rgba, s: &PaintState) {
        self.0.push(Draw::Stroke {
            m: s.transform,
            alpha: s.alpha,
            stroke: stroke.clone(),
            rgba,
            path: path.clone(),
        });
    }
    fn image(&mut self, image: &ImageItem, s: &PaintState) {
        self.0.push(Draw::Image {
            m: s.transform,
            alpha: s.alpha,
            image: image.clone(),
        });
    }
    fn text(&mut self, run: &TextRun, rgba: Rgba, s: &PaintState) {
        self.0.push(Draw::Text {
            m: s.transform,
            alpha: s.alpha,
            run: run.clone(),
            rgba,
        });
    }
    fn push_clip(&mut self, clip: &Clip, transform: &Transform) {
        self.0.push(Draw::Clip {
            m: *transform,
            clip: clip.clone(),
        });
    }
    fn pop_clip(&mut self) {
        self.0.push(Draw::Unclip);
    }
}

// ---------------------------------------------------------------------------
// Comparing

fn close(a: f64, b: f64) -> bool {
    a == b || (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

pub fn matrix(value: &Value) -> Transform {
    let v: Vec<f64> = value.as_array().unwrap().iter().map(num).collect();
    Transform::new(v[0], v[1], v[2], v[3], v[4], v[5])
}

pub fn same_matrix(a: &Transform, b: &Transform) -> bool {
    close(a.a, b.a)
        && close(a.b, b.b)
        && close(a.c, b.c)
        && close(a.d, b.d)
        && close(a.e, b.e)
        && close(a.f, b.f)
}

/// The recorded calls since `beginPath()` as a [`Path`]: `rect` and
/// `roundRect` add what those canvas methods add, a Path2D's SVG data is
/// read as the Path2D constructor reads it.
pub fn path(value: &Value) -> Path {
    let mut p = Path::new();
    for command in value.as_array().unwrap() {
        let c = command.as_array().unwrap();
        let a = |i: usize| num(&c[i]);
        match c[0].as_str().unwrap() {
            "moveTo" => {
                p.move_to(a(1), a(2));
            }
            "lineTo" => {
                p.line_to(a(1), a(2));
            }
            "bezierCurveTo" => {
                p.cubic_to(a(1), a(2), a(3), a(4), a(5), a(6));
            }
            "quadraticCurveTo" => {
                p.quad_to(a(1), a(2), a(3), a(4));
            }
            "arc" => {
                p.arc(a(1), a(2), a(3), a(4), a(5), c[6].as_bool().unwrap());
            }
            "closePath" => {
                p.close();
            }
            "rect" => p
                .commands
                .extend(Path::rect(a(1), a(2), a(3), a(4)).commands),
            "roundRect" => p
                .commands
                .extend(Path::round_rect(a(1), a(2), a(3), a(4), a(5)).commands),
            "svg" => p
                .commands
                .extend(Path::from_svg_path_data(c[1].as_str().unwrap()).commands),
            other => panic!("unknown path call {other}"),
        }
    }
    p
}

fn command_values(c: &PathCommand) -> (u8, Vec<f64>) {
    match *c {
        PathCommand::MoveTo(x, y) => (0, vec![x, y]),
        PathCommand::LineTo(x, y) => (1, vec![x, y]),
        PathCommand::QuadTo(a, b, c, d) => (2, vec![a, b, c, d]),
        PathCommand::CubicTo(a, b, c, d, e, f) => (3, vec![a, b, c, d, e, f]),
        PathCommand::Arc {
            cx,
            cy,
            radius,
            start,
            end,
            anticlockwise,
        } => (
            4,
            vec![
                cx,
                cy,
                radius,
                start,
                end,
                f64::from(u8::from(anticlockwise)),
            ],
        ),
        PathCommand::Close => (5, vec![]),
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.commands.len() == b.commands.len()
        && a.commands.iter().zip(&b.commands).all(|(x, y)| {
            let (kx, vx) = command_values(x);
            let (ky, vy) = command_values(y);
            kx == ky && vx.iter().zip(&vy).all(|(p, q)| close(*p, *q))
        })
}

fn rule(value: &Value) -> FillRule {
    match value.as_str().unwrap() {
        "evenodd" => FillRule::EvenOdd,
        _ => FillRule::NonZero,
    }
}

/// The style a list of assignments leaves: the last one the canvas can
/// parse, or none (the context's black).
fn style(value: &Value) -> Option<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .map(|v| v.as_str().unwrap())
        .find(|s| Color::new(*s).rgba().is_some())
        .map(str::to_owned)
}

fn check_color(expected: &Value, color: &Color, rgba: Rgba) -> Result<(), String> {
    match style(expected) {
        Some(s) if s == color.as_str() => Ok(()),
        Some(s) => Err(format!("colour {:?}, expected {s:?}", color.as_str())),
        None if rgba == Rgba::BLACK => Ok(()),
        None => Err(format!(
            "colour {:?} ({rgba:?}), expected the context's black",
            color.as_str()
        )),
    }
}

fn check_alpha(expected: &Value, alpha: f64) -> Result<(), String> {
    if close(num(expected), alpha) {
        Ok(())
    } else {
        Err(format!("alpha {alpha}, expected {expected}"))
    }
}

fn check_matrix(expected: &Value, m: &Transform) -> Result<(), String> {
    if same_matrix(&matrix(expected), m) {
        Ok(())
    } else {
        Err(format!("matrix {m:?}, expected {expected}"))
    }
}

fn check_path(expected: &Value, actual: &Path) -> Result<(), String> {
    let e = path(expected);
    if same_path(&e, actual) {
        Ok(())
    } else {
        Err(format!(
            "path {:?}\nexpected {:?}",
            actual.commands, e.commands
        ))
    }
}

pub fn check(e: &Value, draw: &Draw, images: &Value) -> Result<(), String> {
    let op = e["op"].as_str().unwrap();
    match (op, draw) {
        (
            "fill",
            Draw::Fill {
                m,
                alpha,
                color,
                rgba,
                rule: r,
                path: p,
            },
        ) => {
            check_matrix(&e["m"], m)?;
            check_alpha(&e["alpha"], *alpha)?;
            check_color(&e["fillStyle"], color, *rgba)?;
            if rule(&e["rule"]) != *r {
                return Err(format!("rule {r:?}, expected {}", e["rule"]));
            }
            check_path(&e["path"], p)
        }
        // fillRect: the canvas draws a rectangle, not a path, so the port
        // must emit FillRect where upstream calls fillRect
        (
            "fillRect",
            Draw::FillRect {
                m,
                alpha,
                color,
                rgba,
                rect,
            },
        ) => {
            check_matrix(&e["m"], m)?;
            check_alpha(&e["alpha"], *alpha)?;
            check_color(&e["fillStyle"], color, *rgba)?;
            let r: Vec<f64> = e["rect"].as_array().unwrap().iter().map(num).collect();
            if [rect.x, rect.y, rect.width, rect.height]
                .iter()
                .zip(&r)
                .all(|(a, b)| close(*a, *b))
            {
                Ok(())
            } else {
                Err(format!("rect {rect:?}, expected {r:?}"))
            }
        }
        (
            "stroke",
            Draw::Stroke {
                m,
                alpha,
                stroke,
                rgba,
                path: p,
            },
        ) => {
            check_matrix(&e["m"], m)?;
            check_alpha(&e["alpha"], *alpha)?;
            check_color(&e["strokeStyle"], &stroke.color, *rgba)?;
            if !close(num(&e["lineWidth"]), stroke.effective_width()) {
                return Err(format!(
                    "lineWidth {}, expected {}",
                    stroke.width, e["lineWidth"]
                ));
            }
            if e["lineCap"] != stroke.cap.as_css() || e["lineJoin"] != stroke.join.as_css() {
                return Err(format!(
                    "cap/join {:?} {:?}, expected {} {}",
                    stroke.cap, stroke.join, e["lineCap"], e["lineJoin"]
                ));
            }
            if !close(num(&e["miterLimit"]), stroke.effective_miter_limit()) {
                return Err(format!("miterLimit {}", stroke.miter_limit));
            }
            let dash: Vec<f64> = e["dash"].as_array().unwrap().iter().map(num).collect();
            let solid = dash.iter().sum::<f64>() <= 0.0;
            match (&stroke.dash, solid) {
                (None, true) => {}
                (Some(d), false) => {
                    let same = d.segments().len() == dash.len()
                        && d.segments().iter().zip(&dash).all(|(a, b)| close(*a, *b))
                        && close(d.offset(), num(&e["dashOffset"]));
                    if !same {
                        return Err(format!(
                            "dash {d:?}, expected {dash:?} at {}",
                            e["dashOffset"]
                        ));
                    }
                }
                (d, _) => return Err(format!("dash {d:?}, expected {dash:?}")),
            }
            check_path(&e["path"], p)
        }
        ("clip", Draw::Clip { m, clip }) => {
            check_matrix(&e["m"], m)?;
            if rule(&e["rule"]) != clip.rule {
                return Err(format!("clip rule {:?}, expected {}", clip.rule, e["rule"]));
            }
            check_path(&e["path"], &clip.path)
        }
        ("unclip", Draw::Unclip) => Ok(()),
        (
            "text",
            Draw::Text {
                m,
                alpha,
                run,
                rgba,
            },
        ) => {
            check_matrix(&e["m"], m)?;
            check_alpha(&e["alpha"], *alpha)?;
            check_color(&e["fillStyle"], &run.color, *rgba)?;
            let checks = [
                (e["text"] == run.text.as_str(), "text"),
                (
                    close(num(&e["x"]), run.x) && close(num(&e["y"]), run.y),
                    "position",
                ),
                (e["font"] == run.font.css().as_str(), "font"),
                (e["textAlign"] == run.align.as_css(), "textAlign"),
                (e["direction"] == run.direction.as_css(), "direction"),
            ];
            match checks.iter().find(|(ok, _)| !ok) {
                Some((_, what)) => Err(format!("text {what}: {run:?}")),
                None => Ok(()),
            }
        }
        ("image", Draw::Image { m, alpha, image }) => {
            check_matrix(&e["m"], m)?;
            check_alpha(&e["alpha"], *alpha)?;
            let name = &e["image"];
            let natural = if let Some(file) = name["file"].as_str() {
                if image.id != file {
                    return Err(format!("image {}, expected file {file}", image.id));
                }
                let n = &images[file];
                Some((num(&n["naturalWidth"]), num(&n["naturalHeight"])))
            } else {
                let builtin = name["builtin"].as_str().unwrap();
                let Some(expected) = builtin_image(builtin) else {
                    return Err(format!("no built-in image {builtin}"));
                };
                if image.id != expected.id {
                    return Err(format!("image {}, expected {}", image.id, expected.id));
                }
                if name["src"] != expected.data_url.as_str() {
                    return Err(format!(
                        "built-in {builtin}'s data URL differs from upstream's"
                    ));
                }
                None
            };
            let args: Vec<f64> = e["args"].as_array().unwrap().iter().map(num).collect();
            let (source, dest) = match args.len() {
                4 => (None, &args[..]),
                8 => (Some(&args[..4]), &args[4..]),
                n => return Err(format!("drawImage with {n} arguments")),
            };
            let d = image.dest;
            if ![d.x, d.y, d.width, d.height]
                .iter()
                .zip(dest)
                .all(|(a, b)| close(*a, *b))
            {
                return Err(format!("dest {d:?}, expected {dest:?}"));
            }
            let source_ok = match (source, image.source) {
                (None, None) => true,
                (Some(s), Some(r)) => [r.x, r.y, r.width, r.height]
                    .iter()
                    .zip(s)
                    .all(|(a, b)| close(*a, *b)),
                // the whole bitmap at its natural size
                (Some(s), None) => natural.is_some_and(|(w, h)| s == [0.0, 0.0, w, h]),
                (None, Some(_)) => false,
            };
            if !source_ok {
                return Err(format!("source {:?}, expected {source:?}", image.source));
            }
            let filter = match image.filter {
                None => "none",
                Some(f) => f.css(),
            };
            if e["filter"] != filter || e["smoothing"] != image.smoothing {
                return Err(format!(
                    "filter/smoothing {:?} {}",
                    image.filter, image.smoothing
                ));
            }
            Ok(())
        }
        _ => Err(format!("{draw:?}, expected {op}")),
    }
}

/// Replay `list` and hold its draws to upstream's `events`, in order:
/// `Ok(n)` with the number of draws compared, or why the first differs.
/// A `clearRect` clears a canvas the display list starts without, so it
/// may only come before the first draw.
pub fn compare(list: &DisplayList, events: &Value, images: &Value) -> Result<usize, String> {
    let mut recorder = Recorder::default();
    list.replay(&mut recorder);
    let expected: Vec<&Value> = events.as_array().unwrap().iter().collect();
    let first_draw = expected.iter().position(|e| e["op"] != "clear");
    let mut kept = Vec::new();
    for (i, e) in expected.into_iter().enumerate() {
        if e["op"] == "clear" {
            if first_draw.is_some_and(|f| i > f) {
                return Err("clearRect after a draw".to_owned());
            }
        } else {
            kept.push(e);
        }
    }
    for (i, (e, draw)) in kept.iter().zip(&recorder.0).enumerate() {
        check(e, draw, images).map_err(|why| format!("#{i}: {why}"))?;
    }
    if kept.len() != recorder.0.len() {
        return Err(format!(
            "{} draws, upstream made {}",
            recorder.0.len(),
            kept.len()
        ));
    }
    Ok(kept.len())
}
