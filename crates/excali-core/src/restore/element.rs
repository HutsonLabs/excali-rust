//! `restoreElement` (`packages/excalidraw/data/restore.ts:517-752`): the
//! per-type rules on top of the base normalisation
//! ([`super::restore_encoded`]). Research page
//! `site/content/research/data-model.md`, section 3.4, "Per-type handling".
//!
//! Like the base, it works on the untyped object in the sentinel form of
//! [`crate::json`] and reproduces the JS semantics that decide the result.

use serde_json::{json, Map, Value};
use std::collections::HashMap;

use excali_math::{clamp, js as math};

use super::{restore_encoded, EscapingEnv, RestoreEnv, RestoreError};
use crate::color;
use crate::constants::{
    COLOR_BLACK, DEFAULT_FONT_SIZE, DEFAULT_STICKY_NOTE_BG, DEFAULT_STICKY_NOTE_SIZE,
    DEFAULT_STROKE_STREAMLINE, MIN_FONT_SIZE, STICKY_NOTE_FALLBACK_FONT_SIZE,
    STICKY_NOTE_MAX_FONT_SIZE, STICKY_NOTE_MIN_SIZE,
};
use crate::element::FontFamily;
use crate::js;
use crate::json;

/// `MAX_LINEAR_PX` (`restore.ts:126`): a line or arrow wider or taller than
/// this is replaced by a deleted 100 x 100 one (`restore.ts:128-157`,
/// upstream issue 11497: a huge dashed stroke froze the editor).
pub const MAX_LINEAR_PX: f64 = 75_000.0;

/// `PRECISION` (`packages/math/src/utils.ts:1`), the tolerance of
/// `pointsEqual`.
const PRECISION: f64 = 10e-5;

/// `restoreElement`'s `opts` (`restore.ts:524-526`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RestoreOptions {
    /// Mark a text element with empty text deleted, bumping its version
    /// (`restore.ts:583-589`).
    pub delete_invisible_elements: bool,
}

/// Which end of an arrow a binding is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindingEnd {
    Start,
    End,
}

/// What [`RestoreEnv::migrate_legacy_binding`] answers: the binding's new
/// `mode` and `fixedPoint` (`restore.ts:405-417`). Restore writes
/// `{mode, elementId, fixedPoint}`, `elementId` from the legacy binding.
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyBinding {
    pub mode: Value,
    pub fixed_point: Value,
}

/// A legacy binding to migrate (see [`RestoreEnv::migrate_legacy_binding`]).
#[derive(Debug, Clone, Copy)]
pub struct LegacyBindingRequest<'a> {
    /// The arrow as `repairBinding` gets it: the element as read, with its
    /// restored `points` and `x ?? 0`, `y ?? 0` (`restore.ts:669-674`).
    pub arrow: &'a Map<String, Value>,
    /// The legacy binding (`startBinding` or `endBinding`) as read.
    pub binding: &'a Value,
    /// The element `binding.elementId` names, as read.
    pub bound_element: &'a Map<String, Value>,
    /// The map it was found in: the elements being restored, or else the
    /// existing ones (`restore.ts:350-360`).
    pub elements: &'a ElementsMap,
    pub end: BindingEnd,
}

/// A JS `Map` key a JSON value can be (SameValueZero). Objects and arrays
/// are keys by identity, so no id read from another element finds one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum MapKey {
    Undefined,
    Null,
    Bool(bool),
    Number(u64),
    String(String),
}

impl MapKey {
    pub(crate) fn of(value: Option<&Value>) -> Option<MapKey> {
        Some(match value {
            None => MapKey::Undefined,
            Some(Value::Null) => MapKey::Null,
            Some(Value::Bool(b)) => MapKey::Bool(*b),
            Some(Value::Number(n)) => {
                let x = n.as_f64()?;
                // SameValueZero: -0 is 0.
                MapKey::Number(if x == 0.0 { 0.0f64 } else { x }.to_bits())
            }
            Some(Value::String(s)) => MapKey::String(s.clone()),
            Some(Value::Array(_) | Value::Object(_)) => return None,
        })
    }
}

/// `arrayToMap(elements)` (`packages/common/src/utils.ts`): elements by
/// `id`, the last element of an id winning, ids compared as JS `Map` keys
/// (`7` and `"7"` are different ids, an element without `id` is under
/// `undefined`). `restoreElement` looks bound elements up in it.
#[derive(Debug, Clone, Default)]
pub struct ElementsMap {
    /// Sentinel form.
    elements: Vec<Map<String, Value>>,
    index: HashMap<MapKey, usize>,
    /// Elements whose id is an object or an array: each its own key.
    unreachable: usize,
}

impl ElementsMap {
    /// The map of `elements`, as read from a file.
    pub fn new(elements: &[Map<String, Value>]) -> ElementsMap {
        ElementsMap::from_encoded(elements.iter().map(json::escape_map).collect())
    }

