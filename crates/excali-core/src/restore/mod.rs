//! Restore: turning whatever a file holds into elements the model accepts,
//! the way upstream's `packages/excalidraw/data/restore.ts` does at the
//! pinned commit (research page `site/content/research/data-model.md`,
//! section 3.4; `site/content/architecture/file-format.md`, "Restore
//! rules").
//!
//! Restore works on the untyped element object, as upstream does: its input
//! is what `JSON.parse` gave, of any shape, and its output is the object
//! the typed model ([`crate::element::Element::from_map`]) then reads. The
//! JS semantics that decide the result (`||` against `??`, `ToNumber`
//! coercion, object spread order, a method call on the wrong type
//! throwing) are reproduced on [`serde_json::Value`]s.
//!
//! This module holds the base normalisation every element goes through,
//! [`restore_element_with_properties`] (`restoreElementWithProperties`,
//! `restore.ts:430-515`), and the helpers it uses: [`normalize_link`] and
//! [`sanitize_url`] (`packages/common/src/url.ts:5-11`,
//! `@braintree/sanitize-url` 6.0.2) and [`normalized_dimensions`]
//! (`getNormalizedDimensions`, `packages/element/src/sizeHelpers.ts:256-283`);
//! and the per-type rules on top of it, [`restore_element`]
//! (`restoreElement`, `restore.ts:517-752`), which reads sticky note
//! colours with [`crate::color`] (`isTransparent`,
//! `packages/common/src/colors.ts:389-391`); and the scene-level passes on
//! top of that, [`restore_elements`] (`restoreElements`,
//! `restore.ts:946-1138`: duplicate ids, invisibly small elements,
//! fractional indices, and with `repair_bindings` frames, bound text,
//! linear bindings, sticky notes, bound text order and elbow arrows) and
//! [`bump_element_versions`] (`bumpElementVersions`, `restore.ts:1150-1173`).
//!
//! Where upstream needs code the crate table places above `excali-core`
//! (element geometry, the elbow arrow router, text measurement), restore
//! asks its [`RestoreEnv`].

mod element;
mod scene;
mod url;

#[cfg(test)]
mod element_tests;
#[cfg(test)]
mod scene_tests;
#[cfg(test)]
mod tests;

use serde_json::{json, Map, Value};
use std::fmt;

use crate::constants::DEFAULT_ELEMENT_PROPS;
use crate::element::ElementType;
use crate::js;
use crate::json;

pub use element::{
    restore_element, BindingEnd, ElementsMap, LegacyBinding, LegacyBindingRequest, RestoreOptions,
    MAX_LINEAR_PX,
};
pub(crate) use element::{restore_element_encoded, MapKey};
pub(crate) use scene::restore_elements_sentinel;
pub use scene::{
    bump_element_versions, restore_elements, ElbowArrowRequest, RestoreElementsError,
    RestoreElementsOptions, StickyNoteLayout, StickyNoteLayoutRequest, TextDimensionsRequest,
};
#[cfg(test)]
use scene::{restore_elements_encoded, SceneCall};

