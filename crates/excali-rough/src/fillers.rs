//! rough.js 4.6.4 `bin/fillers/*`: the pattern fills.
//!
//! `getFiller(o, helper)` picks by `o.fillStyle` (`zigzag`, `cross-hatch`,
//! `dots`, `dashed`, `zigzag-line`, and `hachure` for anything else); each
//! filler turns a polygon list into one `fillSketch` op set whose lines are
//! drawn with `doubleLineFillOps` (`dots`: small ellipses).
//!
//! Where a filler copies the options (`Object.assign({}, o, { ... })`), the
//! copy shares `o.randomizer` if it exists and otherwise gets its own,
//! created from the same seed on its first draw; [`with_copy`] does that.

use excali_math::js;

use crate::core::{Op, OpSet};
use crate::hachure_fill::{hachure_lines_in, Line, PolygonList};
use crate::random::math_random;
use crate::renderer::{double_line_fill_ops, ellipse};
use crate::{Options, Random};

const PI: f64 = std::f64::consts::PI;

/// `getFiller(o, helper).fillPolygons(polygonList, o)`.
pub(crate) fn fill_polygons(list: &mut PolygonList, o: &Options, rng: &mut Random) -> OpSet {
    match o.fill_style.as_str() {
        "zigzag" => zigzag(list, o, rng),
        "cross-hatch" => hatch(list, o, rng),
        "dots" => dots(list, o, rng),
        "dashed" => dashed(list, o, rng),
        "zigzag-line" => zigzag_line(list, o, rng),
        _ => hachure(list, o, rng),
    }
}

/// Runs `f` with the random sequence of a copy of the options: `rng` itself
/// if the original's randomizer exists (the copy shares it), otherwise a
/// fresh one from the same seed that the original never sees.
fn with_copy<T>(rng: &mut Random, seed: i32, f: impl FnOnce(&mut Random) -> T) -> T {
    if rng.has_drawn() {
        f(rng)
    } else {
        f(&mut Random::new(seed))
    }
}

/// The gap rough.js falls back to: `hachureGap`, or `strokeWidth * 4` when
/// it is negative.
fn gap_or_default(o: &Options) -> f64 {
    if o.hachure_gap < 0.0 {
        o.stroke_width * 4.0
    } else {
        o.hachure_gap
    }
}

/// `polygonHachureLines(polygonList, o)` (`scan-line-hachure.js`): the
/// hachure lines at `hachureAngle + 90`. From roughness 1 up, three times in
/// ten the scan steps by the gap instead of 1.
pub(crate) fn polygon_hachure_lines(
    list: &mut PolygonList,
    o: &Options,
    rng: &mut Random,
) -> Vec<Line> {
    let angle = o.hachure_angle + 90.0;
    let gap = js::max(gap_or_default(o), 0.1);
    let mut skip_offset = 1.0;
    if o.roughness >= 1.0 {
        // `(o.randomizer?.next() || Math.random()) > 0.7`
        let drawn = if rng.has_drawn() { rng.next() } else { 0.0 };
        let r = if drawn == 0.0 || drawn.is_nan() {
            math_random()
        } else {
            drawn
        };
        if r > 0.7 {
            skip_offset = gap;
        }
    }
    // `skipOffset || 1`
    let step = if skip_offset == 0.0 || skip_offset.is_nan() {
        1.0
    } else {
        skip_offset
    };
    hachure_lines_in(list, gap, angle, step)
}

/// `lineLength(line)` (`geometry.js`).
fn line_length(line: &Line) -> f64 {
    let [p1, p2] = line;
    ((p1[0] - p2[0]).powi(2) + (p1[1] - p2[1]).powi(2)).sqrt()
}

/// `HachureFiller.renderLines(lines, o)`.
fn render_lines(lines: &[Line], o: &Options, rng: &mut Random) -> Vec<Op> {
    let mut ops = Vec::new();
    for [a, b] in lines {
        ops.extend(double_line_fill_ops(a[0], a[1], b[0], b[1], o, rng));
    }
    ops
}

