//! The static scene: `renderStaticScene`
//! (`packages/excalidraw/renderer/staticScene.ts`) with the helpers it
//! calls from `renderer/helpers.ts`, as a [`DisplayList`].
//!
//! The order of work is upstream's (`_renderStaticScene`, `:274-523`):
//!
//! 1. The scroll is snapped to whole device pixels
//!    ([`snap_scroll_to_device_pixels`]), except when exporting.
//! 2. `bootstrapCanvas` (`helpers.ts:73-127`): the device pixel ratio
//!    scale, and the background: a rectangle over the canvas in the view
//!    background colour through the dark filter of the app state's theme,
//!    white when the canvas rejects that colour, none for `transparent` or
//!    no colour. (Upstream clears the canvas first unless the colour is an
//!    opaque hex colour; a display list starts on a clear canvas.)
//! 3. The zoom scale.
//! 4. The grid ([`stroke_grid`]) when `renderGrid` is on.
//! 5. Every visible element that is not an iframe or embeddable, in order,
//!    inside its own `save()`/`restore()`: skipped when it is text bound to
//!    a container in the scene (it is drawn with the container), else the
//!    element and then its bound text; then, in the editor, its link icon.
//! 6. The iframes and embeddables, on top: the element, then when
//!    exporting (or for an embeddable whose link has not validated) its
//!    placeholder label, when it has a width and a height; then, in the
//!    editor, its link icon.
//! 7. The pending flowchart nodes.
//!
//! An element whose drawing fails is skipped with its bound text and link
//! icon, as upstream's `try`/`catch` skips an element that throws; when
//! only its bound text (or placeholder label) fails, the element stays and
//! the rest is skipped.
//! In steps 5 and 6 an element whose target frame clips it is drawn
//! inside that frame's clip ([`crate::frame`], `clipElementToFrame`,
//! `:358-393`): with its bound text, placeholder label and (for iframes and
//! embeddables) link icon, inside the same `save()`/`restore()`. The group
//! cache of those decisions (`inFrameGroupsMap`) lasts the whole scene. An
//! element whose clip cannot be decided (its outline cannot be built) is
//! skipped like one that cannot be drawn.
//!
//! One difference, by design, where drawing fails: upstream's `try` holds
//! the `context.save()`, the frame clip and the drawing, and the
//! `context.restore()` after them (`:397-452`, `:455-505`), so an element
//! or label that throws after its frame clip is applied leaves the save
//! and the clip in place, and every later element of the scene is drawn
//! inside that frame's clip (with whatever transform the throw left).
//! The port does not reproduce that leak: the failing element's clip
//! holds only what it drew, and the elements after it are drawn as they
//! would be had it drawn (`crates/excali-scene/tests/static_scene.rs`,
//! `a_clipped_element_whose_label_cannot_draw_does_not_clip_what_follows`;
//! `site/content/architecture/rendering-fidelity.md`).
//! Upstream also collects the groups of selected elements over the
//! highlighted frame (`:328-346`) and uses them nowhere; the port leaves
//! that out. Visibility culling happens before this function
//! (`Renderer.getRenderableElements`): the caller passes the visible
//! elements.

use std::collections::{HashMap, HashSet};

use excali_core::color::apply_dark_mode_filter;
use excali_core::constants::COLOR_WHITE;
use excali_core::element::{Element, ElementKind};
use excali_math::js;
use excali_text::text_measurements::TextMetricsProvider;

use crate::bounds::ElementsMap;
use crate::display::{
    bitmap_id, Clip, Color, Dash, DisplayItem, DisplayList, Group, Path, Rect, Stroke, Transform,
};
use crate::element_canvas::{
    render_element_cached, CropPreview, ElementCanvas, ElementCanvasCache, ElementDraw,
};
use crate::export::FrameRendering;
use crate::frame::{frame_clip, get_target_frame, should_apply_frame_clip, CheckedGroups};
use crate::render_element::{
    create_placeholder_embeddable_label, render_element, render_link_icon,
    resolve_element_render_state, with_transform, ElementRenderOverride, ElementRenderState,
    RenderError,
};
use crate::shape::{EmbedsValidationStatus, ShapeError, Theme};
use crate::sticky_note::Clock;