    /// [`ElementsMap::new`] for sentinel-form elements.
    pub(crate) fn from_encoded(elements: Vec<Map<String, Value>>) -> ElementsMap {
        let mut index = HashMap::new();
        let mut unreachable = 0;
        for (i, element) in elements.iter().enumerate() {
            match MapKey::of(element.get("id")) {
                Some(key) => {
                    index.insert(key, i);
                }
                None => unreachable += 1,
            }
        }
        ElementsMap {
            elements,
            index,
            unreachable,
        }
    }

    /// The elements, in the order given (every one, repeated ids included).
    pub(crate) fn elements(&self) -> &[Map<String, Value>] {
        &self.elements
    }

    /// `map.size`.
    pub fn len(&self) -> usize {
        self.index.len() + self.unreachable
    }

    /// `map.size === 0`.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// `map.get(id)`.
    pub fn get(&self, id: &Value) -> Option<Map<String, Value>> {
        self.get_encoded(Some(&json::escape(id)))
            .map(json::decode_map)
    }

    /// `map.get(key)` for a sentinel-form key; `None` is `undefined`.
    pub(crate) fn get_encoded(&self, key: Option<&Value>) -> Option<&Map<String, Value>> {
        let i = *self.index.get(&MapKey::of(key)?)?;
        self.elements.get(i)
    }
}

/// `restoreElement(element, targetElementsMap, existingElementsMap, opts)`
/// (`restore.ts:517-752`): the element with its type's rules applied, or
/// `None` for a type upstream does not restore (anything but `text`,
/// `freedraw`, `image`, `line`, legacy `draw`, `arrow`, `rectangle`,
/// `diamond`, `ellipse`, `iframe`, `embeddable`, `stickynote`, `frame`,
/// `magicframe`; `selection` included). `targets` holds the elements being
/// restored with it and `existing` those already in the scene; arrows look
/// the targets of their legacy bindings up in them.
///
/// - text: the Obsidian `rawText` key is removed; a legacy `font`
///   (`"20px Virgil"`) gives `fontSize` (`parseFloat` of its first word)
///   and `fontFamily` (its second word by exact name, else Excalifont); a
///   non-finite `fontSize` is 20; `lineHeight || (height ? height / lines
///   / fontSize : the lineHeight of fontFamily)`; `textAlign || "left"`,
///   `verticalAlign || "top"`, `containerId ?? null`, `originalText ||
///   text`, `autoResize ?? true`, `labelPosition` clamped to 0..=1 else
///   null, `baseFontSize` clamped to 1..=512 else null; with
///   `delete_invisible_elements`, empty text is deleted and its version
///   bumped;
/// - freedraw: invalid points dropped with their pressures, a non-finite
///   pressure 0.5, `strokeOptions` `{variability: "constant" | "variable"
///   (default), streamline: finite or 0.5}`;
/// - image: `status || "pending"`, `scale || [1, 1]`, `crop ?? null`;
/// - line and `draw`: `type` "line", arrowheads renamed (`dot` to
///   `circle`, `crowfoot_*` to `cardinality_*`), fewer than two valid
///   points become `[[0, 0], [width, height]]`, points re-based to start at
///   `[0, 0]` with `x`, `y` moved (JS `+`), bindings null, `polygon` kept
///   only for a closed `line` of more than 3 points, `width`/`height` from
///   the points;
/// - arrow: a missing `endArrowhead` is "arrow", bindings repaired (with a
///   `mode` kept and `fixedPoint` normalised, legacy ones migrated through
///   [`RestoreEnv::migrate_legacy_binding`]), elbow arrows keep
///   `fixedSegments` only when non-empty with at least 4 points, points
///   re-based;
/// - lines and arrows over [`MAX_LINEAR_PX`] are replaced by a deleted 100 x
///   100 one at the origin;
/// - stickynote: `baseHeight ?? maxHeight ?? height`, then colours never
///   transparent, fill solid, at least 75 x 75 and `height >= baseHeight`,
///   each change bumping the version;
/// - frame and magicframe: `name ?? null`.
///
/// Returns the object in JS property order, or the [`RestoreError`] for
/// input upstream throws on.
pub fn restore_element(
    element: &Map<String, Value>,
    targets: &ElementsMap,
    existing: Option<&ElementsMap>,
    opts: RestoreOptions,
    env: &mut dyn RestoreEnv,
) -> Result<Option<Map<String, Value>>, RestoreError> {
    let element = json::escape_map(element);
    let mut env = EscapingEnv(env);
    restore_element_encoded(&element, targets, existing, opts, &mut env)
        .map(|restored| restored.map(|m| json::decode_map(&m)))
}

