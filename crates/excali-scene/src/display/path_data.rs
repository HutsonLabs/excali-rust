//! `new Path2D(d)`: SVG path data read into canvas path calls.
//!
//! Upstream fills a freedraw's outline with `context.fill(new
//! Path2D(svgPath))` (`renderElement.ts:496-516`). The constructor reads
//! the data with the SVG path grammar as Chrome does (Blink's
//! `SVGPathStringSource` and `SVGPathBuilder`):
//!
//! - numbers: an optional sign, digits with an optional fraction (or a
//!   fraction alone) and an optional exponent (`e`/`E` not followed by `x`
//!   or `m`, then an optional sign and digits); a sign or a second `.` ends
//!   a number and starts the next, so `1.5+21` is two numbers;
//! - separators: white space and at most one comma between parameters;
//! - the data starts with `M`/`m`; a command's parameters may repeat, and
//!   the repeats of a move are lines; parameters after `Z`/`z` are an
//!   error;
//! - arc flags are a single `0` or `1`;
//! - relative commands add the current point; `H`/`V` are lines; `S`/`T`
//!   reflect the previous curve's control point (or take the current point
//!   when the previous segment is not a curve of the same kind); after `Z`
//!   the current point is the subpath's start;
//! - at the first error the path keeps the segments read so far (the SVG
//!   specification's "render up to the error"), and the segment in error is
//!   dropped.
//!
//! Elliptical arcs become cubic Béziers of at most a quarter turn each
//! (Chrome draws them as conics; the two agree to a few millionths of the
//! radius). An arc with a zero radius is a line, one ending where it starts
//! adds nothing. Excalidraw's own path data (`getSvgPathFromStroke`) uses
//! only `M`, `Q`, `L` and `Z`.

use excali_math::js;
use std::f64::consts::{FRAC_PI_2, TAU};

use super::path::Path;

impl Path {
    /// The path `new Path2D(d)` holds.
    pub fn from_svg_path_data(d: &str) -> Path {
        let mut reader = Reader {
            s: d.as_bytes(),
            i: 0,
        };
        let mut b = Builder::default();
        reader.skip_spaces();
        let mut previous: Option<u8> = None;
        while !reader.at_end() {
            let command = match reader.command(previous) {
                Some(c) => c,
                None => break,
            };
            if previous.is_none() && !matches!(command, b'M' | b'm') {
                break;
            }
            if b.segment(command, &mut reader).is_none() {
                break;
            }
            previous = Some(command);
        }
        b.path
    }
}

struct Reader<'a> {
    s: &'a [u8],
    i: usize,
}

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

impl Reader<'_> {
    fn at_end(&self) -> bool {
        self.i >= self.s.len()
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn skip_spaces(&mut self) {
        while self.peek().is_some_and(is_space) {
            self.i += 1;
        }
    }

    /// White space, at most one comma, white space.
    fn skip_delimiter(&mut self) {
        self.skip_spaces();
        if self.peek() == Some(b',') {
            self.i += 1;
            self.skip_spaces();
        }
    }

    fn starts_number(&self) -> bool {
        self.peek()
            .is_some_and(|c| c.is_ascii_digit() || matches!(c, b'+' | b'-' | b'.'))
    }

    /// The next command letter, or the implicit repeat of `previous` when
    /// a number follows.
    fn command(&mut self, previous: Option<u8>) -> Option<u8> {
        let c = self.peek()?;
        if b"MmLlHhVvCcSsQqTtAaZz".contains(&c) {
            self.i += 1;
            self.skip_spaces();
            return Some(c);
        }
        if !self.starts_number() {
            return None;
        }
        match previous? {
            b'Z' | b'z' => None,
            b'M' => Some(b'L'),
            b'm' => Some(b'l'),
            p => Some(p),
        }
    }

    fn number(&mut self) -> Option<f64> {
        let start = self.i;
        let mut j = self.i;
        let s = self.s;
        if j < s.len() && matches!(s[j], b'+' | b'-') {
            j += 1;
        }
        let int_start = j;
        while j < s.len() && s[j].is_ascii_digit() {
            j += 1;
        }
        let mut digits = j > int_start;
        if j < s.len() && s[j] == b'.' {
            j += 1;
            let frac_start = j;
            while j < s.len() && s[j].is_ascii_digit() {
                j += 1;
            }
            digits |= j > frac_start;
        }
        if !digits {
            return None;
        }
        if j < s.len() && matches!(s[j], b'e' | b'E') && !matches!(s.get(j + 1), Some(b'x' | b'm'))
        {
            j += 1;
            if j < s.len() && matches!(s[j], b'+' | b'-') {
                j += 1;
            }
            let exp_start = j;
            while j < s.len() && s[j].is_ascii_digit() {
                j += 1;
            }
            if j == exp_start {
                return None;
            }
        }
        let text = std::str::from_utf8(&s[start..j]).ok()?;
        let value: f64 = text.parse().ok()?;
        self.i = j;
        self.skip_delimiter();
        Some(value)
    }

    fn flag(&mut self) -> Option<bool> {
        let f = match self.peek()? {
            b'0' => false,
            b'1' => true,
            _ => return None,
        };
        self.i += 1;
        self.skip_delimiter();
        Some(f)
    }

    fn numbers<const N: usize>(&mut self) -> Option<[f64; N]> {
        let mut out = [0.0; N];
        for v in &mut out {
            *v = self.number()?;
        }
        Some(out)
    }
}

