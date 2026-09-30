//! How a sticky note is painted: its outline, the shadow under it and the
//! creation-date footer, shared by the canvas ([`crate::render_element`])
//! and SVG ([`crate::svg_scene`]) renderers.
//!
//! Upstream: `packages/element/src/stickyNote.ts:190-370`
//! (`getStickyNoteRenderPoints`, `getStickyNotePathCommands`), `:425-500`
//! (`getStickyNoteDateLabel`, `getStickyNoteFooter`),
//! `packages/common/src/random.ts:19-33` (`seededRandom`) and the constants
//! of `packages/common/src/constants.ts:224-267`, at the pinned commit.
//!
//! - The outline is the note's box with each corner jittered by up to
//!   `min([0, 1.5, 8][roughness], 1.2% of the shorter side)`, drawn from
//!   the note's seed by mulberry32 ([`seeded_random`]); the shadow is the
//!   same outline offset by [`STICKY_NOTE_SHADOW_OFFSET`] with the seed
//!   plus one.
//! - Corners are rounded by quadratic curves when the note has roundness
//!   (4% of the shorter side, at most 16), and at roughness 2 one corner,
//!   picked by the seed, is lifted: bent inward by a few pixels (half as
//!   far for the shadow).
//! - The footer is the creation date, `"7 Sep"` or `"7 Sep 2025"`, in the
//!   viewer's local time, at the bottom right; the year is dropped in the
//!   current year and on a note too narrow for it. It reads a [`Clock`].

use excali_core::constants::{STICKY_NOTE_MIN_SIZE, STICKY_NOTE_PADDING};
use excali_core::element::Element;
use excali_math::js;

/// `STICKY_NOTE_SHADOW_OFFSET` (`constants.ts:264`).
pub const STICKY_NOTE_SHADOW_OFFSET: f64 = 3.0;
/// `STICKY_NOTE_SHADOW_OPACITY` (`constants.ts:265`).
pub const STICKY_NOTE_SHADOW_OPACITY: f64 = 0.16;
/// `STICKY_NOTE_EDGE_SHADOW_WIDTH` (`constants.ts:266`).
pub const STICKY_NOTE_EDGE_SHADOW_WIDTH: f64 = 0.5;
/// `STICKY_NOTE_EDGE_SHADOW_OPACITY` (`constants.ts:267`).
pub const STICKY_NOTE_EDGE_SHADOW_OPACITY: f64 = 0.08;
/// `STICKY_NOTE_FOOTER.fontSize` (`constants.ts:245-252`).
pub const STICKY_NOTE_FOOTER_FONT_SIZE: f64 = 12.0;
/// `STICKY_NOTE_FOOTER.fontFamily`.
pub const STICKY_NOTE_FOOTER_FONT_FAMILY: &str = "Helvetica, Arial, sans-serif";
/// `STICKY_NOTE_FOOTER.baselineFromBottom`.
pub const STICKY_NOTE_FOOTER_BASELINE_FROM_BOTTOM: f64 = 14.0;
/// `STICKY_NOTE_FOOTER.opacity`.
pub const STICKY_NOTE_FOOTER_OPACITY: f64 = 1.0;
/// `STICKY_NOTE_FOOTER.minBodyWidthForYear`.
pub const STICKY_NOTE_FOOTER_MIN_BODY_WIDTH_FOR_YEAR: f64 = 80.0;

/// `STICKY_NOTE_RENDER_ROUGHNESS` (`stickyNote.ts:63`).
const STICKY_NOTE_RENDER_ROUGHNESS: [f64; 3] = [0.0, 1.5, 8.0];
/// `STICKY_NOTE_CORNER_RADIUS_RATIO` (`stickyNote.ts:64`).
const STICKY_NOTE_CORNER_RADIUS_RATIO: f64 = 0.04;
/// `STICKY_NOTE_MAX_CORNER_RADIUS` (`stickyNote.ts:65`).
const STICKY_NOTE_MAX_CORNER_RADIUS: f64 = 16.0;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// What the footer reads besides the note: `Date.now()` and the viewer's
/// time zone.
#[derive(Clone, Copy, Debug)]
pub struct Clock {
    /// `Date.now()`: milliseconds since the epoch.
    pub now: f64,
    /// Minutes east of UTC at a time (`-new Date(t).getTimezoneOffset()`),
    /// which is how `Date#getDate`, `getMonth` and `getFullYear` read it.
    pub utc_offset_minutes: fn(f64) -> f64,
}

