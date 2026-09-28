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
//! (`getNormalizedDimensions`, `packages/element/src/sizeHelpers.ts:256-283`).

mod url;

#[cfg(test)]
mod tests;

use serde_json::{json, Map, Value};
use std::fmt;

use crate::constants::DEFAULT_ELEMENT_PROPS;
use crate::element::ElementType;
use crate::js;
use crate::json;

/// Where restore gets what upstream draws from global state.
pub trait RestoreEnv {
    /// `getUpdatedTimestamp()` (`packages/common/src/utils.ts:552`): epoch
    /// milliseconds, `Date.now()` upstream.
    fn now(&mut self) -> f64;
    /// `randomId()` (`packages/common/src/random.ts:16`): a fresh element
    /// id, a 21-character nanoid upstream.
    fn random_id(&mut self) -> String;
}

/// Upstream's test mode (`isTestEnv()`): ids are `id0`, `id1`, ... and the
/// timestamp is always 1. Restoring with it gives exactly what upstream's
/// test suite and `tools/goldens/restore-fixtures.mjs` record.
#[derive(Debug, Clone, Default)]
pub struct TestEnv {
    next_id: u64,
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
}

impl fmt::Display for RestoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            RestoreError::BoundElementIdsNotArray => {
                "element.boundElementIds.map is not a function"
            }
            RestoreError::LinkNotString => "link.trim is not a function",
            RestoreError::NoPrimitiveValue => "Cannot convert object to primitive value",
        })
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

/// A [`RestoreEnv`] whose ids are put in the sentinel form.
struct EscapingEnv<'a>(&'a mut dyn RestoreEnv);

impl RestoreEnv for EscapingEnv<'_> {
    fn now(&mut self) -> f64 {
        self.0.now()
    }

    fn random_id(&mut self) -> String {
        json::escape_str(&self.0.random_id()).into_owned()
    }
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