/// Where restore gets what upstream draws from global state, and the
/// element geometry it needs for one migration.
pub trait RestoreEnv {
    /// `getUpdatedTimestamp()` (`packages/common/src/utils.ts:552`): epoch
    /// milliseconds, `Date.now()` upstream.
    fn now(&mut self) -> f64;
    /// `randomId()` (`packages/common/src/random.ts:16`): a fresh element
    /// id, a 21-character nanoid upstream.
    fn random_id(&mut self) -> String;
    /// `randomInteger()` (`packages/common/src/random.ts:9`): an integer in
    /// `0..2^31`, `Math.floor(random.next() * 2 ** 31)` over roughjs'
    /// `Random` upstream. Restore draws one for each `versionNonce` it
    /// bumps.
    fn random_integer(&mut self) -> f64;
    /// The migration of a legacy arrow binding, one without `mode`, whose
    /// target exists (`repairBinding`, `restore.ts:362-418`): the binding
    /// point's global position decides `mode` (`inside` when it lies in the
    /// target, else `orbit`) and the `fixedPoint` is computed against the
    /// target. That needs element shapes, bounds and hit testing
    /// (`LinearElementEditor.getPointAtIndexGlobalCoordinates`,
    /// `isPointInElement`, `projectFixedPointOntoDiagonal`,
    /// `calculateFixedPointForNonElbowArrowBinding`), which the crate table
    /// of `site/content/architecture/overview.md` places in `excali-editor`
    /// (`excali-core` may only use `excali-math`), so the environment
    /// supplies them.
    ///
    /// `None` is what upstream gives when that computation throws: the
    /// binding is dropped (`restore.ts:423-427`). The default has no
    /// geometry and answers `None`, so until `excali-editor` implements this
    /// (task ex-116, which milestone M1 depends on) a legacy file's arrow
    /// bindings to existing targets are dropped on load where upstream
    /// keeps them.
    fn migrate_legacy_binding(
        &mut self,
        request: LegacyBindingRequest<'_>,
    ) -> Option<LegacyBinding> {
        let _ = request;
        None
    }

    /// `refreshTextDimensions(text, container, elementsMap)`
    /// (`packages/element/src/newElement.ts:533-...`), which
    /// `restoreElements` calls with `refreshDimensions` (`restore.ts:1032-1045`):
    /// the keys to assign to the text (`text`, `x`, `y`, `width`, `height`,
    /// and `autoResize` when it starts wrapping), re-wrapped and measured
    /// for its font and container. `None` is upstream's `undefined` (a
    /// deleted text) and leaves the text as it is.
    ///
    /// Text wrapping and measurement belong to `excali-text` (crate table of
    /// `site/content/architecture/overview.md`; `excali-core` may only use
    /// `excali-math`), so the environment supplies them. The default has no
    /// text measurement and answers `None`. Upstream's own callers never
    /// pass `refreshDimensions` (file loading and the initial scene pass
    /// only repair bindings).
    fn refresh_text_dimensions(
        &mut self,
        request: TextDimensionsRequest<'_>,
    ) -> Option<Map<String, Value>> {
        let _ = request;
        None
    }

    /// `getStickyNoteLayout(note, label)`
    /// (`packages/element/src/stickyNote.ts:669-...`), which
    /// `restoreElements` calls for each sticky note with
    /// `refreshDimensions` (`restore.ts:931-941`): the keys to assign to
    /// the note and to its label, the label wrapped and its font fitted.
    /// Like [`RestoreEnv::refresh_text_dimensions`] it measures text, so
    /// the environment supplies it; the default answers `None`, which
    /// leaves both as they are.
    fn sticky_note_layout(
        &mut self,
        request: StickyNoteLayoutRequest<'_>,
    ) -> Option<StickyNoteLayout> {
        let _ = request;
        None
    }

    /// `updateElbowArrowPoints(arrow, elementsMap, {points})`
    /// (`packages/element/src/elbowArrow.ts:907-...`), which
    /// `restoreElements` calls with `repairBindings` for an unbound elbow
    /// arrow whose segments are not all axis-aligned (`restore.ts:1076-1093`):
    /// the keys to assign to the arrow (`points`, `x`, `y`, `width`,
    /// `height`, `fixedSegments`, `startIsSpecial`, `endIsSpecial`), the
    /// arrow re-routed between `[0, 0]` and its last point. Its `index` is
    /// kept whatever the answer holds.
    ///
    /// The router (A* over a grid, task ex-211) belongs to `excali-editor`
    /// (crate table of `site/content/architecture/overview.md`), so the
    /// environment supplies it. The default answers `None`, which keeps the
    /// arrow as restored.
    fn update_elbow_arrow_points(
        &mut self,
        request: ElbowArrowRequest<'_>,
    ) -> Option<Map<String, Value>> {
        let _ = request;
        None
    }
}