/// [`restore_element`] on sentinel-form values.
pub(crate) fn restore_element_encoded(
    element: &Map<String, Value>,
    targets: &ElementsMap,
    existing: Option<&ElementsMap>,
    opts: RestoreOptions,
    env: &mut dyn RestoreEnv,
) -> Result<Option<Map<String, Value>>, RestoreError> {
    let Some(Value::String(ty)) = element.get("type") else {
        return Ok(None);
    };
    let restored = match ty.as_str() {
        "text" => text(element, opts, env)?,
        "freedraw" => freedraw(element, env)?,
        "image" => image(element, env)?,
        "line" | "draw" => line(element, env)?,
        "arrow" => arrow(element, targets, existing, env)?,
        "ellipse" | "rectangle" | "diamond" | "iframe" | "embeddable" => {
            restore_encoded(element, &[], env)?
        }
        "stickynote" => sticky_note(element, env)?,
        "magicframe" | "frame" => {
            let name = js::nullish_or(element.get("name"), || Some(Value::Null));
            restore_encoded(element, &[("name", name)], env)?
        }
        _ => return Ok(None),
    };
    Ok(Some(restored))
}

// -- text ---------------------------------------------------------------------------

/// `restore.ts:531-591`.
fn text(
    element: &Map<String, Value>,
    opts: RestoreOptions,
    env: &mut dyn RestoreEnv,
) -> Result<Map<String, Value>, RestoreError> {
    let mut el = element.clone();
    // Obsidian's legacy attribute.
    el.shift_remove("rawText");

    let mut font_size = el.get("fontSize").cloned();
    let mut font_family = el.get("fontFamily").cloned();
    if let Some(font) = el.get("font") {
        // `const [fontPx, _fontFamily] = element.font.split(" ")`
        let font = match font {
            Value::String(s) => s,
            Value::Null => return Err(RestoreError::FontNull),
            _ => return Err(RestoreError::FontNotString),
        };
        let mut words = font.split(' ');
        let px = words.next().unwrap_or("");
        font_size = Some(js::number(js::parse_float(px)));
        font_family = Some(font_family_by_name(words.next()));
    }
    let font_size = match js::finite_number(font_size.as_ref()) {
        Some(_) => font_size,
        None => Some(js::number(DEFAULT_FONT_SIZE)),
    };
    let text = match el.get("text") {
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        _ => String::new(),
    };
    // Old files without a line height get the one their height implies;
    // programmatic elements without a height the family's.
    let line_height = if js::truthy(el.get("lineHeight")) {
        el.get("lineHeight").cloned()
    } else if js::truthy(el.get("height")) {
        Some(detect_line_height(&el)?)
    } else {
        Some(js::number(family_line_height(el.get("fontFamily"))?))
    };
    let label_position = match js::finite_number(el.get("labelPosition")) {
        Some(x) => js::number(clamp(x, 0.0, 1.0)),
        None => Value::Null,
    };
    let base_font_size = match js::finite_number(el.get("baseFontSize")) {
        Some(x) => js::number(sticky_note_font_size(x)),
        None => Value::Null,
    };
    let text_value = Value::String(text.clone());
    let extra = [
        ("fontSize", font_size),
        ("fontFamily", font_family),
        ("text", Some(text_value.clone())),
        (
            "textAlign",
            js::or(el.get("textAlign"), || Some(json!("left"))),
        ),
        (
            "verticalAlign",
            js::or(el.get("verticalAlign"), || Some(json!("top"))),
        ),
        (
            "containerId",
            js::nullish_or(el.get("containerId"), || Some(Value::Null)),
        ),
        (
            "originalText",
            js::or(el.get("originalText"), || Some(text_value.clone())),
        ),
        (
            "autoResize",
            js::nullish_or(el.get("autoResize"), || Some(json!(true))),
        ),
        ("lineHeight", line_height),
        ("labelPosition", Some(label_position)),
        ("baseFontSize", Some(base_font_size)),
    ];
    let mut restored = restore_encoded(&el, &extra, env)?;

    // Empty text is kept in the array, deleted, for collaboration.
    if opts.delete_invisible_elements && text.is_empty() && !js::truthy(restored.get("isDeleted")) {
        restored.insert("originalText".to_owned(), text_value);
        restored.insert("isDeleted".to_owned(), json!(true));
        bump_version(&mut restored, env)?;
    }
    Ok(restored)
}

/// `getFontFamilyByName` (`restore.ts:289-296`): a `FONT_FAMILY` key,
/// matched exactly, else `DEFAULT_FONT_FAMILY`.
fn font_family_by_name(name: Option<&str>) -> Value {
    let family = name
        .and_then(|name| {
            FontFamily::ELEMENT_FAMILIES
                .into_iter()
                .find(|f| f.name() == Some(name))
        })
        .unwrap_or(FontFamily::DEFAULT);
    json!(family.0)
}

