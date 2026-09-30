//! `packages/math/src/pca.ts`: principal component analysis of a 2D point
//! set, and the standardized moments shape recognition reads from it.

use crate::js;
use crate::point::point_from;
use crate::types::{Point, Space, Vector};
use crate::vector::{vector, vector_normal, vector_normalize, vector_scale};

/// The principal axes of a point set, i.e. the eigen decomposition of its
/// 2x2 covariance matrix.
///
/// The axes are orthonormal and ordered by variance, so `major` is the
/// direction along which the points spread the most. Together with
/// `centroid` they define a canonical frame: expressing the points in it
/// removes translation and rotation, and dividing by
/// `sqrt(major_variance)` removes scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PrincipalAxes<S: Space> {
    pub centroid: Point<S>,
    /// Unit vector along the direction of largest variance.
    pub major: Vector,
    /// Unit vector along the direction of smallest variance, normal to
    /// major.
    pub minor: Vector,
    /// Variance along `major` (the larger eigenvalue, λ₁).
    pub major_variance: f64,
    /// Variance along `minor` (the smaller eigenvalue, λ₂).
    pub minor_variance: f64,
}

/// A point expressed in a principal axes frame: `u` along the major axis,
/// `v` along the minor axis, both relative to the centroid.
pub type PrincipalCoords = [f64; 2];

/// `centroid(points)`.
pub fn centroid<S: Space>(points: &[Point<S>]) -> Point<S> {
    let mut cx = 0.0;
    let mut cy = 0.0;
    for p in points {
        cx += p.x;
        cy += p.y;
    }
    let n = points.len() as f64;
    point_from(cx / n, cy / n)
}

/// `principalAxes(points)`: the centroid and the eigen decomposition of the
/// covariance matrix `[[μ20, μ11], [μ11, μ02]]`. Upstream logs an error for
/// fewer than two points and computes on; so does the port, silently.
pub fn principal_axes<S: Space>(points: &[Point<S>]) -> PrincipalAxes<S> {
    let c = centroid(points);

    let mut m20 = 0.0;
    let mut m02 = 0.0;
    let mut m11 = 0.0;
    for p in points {
        let dx = p.x - c.x;
        let dy = p.y - c.y;
        m20 += dx * dx;
        m02 += dy * dy;
        m11 += dx * dy;
    }
    let n = points.len() as f64;
    m20 /= n;
    m02 /= n;
    m11 /= n;

    // Eigenvalues of [[m20, m11], [m11, m02]].
    let trace = m20 + m02;
    let diff = js::hypot(m20 - m02, 2.0 * m11);
    let major_variance = (trace + diff) / 2.0;
    let minor_variance = (trace - diff) / 2.0;

    // Eigenvector for the larger eigenvalue. When m11 is 0 the covariance is
    // already diagonal and the axes are the coordinate axes.
    let major = if m11.abs() > f64::EPSILON {
        vector_normalize(vector(major_variance - m02, m11))
    } else if m20 >= m02 {
        vector(1.0, 0.0)
    } else {
        vector(0.0, 1.0)
    };

    PrincipalAxes {
        centroid: c,
        major,
        minor: vector_normal(major),
        major_variance,
        minor_variance,
    }
}

/// `principalCoords(points, axes)`: the points in the frame of `axes`.
pub fn principal_coords<S: Space>(
    points: &[Point<S>],
    axes: &PrincipalAxes<S>,
) -> Vec<PrincipalCoords> {
    principal_coords_with(points, axes, 1.0)
}

/// `principalCoords(points, axes, scale)`.
pub fn principal_coords_with<S: Space>(
    points: &[Point<S>],
    axes: &PrincipalAxes<S>,
    scale: f64,
) -> Vec<PrincipalCoords> {
    let PrincipalAxes {
        centroid: c,
        major,
        minor,
        ..
    } = *axes;
    points
        .iter()
        .map(|p| {
            let dx = p.x - c.x;
            let dy = p.y - c.y;
            [
                (dx * major.x + dy * major.y) * scale,
                (dx * minor.x + dy * minor.y) * scale,
            ]
        })
        .collect()
}

/// `orientPrincipalAxes(points, axes)`: the axes with `major` flipped, when
/// needed, to point toward the denser end of the point cloud (resolving the
/// eigenvector's sign). The variances are unchanged.
pub fn orient_principal_axes<S: Space>(
    points: &[Point<S>],
    axes: &PrincipalAxes<S>,
) -> PrincipalAxes<S> {
    let u: Vec<f64> = principal_coords(points, axes)
        .iter()
        .map(|[u, _]| *u)
        .collect();
    if skewness(&u) <= 0.0 {
        return *axes;
    }
    let major = vector_scale(axes.major, -1.0);
    PrincipalAxes {
        major,
        minor: vector_normal(major),
        ..*axes
    }
}

/// `elongation(axes)`: the minor over the major variance, in `[0, 1]`: 0
/// for a perfectly straight stroke, 1 for one with no preferred direction.
pub fn elongation<S: Space>(axes: &PrincipalAxes<S>) -> f64 {
    if axes.major_variance > 0.0 {
        axes.minor_variance / axes.major_variance
    } else {
        1.0
    }
}

/// `standardizedMoment(values, order)`: the `order`-th moment about the
/// mean over `sigma ** order`; 0 for a sample with no spread.
pub fn standardized_moment(values: &[f64], order: f64) -> f64 {
    let n = values.len() as f64;
    let mut mean = 0.0;
    for v in values {
        mean += v;
    }
    mean /= n;

    let mut variance = 0.0;
    let mut moment = 0.0;
    for v in values {
        let d = v - mean;
        variance += d * d;
        moment += js::pow(d, order);
    }
    variance /= n;
    moment /= n;

    let sigma = variance.sqrt();
    if sigma > f64::EPSILON {
        moment / js::pow(sigma, order)
    } else {
        0.0
    }
}

/// `skewness(values)`: the third standardized moment.
pub fn skewness(values: &[f64]) -> f64 {
    standardized_moment(values, 3.0)
}

/// `kurtosis(values)`: the fourth standardized moment (not excess
/// kurtosis: a normal sample gives 3).
pub fn kurtosis(values: &[f64]) -> f64 {
    standardized_moment(values, 4.0)
}