/// Upstream's test mode (`isTestEnv()`) after `reseed(seed)`
/// (`packages/common/src/random.ts`): ids are `id0`, `id1`, ..., the
/// timestamp is always 1, and `randomInteger()` follows roughjs'
/// `Random(seed)` (Park-Miller, multiplier 48271). The default is
/// `reseed(1)`, what `tools/goldens/restore-fixtures.mjs` does before each
/// case, so restoring with it gives exactly what the fixtures record. It
/// has no geometry for [`RestoreEnv::migrate_legacy_binding`].
#[derive(Debug, Clone)]
pub struct TestEnv {
    next_id: u64,
    seed: i32,
}

impl TestEnv {
    /// Upstream's test mode after `reseed(seed)`. Seed 0 is roughjs'
    /// `Math.random` fallback, which has no fixed sequence; it is taken as 1.
    pub fn with_seed(seed: i32) -> TestEnv {
        TestEnv {
            next_id: 0,
            seed: if seed == 0 { 1 } else { seed },
        }
    }
}

impl Default for TestEnv {
    fn default() -> TestEnv {
        TestEnv::with_seed(1)
    }
}

impl RestoreEnv for TestEnv {
    fn now(&mut self) -> f64 {
        1.0
    }

    fn random_id(&mut self) -> String {
        let id = format!("id{}", self.next_id);
        self.next_id += 1;
        id
    }

    fn random_integer(&mut self) -> f64 {
        // roughjs/bin/math.js: seed = Math.imul(48271, seed), then
        // ((2 ** 31 - 1) & seed) / 2 ** 31; randomInteger floors that times
        // 2 ** 31, which is the masked seed itself.
        self.seed = self.seed.wrapping_mul(48271);
        f64::from(self.seed & 0x7FFF_FFFF)
    }
}

/// Upstream throws while restoring the element, and `restoreElements`
/// drops it (`restore.ts:977-979`). The message is V8's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreError {
    /// A truthy `boundElementIds` that is not an array: `.map` is called
    /// on it (`restore.ts:486-487`).
    BoundElementIdsNotArray,
    /// A truthy `link` that is not a string: `normalizeLink` calls
    /// `.trim()` on it (`restore.ts:491`, `url.ts:6`).
    LinkNotString,
    /// A `width`, `height`, `x` or `y` that `getNormalizedDimensions`
    /// compares or subtracts is an object (or an array holding one) with an
    /// own `toString` key: `ToPrimitive` finds no callable method
    /// (`sizeHelpers.ts:271-281`).
    NoPrimitiveValue,
    /// A text element's legacy `font` is `null` (`restore.ts:539-541`).
    FontNull,
    /// A text element's legacy `font` is neither a string nor `null`.
    FontNotString,
    /// A text element's line height is detected from its height and its
    /// `text` is missing (`detectLineHeight`, `textMeasurements.ts:80-85`,
    /// calls `.replace` on it).
    TextUndefined,
    /// As [`RestoreError::TextUndefined`] with `text` `null`.
    TextNull,
    /// As [`RestoreError::TextUndefined`] with `text` neither a string nor
    /// `null`.
    TextNotString,
    /// A sticky note colour object with its own `hasOwnProperty` key, which
    /// tinycolor calls (`isTransparent`).
    HasOwnPropertyNotFunction,
}

impl fmt::Display for RestoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            RestoreError::BoundElementIdsNotArray => {
                "element.boundElementIds.map is not a function"
            }
            RestoreError::LinkNotString => "link.trim is not a function",
            RestoreError::NoPrimitiveValue => "Cannot convert object to primitive value",
            RestoreError::FontNull => "Cannot read properties of null (reading 'split')",
            RestoreError::FontNotString => {
                "element.font.split is not a function or its return value is not iterable"
            }
            RestoreError::TextUndefined => {
                "Cannot read properties of undefined (reading 'replace')"
            }
            RestoreError::TextNull => "Cannot read properties of null (reading 'replace')",
            RestoreError::TextNotString => "str.replace is not a function",
            RestoreError::HasOwnPropertyNotFunction => "color.hasOwnProperty is not a function",
        })
    }
}

