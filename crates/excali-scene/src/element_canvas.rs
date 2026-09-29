//! The per-element bitmap cache: how upstream draws an element in the
//! editor (`packages/element/src/renderElement.ts`). Outside export every
//! element but a frame is rasterised once into a canvas of its own and
//! blitted from it on every frame after.
//!
//! - [`get_canvas_padding`] (`getCanvasPadding`, `:102-116`): the margin
//!   around the element in the bitmap: freedraw `strokeWidth × 12`, text
//!   `fontSize / 2`, an arrow 40 with an end arrowhead (upstream tests
//!   `endArrowhead || endArrowhead`, so a start arrowhead alone is 20),
//!   everything else 20.
//! - [`capped_element_canvas_size`] (`cappedElementCanvasSize`,
//!   `:216-269`): `(size × dpr + 2 × padding) × zoom`, the scale lowered so
//!   neither side exceeds [`WIDTH_HEIGHT_LIMIT`] and then so the area does
//!   not exceed [`AREA_LIMIT`], floored. Lines, arrows and freedraw use the
//!   width and height of their bounds.
//! - [`generate_element_canvas`] (`generateElementCanvas`, `:271-339`): the
//!   bitmap's draws: `drawElementOnCanvas` under `translate(padding ×
//!   scale)` and `scale(dpr × scale)`, lines, arrows and freedraw first
//!   translated by how far their origin lies right of and below their
//!   bounds. No bitmap when a side floors to 0.
//! - [`ElementCanvasCache`] (`elementWithCanvasCache` and
//!   `generateElementWithCanvas`, `:682-728`): one bitmap per element,
//!   made again when the zoom changes (unless a zoom gesture is running,
//!   `shouldCacheIgnoreZoom`), or the theme, the image crop or the opacity
//!   of the containing frame (`opacity || 100`) does. Upstream keys the
//!   cache by the element object, which every update replaces
//!   (`newElementWith`) or clears (`mutateElement` → `ShapeCache.delete`);
//!   the port keys it by id and holds the element's `version` and
//!   `versionNonce`, which every update bumps. The device pixel ratio is
//!   not part of the key, as upstream's is not.
//! - [`draw_element_from_canvas`] (`drawElementFromCanvas`, `:762-934`):
//!   the blit. The matrix is the context's scaled by `1 / dpr`, rotated
//!   about the element's centre (images also flipped by `element.scale`
//!   once loaded), and the bitmap lands at the element's top left minus the
//!   padding. When [`can_snap_element`] (`:747-752`: no zoom gesture and an
//!   angle of 0 or a right angle) the matrix's origin is moved onto the
//!   rounded device position of the blit and the bitmap drawn at (0, 0),
//!   with [`SNAP_TIE_BIAS`] breaking ties; a label bound to an unrotated
//!   container rounds its container's origin and its own offset from it
//!   separately. An arrow with a label is clipped even-odd to leave a hole
//!   of the label's size plus `BOUND_TEXT_PADDING`.
//! - [`render_element_cached`] (`renderElement`, `:963-1009`, and
//!   `drawElement`'s cached branches, `:1066-1098` and `:1191-1264`): the
//!   element's alpha, smoothing turned off for a snapped blit (not for
//!   freedraw), the bitmap from the cache, the blit. Frames, and every
//!   element when exporting, are drawn as vectors ([`render_element`]).
//!
//! The render offset of a host override is applied in the blit, before
//! snapping, as upstream applies it.
//! - The crop editor's preview (`:1220-1251`): the image being cropped
//!   (`croppingElementId`), when it has a crop, is drawn over its whole
//!   image ([`crate::crop::get_uncropped_image_element`]), rasterised for
//!   that draw only and blitted at `globalAlpha` 0.1 (not times the
//!   element's opacity) with no render offset
//!   ([`ElementDraw::CropPreview`]).
//!
//! Fixture: `tests/fixtures/element-canvas.json`, from upstream's
//! `renderElement` on its editor path (`tools/goldens/element-canvas.mjs`).

use std::collections::HashMap;

