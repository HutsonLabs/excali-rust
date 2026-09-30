//! Drawing a new element: the element a drawing tool's press creates, with
//! the attributes App takes from the app state's `currentItem*` keys, and
//! its size as the pointer drags.
//!
//! Upstream, at the pinned commit:
//!
//! - `packages/element/src/newElement.ts`: `_newElementBase` (`:87-172`),
//!   `newElement`, `newFrameElement` (`:263-278`), `newFreeDrawElement`
//!   (`:583-603`), `newLinearElement` (`:604-630`), `newArrowElement`
//!   (`:632-672`);
//! - `packages/excalidraw/components/App.tsx`: the attributes of
//!   `createGenericElementOnPointerDown` (`:10534-10601`,
//!   `getCurrentItemRoundness` and `getCurrentItemStrokeWidth`
//!   `:10508-10532`), `handleLinearElementOnPointerDown` (`:10313-10366`),
//!   `handleFreeDrawElementOnPointerDown` (`:9988-10035`) and
//!   `createFrameElementOnPointerDown` (`:10603-10634`, `FRAME_STYLE`);
//! - `packages/element/src/dragElements.ts:294-406`: `dragNewElement`;
//! - `packages/element/src/sizeHelpers.ts:158-185`: `getPerfectElementSize`.

use excali_core::app_state::AppState;
use excali_core::constants::stroke_width_by_key;
use excali_core::element::{Element, ElementType, StrokeWidthKey};
use excali_math::js;
use serde_json::{json, Map, Value};

use crate::resize_elements::SHIFT_LOCKING_ANGLE;

/// `ROUNDNESS.PROPORTIONAL_RADIUS` and `ROUNDNESS.ADAPTIVE_RADIUS`.
pub(crate) const PROPORTIONAL_RADIUS: u8 = 2;
pub(crate) const ADAPTIVE_RADIUS: u8 = 3;

/// `DEFAULT_STROKE_STREAMLINE` (`common/src/constants.ts:622`).
pub const DEFAULT_STROKE_STREAMLINE: f64 = 0.5;

fn state(app_state: &AppState, key: &str) -> Value {
    app_state.get(key).cloned().unwrap_or(Value::Null)
}

fn state_str<'a>(app_state: &'a AppState, key: &str) -> Option<&'a str> {
    app_state.get(key).and_then(Value::as_str)
}

/// `getStrokeWidthByKey(type, appState.currentItemStrokeWidthKey)`.
pub(crate) fn current_stroke_width(ty: ElementType, app_state: &AppState) -> f64 {
    let key = match state_str(app_state, "currentItemStrokeWidthKey") {
        Some("thin") => StrokeWidthKey::Thin,
        Some("bold") => StrokeWidthKey::Bold,
        _ => StrokeWidthKey::Medium,
    };
    stroke_width_by_key(ty, key)
}

/// `isUsingAdaptiveRadius(type)` (`typeChecks.ts:357-362`).
pub(crate) fn is_using_adaptive_radius(tool: &str) -> bool {
    matches!(tool, "rectangle" | "embeddable" | "iframe" | "image")
}

fn roundness(kind: Option<u8>) -> Value {
    kind.map_or(Value::Null, |t| json!({ "type": t }))
}

/// `_newElementBase(type, opts)`: the keys every element has, in
/// upstream's order, `opts` holding the constructor options given
/// (missing ones take `DEFAULT_ELEMENT_PROPS`).
pub(crate) fn new_element_base(
    ty: &str,
    opts: &Map<String, Value>,
    id: &str,
    seed: f64,
    timestamp: f64,
) -> Map<String, Value> {
    let opt = |key: &str, default: Value| opts.get(key).cloned().unwrap_or(default);
    let mut m = Map::new();
    m.insert("id".into(), json!(id));
    m.insert("type".into(), json!(ty));
    m.insert("x".into(), opt("x", json!(0)));
    m.insert("y".into(), opt("y", json!(0)));
    m.insert("width".into(), opt("width", json!(0)));
    m.insert("height".into(), opt("height", json!(0)));
    m.insert("angle".into(), json!(0));
    m.insert("strokeColor".into(), opt("strokeColor", json!("#1e1e1e")));
    m.insert(
        "backgroundColor".into(),
        opt("backgroundColor", json!("transparent")),
    );
    m.insert("fillStyle".into(), opt("fillStyle", json!("solid")));
    m.insert("strokeWidth".into(), opt("strokeWidth", json!(2)));
    m.insert("strokeStyle".into(), opt("strokeStyle", json!("solid")));
    m.insert("roughness".into(), opt("roughness", json!(1)));
    m.insert("opacity".into(), opt("opacity", json!(100)));
    m.insert("groupIds".into(), json!([]));
    m.insert("frameId".into(), opt("frameId", Value::Null));
    m.insert("index".into(), Value::Null);
    m.insert("roundness".into(), opt("roundness", Value::Null));
    m.insert("seed".into(), json!(seed));
    m.insert("version".into(), json!(1));
    m.insert("versionNonce".into(), json!(0));
    m.insert("isDeleted".into(), json!(false));
    m.insert("boundElements".into(), Value::Null);
    m.insert("updated".into(), json!(timestamp));
    m.insert("created".into(), json!(timestamp));
    m.insert("link".into(), Value::Null);
    m.insert("locked".into(), opt("locked", json!(false)));
    m
}