#[derive(Default)]
struct Builder {
    path: Path,
    current: (f64, f64),
    start: (f64, f64),
    /// The previous segment's last control point and whether it was a
    /// cubic (`C`/`S`) or a quadratic (`Q`/`T`).
    last_cubic: Option<(f64, f64)>,
    last_quad: Option<(f64, f64)>,
}

impl Builder {
    /// Reads one segment's parameters and adds it; `None` on an error.
    fn segment(&mut self, command: u8, r: &mut Reader<'_>) -> Option<()> {
        let relative = command.is_ascii_lowercase();
        let (ox, oy) = if relative { self.current } else { (0.0, 0.0) };
        let mut cubic = None;
        let mut quad = None;
        match command.to_ascii_uppercase() {
            b'M' => {
                let [x, y] = r.numbers()?;
                let p = (ox + x, oy + y);
                self.path.move_to(p.0, p.1);
                self.current = p;
                self.start = p;
            }
            b'L' => {
                let [x, y] = r.numbers()?;
                self.line(ox + x, oy + y);
            }
            b'H' => {
                let [x] = r.numbers()?;
                self.line(ox + x, self.current.1);
            }
            b'V' => {
                let [y] = r.numbers()?;
                self.line(self.current.0, oy + y);
            }
            b'C' => {
                let [x1, y1, x2, y2, x, y] = r.numbers()?;
                self.path
                    .cubic_to(ox + x1, oy + y1, ox + x2, oy + y2, ox + x, oy + y);
                cubic = Some((ox + x2, oy + y2));
                self.current = (ox + x, oy + y);
            }
            b'S' => {
                let [x2, y2, x, y] = r.numbers()?;
                let (x1, y1) = self.reflect(self.last_cubic);
                self.path.cubic_to(x1, y1, ox + x2, oy + y2, ox + x, oy + y);
                cubic = Some((ox + x2, oy + y2));
                self.current = (ox + x, oy + y);
            }
            b'Q' => {
                let [x1, y1, x, y] = r.numbers()?;
                self.path.quad_to(ox + x1, oy + y1, ox + x, oy + y);
                quad = Some((ox + x1, oy + y1));
                self.current = (ox + x, oy + y);
            }
            b'T' => {
                let [x, y] = r.numbers()?;
                let (x1, y1) = self.reflect(self.last_quad);
                self.path.quad_to(x1, y1, ox + x, oy + y);
                quad = Some((x1, y1));
                self.current = (ox + x, oy + y);
            }
            b'A' => {
                let [rx, ry, rotation] = r.numbers()?;
                let large = r.flag()?;
                let sweep = r.flag()?;
                let [x, y] = r.numbers()?;
                self.arc(rx, ry, rotation, large, sweep, (ox + x, oy + y));
            }
            b'Z' => {
                self.path.close();
                self.current = self.start;
            }
            _ => return None,
        }
        self.last_cubic = cubic;
        self.last_quad = quad;
        Some(())
    }