impl From<js::TypeError> for RestoreError {
    /// The `TypeError`s the shared JS conversions ([`crate::js`]) and the
    /// tinycolor object reader ([`crate::color`]) raise on a JSON value:
    /// `hasOwnProperty` shadowed by an own key, or else `ToPrimitive`
    /// finding no callable method.
    fn from(err: js::TypeError) -> RestoreError {
        if err.0 == RestoreError::HasOwnPropertyNotFunction.to_string() {
            RestoreError::HasOwnPropertyNotFunction
        } else {
            RestoreError::NoPrimitiveValue
        }
    }
}

impl std::error::Error for RestoreError {}

/// `restoreElementWithProperties(element, extra)` (`restore.ts:430-515`).
///
/// Every base field gets upstream's default when missing: fields read with
/// `||` (`version`, `id`, `fillStyle`, `strokeWidth`, `angle`,
/// `strokeColor`, `backgroundColor`, `width`, `height`, `link`) also
/// replace `0`, `""` and `false`; the others (`??`, and `opacity == null`)
/// only `null`. Present values are kept as they are, whatever their type.
/// Then:
///
/// - `roundness`: when not truthy and legacy `strokeSharpness` is
///   `"round"`, `{type: 1}` (legacy) for the adaptive-radius types and
///   `{type: 2}` (proportional) for all others, by the element's own `type`
///   (`restore.ts:475-485`), else `null`;
/// - `boundElements`: legacy `boundElementIds` become
///   `[{type: "arrow", id}]`, else `boundElements ?? []`
///   (`restore.ts:486-488`);
/// - `link`: through [`normalize_link`];
/// - `customData`: from `extra` if it has the key, else from the element if
///   it has it, else absent (`restore.ts:495-498`);
/// - a negative `width` or `height` is flipped and `x` or `y` moved by it
///   ([`normalized_dimensions`]);
/// - the result is `{...element, ...base, ...dimensions, ...extra}`: keys
///   the element had keep their place (unknown ones included), missing base
///   keys are appended in upstream's order, then new `extra` keys;
///   `strokeSharpness` and `boundElementIds` are deleted
///   (`restore.ts:500-512`).
///
/// `extra` is the per-type part, as `(key, value)` in order; `None` is a
/// key set to `undefined`, which removes it from the result. `extra`'s
/// `type`, `x`, `y` and `customData` also feed the base fields, as
/// upstream's `extra.type || element.type` and `extra.x ?? element.x`.
///
/// Returns the object in JS property order, or the [`RestoreError`] for
/// input upstream throws on.
pub fn restore_element_with_properties(
    element: &Map<String, Value>,
    extra: &[(&str, Option<Value>)],
    env: &mut dyn RestoreEnv,
) -> Result<Map<String, Value>, RestoreError> {
    let element = json::escape_map(element);
    let extra: Vec<(&str, Option<Value>)> = extra
        .iter()
        .map(|(k, v)| (*k, v.as_ref().map(json::escape)))
        .collect();
    let mut env = EscapingEnv(env);
    restore_encoded(&element, &extra, &mut env).map(|m| json::decode_map(&m))
}

/// `normalizeLink(link)` (`packages/common/src/url.ts:5-11`): trimmed
/// (ECMAScript whitespace); empty stays empty; otherwise double quotes
/// escaped as `&quot;` and the result passed through [`sanitize_url`].
pub fn normalize_link(link: &str) -> String {
    json::decode_str(&url::normalize_link_encoded(&json::escape_str(link))).into_owned()
}

/// `sanitizeUrl(url)` from `@braintree/sanitize-url` 6.0.2: HTML numeric
/// entities decoded, `&newline;`/`&tab;` and control characters removed,
/// trimmed; empty becomes `about:blank`, and so does a URL whose scheme is
/// `javascript:`, `data:` or `vbscript:` (any case, after leading
/// non-word characters, with `:` or `&colon;`). Relative URLs (`.` or `/`
/// first) and every other URL are returned as sanitised.
pub fn sanitize_url(url: &str) -> String {
    json::decode_str(&url::sanitize_url_encoded(&json::escape_str(url))).into_owned()
}