/// `GridLineColor[THEME.LIGHT].bold` (`staticScene.ts:57-66`).
pub const GRID_LINE_COLOR_BOLD: &str = "#dddddd";
/// `GridLineColor[THEME.LIGHT].regular`.
pub const GRID_LINE_COLOR_REGULAR: &str = "#e5e5e5";

/// `GridLineColor[theme]`: `(bold, regular)`, through the dark filter in
/// the dark theme.
pub fn grid_line_colors(theme: Theme) -> (String, String) {
    let dark = theme == Theme::Dark;
    (
        apply_dark_mode_filter(GRID_LINE_COLOR_BOLD, dark),
        apply_dark_mode_filter(GRID_LINE_COLOR_REGULAR, dark),
    )
}

/// An image of the image cache (`AppClassProperties["imageCache"]`): the
/// port only needs to know it is loaded and its MIME type; the bitmap is
/// the backend's, found by file id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CachedImage {
    pub mime_type: String,
}

/// What the static canvas reads of the app state (`StaticCanvasAppState`,
/// `packages/excalidraw/types.ts:205-221`).
#[derive(Clone, Debug, PartialEq)]
pub struct StaticCanvasAppState {
    pub zoom: f64,
    pub scroll_x: f64,
    pub scroll_y: f64,
    /// Picks the background's and the frame outlines' dark filter.
    pub theme: Theme,
    /// `None` for no background (upstream's `null`).
    pub view_background_color: Option<String>,
    pub grid_size: f64,
    pub grid_step: f64,
    pub frame_rendering: FrameRendering,
    pub selected_element_ids: HashSet<String>,
    pub hovered_element_ids: HashSet<String>,
    /// `openDialog?.name`.
    pub open_dialog: Option<String>,
    /// The frame selected elements are being dragged over, which becomes
    /// their clip (`getTargetFrame`, `frame.ts:790-815`).
    pub frame_to_highlight: Option<Element>,
    pub selected_elements_are_being_dragged: bool,
    pub editing_group_id: Option<String>,
    /// `shouldCacheIgnoreZoom`: a zoom gesture is running, so the
    /// per-element bitmaps keep their zoom and are not snapped
    /// ([`crate::element_canvas`]).
    pub should_cache_ignore_zoom: bool,
    /// `croppingElementId`: the image being cropped, drawn over its
    /// uncropped image at alpha 0.1 (`renderElement.ts:1220-1251`).
    pub cropping_element_id: Option<String>,
}

impl Default for StaticCanvasAppState {
    /// The default app state's values (`appState.ts`): zoom 1, no scroll,
    /// light, white, grid 20 with a bold line every 5.
    fn default() -> Self {
        StaticCanvasAppState {
            zoom: 1.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            theme: Theme::Light,
            view_background_color: Some(COLOR_WHITE.to_owned()),
            grid_size: excali_core::constants::DEFAULT_GRID_SIZE,
            grid_step: excali_core::constants::DEFAULT_GRID_STEP,
            frame_rendering: FrameRendering::default(),
            selected_element_ids: HashSet::new(),
            hovered_element_ids: HashSet::new(),
            open_dialog: None,
            frame_to_highlight: None,
            selected_elements_are_being_dragged: false,
            editing_group_id: None,
            should_cache_ignore_zoom: false,
            cropping_element_id: None,
        }
    }
}