/// `HachureFiller._fillPolygons`.
fn hachure(list: &mut PolygonList, o: &Options, rng: &mut Random) -> OpSet {
    let lines = polygon_hachure_lines(list, o, rng);
    OpSet::fill_sketch(render_lines(&lines, o, rng))
}

/// `HatchFiller`: hachure, then hachure again at 90 degrees more over the
/// same (already rotated there and back) polygons.
fn hatch(list: &mut PolygonList, o: &Options, rng: &mut Random) -> OpSet {
    let mut set = hachure(list, o, rng);
    let o2 = Options {
        hachure_angle: o.hachure_angle + 90.0,
        ..o.clone()
    };
    let set2 = with_copy(rng, o.seed, |r| hachure(list, &o2, r));
    set.ops.extend(set2.ops);
    set
}

/// `ZigZagFiller`: each hachure line becomes two lines to its end from
/// points half a gap either side of its start.
fn zigzag(list: &mut PolygonList, o: &Options, rng: &mut Random) -> OpSet {
    let gap = js::max(gap_or_default(o), 0.1);
    let o2 = Options {
        hachure_gap: gap,
        ..o.clone()
    };
    let lines = with_copy(rng, o.seed, |r| polygon_hachure_lines(list, &o2, r));
    let zigzag_angle = (PI / 180.0) * o.hachure_angle;
    let dgx = gap * 0.5 * zigzag_angle.cos();
    let dgy = gap * 0.5 * zigzag_angle.sin();
    let mut zigzag_lines = Vec::new();
    for line in &lines {
        let length = line_length(line);
        // `if (lineLength([p1, p2]))`: 0 and NaN skip
        if length != 0.0 && !length.is_nan() {
            let [p1, p2] = *line;
            zigzag_lines.push([[p1[0] - dgx, p1[1] + dgy], p2]);
            zigzag_lines.push([[p1[0] + dgx, p1[1] - dgy], p2]);
        }
    }
    OpSet::fill_sketch(render_lines(&zigzag_lines, o, rng))
}

/// The line's ends ordered by x and its slope angle, as the dashed and
/// zigzag-line fillers compute them.
fn left_to_right(line: &Line) -> ([f64; 2], f64) {
    let (mut p1, mut p2) = (line[0], line[1]);
    if p1[0] > p2[0] {
        p1 = line[1];
        p2 = line[0];
    }
    let alpha = ((p2[1] - p1[1]) / (p2[0] - p1[0])).atan();
    (p1, alpha)
}

/// A loop count from rough.js's `for (let i = 0; i < count; i++)`. An
/// infinite count (a zero dash or zigzag length) never ends there; the port
/// draws nothing for that line instead.
fn loop_count(count: f64) -> u64 {
    if count.is_finite() && count > 0.0 {
        count.ceil() as u64
    } else {
        0
    }
}

/// `DashedFiller`: each hachure line cut into dashes of `dashOffset` with
/// gaps of `dashGap` (both default to the hachure gap), centred on the line.
fn dashed(list: &mut PolygonList, o: &Options, rng: &mut Random) -> OpSet {
    let lines = polygon_hachure_lines(list, o, rng);
    let offset = if o.dash_offset < 0.0 {
        gap_or_default(o)
    } else {
        o.dash_offset
    };
    let gap = if o.dash_gap < 0.0 {
        gap_or_default(o)
    } else {
        o.dash_gap
    };
    let mut ops = Vec::new();
    for line in &lines {
        let length = line_length(line);
        let count = (length / (offset + gap)).floor();
        let start_offset = (length + gap - (count * (offset + gap))) / 2.0;
        let (p1, alpha) = left_to_right(line);
        let (cos, sin) = (alpha.cos(), alpha.sin());
        for i in 0..loop_count(count) {
            let lstart = i as f64 * (offset + gap);
            let lend = lstart + offset;
            let start = [
                p1[0] + (lstart * cos) + (start_offset * cos),
                p1[1] + lstart * sin + (start_offset * sin),
            ];
            let end = [
                p1[0] + (lend * cos) + (start_offset * cos),
                p1[1] + (lend * sin) + (start_offset * sin),
            ];
            ops.extend(double_line_fill_ops(
                start[0], start[1], end[0], end[1], o, rng,
            ));
        }
    }
    OpSet::fill_sketch(ops)
}

