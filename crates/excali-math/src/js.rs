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
//! - `**` / `Math.pow` answer NaN for a NaN exponent and for `±1 **
//!   ±Infinity`, where `f64::powf` answers 1.
//! - `Array.prototype.sort(comparator)` never fails, whatever the comparator
//!   answers (NaN included), and for an inconsistent comparator the order it
//!   leaves is V8's TimSort's own; `slice::sort_by` may panic when the
//!   ordering it is given is not total, and would order differently anyway,
//!   so [`sort`] is a port of V8's TimSort.

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

/// `x ** y` and `Math.pow(x, y)` (ES `Number::exponentiate`): C's `pow`
/// except that a NaN exponent always gives NaN and `±1 ** ±Infinity` is NaN
/// (C answers 1 for both). The finite results are the platform's `pow`, the
/// same caveat as `Math.sin` and friends: on arm64 macOS it agrees with V8
/// on `x ** 2` and `x ** 3` (checked on 200 000 arguments while porting
/// `bezierEquation`), and `x ** 2` is exactly `x * x` everywhere.
pub fn pow(x: f64, y: f64) -> f64 {
    if y.is_nan() || (y.is_infinite() && x.abs() == 1.0) {
        return f64::NAN;
    }
    x.powf(y)
}

/// `array.sort(comparator)` as V8 runs it: TimSort, ported from
/// `third_party/v8/builtins/array-sort.tq` (`ArrayTimSort`,
/// `ArrayTimSortImpl` and the macros they call; V8 14.6, the one in Node
/// 26). `SortCompareUserFn` turns a NaN answer into +0, and every decision
/// is `order < 0` or its negation, so NaN counts as "not less". For a consistent comparator the
/// result is the stable order; for an inconsistent one (NaN or infinite
/// coordinates) it is the same permutation V8 produces, step for step,
/// instead of a panic. goldens/js-sort.json pins that against V8.
pub fn sort<T: Clone>(items: &mut [T], comparator: impl FnMut(&T, &T) -> f64) {
    let mut state = SortState {
        work: items,
        compare: comparator,
        min_gallop: MIN_GALLOP_WINS,
        runs: Vec::new(),
        temp: Vec::new(),
    };
    let length = state.work.len();
    if length < 8 {
        // `ArrayTimSort`: "Faster for small arrays."
        state.binary_insertion_sort(0, 0, length);
    } else {
        state.sort();
    }
}

/// `kMinGallopWins`: the initial `minGallop`, and the number of consecutive
/// wins that keeps a merge in galloping mode.
const MIN_GALLOP_WINS: usize = 7;

struct SortState<'a, T, F> {
    work: &'a mut [T],
    compare: F,
    min_gallop: usize,
    /// Pending runs as (base, length).
    runs: Vec<(usize, usize)>,
    temp: Vec<T>,
}

/// The `goto` targets of `MergeLow` / `MergeHigh`.
enum MergeEnd {
    Succeed,
    CopyOne,
}

