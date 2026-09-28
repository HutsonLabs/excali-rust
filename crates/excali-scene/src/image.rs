//! Image elements as upstream draws them (`renderElement.ts`).
//!
//! - [`draw_image_element`]: the `"image"` case of `drawElementOnCanvas`
//!   (`packages/element/src/renderElement.ts:517-624`) in the element's own
//!   coordinates. A loaded file is one `drawImage` of the crop (or the whole
//!   image) onto `(0, 0, width, height)`, clipped to
//!   `roundRect(getCornerRadius(min(w, h)))` when the element is rounded,
//!   with `DARK_THEME_FILTER` when an SVG file is drawn in the dark theme.
//!   Anything else draws `drawImagePlaceholder` (`:361-385`): a `#E7E7E7`
//!   (light) or `#2E2E2E` (dark) box and upstream's placeholder image, or
//!   its error placeholder when the element's status is `"error"`
//!   ([`Placeholder`], built-in SVG images every backend has), sized by
//!   [`placeholder_icon_size`].
//! - [`render_image_element`]: `renderElement` exporting it (`:963-1009`,
//!   `:1111-1190`): the render opacity, then translate to the centre,
//!   rotate, `scale(element.scale)` (after rotating, which is what mirrors
//!   the image), translate back.
//!
//! Which files are drawable is the caller's image cache ([`ImageCache`]),
//! as `renderConfig.imageCache` is upstream's: `updateImageCache` loads
//! each file of an initialized image element (`element/src/image.ts:36-87`)
//! and a file that is still loading, or failed to load, has no image.

use std::collections::HashMap;

use excali_core::document::FileMimeType;
use excali_core::element::{Element, ElementKind, ImageFields, ImageStatus};
use excali_math::js;

use crate::bounds::{get_element_absolute_coords, ElementsMap};
use crate::display::{
    BuiltinImage, Clip, Color, DisplayItem, FillRule, Group, ImageFilter, ImageItem, Path, Rect,
    Transform,
};
use crate::shape::Theme;
use crate::utils::get_corner_radius;

/// `drawImagePlaceholder`'s box in the light theme (`renderElement.ts:366`).
pub const PLACEHOLDER_BACKGROUND_LIGHT: &str = "#E7E7E7";
/// `drawImagePlaceholder`'s box in the dark theme (`renderElement.ts:366`).
pub const PLACEHOLDER_BACKGROUND_DARK: &str = "#2E2E2E";

/// One of upstream's two placeholder images.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Placeholder {
    /// `IMAGE_PLACEHOLDER_IMG`: an image not (yet) loaded.
    Image,
    /// `IMAGE_ERROR_PLACEHOLDER_IMG`: an element whose status is `"error"`.
    Error,
}

impl Placeholder {
    /// The placeholder `drawImagePlaceholder` draws for an element with
    /// `status` (`renderElement.ts:377-379`).
    pub fn for_status(status: ImageStatus) -> Placeholder {
        match status {
            ImageStatus::Error => Placeholder::Error,
            ImageStatus::Pending | ImageStatus::Saved => Placeholder::Image,
        }
    }

    /// The built-in image the display list names it by.
    pub fn image(self) -> BuiltinImage {
        match self {
            Placeholder::Image => BuiltinImage::ImagePlaceholder,
            Placeholder::Error => BuiltinImage::ImageErrorPlaceholder,
        }
    }

    /// The SVG upstream loads the placeholder from.
    pub fn svg(self) -> &'static str {
        self.image().svg()
    }
}

/// `drawImagePlaceholder`'s icon size (`renderElement.ts:369-374`):
/// `min(min(w, h), min(min(w, h) * 0.4, 100))`, with `Math.min`'s NaN.
pub fn placeholder_icon_size(width: f64, height: f64) -> f64 {
    let side = js::min(width, height);
    js::min(side, js::min(side * 0.4, 100.0))
}

/// An entry of the image cache (`AppClassProperties["imageCache"]`): what
/// `updateImageCache` stores for a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageCacheEntry {
    /// The file's image has loaded; `mime_type` is the file's.
    Ready { mime_type: FileMimeType },
    /// The load is still in progress, or failed (the cache keeps the
    /// load's promise): the placeholder is drawn.
    Loading,
}

/// Upstream's `renderConfig.imageCache`: the entry for a file id, if any.
pub trait ImageCache {
    fn get(&self, file_id: &str) -> Option<&ImageCacheEntry>;
}

impl ImageCache for HashMap<String, ImageCacheEntry> {
    fn get(&self, file_id: &str) -> Option<&ImageCacheEntry> {
        HashMap::get(self, file_id)
    }
}

/// An empty cache: every image draws its placeholder.
impl ImageCache for () {
    fn get(&self, _: &str) -> Option<&ImageCacheEntry> {
        None
    }
}

