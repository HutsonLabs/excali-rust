//! The laser pointer: upstream's vendored `@excalidraw/laser-pointer` 1.3.1
//! (`packages/laser-pointer/src/{state,math,simplify}.ts` at the pinned
//! commit), statement for statement.
//!
//! Points are `[x, y, r]`; `r` is the pressure the caller passes (the
//! laser tool passes a timestamp, freedraw a constant 1) and goes through
//! the same vector arithmetic as `x` and `y`, so outline points carry it too.
//! `x ** 2` is `x * x` (V8's `Math.pow` returns the correctly rounded
//! product for an exponent of 2), and `Math.sin`, `Math.cos` and
//! `Math.atan2` go through [`js::sin`], [`js::cos`] and [`js::atan2`]
//! (V8's fdlibm, ported), which return V8's doubles on every platform where
//! the platform's are an ulp away on a few percent of arguments (on 200,000
//! random arguments under Node 26 on macOS arm64: sin 4.1%, atan2 20.6%).
//! An ulp is enough to move an outline point across the hit testing
//! simplification tolerance (`CONSTANT_WIDTH_COLLISION_SIMPLIFY_TOLERANCE`).

use std::f64::consts::PI;
use std::fmt;
use std::sync::Arc;

use excali_math::js;

/// `Point`: `[x, y, r]`.
pub type LaserPoint = [f64; 3];

/// The argument of a [`SizeMapping`]: `SizeMappingDetails`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SizeMappingDetails {
    /// The point's `r`.
    pub pressure: f64,
    /// The length of the stroke up to the point (0 for the caps of one or two
    /// points and for the start cap).
    pub running_length: f64,
    /// The point's index in the stroke.
    pub current_index: usize,
    /// The number of points in the stroke (not a length, despite the name).
    pub total_length: usize,
}

/// `sizeMapping`: the factor applied to the size at a point.
pub type SizeMapping = Arc<dyn Fn(&SizeMappingDetails) -> f64 + Send + Sync>;

/// `simplifyPhase`: where Douglas-Peucker simplification (`simplify > 0`)
/// applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimplifyPhase {
    /// When the tail is stabilised: not implemented upstream, so it fails
    /// with [`LaserPointerError::NotImplemented`].
    Tail,
    /// On the outline (the default).
    Output,
    /// On the input points before the outline is built.
    Input,
}

/// `LaserPointerOptions`. [`Default`] is `LaserPointer.defaults`.
#[derive(Clone)]
pub struct LaserPointerOptions {
    /// The stroke radius. Default 2.
    pub size: f64,
    /// Each new point moves `1 - streamline` of the way from the last one.
    /// Default 0.45.
    pub streamline: f64,
    /// The Douglas-Peucker tolerance; 0 disables simplification. Default 0.1.
    pub simplify: f64,
    /// Default [`SimplifyPhase::Output`].
    pub simplify_phase: SimplifyPhase,
    /// Draw a dot at the head when nothing else is visible, and give the end
    /// cap the full size. Default `false`.
    pub keep_head: bool,
    /// Default `() => 1`.
    pub size_mapping: SizeMapping,
}

impl Default for LaserPointerOptions {
    fn default() -> Self {
        Self {
            size: 2.0,
            streamline: 0.45,
            simplify: 0.1,
            simplify_phase: SimplifyPhase::Output,
            keep_head: false,
            size_mapping: Arc::new(|_| 1.0),
        }
    }
}

impl fmt::Debug for LaserPointerOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LaserPointerOptions")
            .field("size", &self.size)
            .field("streamline", &self.streamline)
            .field("simplify", &self.simplify)
            .field("simplify_phase", &self.simplify_phase)
            .field("keep_head", &self.keep_head)
            .finish_non_exhaustive()
    }
}

/// What upstream throws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaserPointerError {
    /// `stabilizeTail` with `simplify > 0` and `simplifyPhase: "tail"`
    /// throws `new Error("Not implemented yet")`.
    NotImplemented,
}

impl fmt::Display for LaserPointerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LaserPointerError::NotImplemented => f.write_str("Not implemented yet"),
        }
    }
}

impl std::error::Error for LaserPointerError {}

/// `LaserPointer.constants.cornerDetectionMaxAngle`, in degrees.
pub const CORNER_DETECTION_MAX_ANGLE: f64 = 75.0;

