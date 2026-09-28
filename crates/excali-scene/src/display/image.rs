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

/// An image every backend has without being given it: upstream's
/// placeholders, SVG documents it loads from `data:image/svg+xml` URLs when
/// `renderElement.ts` is imported (`IMAGE_PLACEHOLDER_IMG` and
/// `IMAGE_ERROR_PLACEHOLDER_IMG`, `:342-359`) and draws with `drawImage`
/// for an image element whose file is not ready (`drawImagePlaceholder`,
/// `:361-385`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuiltinImage {
    /// `IMAGE_PLACEHOLDER_IMG`: Font Awesome's `image` icon.
    ImagePlaceholder,
    /// `IMAGE_ERROR_PLACEHOLDER_IMG`: the icon with a "not allowed" sign,
    /// for an element whose status is `"error"`.
    ImageErrorPlaceholder,
}

impl BuiltinImage {
    pub const ALL: [BuiltinImage; 2] = [
        BuiltinImage::ImagePlaceholder,
        BuiltinImage::ImageErrorPlaceholder,
    ];

    /// The id an [`ImageItem`] names the image by. File ids are SHA-1 hex
    /// digests or nanoids, which never contain a colon.
    pub fn id(self) -> &'static str {
        match self {
            BuiltinImage::ImagePlaceholder => "builtin:image-placeholder",
            BuiltinImage::ImageErrorPlaceholder => "builtin:image-error-placeholder",
        }
    }

    /// The built-in image named `id`, if it is one.
    pub fn from_id(id: &str) -> Option<BuiltinImage> {
        BuiltinImage::ALL.into_iter().find(|b| b.id() == id)
    }

    /// The SVG document, as upstream writes it (the body of its data URL
    /// before `encodeURIComponent`).
    pub fn svg(self) -> &'static str {
        match self {
            BuiltinImage::ImagePlaceholder => IMAGE_PLACEHOLDER_SVG,
            BuiltinImage::ImageErrorPlaceholder => IMAGE_ERROR_PLACEHOLDER_SVG,
        }
    }

    /// Upstream's `src`: `data:image/svg+xml,` and the document through
    /// `encodeURIComponent`.
    pub fn data_url(self) -> String {
        let mut url = String::from("data:image/svg+xml,");
        for b in self.svg().bytes() {
            // encodeURIComponent keeps A-Z a-z 0-9 - _ . ! ~ * ' ( ).
            if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
                url.push(char::from(b));
            } else {
                url.push_str(&format!("%{b:02X}"));
            }
        }
        url
    }

    /// The side of the document's square `viewBox` (`0 0 side side`); the
    /// documents have no `width` or `height`.
    pub fn view_box(self) -> f64 {
        match self {
            BuiltinImage::ImagePlaceholder => 512.0,
            BuiltinImage::ImageErrorPlaceholder => 668.0,
        }
    }
}

/// `IMAGE_PLACEHOLDER_IMG`'s SVG (`renderElement.ts:342-349`).
const IMAGE_PLACEHOLDER_SVG: &str = r##"<svg aria-hidden="true" focusable="false" data-prefix="fas" data-icon="image" class="svg-inline--fa fa-image fa-w-16" role="img" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512"><path fill="#888" d="M464 448H48c-26.51 0-48-21.49-48-48V112c0-26.51 21.49-48 48-48h416c26.51 0 48 21.49 48 48v288c0 26.51-21.49 48-48 48zM112 120c-30.928 0-56 25.072-56 56s25.072 56 56 56 56-25.072 56-56-25.072-56-56-56zM64 384h384V272l-87.515-87.515c-4.686-4.686-12.284-4.686-16.971 0L208 320l-55.515-55.515c-4.686-4.686-12.284-4.686-16.971 0L64 336v48z"></path></svg>"##;

/// `IMAGE_ERROR_PLACEHOLDER_IMG`'s SVG (`renderElement.ts:351-359`).
const IMAGE_ERROR_PLACEHOLDER_SVG: &str = r##"<svg viewBox="0 0 668 668" xmlns="http://www.w3.org/2000/svg" xml:space="preserve" style="fill-rule:evenodd;clip-rule:evenodd;stroke-linejoin:round;stroke-miterlimit:2"><path d="M464 448H48c-26.51 0-48-21.49-48-48V112c0-26.51 21.49-48 48-48h416c26.51 0 48 21.49 48 48v288c0 26.51-21.49 48-48 48ZM112 120c-30.928 0-56 25.072-56 56s25.072 56 56 56 56-25.072 56-56-25.072-56-56-56ZM64 384h384V272l-87.515-87.515c-4.686-4.686-12.284-4.686-16.971 0L208 320l-55.515-55.515c-4.686-4.686-12.284-4.686-16.971 0L64 336v48Z" style="fill:#888;fill-rule:nonzero" transform="matrix(.81709 0 0 .81709 124.825 145.825)"/><path d="M256 8C119.034 8 8 119.033 8 256c0 136.967 111.034 248 248 248s248-111.034 248-248S392.967 8 256 8Zm130.108 117.892c65.448 65.448 70 165.481 20.677 235.637L150.47 105.216c70.204-49.356 170.226-44.735 235.638 20.676ZM125.892 386.108c-65.448-65.448-70-165.481-20.677-235.637L361.53 406.784c-70.203 49.356-170.226 44.736-235.638-20.676Z" style="fill:#888;fill-rule:nonzero" transform="matrix(.30366 0 0 .30366 506.822 60.065)"/></svg>"##;

/// `drawImage(image, sx, sy, sw, sh, dx, dy, dw, dh)`
/// (`renderElement.ts:517-624`).
///
/// The image is named by `id`: upstream's `fileId`, which each backend
/// resolves through its own image store (drawing nothing for an id it does
/// not have), or a [`BuiltinImage`], which every backend has. The scene
/// draws upstream's placeholder, a built-in SVG, for an image that is not
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
