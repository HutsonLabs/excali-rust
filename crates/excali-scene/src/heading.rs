//! Cardinal headings (`packages/element/src/heading.ts:37-67`): the four
//! directions an elbow arrow's segments run in.
//!
//! Upstream's `Heading` is a `[1, 0] | [0, 1] | [-1, 0] | [0, -1]` tuple;
//! here it is an enum with [`Heading::vector`] giving the tuple back.

/// `Heading`: `HEADING_RIGHT`, `HEADING_DOWN`, `HEADING_LEFT`,
/// `HEADING_UP`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Heading {
    Right,
    Down,
    Left,
    Up,
}

impl Heading {
    /// The unit vector upstream stores for the heading (`HEADING_RIGHT =
    /// [1, 0]`, `HEADING_DOWN = [0, 1]`, `HEADING_LEFT = [-1, 0]`,
    /// `HEADING_UP = [0, -1]`).
    pub fn vector(self) -> [f64; 2] {
        match self {
            Heading::Right => [1.0, 0.0],
            Heading::Down => [0.0, 1.0],
            Heading::Left => [-1.0, 0.0],
            Heading::Up => [0.0, -1.0],
        }
    }
}

/// `vectorToHeading(vec)` (`heading.ts:37-49`): right when `x > |y|`, left
/// when `x <= -|y|` (the zero vector included), down when `y > |x|`, and up
/// otherwise (the remaining diagonals and NaN).
pub fn vector_to_heading(vec: [f64; 2]) -> Heading {
    let [x, y] = vec;
    let abs_x = x.abs();
    let abs_y = y.abs();
    if x > abs_y {
        Heading::Right
    } else if x <= -abs_y {
        Heading::Left
    } else if y > abs_x {
        Heading::Down
    } else {
        Heading::Up
    }
}

/// `headingForPoint(p, o)` (`heading.ts:51-54`): the heading of the vector
/// from `o` to `p` (`vectorFromPoint(p, o) = p - o`).
pub fn heading_for_point(p: [f64; 2], o: [f64; 2]) -> Heading {
    vector_to_heading([p[0] - o[0], p[1] - o[1]])
}

/// `headingForPointIsHorizontal(p, o)` (`heading.ts:56-59`).
pub fn heading_for_point_is_horizontal(p: [f64; 2], o: [f64; 2]) -> bool {
    heading_is_horizontal(heading_for_point(p, o))
}

/// `headingIsHorizontal(a)` (`heading.ts:64-65`): right or left.
pub fn heading_is_horizontal(a: Heading) -> bool {
    matches!(a, Heading::Right | Heading::Left)
}

/// `headingIsVertical(a)` (`heading.ts:67`): up or down.
pub fn heading_is_vertical(a: Heading) -> bool {
    !heading_is_horizontal(a)
}