/// `StaticCanvasRenderConfig` (`packages/excalidraw/scene/types.ts:28-45`).
#[derive(Clone, Debug, PartialEq)]
pub struct StaticCanvasRenderConfig {
    /// The fill of outline arrowheads.
    pub canvas_background_color: String,
    /// Not upstream (ex-807): the embedding host's canvas colour
    /// (`--excali-canvas-background` on `<excali-editor>`), painted in
    /// place of the app state's `viewBackgroundColor` and used as the
    /// outline arrowheads' fill, both without the dark filter: the host's
    /// palette is already the one it wants on screen. `None` (every export,
    /// and the editor unless the host sets one over a scene on upstream's
    /// default background) draws as upstream does.
    pub host_canvas_background: Option<String>,
    pub image_cache: HashMap<String, CachedImage>,
    pub render_grid: bool,
    /// `renderLinks !== false`: link icons in the editor.
    pub render_links: bool,
    pub is_exporting: bool,
    pub embeds_validation_status: EmbedsValidationStatus,
    pub elements_pending_erasure: HashSet<String>,
    pub pending_flowchart_nodes: Vec<Element>,
    /// Picks the elements' and the grid's dark filter.
    pub theme: Theme,
    pub element_render_overrides: HashMap<String, ElementRenderOverride>,
    /// `window.location.host`, which tells element links from other links
    /// (`isElementLink`).
    pub location_host: String,
    /// `Date.now()` and the viewer's time zone, which a sticky note's date
    /// footer reads.
    pub clock: Clock,
}

impl Default for StaticCanvasRenderConfig {
    fn default() -> Self {
        StaticCanvasRenderConfig {
            canvas_background_color: COLOR_WHITE.to_owned(),
            host_canvas_background: None,
            image_cache: HashMap::new(),
            render_grid: true,
            render_links: true,
            is_exporting: false,
            embeds_validation_status: HashMap::new(),
            elements_pending_erasure: HashSet::new(),
            pending_flowchart_nodes: Vec::new(),
            theme: Theme::Light,
            element_render_overrides: HashMap::new(),
            location_host: String::new(),
            clock: Clock::default(),
        }
    }
}

/// `StaticSceneRenderConfig` (`scene/types.ts`): the canvas and what to
/// draw on it.
pub struct StaticScene<'a> {
    /// `canvas.width` and `canvas.height`, in device pixels.
    pub canvas_width: f64,
    pub canvas_height: f64,
    /// The device pixel ratio.
    pub scale: f64,
    /// The renderable elements, where containers, bound text and frames
    /// are looked up.
    pub elements_map: &'a ElementsMap<'a>,
    /// Every element, for render offsets.
    pub all_elements_map: &'a ElementsMap<'a>,
    /// What to draw, in order.
    pub visible_elements: &'a [&'a Element],
    pub app_state: &'a StaticCanvasAppState,
    pub render_config: &'a StaticCanvasRenderConfig,
    /// Measures the placeholder labels of iframes and embeddables.
    pub text_metrics: &'a dyn TextMetricsProvider,
}

/// `snapScrollToDevicePixels(appState, scale)` (`helpers.ts:47-63`): the
/// scroll rounded to whole device pixels (`Math.round(scroll × zoom × dpr)
/// / (zoom × dpr)`), unchanged when there are no device pixels.
pub fn snap_scroll_to_device_pixels(
    scroll_x: f64,
    scroll_y: f64,
    zoom: f64,
    scale: f64,
) -> (f64, f64) {
    let device_pixels = zoom * scale;
    if device_pixels.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return (scroll_x, scroll_y);
    }
    (
        js::round(scroll_x * device_pixels) / device_pixels,
        js::round(scroll_y * device_pixels) / device_pixels,
    )
}

/// `strokeGrid`'s arguments (`staticScene.ts:68-81`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridConfig {
    /// Grid cell size in scene units.
    pub grid_size: f64,
    /// Every `grid_step`-th line is bold; 1 disables bold lines.
    pub grid_step: f64,
    pub scroll_x: f64,
    pub scroll_y: f64,
    pub zoom: f64,
    pub theme: Theme,
    /// The canvas size in scene units.
    pub width: f64,
    pub height: f64,
    /// The device pixel ratio.
    pub scale: f64,
}