/// A time zone at UTC: `utc_offset_minutes` of 0 at every time.
pub fn utc(_time: f64) -> f64 {
    0.0
}

impl Clock {
    /// `now` in UTC.
    pub fn utc(now: f64) -> Clock {
        Clock {
            now,
            utc_offset_minutes: utc,
        }
    }
}

impl Default for Clock {
    /// The epoch, in UTC.
    fn default() -> Clock {
        Clock::utc(0.0)
    }
}

impl PartialEq for Clock {
    fn eq(&self, other: &Clock) -> bool {
        self.now.to_bits() == other.now.to_bits()
            && self.utc_offset_minutes as usize == other.utc_offset_minutes as usize
    }
}

/// `x >>> 0`: ToUint32.
fn to_uint32(x: f64) -> u32 {
    if !x.is_finite() {
        return 0;
    }
    (x.trunc().rem_euclid(4_294_967_296.0)) as u32
}

/// `seededRandom(seed)` (`common/src/random.ts:24-33`): mulberry32, numbers
/// in `[0, 1)`.
pub fn seeded_random(seed: f64) -> impl FnMut() -> f64 {
    let mut value = to_uint32(seed);
    move || {
        value = value.wrapping_add(0x6d2b_79f5);
        let mut next = value;
        next = (next ^ (next >> 15)).wrapping_mul(next | 1);
        next ^= next.wrapping_add((next ^ (next >> 7)).wrapping_mul(next | 61));
        f64::from(next ^ (next >> 14)) / 4_294_967_296.0
    }
}

/// A point of the outline, in the note's own coordinates.
pub type RenderPoint = [f64; 2];

/// `StickyNotePathCommand`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StickyNotePathCommand {
    Move(RenderPoint),
    Line(RenderPoint),
    Quadratic {
        control: RenderPoint,
        point: RenderPoint,
    },
}

impl StickyNotePathCommand {
    /// The point the command ends at.
    pub fn point(&self) -> RenderPoint {
        match *self {
            StickyNotePathCommand::Move(p) | StickyNotePathCommand::Line(p) => p,
            StickyNotePathCommand::Quadratic { point, .. } => point,
        }
    }
}

fn jitter(random: &mut impl FnMut() -> f64, amount: f64) -> f64 {
    (random() * 2.0 - 1.0) * amount
}

/// `pointAtDistance(from, to, distance)` (`stickyNote.ts:193-211`): the
/// point `distance` from `from` towards `to`, never past it.
fn point_at_distance(from: RenderPoint, to: RenderPoint, distance: f64) -> RenderPoint {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    let length = js::hypot(dx, dy);
    if length == 0.0 || length.is_nan() {
        return from;
    }
    let ratio = js::min(distance / length, 1.0);
    [from[0] + dx * ratio, from[1] + dy * ratio]
}

/// `getStickyNoteCornerRadius(element)` (`stickyNote.ts:213-224`).
pub fn get_sticky_note_corner_radius(element: &Element) -> f64 {
    let b = &element.base;
    if b.roundness.is_none() {
        return 0.0;
    }
    js::min(
        js::min(b.width, b.height) * STICKY_NOTE_CORNER_RADIUS_RATIO,
        STICKY_NOTE_MAX_CORNER_RADIUS,
    )
}

