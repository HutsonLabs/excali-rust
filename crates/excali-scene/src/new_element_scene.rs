//! The new-element scene: `renderNewElementScene`
//! (`packages/excalidraw/renderer/renderNewElementScene.ts`), the element
//! being created (or a tool dragged out of the toolbar) drawn alone on its
//! own canvas above the static one, as a [`DisplayList`].
//!
//! The order of work is upstream's (`_renderNewElementScene`, `:20-91`):
//!
//! 1. The scroll is snapped to whole device pixels
//!    ([`snap_scroll_to_device_pixels`]): the same scroll the static scene
//!    draws at, so the element lands where it will once it is in the scene.
//! 2. `bootstrapCanvas` with neither theme nor background: the device
//!    pixel ratio scale and a clear (the canvas layer's, `excali_ui::layers`).
//! 3. The zoom scale.
//! 4. With an element that is not a selection box: nothing when it is
//!    invisibly small ([`is_invisibly_small_element`]); otherwise inside its
//!    target frame's clip when frames render and clip and it names a frame
//!    (or one is highlighted) that clips it (`shouldApplyFrameClip`), the
//!    element (`renderElement`).
//! 5. Without one (or with a selection box): the canvas is cleared again
//!    under the zoom, which [`render_new_element_scene`] reports as `None`.
//!
//! An element whose drawing fails draws nothing, where upstream's throw
//! would leave the render.

use excali_core::element::{Element, ElementKind};

use crate::bounds::ElementsMap;
use crate::display::{Clip, DisplayItem, DisplayList, Group, Transform};
use crate::frame::{frame_clip, get_target_frame, should_apply_frame_clip};
use crate::render_element::{render_element, with_transform};
use crate::shape::ShapeError;
use crate::static_scene::{
    snap_scroll_to_device_pixels, StaticCanvasAppState, StaticCanvasRenderConfig,
};

/// `INVISIBLY_SMALL_ELEMENT_SIZE` (`packages/element/src/sizeHelpers.ts:55`).
pub const INVISIBLY_SMALL_ELEMENT_SIZE: f64 = 0.1;

/// `isInvisiblySmallElement(element)` (`sizeHelpers.ts:61-78`): a line,
/// arrow or freedraw with fewer than two points, an arrow of two points
/// closer than [`INVISIBLY_SMALL_ELEMENT_SIZE`] on both axes, or any other
/// element with a zero width and height.
pub fn is_invisibly_small_element(element: &Element) -> bool {
    let points = match &element.kind {
        ElementKind::Line(line) => &line.linear.points,
        ElementKind::Arrow(arrow) => &arrow.linear.points,
        ElementKind::Freedraw(freedraw) => &freedraw.points,
        _ => return element.base.width == 0.0 && element.base.height == 0.0,
    };
    points.len() < 2
        || (points.len() == 2
            && matches!(element.kind, ElementKind::Arrow(_))
            && points_equal(points[0], points[1], INVISIBLY_SMALL_ELEMENT_SIZE))
}

/// `pointsEqual(a, b, tolerance)` (`packages/math/src/point.ts:108-115`):
/// both coordinates closer than `tolerance`.
fn points_equal(a: [f64; 2], b: [f64; 2], tolerance: f64) -> bool {
    (a[0] - b[0]).abs() < tolerance && (a[1] - b[1]).abs() < tolerance
}

/// `NewElementSceneRenderConfig` (`scene/types.ts`): the canvas and what
/// to draw on it.
pub struct NewElementScene<'a> {
    /// `canvas.width` and `canvas.height`, in device pixels.
    pub canvas_width: f64,
    pub canvas_height: f64,
    /// The device pixel ratio.
    pub scale: f64,
    /// The element being created, if any.
    pub new_element: Option<&'a Element>,
    /// The renderable elements, where containers, bound text and frames
    /// are looked up.
    pub elements_map: &'a ElementsMap<'a>,
    /// Every element, for render offsets.
    pub all_elements_map: &'a ElementsMap<'a>,
    pub app_state: &'a StaticCanvasAppState,
    /// The editor's render config for this canvas (App.tsx:2700-2716: no
    /// grid, not exporting).
    pub render_config: &'a StaticCanvasRenderConfig,
}

/// `renderNewElementScene(renderConfig)` (`renderNewElementScene.ts`): the
/// new element under the device pixel ratio and the zoom at the snapped
/// scroll, to replay after the canvas's bootstrap. `None` when there is no
/// element to draw (none, or a selection box), where upstream clears the
/// canvas again under the zoom; an empty list when the element is
/// invisibly small or cannot be drawn.
pub fn render_new_element_scene(scene: &NewElementScene<'_>) -> Option<DisplayList> {
    let element = scene
        .new_element
        .filter(|e| !matches!(e.kind, ElementKind::Selection))?;
    // the same whole-device-pixel scroll the static scene draws at
    let raw = scene.app_state;
    let (scroll_x, scroll_y) =
        snap_scroll_to_device_pixels(raw.scroll_x, raw.scroll_y, raw.zoom, scene.scale);
    let app_state = StaticCanvasAppState {
        scroll_x,
        scroll_y,
        ..raw.clone()
    };
    // e.g. when creating arrows and we're still below the arrow drag
    // distance threshold
    if is_invisibly_small_element(element) {
        return Some(DisplayList::default());
    }
    let Ok(clip) = new_element_frame_clip(element, scene, &app_state) else {
        return Some(DisplayList::default());
    };
    let Ok(item) = render_element(
        element,
        scene.elements_map,
        scene.all_elements_map,
        scene.render_config,
        &app_state,
        None,
    ) else {
        return Some(DisplayList::default());
    };
    let drawn = match clip {
        Some((to_frame, clip, back)) => DisplayItem::Group(Group {
            transform: to_frame,
            clip: Some(clip),
            ..Group::new(vec![with_transform(back, vec![item])])
        }),
        None => item,
    };
    let zoom = app_state.zoom;
    Some(DisplayList::from_iter([with_transform(
        Transform::scale(scene.scale, scene.scale),
        vec![with_transform(Transform::scale(zoom, zoom), vec![drawn])],
    )]))
}

/// The frame clip of `renderNewElementScene.ts:59-72`: the element's
/// frame, or the highlighted frame, when frames render and clip; its target
/// frame clips it when `shouldApplyFrameClip` says so. Unlike the static
/// scene's `clipElementToFrame`, no render offsets are applied. An error
/// when the decision cannot be made (an outline cannot be built), where
/// upstream throws and the element is not drawn.
fn new_element_frame_clip(
    element: &Element,
    scene: &NewElementScene<'_>,
    app_state: &StaticCanvasAppState,
) -> Result<Option<(Transform, Clip, Transform)>, ShapeError> {
    let has_frame = element
        .base
        .frame_id
        .as_deref()
        .is_some_and(|id| !id.is_empty())
        || app_state
            .frame_to_highlight
            .as_ref()
            .is_some_and(|f| !f.base.id.is_empty());
    let fr = app_state.frame_rendering;
    if !(has_frame && fr.enabled && fr.clip) {
        return Ok(None);
    }
    let Some(frame) = get_target_frame(element, scene.elements_map, app_state) else {
        return Ok(None);
    };
    let clips = should_apply_frame_clip(element, frame, app_state, scene.elements_map, None)?;
    Ok(clips.then(|| frame_clip(frame, app_state)))
}
