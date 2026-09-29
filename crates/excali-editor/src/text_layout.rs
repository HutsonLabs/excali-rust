//! Laying out bound text after an edit: `redrawTextBoundingBox`, the sticky
//! note fit, and where an arrow's label sits.
//!
//! Upstream: `packages/element/src/textElement.ts:51-152`
//! (`redrawTextBoundingBox`), `stickyNote.ts:363-..` (the font ceiling,
//! `fitStickyNoteFont`, `getStickyNoteLayout`, `updateStickyNoteLayout`),
//! `sizeHelpers.ts:28-54` (`getPositionAfterHeightChange`) and
//! `containerCache.ts` (the original container heights), at the pinned
//! commit.
//!
//! - [`TextLayouter`] holds what text layout reads besides the scene: the
//!   line-width provider, the per-character width cache wrapping fills,
//!   and the [`OriginalContainerCache`] (a module global upstream, kept
//!   across edits). Its [`TextLayouter::redraw_text_bounding_box`] is the
//!   leaf layout call undo and redo make for each container and label pair
//!   (`HistoryEnv::redraw_text_bounding_box`), and text editing makes on
//!   submit.
//! - A label in a container is re-wrapped from its `originalText` to the
//!   container's room, measured, and placed by `computeBoundTextPosition`;
//!   the container grows (never shrinks) when the text no longer fits. A
//!   free text is re-wrapped at its own width unless it grows with its
//!   content (`autoResize`).
//! - A sticky note owns both halves: [`get_sticky_note_layout`] wraps the
//!   label at the note's width, steps the font down from the ceiling
//!   (`baseFontSize`) until the text fits, grows the note past its
//!   `baseHeight` only when it still overflows at the smallest size, and
//!   places the label.
//! - An arrow's label is placed along the arrow's path
//!   ([`SceneArrowGeometry`], `LinearElementEditor.getBoundTextElementPosition`).
//!
//! The arrows bound to a sticky note follow it after its layout
//! (`updateBoundElements`); arrow routing belongs to arrow binding, so the
//! caller hands that step in.

use std::collections::HashMap;

use excali_core::constants::{
    MIN_FONT_SIZE, STICKY_NOTE_BODY_INSET_Y, STICKY_NOTE_FALLBACK_FONT_SIZE,
    STICKY_NOTE_MAX_FONT_SIZE, STICKY_NOTE_MIN_SIZE, STICKY_NOTE_PADDING,
};
use excali_core::element::{Element, ElementKind, TextFields};
use excali_core::fractional_index::{ChangeStamp, SceneElementsMap};
use excali_math::js;
use excali_scene::bounds::ElementsMap;
use excali_scene::linear_element::get_bound_text_element_position;
use excali_text::font_metadata::get_font_string;
use excali_text::new_element::TextLayout;
use excali_text::text_element::{
    compute_bound_text_position, compute_container_dimension_for_bound_text,
    get_bound_text_max_height, get_bound_text_max_width, ArrowLabelGeometry,
};
use excali_text::text_measurements::{measure_text, CharWidthCache, TextMetricsProvider};
use excali_text::text_wrapping::wrap_text;
use serde_json::{Map, Value};

use crate::js_value::num;
use crate::mutate::mutate_element;

/// `STICKY_NOTE_MIN_FONT_SIZE` (`constants.ts:224`): the smallest size the
/// sticky note fit steps down to.
pub const STICKY_NOTE_MIN_FONT_SIZE: f64 = 16.0;
/// `STICKY_NOTE_FONT_STEP` (`constants.ts:227`): the fit's step.
pub const STICKY_NOTE_FONT_STEP: f64 = 2.0;

/// `originalContainerCache` (`containerCache.ts`): the height a container
/// had before its text grew it, by container id. Text editing shrinks a
/// container back towards it as text is removed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OriginalContainerCache {
    heights: HashMap<String, f64>,
}

impl OriginalContainerCache {
    pub fn new() -> OriginalContainerCache {
        OriginalContainerCache::default()
    }

    /// `updateOriginalContainerCache(id, height)`: the entry set to
    /// `height`, returned.
    pub fn update(&mut self, id: &str, height: f64) -> f64 {
        self.heights.insert(id.to_owned(), height);
        height
    }

    /// `originalContainerCache[id]?.height`.
    pub fn get(&self, id: &str) -> Option<f64> {
        self.heights.get(id).copied()
    }