use excali_core::constants::BOUND_TEXT_PADDING;
use excali_core::element::{Element, ElementKind, ImageCrop};
use excali_math::{is_right_angle_rads, js, Radians};

use crate::bounds::{
    get_bound_text_element, get_container_element, get_element_absolute_coords, ElementsMap,
};
use crate::crop::get_uncropped_image_element;
use crate::display::{
    bitmap_id, Blit, Clip, DisplayItem, DisplayList, FillRule, Path, Rect, Transform,
};
use crate::render_element::{
    distance, draw_element_on_canvas, element_alpha, get_containing_frame, render_element,
    resolve_element_render_state, rotate, with_transform, ElementRenderState, RenderError,
};
use crate::shape::Theme;
use crate::static_scene::{StaticCanvasAppState, StaticCanvasRenderConfig};

/// `AREA_LIMIT` (`renderElement.ts:231`): about Safari mobile's canvas area
/// limit, in device pixels.
pub const AREA_LIMIT: f64 = 16_777_216.0;
/// `WIDTH_HEIGHT_LIMIT` (`renderElement.ts:233`): about Safari's canvas
/// side limit.
pub const WIDTH_HEIGHT_LIMIT: f64 = 32_767.0;
/// `SNAP_TIE_BIAS` (`renderElement.ts:760`): breaks a `Math.round` tie at
/// exactly half a device pixel the same way every frame.
pub const SNAP_TIE_BIAS: f64 = 1e-6;

/// `getCanvasPadding(element)` (`renderElement.ts:102-116`).
pub fn get_canvas_padding(element: &Element) -> f64 {
    match &element.kind {
        ElementKind::Freedraw(_) => element.base.stroke_width * 12.0,
        ElementKind::Text(text) => text.font_size / 2.0,
        ElementKind::Arrow(arrow) if arrow.linear.end_arrowhead.is_some() => 40.0,
        _ => 20.0,
    }
}

/// Lines, arrows and freedraw: sized and offset by their bounds.
fn is_linear_or_freedraw(element: &Element) -> bool {
    matches!(
        element.kind,
        ElementKind::Line(_) | ElementKind::Arrow(_) | ElementKind::Freedraw(_)
    )
}

/// A bitmap's backing size in device pixels and the scale it is drawn at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ElementCanvasSize {
    pub width: f64,
    pub height: f64,
    pub scale: f64,
}

/// `cappedElementCanvasSize(element, elementsMap, zoom)`
/// (`renderElement.ts:216-269`) at `device_pixel_ratio`.
pub fn capped_element_canvas_size(
    element: &Element,
    elements_map: &ElementsMap<'_>,
    zoom: f64,
    device_pixel_ratio: f64,
) -> ElementCanvasSize {
    let padding = get_canvas_padding(element);
    let [x1, y1, x2, y2, _, _] = get_element_absolute_coords(element, elements_map, false);
    let (element_width, element_height) = if is_linear_or_freedraw(element) {
        (distance(x1, x2), distance(y1, y2))
    } else {
        (element.base.width, element.base.height)
    };
    let width = element_width * device_pixel_ratio + padding * 2.0;
    let height = element_height * device_pixel_ratio + padding * 2.0;
    let mut scale = zoom;
    // rescale to ensure width and height is within limits
    if width * scale > WIDTH_HEIGHT_LIMIT || height * scale > WIDTH_HEIGHT_LIMIT {
        scale = js::min(WIDTH_HEIGHT_LIMIT / width, WIDTH_HEIGHT_LIMIT / height);
    }
    // rescale to ensure canvas area is within limits
    if width * height * scale * scale > AREA_LIMIT {
        scale = (AREA_LIMIT / (width * height)).sqrt();
    }
    ElementCanvasSize {
        width: (width * scale).floor(),
        height: (height * scale).floor(),
        scale,
    }
}