impl<T: Clone, F: FnMut(&T, &T) -> f64> SortState<'_, T, F> {
    /// `order < 0` for `order = comparefn(a, b)`.
    fn less(&mut self, a: &T, b: &T) -> bool {
        (self.compare)(a, b) < 0.0
    }

    /// `ArrayTimSortImpl`.
    fn sort(&mut self) {
        let length = self.work.len();
        if length < 2 {
            return;
        }
        let mut remaining = length;
        let mut low = 0;
        let min_run_length = compute_min_run_length(remaining);
        while remaining != 0 {
            let mut current_run_length = self.count_and_make_run(low, low + remaining);
            if current_run_length < min_run_length {
                let forced_run_length = min_run_length.min(remaining);
                self.binary_insertion_sort(low, low + current_run_length, low + forced_run_length);
                current_run_length = forced_run_length;
            }
            self.runs.push((low, current_run_length));
            self.merge_collapse();
            low += current_run_length;
            remaining -= current_run_length;
        }
        self.merge_force_collapse();
    }

    /// `CountAndMakeRun`: the length of the run starting at `low_arg`,
    /// reversed in place if it is strictly descending.
    fn count_and_make_run(&mut self, low_arg: usize, high: usize) -> usize {
        let low = low_arg + 1;
        if low == high {
            return 1;
        }
        let mut run_length = 2;
        let element_low = self.work[low].clone();
        let element_low_pre = self.work[low - 1].clone();
        let is_descending = self.less(&element_low, &element_low_pre);
        let mut previous = element_low;
        for idx in low + 1..high {
            let current = self.work[idx].clone();
            // NaN compares as +0: it ends a descending run and extends an
            // ascending one.
            let less = self.less(&current, &previous);
            if is_descending != less {
                break;
            }
            previous = current;
            run_length += 1;
        }
        if is_descending {
            self.work[low_arg..low_arg + run_length].reverse();
        }
        run_length
    }

    /// `BinaryInsertionSort`: `[low, start_arg)` is sorted; insert the rest
    /// of `[low, high)` one by one, each after any element it ties with.
    fn binary_insertion_sort(&mut self, low: usize, start_arg: usize, high: usize) {
        let mut start = if low == start_arg {
            start_arg + 1
        } else {
            start_arg
        };
        while start < high {
            let mut left = low;
            let mut right = start;
            let pivot = self.work[start].clone();
            while left < right {
                let mid = left + ((right - left) >> 1);
                let element = self.work[mid].clone();
                if self.less(&pivot, &element) {
                    right = mid;
                } else {
                    left = mid + 1;
                }
            }
            self.work[left..=start].rotate_right(1);
            start += 1;
        }
    }

    /// `RunInvariantEstablished`.
    fn run_invariant_established(&self, n: usize) -> bool {
        if n < 2 {
            return true;
        }
        self.runs[n - 2].1 > self.runs[n - 1].1 + self.runs[n].1
    }

    /// `MergeCollapse`.
    fn merge_collapse(&mut self) {
        while self.runs.len() > 1 {
            let mut n = self.runs.len() - 2;
            if !self.run_invariant_established(n + 1) || !self.run_invariant_established(n) {
                if self.runs[n - 1].1 < self.runs[n + 1].1 {
                    n -= 1;
                }
                self.merge_at(n);
            } else if self.runs[n].1 <= self.runs[n + 1].1 {
                self.merge_at(n);
            } else {
                break;
            }
        }
    }

    /// `MergeForceCollapse`.
    fn merge_force_collapse(&mut self) {
        while self.runs.len() > 1 {
            let mut n = self.runs.len() - 2;
            if n > 0 && self.runs[n - 1].1 < self.runs[n + 1].1 {
                n -= 1;
            }
            self.merge_at(n);
        }
    }

    /// `MergeAt`: merges pending runs `i` and `i + 1`.
    fn merge_at(&mut self, i: usize) {
        let (mut base_a, mut length_a) = self.runs[i];
        let (base_b, mut length_b) = self.runs[i + 1];
        self.runs[i].1 = length_a + length_b;
        self.runs.remove(i + 1);

        // Where does b start in a? Elements in a before that are in place.
        let key_right = self.work[base_b].clone();
        let k = gallop_right(self, Source::Work, &key_right, base_a, length_a, 0);
        base_a += k;
        length_a -= k;
        if length_a == 0 {
            return;
        }

        // Where does a end in b? Elements in b after that are in place.
        let key_left = self.work[base_a + length_a - 1].clone();
        length_b = gallop_left(
            self,
            Source::Work,
            &key_left,
            base_b,
            length_b,
            length_b - 1,
        );
        if length_b == 0 {
            return;
        }

        if length_a <= length_b {
            self.merge_low(base_a, length_a, base_b, length_b);
        } else {
            self.merge_high(base_a, length_a, base_b, length_b);
        }
    }

    /// `MergeLow`: merges in place, copying run a (the shorter) to temp.
    fn merge_low(
        &mut self,
        base_a: usize,
        length_a_arg: usize,
        base_b: usize,
        length_b_arg: usize,
    ) {
        let mut length_a = length_a_arg;
        let mut length_b = length_b_arg;
        self.temp = self.work[base_a..base_a + length_a].to_vec();
        let mut dest = base_a;
        let mut cursor_temp = 0;
        let mut cursor_b = base_b;

        self.work[dest] = self.work[cursor_b].clone();
        dest += 1;
        cursor_b += 1;

        let end = 'merge: {
            length_b -= 1;
            if length_b == 0 {
                break 'merge MergeEnd::Succeed;
            }
            if length_a == 1 {
                break 'merge MergeEnd::CopyOne;
            }
            let mut min_gallop = self.min_gallop;
            loop {
                let mut nof_wins_a = 0;
                let mut nof_wins_b = 0;
                // One pair at a time until one run wins min_gallop times in a row.
                loop {
                    let b = self.work[cursor_b].clone();
                    let a = self.temp[cursor_temp].clone();
                    if self.less(&b, &a) {
                        self.work[dest] = b;
                        dest += 1;
                        cursor_b += 1;
                        nof_wins_b += 1;
                        length_b -= 1;
                        nof_wins_a = 0;
                        if length_b == 0 {
                            break 'merge MergeEnd::Succeed;
                        }
                        if nof_wins_b >= min_gallop {
                            break;
                        }
                    } else {
                        self.work[dest] = a;
                        dest += 1;
                        cursor_temp += 1;
                        nof_wins_a += 1;
                        length_a -= 1;
                        nof_wins_b = 0;
                        if length_a == 1 {
                            break 'merge MergeEnd::CopyOne;
                        }
                        if nof_wins_a >= min_gallop {
                            break;
                        }
                    }
                }

                // Galloping, until neither run wins MIN_GALLOP_WINS at a time.
                min_gallop += 1;
                let mut first_iteration = true;
                while nof_wins_a >= MIN_GALLOP_WINS
                    || nof_wins_b >= MIN_GALLOP_WINS
                    || first_iteration
                {
                    first_iteration = false;
                    min_gallop = min_gallop.saturating_sub(1).max(1);
                    self.min_gallop = min_gallop;

                    let key = self.work[cursor_b].clone();
                    nof_wins_a = gallop_right(self, Source::Temp, &key, cursor_temp, length_a, 0);
                    if nof_wins_a > 0 {
                        self.work[dest..dest + nof_wins_a]
                            .clone_from_slice(&self.temp[cursor_temp..cursor_temp + nof_wins_a]);
                        dest += nof_wins_a;
                        cursor_temp += nof_wins_a;
                        length_a -= nof_wins_a;
                        if length_a == 1 {
                            break 'merge MergeEnd::CopyOne;
                        }
                        // Impossible for a consistent comparator, but it may not be.
                        if length_a == 0 {
                            break 'merge MergeEnd::Succeed;
                        }
                    }
                    self.work[dest] = self.work[cursor_b].clone();
                    dest += 1;
                    cursor_b += 1;
                    length_b -= 1;
                    if length_b == 0 {
                        break 'merge MergeEnd::Succeed;
                    }

                    let key = self.temp[cursor_temp].clone();
                    nof_wins_b = gallop_left(self, Source::Work, &key, cursor_b, length_b, 0);
                    if nof_wins_b > 0 {
                        copy_within(self.work, cursor_b, dest, nof_wins_b);
                        dest += nof_wins_b;
                        cursor_b += nof_wins_b;
                        length_b -= nof_wins_b;
                        if length_b == 0 {
                            break 'merge MergeEnd::Succeed;
                        }
                    }
                    self.work[dest] = self.temp[cursor_temp].clone();
                    dest += 1;
                    cursor_temp += 1;
                    length_a -= 1;
                    if length_a == 1 {
                        break 'merge MergeEnd::CopyOne;
                    }
                }
                // Penalize leaving galloping mode.
                min_gallop += 1;
                self.min_gallop = min_gallop;
            }
        };

        match end {
            MergeEnd::Succeed => {
                if length_a > 0 {
                    self.work[dest..dest + length_a]
                        .clone_from_slice(&self.temp[cursor_temp..cursor_temp + length_a]);
                }
            }
            MergeEnd::CopyOne => {
                // The last element of run a belongs at the end of the merge.
                copy_within(self.work, cursor_b, dest, length_b);
                self.work[dest + length_b] = self.temp[cursor_temp].clone();
            }
        }
    }

    /// `MergeHigh`: merges backwards in place, copying run b (the shorter)
    /// to temp.
    fn merge_high(
        &mut self,
        base_a: usize,
        length_a_arg: usize,
        base_b: usize,
        length_b_arg: usize,
    ) {
        let mut length_a = length_a_arg;
        let mut length_b = length_b_arg;
        self.temp = self.work[base_b..base_b + length_b].to_vec();
        // Cursors can step one below zero (V8's Smis are signed): isize.
        let mut dest = (base_b + length_b - 1) as isize;
        let mut cursor_temp = length_b as isize - 1;
        let mut cursor_a = (base_a + length_a - 1) as isize;

        self.work[dest as usize] = self.work[cursor_a as usize].clone();
        dest -= 1;
        cursor_a -= 1;

        let end = 'merge: {
            length_a -= 1;
            if length_a == 0 {
                break 'merge MergeEnd::Succeed;
            }
            if length_b == 1 {
                break 'merge MergeEnd::CopyOne;
            }
            let mut min_gallop = self.min_gallop;
            loop {
                let mut nof_wins_a = 0;
                let mut nof_wins_b = 0;
                loop {
                    let b = self.temp[cursor_temp as usize].clone();
                    let a = self.work[cursor_a as usize].clone();
                    if self.less(&b, &a) {
                        self.work[dest as usize] = a;
                        dest -= 1;
                        cursor_a -= 1;
                        nof_wins_a += 1;
                        length_a -= 1;
                        nof_wins_b = 0;
                        if length_a == 0 {
                            break 'merge MergeEnd::Succeed;
                        }
                        if nof_wins_a >= min_gallop {
                            break;
                        }
                    } else {
                        self.work[dest as usize] = b;
                        dest -= 1;
                        cursor_temp -= 1;
                        nof_wins_b += 1;
                        length_b -= 1;
                        nof_wins_a = 0;
                        if length_b == 1 {
                            break 'merge MergeEnd::CopyOne;
                        }
                        if nof_wins_b >= min_gallop {
                            break;
                        }
                    }
                }

                min_gallop += 1;
                let mut first_iteration = true;
                while nof_wins_a >= MIN_GALLOP_WINS
                    || nof_wins_b >= MIN_GALLOP_WINS
                    || first_iteration
                {
                    first_iteration = false;
                    min_gallop = min_gallop.saturating_sub(1).max(1);
                    self.min_gallop = min_gallop;

                    let key = self.temp[cursor_temp as usize].clone();
                    let k = gallop_right(self, Source::Work, &key, base_a, length_a, length_a - 1);
                    nof_wins_a = length_a - k;
                    if nof_wins_a > 0 {
                        dest -= nof_wins_a as isize;
                        cursor_a -= nof_wins_a as isize;
                        copy_within(
                            self.work,
                            (cursor_a + 1) as usize,
                            (dest + 1) as usize,
                            nof_wins_a,
                        );
                        length_a -= nof_wins_a;
                        if length_a == 0 {
                            break 'merge MergeEnd::Succeed;
                        }
                    }
                    self.work[dest as usize] = self.temp[cursor_temp as usize].clone();
                    dest -= 1;
                    cursor_temp -= 1;
                    length_b -= 1;
                    if length_b == 1 {
                        break 'merge MergeEnd::CopyOne;
                    }

                    let key = self.work[cursor_a as usize].clone();
                    let k = gallop_left(self, Source::Temp, &key, 0, length_b, length_b - 1);
                    nof_wins_b = length_b - k;
                    if nof_wins_b > 0 {
                        dest -= nof_wins_b as isize;
                        cursor_temp -= nof_wins_b as isize;
                        let from = (cursor_temp + 1) as usize;
                        let to = (dest + 1) as usize;
                        self.work[to..to + nof_wins_b]
                            .clone_from_slice(&self.temp[from..from + nof_wins_b]);
                        length_b -= nof_wins_b;
                        if length_b == 1 {
                            break 'merge MergeEnd::CopyOne;
                        }
                        // Impossible for a consistent comparator, but it may not be.
                        if length_b == 0 {
                            break 'merge MergeEnd::Succeed;
                        }
                    }
                    self.work[dest as usize] = self.work[cursor_a as usize].clone();
                    dest -= 1;
                    cursor_a -= 1;
                    length_a -= 1;
                    if length_a == 0 {
                        break 'merge MergeEnd::Succeed;
                    }
                }
                min_gallop += 1;
                self.min_gallop = min_gallop;
            }
        };

        match end {
            MergeEnd::Succeed => {
                if length_b > 0 {
                    let to = (dest + 1) as usize - length_b;
                    self.work[to..to + length_b].clone_from_slice(&self.temp[..length_b]);
                }
            }
            MergeEnd::CopyOne => {
                // The first element of run b belongs at the front of the merge.
                dest -= length_a as isize;
                cursor_a -= length_a as isize;
                copy_within(
                    self.work,
                    (cursor_a + 1) as usize,
                    (dest + 1) as usize,
                    length_a,
                );
                self.work[dest as usize] = self.temp[cursor_temp as usize].clone();
            }
        }
    }
}