/// `getNormalizedDimensions` (`packages/element/src/sizeHelpers.ts:256-283`)
/// for numbers: a negative width becomes its absolute value and `x` moves
/// left by it, likewise height and `y`. Returns `(x, y, width, height)`.
pub fn normalized_dimensions(x: f64, y: f64, width: f64, height: f64) -> (f64, f64, f64, f64) {
    let (x, width) = normalized_axis(x, width);
    let (y, height) = normalized_axis(y, height);
    (x, y, width, height)
}

fn normalized_axis(position: f64, size: f64) -> (f64, f64) {
    if size < 0.0 {
        let next = size.abs();
        (position - next, next)
    } else {
        (position, size)
    }
}

/// A [`RestoreEnv`] whose ids are put in the sentinel form, and whose
/// legacy binding migration sees public values.
pub(crate) struct EscapingEnv<'a>(pub(crate) &'a mut dyn RestoreEnv);

impl RestoreEnv for EscapingEnv<'_> {
    fn now(&mut self) -> f64 {
        self.0.now()
    }

    fn random_id(&mut self) -> String {
        json::escape_str(&self.0.random_id()).into_owned()
    }

    fn random_integer(&mut self) -> f64 {
        self.0.random_integer()
    }

    fn migrate_legacy_binding(
        &mut self,
        request: LegacyBindingRequest<'_>,
    ) -> Option<LegacyBinding> {
        let arrow = json::decode_map(request.arrow);
        let binding = json::decode(request.binding);
        let bound_element = json::decode_map(request.bound_element);
        let answer = self.0.migrate_legacy_binding(LegacyBindingRequest {
            arrow: &arrow,
            binding: &binding,
            bound_element: &bound_element,
            elements: request.elements,
            end: request.end,
        })?;
        Some(LegacyBinding {
            mode: json::escape(&answer.mode),
            fixed_point: json::escape(&answer.fixed_point),
        })
    }

    fn refresh_text_dimensions(
        &mut self,
        request: TextDimensionsRequest<'_>,
    ) -> Option<Map<String, Value>> {
        let text = json::decode_map(request.text);
        let container = request.container.map(json::decode_map);
        let elements = decode_all(request.elements);
        let answer = self.0.refresh_text_dimensions(TextDimensionsRequest {
            text: &text,
            container: container.as_ref(),
            elements: &elements,
        })?;
        Some(json::escape_map(&answer))
    }

    fn sticky_note_layout(
        &mut self,
        request: StickyNoteLayoutRequest<'_>,
    ) -> Option<StickyNoteLayout> {
        let note = json::decode_map(request.note);
        let text = request.text.map(json::decode_map);
        let elements = decode_all(request.elements);
        let answer = self.0.sticky_note_layout(StickyNoteLayoutRequest {
            note: &note,
            text: text.as_ref(),
            elements: &elements,
        })?;
        Some(StickyNoteLayout {
            container: json::escape_map(&answer.container),
            text: answer.text.as_ref().map(json::escape_map),
        })
    }

    fn update_elbow_arrow_points(
        &mut self,
        request: ElbowArrowRequest<'_>,
    ) -> Option<Map<String, Value>> {
        let arrow = json::decode_map(request.arrow);
        let points: Vec<Value> = request.points.iter().map(json::decode).collect();
        let elements = decode_all(request.elements);
        let answer = self.0.update_elbow_arrow_points(ElbowArrowRequest {
            arrow: &arrow,
            points: &points,
            elements: &elements,
        })?;
        Some(json::escape_map(&answer))
    }
}

fn decode_all(elements: &[Map<String, Value>]) -> Vec<Map<String, Value>> {
    elements.iter().map(json::decode_map).collect()
}

/// The keys of a JS object literal with their values in property creation
/// order; `None` is a key holding `undefined`, which keeps its place but
/// is not written.
#[derive(Default)]
struct JsObject(Vec<(String, Option<Value>)>);

impl JsObject {
    fn from_map(map: &Map<String, Value>) -> JsObject {
        JsObject(
            map.iter()
                .map(|(k, v)| (k.clone(), Some(v.clone())))
                .collect(),
        )
    }