/// `strokeGrid(...)` (`staticScene.ts:68-163`): the grid lines inside one
/// `save()`/`restore()`, vertical lines first, one stroke each.
///
/// - A line at `x` is bold when `gridStep > 1` and `round(x - scrollX)` is
///   a multiple of `gridStep × gridSize`.
/// - Regular lines are left out when `gridSize × zoom < 10`.
/// - Bold lines are solid in `#dddddd`, up to 4 CSS pixels wide; regular
///   lines 1 wide, dashed `[w × 3, space + (w + space)]` with `space =
///   1 / zoom`, in `#e5e5e5` (dark: both filtered).
/// - A line at least a device pixel wide is a whole number of device
///   pixels wide and centred to cover them (on the half pixel for odd
///   counts); a thinner one keeps its width and position.
///
/// The canvas keeps a `lineWidth` or `setLineDash` it rejects (a width of
/// 0, a NaN dash) at the previous line's value, and so does the port. Grid
/// sizes of 0 or less and canvases of unbounded size draw nothing, where
/// upstream's loop would not end.
pub fn stroke_grid(config: &GridConfig) -> DisplayItem {
    let GridConfig {
        grid_size,
        grid_step,
        scroll_x,
        scroll_y,
        zoom,
        theme,
        width,
        height,
        scale,
    } = *config;
    let (bold_color, regular_color) = grid_line_colors(theme);
    let offset_x = (scroll_x % grid_size) - grid_size;
    let offset_y = (scroll_y % grid_size) - grid_size;
    let actual_grid_size = grid_size * zoom;
    let space_width = 1.0 / zoom;
    // scene units → device pixels
    let device_pixels = zoom * scale;

    let snap = |position: f64, max_width_in_css_pixels: f64| -> (f64, f64) {
        let width_in_device_pixels = js::min(scale, max_width_in_css_pixels * device_pixels);
        if width_in_device_pixels < 1.0 {
            return (position, width_in_device_pixels / device_pixels);
        }
        let whole_width = js::round(width_in_device_pixels);
        let center = if whole_width % 2.0 != 0.0 && !whole_width.is_nan() {
            0.5
        } else {
            0.0
        };
        (
            (js::round(position * device_pixels - center) + center) / device_pixels,
            whole_width / device_pixels,
        )
    };

    let mut items = Vec::new();
    // the context's lineWidth and dash, which a rejected assignment keeps
    let mut line_width = 1.0;
    let mut dash: Vec<f64> = Vec::new();
    let mut line =
        |items: &mut Vec<DisplayItem>, is_bold: bool, from: (f64, f64), to: (f64, f64), lw: f64| {
            if lw.is_finite() && lw > 0.0 {
                line_width = lw;
            }
            let line_dash = [lw * 3.0, space_width + (lw + space_width)];
            let next: &[f64] = if is_bold { &[] } else { &line_dash };
            if next.iter().all(|v| v.is_finite() && *v >= 0.0) {
                dash = next.to_vec();
            }
            let color = if is_bold { &bold_color } else { &regular_color };
            let mut path = Path::new();
            path.move_to(from.0, from.1).line_to(to.0, to.1);
            items.push(DisplayItem::Stroke {
                path,
                stroke: Stroke::new(Color::new(color.as_str()), line_width)
                    .with_dash(Dash::new(&dash, 0.0)),
            });
        };

    let drawable = grid_size > 0.0
        && (offset_x + width + grid_size * 2.0).is_finite()
        && (offset_y + height + grid_size * 2.0).is_finite();
    if drawable {
        // vertical lines
        let mut x = offset_x;
        while x < offset_x + width + grid_size * 2.0 {
            let is_bold =
                grid_step > 1.0 && js::round(x - scroll_x) % (grid_step * grid_size) == 0.0;
            // don't render regular lines when zoomed out and they're barely
            // visible
            if is_bold || actual_grid_size >= 10.0 || actual_grid_size.is_nan() {
                let (position, lw) = snap(x, if is_bold { 4.0 } else { 1.0 });
                line(
                    &mut items,
                    is_bold,
                    (position, offset_y - grid_size),
                    (position, (offset_y + height + grid_size * 2.0).ceil()),
                    lw,
                );
            }
            x += grid_size;
        }
        let mut y = offset_y;
        while y < offset_y + height + grid_size * 2.0 {
            let is_bold =
                grid_step > 1.0 && js::round(y - scroll_y) % (grid_step * grid_size) == 0.0;
            if is_bold || actual_grid_size >= 10.0 || actual_grid_size.is_nan() {
                let (position, lw) = snap(y, if is_bold { 4.0 } else { 1.0 });
                line(
                    &mut items,
                    is_bold,
                    (offset_x - grid_size, position),
                    ((offset_x + width + grid_size * 2.0).ceil(), position),
                    lw,
                );
            }
            y += grid_size;
        }
    }
    DisplayItem::Group(Group::new(items))
}