/// What a cached bitmap was made for (`ExcalidrawElementWithCanvas`,
/// `renderElement.ts:204-214`), and the element update it shows.
#[derive(Clone, Debug, PartialEq)]
pub struct ElementCanvasKey {
    /// The element's `version` and `versionNonce`: upstream's key is the
    /// element object, which an update replaces or clears.
    pub version: f64,
    pub version_nonce: f64,
    /// `appState.theme`.
    pub theme: Theme,
    /// `zoomValue`.
    pub zoom_value: f64,
    /// `imageCrop`: an image's crop, `None` for other elements.
    pub image_crop: Option<ImageCrop>,
    /// `getContainingFrame(element)?.opacity || 100`.
    pub containing_frame_opacity: f64,
}

impl ElementCanvasKey {
    /// The key a bitmap of `element` made now would have.
    pub fn of(
        element: &Element,
        elements_map: &ElementsMap<'_>,
        app_state: &StaticCanvasAppState,
    ) -> ElementCanvasKey {
        let frame_opacity = get_containing_frame(element, elements_map)
            .map(|frame| frame.base.opacity)
            .unwrap_or(100.0);
        ElementCanvasKey {
            version: element.base.version,
            version_nonce: element.base.version_nonce,
            theme: app_state.theme,
            zoom_value: app_state.zoom,
            image_crop: match &element.kind {
                ElementKind::Image(image) => image.crop,
                _ => None,
            },
            // `|| 100`: 0 and NaN are falsy
            containing_frame_opacity: if frame_opacity == 0.0 || frame_opacity.is_nan() {
                100.0
            } else {
                frame_opacity
            },
        }
    }

    /// Whether a bitmap made for `self` must be made again for `next`
    /// (`generateElementWithCanvas`, `renderElement.ts:693-711`): a zoom
    /// change counts unless a zoom gesture is running.
    pub fn is_stale(&self, next: &ElementCanvasKey, should_cache_ignore_zoom: bool) -> bool {
        self.version != next.version
            || self.version_nonce != next.version_nonce
            || (self.zoom_value != next.zoom_value && !should_cache_ignore_zoom)
            || self.theme != next.theme
            || self.image_crop != next.image_crop
            || self.containing_frame_opacity != next.containing_frame_opacity
    }
}

/// A bitmap to make (`generateElementCanvas`): its backing size, the scale
/// it is drawn at, and its draws from the canvas's identity matrix.
#[derive(Clone, Debug, PartialEq)]
pub struct ElementCanvas {
    pub key: ElementCanvasKey,
    /// `canvas.width` and `canvas.height`, in device pixels.
    pub width: f64,
    pub height: f64,
    pub scale: f64,
    /// `canvasOffsetX` and `canvasOffsetY`: where a line's, arrow's or
    /// freedraw's origin lies in its bitmap. Upstream starts X at -100 for
    /// other elements and never reads it.
    pub canvas_offset_x: f64,
    pub canvas_offset_y: f64,
    pub content: DisplayList,
}

/// `generateElementCanvas(element, elementsMap, zoom, renderConfig,
/// appState)` (`renderElement.ts:271-339`) at `device_pixel_ratio`: `None`
/// when a side of the bitmap would be 0.
pub fn generate_element_canvas(
    element: &Element,
    elements_map: &ElementsMap<'_>,
    config: &StaticCanvasRenderConfig,
    app_state: &StaticCanvasAppState,
    device_pixel_ratio: f64,
) -> Result<Option<ElementCanvas>, RenderError> {
    let padding = get_canvas_padding(element);
    let ElementCanvasSize {
        width,
        height,
        scale,
    } = capped_element_canvas_size(element, elements_map, app_state.zoom, device_pixel_ratio);
    // `!width || !height`: 0 and NaN
    if width == 0.0 || height == 0.0 || width.is_nan() || height.is_nan() {
        return Ok(None);
    }
    let mut canvas_offset_x = -100.0;
    let mut canvas_offset_y = 0.0;
    let offset = if is_linear_or_freedraw(element) {
        let [x1, y1, ..] = get_element_absolute_coords(element, elements_map, false);
        let b = &element.base;
        canvas_offset_x = if b.x > x1 {
            distance(b.x, x1) * device_pixel_ratio * scale
        } else {
            0.0
        };
        canvas_offset_y = if b.y > y1 {
            distance(b.y, y1) * device_pixel_ratio * scale
        } else {
            0.0
        };
        Some(Transform::translate(canvas_offset_x, canvas_offset_y))
    } else {
        None
    };
    let drawing = draw_element_on_canvas(element, config)?;
    let k = device_pixel_ratio * scale;
    let placed = with_transform(
        Transform::translate(padding * scale, padding * scale),
        vec![with_transform(Transform::scale(k, k), drawing)],
    );
    let item = match offset {
        Some(t) => with_transform(t, vec![placed]),
        None => placed,
    };
    Ok(Some(ElementCanvas {
        key: ElementCanvasKey::of(element, elements_map, app_state),
        width,
        height,
        scale,
        canvas_offset_x,
        canvas_offset_y,
        content: DisplayList { items: vec![item] },
    }))
}