/// `LaserPointer.constants.maxTailLength`: the tail is stabilised once its
/// [run length](run_length) exceeds this.
pub const MAX_TAIL_LENGTH: f64 = 50.0;

/// `LaserPointer`: builds the outline of a stroke point by point.
#[derive(Clone, Debug)]
pub struct LaserPointer {
    /// `options`, public and mutable as upstream's (the laser tool clears
    /// `keepHead` when a trail ends).
    pub options: LaserPointerOptions,
    original_points: Vec<LaserPoint>,
    stable_points: Vec<LaserPoint>,
    tail_points: Vec<LaserPoint>,
    is_fresh: bool,
}

impl LaserPointer {
    /// `new LaserPointer(options)`.
    pub fn new(options: LaserPointerOptions) -> Self {
        Self {
            options,
            original_points: Vec::new(),
            stable_points: Vec::new(),
            tail_points: Vec::new(),
            is_fresh: true,
        }
    }

    /// `LaserPointer.constants.cornerDetectionVariance`: the corner angle
    /// is halved above a smoothed speed of 35.
    pub fn corner_detection_variance(speed: f64) -> f64 {
        if speed > 35.0 {
            0.5
        } else {
            1.0
        }
    }

    /// `originalPoints`: the points [`add_point`](Self::add_point) kept,
    /// before streamlining.
    pub fn original_points(&self) -> &[LaserPoint] {
        &self.original_points
    }

    fn last_point(&self) -> LaserPoint {
        *self
            .tail_points
            .last()
            .or(self.stable_points.last())
            .expect("a pointer that is not fresh has a stable point")
    }

    /// `addPoint(point)`. A point at the same `x` and `y` as the last one
    /// added is ignored. Fails only where upstream throws: stabilising a tail
    /// longer than [`MAX_TAIL_LENGTH`] under [`SimplifyPhase::Tail`] with
    /// `simplify > 0`, after the point has been recorded.
    pub fn add_point(&mut self, point: LaserPoint) -> Result<(), LaserPointerError> {
        if let Some(last) = self.original_points.last() {
            if last[0] == point[0] && last[1] == point[1] {
                return Ok(());
            }
        }

        self.original_points.push(point);

        if self.is_fresh {
            self.is_fresh = false;
            self.stable_points.push(point);
            return Ok(());
        }

        let mut point = point;
        if self.options.streamline > 0.0 {
            point = plerp(self.last_point(), point, 1.0 - self.options.streamline);
        }

        self.tail_points.push(point);

        if run_length(&self.tail_points) > MAX_TAIL_LENGTH {
            self.stabilize_tail()?;
        }
        Ok(())
    }

    /// `close()`: stabilises the tail.
    pub fn close(&mut self) -> Result<(), LaserPointerError> {
        self.stabilize_tail()
    }

    /// `stabilizeTail()`: moves the tail points to the stable points.
    pub fn stabilize_tail(&mut self) -> Result<(), LaserPointerError> {
        if self.options.simplify > 0.0 && self.options.simplify_phase == SimplifyPhase::Tail {
            return Err(LaserPointerError::NotImplemented);
        }
        self.stable_points.append(&mut self.tail_points);
        Ok(())
    }

    /// `getSize(sizeOverride, pressure, index, totalLength, runningLength)`.
    fn get_size(
        &self,
        size_override: Option<f64>,
        pressure: f64,
        index: usize,
        total_length: usize,
        running_length: f64,
    ) -> f64 {
        size_override.unwrap_or(self.options.size)
            * (self.options.size_mapping)(&SizeMappingDetails {
                pressure,
                running_length,
                current_index: index,
                total_length,
            })
    }