    fn line(&mut self, x: f64, y: f64) {
        self.path.line_to(x, y);
        self.current = (x, y);
    }

    /// The previous control point reflected about the current point, or
    /// the current point.
    fn reflect(&self, control: Option<(f64, f64)>) -> (f64, f64) {
        let (cx, cy) = self.current;
        match control {
            Some((x, y)) => (2.0 * cx - x, 2.0 * cy - y),
            None => (cx, cy),
        }
    }

    /// The SVG arc from the current point to `end` (SVG 2, "Elliptical arc
    /// implementation notes", B.2.4 and B.2.5), as cubics.
    fn arc(&mut self, rx: f64, ry: f64, rotation: f64, large: bool, sweep: bool, end: (f64, f64)) {
        let (x1, y1) = self.current;
        let (x2, y2) = end;
        if x1 == x2 && y1 == y2 {
            return;
        }
        let (mut rx, mut ry) = (rx.abs(), ry.abs());
        if rx == 0.0 || ry == 0.0 {
            self.line(x2, y2);
            return;
        }
        let phi = rotation.to_radians();
        let (sin_phi, cos_phi) = (js::sin(phi), js::cos(phi));
        let dx = (x1 - x2) / 2.0;
        let dy = (y1 - y2) / 2.0;
        let x1p = cos_phi * dx + sin_phi * dy;
        let y1p = -sin_phi * dx + cos_phi * dy;
        let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
        if lambda > 1.0 {
            let s = lambda.sqrt();
            rx *= s;
            ry *= s;
        }
        let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
        let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
        let mut coef = (num / den).max(0.0).sqrt();
        if large == sweep {
            coef = -coef;
        }
        let cxp = coef * rx * y1p / ry;
        let cyp = -coef * ry * x1p / rx;
        let cx = cos_phi * cxp - sin_phi * cyp + (x1 + x2) / 2.0;
        let cy = sin_phi * cxp + cos_phi * cyp + (y1 + y2) / 2.0;
        let angle = |ux: f64, uy: f64, vx: f64, vy: f64| {
            let a = js::atan2(ux * vy - uy * vx, ux * vx + uy * vy);
            if a.is_nan() {
                0.0
            } else {
                a
            }
        };
        let (ux, uy) = ((x1p - cxp) / rx, (y1p - cyp) / ry);
        let (vx, vy) = ((-x1p - cxp) / rx, (-y1p - cyp) / ry);
        let theta1 = angle(1.0, 0.0, ux, uy);
        let mut delta = angle(ux, uy, vx, vy) % TAU;
        if !sweep && delta > 0.0 {
            delta -= TAU;
        } else if sweep && delta < 0.0 {
            delta += TAU;
        }
        let segments = (delta.abs() / FRAC_PI_2).ceil().max(1.0) as usize;
        let step = delta / segments as f64;
        let k = 4.0 / 3.0 * js::tan(step / 4.0);
        let point = |t: f64| {
            let (s, c) = (js::sin(t), js::cos(t));
            (
                cx + rx * c * cos_phi - ry * s * sin_phi,
                cy + rx * c * sin_phi + ry * s * cos_phi,
            )
        };
        let derivative = |t: f64| {
            let (s, c) = (js::sin(t), js::cos(t));
            (
                -rx * s * cos_phi - ry * c * sin_phi,
                -rx * s * sin_phi + ry * c * cos_phi,
            )
        };
        let mut t = theta1;
        for i in 0..segments {
            let next = if i + 1 == segments {
                theta1 + delta
            } else {
                t + step
            };
            let p0 = point(t);
            let d0 = derivative(t);
            let d1 = derivative(next);
            let p1 = if i + 1 == segments { end } else { point(next) };
            self.path.cubic_to(
                p0.0 + k * d0.0,
                p0.1 + k * d0.1,
                p1.0 - k * d1.0,
                p1.1 - k * d1.1,
                p1.0,
                p1.1,
            );
            t = next;
        }
        self.current = end;
    }
}
