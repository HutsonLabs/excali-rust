//! `packages/math/src/range.ts`.

use crate::js;
use crate::types::InclusiveRange;

/// `rangeInclusive(start, end)`.
pub const fn range_inclusive(start: f64, end: f64) -> InclusiveRange {
    InclusiveRange(start, end)
}

/// `rangeInclusiveFromPair([start, end])`.
pub const fn range_inclusive_from_pair(pair: [f64; 2]) -> InclusiveRange {
    InclusiveRange(pair[0], pair[1])
}

/// `rangesOverlap(a, b)`: e.g. `[1, 3]` overlaps `[2, 4]` but not `[4, 5]`.
pub fn ranges_overlap(a: InclusiveRange, b: InclusiveRange) -> bool {
    let InclusiveRange(a0, a1) = a;
    let InclusiveRange(b0, b1) = b;
    if a0 <= b0 {
        return a1 >= b0;
    }

    if a0 >= b0 {
        return b1 >= a0;
    }

    false
}

/// `rangeIntersection(a, b)`: e.g. `[1, 3] ∩ [2, 4] = [2, 3]`, or `None`.
pub fn range_intersection(a: InclusiveRange, b: InclusiveRange) -> Option<InclusiveRange> {
    let range_start = js::max(a.0, b.0);
    let range_end = js::min(a.1, b.1);

    if range_start <= range_end {
        return Some(InclusiveRange(range_start, range_end));
    }

    None
}

/// `rangeIncludesValue(value, [min, max])`.
pub fn range_includes_value(value: f64, range: InclusiveRange) -> bool {
    let InclusiveRange(min, max) = range;
    value >= min && value <= max
}