/// The element a press of the drawing tool `tool` creates at `origin`
/// (already on the grid), in the frame `frame_id`: `rectangle`, `diamond`,
/// `ellipse` and `selection` (`newElement`), `arrow` (`newArrowElement`),
/// `line` (`newLinearElement`), `freedraw` (`newFreeDrawElement`, a mouse:
/// simulated pressure, the default streamline) and `frame`
/// (`newFrameElement` with `FRAME_STYLE`), with App's attributes. A line or
/// an arrow has no points yet (App gives it two with `mutateElement`).
/// `None` for any other tool. `id`, `seed` and `timestamp` are the drawn
/// `randomId()`, `randomInteger()` and `getUpdatedTimestamp()`.
pub fn new_element_for_tool(
    tool: &str,
    app_state: &AppState,
    origin: [f64; 2],
    frame_id: Option<&str>,
    id: &str,
    seed: f64,
    timestamp: f64,
) -> Option<Element> {
    let ty = ElementType::parse(tool)?;
    let [x, y] = origin;
    let mut opts = Map::new();
    opts.insert("x".into(), json!(x));
    opts.insert("y".into(), json!(y));
    opts.insert("frameId".into(), frame_id.map_or(Value::Null, |f| json!(f)));
    opts.insert("locked".into(), json!(false));
    if ty == ElementType::Frame {
        // FRAME_STYLE (constants.ts:206-215) over the current opacity
        opts.insert("opacity".into(), state(app_state, "currentItemOpacity"));
        opts.insert("strokeColor".into(), json!("#bbb"));
        opts.insert("strokeWidth".into(), json!(2));
        opts.insert("strokeStyle".into(), json!("solid"));
        opts.insert("fillStyle".into(), json!("solid"));
        opts.insert("roughness".into(), json!(0));
        opts.insert("roundness".into(), Value::Null);
        opts.insert("backgroundColor".into(), json!("transparent"));
        let mut m = new_element_base("frame", &opts, id, seed, timestamp);
        m.insert("name".into(), Value::Null);
        return Element::from_map(m).ok();
    }
    for (opt, key) in [
        ("strokeColor", "currentItemStrokeColor"),
        ("backgroundColor", "currentItemBackgroundColor"),
        ("fillStyle", "currentItemFillStyle"),
        ("strokeStyle", "currentItemStrokeStyle"),
        ("roughness", "currentItemRoughness"),
        ("opacity", "currentItemOpacity"),
    ] {
        opts.insert(opt.into(), state(app_state, key));
    }
    opts.insert(
        "strokeWidth".into(),
        json!(current_stroke_width(ty, app_state)),
    );
    let round = state_str(app_state, "currentItemRoundness") == Some("round");
    let m = match ty {
        ElementType::Rectangle
        | ElementType::Diamond
        | ElementType::Ellipse
        | ElementType::Selection => {
            // getCurrentItemRoundness, an ellipse included (its shape does
            // not draw it)
            let kind = round.then(|| {
                if is_using_adaptive_radius(tool) {
                    ADAPTIVE_RADIUS
                } else {
                    PROPORTIONAL_RADIUS
                }
            });
            opts.insert("roundness".into(), roundness(kind));
            new_element_base(tool, &opts, id, seed, timestamp)
        }
        ElementType::Arrow => {
            let arrow_type = state_str(app_state, "currentItemArrowType").unwrap_or("round");
            opts.insert(
                "roundness".into(),
                roundness((arrow_type == "round").then_some(PROPORTIONAL_RADIUS)),
            );
            let mut m = new_element_base("arrow", &opts, id, seed, timestamp);
            let arrowhead = |key: &str| match state(app_state, key) {
                Value::String(s) if !s.is_empty() => Value::String(s),
                _ => Value::Null,
            };
            m.insert("points".into(), json!([]));
            m.insert("startBinding".into(), Value::Null);
            m.insert("endBinding".into(), Value::Null);
            m.insert(
                "startArrowhead".into(),
                arrowhead("currentItemStartArrowhead"),
            );
            m.insert("endArrowhead".into(), arrowhead("currentItemEndArrowhead"));
            if arrow_type == "elbow" {
                m.insert("elbowed".into(), json!(true));
                m.insert("fixedSegments".into(), json!([]));
                m.insert("startIsSpecial".into(), json!(false));
                m.insert("endIsSpecial".into(), json!(false));
            } else {
                m.insert("elbowed".into(), json!(false));
            }
            m
        }
        ElementType::Line => {
            opts.insert(
                "roundness".into(),
                roundness(round.then_some(PROPORTIONAL_RADIUS)),
            );
            let mut m = new_element_base("line", &opts, id, seed, timestamp);
            m.insert("points".into(), json!([]));
            m.insert("startBinding".into(), Value::Null);
            m.insert("endBinding".into(), Value::Null);
            m.insert("startArrowhead".into(), Value::Null);
            m.insert("endArrowhead".into(), Value::Null);
            m.insert("polygon".into(), json!(false));
            m
        }
        ElementType::Freedraw => {
            opts.insert("roundness".into(), Value::Null);
            let mut m = new_element_base("freedraw", &opts, id, seed, timestamp);
            m.insert("points".into(), json!([[0, 0]]));
            m.insert("pressures".into(), json!([]));
            m.insert("simulatePressure".into(), json!(true));
            m.insert(
                "strokeOptions".into(),
                json!({
                    "variability": state(app_state, "currentItemStrokeVariability"),
                    "streamline": DEFAULT_STROKE_STREAMLINE,
                }),
            );
            m
        }
        _ => return None,
    };
    Element::from_map(m).ok()
}