/// `getStickyNoteRenderPoints(element, { offsetX, offsetY, seedOffset })`
/// (`stickyNote.ts:226-273`): the four corners, top left first, clockwise.
pub fn get_sticky_note_render_points(
    element: &Element,
    offset_x: f64,
    offset_y: f64,
    seed_offset: f64,
) -> [RenderPoint; 4] {
    let b = &element.base;
    let roughness = js::max(0.0, js::min(2.0, js::round(b.roughness)));
    // STICKY_NOTE_RENDER_ROUGHNESS[NaN] is undefined, and Math.min with it NaN
    let level = if roughness.is_nan() {
        f64::NAN
    } else {
        STICKY_NOTE_RENDER_ROUGHNESS[roughness as usize]
    };
    let amount = js::min(level, js::min(b.width, b.height) * 0.012);

    if amount == 0.0 || amount.is_nan() {
        return [
            [offset_x, offset_y],
            [offset_x + b.width, offset_y],
            [offset_x + b.width, offset_y + b.height],
            [offset_x, offset_y + b.height],
        ];
    }

    let mut random = seeded_random(b.seed + seed_offset);
    let mut next = |base: f64| base + jitter(&mut random, amount);
    let p0 = [next(offset_x), next(offset_y)];
    let p1 = [next(offset_x + b.width), next(offset_y)];
    let p2 = [next(offset_x + b.width), next(offset_y + b.height)];
    let p3 = [next(offset_x), next(offset_y + b.height)];
    [p0, p1, p2, p3]
}

/// `getStickyNotePathCommands(element, { shadow })`
/// (`stickyNote.ts:275-370`): the outline (or its shadow) as a closed path
/// of moves, lines and quadratic curves.
pub fn get_sticky_note_path_commands(
    element: &Element,
    shadow: bool,
) -> Vec<StickyNotePathCommand> {
    use StickyNotePathCommand::{Line, Move, Quadratic};

    let b = &element.base;
    let points = if shadow {
        get_sticky_note_render_points(
            element,
            STICKY_NOTE_SHADOW_OFFSET,
            STICKY_NOTE_SHADOW_OFFSET,
            1.0,
        )
    } else {
        get_sticky_note_render_points(element, 0.0, 0.0, 0.0)
    };
    let radius = get_sticky_note_corner_radius(element);
    // The note's seed keeps the same corner for the paper and shadow
    // through redraws and resizes.
    let lifted_corner = if b.roughness == 2.0 {
        Some((seeded_random(b.seed)() * points.len() as f64).floor() as usize)
    } else {
        None
    };

    // `!radius`: 0 or NaN
    if (radius == 0.0 || radius.is_nan()) && lifted_corner.is_none() {
        let mut commands = vec![Move(points[0])];
        commands.extend(points[1..].iter().map(|&p| Line(p)));
        return commands;
    }

    let n = points.len();
    let corners: Vec<Vec<StickyNotePathCommand>> = points
        .iter()
        .enumerate()
        .map(|(index, &point)| {
            let prev = points[(index + n - 1) % n];
            let next = points[(index + 1) % n];
            let corner_radius = js::min(
                js::min(
                    radius,
                    js::hypot(point[0] - prev[0], point[1] - prev[1]) / 2.0,
                ),
                js::hypot(point[0] - next[0], point[1] - next[1]) / 2.0,
            );

            if lifted_corner == Some(index) {
                let size = js::min(b.width, b.height);
                let reach = js::min(size * 0.18, 40.0);
                // The shadow follows the paper inward at half the bend,
                // keeping its usual down-right offset from the light at the
                // top left.
                let lift = js::min(size * 0.02, 5.0) * if shadow { 0.5 } else { 1.0 };
                // The bend joins the straight edges before the corner,
                // keeping it local.
                let tip = [
                    point[0]
                        + if index == 1 || index == 2 {
                            -lift
                        } else {
                            lift
                        },
                    point[1] + if index >= 2 { -lift } else { lift },
                ];
                let start = point_at_distance(point, prev, reach);
                let end = point_at_distance(point, next, reach);
                return vec![
                    Line(start),
                    Quadratic {
                        control: point_at_distance(point, prev, reach / 2.0),
                        point: point_at_distance(tip, start, corner_radius),
                    },
                    Quadratic {
                        control: tip,
                        point: point_at_distance(tip, end, corner_radius),
                    },
                    Quadratic {
                        control: point_at_distance(point, next, reach / 2.0),
                        point: end,
                    },
                ];
            }

            vec![
                Line(point_at_distance(point, prev, corner_radius)),
                Quadratic {
                    control: point,
                    point: point_at_distance(point, next, corner_radius),
                },
            ]
        })
        .collect();

    let mut commands = vec![Move(corners[0][corners[0].len() - 1].point())];
    for index in 1..=n {
        commands.extend(corners[index % n].iter().copied());
    }
    commands
}