/// `getLineHeight(fontFamily)` (`packages/common/src/font-metadata.ts:175-181`):
/// `FONT_METADATA[fontFamily]?.metrics.lineHeight`, Excalifont's when the
/// family has no entry. The lookup is by property key, so `"2"` and `[2]`
/// find Helvetica like `2` does.
fn family_line_height(family: Option<&Value>) -> Result<f64, RestoreError> {
    let key = match family {
        None => "undefined".to_owned(),
        Some(v) => js::to_string(Some(v))?,
    };
    // FONT_METADATA, font-metadata.ts:35-148: Excalifont 5, Nunito 6,
    // Comic Shanns 8, Virgil 1, Assistant 10, Xiaolai 100 and Segoe UI
    // Emoji 1000 are 1.25; Lilita One 7, Helvetica 2 and Liberation Sans 9
    // are 1.15; Cascadia 3 is 1.2.
    Ok(match key.as_str() {
        "2" | "7" | "9" => 1.15,
        "3" => 1.2,
        _ => 1.25,
    })
}

/// `detectLineHeight(element)` (`packages/element/src/textMeasurements.ts:80-85`):
/// `height / lines / fontSize`, the line count after normalising line
/// ends (`\r\n` and `\r` are one break each).
fn detect_line_height(el: &Map<String, Value>) -> Result<Value, RestoreError> {
    let text = match el.get("text") {
        Some(Value::String(s)) => s,
        None => return Err(RestoreError::TextUndefined),
        Some(Value::Null) => return Err(RestoreError::TextNull),
        Some(_) => return Err(RestoreError::TextNotString),
    };
    let mut lines = 1.0;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\n' => lines += 1.0,
            '\r' => {
                lines += 1.0;
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
            }
            _ => {}
        }
    }
    let height = number_or_nan(el.get("height"))?;
    let font_size = number_or_nan(el.get("fontSize"))?;
    Ok(js::number(height / lines / font_size))
}

/// `normalizeStickyNoteFontSize` (`packages/element/src/stickyNote.ts:379-384`).
pub(super) fn sticky_note_font_size(size: f64) -> f64 {
    if !size.is_finite() {
        return STICKY_NOTE_FALLBACK_FONT_SIZE;
    }
    math::min(STICKY_NOTE_MAX_FONT_SIZE, math::max(MIN_FONT_SIZE, size))
}

// -- freedraw -----------------------------------------------------------------------

/// `restore.ts:592-604`.
fn freedraw(
    el: &Map<String, Value>,
    env: &mut dyn RestoreEnv,
) -> Result<Map<String, Value>, RestoreError> {
    let (points, pressures) = freedraw_points(el.get("points"), el.get("pressures"));
    let extra = [
        ("points", Some(points)),
        ("simulatePressure", el.get("simulatePressure").cloned()),
        (
            "strokeOptions",
            Some(freedraw_stroke_options(el.get("strokeOptions"))),
        ),
        ("pressures", Some(pressures)),
    ];
    restore_encoded(el, &extra, env)
}

/// `restoreFreedrawPoints` (`restore.ts:184-218`): valid points, and the
/// pressure at each kept point's index when there is one (non-finite ones
/// 0.5).
fn freedraw_points(points: Option<&Value>, pressures: Option<&Value>) -> (Value, Value) {
    let Some(Value::Array(points)) = points else {
        return (json!([]), json!([]));
    };
    let pressures: &[Value] = match pressures {
        Some(Value::Array(p)) => p,
        _ => &[],
    };
    let mut kept = Vec::new();
    let mut kept_pressures = Vec::new();
    for (i, point) in points.iter().enumerate() {
        if let Some(point) = valid_point(point) {
            kept.push(point_value(point));
            if let Some(pressure) = pressures.get(i) {
                kept_pressures.push(match js::finite_number(Some(pressure)) {
                    Some(_) => pressure.clone(),
                    None => json!(0.5),
                });
            }
        }
    }
    (Value::Array(kept), Value::Array(kept_pressures))
}

/// `restoreFreedrawStrokeOptions` (`restore.ts:273-287`).
fn freedraw_stroke_options(options: Option<&Value>) -> Value {
    // `strokeOptions && typeof strokeOptions === "object"`: an object or
    // an array.
    let options = options.filter(|o| o.is_object() || o.is_array());
    let field = |key: &str| options.and_then(|o| o.get(key));
    let variability = match field("variability") {
        Some(Value::String(s)) if s == "constant" || s == "variable" => json!(s),
        _ => json!("variable"),
    };
    let streamline = match js::finite_number(field("streamline")) {
        Some(_) => field("streamline").cloned().unwrap_or(Value::Null),
        None => js::number(DEFAULT_STROKE_STREAMLINE),
    };
    let mut out = Map::new();
    out.insert("variability".to_owned(), variability);
    out.insert("streamline".to_owned(), streamline);
    Value::Object(out)
}

// -- image --------------------------------------------------------------------------

/// `restore.ts:605-611`.
fn image(
    el: &Map<String, Value>,
    env: &mut dyn RestoreEnv,
) -> Result<Map<String, Value>, RestoreError> {
    let extra = [
        (
            "status",
            js::or(el.get("status"), || Some(json!("pending"))),
        ),
        ("fileId", el.get("fileId").cloned()),
        ("scale", js::or(el.get("scale"), || Some(json!([1, 1])))),
        ("crop", js::nullish_or(el.get("crop"), || Some(Value::Null))),
    ];
    restore_encoded(el, &extra, env)
}