/// A cached bitmap: what it was made for, its size and scale, and the
/// backend's surface holding it.
#[derive(Clone, Debug, PartialEq)]
pub struct ElementBitmap<S> {
    pub key: ElementCanvasKey,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
    pub surface: S,
}

/// `elementWithCanvasCache`: one bitmap per element id. `S` is the
/// backend's surface (a canvas in the browser).
#[derive(Clone, Debug)]
pub struct ElementCanvasCache<S> {
    entries: HashMap<String, ElementBitmap<S>>,
}

impl<S> Default for ElementCanvasCache<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> ElementCanvasCache<S> {
    pub fn new() -> Self {
        ElementCanvasCache {
            entries: HashMap::new(),
        }
    }

    /// The bitmap cached for the element `id`.
    pub fn get(&self, id: &str) -> Option<&ElementBitmap<S>> {
        self.entries.get(id)
    }

    /// Forget the element's bitmap (`ShapeCache.delete`), returning it.
    pub fn delete(&mut self, id: &str) -> Option<ElementBitmap<S>> {
        self.entries.remove(id)
    }

    /// Keep only the bitmaps of the ids `keep` accepts: what the WeakMap
    /// lets go once an element object is unreachable.
    pub fn retain(&mut self, mut keep: impl FnMut(&str) -> bool) {
        self.entries.retain(|id, _| keep(id));
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `generateElementWithCanvas(element, elementsMap, renderConfig,
    /// appState)` (`renderElement.ts:687-728`): the cached bitmap, made
    /// again (through `make_surface`) when [`ElementCanvasKey::is_stale`].
    /// `None` when a new bitmap would have a side of 0; the old one stays
    /// cached, as upstream leaves it.
    pub fn generate_element_with_canvas(
        &mut self,
        element: &Element,
        elements_map: &ElementsMap<'_>,
        config: &StaticCanvasRenderConfig,
        app_state: &StaticCanvasAppState,
        device_pixel_ratio: f64,
        make_surface: impl FnOnce(ElementCanvas) -> S,
    ) -> Result<Option<&ElementBitmap<S>>, RenderError> {
        let id = element.base.id.as_str();
        let stale = match self.entries.get(id) {
            None => true,
            Some(prev) => prev.key.is_stale(
                &ElementCanvasKey::of(element, elements_map, app_state),
                app_state.should_cache_ignore_zoom,
            ),
        };
        if stale {
            let Some(canvas) = generate_element_canvas(
                element,
                elements_map,
                config,
                app_state,
                device_pixel_ratio,
            )?
            else {
                return Ok(None);
            };
            let (key, width, height, scale) = (
                canvas.key.clone(),
                canvas.width,
                canvas.height,
                canvas.scale,
            );
            let surface = make_surface(canvas);
            let bitmap = ElementBitmap {
                key,
                width,
                height,
                scale,
                surface,
            };
            self.entries.insert(id.to_owned(), bitmap);
        }
        Ok(self.entries.get(id))
    }
}

/// `canSnapElement(element, appState)` (`renderElement.ts:747-752`): no
/// zoom gesture, and an angle of 0 (or NaN, falsy) or a right angle.
pub fn can_snap_element(angle: Radians, should_cache_ignore_zoom: bool) -> bool {
    !should_cache_ignore_zoom && (angle.0 == 0.0 || angle.0.is_nan() || is_right_angle_rads(angle))
}

/// `ctx.transform(t)` on the matrix `m`: ignored when an entry is not
/// finite, as the canvas ignores such a call.
fn then(m: Transform, t: Transform) -> Transform {
    if [t.a, t.b, t.c, t.d, t.e, t.f].iter().all(|v| v.is_finite()) {
        m.concat(&t)
    } else {
        m
    }
}

/// `isPendingImageElement` (`renderElement.ts:95-100`): an image with a
/// file the image cache does not hold yet.
fn is_pending_image(element: &Element, config: &StaticCanvasRenderConfig) -> bool {
    match &element.kind {
        // `isInitializedImageElement`: `!!element.fileId`
        ElementKind::Image(image) => image
            .file_id
            .as_ref()
            .is_some_and(|id| !id.0.is_empty() && !config.image_cache.contains_key(&id.0)),
        _ => false,
    }
}

/// Where and how a bitmap is drawn: the clip (the arrow label hole) under
/// its matrix, the blit matrix and the destination.
#[derive(Clone, Debug, PartialEq)]
pub struct ElementBlitPlacement {
    pub clip: Option<(Clip, Transform)>,
    pub transform: Transform,
    pub dest: Rect,
}

/// `drawElementFromCanvas(elementWithCanvas, context, renderConfig,
/// appState, allElementsMap, positionOffset)` (`renderElement.ts:762-934`)
/// for a bitmap of `size`, on a context whose matrix is `base`.
#[allow(clippy::too_many_arguments)]
pub fn draw_element_from_canvas(
    size: ElementCanvasSize,
    element: &Element,
    config: &StaticCanvasRenderConfig,
    app_state: &StaticCanvasAppState,
    all_elements_map: &ElementsMap<'_>,
    device_pixel_ratio: f64,
    offset: [f64; 2],
    base: Transform,
) -> ElementBlitPlacement {
    let dpr = device_pixel_ratio;
    let [ox, oy] = offset;
    let (sx, sy) = (app_state.scroll_x, app_state.scroll_y);
    let padding = get_canvas_padding(element);
    let [x1, y1, x2, y2, _, _] = get_element_absolute_coords(element, all_elements_map, false);
    let cx = ((x1 + x2) / 2.0 + ox + sx) * dpr;
    let cy = ((y1 + y2) / 2.0 + oy + sy) * dpr;

    let mut m = then(base, Transform::scale(1.0 / dpr, 1.0 / dpr));

    let clip = match (
        &element.kind,
        get_bound_text_element(element, all_elements_map),
    ) {
        (ElementKind::Arrow(_), Some(label)) => {
            let [.., label_cx, label_cy] =
                get_element_absolute_coords(label, all_elements_map, false);
            let (w, h) = (label.base.width, label.base.height);
            // generously covers the arrow's blit at any rotation
            let outer_half = js::max(distance(x1, x2), distance(y1, y2)) * dpr + padding * 10.0;
            let mut path = Path::rect(
                cx - outer_half,
                cy - outer_half,
                outer_half * 2.0,
                outer_half * 2.0,
            );
            path.commands.extend(
                Path::rect(
                    (label_cx - w / 2.0 - BOUND_TEXT_PADDING + ox + sx) * dpr,
                    (label_cy - h / 2.0 - BOUND_TEXT_PADDING + oy + sy) * dpr,
                    (w + BOUND_TEXT_PADDING * 2.0) * dpr,
                    (h + BOUND_TEXT_PADDING * 2.0) * dpr,
                )
                .commands,
            );
            Some((
                Clip {
                    path,
                    rule: FillRule::EvenOdd,
                },
                m,
            ))
        }
        _ => None,
    };

    // rotation and scale originate from the element centre
    m = then(m, Transform::translate(cx, cy));
    m = then(m, rotate(element.base.angle.0));
    if let ElementKind::Image(image) = &element.kind {
        if !is_pending_image(element, config) {
            m = then(m, Transform::scale(image.scale[0], image.scale[1]));
        }
    }
    m = then(m, Transform::translate(-cx, -cy));

    let mut draw_x = (x1 + ox + sx) * dpr - padding;
    let mut draw_y = (y1 + oy + sy) * dpr - padding;

    if can_snap_element(
        Radians(element.base.angle.0),
        app_state.should_cache_ignore_zoom,
    ) {
        let Transform { a, b, c, d, e, f } = m;
        let container = match element.kind {
            ElementKind::Text(_) => get_container_element(element, all_elements_map),
            _ => None,
        };
        // A bound label shares its unrotated container's offset and
        // anchor (`!container.angle`: 0 and NaN). Other bitmaps anchor to
        // themselves.
        let anchor = container.filter(|c| c.base.angle.0 == 0.0 || c.base.angle.0.is_nan());
        let (anchor_scene_x, anchor_scene_y, anchor_padding) = match anchor {
            Some(anchor) => {
                let [ax, ay, ..] = get_element_absolute_coords(anchor, all_elements_map, false);
                (ax, ay, get_canvas_padding(anchor))
            }
            None => (x1, y1, padding),
        };
        let anchor_x = (anchor_scene_x + ox + sx) * dpr - anchor_padding;
        let anchor_y = (anchor_scene_y + oy + sy) * dpr - anchor_padding;
        // the relative vector, formed before the scroll translation
        let dx = (x1 - anchor_scene_x) * dpr + anchor_padding - padding;
        let dy = (y1 - anchor_scene_y) * dpr + anchor_padding - padding;
        let snapped = Transform::new(
            a,
            b,
            c,
            d,
            js::round(a * anchor_x + c * anchor_y + e) + js::round(a * dx + c * dy + SNAP_TIE_BIAS),
            js::round(b * anchor_x + d * anchor_y + f) + js::round(b * dx + d * dy + SNAP_TIE_BIAS),
        );
        // setTransform ignores a call with a non-finite entry
        if [snapped.e, snapped.f].iter().all(|v| v.is_finite()) {
            m = snapped;
        }
        draw_x = 0.0;
        draw_y = 0.0;
    }

    ElementBlitPlacement {
        clip,
        transform: m,
        dest: Rect::new(
            draw_x,
            draw_y,
            size.width / size.scale,
            size.height / size.scale,
        ),
    }
}

/// How [`render_element_cached`] draws an element.
#[derive(Clone, Debug, PartialEq)]
pub enum ElementDraw {
    /// As vectors ([`render_element`]): frames, and everything when
    /// exporting.
    Vector(DisplayItem),
    /// From its cached bitmap.
    Blit(Blit),
    /// The image being cropped: its uncropped preview, then its cached
    /// bitmap.
    CropPreview(Box<CropPreview>),
}

/// `globalAlpha` of the crop editor's uncropped preview
/// (`renderElement.ts:1226`).
pub const CROP_PREVIEW_ALPHA: f64 = 0.1;

/// The crop editor's preview of the image being cropped
/// (`renderElement.ts:1220-1251`).
#[derive(Clone, Debug, PartialEq)]
pub struct CropPreview {
    /// The uncropped image's bitmap, made for this draw only (upstream
    /// does not cache it); a backend draws it under
    /// [`CropPreview::preview`]'s id.
    pub uncropped: ElementCanvas,
    /// Its blit: [`CROP_PREVIEW_ALPHA`], no render offset.
    pub preview: Blit,
    /// The element's own blit, drawn after.
    pub blit: Blit,
}

/// The image `element` is the one being cropped and has a crop.
pub(crate) fn is_cropping(element: &Element, app_state: &StaticCanvasAppState) -> bool {
    app_state.cropping_element_id.as_deref() == Some(element.base.id.as_str())
        && matches!(&element.kind, ElementKind::Image(image) if image.crop.is_some())
}

/// `renderElement(element, elementsMap, allElementsMap, rc, context,
/// renderConfig, appState, renderState)` (`renderElement.ts:963-1009`) in
/// the editor, on a context whose matrix is `base` (the device pixel ratio
/// times the zoom, in the static scene): the element's bitmap from `cache`
/// (made through `make_surface` when missing or stale) and its blit.
/// `Ok(None)` when there is no bitmap to draw. `render_state` defaults to
/// [`resolve_element_render_state`] of the element.
#[allow(clippy::too_many_arguments)]
pub fn render_element_cached<S>(
    element: &Element,
    elements_map: &ElementsMap<'_>,
    all_elements_map: &ElementsMap<'_>,
    config: &StaticCanvasRenderConfig,
    app_state: &StaticCanvasAppState,
    device_pixel_ratio: f64,
    base: Transform,
    render_state: Option<ElementRenderState>,
    cache: &mut ElementCanvasCache<S>,
    make_surface: impl FnOnce(ElementCanvas) -> S,
) -> Result<Option<ElementDraw>, RenderError> {
    let state = render_state.unwrap_or_else(|| {
        resolve_element_render_state(element, elements_map, config, all_elements_map)
    });
    let vector = config.is_exporting
        || matches!(
            element.kind,
            ElementKind::Frame(_) | ElementKind::MagicFrame(_) | ElementKind::Selection
        );
    if vector {
        let item = render_element(
            element,
            elements_map,
            all_elements_map,
            config,
            app_state,
            Some(state),
        )?;
        return Ok(Some(ElementDraw::Vector(item)));
    }
    let alpha = element_alpha(element, app_state, &state);
    let Some(bitmap) = cache.generate_element_with_canvas(
        element,
        all_elements_map,
        config,
        app_state,
        device_pixel_ratio,
        make_surface,
    )?
    else {
        return Ok(None);
    };
    let size = ElementCanvasSize {
        width: bitmap.width,
        height: bitmap.height,
        scale: bitmap.scale,
    };
    // freedraw keeps the context's smoothing (`:1066-1098`); the others
    // turn it off where they snap (`:1205-1218`)
    let smoothing = match element.kind {
        ElementKind::Freedraw(_) => None,
        _ if can_snap_element(
            Radians(element.base.angle.0),
            app_state.should_cache_ignore_zoom,
        ) =>
        {
            Some(false)
        }
        _ => None,
    };
    let preview = if is_cropping(element, app_state) {
        let uncropped = get_uncropped_image_element(element, elements_map);
        generate_element_canvas(
            &uncropped,
            all_elements_map,
            config,
            app_state,
            device_pixel_ratio,
        )?
        .map(|canvas| {
            let size = ElementCanvasSize {
                width: canvas.width,
                height: canvas.height,
                scale: canvas.scale,
            };
            // the preview stays at document coordinates
            let placement = draw_element_from_canvas(
                size,
                &uncropped,
                config,
                app_state,
                all_elements_map,
                device_pixel_ratio,
                [0.0, 0.0],
                base,
            );
            let preview = Blit {
                id: bitmap_id(&format!("{}:uncropped", element.base.id)),
                alpha: CROP_PREVIEW_ALPHA,
                smoothing,
                clip: placement.clip,
                transform: placement.transform,
                dest: placement.dest,
            };
            (canvas, preview)
        })
    } else {
        None
    };
    let placement = draw_element_from_canvas(
        size,
        element,
        config,
        app_state,
        all_elements_map,
        device_pixel_ratio,
        state.offset,
        base,
    );
    let blit = Blit {
        id: bitmap_id(&element.base.id),
        alpha,
        smoothing,
        clip: placement.clip,
        transform: placement.transform,
        dest: placement.dest,
    };
    Ok(Some(match preview {
        Some((uncropped, preview)) => ElementDraw::CropPreview(Box::new(CropPreview {
            uncropped,
            preview,
            blit,
        })),
        None => ElementDraw::Blit(blit),
    }))
}
