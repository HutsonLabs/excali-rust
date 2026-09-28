//! Paths in the canvas's vocabulary, and the canonical form that backends
//! without canvas path semantics (tiny-skia, SVG) draw.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// The winding rule of a fill or clip: `fill(rule)`, `clip(rule)`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FillRule {
    /// `"nonzero"`, the canvas default.
    #[default]
    NonZero,
    /// `"evenodd"`: roughjs fills `curve`, `polygon` and `path` shapes this
    /// way, and upstream punches arrow-label holes with it.
    EvenOdd,
}

impl FillRule {
    /// The `CanvasFillRule` string.
    pub fn as_css(self) -> &'static str {
        match self {
            FillRule::NonZero => "nonzero",
            FillRule::EvenOdd => "evenodd",
        }
    }
}

/// One path-building call, with the argument order of the
/// `CanvasRenderingContext2D` method of the same name.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathCommand {
    /// `moveTo(x, y)`.
    MoveTo(f64, f64),
    /// `lineTo(x, y)`.
    LineTo(f64, f64),
    /// `quadraticCurveTo(cpx, cpy, x, y)`.
    QuadTo(f64, f64, f64, f64),
    /// `bezierCurveTo(cp1x, cp1y, cp2x, cp2y, x, y)`.
    CubicTo(f64, f64, f64, f64, f64, f64),
    /// `arc(cx, cy, radius, start, end, anticlockwise)`: angles in radians
    /// from the positive x axis, clockwise on screen.
    Arc {
        cx: f64,
        cy: f64,
        radius: f64,
        start: f64,
        end: f64,
        anticlockwise: bool,
    },
    /// `closePath()`.
    Close,
}

/// A path: the calls between `beginPath()` and `fill`/`stroke`/`clip`.
///
/// The commands are kept exactly as given, so a canvas backend replays them
/// verbatim and the browser applies its own rules. [`Path::canonical`]
/// applies those rules for backends that need explicit geometry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    pub commands: Vec<PathCommand>,
}

impl Path {
    /// An empty path (`beginPath()`).
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the path has no commands.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// `moveTo(x, y)`.
    pub fn move_to(&mut self, x: f64, y: f64) -> &mut Self {
        self.commands.push(PathCommand::MoveTo(x, y));
        self
    }

    /// `lineTo(x, y)`.
    pub fn line_to(&mut self, x: f64, y: f64) -> &mut Self {
        self.commands.push(PathCommand::LineTo(x, y));
        self
    }

    /// `quadraticCurveTo(cpx, cpy, x, y)`.
    pub fn quad_to(&mut self, cpx: f64, cpy: f64, x: f64, y: f64) -> &mut Self {
        self.commands.push(PathCommand::QuadTo(cpx, cpy, x, y));
        self
    }

    /// `bezierCurveTo(cp1x, cp1y, cp2x, cp2y, x, y)`.
    pub fn cubic_to(
        &mut self,
        cp1x: f64,
        cp1y: f64,
        cp2x: f64,
        cp2y: f64,
        x: f64,
        y: f64,
    ) -> &mut Self {
        self.commands
            .push(PathCommand::CubicTo(cp1x, cp1y, cp2x, cp2y, x, y));
        self
    }

    /// `arc(cx, cy, radius, start, end, anticlockwise)`.
    pub fn arc(
        &mut self,
        cx: f64,
        cy: f64,
        radius: f64,
        start: f64,
        end: f64,
        anticlockwise: bool,
    ) -> &mut Self {
        self.commands.push(PathCommand::Arc {
            cx,
            cy,
            radius,
            start,
            end,
            anticlockwise,
        });
        self
    }

    /// `closePath()`.
    pub fn close(&mut self) -> &mut Self {
        self.commands.push(PathCommand::Close);
        self
    }

    /// The path `rect(x, y, w, h)` builds: a closed subpath through the four
    /// corners, then a new subpath at `(x, y)` (HTML canvas, "The rect(x, y,
    /// w, h) method").
    pub fn rect(x: f64, y: f64, w: f64, h: f64) -> Self {
        let mut p = Path::new();
        p.move_to(x, y)
            .line_to(x + w, y)
            .line_to(x + w, y + h)
            .line_to(x, y + h)
            .close()
            .move_to(x, y);
        p
    }

