//! Transform handles: where the eight resize handles and the rotation handle
//! of a selection sit (`packages/element/src/transformHandles.ts`).
//!
//! A handle is a square `size / zoom` wide, where the size depends on the
//! pointer ([`transform_handle_size`]: mouse 8, pen 16, touch 28,
//! `transformHandles.ts:49-53`). Corner handles sit outside the selection
//! box by the dashed-line margin; the rotation handle sits
//! [`ROTATION_RESIZE_HANDLE_GAP`] above the top edge; side handles are only
//! laid out when that side is longer than five mouse handles
//! (`transformHandles.ts:216-267`). Every handle is laid out on the
//! unrotated box and then turned about the box's centre with the element
//! (`generateTransformHandle`, `:95-110`).

use excali_core::constants::DEFAULT_TRANSFORM_HANDLE_SPACING;
use excali_core::element::{Element, ElementKind};
use excali_math::{point_from, point_rotate_rads, GlobalPoint, Radians};
use excali_scene::bounds::{get_element_absolute_coords, ElementsMap};

use crate::tools::PointerType;

/// `TransformHandleDirection`: the eight resize handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransformHandleDirection {
    N,
    S,
    W,
    E,
    Nw,
    Ne,
    Sw,
    Se,
}

impl TransformHandleDirection {
    /// The name upstream uses (`"n"`, `"se"`, ...).
    pub fn as_str(self) -> &'static str {
        match self {
            TransformHandleDirection::N => "n",
            TransformHandleDirection::S => "s",
            TransformHandleDirection::W => "w",
            TransformHandleDirection::E => "e",
            TransformHandleDirection::Nw => "nw",
            TransformHandleDirection::Ne => "ne",
            TransformHandleDirection::Sw => "sw",
            TransformHandleDirection::Se => "se",
        }
    }

    /// The direction named `name`.
    pub fn parse(name: &str) -> Option<TransformHandleDirection> {
        Some(match name {
            "n" => TransformHandleDirection::N,
            "s" => TransformHandleDirection::S,
            "w" => TransformHandleDirection::W,
            "e" => TransformHandleDirection::E,
            "nw" => TransformHandleDirection::Nw,
            "ne" => TransformHandleDirection::Ne,
            "sw" => TransformHandleDirection::Sw,
            "se" => TransformHandleDirection::Se,
            _ => return None,
        })
    }

    /// `handleDirection.includes(c)` for `c` one of `n`, `s`, `w`, `e`.
    pub fn includes(self, c: char) -> bool {
        self.as_str().contains(c)
    }

    /// `handleDirection.length === 1`: a side handle.
    pub fn is_side(self) -> bool {
        self.as_str().len() == 1
    }
}

/// `TransformHandleType`: a resize handle or the rotation handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransformHandleType {
    Resize(TransformHandleDirection),
    Rotation,
}

impl TransformHandleType {
    /// The name upstream uses (`"rotation"` for the rotation handle).
    pub fn as_str(self) -> &'static str {
        match self {
            TransformHandleType::Resize(d) => d.as_str(),
            TransformHandleType::Rotation => "rotation",
        }
    }

    /// The handle named `name`.
    pub fn parse(name: &str) -> Option<TransformHandleType> {
        if name == "rotation" {
            return Some(TransformHandleType::Rotation);
        }
        TransformHandleDirection::parse(name).map(TransformHandleType::Resize)
    }

    /// The resize direction, `None` for the rotation handle.
    pub fn direction(self) -> Option<TransformHandleDirection> {
        match self {
            TransformHandleType::Resize(d) => Some(d),
            TransformHandleType::Rotation => None,
        }
    }
}

/// `TransformHandle`: `[x, y, width, height]` of the handle's square,
/// whose centre is turned with the element.
pub type TransformHandle = [f64; 4];

/// `TransformHandles`: the handles laid out. A handle omitted, or a side
/// handle of a side too short for one, is `None`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TransformHandles {
    pub nw: Option<TransformHandle>,
    pub ne: Option<TransformHandle>,
    pub sw: Option<TransformHandle>,
    pub se: Option<TransformHandle>,
    pub rotation: Option<TransformHandle>,
    pub n: Option<TransformHandle>,
    pub s: Option<TransformHandle>,
    pub w: Option<TransformHandle>,
    pub e: Option<TransformHandle>,
}

impl TransformHandles {
    /// The handle of `kind`, when laid out.
    pub fn get(&self, kind: TransformHandleType) -> Option<TransformHandle> {
        use TransformHandleDirection as D;
        match kind {
            TransformHandleType::Rotation => self.rotation,
            TransformHandleType::Resize(d) => match d {
                D::N => self.n,
                D::S => self.s,
                D::W => self.w,
                D::E => self.e,
                D::Nw => self.nw,
                D::Ne => self.ne,
                D::Sw => self.sw,
                D::Se => self.se,
            },
        }
    }

