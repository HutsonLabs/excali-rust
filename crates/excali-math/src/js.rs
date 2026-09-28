//! JavaScript `Math` functions whose results differ from Rust's `f64`
//! methods, reimplemented so the port computes the doubles upstream computes.
//!
//! - `Math.hypot` is not libm's `hypot`: V8 (`src/builtins/math.tq`,
//!   `MathHypot`) scales by the largest magnitude and sums with Kahan
//!   compensation, which can differ from libm in the last bit.
//! - `Math.round` rounds halves towards +infinity (`-2.5` -> `-2`) and keeps
//!   the sign of zero (`-0.4` -> `-0`); `f64::round` rounds halves away from
//!   zero.
//! - `Math.min` / `Math.max` return NaN if either argument is NaN and order
//!   `-0` below `+0`; `f64::min` / `f64::max` ignore NaN.
//! - `Array.prototype.sort(comparator)` never fails, whatever the comparator
//!   answers (NaN included); `slice::sort_by` may panic when the ordering it
//!   is given is not total, so [`sort`] is a merge sort of its own.

/// `Math.hypot(a, b)` as V8 computes it.
pub fn hypot(a: f64, b: f64) -> f64 {
    let (a, b) = (a.abs(), b.abs());
    let one_arg_is_nan = a.is_nan() || b.is_nan();
    let mut max = 0.0_f64;
    for v in [a, b] {
        if !v.is_nan() && v > max {
            max = v;
        }
    }
    if max == f64::INFINITY {
        return f64::INFINITY;
    }
    if one_arg_is_nan {
        return f64::NAN;
    }
    if max == 0.0 {
        return 0.0;
    }
    // Kahan summation, normalised to the largest magnitude.
    let mut sum = 0.0_f64;
    let mut compensation = 0.0_f64;
    for v in [a, b] {
        let n = v / max;
        let summand = n * n - compensation;
        let preliminary = sum + summand;
        compensation = (preliminary - sum) - summand;
        sum = preliminary;
    }
    sum.sqrt() * max
}

/// `Math.round(x)`: the nearest integer, halves rounded towards +infinity,
/// the sign of zero preserved.
pub fn round(x: f64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    let floor = x.floor();
    // x - floor is exact for every finite double.
    let rounded = if x - floor >= 0.5 { floor + 1.0 } else { floor };
    if rounded == 0.0 && x < 0.0 {
        -0.0
    } else {
        rounded
    }
}

/// `Math.max(a, b)`.
pub fn max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == b {
        // Only the zeros compare equal with different bits: +0 wins.
        return if a.is_sign_negative() { b } else { a };
    }
    if a > b {
        a
    } else {
        b
    }
}

/// `Math.min(a, b)`.
pub fn min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == b {
        // Only the zeros compare equal with different bits: -0 wins.
        return if a.is_sign_negative() { a } else { b };
    }
    if a < b {
        a
    } else {
        b
    }
}

/// `array.sort(comparator)` for a comparator returning a JavaScript number:
/// stable, and `b` moves ahead of `a` only when `comparator(a, b) > 0`, so a
/// result of 0 or NaN keeps the pair in input order. For a consistent
/// comparator this is the order any stable sort produces (the one V8's
/// TimSort produces); for an inconsistent one (NaN coordinates) it returns
/// some permutation of the input instead of panicking, as JavaScript does.
pub fn sort<T: Clone>(items: &mut [T], mut comparator: impl FnMut(&T, &T) -> f64) {
    let mut scratch = items.to_vec();
    merge_sort(items, &mut scratch, &mut comparator);
}

fn merge_sort<T: Clone>(
    items: &mut [T],
    scratch: &mut [T],
    comparator: &mut impl FnMut(&T, &T) -> f64,
) {
    let len = items.len();
    if len <= 16 {
        // Stable insertion sort.
        for i in 1..len {
            let mut j = i;
            while j > 0 && comparator(&items[j - 1], &items[j]) > 0.0 {
                items.swap(j - 1, j);
                j -= 1;
            }
        }
        return;
    }
    let mid = len / 2;
    {
        let (left, right) = items.split_at_mut(mid);
        let (left_scratch, right_scratch) = scratch.split_at_mut(mid);
        merge_sort(left, left_scratch, comparator);
        merge_sort(right, right_scratch, comparator);
    }
    scratch[..len].clone_from_slice(items);
    let (left, right) = scratch[..len].split_at(mid);
    let (mut i, mut j) = (0, 0);
    for slot in items.iter_mut() {
        // Take from the right only when it must precede the left: ties and
        // NaN keep the left (earlier) element first.
        let take_right =
            i == left.len() || (j < right.len() && comparator(&left[i], &right[j]) > 0.0);
        if take_right {
            *slot = right[j].clone();
            j += 1;
        } else {
            *slot = left[i].clone();
            i += 1;
        }
    }
}