    /// `getStrokeOutline(sizeOverride)`: the closed outline polygon of the
    /// stroke so far, with `size_override` in place of `options.size` where
    /// upstream uses it.
    pub fn get_stroke_outline(&self, size_override: Option<f64>) -> Vec<LaserPoint> {
        if self.is_fresh {
            return Vec::new();
        }

        let mut points: Vec<LaserPoint> = self
            .stable_points
            .iter()
            .chain(&self.tail_points)
            .copied()
            .collect();

        if self.options.simplify > 0.0 && self.options.simplify_phase == SimplifyPhase::Input {
            points = douglas_peucker(&points, self.options.simplify);
        }

        let len = points.len();

        if len == 0 {
            return Vec::new();
        }

        if len == 1 {
            let c = points[0];
            let size = self.get_size(size_override, c[2], 0, len, 0.0);
            if size < 0.5 {
                return Vec::new();
            }
            let mut ps = Vec::new();
            let mut theta = 0.0;
            while theta <= PI * 2.0 {
                ps.push(add(c, smul(rot([1.0, 0.0, 0.0], theta), size)));
                theta += PI / 16.0;
            }
            ps.push(add(
                c,
                smul(
                    [1.0, 0.0, 0.0],
                    self.get_size(size_override, c[2], 0, len, 0.0),
                ),
            ));
            return ps;
        }

        if len == 2 {
            let c = points[0];
            let n = points[1];
            let c_size = self.get_size(size_override, c[2], 0, len, 0.0);
            let n_size = self.get_size(size_override, n[2], 0, len, 0.0);
            if c_size < 0.5 || n_size < 0.5 {
                return Vec::new();
            }
            let mut ps = Vec::new();
            let p_angle = angle(c, [c[0], c[1] - 100.0, c[2]], n);
            let mut theta = p_angle;
            while theta <= PI + p_angle {
                ps.push(add(c, smul(rot([1.0, 0.0, 0.0], theta), c_size)));
                theta += PI / 16.0;
            }
            let mut theta = PI + p_angle;
            while theta <= PI * 2.0 + p_angle {
                ps.push(add(n, smul(rot([1.0, 0.0, 0.0], theta), n_size)));
                theta += PI / 16.0;
            }
            ps.push(ps[0]);
            return ps;
        }

        let mut forward_points: Vec<LaserPoint> = Vec::new();
        let mut backward_points: Vec<LaserPoint> = Vec::new();

        let mut prev_speed = 0.0;

        let mut visible_start_index = 0;
        let mut running_length = 0.0;

        for i in 1..len - 1 {
            let p = points[i - 1];
            let c = points[i];
            let n = points[i + 1];

            let pressure = c[2];

            let d = dist(p, c);
            running_length += d;
            let speed = prev_speed + (d - prev_speed) * 0.2;

            let c_size = self.get_size(size_override, pressure, i, len, running_length);

            if c_size == 0.0 {
                visible_start_index = i + 1;
                continue;
            }

            let dir_pc = norm(sub(p, c));
            let dir_nc = norm(sub(n, c));
            let p1dir_pc = rot(dir_pc, PI / 2.0);
            let p2dir_pc = rot(dir_pc, -PI / 2.0);
            let p1dir_nc = rot(dir_nc, PI / 2.0);
            let p2dir_nc = rot(dir_nc, -PI / 2.0);

            let p1_pc = add(c, smul(p1dir_pc, c_size));
            let p2_pc = add(c, smul(p2dir_pc, c_size));
            let p1_nc = add(c, smul(p1dir_nc, c_size));
            let p2_nc = add(c, smul(p2dir_nc, c_size));

            let ftdir = add(p1dir_pc, p2dir_nc);
            let btdir = add(p2dir_pc, p1dir_nc);

            let pa_pc = add(
                c,
                smul(
                    if mag(ftdir) == 0.0 {
                        dir_pc
                    } else {
                        norm(ftdir)
                    },
                    c_size,
                ),
            );
            let pa_nc = add(
                c,
                smul(
                    if mag(btdir) == 0.0 {
                        dir_nc
                    } else {
                        norm(btdir)
                    },
                    c_size,
                ),
            );

            let c_angle = norm_angle(angle(c, p, n));
            let d_angle =
                (CORNER_DETECTION_MAX_ANGLE / 180.0) * PI * Self::corner_detection_variance(speed);

            if c_angle.abs() < d_angle {
                // turn angle
                let t_angle = norm_angle(PI - c_angle).abs();

                if t_angle == 0.0 {
                    continue;
                }

                if c_angle < 0.0 {
                    backward_points.push(p2_pc);
                    backward_points.push(pa_nc);

                    let mut theta = 0.0;
                    while theta <= t_angle {
                        forward_points.push(add(c, rot(smul(p1dir_pc, c_size), theta)));
                        theta += t_angle / 4.0;
                    }

                    let mut theta = t_angle;
                    while theta >= 0.0 {
                        backward_points.push(add(c, rot(smul(p1dir_pc, c_size), theta)));
                        theta -= t_angle / 4.0;
                    }

                    backward_points.push(pa_nc);
                    backward_points.push(p1_nc);
                } else {
                    forward_points.push(p1_pc);
                    forward_points.push(pa_pc);

                    let mut theta = 0.0;
                    while theta <= t_angle {
                        backward_points.push(add(c, rot(smul(p1dir_pc, -c_size), -theta)));
                        theta += t_angle / 4.0;
                    }

                    let mut theta = t_angle;
                    while theta >= 0.0 {
                        forward_points.push(add(c, rot(smul(p1dir_pc, -c_size), -theta)));
                        theta -= t_angle / 4.0;
                    }
                    forward_points.push(pa_pc);
                    forward_points.push(p2_nc);
                }
            } else {
                forward_points.push(pa_pc);
                backward_points.push(pa_nc);
            }

            prev_speed = speed;
        }

        if visible_start_index >= len - 2 {
            if self.options.keep_head {
                let c = points[len - 1];
                let mut ps = Vec::new();
                let mut theta = 0.0;
                while theta <= PI * 2.0 {
                    ps.push(add(c, smul(rot([1.0, 0.0, 0.0], theta), self.options.size)));
                    theta += PI / 16.0;
                }
                ps.push(add(c, smul([1.0, 0.0, 0.0], self.options.size)));
                return ps;
            }
            return Vec::new();
        }

        let first = points[visible_start_index];
        let second = points[visible_start_index + 1];
        let penultimate = points[len - 2];
        let ultimate = points[len - 1];

        let dir_fs = norm(sub(second, first));
        let dir_pu = norm(sub(penultimate, ultimate));

        let ppdir_fs = rot(dir_fs, -PI / 2.0);
        let ppdir_pu = rot(dir_pu, PI / 2.0);

        let start_cap_size = self.get_size(size_override, first[2], 0, len, 0.0);
        // built in push order, then reversed: upstream unshifts
        let mut start_cap: Vec<LaserPoint> = Vec::new();

        let end_cap_size = if self.options.keep_head {
            self.options.size
        } else {
            self.get_size(size_override, penultimate[2], len - 2, len, running_length)
        };

        let mut end_cap: Vec<LaserPoint> = Vec::new();

        // Lowered threshold to 0.1,
        // ensuring virtually all strokes get proper rounded caps for visual consistency.
        if start_cap_size > 0.1 {
            let mut theta = 0.0;
            while theta <= PI {
                start_cap.push(add(first, rot(smul(ppdir_fs, start_cap_size), -theta)));
                theta += PI / 16.0;
            }
            start_cap.push(add(first, smul(ppdir_fs, -start_cap_size)));
            start_cap.reverse();
        } else {
            start_cap.push(first);
        }

        let mut theta = 0.0;
        while theta <= PI * 3.0 {
            end_cap.push(add(ultimate, rot(smul(ppdir_pu, -end_cap_size), -theta)));
            theta += PI / 16.0;
        }

        let mut stroke_outline = start_cap.clone();
        stroke_outline.extend(forward_points);
        stroke_outline.extend(end_cap.into_iter().rev());
        stroke_outline.extend(backward_points.into_iter().rev());

        if let Some(&start) = start_cap.first() {
            stroke_outline.push(start);
        }

        if self.options.simplify > 0.0 && self.options.simplify_phase == SimplifyPhase::Output {
            return douglas_peucker(&stroke_outline, self.options.simplify);
        }

        stroke_outline
    }
}