    /// `resetOriginalContainerCache(id)`.
    pub fn reset(&mut self, id: &str) {
        self.heights.remove(id);
    }

    /// Every entry, `{ id: { height } }` as upstream's object holds them,
    /// keys in UTF-16 order.
    pub fn to_json(&self) -> Value {
        let mut ids: Vec<&String> = self.heights.keys().collect();
        ids.sort_by(|a, b| crate::js_value::compare_utf16(a, b));
        let mut out = Map::new();
        for id in ids {
            let mut entry = Map::new();
            entry.insert("height".into(), num(self.heights[id]));
            out.insert(id.clone(), Value::Object(entry));
        }
        Value::Object(out)
    }
}

/// `LinearElementEditor.getBoundTextElementPosition` over the scene: an
/// arrow's label at its `labelPosition` along the path, or at the arrow's
/// middle.
#[derive(Clone, Copy, Debug, Default)]
pub struct SceneArrowGeometry;

impl ArrowLabelGeometry for SceneArrowGeometry {
    fn bound_text_element_position(
        &mut self,
        arrow: &Element,
        text: &Element,
        elements: &[Element],
    ) -> Option<[f64; 2]> {
        let map = ElementsMap::new(elements.iter().filter(|e| !e.base.is_deleted));
        Some(get_bound_text_element_position(arrow, text, &map))
    }
}

/// Which edge `getPositionAfterHeightChange` holds still
/// (`VerticalResizeAnchor`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VerticalAnchor {
    #[default]
    Top,
    Bottom,
    Center,
}

/// `getPositionAfterHeightChange(element, nextHeight, anchor)`
/// (`sizeHelpers.ts:28-54`): the element's `x` and `y` once its height is
/// `next_height`, so that the anchored edge (or the centre) stays where it
/// is on screen whatever the rotation.
pub fn get_position_after_height_change(
    element: &Element,
    next_height: f64,
    anchor: VerticalAnchor,
) -> [f64; 2] {
    let b = &element.base;
    let delta = (b.height - next_height) / 2.0;
    if anchor == VerticalAnchor::Center {
        return [b.x, b.y + delta];
    }
    let sin = js::sin(b.angle.0);
    let cos = js::cos(b.angle.0);
    if anchor == VerticalAnchor::Bottom {
        return [b.x - delta * sin, b.y + delta * (1.0 + cos)];
    }
    [b.x + delta * sin, b.y + delta * (1.0 - cos)]
}

/// `normalizeStickyNoteFontSize(fontSize)` (`stickyNote.ts:366-371`): the
/// ceiling clamped to `[MIN_FONT_SIZE, STICKY_NOTE_MAX_FONT_SIZE]`, the
/// fallback for a non-finite one.
pub fn normalize_sticky_note_font_size(font_size: f64) -> f64 {
    if !font_size.is_finite() {
        return STICKY_NOTE_FALLBACK_FONT_SIZE;
    }
    js::min(STICKY_NOTE_MAX_FONT_SIZE, js::max(MIN_FONT_SIZE, font_size))
}

/// What a caller of [`get_sticky_note_layout`] may ask for
/// (`StickyNoteLayoutOpts`); `None` keeps the live value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StickyNoteLayoutOpts {
    /// The unwrapped text to lay out (default: the label's `originalText`).
    pub original_text: Option<String>,
    /// The base height to keep (default: the note's `baseHeight`, or its
    /// height without one).
    pub base_height: Option<f64>,
    /// The font ceiling (default: the label's `baseFontSize`, else its
    /// `fontSize`).
    pub base_font_size: Option<f64>,
    /// The edge that stays put when the height changes.
    pub anchor: VerticalAnchor,
}

/// The note's half of a [`StickyNoteLayout`].
#[derive(Clone, Debug, PartialEq)]
pub struct StickyNoteGeometry {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub base_height: f64,
}

/// The label's half of a [`StickyNoteLayout`].
#[derive(Clone, Debug, PartialEq)]
pub struct StickyNoteLabel {
    pub text: String,
    pub font_size: f64,
    pub base_font_size: f64,
    pub width: f64,
    pub height: f64,
    pub x: f64,
    pub y: f64,
    pub angle: f64,
}

/// `StickyNoteLayout`: the updates for the note and for its label.
#[derive(Clone, Debug, PartialEq)]
pub struct StickyNoteLayout {
    pub container: StickyNoteGeometry,
    pub text: Option<StickyNoteLabel>,
}