fn image_fields(element: &Element) -> &ImageFields {
    match &element.kind {
        ElementKind::Image(fields) => fields,
        _ => panic!("not an image element: {}", element.base.id),
    }
}

/// The `"image"` case of `drawElementOnCanvas`
/// (`renderElement.ts:517-624`): what an image element draws in its own
/// coordinates, `(0, 0)` to `(width, height)`.
///
/// Panics when `element` is not an image element.
pub fn draw_image_element(
    element: &Element,
    theme: Theme,
    cache: &impl ImageCache,
) -> Vec<DisplayItem> {
    let fields = image_fields(element);
    let b = &element.base;
    // isInitializedImageElement: a file id; the cache entry's image, once
    // it has loaded.
    let ready = fields
        .file_id
        .as_ref()
        .and_then(|id| match cache.get(&id.0)? {
            ImageCacheEntry::Ready { mime_type } => Some((id, *mime_type)),
            ImageCacheEntry::Loading => None,
        });
    let Some((file_id, mime_type)) = ready else {
        return draw_image_placeholder(b.width, b.height, fields.status, theme);
    };
    let mut image = ImageItem::new(file_id.0.clone(), Rect::new(0.0, 0.0, b.width, b.height));
    image.source = fields
        .crop
        .map(|crop| Rect::new(crop.x, crop.y, crop.width, crop.height));
    if theme == Theme::Dark && mime_type == FileMimeType::Svg {
        image.filter = Some(ImageFilter::DarkTheme);
    }
    let image = DisplayItem::Image(image);
    if b.roundness.is_none() {
        return vec![image];
    }
    let radius = get_corner_radius(js::min(b.width, b.height), element);
    vec![DisplayItem::Group(Group {
        transform: Transform::IDENTITY,
        opacity: 1.0,
        clip: Some(Clip {
            path: Path::round_rect(0.0, 0.0, b.width, b.height, radius),
            rule: FillRule::NonZero,
        }),
        items: vec![image],
    })]
}

/// `drawImagePlaceholder` (`renderElement.ts:361-385`): `fillRect` of the
/// box, then `drawImage` of the placeholder centred, `size` square.
fn draw_image_placeholder(
    width: f64,
    height: f64,
    status: ImageStatus,
    theme: Theme,
) -> Vec<DisplayItem> {
    let background = match theme {
        Theme::Dark => PLACEHOLDER_BACKGROUND_DARK,
        Theme::Light => PLACEHOLDER_BACKGROUND_LIGHT,
    };
    let size = placeholder_icon_size(width, height);
    let dest = Rect::new(
        width / 2.0 - size / 2.0,
        height / 2.0 - size / 2.0,
        size,
        size,
    );
    vec![
        DisplayItem::FillRect {
            rect: Rect::new(0.0, 0.0, width, height),
            color: Color::new(background),
        },
        DisplayItem::Image(ImageItem::new(
            Placeholder::for_status(status).image().id(),
            dest,
        )),
    ]
}

/// `renderElement` exporting an image element (`renderElement.ts:963-1009`
/// and the image branch of `:1111-1190`): `globalAlpha = opacity`, then
/// `translate(cx + scrollX, cy + scrollY)`, `rotate(angle)`,
/// `scale(scale[0], scale[1])` and `translate(-w / 2, -h / 2)` around
/// [`draw_image_element`].
///
/// `opacity` is `resolveElementRenderState`'s: the element's opacity times
/// its frame's, as a fraction. Each canvas call is its own group, so a
/// non-finite angle or scale drops only that call, as the canvas does.
///
/// Panics when `element` is not an image element.
pub fn render_image_element(
    element: &Element,
    scroll: (f64, f64),
    opacity: f64,
    theme: Theme,
    cache: &impl ImageCache,
) -> DisplayItem {
    let fields = image_fields(element);
    let b = &element.base;
    let [x1, y1, x2, y2, ..] = get_element_absolute_coords(element, &ElementsMap::default(), false);
    let cx = (x1 + x2) / 2.0 + scroll.0;
    let cy = (y1 + y2) / 2.0 + scroll.1;
    let shift_x = (x2 - x1) / 2.0 - (b.x - x1);
    let shift_y = (y2 - y1) / 2.0 - (b.y - y1);
    let calls = [
        Transform::translate(cx, cy),
        Transform::rotate(b.angle.0),
        Transform::scale(fields.scale[0], fields.scale[1]),
        Transform::translate(-shift_x, -shift_y),
    ];
    let mut items = draw_image_element(element, theme, cache);
    for transform in calls.into_iter().rev() {
        items = vec![DisplayItem::Group(Group {
            transform,
            opacity: 1.0,
            clip: None,
            items,
        })];
    }
    DisplayItem::Group(Group {
        transform: Transform::IDENTITY,
        opacity,
        clip: None,
        items,
    })
}