// -- line / draw ---------------------------------------------------------------------

type Point = (f64, f64);

/// `isValidPoint` (`packages/math/src/point.ts:257-264`): a two-item array
/// of finite numbers.
fn valid_point(value: &Value) -> Option<Point> {
    match value.as_array()?.as_slice() {
        [x, y] => Some((js::finite_number(Some(x))?, js::finite_number(Some(y))?)),
        _ => None,
    }
}

fn point_value((x, y): Point) -> Value {
    json!([js::number(x), js::number(y)])
}

fn points_value(points: &[Point]) -> Value {
    Value::Array(points.iter().copied().map(point_value).collect())
}

/// `restoreLinearElementPoints` (`restore.ts:159-182`): the valid points,
/// or `[[0, 0], [width, height]]` (non-finite sizes 0) when fewer than two.
fn linear_points(
    points: Option<&Value>,
    width: Option<&Value>,
    height: Option<&Value>,
) -> Vec<Point> {
    let restored: Vec<Point> = match points {
        Some(Value::Array(points)) => points.iter().filter_map(valid_point).collect(),
        _ => Vec::new(),
    };
    if restored.len() < 2 {
        return vec![
            (0.0, 0.0),
            (
                js::finite_number(width).unwrap_or(0.0),
                js::finite_number(height).unwrap_or(0.0),
            ),
        ];
    }
    restored
}

/// `getNormalizedPoints` (`packages/element/src/linearElementEditor.ts:106-125`):
/// the points moved so the first is `[0, 0]`, and that offset.
fn rebased(points: &[Point]) -> (Vec<Point>, Point) {
    let (ox, oy) = points[0];
    (
        points.iter().map(|&(x, y)| (x - ox, y - oy)).collect(),
        (ox, oy),
    )
}

/// `getSizeFromPoints` (`packages/common/src/points.ts:10-19`).
fn size_from_points(points: &[Point]) -> (f64, f64) {
    let xs: Vec<f64> = points.iter().map(|p| p.0).collect();
    let ys: Vec<f64> = points.iter().map(|p| p.1).collect();
    (js_max(&xs) - js_min(&xs), js_max(&ys) - js_min(&ys))
}

/// `Math.max(...values)`.
fn js_max(values: &[f64]) -> f64 {
    values.iter().copied().fold(f64::NEG_INFINITY, math::max)
}

/// `Math.min(...values)`.
fn js_min(values: &[f64]) -> f64 {
    values.iter().copied().fold(f64::INFINITY, math::min)
}

/// `isValidPolygon` (`packages/element/src/typeChecks.ts:397-401`): more
/// than 3 points, the last equal to the first within `PRECISION`.
fn is_valid_polygon(points: &[Point]) -> bool {
    match (points.first(), points.last()) {
        (Some(a), Some(b)) if points.len() > 3 => {
            (a.0 - b.0).abs() < PRECISION && (a.1 - b.1).abs() < PRECISION
        }
        _ => false,
    }
}

/// `normalizeArrowhead` (`packages/element/src/arrowheads.ts:3-21`):
/// missing or null is null, the legacy names are renamed, anything else is
/// kept as it is.
fn normalize_arrowhead(arrowhead: Option<&Value>) -> Value {
    match arrowhead {
        None | Some(Value::Null) => Value::Null,
        Some(Value::String(s)) => json!(match s.as_str() {
            "dot" => "circle",
            "crowfoot_one" => "cardinality_one",
            "crowfoot_many" => "cardinality_many",
            "crowfoot_one_or_many" => "cardinality_one_or_many",
            other => other,
        }),
        Some(other) => other.clone(),
    }
}

/// `value + n` for a value that may be `undefined` (NaN).
pub(super) fn plus(value: Option<&Value>, n: f64) -> Result<Value, RestoreError> {
    match value {
        Some(v) => Ok(js::plus_number(v, n)?),
        None => Ok(js::number(f64::NAN)),
    }
}

/// `x ?? 0`.
fn or_zero(value: Option<&Value>) -> Value {
    js::nullish_or(value, || Some(json!(0))).unwrap_or(Value::Null)
}