/// `ZigZagLineFiller`: each hachure line (spaced `gap + zigzagOffset`)
/// drawn as a zigzag of teeth `zigzagOffset` long.
fn zigzag_line(list: &mut PolygonList, o: &Options, rng: &mut Random) -> OpSet {
    let gap = gap_or_default(o);
    let zo = if o.zigzag_offset < 0.0 {
        gap
    } else {
        o.zigzag_offset
    };
    let o = Options {
        hachure_gap: gap + zo,
        ..o.clone()
    };
    with_copy(rng, o.seed, |rng| {
        let lines = polygon_hachure_lines(list, &o, rng);
        let mut ops = Vec::new();
        for line in &lines {
            let length = line_length(line);
            let count = js::round(length / (2.0 * zo));
            let (p1, alpha) = left_to_right(line);
            for i in 0..loop_count(count) {
                let i = i as f64;
                let lstart = i * 2.0 * zo;
                let lend = (i + 1.0) * 2.0 * zo;
                let dz = (2.0 * zo.powi(2)).sqrt();
                let start = [p1[0] + (lstart * alpha.cos()), p1[1] + lstart * alpha.sin()];
                let end = [p1[0] + (lend * alpha.cos()), p1[1] + (lend * alpha.sin())];
                let middle = [
                    start[0] + dz * (alpha + PI / 4.0).cos(),
                    start[1] + dz * (alpha + PI / 4.0).sin(),
                ];
                ops.extend(double_line_fill_ops(
                    start[0], start[1], middle[0], middle[1], &o, rng,
                ));
                ops.extend(double_line_fill_ops(
                    middle[0], middle[1], end[0], end[1], &o, rng,
                ));
            }
        }
        OpSet::fill_sketch(ops)
    })
}

/// `DotFiller`: an ellipse `fillWeight` across every gap along vertical scan
/// lines, each jittered by up to a quarter gap with `Math.random` (so, unlike
/// every other fill, dots differ from call to call whatever the seed).
fn dots(list: &mut PolygonList, o: &Options, rng: &mut Random) -> OpSet {
    let o = Options {
        hachure_angle: 0.0,
        ..o.clone()
    };
    with_copy(rng, o.seed, |rng| {
        let lines = polygon_hachure_lines(list, &o, rng);
        let mut ops = Vec::new();
        let mut gap = o.hachure_gap;
        if gap < 0.0 {
            gap = o.stroke_width * 4.0;
        }
        gap = js::max(gap, 0.1);
        let mut fweight = o.fill_weight;
        if fweight < 0.0 {
            fweight = o.stroke_width / 2.0;
        }
        let ro = gap / 4.0;
        for line in &lines {
            let length = line_length(line);
            let dl = length / gap;
            let count = dl.ceil() - 1.0;
            let offset = length - (count * gap);
            let x = ((line[0][0] + line[1][0]) / 2.0) - (gap / 4.0);
            let min_y = js::min(line[0][1], line[1][1]);
            for i in 0..loop_count(count) {
                let y = min_y + offset + (i as f64 * gap);
                let cx = (x - ro) + math_random() * 2.0 * ro;
                let cy = (y - ro) + math_random() * 2.0 * ro;
                let el = ellipse(cx, cy, fweight, fweight, &o, rng);
                ops.extend(el.ops);
            }
        }
        OpSet::fill_sketch(ops)
    })
}