impl StickyNoteGeometry {
    /// The keys to assign, in upstream's order (`{ x, y, width, height,
    /// baseHeight }`).
    pub fn to_map(&self) -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("x".into(), num(self.x));
        m.insert("y".into(), num(self.y));
        m.insert("width".into(), num(self.width));
        m.insert("height".into(), num(self.height));
        m.insert("baseHeight".into(), num(self.base_height));
        m
    }
}

impl StickyNoteLabel {
    /// The keys to assign, in upstream's order (`{ text, fontSize,
    /// baseFontSize, width, height, x, y, angle }`).
    pub fn to_map(&self) -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("text".into(), Value::String(self.text.clone()));
        m.insert("fontSize".into(), num(self.font_size));
        m.insert("baseFontSize".into(), num(self.base_font_size));
        m.insert("width".into(), num(self.width));
        m.insert("height".into(), num(self.height));
        m.insert("x".into(), num(self.x));
        m.insert("y".into(), num(self.y));
        m.insert("angle".into(), num(self.angle));
        m
    }
}

/// One candidate of the font fit (`FontFit`).
#[derive(Clone, Debug)]
struct FontFit {
    text: String,
    font_size: f64,
    width: f64,
    height: f64,
}

/// `fitStickyNoteFont(fit, {...})` (`stickyNote.ts:563-640`): the largest
/// size on the grid `{ceiling - k·STEP} ∪ {min}` whose wrapped text fits,
/// `min` when none does, found by binary search warm-started from the
/// previous size.
fn fit_sticky_note_font(
    fit: &mut dyn FnMut(f64) -> FontFit,
    base_font_size: f64,
    font_size_min: f64,
    max_width: f64,
    max_height: f64,
    warm_start: f64,
) -> FontFit {
    let steps = js::max(
        0.0,
        ((base_font_size - font_size_min) / STICKY_NOTE_FONT_STEP).ceil(),
    );
    let size_at = |index: f64| {
        if index >= steps {
            font_size_min
        } else {
            base_font_size - index * STICKY_NOTE_FONT_STEP
        }
    };
    let mut fits: HashMap<u64, FontFit> = HashMap::new();
    let mut at = |index: f64, fit: &mut dyn FnMut(f64) -> FontFit| -> FontFit {
        fits.entry(index.to_bits())
            .or_insert_with(|| fit(size_at(index)))
            .clone()
    };
    let mut does_fit = |index: f64, fit: &mut dyn FnMut(f64) -> FontFit| -> bool {
        let result = at(index, fit);
        result.width <= max_width && result.height <= max_height
    };

    if steps == 0.0 {
        return at(0.0, fit);
    }

    // the previous fitted size, snapped onto the grid and clamped into the
    // current interval
    let warm = js::min(
        js::max(
            js::round((base_font_size - warm_start) / STICKY_NOTE_FONT_STEP),
            0.0,
        ),
        steps,
    );

    let (mut lo, mut hi);
    if does_fit(warm, fit) {
        if warm == 0.0 || !does_fit(warm - 1.0, fit) {
            return at(warm, fit);
        }
        lo = 0.0;
        hi = warm - 1.0;
    } else {
        if warm == steps {
            return at(steps, fit);
        }
        lo = warm + 1.0;
        hi = steps;
    }
    while lo < hi {
        // (lo + hi) >> 1
        let mid = ((lo + hi) as i32 >> 1) as f64;
        if does_fit(mid, fit) {
            hi = mid;
        } else {
            lo = mid + 1.0;
        }
    }
    at(lo, fit)
}

/// `!text.trim()`: nothing but JavaScript whitespace.
fn is_blank(text: &str) -> bool {
    text.chars().all(|c| c.is_whitespace() || c == '\u{feff}')
}

fn text_fields(element: &Element) -> Option<&TextFields> {
    match &element.kind {
        ElementKind::Text(t) => Some(t),
        _ => None,
    }
}