    /// The handles laid out, in the order upstream's object holds its keys
    /// (`Object.keys`): `nw`, `ne`, `sw`, `se`, `rotation`, then `n`, `s`,
    /// `w`, `e` (`transformHandles.ts:154-267`). The first hit of a scan
    /// in this order wins.
    pub fn iter(&self) -> impl Iterator<Item = (TransformHandleType, TransformHandle)> + '_ {
        use TransformHandleDirection as D;
        [
            TransformHandleType::Resize(D::Nw),
            TransformHandleType::Resize(D::Ne),
            TransformHandleType::Resize(D::Sw),
            TransformHandleType::Resize(D::Se),
            TransformHandleType::Rotation,
            TransformHandleType::Resize(D::N),
            TransformHandleType::Resize(D::S),
            TransformHandleType::Resize(D::W),
            TransformHandleType::Resize(D::E),
        ]
        .into_iter()
        .filter_map(|kind| self.get(kind).map(|h| (kind, h)))
    }

    /// Whether no handle is laid out (`{}`).
    pub fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }
}

/// The handles to leave out (`{ [T in TransformHandleType]?: boolean }`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OmitSides {
    pub n: bool,
    pub s: bool,
    pub w: bool,
    pub e: bool,
    pub nw: bool,
    pub ne: bool,
    pub sw: bool,
    pub se: bool,
    pub rotation: bool,
}

/// `DEFAULT_OMIT_SIDES` (`transformHandles.ts:57-62`): no side handles
/// (sides are resized from the selection border instead).
pub const DEFAULT_OMIT_SIDES: OmitSides = OmitSides {
    n: true,
    s: true,
    w: true,
    e: true,
    nw: false,
    ne: false,
    sw: false,
    se: false,
    rotation: false,
};

/// `OMIT_SIDES_FOR_MULTIPLE_ELEMENTS` (`transformHandles.ts:64-69`).
pub const OMIT_SIDES_FOR_MULTIPLE_ELEMENTS: OmitSides = DEFAULT_OMIT_SIDES;

/// `OMIT_SIDES_FOR_FRAME` (`transformHandles.ts:71-77`): no side handles
/// and no rotation.
pub const OMIT_SIDES_FOR_FRAME: OmitSides = OmitSides {
    rotation: true,
    ..DEFAULT_OMIT_SIDES
};

/// `OMIT_SIDES_FOR_LINE_SLASH` (`transformHandles.ts:79-86`): a two-point
/// line along `/` keeps only the `ne` and `sw` corners (and rotation).
const OMIT_SIDES_FOR_LINE_SLASH: OmitSides = OmitSides {
    nw: true,
    se: true,
    ..DEFAULT_OMIT_SIDES
};

/// `OMIT_SIDES_FOR_LINE_BACKSLASH` (`transformHandles.ts:88-93`).
const OMIT_SIDES_FOR_LINE_BACKSLASH: OmitSides = DEFAULT_OMIT_SIDES;

/// `transformHandleSizes` (`transformHandles.ts:49-53`): the handle size in
/// screen pixels for a pointer type.
pub fn transform_handle_size(pointer_type: PointerType) -> f64 {
    match pointer_type {
        PointerType::Mouse => 8.0,
        PointerType::Pen => 16.0,
        PointerType::Touch => 28.0,
    }
}

/// `ROTATION_RESIZE_HANDLE_GAP` (`transformHandles.ts:55`): screen pixels
/// between the top handles and the rotation handle.
pub const ROTATION_RESIZE_HANDLE_GAP: f64 = 16.0;

/// The default dashed-line `margin` of [`get_transform_handles_from_coords`]
/// (`transformHandles.ts:139`).
pub const DEFAULT_HANDLE_MARGIN: f64 = 4.0;

/// `EditorInterface["formFactor"]` (`common/src/editorInterface.ts:4`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormFactor {
    Phone,
    Tablet,
    Desktop,
}

/// The parts of `EditorInterface` (`common/src/editorInterface.ts:3-13`)
/// the transform code reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorInterface {
    pub form_factor: FormFactor,
    /// `userAgent.isMobileDevice`.
    pub is_mobile_device: bool,
}

impl EditorInterface {
    pub fn new(form_factor: FormFactor, is_mobile_device: bool) -> EditorInterface {
        EditorInterface {
            form_factor,
            is_mobile_device,
        }
    }

    /// A desktop browser.
    pub fn desktop() -> EditorInterface {
        EditorInterface::new(FormFactor::Desktop, false)
    }
}