/// Days since the epoch as a proleptic Gregorian `(year, month0, day)`.
fn civil_from_days(days: f64) -> (f64, usize, f64) {
    // Howard Hinnant's days_from_civil inverse, in f64 (JS time values fit)
    let z = days + 719_468.0;
    let era = (z / 146_097.0).floor();
    let doe = z - era * 146_097.0;
    let yoe = ((doe - (doe / 1460.0).floor() + (doe / 36_524.0).floor()
        - (doe / 146_096.0).floor())
        / 365.0)
        .floor();
    let doy = doe - (365.0 * yoe + (yoe / 4.0).floor() - (yoe / 100.0).floor());
    let mp = ((5.0 * doy + 2.0) / 153.0).floor();
    let day = doy - ((153.0 * mp + 2.0) / 5.0).floor() + 1.0;
    let month0 = if mp < 10.0 { mp + 2.0 } else { mp - 10.0 };
    let year = yoe + era * 400.0 + if month0 <= 1.0 { 1.0 } else { 0.0 };
    (year, month0 as usize, day)
}

/// `new Date(time)` read in local time (`getFullYear`, `getMonth`,
/// `getDate`); `None` for an invalid date (`TimeClip`: beyond ±8.64e15 ms).
fn local_date(time: f64, clock: &Clock) -> Option<(f64, usize, f64)> {
    let time = time.trunc();
    if !time.is_finite() || time.abs() > 8.64e15 {
        return None;
    }
    let local = time + (clock.utc_offset_minutes)(time) * 60_000.0;
    Some(civil_from_days((local / 86_400_000.0).floor()))
}

/// `getStickyNoteDateLabel(created, { short, now })`
/// (`stickyNote.ts:447-465`): `"7 Sep"`, with the year appended unless
/// `short` or in `now`'s year; `None` without a usable timestamp.
pub fn get_sticky_note_date_label(
    created: Option<f64>,
    short: bool,
    clock: &Clock,
) -> Option<String> {
    let created = created.filter(|c| c.is_finite())?;
    let (year, month0, day) = local_date(created, clock)?;
    let label = format!("{} {}", day, MONTHS[month0]);
    let now_year = local_date(clock.now, clock).map(|(y, _, _)| y);
    if short || now_year == Some(year) {
        Some(label)
    } else {
        Some(format!("{label} {year}"))
    }
}

/// What the footer paints and where (`getStickyNoteFooter`,
/// `stickyNote.ts:471-500`), in the note's own coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct StickyNoteFooter {
    pub text: String,
    pub x: f64,
    pub y: f64,
}

/// `getStickyNoteFooter(element, now)`: `None` for a note under
/// `STICKY_NOTE_MIN_SIZE` either way (the 0×0 creation draft) and for one
/// without a timestamp.
pub fn get_sticky_note_footer(element: &Element, clock: &Clock) -> Option<StickyNoteFooter> {
    let b = &element.base;
    if b.width < STICKY_NOTE_MIN_SIZE || b.height < STICKY_NOTE_MIN_SIZE {
        return None;
    }
    let short = b.width - STICKY_NOTE_PADDING * 2.0 < STICKY_NOTE_FOOTER_MIN_BODY_WIDTH_FOR_YEAR;
    let text = get_sticky_note_date_label(b.created, short, clock)?;
    Some(StickyNoteFooter {
        text,
        x: b.width - STICKY_NOTE_PADDING,
        y: b.height - STICKY_NOTE_FOOTER_BASELINE_FROM_BOTTOM,
    })
}