/// `restore.ts:612-655`.
fn line(
    el: &Map<String, Value>,
    env: &mut dyn RestoreEnv,
) -> Result<Map<String, Value>, RestoreError> {
    let start_arrowhead = normalize_arrowhead(el.get("startArrowhead"));
    let end_arrowhead = normalize_arrowhead(el.get("endArrowhead"));
    let mut x = el.get("x").cloned();
    let mut y = el.get("y").cloned();
    let mut points = linear_points(el.get("points"), el.get("width"), el.get("height"));
    if points[0].0 != 0.0 || points[0].1 != 0.0 {
        let (moved, (ox, oy)) = rebased(&points);
        points = moved;
        x = Some(plus(Some(&or_zero(x.as_ref())), ox)?);
        y = Some(plus(Some(&or_zero(y.as_ref())), oy)?);
    }
    let (width, height) = size_from_points(&points);
    let mut extra = vec![
        ("type", Some(json!("line"))),
        ("startBinding", Some(Value::Null)),
        ("endBinding", Some(Value::Null)),
        ("startArrowhead", Some(start_arrowhead)),
        ("endArrowhead", Some(end_arrowhead)),
        ("points", Some(points_value(&points))),
        ("x", x),
        ("y", y),
    ];
    if el.get("type").and_then(Value::as_str) == Some("line") {
        let polygon = if is_valid_polygon(&points) {
            js::nullish_or(el.get("polygon"), || Some(json!(false)))
        } else {
            Some(json!(false))
        };
        extra.push(("polygon", polygon));
    }
    extra.push(("width", Some(js::number(width))));
    extra.push(("height", Some(js::number(height))));
    let restored = restore_encoded(el, &extra, env)?;
    oversized(restored, width, height)
}

/// `handleOversizedLinearElements` (`restore.ts:132-157`). `width` and
/// `height` are the ones from the points, as numbers (a JSON value cannot
/// hold the infinite size of an overflowing span).
fn oversized(
    mut el: Map<String, Value>,
    width: f64,
    height: f64,
) -> Result<Map<String, Value>, RestoreError> {
    if width <= MAX_LINEAR_PX && height <= MAX_LINEAR_PX {
        return Ok(el);
    }
    // The console.error message interpolates these with ToString.
    for key in ["id", "width", "height", "x", "y"] {
        if let Some(value) = el.get(key) {
            js::to_string(Some(value))?;
        }
    }
    el.insert("x".to_owned(), json!(0));
    el.insert("y".to_owned(), json!(0));
    el.insert("width".to_owned(), json!(100));
    el.insert("height".to_owned(), json!(100));
    el.insert("points".to_owned(), json!([[0, 0], [100, 100]]));
    el.insert("isDeleted".to_owned(), json!(true));
    Ok(el)
}

// -- arrow ---------------------------------------------------------------------------

/// `restore.ts:656-723`.
fn arrow(
    el: &Map<String, Value>,
    targets: &ElementsMap,
    existing: Option<&ElementsMap>,
    env: &mut dyn RestoreEnv,
) -> Result<Map<String, Value>, RestoreError> {
    let start_arrowhead = normalize_arrowhead(el.get("startArrowhead"));
    let end_arrowhead = match el.get("endArrowhead") {
        None => json!("arrow"),
        some => normalize_arrowhead(some),
    };
    let points = linear_points(el.get("points"), el.get("width"), el.get("height"));
    let x = or_zero(el.get("x"));
    let y = or_zero(el.get("y"));
    let mut with_points = el.clone();
    with_points.insert("points".to_owned(), points_value(&points));
    with_points.insert("x".to_owned(), x.clone());
    with_points.insert("y".to_owned(), y.clone());

    let mut binding = |key: &str, end: BindingEnd| {
        repair_binding(&with_points, el.get(key), targets, existing, end, env)
    };
    let start_binding = binding("startBinding", BindingEnd::Start);
    let end_binding = binding("endBinding", BindingEnd::End);

    // isElbowArrow: type "arrow" and a truthy `elbowed`.
    let elbow = js::truthy(el.get("elbowed"));
    let (width, height) = size_from_points(&points);
    let mut extra = vec![
        ("type", el.get("type").cloned()),
        ("startBinding", Some(start_binding)),
        ("endBinding", Some(end_binding)),
        ("startArrowhead", Some(start_arrowhead)),
        ("endArrowhead", Some(end_arrowhead)),
        ("points", Some(points_value(&points))),
        ("x", Some(x)),
        ("y", Some(y)),
        (
            "elbowed",
            if elbow {
                Some(json!(true))
            } else {
                el.get("elbowed").cloned()
            },
        ),
        ("width", Some(js::number(width))),
        ("height", Some(js::number(height))),
    ];
    if elbow {
        let keep = js::truthy(length(el.get("fixedSegments")).as_ref()) && points.len() >= 4;
        let fixed_segments = if keep {
            el.get("fixedSegments").cloned()
        } else {
            Some(Value::Null)
        };
        extra.push(("fixedSegments", fixed_segments));
        extra.push(("startIsSpecial", el.get("startIsSpecial").cloned()));
        extra.push(("endIsSpecial", el.get("endIsSpecial").cloned()));
    }
    let mut restored = restore_encoded(el, &extra, env)?;

    // getNormalizeElementPointsAndCoords on the restored arrow.
    let (points, (ox, oy)) = rebased(&points);
    let x = plus(restored.get("x"), ox)?;
    let y = plus(restored.get("y"), oy)?;
    restored.insert("points".to_owned(), points_value(&points));
    restored.insert("x".to_owned(), x);
    restored.insert("y".to_owned(), y);
    oversized(restored, width, height)
}