/// Which array a gallop searches: V8 passes `workArray` or `tempArray`.
#[derive(Clone, Copy)]
enum Source {
    Work,
    Temp,
}

fn element<T: Clone, F>(state: &SortState<'_, T, F>, source: Source, i: usize) -> T {
    match source {
        Source::Work => state.work[i].clone(),
        Source::Temp => state.temp[i].clone(),
    }
}

/// `GallopLeft`: the leftmost position in the sorted `[base, base + length)`
/// at which `key` belongs (after every element less than it), searching
/// out from `hint`.
fn gallop_left<T: Clone, F: FnMut(&T, &T) -> f64>(
    state: &mut SortState<'_, T, F>,
    source: Source,
    key: &T,
    base: usize,
    length: usize,
    hint: usize,
) -> usize {
    let mut lo = 0;
    let mut ofs = 1;
    let hint_element = element(state, source, base + hint);
    let (mut last_ofs, mut offset) = if state.less(&hint_element, key) {
        // a[hint] < key: gallop right until a[hint + lo] < key <= a[hint + ofs].
        let max_ofs = length - hint;
        while ofs < max_ofs {
            let e = element(state, source, base + hint + ofs);
            if !state.less(&e, key) {
                break;
            }
            lo = ofs;
            ofs = (ofs << 1) + 1;
        }
        ofs = ofs.min(max_ofs);
        (lo + hint + 1, ofs + hint)
    } else {
        // key <= a[hint]: gallop left until a[hint - ofs] < key <= a[hint - lo].
        let max_ofs = hint + 1;
        while ofs < max_ofs {
            let e = element(state, source, base + hint - ofs);
            if state.less(&e, key) {
                break;
            }
            lo = ofs;
            ofs = (ofs << 1) + 1;
        }
        ofs = ofs.min(max_ofs);
        // lastOfs = hint - ofs (possibly -1), then lastOfs++.
        (hint + 1 - ofs, hint - lo)
    };
    // a[last_ofs - 1] < key <= a[offset]: binary search in between.
    while last_ofs < offset {
        let m = last_ofs + ((offset - last_ofs) >> 1);
        let e = element(state, source, base + m);
        if state.less(&e, key) {
            last_ofs = m + 1;
        } else {
            offset = m;
        }
    }
    offset
}