/// `canResizeFromSides(editorInterface)` (`transformHandles.ts:112-121`):
/// everywhere but on a phone's mobile browser.
pub fn can_resize_from_sides(editor: &EditorInterface) -> bool {
    !(editor.form_factor == FormFactor::Phone && editor.is_mobile_device)
}

/// `getOmitSidesForEditorInterface(editorInterface)`
/// (`transformHandles.ts:123-131`): [`DEFAULT_OMIT_SIDES`] where sides can
/// be resized from the border, else nothing omitted (side handles shown).
pub fn get_omit_sides_for_editor_interface(editor: &EditorInterface) -> OmitSides {
    if can_resize_from_sides(editor) {
        DEFAULT_OMIT_SIDES
    } else {
        OmitSides::default()
    }
}

/// `generateTransformHandle` (`transformHandles.ts:95-110`): the square at
/// `x, y` with its centre turned about `cx, cy`.
fn generate_transform_handle(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    cx: f64,
    cy: f64,
    angle: f64,
) -> TransformHandle {
    let p: GlobalPoint = point_rotate_rads(
        point_from(x + width / 2.0, y + height / 2.0),
        point_from(cx, cy),
        Radians(angle),
    );
    [p.x - width / 2.0, p.y - height / 2.0, width, height]
}

/// `getTransformHandlesFromCoords([x1, y1, x2, y2, cx, cy], angle, zoom,
/// pointerType, omitSides, margin, spacing)` (`transformHandles.ts:133-270`).
/// `margin` defaults to [`DEFAULT_HANDLE_MARGIN`] and `spacing` to
/// `DEFAULT_TRANSFORM_HANDLE_SPACING`.
pub fn get_transform_handles_from_coords(
    [x1, y1, x2, y2, cx, cy]: [f64; 6],
    angle: f64,
    zoom: f64,
    pointer_type: PointerType,
    omit_sides: &OmitSides,
    margin: Option<f64>,
    spacing: Option<f64>,
) -> TransformHandles {
    let margin = margin.unwrap_or(DEFAULT_HANDLE_MARGIN);
    let spacing = spacing.unwrap_or(DEFAULT_TRANSFORM_HANDLE_SPACING);
    let size = transform_handle_size(pointer_type);
    let handle_width = size / zoom;
    let handle_height = size / zoom;

    let handle_margin_x = size / zoom;
    let handle_margin_y = size / zoom;

    let width = x2 - x1;
    let height = y2 - y1;
    let dashed_line_margin = margin / zoom;
    let centering_offset = (size - spacing * 2.0) / (2.0 * zoom);

    let handle = |x: f64, y: f64| {
        generate_transform_handle(x, y, handle_width, handle_height, cx, cy, angle)
    };
    let left = x1 - dashed_line_margin - handle_margin_x + centering_offset;
    let top = y1 - dashed_line_margin - handle_margin_y + centering_offset;
    let right = x2 + dashed_line_margin - centering_offset;
    let bottom = y2 + dashed_line_margin - centering_offset;

    let mut handles = TransformHandles {
        nw: (!omit_sides.nw).then(|| handle(left, top)),
        ne: (!omit_sides.ne).then(|| handle(right, top)),
        sw: (!omit_sides.sw).then(|| handle(left, bottom)),
        se: (!omit_sides.se).then(|| handle(right, bottom)),
        rotation: (!omit_sides.rotation).then(|| {
            handle(
                x1 + width / 2.0 - handle_width / 2.0,
                y1 - dashed_line_margin - handle_margin_y + centering_offset
                    - ROTATION_RESIZE_HANDLE_GAP / zoom,
            )
        }),
        ..TransformHandles::default()
    };

    // We only want to show height handles (all cardinal directions) above a
    // certain size. Note: we render using "mouse" size so we should also use
    // "mouse" size for this check
    let minimum_size_for_eight_handles = (5.0 * transform_handle_size(PointerType::Mouse)) / zoom;
    if width.abs() > minimum_size_for_eight_handles {
        if !omit_sides.n {
            handles.n = Some(handle(x1 + width / 2.0 - handle_width / 2.0, top));
        }
        if !omit_sides.s {
            handles.s = Some(handle(x1 + width / 2.0 - handle_width / 2.0, bottom));
        }
    }
    if height.abs() > minimum_size_for_eight_handles {
        if !omit_sides.w {
            handles.w = Some(handle(left, y1 + height / 2.0 - handle_height / 2.0));
        }
        if !omit_sides.e {
            handles.e = Some(handle(right, y1 + height / 2.0 - handle_height / 2.0));
        }
    }
    handles
}

fn is_linear(element: &Element) -> bool {
    matches!(element.kind, ElementKind::Line(_) | ElementKind::Arrow(_))
}

pub(crate) fn is_elbow_arrow(element: &Element) -> bool {
    matches!(&element.kind, ElementKind::Arrow(a) if a.elbowed)
}

