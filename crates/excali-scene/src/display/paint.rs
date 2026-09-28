//! Colours and stroke styles, with the canvas's value rules.

use excali_core::json::number_to_string;

/// A CSS colour as upstream holds it: the stroke, background or constant
/// string, dark-filtered already when the scene draws dark.
///
/// The text is kept as given. Upstream assigns it to `fillStyle` and
/// `strokeStyle` verbatim, so the Canvas 2D backend does the same and the
/// browser parses it; the SVG writer prints it unchanged (upstream's export
/// writes `stroke="#1e1e1e"`); backends without a browser receive it
/// resolved by [`Color::rgba`] from [`crate::display::Painter`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Color(String);

impl Color {
    pub fn new(css: impl Into<String>) -> Self {
        Self(css.into())
    }

    /// The colour as given.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The colour as the canvas reads a `fillStyle` or `strokeStyle`
    /// assignment of it (the CSS Color 4 parser in `display/css_color.rs`,
    /// checked against Chrome), or `None` when the canvas ignores the
    /// assignment, such as for `""` (roughjs's `o.fill || ''`), `"none"` or
    /// `"blue-ish"`. An ignored assignment leaves the context's current
    /// style, which [`crate::display::DisplayList::replay`] substitutes.
    pub fn rgba(&self) -> Option<Rgba> {
        super::css_color::parse(&self.0)
    }
}

impl From<&str> for Color {
    fn from(css: &str) -> Self {
        Self::new(css)
    }
}

impl From<String> for Color {
    fn from(css: String) -> Self {
        Self(css)
    }
}

/// A resolved colour: 8-bit sRGB components, unpremultiplied, and an alpha
/// in `0..=1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f64,
}

impl Rgba {
    /// Opaque black, a fresh context's `fillStyle` and `strokeStyle`.
    pub const BLACK: Rgba = Rgba {
        r: 0,
        g: 0,
        b: 0,
        a: 1.0,
    };

    /// `rgba(r, g, b, a)`, with the alpha printed as JavaScript prints
    /// numbers: how the Canvas 2D backend sets a resolved colour, such as a
    /// base state's current style (`excali_canvas2d::paint_from`).
    pub fn css(&self) -> String {
        format!(
            "rgba({}, {}, {}, {})",
            self.r,
            self.g,
            self.b,
            number_to_string(self.a)
        )
    }
}

/// `lineCap`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LineCap {
    /// `"butt"`, the canvas default.
    #[default]
    Butt,
    /// `"round"`: upstream sets it before drawing every roughjs shape
    /// (`renderElement.ts:473-495`).
    Round,
    /// `"square"`.
    Square,
}

impl LineCap {
    pub fn as_css(self) -> &'static str {
        match self {
            LineCap::Butt => "butt",
            LineCap::Round => "round",
            LineCap::Square => "square",
        }
    }
}

/// `lineJoin`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LineJoin {
    /// `"miter"`, the canvas default.
    #[default]
    Miter,
    /// `"round"`, as upstream draws roughjs shapes.
    Round,
    /// `"bevel"`.
    Bevel,
}

impl LineJoin {
    pub fn as_css(self) -> &'static str {
        match self {
            LineJoin::Miter => "miter",
            LineJoin::Round => "round",
            LineJoin::Bevel => "bevel",
        }
    }
}

/// A dash pattern that draws dashed: `setLineDash(segments)` and
/// `lineDashOffset = offset` after the canvas's rules.
#[derive(Clone, Debug, PartialEq)]
pub struct Dash {
    segments: Vec<f64>,
    offset: f64,
}

impl Dash {
    /// The pattern `setLineDash(segments)` and `lineDashOffset = offset`
    /// leave on a fresh context, or `None` when the line stays solid:
    ///
    /// - a list with a negative, infinite or NaN entry is ignored;
    /// - an odd-length list is repeated to make it even;
    /// - an empty list, or one whose entries sum to 0, is solid;
    /// - a non-finite offset is ignored, leaving 0.
    pub fn new(segments: &[f64], offset: f64) -> Option<Dash> {
        if segments.iter().any(|s| !s.is_finite() || *s < 0.0) {
            return None;
        }
        if segments.iter().sum::<f64>() <= 0.0 {
            return None;
        }
        let mut list = segments.to_vec();
        if list.len() % 2 == 1 {
            list.extend_from_slice(segments);
        }
        Some(Dash {
            segments: list,
            offset: if offset.is_finite() { offset } else { 0.0 },
        })
    }

    /// The even-length list of dash and gap lengths, in user units.
    pub fn segments(&self) -> &[f64] {
        &self.segments
    }

    /// `lineDashOffset`: how far into the pattern the stroke starts.
    pub fn offset(&self) -> f64 {
        self.offset
    }
}

/// How a path is stroked: `strokeStyle`, `lineWidth`, `lineCap`,
/// `lineJoin`, `miterLimit` and the dash.
#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub color: Color,
    /// `lineWidth` as given; see [`Stroke::effective_width`].
    pub width: f64,
    pub cap: LineCap,
    pub join: LineJoin,
    /// `miterLimit` as given; see [`Stroke::effective_miter_limit`].
    pub miter_limit: f64,
    /// `None` for a solid line.
    pub dash: Option<Dash>,
}

impl Stroke {
    /// A solid stroke with the canvas defaults: butt caps, miter joins,
    /// miter limit 10.
    pub fn new(color: Color, width: f64) -> Self {
        Self {
            color,
            width,
            cap: LineCap::default(),
            join: LineJoin::default(),
            miter_limit: 10.0,
            dash: None,
        }
    }

    pub fn with_cap(mut self, cap: LineCap) -> Self {
        self.cap = cap;
        self
    }

    pub fn with_join(mut self, join: LineJoin) -> Self {
        self.join = join;
        self
    }

    pub fn with_dash(mut self, dash: Option<Dash>) -> Self {
        self.dash = dash;
        self
    }

    /// The width drawn: a `lineWidth` assignment of zero, a negative,
    /// infinite or NaN value is ignored, which leaves a fresh context's 1.
    pub fn effective_width(&self) -> f64 {
        if self.width.is_finite() && self.width > 0.0 {
            self.width
        } else {
            1.0
        }
    }

    /// The miter limit drawn, under the same rule with the default 10.
    pub fn effective_miter_limit(&self) -> f64 {
        if self.miter_limit.is_finite() && self.miter_limit > 0.0 {
            self.miter_limit
        } else {
            10.0
        }
    }
}