    /// The path `roundRect(x, y, w, h, radius)` builds with one radius for
    /// every corner (HTML canvas, "The roundRect(x, y, w, h, radii)
    /// method"), as upstream clips rounded images
    /// (`renderElement.ts:528-538`) and frames (`staticScene.ts:165-189`):
    ///
    /// - a negative width or height moves the origin to the other side;
    /// - radii that do not fit are scaled by
    ///   `min(w / 2r, h / 2r)`;
    /// - the subpath starts after the top-left corner and runs clockwise
    ///   with a quarter arc per corner, is closed, and a new subpath starts
    ///   at `(x, y)`.
    ///
    /// Non-finite arguments add nothing (the method returns early), and so
    /// does a negative radius (the method throws a `RangeError`).
    pub fn round_rect(x: f64, y: f64, w: f64, h: f64, radius: f64) -> Self {
        let mut p = Path::new();
        if ![x, y, w, h, radius].iter().all(|v| v.is_finite()) || radius < 0.0 {
            return p;
        }
        let (x, w) = if w < 0.0 { (x + w, -w) } else { (x, w) };
        let (y, h) = if h < 0.0 { (y + h, -h) } else { (y, h) };
        let mut r = radius;
        if r > 0.0 {
            let scale = (w / (2.0 * r)).min(h / (2.0 * r));
            if scale < 1.0 {
                r *= scale;
            }
        }
        p.move_to(x + r, y)
            .line_to(x + w - r, y)
            .arc(x + w - r, y + r, r, -FRAC_PI_2, 0.0, false)
            .line_to(x + w, y + h - r)
            .arc(x + w - r, y + h - r, r, 0.0, FRAC_PI_2, false)
            .line_to(x + r, y + h)
            .arc(x + r, y + h - r, r, FRAC_PI_2, PI, false)
            .line_to(x, y + r)
            .arc(x + r, y + r, r, PI, PI + FRAC_PI_2, false)
            .close()
            .move_to(x, y);
        p
    }