// -- math.ts --------------------------------------------------------------------

fn add([ax, ay, ar]: LaserPoint, [bx, by, br]: LaserPoint) -> LaserPoint {
    [ax + bx, ay + by, ar + br]
}

fn sub([ax, ay, ar]: LaserPoint, [bx, by, br]: LaserPoint) -> LaserPoint {
    [ax - bx, ay - by, ar - br]
}

fn smul([x, y, r]: LaserPoint, s: f64) -> LaserPoint {
    [x * s, y * s, r * s]
}

fn norm([x, y, r]: LaserPoint) -> LaserPoint {
    [x / (x * x + y * y).sqrt(), y / (x * x + y * y).sqrt(), r]
}

fn rot([x, y, r]: LaserPoint, rad: f64) -> LaserPoint {
    [
        js::cos(rad) * x - js::sin(rad) * y,
        js::sin(rad) * x + js::cos(rad) * y,
        r,
    ]
}

fn plerp(a: LaserPoint, b: LaserPoint, t: f64) -> LaserPoint {
    add(a, smul(sub(b, a), t))
}

fn angle(p: LaserPoint, p1: LaserPoint, p2: LaserPoint) -> f64 {
    js::atan2(p2[1] - p[1], p2[0] - p[0]) - js::atan2(p1[1] - p[1], p1[0] - p[0])
}