/// `getStickyNoteLayout(container, textElement, opts)`
/// (`stickyNote.ts:648-729`): the note's geometry and its label's layout.
///
/// Without a label the note sits at its base height. Otherwise the label's
/// text is wrapped at the note's padded width and the font fitted under
/// the ceiling to the padded body (a blank text keeps the ceiling); the
/// note grows past its base height only as far as the fitted text needs,
/// holding `opts.anchor`, and the label is placed by
/// `computeBoundTextPosition` in the note as laid out.
pub fn get_sticky_note_layout(
    layout: &mut TextLayout<'_>,
    container: &Element,
    text_element: Option<&Element>,
    opts: &StickyNoteLayoutOpts,
) -> StickyNoteLayout {
    let base_width = js::max(container.base.width, STICKY_NOTE_MIN_SIZE);
    let note_base_height = match &container.kind {
        ElementKind::StickyNote(note) => note.base_height,
        _ => 0.0,
    };
    // container.baseHeight || container.height
    let live_base_height = if note_base_height == 0.0 || note_base_height.is_nan() {
        container.base.height
    } else {
        note_base_height
    };
    let base_height = js::max(
        opts.base_height.unwrap_or(live_base_height),
        STICKY_NOTE_MIN_SIZE,
    );
    let anchor = opts.anchor;

    let geometry = |height: f64| {
        let [x, y] = get_position_after_height_change(container, height, anchor);
        StickyNoteGeometry {
            x,
            y,
            width: base_width,
            height,
            base_height,
        }
    };

    let Some((text_element, fields)) = text_element.and_then(|t| Some((t, text_fields(t)?))) else {
        // an empty note sits at its base height
        return StickyNoteLayout {
            container: geometry(base_height),
            text: None,
        };
    };

    let original_text = opts
        .original_text
        .clone()
        .unwrap_or_else(|| fields.original_text.clone());
    let base_font_size = normalize_sticky_note_font_size(
        opts.base_font_size
            .or(fields.base_font_size)
            .unwrap_or(fields.font_size),
    );
    let font_size_min = js::min(STICKY_NOTE_MIN_FONT_SIZE, base_font_size);
    let max_width = js::max(base_width - STICKY_NOTE_PADDING * 2.0, 1.0);
    let max_height = js::max(base_height - STICKY_NOTE_BODY_INSET_Y, 0.0);
    let (font_family, line_height) = (fields.font_family, fields.line_height);

    let blank = is_blank(&original_text);
    let fitted = if blank {
        let metrics = measure_text(
            "",
            &get_font_string(base_font_size, font_family),
            line_height,
            layout.provider,
        );
        FontFit {
            text: String::new(),
            font_size: base_font_size,
            width: metrics.width,
            height: metrics.height,
        }
    } else {
        let provider = layout.provider;
        let char_widths = &mut *layout.char_widths;
        let mut fit = |font_size: f64| {
            let font = get_font_string(font_size, font_family);
            let text = wrap_text(&original_text, &font, max_width, provider, char_widths);
            let metrics = measure_text(&text, &font, line_height, provider);
            FontFit {
                text,
                font_size,
                width: metrics.width,
                height: metrics.height,
            }
        };
        fit_sticky_note_font(
            &mut fit,
            base_font_size,
            font_size_min,
            max_width,
            max_height,
            fields.font_size,
        )
    };

    let height = if blank {
        base_height
    } else {
        js::max(base_height, fitted.height + STICKY_NOTE_BODY_INSET_Y)
    };
    let next_container = geometry(height);

    let mut laid_out_container = container.clone();
    laid_out_container.base.x = next_container.x;
    laid_out_container.base.y = next_container.y;
    laid_out_container.base.width = next_container.width;
    laid_out_container.base.height = next_container.height;
    if let ElementKind::StickyNote(note) = &mut laid_out_container.kind {
        note.base_height = next_container.base_height;
    }
    let mut laid_out_text = text_element.clone();
    laid_out_text.base.width = fitted.width;
    laid_out_text.base.height = fitted.height;
    if let ElementKind::Text(t) = &mut laid_out_text.kind {
        t.text.clone_from(&fitted.text);
        t.font_size = fitted.font_size;
    }
    // computeBoundTextPosition only consults the map for arrow containers
    let [x, y] =
        compute_bound_text_position(&laid_out_container, &laid_out_text, &[], layout.geometry)
            .unwrap_or([laid_out_text.base.x, laid_out_text.base.y]);

    StickyNoteLayout {
        container: next_container,
        text: Some(StickyNoteLabel {
            text: fitted.text,
            font_size: fitted.font_size,
            base_font_size,
            width: fitted.width,
            height: fitted.height,
            x,
            y,
            angle: container.base.angle.0,
        }),
    }
}

/// The arrows bound to an element follow it: `updateBoundElements(element,
/// scene)` (`binding.ts:1321`), which arrow binding implements. It gets the
/// scene and the id of the element that moved or resized.
pub type BoundElementsUpdate<'a> =
    &'a mut dyn FnMut(&mut dyn ChangeStamp, &mut SceneElementsMap, &str) -> Result<(), String>;