pub(crate) fn is_frame_like(element: &Element) -> bool {
    matches!(
        element.kind,
        ElementKind::Frame(_) | ElementKind::MagicFrame(_)
    )
}

/// `getTransformHandles(element, zoom, elementsMap, pointerType,
/// omitSides)` (`transformHandles.ts:272-326`): the handles of one element.
///
/// - Locked elements and elbow arrows have none.
/// - A two-point line, arrow or freedraw keeps only the corners along its
///   diagonal (`/` or `\`), whatever `omit_sides` says.
/// - Frames are never rotated.
/// - Lines and arrows sit 8 further out; images have no margin and no
///   spacing.
///
/// Upstream's default `omitSides` is [`DEFAULT_OMIT_SIDES`].
pub fn get_transform_handles(
    element: &Element,
    zoom: f64,
    elements_map: &ElementsMap<'_>,
    pointer_type: PointerType,
    omit_sides: &OmitSides,
) -> TransformHandles {
    // so that when locked element is selected (especially when you toggle
    // lock via keyboard) the locked element is visually distinct, indicating
    // you can't move/resize
    if element.base.locked || is_elbow_arrow(element) {
        return TransformHandles::default();
    }

    let mut omit = *omit_sides;
    match &element.kind {
        ElementKind::Freedraw(_) | ElementKind::Line(_) | ElementKind::Arrow(_) => {
            let points = element.kind.points().unwrap_or(&[]);
            if points.len() == 2 {
                // only check the last point because starting point is always (0,0)
                let [px, py] = points[1];
                if px == 0.0 || py == 0.0 {
                    omit = OMIT_SIDES_FOR_LINE_BACKSLASH;
                } else if px > 0.0 && py < 0.0 {
                    omit = OMIT_SIDES_FOR_LINE_SLASH;
                } else if px > 0.0 && py > 0.0 {
                    omit = OMIT_SIDES_FOR_LINE_BACKSLASH;
                } else if px < 0.0 && py > 0.0 {
                    omit = OMIT_SIDES_FOR_LINE_SLASH;
                } else if px < 0.0 && py < 0.0 {
                    omit = OMIT_SIDES_FOR_LINE_BACKSLASH;
                }
            }
        }
        _ if is_frame_like(element) => omit.rotation = true,
        _ => {}
    }
    let is_image = matches!(element.kind, ElementKind::Image(_));
    let margin = if is_linear(element) {
        DEFAULT_TRANSFORM_HANDLE_SPACING + 8.0
    } else if is_image {
        0.0
    } else {
        DEFAULT_TRANSFORM_HANDLE_SPACING
    };
    get_transform_handles_from_coords(
        get_element_absolute_coords(element, elements_map, true),
        element.base.angle.0,
        zoom,
        pointer_type,
        &omit,
        Some(margin),
        if is_image { Some(0.0) } else { None },
    )
}

/// `appState.selectedLinearElement`'s editing state, as
/// [`has_bounding_box`] and [`crate::transform::TransformSession::begin`]
/// read it (the rest of `LinearElementEditor` is ex-511's).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectedLinearElementState {
    pub is_editing: bool,
    pub is_dragging: bool,
    /// `hoverPointIndex`: the point under the pointer, `-1` for none.
    pub hover_point_index: i64,
}

impl Default for SelectedLinearElementState {
    /// A linear element just selected: not edited, not dragged, no point
    /// hovered (`hoverPointIndex: -1`, `linearElementEditor.ts`).
    fn default() -> Self {
        SelectedLinearElementState {
            is_editing: false,
            is_dragging: false,
            hover_point_index: -1,
        }
    }
}

/// `hasBoundingBox(elements, appState, editorInterface)`
/// (`transformHandles.ts:328-354`): whether the selection shows its
/// transform box. Not while a line is edited or its point dragged; always
/// for several elements; never for a lone elbow arrow; for a lone line or
/// arrow only with more than two points and not on a mobile device.
pub fn has_bounding_box(
    elements: &[&Element],
    selected_linear_element: Option<SelectedLinearElementState>,
    editor: &EditorInterface,
) -> bool {
    if selected_linear_element.is_some_and(|l| l.is_editing || l.is_dragging) {
        return false;
    }
    if elements.len() > 1 {
        return true;
    }
    let Some(element) = elements.first() else {
        // `elements[0]` is undefined: isElbowArrow and isLinearElement are
        // false for it
        return true;
    };
    if is_elbow_arrow(element) {
        // Elbow arrows cannot be resized as single selected elements
        return false;
    }
    if !is_linear(element) {
        return true;
    }
    // on mobile/tablet we currently don't show bbox because of resize issues
    element.kind.points().map_or(0, <[_]>::len) > 2 && !editor.is_mobile_device
}