/// `GallopRight`: the rightmost position in the sorted `[base, base +
/// length)` at which `key` belongs (after every element not greater than
/// it), searching out from `hint`.
fn gallop_right<T: Clone, F: FnMut(&T, &T) -> f64>(
    state: &mut SortState<'_, T, F>,
    source: Source,
    key: &T,
    base: usize,
    length: usize,
    hint: usize,
) -> usize {
    let mut lo = 0;
    let mut ofs = 1;
    let hint_element = element(state, source, base + hint);
    let (mut last_ofs, mut offset) = if state.less(key, &hint_element) {
        // key < a[hint]: gallop left until a[hint - ofs] <= key < a[hint - lo].
        let max_ofs = hint + 1;
        while ofs < max_ofs {
            let e = element(state, source, base + hint - ofs);
            if !state.less(key, &e) {
                break;
            }
            lo = ofs;
            ofs = (ofs << 1) + 1;
        }
        ofs = ofs.min(max_ofs);
        // lastOfs = hint - ofs (possibly -1), then lastOfs++.
        (hint + 1 - ofs, hint - lo)
    } else {
        // a[hint] <= key: gallop right until a[hint + lo] <= key < a[hint + ofs].
        let max_ofs = length - hint;
        while ofs < max_ofs {
            let e = element(state, source, base + hint + ofs);
            if state.less(key, &e) {
                break;
            }
            lo = ofs;
            ofs = (ofs << 1) + 1;
        }
        ofs = ofs.min(max_ofs);
        (lo + hint + 1, ofs + hint)
    };
    while last_ofs < offset {
        let m = last_ofs + ((offset - last_ofs) >> 1);
        let e = element(state, source, base + m);
        if state.less(key, &e) {
            offset = m;
        } else {
            last_ofs = m + 1;
        }
    }
    offset
}

/// `ComputeMinRunLength`: n itself below 64, else a length in [32, 64] such
/// that n / length is a power of two or just under one.
fn compute_min_run_length(n_arg: usize) -> usize {
    let mut n = n_arg;
    let mut r = 0;
    while n >= 64 {
        r |= n & 1;
        n >>= 1;
    }
    n + r
}

/// V8's `Copy` within one array: `memmove` semantics.
fn copy_within<T: Clone>(items: &mut [T], src: usize, dst: usize, len: usize) {
    if src < dst {
        for i in (0..len).rev() {
            items[dst + i] = items[src + i].clone();
        }
    } else {
        for i in 0..len {
            items[dst + i] = items[src + i].clone();
        }
    }
}
