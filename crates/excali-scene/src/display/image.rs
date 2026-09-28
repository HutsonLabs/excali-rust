//! Bitmap draws: one `drawImage` call each.

use excali_core::color::dark_mode_filter_rgb;

/// An axis-aligned rectangle in user units (or image pixels for a source
/// rectangle).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// The same area with a non-negative width and height, as `drawImage`
    /// reads its source and destination rectangles.
    pub fn normalized(&self) -> Rect {
        let (x, width) = if self.width < 0.0 {
            (self.x + self.width, -self.width)
        } else {
            (self.x, self.width)
        };
        let (y, height) = if self.height < 0.0 {
            (self.y + self.height, -self.height)
        } else {
            (self.y, self.height)
        };
        Rect::new(x, y, width, height)
    }

    /// Whether the rectangle covers no area (`drawImage` draws nothing for
    /// a zero-sized source or destination).
    pub fn is_empty(&self) -> bool {
        self.width == 0.0 || self.height == 0.0
    }
}

/// A CSS filter applied to an image as it is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageFilter {
    /// `DARK_THEME_FILTER` (`common/src/constants.ts:204`), which upstream
    /// applies to SVG images in dark mode (`renderElement.ts:549-609`).
    DarkTheme,
}

impl ImageFilter {
    /// The `filter` string the Canvas 2D backend assigns.
    pub fn css(self) -> &'static str {
        match self {
            ImageFilter::DarkTheme => "invert(93%) hue-rotate(180deg)",
        }
    }

    /// The filter on one unpremultiplied pixel, for backends without CSS
    /// filters: upstream's numeric version of the same filter
    /// (`applyDarkModeFilter`, `colors.ts:86-116`). Alpha is unchanged.
    pub fn apply_rgb(self, r: u8, g: u8, b: u8) -> (u8, u8, u8) {
        match self {
            ImageFilter::DarkTheme => {
                let (r, g, b) = dark_mode_filter_rgb(f64::from(r), f64::from(g), f64::from(b));
                // Whole components in 0..=255 (both steps round and clamp).
                (r as u8, g as u8, b as u8)
            }
        }
    }
}

/// `drawImage(image, sx, sy, sw, sh, dx, dy, dw, dh)`
/// (`renderElement.ts:517-624`).
///
/// The bitmap is named by `id` (upstream's `fileId`); each backend resolves
/// ids through its own image store and draws nothing for an id it does not
/// have. The scene draws upstream's placeholder for an image that is not
/// ready.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageItem {
    pub id: String,
    /// The source rectangle in image pixels (`element.crop`); `None` for
    /// the whole image at its natural size.
    pub source: Option<Rect>,
    /// Where the source lands, in user units.
    pub dest: Rect,
    /// `imageSmoothingEnabled`: interpolate when scaling. Upstream turns it
    /// off for pixel-snapped blits (`renderElement.ts:1205-1264`).
    pub smoothing: bool,
    pub filter: Option<ImageFilter>,
}

impl ImageItem {
    /// The whole image into `dest`, smoothed, unfiltered.
    pub fn new(id: impl Into<String>, dest: Rect) -> Self {
        Self {
            id: id.into(),
            source: None,
            dest,
            smoothing: true,
            filter: None,
        }
    }
}
