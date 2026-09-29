//! A transform gesture: the pointer pressed on a handle of the selection,
//! then moved (`App.tsx`: `initialPointerDownState` `:9389-9430`,
//! `handleSelectionOnPointerDown` `:9515-9600`, `maybeHandleResize`
//! `:13684-13833`).
//!
//! [`TransformSession::begin`] finds the handle under the pointer, keeps
//! copies of the elements as they were (every resize is computed from
//! them, so a gesture never accumulates rounding or modifier changes) and
//! how far the pointer is from the grabbed edge. [`TransformSession::update`]
//! runs [`transform_elements`] for a pointer position with the keyboard's
//! modifiers. Snapping to other elements (`snapResizingElements`) is not
//! applied here; the grid is.

use std::collections::HashSet;

use excali_core::element::{Element, ElementKind};
use excali_math::js;
use excali_scene::bounds::get_common_bounds;

use crate::resize_elements::{
    get_resize_arrow_direction, get_resize_offset_xy, transform_elements, ArrowDirection,
    TransformEnv, TransformFlags,
};
use crate::resize_test::{
    get_element_with_transform_handle_type, get_transform_handle_type_from_coords,
};
use crate::scene::Scene;
use crate::tools::PointerType;
use crate::transform_handles::{
    is_elbow_arrow, is_frame_like, EditorInterface, SelectedLinearElementState, TransformHandleType,
};

/// The keys held during a transform: Shift keeps the aspect ratio (frees it
/// for images and a sticky note's corners) and rotates in 15 degree steps,
/// Alt resizes from the centre, Ctrl/Cmd ignores the grid
/// (`common/src/keys.ts:145-153`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TransformModifiers {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}

/// A transform gesture from pointer-down (`PointerDownState.resize` and
/// `originalElements`).
#[derive(Debug, Clone, PartialEq)]
pub struct TransformSession {
    selected: Vec<String>,
    original_elements: Vec<Element>,
    handle: Option<TransformHandleType>,
    offset: [f64; 2],
    center: [f64; 2],
    arrow_direction: ArrowDirection,
}

fn is_linear(e: &Element) -> bool {
    matches!(e.kind, ElementKind::Line(_) | ElementKind::Arrow(_))
}

/// `getGridPoint(x, y, gridSize)` (`common/src/points.ts:69-81`).
pub fn get_grid_point(x: f64, y: f64, grid_size: Option<f64>) -> [f64; 2] {
    match grid_size {
        Some(g) if g != 0.0 && !g.is_nan() => [js::round(x / g) * g, js::round(y / g) * g],
        _ => [x, y],
    }
}

impl TransformSession {
    /// The pointer pressed at `origin` (scene coordinates) with
    /// `selected` selected and `selected_linear_element` as
    /// `appState.selectedLinearElement`. One element that is not an elbow
    /// arrow (nor, on a mobile device or with two points, a line) is tested
    /// with [`get_element_with_transform_handle_type`], unless the linear
    /// element editor is editing or one of its points is hovered; several
    /// with [`get_transform_handle_type_from_coords`] on their common box.
    /// [`TransformSession::handle`] is `None` when the pointer is on no
    /// handle; the gesture then transforms nothing.
    pub fn begin(
        scene: &Scene,
        selected: &[String],
        origin: [f64; 2],
        zoom: f64,
        pointer_type: PointerType,
        editor: &EditorInterface,
        selected_linear_element: Option<SelectedLinearElementState>,
    ) -> TransformSession {
        let all = scene.non_deleted();
        let elements_map = scene.elements_map();
        let sel = scene.selected(selected);
        let [min_x, min_y, max_x, max_y] = get_common_bounds(&sel);
        let original_elements: Vec<Element> = all.iter().map(|e| (*e).clone()).collect();
        let selected_ids: HashSet<String> = selected.iter().cloned().collect();

        let mut handle = None;
        if sel.len() == 1
            && !selected_linear_element.is_some_and(|l| l.is_editing)
            && !is_elbow_arrow(sel[0])
            && !(is_linear(sel[0])
                && (editor.is_mobile_device || sel[0].kind.points().map_or(0, <[_]>::len) == 2))
            && !selected_linear_element.is_some_and(|l| l.hover_point_index != -1)
        {
            handle = get_element_with_transform_handle_type(
                &all,
                &selected_ids,
                origin[0],
                origin[1],
                zoom,
                pointer_type,
                &elements_map,
                editor,
            )
            .map(|(_, h)| h);
        } else if sel.len() > 1 {
            handle = get_transform_handle_type_from_coords(
                get_common_bounds(&sel),
                origin[0],
                origin[1],
                zoom,
                pointer_type,
                editor,
            );
        }

        let mut offset = [0.0, 0.0];
        let mut arrow_direction = ArrowDirection::Origin;
        if handle.is_some() {
            offset = get_resize_offset_xy(handle, &sel, &elements_map, origin[0], origin[1]);
            if sel.len() == 1
                && is_linear(sel[0])
                && sel[0].kind.points().map_or(0, <[_]>::len) == 2
            {
                arrow_direction = get_resize_arrow_direction(handle, sel[0]);
            }
        }

        TransformSession {
            selected: sel.iter().map(|e| e.base.id.clone()).collect(),
            original_elements,
            handle,
            offset,
            center: [(max_x + min_x) / 2.0, (max_y + min_y) / 2.0],
            arrow_direction,
        }
    }

