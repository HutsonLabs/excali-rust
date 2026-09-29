//! `packages/math/src/angle.ts`.

use std::f64::consts::PI;

use crate::js;
use crate::types::{Degrees, Point, PolarCoords, Radians, Space};
use crate::utils::PRECISION;

/// `normalizeRadians(angle)`: the angle in `[0, 2π)` (`angle.ts:11`).
pub fn normalize_radians(angle: Radians) -> Radians {
    let a = angle.0;
    if a < 0.0 {
        Radians((a % (2.0 * PI)) + 2.0 * PI)
    } else {
        Radians(a % (2.0 * PI))
    }
}

/// `cartesian2Polar([x, y])`: the radius and normalised angle of a point
/// around the origin (`angle.ts:21`).
pub fn cartesian2_polar<S: Space>(p: Point<S>) -> PolarCoords {
    PolarCoords {
        radius: js::hypot(p.x, p.y),
        angle: normalize_radians(Radians(js::atan2(p.y, p.x))),
    }
}

/// `degreesToRadians(degrees)`.
pub fn degrees_to_radians(degrees: Degrees) -> Radians {
    Radians((degrees.0 * PI) / 180.0)
}

/// `radiansToDegrees(radians)`.
pub fn radians_to_degrees(radians: Radians) -> Degrees {
    Degrees((radians.0 * 180.0) / PI)
}

/// `isRightAngleRads(rads)`: whether the angle is a multiple of π/2
/// (`angle.ts:43`).
pub fn is_right_angle_rads(rads: Radians) -> bool {
    js::sin(2.0 * rads.0).abs() < PRECISION
}

/// `radiansBetweenAngles(a, min, max)`: whether `a` lies in the arc from
/// `min` to `max`, wrapping through 0 when `min > max` (`angle.ts:47`).
pub fn radians_between_angles(a: Radians, min: Radians, max: Radians) -> bool {
    let a = normalize_radians(a).0;
    let min = normalize_radians(min).0;
    let max = normalize_radians(max).0;

    if min < max {
        return a >= min && a <= max;
    }

    // The range wraps around the 0 angle
    a >= min || a <= max
}

/// `radiansDifference(a, b)`: the absolute angle between two directions, in
/// `[0, π]` (`angle.ts:64`).
pub fn radians_difference(a: Radians, b: Radians) -> Radians {
    let a = normalize_radians(a).0;
    let b = normalize_radians(b).0;

    let mut diff = a - b;

    if diff < -PI {
        diff += 2.0 * PI;
    } else if diff > PI {
        diff -= 2.0 * PI;
    }

    Radians(diff.abs())
}