/// `getPerfectElementSize(elementType, width, height)`
/// (`sizeHelpers.ts:158-185`): a line, arrow or freedraw locked to the
/// nearest 15 degree angle; any other shape but the selection box square.
pub fn get_perfect_element_size(element_type: &str, width: f64, height: f64) -> (f64, f64) {
    let abs_width = width.abs();
    let abs_height = height.abs();
    let (mut width, mut height) = (width, height);
    if matches!(element_type, "line" | "arrow" | "freedraw") {
        let locked_angle =
            js::round(js::atan(abs_height / abs_width) / SHIFT_LOCKING_ANGLE) * SHIFT_LOCKING_ANGLE;
        if locked_angle == 0.0 {
            height = 0.0;
        } else if locked_angle == std::f64::consts::FRAC_PI_2 {
            width = 0.0;
        } else {
            let h = abs_width * js::tan(locked_angle) * sign(height);
            // `|| height`: 0 and NaN fall back
            height = if h == 0.0 || h.is_nan() { height } else { h };
        }
    } else if element_type != "selection" {
        height = abs_width * sign(height);
    }
    (width, height)
}

/// `Math.sign`.
fn sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        x
    }
}

/// `dragNewElement`'s arguments.
#[derive(Debug, Clone, Copy)]
pub struct DragNewElement<'a> {
    pub element: &'a Element,
    /// `appState.activeTool.type`.
    pub element_type: &'a str,
    /// Where the drag started (`originX`, `originY`).
    pub origin: [f64; 2],
    /// The pointer (`x`, `y`).
    pub pointer: [f64; 2],
    pub width: f64,
    pub height: f64,
    pub maintain_aspect_ratio: bool,
    pub resize_from_center: bool,
    pub width_aspect_ratio: Option<f64>,
    /// `originSnapOffset`: where the pointer snapped before the press,
    /// added to the corner.
    pub origin_offset: Option<[f64; 2]>,
}