/// `mutateElement(elements[id], updates)` in place: whether it changed.
pub(crate) fn mutate_in(
    stamp: &mut dyn ChangeStamp,
    elements: &mut SceneElementsMap,
    id: &str,
    updates: Map<String, Value>,
) -> Result<bool, String> {
    let Some(mut element) = elements.get(id).cloned() else {
        return Err(format!("no element {id}"));
    };
    let changed =
        mutate_element(&mut element, elements, updates, stamp).map_err(|e| e.to_string())?;
    if changed {
        elements.insert(id.to_owned(), element);
    }
    Ok(changed)
}

/// The scene's non-deleted elements, in order (`getNonDeletedElementsMap`).
fn non_deleted(elements: &SceneElementsMap) -> Vec<Element> {
    elements
        .values()
        .filter(|e| !e.base.is_deleted)
        .cloned()
        .collect()
}

/// Text layout's state besides the scene: the line-width provider, the
/// wrapping cache and the original container heights.
#[derive(Debug, Default)]
pub struct TextLayouter<P> {
    pub provider: P,
    pub char_widths: CharWidthCache,
    pub container_cache: OriginalContainerCache,
}

impl<P: TextMetricsProvider> TextLayouter<P> {
    pub fn new(provider: P) -> TextLayouter<P> {
        TextLayouter {
            provider,
            char_widths: CharWidthCache::new(),
            container_cache: OriginalContainerCache::new(),
        }
    }