/// `value?.length`: a string's UTF-16 length, an array's length, an
/// object's `length` key; `undefined` for everything else.
fn length(value: Option<&Value>) -> Option<Value> {
    match value? {
        Value::String(s) => Some(json!(json::to_utf16(s).len())),
        Value::Array(items) => Some(json!(items.len())),
        Value::Object(map) => map.get("length").cloned(),
        _ => None,
    }
}

/// `normalizeFixedPoint` (`packages/element/src/binding.ts:2752-2778`):
/// anything but two finite numbers is `[0.5001, 0.5001]`; ratios clamped
/// to -10..=10; if either is within 0.0001 of 0.5, each such one becomes
/// 0.5001.
fn normalize_fixed_point(fixed_point: Option<&Value>) -> Value {
    const BOUND: f64 = 10.0;
    const EPSILON: f64 = 0.0001;
    let point = fixed_point
        .and_then(Value::as_array)
        .and_then(|a| valid_point(&Value::Array(a.clone())));
    let Some((x, y)) = point else {
        return json!([0.5001, 0.5001]);
    };
    let clamped = [clamp(x, -BOUND, BOUND), clamp(y, -BOUND, BOUND)];
    let near_half = |r: f64| (r - 0.5).abs() < EPSILON;
    let ratios = if clamped.iter().any(|&r| near_half(r)) {
        clamped.map(|r| if near_half(r) { 0.5001 } else { r })
    } else {
        clamped
    };
    json!([js::number(ratios[0]), js::number(ratios[1])])
}

/// `{...value}`: an object's own keys, an array's or a string's indices
/// (a string by UTF-16 code unit), nothing for other primitives.
fn spread(value: &Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map.clone(),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(i, v)| (i.to_string(), v.clone()))
            .collect(),
        Value::String(s) => json::to_utf16(s)
            .iter()
            .enumerate()
            .map(|(i, &unit)| (i.to_string(), Value::String(json::from_utf16(&[unit]))))
            .collect(),
        _ => Map::new(),
    }
}

/// `repairBinding` (`restore.ts:298-428`) for a binding as read.
fn repair_binding(
    arrow: &Map<String, Value>,
    binding: Option<&Value>,
    targets: &ElementsMap,
    existing: Option<&ElementsMap>,
    end: BindingEnd,
    env: &mut dyn RestoreEnv,
) -> Value {
    let Some(binding) = binding.filter(|b| js::truthy(Some(b))) else {
        return Value::Null;
    };
    // A property of the binding; primitives have none of these.
    let field = |key: &str| binding.as_object().and_then(|m| m.get(key));

    // Elbow arrows: the binding spread, fixedPoint normalised, mode
    // defaulting to "orbit".
    if js::truthy(arrow.get("elbowed")) {
        let mut repaired = spread(binding);
        repaired.insert(
            "fixedPoint".to_owned(),
            normalize_fixed_point(field("fixedPoint")),
        );
        repaired.insert(
            "mode".to_owned(),
            js::or(field("mode"), || Some(json!("orbit"))).unwrap_or(Value::Null),
        );
        return Value::Object(json::ordered_like_js(repaired));
    }

    // Binding schema v2: kept if it names an element.
    if js::truthy(field("mode")) {
        if !js::truthy(field("elementId")) {
            return Value::Null;
        }
        let mut repaired = Map::new();
        repaired.insert(
            "elementId".to_owned(),
            field("elementId").cloned().unwrap_or(Value::Null),
        );
        repaired.insert(
            "mode".to_owned(),
            field("mode").cloned().unwrap_or(Value::Null),
        );
        repaired.insert(
            "fixedPoint".to_owned(),
            normalize_fixed_point(field("fixedPoint")),
        );
        return Value::Object(repaired);
    }

    // Binding schema v1 (legacy): migrated when its element exists.
    let element_id = field("elementId");
    let found = targets
        .get_encoded(element_id)
        .map(|e| (e, targets))
        .or_else(|| existing.and_then(|m| m.get_encoded(element_id).map(|e| (e, m))));
    let Some((bound_element, elements)) = found else {
        return Value::Null;
    };
    let Some(migrated) = env.migrate_legacy_binding(LegacyBindingRequest {
        arrow,
        binding,
        bound_element,
        elements,
        end,
    }) else {
        return Value::Null;
    };
    let mut repaired = Map::new();
    repaired.insert("mode".to_owned(), migrated.mode);
    if let Some(id) = element_id {
        repaired.insert("elementId".to_owned(), id.clone());
    }
    repaired.insert("fixedPoint".to_owned(), migrated.fixed_point);
    Value::Object(repaired)
}

// -- sticky note ---------------------------------------------------------------------