/// `bootstrapCanvas`'s background (`helpers.ts:95-124`), in CSS pixels.
/// The host's colour (`host`, ex-807), when there is one, is painted as
/// given: no dark filter, and not checked against the canvas's parser (the
/// web runtime hands over a colour the canvas already read back).
fn background(
    width: f64,
    height: f64,
    color: Option<&str>,
    theme: Theme,
    host: Option<&str>,
) -> Option<DisplayItem> {
    if let Some(host) = host {
        return Some(DisplayItem::FillRect {
            rect: Rect::new(0.0, 0.0, width, height),
            color: Color::new(host),
        });
    }
    let color = color?;
    if color == "transparent" {
        return None;
    }
    // COLOR_WHITE first, so a colour the canvas rejects paints white
    let filtered = apply_dark_mode_filter(color, theme == Theme::Dark);
    let fill = if Color::new(filtered.as_str()).rgba().is_some() {
        filtered
    } else {
        COLOR_WHITE.to_owned()
    };
    // fillRect: the canvas draws a rectangle, not a path
    Some(DisplayItem::FillRect {
        rect: Rect::new(0.0, 0.0, width, height),
        color: Color::new(fill),
    })
}

/// `getRenderElementWithPositionOverride(element, offset)`
/// (`renderElement.ts:134-149`): the element moved by the offset, itself
/// when there is none.
fn with_position_override(element: &Element, offset: [f64; 2]) -> std::borrow::Cow<'_, Element> {
    if offset[0] == 0.0 && offset[1] == 0.0 {
        return std::borrow::Cow::Borrowed(element);
    }
    let mut moved = element.clone();
    moved.base.x += offset[0];
    moved.base.y += offset[1];
    std::borrow::Cow::Owned(moved)
}

/// `clipElementToFrame(element, renderState)` (`staticScene.ts:358-393`):
/// the frame clip to draw the element under, if any. Frames must render
/// and clip, and the element must name a frame (or a frame be
/// highlighted); its target frame, moved by the frame's render offset,
/// clips it when the element belongs to it and either is translated, or
/// when [`should_apply_frame_clip`] says so for the element at its drawn
/// position.
fn clip_element_to_frame(
    element: &Element,
    render_state: &ElementRenderState,
    scene: &StaticScene<'_>,
    app_state: &StaticCanvasAppState,
    in_frame_groups: &mut CheckedGroups,
) -> Result<Option<(Transform, Clip, Transform)>, ShapeError> {
    let frame_id = element.base.frame_id.as_deref().filter(|id| !id.is_empty());
    let highlighted = app_state
        .frame_to_highlight
        .as_ref()
        .is_some_and(|f| !f.base.id.is_empty());
    let fr = app_state.frame_rendering;
    if !(frame_id.is_some() || highlighted) || !fr.enabled || !fr.clip {
        return Ok(None);
    }
    let Some(target_frame) = get_target_frame(element, scene.elements_map, app_state) else {
        return Ok(None);
    };
    let frame_state = resolve_element_render_state(
        target_frame,
        scene.elements_map,
        scene.render_config,
        scene.all_elements_map,
    );
    let frame = with_position_override(target_frame, frame_state.offset);
    let is_translated =
        |state: &ElementRenderState| state.offset[0] != 0.0 || state.offset[1] != 0.0;
    let clip = (frame_id == Some(frame.base.id.as_str())
        && (is_translated(render_state) || is_translated(&frame_state)))
        || should_apply_frame_clip(
            &with_position_override(element, render_state.offset),
            &frame,
            app_state,
            scene.elements_map,
            Some(in_frame_groups),
        )?;
    Ok(clip.then(|| frame_clip(&frame, app_state)))
}