    /// Runs `f` with the provider, the wrapping cache and the arrow
    /// geometry as a [`TextLayout`], and the container cache.
    pub fn with_layout<R>(
        &mut self,
        f: impl FnOnce(&mut TextLayout<'_>, &mut OriginalContainerCache) -> R,
    ) -> R {
        let mut geometry = SceneArrowGeometry;
        let mut layout = TextLayout {
            provider: &self.provider,
            char_widths: &mut self.char_widths,
            geometry: &mut geometry,
        };
        f(&mut layout, &mut self.container_cache)
    }

    /// `updateStickyNoteLayout(container, scene, { text })`
    /// (`stickyNote.ts:858-890`): [`get_sticky_note_layout`] applied to the
    /// note and to `text_id` (the note's label by default) in the scene,
    /// then the arrows bound to the note laid out by `bound_elements`
    /// unless the note is deleted.
    pub fn update_sticky_note_layout(
        &mut self,
        stamp: &mut dyn ChangeStamp,
        elements: &mut SceneElementsMap,
        container_id: &str,
        text_id: Option<&str>,
        opts: &StickyNoteLayoutOpts,
        bound_elements: BoundElementsUpdate<'_>,
    ) -> Result<StickyNoteLayout, String> {
        let container = elements
            .get(container_id)
            .cloned()
            .ok_or_else(|| format!("no element {container_id}"))?;
        let text_id = match text_id {
            Some(id) => Some(id.to_owned()),
            None => bound_text_id(&container, elements),
        };
        let text = text_id.as_deref().and_then(|id| elements.get(id)).cloned();
        let layout = self.with_layout(|layout, _| {
            get_sticky_note_layout(layout, &container, text.as_ref(), opts)
        });
        mutate_in(stamp, elements, container_id, layout.container.to_map())?;
        if let (Some(text), Some(label)) = (&text, &layout.text) {
            mutate_in(stamp, elements, &text.base.id, label.to_map())?;
        }
        if !container.base.is_deleted {
            bound_elements(stamp, elements, container_id)?;
        }
        Ok(layout)
    }

    /// `redrawTextBoundingBox(textElement, container, scene)`
    /// (`textElement.ts:51-152`) on the scene's elements `text_id` and
    /// `container_id`.
    ///
    /// - In a sticky note: [`Self::update_sticky_note_layout`] with this
    ///   label.
    /// - In any other container, or for a free text that does not grow with
    ///   its content: the `originalText` wrapped to the container's room
    ///   (the text's own width), measured; the width follows the text when
    ///   it grows with it. A container too small for the text grows to fit
    ///   (its height remembered in the [`OriginalContainerCache`]; an arrow
    ///   only widens), and the label is placed by
    ///   `computeBoundTextPosition` with the container's angle (0 on an
    ///   arrow).
    /// - A free text that grows with its content keeps its lines and is
    ///   measured.
    pub fn redraw_text_bounding_box(
        &mut self,
        stamp: &mut dyn ChangeStamp,
        elements: &mut SceneElementsMap,
        text_id: &str,
        container_id: Option<&str>,
        bound_elements: BoundElementsUpdate<'_>,
    ) -> Result<(), String> {
        let text_element = elements
            .get(text_id)
            .cloned()
            .ok_or_else(|| format!("no element {text_id}"))?;
        let Some(fields) = text_fields(&text_element).cloned() else {
            return Err(format!("{text_id} is not a text"));
        };
        let container = match container_id {
            Some(id) => Some(
                elements
                    .get(id)
                    .cloned()
                    .ok_or_else(|| format!("no element {id}"))?,
            ),
            None => None,
        };
        if let Some(c) = &container {
            if matches!(c.kind, ElementKind::StickyNote(_)) {
                // the sticky fit owns both halves (label and note geometry)
                self.update_sticky_note_layout(
                    stamp,
                    elements,
                    &c.base.id,
                    Some(text_id),
                    &StickyNoteLayoutOpts::default(),
                    bound_elements,
                )?;
                return Ok(());
            }
        }
        let is_arrow = container
            .as_ref()
            .is_some_and(|c| matches!(c.kind, ElementKind::Arrow(_)));
        let font = get_font_string(fields.font_size, fields.font_family);
        let angle = match &container {
            Some(_) if is_arrow => 0.0,
            Some(c) => c.base.angle.0,
            None => text_element.base.angle.0,
        };
        let mut text = fields.text.clone();
        if container.is_some() || !fields.auto_resize {
            let max_width = match &container {
                Some(c) => get_bound_text_max_width(c, Some(&text_element)),
                None => text_element.base.width,
            };
            text = wrap_text(
                &fields.original_text,
                &font,
                max_width,
                &self.provider,
                &mut self.char_widths,
            );
        }
        let metrics = measure_text(&text, &font, fields.line_height, &self.provider);
        // only unwrapped text and bound texts (always autoResize) take the
        // measured width
        let width = if fields.auto_resize {
            metrics.width
        } else {
            text_element.base.width
        };
        let (mut x, mut y) = (text_element.base.x, text_element.base.y);

        if let Some(container) = &container {
            let max_container_height = get_bound_text_max_height(container, &text_element);
            let max_container_width = get_bound_text_max_width(container, Some(&text_element));
            let id = container.base.id.as_str();
            if !is_arrow && metrics.height > max_container_height {
                let next_height = compute_container_dimension_for_bound_text(
                    metrics.height,
                    container.element_type(),
                );
                mutate_in(stamp, elements, id, one("height", num(next_height)))?;
                self.container_cache.update(id, next_height);
            }
            if metrics.width > max_container_width {
                let next_width = compute_container_dimension_for_bound_text(
                    metrics.width,
                    container.element_type(),
                );
                mutate_in(stamp, elements, id, one("width", num(next_width)))?;
            }
            let mut updated = text_element.clone();
            updated.base.width = width;
            updated.base.height = metrics.height;
            updated.base.angle.0 = angle;
            if let ElementKind::Text(t) = &mut updated.kind {
                t.text.clone_from(&text);
            }
            let live_container = elements[id].clone();
            let scene = non_deleted(elements);
            let mut geometry = SceneArrowGeometry;
            if let Some([px, py]) =
                compute_bound_text_position(&live_container, &updated, &scene, &mut geometry)
            {
                x = px;
                y = py;
            }
        }

        let mut updates = Map::new();
        updates.insert("x".into(), num(x));
        updates.insert("y".into(), num(y));
        updates.insert("text".into(), Value::String(text));
        updates.insert("width".into(), num(width));
        updates.insert("height".into(), num(metrics.height));
        updates.insert("angle".into(), num(angle));
        mutate_in(stamp, elements, text_id, updates)?;
        Ok(())
    }
}

/// A one-key update.
pub(crate) fn one(key: &str, value: Value) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert(key.to_owned(), value);
    m
}

/// `getBoundTextElementId(container)` among the scene's elements: the
/// first `text` entry of its `boundElements` naming an element of the
/// scene.
pub fn bound_text_id(container: &Element, elements: &SceneElementsMap) -> Option<String> {
    container
        .base
        .bound_elements
        .iter()
        .flatten()
        .find(|b| b.kind == excali_core::element::BoundElementType::Text)
        .map(|b| b.id.clone())
        .filter(|id| elements.get(id).is_some_and(|e| !e.base.is_deleted))
}