    /// The same geometry with the canvas's path rules made explicit, using
    /// only `MoveTo`, `LineTo`, `QuadTo`, `CubicTo` and `Close`:
    ///
    /// - a command with an infinite or NaN argument is dropped (every path
    ///   method returns early on one), and so is an arc with a negative
    ///   radius (`arc` throws `IndexSizeError`);
    /// - a drawing command with no current subpath starts one: `lineTo` at
    ///   its end point, the curves at their first control point;
    /// - after `Close` the next drawing command continues from the closed
    ///   subpath's first point, which is written as an explicit `MoveTo`;
    /// - an arc draws a line from the current point to its start (or moves
    ///   there when there is no subpath), then its sweep as cubic Béziers of
    ///   at most a quarter turn each. The sweep is the whole circle when it
    ///   reaches 2π in the arc's direction, and otherwise the angle from
    ///   start to end going that way. A zero radius adds a line to the
    ///   centre.
    pub fn canonical(&self) -> Path {
        let mut out = Path::new();
        // The first point of the current subpath, and the current point.
        let mut start: Option<(f64, f64)> = None;
        let mut current: Option<(f64, f64)> = None;
        let mut closed = false;
        let finite = |vals: &[f64]| vals.iter().all(|v| v.is_finite());

        // Starts a subpath at `p` when there is none, or re-opens the one
        // just closed at its first point.
        let ensure = |out: &mut Path,
                      start: &mut Option<(f64, f64)>,
                      current: &mut Option<(f64, f64)>,
                      closed: &mut bool,
                      p: (f64, f64)| {
            if *closed {
                let s = start.expect("a closed subpath has a start");
                out.move_to(s.0, s.1);
                *current = Some(s);
                *closed = false;
            } else if current.is_none() {
                out.move_to(p.0, p.1);
                *start = Some(p);
                *current = Some(p);
            }
        };

        for command in &self.commands {
            match *command {
                PathCommand::MoveTo(x, y) => {
                    if !finite(&[x, y]) {
                        continue;
                    }
                    out.move_to(x, y);
                    start = Some((x, y));
                    current = Some((x, y));
                    closed = false;
                }
                PathCommand::LineTo(x, y) => {
                    if !finite(&[x, y]) {
                        continue;
                    }
                    if current.is_none() && !closed {
                        // "Ensure there is a subpath for (x, y)": the moveTo
                        // is the whole effect.
                        ensure(&mut out, &mut start, &mut current, &mut closed, (x, y));
                        continue;
                    }
                    ensure(&mut out, &mut start, &mut current, &mut closed, (x, y));
                    out.line_to(x, y);
                    current = Some((x, y));
                }
                PathCommand::QuadTo(cx, cy, x, y) => {
                    if !finite(&[cx, cy, x, y]) {
                        continue;
                    }
                    ensure(&mut out, &mut start, &mut current, &mut closed, (cx, cy));
                    out.quad_to(cx, cy, x, y);
                    current = Some((x, y));
                }
                PathCommand::CubicTo(c1x, c1y, c2x, c2y, x, y) => {
                    if !finite(&[c1x, c1y, c2x, c2y, x, y]) {
                        continue;
                    }
                    ensure(&mut out, &mut start, &mut current, &mut closed, (c1x, c1y));
                    out.cubic_to(c1x, c1y, c2x, c2y, x, y);
                    current = Some((x, y));
                }
                PathCommand::Arc {
                    cx,
                    cy,
                    radius,
                    start: a0,
                    end: a1,
                    anticlockwise,
                } => {
                    if !finite(&[cx, cy, radius, a0, a1]) || radius < 0.0 {
                        continue;
                    }
                    let sweep = arc_sweep(a0, a1, anticlockwise);
                    let first = (cx + radius * a0.cos(), cy + radius * a0.sin());
                    if current.is_none() && !closed {
                        ensure(&mut out, &mut start, &mut current, &mut closed, first);
                    } else {
                        ensure(&mut out, &mut start, &mut current, &mut closed, first);
                        out.line_to(first.0, first.1);
                    }
                    current = Some(first);
                    if radius == 0.0 || sweep == 0.0 {
                        continue;
                    }
                    let segments = (sweep.abs() / FRAC_PI_2).ceil().max(1.0) as usize;
                    let step = sweep / segments as f64;
                    let k = 4.0 / 3.0 * (step / 4.0).tan() * radius;
                    let mut angle = a0;
                    for i in 0..segments {
                        let next = if i + 1 == segments {
                            a0 + sweep
                        } else {
                            angle + step
                        };
                        let (s0, c0) = angle.sin_cos();
                        let (s1, c1) = next.sin_cos();
                        let (x0, y0) = (cx + radius * c0, cy + radius * s0);
                        let (x1, y1) = (cx + radius * c1, cy + radius * s1);
                        out.cubic_to(x0 - k * s0, y0 + k * c0, x1 + k * s1, y1 - k * c1, x1, y1);
                        angle = next;
                    }
                    let (s, c) = (a0 + sweep).sin_cos();
                    current = Some((cx + radius * c, cy + radius * s));
                }
                PathCommand::Close => {
                    if current.is_some() && !closed {
                        out.close();
                        closed = true;
                        current = start;
                    }
                }
            }
        }
        out
    }
}

/// The signed sweep of `arc(…, start, end, anticlockwise)`: positive
/// clockwise, negative anticlockwise, `±2π` for a whole circle.
fn arc_sweep(start: f64, end: f64, anticlockwise: bool) -> f64 {
    if !anticlockwise {
        let d = end - start;
        if d >= TAU {
            TAU
        } else {
            d.rem_euclid(TAU)
        }
    } else {
        let d = start - end;
        if d >= TAU {
            -TAU
        } else {
            -d.rem_euclid(TAU)
        }
    }
}