/// `items` inside the frame clip `clip` (translate, clip, translate
/// back), or as they are.
fn clipped(clip: Option<(Transform, Clip, Transform)>, items: Vec<DisplayItem>) -> DisplayItem {
    match clip {
        Some((to_frame, clip, back)) => DisplayItem::Group(Group {
            transform: to_frame,
            clip: Some(clip),
            ..Group::new(vec![with_transform(back, items)])
        }),
        None => DisplayItem::Group(Group::new(items)),
    }
}

/// `renderStaticScene(renderConfig)` (`staticScene.ts:274-545`): the
/// static canvas as a display list to replay from a fresh context, in the
/// order of work described in the module documentation.
pub fn render_static_scene(scene: &StaticScene<'_>) -> DisplayList {
    render_static_scene_with(scene, &mut |element, app_state, state| {
        render_element(
            element,
            scene.elements_map,
            scene.all_elements_map,
            scene.render_config,
            app_state,
            state,
        )
        .map(|item| vec![item])
    })
}

/// [`render_static_scene`] in the editor: every element, bound text,
/// embeddable and pending flowchart node goes through `renderElement`'s
/// editor path ([`render_element_cached`], `renderElement.ts:963-1009`),
/// drawn from its bitmap in `cache` ([`DisplayItem::Blit`]) unless it is a
/// frame or the scene is an export. `make_surface` makes the backend's
/// surface of each bitmap the cache makes or makes again, given the id its
/// blit draws; a pan reuses them all. The crop editor's uncropped preview
/// ([`ElementDraw::CropPreview`]) is made on every frame that draws it,
/// under the preview blit's id, and what `make_surface` returns for it is
/// not kept.
pub fn render_static_scene_cached<S>(
    scene: &StaticScene<'_>,
    cache: &mut ElementCanvasCache<S>,
    make_surface: &mut dyn FnMut(&Element, &str, ElementCanvas) -> S,
) -> DisplayList {
    // bootstrapCanvas's `scale(dpr)`, then the zoom: the matrix every
    // element is drawn on
    let zoom = scene.app_state.zoom;
    let base = Transform::scale(scene.scale, scene.scale).concat(&Transform::scale(zoom, zoom));
    render_static_scene_with(scene, &mut |element, app_state, state| {
        let draw = render_element_cached(
            element,
            scene.elements_map,
            scene.all_elements_map,
            scene.render_config,
            app_state,
            scene.scale,
            base,
            state,
            cache,
            |canvas| make_surface(element, &bitmap_id(&element.base.id), canvas),
        )?;
        Ok(match draw {
            Some(ElementDraw::Vector(item)) => vec![item],
            Some(ElementDraw::Blit(blit)) => vec![DisplayItem::Blit(blit)],
            // the crop editor's uncropped image, made for this frame only
            // under its blit's id, then the element's own bitmap
            Some(ElementDraw::CropPreview(crop)) => {
                let CropPreview {
                    uncropped,
                    preview,
                    blit,
                } = *crop;
                make_surface(element, &preview.id, uncropped);
                vec![DisplayItem::Blit(preview), DisplayItem::Blit(blit)]
            }
            // no bitmap (a side of 0): nothing drawn
            None => Vec::new(),
        })
    })
}

/// One element's draws on the scene's snapped app state.
type DrawElement<'f> = dyn FnMut(
        &Element,
        &StaticCanvasAppState,
        Option<ElementRenderState>,
    ) -> Result<Vec<DisplayItem>, RenderError>
    + 'f;