fn norm_angle(a: f64) -> f64 {
    js::atan2(js::sin(a), js::cos(a))
}

fn mag([x, y, _]: LaserPoint) -> f64 {
    (x * x + y * y).sqrt()
}

fn dist([ax, ay, _]: LaserPoint, [bx, by, _]: LaserPoint) -> f64 {
    ((bx - ax) * (bx - ax) + (by - ay) * (by - ay)).sqrt()
}

/// `runLength(ps)`: the length of the polyline, with the last segment
/// counted twice as upstream does; 0 below two points.
pub fn run_length(ps: &[LaserPoint]) -> f64 {
    if ps.len() < 2 {
        return 0.0;
    }
    let mut len = 0.0;
    for i in 1..ps.len() {
        len += dist(ps[i - 1], ps[i]);
    }
    len += dist(ps[ps.len() - 2], ps[ps.len() - 1]);
    len
}

fn clamp(v: f64, min: f64, max: f64) -> f64 {
    js::max(min, js::min(max, v))
}

/// `distancePointToSegment(p3, p1, p2)`.
fn distance_point_to_segment(p3: LaserPoint, p1: LaserPoint, p2: LaserPoint) -> f64 {
    let s_mag = dist(p1, p2);

    if s_mag == 0.0 {
        return dist(p3, p1);
    }

    let u = clamp(
        ((p3[0] - p1[0]) * (p2[0] - p1[0]) + (p3[1] - p1[1]) * (p2[1] - p1[1])) / (s_mag * s_mag),
        0.0,
        1.0,
    );

    let pi = [
        p1[0] + u * (p2[0] - p1[0]),
        p1[1] + u * (p2[1] - p1[1]),
        p3[2],
    ];

    dist(pi, p3)
}

// -- simplify.ts ----------------------------------------------------------------

/// `douglasPeucker(points, epsilon)` (`simplify.ts`), including upstream's
/// repetition of the split point at the head of the second half. An
/// `epsilon` of 0, or two points or fewer, return the points unchanged.
///
/// Where upstream would index `points[-1]` (a negative `epsilon` with
/// every point on the chord, so no point is farther than 0), the chord's
/// ends are returned.
pub fn douglas_peucker(points: &[LaserPoint], epsilon: f64) -> Vec<LaserPoint> {
    if epsilon == 0.0 {
        return points.to_vec();
    }

    if points.len() <= 2 {
        return points.to_vec();
    }

    let first = points[0];
    let last = points[points.len() - 1];

    let (max_distance, max_index) = points.iter().enumerate().fold(
        (0.0, None),
        |(max_distance, max_index), (index, &point)| {
            let distance = distance_point_to_segment(point, first, last);
            if distance > max_distance {
                (distance, Some(index))
            } else {
                (max_distance, max_index)
            }
        },
    );

    if let (true, Some(max_index)) = (max_distance >= epsilon, max_index) {
        let max_index_point = points[max_index];

        let mut left = vec![first];
        left.extend_from_slice(&points[1..max_index]);
        left.push(max_index_point);
        let mut left = douglas_peucker(&left, epsilon);
        left.pop();

        let mut right = vec![max_index_point];
        right.extend_from_slice(&points[max_index..points.len() - 1]);
        right.push(last);
        let right = douglas_peucker(&right, epsilon);

        let mut out = left;
        out.push(max_index_point);
        out.extend_from_slice(&right[1..]);
        return out;
    }
    vec![first, last]
}