/// `restore.ts:732-740`.
fn sticky_note(
    el: &Map<String, Value>,
    env: &mut dyn RestoreEnv,
) -> Result<Map<String, Value>, RestoreError> {
    let base_height = js::nullish_or(el.get("baseHeight"), || {
        js::nullish_or(el.get("maxHeight"), || el.get("height").cloned())
    });
    let restored = restore_encoded(el, &[("baseHeight", base_height)], env)?;
    let styled = sticky_note_style(restored, env)?;
    sticky_note_geometry(styled, env)
}

/// `normalizeStickyNoteStyle` (`packages/element/src/newElement.ts:185-199`,
/// `stickyNote.ts:67-81`): a missing or transparent background is the
/// default sticky yellow, a missing or transparent stroke black, the fill
/// solid.
fn sticky_note_style(
    el: Map<String, Value>,
    env: &mut dyn RestoreEnv,
) -> Result<Map<String, Value>, RestoreError> {
    let color = |value: Option<&Value>, default: &str| -> Result<Value, RestoreError> {
        match value {
            Some(v) if js::truthy(Some(v)) && color::alpha_of_value(Some(v))? != 0.0 => {
                Ok(v.clone())
            }
            _ => Ok(json!(default)),
        }
    };
    let background = color(el.get("backgroundColor"), DEFAULT_STICKY_NOTE_BG)?;
    let stroke = color(el.get("strokeColor"), COLOR_BLACK)?;
    new_element_with(
        el,
        vec![
            ("backgroundColor", Update::Value(background)),
            ("strokeColor", Update::Value(stroke)),
            ("fillStyle", Update::Value(json!("solid"))),
        ],
        env,
    )
}

/// `normalizeStickyNoteGeometry` (`packages/element/src/newElement.ts:201-221`):
/// at least 75 x 75, `baseHeight` from `baseHeight || height || 250` (at
/// least 75), `height` at least `baseHeight`.
fn sticky_note_geometry(
    el: Map<String, Value>,
    env: &mut dyn RestoreEnv,
) -> Result<Map<String, Value>, RestoreError> {
    let width = math::max(number_or_nan(el.get("width"))?, STICKY_NOTE_MIN_SIZE);
    let base = js::or(el.get("baseHeight"), || {
        js::or(el.get("height"), || {
            Some(js::number(DEFAULT_STICKY_NOTE_SIZE))
        })
    });
    let base_height = math::max(number_or_nan(base.as_ref())?, STICKY_NOTE_MIN_SIZE);
    let height = math::max(number_or_nan(el.get("height"))?, base_height);
    new_element_with(
        el,
        vec![
            ("width", Update::Number(width)),
            ("height", Update::Number(height)),
            ("baseHeight", Update::Number(base_height)),
        ],
        env,
    )
}

/// A value `newElementWith` sets: a JSON value, or a number computed here
/// (kept as an `f64` so NaN compares as JS does).
enum Update {
    Value(Value),
    Number(f64),
}

/// `newElementWith(element, updates)` (`packages/element/src/mutateElement.ts:149-181`):
/// unchanged when every update is `===` the current value and not an
/// object; otherwise the updates applied and the version bumped
/// (`version + 1`, a fresh `versionNonce`, `updated` now).
fn new_element_with(
    mut el: Map<String, Value>,
    updates: Vec<(&str, Update)>,
    env: &mut dyn RestoreEnv,
) -> Result<Map<String, Value>, RestoreError> {
    let unchanged = updates.iter().all(|(key, update)| match update {
        Update::Number(x) => el.get(*key).and_then(Value::as_f64) == Some(*x),
        Update::Value(Value::Object(_) | Value::Array(_)) => false,
        Update::Value(v) => js::strictly_equal(el.get(*key), Some(v)),
    });
    if unchanged {
        return Ok(el);
    }
    for (key, update) in updates {
        let value = match update {
            Update::Value(v) => v,
            Update::Number(x) => js::number(x),
        };
        el.insert(key.to_owned(), value);
    }
    bump_version(&mut el, env)?;
    Ok(el)
}

/// `version + 1` (JS `+`), a fresh `versionNonce` and `updated` now, as
/// `bumpVersion` and `newElementWith` do (`mutateElement.ts:174-196`).
pub(super) fn bump_version(
    el: &mut Map<String, Value>,
    env: &mut dyn RestoreEnv,
) -> Result<(), RestoreError> {
    let version = plus(el.get("version"), 1.0)?;
    el.insert("version".to_owned(), version);
    el.insert("versionNonce".to_owned(), js::number(env.random_integer()));
    el.insert("updated".to_owned(), js::number(env.now()));
    Ok(())
}

/// `ToNumber` of a value that may be `undefined` (NaN).
fn number_or_nan(value: Option<&Value>) -> Result<f64, RestoreError> {
    match value {
        Some(v) => Ok(js::to_number(Some(v))?),
        None => Ok(f64::NAN),
    }
}