    fn get(&self, key: &str) -> Option<&Value> {
        self.entry(key).and_then(Option::as_ref)
    }

    fn entry(&self, key: &str) -> Option<&Option<Value>> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// `object[key] = value`: an existing key keeps its place.
    fn set(&mut self, key: &str, value: Option<Value>) {
        match self.0.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = value,
            None => self.0.push((key.to_owned(), value)),
        }
    }

    /// `delete object[key]`.
    fn delete(&mut self, key: &str) {
        self.0.retain(|(k, _)| k != key);
    }

    /// `{...self, ...other}` onto self.
    fn spread(&mut self, other: &JsObject) {
        for (k, v) in &other.0 {
            self.set(k, v.clone());
        }
    }

    /// The object `JSON.stringify` writes: `undefined` keys dropped, keys
    /// in JS property order.
    fn into_map(self) -> Map<String, Value> {
        json::ordered_like_js(
            self.0
                .into_iter()
                .filter_map(|(k, v)| v.map(|v| (k, v)))
                .collect(),
        )
    }
}

/// `isUsingAdaptiveRadius(type)` (`packages/element/src/typeChecks.ts:322-326`)
/// for whatever `type` holds.
fn uses_adaptive_radius(ty: Option<&Value>) -> bool {
    ty.and_then(Value::as_str)
        .and_then(ElementType::parse)
        .is_some_and(ElementType::uses_adaptive_radius)
}

/// [`restore_element_with_properties`] on sentinel-form values.
pub(crate) fn restore_encoded(
    element: &Map<String, Value>,
    extra: &[(&str, Option<Value>)],
    env: &mut dyn RestoreEnv,
) -> Result<Map<String, Value>, RestoreError> {
    let props = DEFAULT_ELEMENT_PROPS;
    let el = |key: &str| element.get(key);
    let mut extra_object = JsObject::default();
    for (k, v) in extra {
        extra_object.set(k, v.clone());
    }
    let ex = |key: &str| extra_object.get(key);
    let value = |v: Value| move || Some(v);

    let mut base = JsObject::default();
    base.set("type", js::or(ex("type"), || el("type").cloned()));
    base.set("version", js::or(el("version"), value(json!(1))));
    base.set(
        "versionNonce",
        js::nullish_or(el("versionNonce"), value(json!(0))),
    );
    base.set("index", js::nullish_or(el("index"), value(Value::Null)));
    base.set(
        "isDeleted",
        js::nullish_or(el("isDeleted"), value(json!(false))),
    );
    base.set(
        "id",
        js::or(el("id"), || Some(Value::String(env.random_id()))),
    );
    base.set(
        "fillStyle",
        js::or(el("fillStyle"), value(json!(props.fill_style))),
    );
    base.set(
        "strokeWidth",
        js::or(el("strokeWidth"), value(js::number(props.stroke_width))),
    );
    base.set(
        "strokeStyle",
        js::nullish_or(el("strokeStyle"), value(json!(props.stroke_style))),
    );
    base.set(
        "roughness",
        js::nullish_or(el("roughness"), value(js::number(props.roughness))),
    );
    base.set(
        "opacity",
        js::nullish_or(el("opacity"), value(js::number(props.opacity))),
    );
    base.set("angle", js::or(el("angle"), value(json!(0))));
    base.set(
        "x",
        js::nullish_or(ex("x"), || js::nullish_or(el("x"), value(json!(0)))),
    );
    base.set(
        "y",
        js::nullish_or(ex("y"), || js::nullish_or(el("y"), value(json!(0)))),
    );
    base.set(
        "strokeColor",
        js::or(el("strokeColor"), value(json!(props.stroke_color))),
    );
    base.set(
        "backgroundColor",
        js::or(el("backgroundColor"), value(json!(props.background_color))),
    );
    base.set("width", js::or(el("width"), value(json!(0))));
    base.set("height", js::or(el("height"), value(json!(0))));
    base.set("seed", js::nullish_or(el("seed"), value(json!(1))));
    base.set("groupIds", js::nullish_or(el("groupIds"), value(json!([]))));
    base.set("frameId", js::nullish_or(el("frameId"), value(Value::Null)));
    base.set("roundness", Some(roundness(element)));
    base.set("boundElements", Some(bound_elements(element)?));
    base.set(
        "updated",
        js::nullish_or(el("updated"), || Some(js::number(env.now()))),
    );
    base.set("created", js::nullish_or(el("created"), value(Value::Null)));
    base.set("link", Some(link(el("link"))?));
    base.set(
        "locked",
        js::nullish_or(el("locked"), value(json!(props.locked))),
    );
    if let Some(custom) = extra_object.entry("customData") {
        base.set("customData", custom.clone());
    } else if let Some(custom) = el("customData") {
        base.set("customData", Some(custom.clone()));
    }

    let dimensions = dimensions(&base)?;

    let mut ret = JsObject::from_map(element);
    ret.spread(&base);
    ret.spread(&dimensions);
    ret.spread(&extra_object);
    ret.delete("strokeSharpness");
    ret.delete("boundElementIds");
    Ok(ret.into_map())
}