/// `dragNewElement({...})` (`dragElements.ts:294-406`) for an element that
/// is not text: its new `[x, y, width, height]`, or `None` when either
/// side is zero (upstream leaves the element).
pub fn drag_new_element(args: &DragNewElement<'_>) -> Option<[f64; 4]> {
    let [origin_x, origin_y] = args.origin;
    let [x, y] = args.pointer;
    let (mut width, mut height) = (args.width, args.height);
    if args.maintain_aspect_ratio && args.element.kind.element_type() != ElementType::Selection {
        match args.width_aspect_ratio.filter(|r| *r != 0.0 && !r.is_nan()) {
            Some(ratio) => height = width / ratio,
            None => {
                // the pointer sticks to one side of the box
                (width, height) = if (y - origin_y).abs() > (x - origin_x).abs() {
                    get_perfect_element_size(
                        args.element_type,
                        height,
                        if x < origin_x { -width } else { width },
                    )
                } else {
                    get_perfect_element_size(
                        args.element_type,
                        width,
                        if y < origin_y { -height } else { height },
                    )
                };
                if height < 0.0 {
                    height = -height;
                }
            }
        }
    }
    let mut new_x = if x < origin_x {
        origin_x - width
    } else {
        origin_x
    };
    let mut new_y = if y < origin_y {
        origin_y - height
    } else {
        origin_y
    };
    if args.resize_from_center {
        width += width;
        height += height;
        new_x = origin_x - width / 2.0;
        new_y = origin_y - height / 2.0;
    }
    let [dx, dy] = args.origin_offset.unwrap_or([0.0, 0.0]);
    (width != 0.0 && height != 0.0).then_some([new_x + dx, new_y + dy, width, height])
}

/// `MINIMUM_ARROW_SIZE` (`common/src/constants.ts:29`): a new line or
/// arrow released closer than this (screen pixels) to its press is not
/// drawn by dragging.
pub const MINIMUM_ARROW_SIZE: f64 = 20.0;

/// `getLockedLinearCursorAlignSize(originX, originY, x, y)`
/// (`sizeHelpers.ts:187-253`, without a custom angle): the offset from
/// the origin to the pointer projected on the nearest 15 degree line.
pub fn get_locked_linear_cursor_align_size(
    origin_x: f64,
    origin_y: f64,
    x: f64,
    y: f64,
) -> (f64, f64) {
    get_locked_linear_cursor_align_size_with_angle(origin_x, origin_y, x, y, None)
}

/// `getLockedLinearCursorAlignSize(originX, originY, x, y, customAngle)`
/// (`sizeHelpers.ts:187-253`): as [`get_locked_linear_cursor_align_size`],
/// but between the two 15 degree lines around a (truthy) `custom_angle`
/// the line's own angle is kept when the pointer is within 2.5 degrees of
/// it, else the nearer of the two.
pub fn get_locked_linear_cursor_align_size_with_angle(
    origin_x: f64,
    origin_y: f64,
    x: f64,
    y: f64,
    custom_angle: Option<f64>,
) -> (f64, f64) {
    use excali_math::{normalize_radians, radians_between_angles, radians_difference, Radians};
    let mut width = x - origin_x;
    let mut height = y - origin_y;
    let angle = js::atan2(height, width);
    let mut locked_angle = js::round(angle / SHIFT_LOCKING_ANGLE) * SHIFT_LOCKING_ANGLE;
    if let Some(custom) = custom_angle.filter(|a| *a != 0.0 && !a.is_nan()) {
        let lower = (custom / SHIFT_LOCKING_ANGLE).floor() * SHIFT_LOCKING_ANGLE;
        if radians_between_angles(
            Radians(angle),
            Radians(lower),
            Radians(lower + SHIFT_LOCKING_ANGLE),
        ) {
            if radians_difference(Radians(angle), Radians(custom)).0 < SHIFT_LOCKING_ANGLE / 6.0 {
                locked_angle = custom;
            } else if normalize_radians(Radians(angle)).0 > normalize_radians(Radians(custom)).0 {
                locked_angle = lower + SHIFT_LOCKING_ANGLE;
            } else {
                locked_angle = lower;
            }
        }
    }
    if locked_angle == 0.0 {
        height = 0.0;
    } else if locked_angle == std::f64::consts::FRAC_PI_2 {
        width = 0.0;
    } else {
        // the locked line y = mx + b, and the one through the cursor
        // perpendicular to it
        let a1 = js::tan(locked_angle);
        let b1 = -1.0;
        let c1 = origin_y - a1 * origin_x;
        let a2 = -1.0 / a1;
        let b2 = -1.0;
        let c2 = y - a2 * x;
        let intersect_x = (b1 * c2 - b2 * c1) / (a1 * b2 - a2 * b1);
        let intersect_y = (c1 * a2 - c2 * a1) / (a1 * b2 - a2 * b1);
        width = intersect_x - origin_x;
        height = intersect_y - origin_y;
    }
    (width, height)
}