fn render_static_scene_with(scene: &StaticScene<'_>, draw: &mut DrawElement<'_>) -> DisplayList {
    let config = scene.render_config;
    let is_exporting = config.is_exporting;
    let raw = scene.app_state;
    // export draws vectors, not cached bitmaps — nothing to keep on the grid
    let snapped;
    let app_state = if is_exporting {
        raw
    } else {
        let (scroll_x, scroll_y) =
            snap_scroll_to_device_pixels(raw.scroll_x, raw.scroll_y, raw.zoom, scene.scale);
        snapped = StaticCanvasAppState {
            scroll_x,
            scroll_y,
            ..raw.clone()
        };
        &snapped
    };
    let normalized_width = scene.canvas_width / scene.scale;
    let normalized_height = scene.canvas_height / scene.scale;
    let zoom = app_state.zoom;

    let mut scene_items = Vec::new();

    // Grid
    if config.render_grid {
        scene_items.push(stroke_grid(&GridConfig {
            grid_size: app_state.grid_size,
            grid_step: app_state.grid_step,
            scroll_x: app_state.scroll_x,
            scroll_y: app_state.scroll_y,
            zoom,
            theme: config.theme,
            width: normalized_width / zoom,
            height: normalized_height / zoom,
            scale: scene.scale,
        }));
    }

    let elements_map = scene.elements_map;
    let all_elements_map = scene.all_elements_map;
    let mut render = |element: &Element, state| draw(element, app_state, state);
    let link_icon = |element: &Element, state: &ElementRenderState| {
        if is_exporting || !config.render_links {
            return None;
        }
        render_link_icon(element, app_state, elements_map, state, config, scene.scale)
    };
    let is_iframe_like =
        |e: &Element| matches!(e.kind, ElementKind::Iframe | ElementKind::Embeddable);
    let mut in_frame_groups = CheckedGroups::new();

    // Paint visible elements
    for &element in scene.visible_elements.iter().filter(|e| !is_iframe_like(e)) {
        if let ElementKind::Text(text) = &element.kind {
            if text
                .container_id
                .as_deref()
                .is_some_and(|id| !id.is_empty() && elements_map.get(id).is_some())
            {
                // will be rendered with the container
                continue;
            }
        }
        let bound_text = crate::bounds::get_bound_text_element(element, elements_map);
        let state = resolve_element_render_state(element, elements_map, config, all_elements_map);
        let Ok(clip) =
            clip_element_to_frame(element, &state, scene, app_state, &mut in_frame_groups)
        else {
            continue;
        };
        let Ok(mut items) = render(element, Some(state)) else {
            continue;
        };
        if let Some(text) = bound_text {
            // what names itself the bound text may be any element; when it
            // cannot be drawn the container stays drawn and the icon is
            // skipped, as upstream's throw leaves them
            let Ok(item) = render(text, None) else {
                scene_items.push(clipped(clip, items));
                continue;
            };
            items.extend(item);
        }
        scene_items.push(clipped(clip, items));
        scene_items.extend(link_icon(element, &state));
    }

    // render embeddables on top
    for &element in scene.visible_elements.iter().filter(|e| is_iframe_like(e)) {
        let state = resolve_element_render_state(element, elements_map, config, all_elements_map);
        let Ok(clip) =
            clip_element_to_frame(element, &state, scene, app_state, &mut in_frame_groups)
        else {
            continue;
        };
        let Ok(mut items) = render(element, Some(state)) else {
            continue;
        };
        let b = &element.base;
        let unvalidated_embed = matches!(element.kind, ElementKind::Embeddable)
            && config.embeds_validation_status.get(&b.id) != Some(&true);
        let truthy = |v: f64| v != 0.0 && !v.is_nan();
        if (is_exporting || unvalidated_embed) && truthy(b.width) && truthy(b.height) {
            let label = create_placeholder_embeddable_label(element, scene.text_metrics);
            let Ok(item) = render(&label, None) else {
                scene_items.push(clipped(clip, items));
                continue;
            };
            items.extend(item);
        }
        items.extend(link_icon(element, &state));
        scene_items.push(clipped(clip, items));
    }

    // render pending nodes for flowcharts
    for node in &config.pending_flowchart_nodes {
        if let Ok(items) = render(node, None) {
            scene_items.extend(items);
        }
    }

    // bootstrapCanvas: the device pixel ratio and the background; then the
    // zoom
    let mut root = Vec::new();
    root.extend(background(
        normalized_width,
        normalized_height,
        app_state.view_background_color.as_deref(),
        app_state.theme,
        config.host_canvas_background.as_deref(),
    ));
    root.push(with_transform(Transform::scale(zoom, zoom), scene_items));
    DisplayList::from_iter([with_transform(
        Transform::scale(scene.scale, scene.scale),
        root,
    )])
}