/// `restore.ts:475-485`.
fn roundness(element: &Map<String, Value>) -> Value {
    let current = element.get("roundness");
    if js::truthy(current) {
        return current.cloned().unwrap_or(Value::Null);
    }
    if element.get("strokeSharpness").and_then(Value::as_str) == Some("round") {
        // For old elements that would now use the adaptive radius
        // algorithm, the legacy one.
        let ty = if uses_adaptive_radius(element.get("type")) {
            1
        } else {
            2
        };
        return json!({ "type": ty });
    }
    Value::Null
}

/// `restore.ts:486-488`.
fn bound_elements(element: &Map<String, Value>) -> Result<Value, RestoreError> {
    let ids = element.get("boundElementIds");
    if js::truthy(ids) {
        let Some(Value::Array(ids)) = ids else {
            return Err(RestoreError::BoundElementIdsNotArray);
        };
        return Ok(Value::Array(
            ids.iter()
                .map(|id| {
                    let mut entry = Map::new();
                    entry.insert("type".to_owned(), json!("arrow"));
                    entry.insert("id".to_owned(), id.clone());
                    Value::Object(entry)
                })
                .collect(),
        ));
    }
    Ok(js::nullish_or(element.get("boundElements"), || Some(json!([]))).unwrap_or(Value::Null))
}

/// `element.link ? normalizeLink(element.link) : null` (`restore.ts:491`).
fn link(link: Option<&Value>) -> Result<Value, RestoreError> {
    if !js::truthy(link) {
        return Ok(Value::Null);
    }
    match link {
        Some(Value::String(s)) => Ok(Value::String(url::normalize_link_encoded(s))),
        _ => Err(RestoreError::LinkNotString),
    }
}

/// `getNormalizedDimensions(base)` (`sizeHelpers.ts:256-283`) on JS values:
/// `width < 0` and `Math.abs(width)` coerce with `ToNumber`, and so does
/// `x - nextWidth`, and throw where that conversion does. Returns `{width,
/// height, x, y}` in that order.
fn dimensions(base: &JsObject) -> Result<JsObject, RestoreError> {
    let mut ret = JsObject::default();
    let field = |key: &str| base.entry(key).cloned().unwrap_or(None);
    ret.set("width", field("width"));
    ret.set("height", field("height"));
    ret.set("x", field("x"));
    ret.set("y", field("y"));
    for (size_key, position_key) in [("width", "x"), ("height", "y")] {
        // The only TypeError ToNumber raises on a JSON value.
        let to_number =
            |key: &str| js::to_number(base.get(key)).map_err(|_| RestoreError::NoPrimitiveValue);
        let size = to_number(size_key)?;
        if size < 0.0 {
            let next = size.abs();
            let position = to_number(position_key)?;
            ret.set(size_key, Some(js::number(next)));
            ret.set(position_key, Some(js::number(position - next)));
        }
    }
    Ok(ret)
}