    /// The handle pressed, if any.
    pub fn handle(&self) -> Option<TransformHandleType> {
        self.handle
    }

    /// `resize.offset`: subtracted from the pointer on every move.
    pub fn offset(&self) -> [f64; 2] {
        self.offset
    }

    /// `resize.center`: the centre of the selection's box, what several
    /// elements rotate about.
    pub fn center(&self) -> [f64; 2] {
        self.center
    }

    /// `resize.arrowDirection`: for a lone two-point line, which end its
    /// corner handle drags.
    pub fn arrow_direction(&self) -> ArrowDirection {
        self.arrow_direction
    }

    /// The elements as they were at pointer-down.
    pub fn original_elements(&self) -> &[Element] {
        &self.original_elements
    }

    /// `maybeHandleResize` for the pointer at `pointer`: nothing without a
    /// handle, for a rotation of a selection with a frame or for a lone
    /// elbow arrow; otherwise [`transform_elements`] with the pointer less
    /// the offset (on the grid, unless Ctrl is held). Images, and a lone
    /// sticky note's corners, keep their aspect ratio unless Shift is held.
    /// Returns whether a transform happened.
    pub fn update(
        &self,
        scene: &mut Scene,
        env: &mut dyn TransformEnv,
        pointer: [f64; 2],
        modifiers: TransformModifiers,
        grid_size: Option<f64>,
    ) -> bool {
        let Some(handle) = self.handle else {
            return false;
        };
        let sel = scene.selected(&self.selected);
        if (handle == TransformHandleType::Rotation && sel.iter().any(|e| is_frame_like(e)))
            || (sel.len() == 1 && is_elbow_arrow(sel[0]))
        {
            return false;
        }
        let resize = get_grid_point(
            pointer[0] - self.offset[0],
            pointer[1] - self.offset[1],
            if modifiers.ctrl { None } else { grid_size },
        );
        // images are proportional by default, and so is a sticky note's
        // corner (its label's font ceiling scales with it); Shift frees them
        let proportional_by_default = sel.iter().any(|e| matches!(e.kind, ElementKind::Image(_)))
            || (sel.len() == 1
                && matches!(sel[0].kind, ElementKind::StickyNote(_))
                && handle.direction().is_some_and(|d| !d.is_side()));
        let selected = self.selected.clone();
        transform_elements(
            &self.original_elements,
            Some(handle),
            &selected,
            scene,
            env,
            TransformFlags {
                rotate_with_discrete_angle: modifiers.shift,
                resize_from_center: modifiers.alt,
                maintain_aspect_ratio: if proportional_by_default {
                    !modifiers.shift
                } else {
                    modifiers.shift
                },
            },
            resize,
            self.center,
        )
    }
}
