//! The stats panel: `Stats` (`components/Stats/index.tsx`, `Stats.scss`)
//! in its `Island`, as a [`dom`](crate::dom) tree.
//!
//! Two collapsible sections whose open state is the bitmask
//! `appState.stats.panels` (`STATS_PANELS`, `constants.ts:584`): General,
//! with the scene's shape count, width and height and, in grid mode, the
//! grid step (`index.tsx:195-236`); and Shape properties, while something
//! is selected and not a frame together with its children, with the
//! element's type, X, Y, W, H, angle and font size for one element
//! (`Position`, `Dimension`, `Angle`, `FontSize`, `index.tsx:248-345`) or
//! the count and each property, "Mixed" where the atomic units differ, for
//! several (`MultiPosition`, `MultiDimension`, `MultiAngle`,
//! `MultiFontSize`, `index.tsx:362-415`). A header's click toggles its bit
//! ([`toggle_panel`]); the close button runs `toggleStats`. Each value is
//! a `DragInput` (`DragInput.tsx`), whose typed value the panel hands back
//! once [`typed_value`] accepts it. What the panel does is a list of
//! [`StatsEvent`]s the caller applies; [`StatsPanel::controls`] keeps the
//! click handlers, in document order, so its behaviour is checkable
//! without a browser. See `site/content/research/ui-design-system.md`
//! section 3.9.

use std::cell::Cell;
use std::rc::Rc;

use excali_core::app_state::AppState;
use excali_core::constants::{STATS_PANEL_ELEMENT_PROPERTIES, STATS_PANEL_GENERAL_STATS};
use excali_core::element::{Element as SceneElement, ElementKind, ElementType};
use excali_core::json::number_to_string;
use excali_editor::actions::{
    elements_are_in_same_group, frame_and_children_selected_together, has_bound_text_element,
};
use excali_editor::crop::{get_flip_adjusted_crop_position, get_uncropped_width_and_height};
use excali_math::{js, point_from, point_rotate_rads, radians_to_degrees, GlobalPoint, Radians};
use excali_scene::bounds::{get_bound_text_element, get_common_bounds, ElementsMap};
use excali_scene::frame::is_frame_like;
use excali_scene::shape::Theme;
use serde_json::{json, Value};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::Document;

use crate::dom::{class_names, Element, Node};
use crate::icons;
use crate::primitives::{island, IslandProps};

/// `Stats.scss` and `DragInput.scss` compiled (`tools/goldens/stats.mjs`).
pub const STATS_CSS: &str = include_str!("stats.css");

/// `SMALLEST_DELTA` (`Stats/utils.ts:43`).
pub const SMALLEST_DELTA: f64 = 0.01;

/// A value the panel edits (`StatsInputProperty`, `Stats/utils.ts:34-41`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatsProperty {
    X,
    Y,
    Width,
    Height,
    Angle,
    FontSize,
    GridStep,
}

/// What a handler of the panel does.
#[derive(Clone, Debug, PartialEq)]
pub enum StatsEvent {
    /// The close button: `actionManager.executeAction(actionToggleStats)`.
    Close,
    /// A section header: [`toggle_panel`] with this bit.
    TogglePanel(u32),
    /// A drag input's typed value, accepted by [`typed_value`]: the
    /// component's `dragInputCallback` with `nextValue`.
    Input { property: StatsProperty, value: f64 },
    /// Enter in a drag input: `app.focusContainer()`.
    FocusContainer,
}

/// Applies an event.
pub type OnStatsEvent = Rc<dyn Fn(StatsEvent)>;

/// What `Stats` reads: the scene (deleted elements included), the app
/// state and the host's `gridModeEnabled` prop (`isGridModeEnabled`,
/// `snapping.ts:159-160`).
pub struct StatsProps<'a> {
    pub elements: &'a [SceneElement],
    pub app_state: &'a AppState,
    pub grid_mode_enabled: Option<bool>,
}

/// A rendered panel: its tree and, in document order, each clickable
/// control's name (`close`, `generalStats`, `elementProperties`) and event.
pub struct StatsPanel {
    pub element: Element,
    pub controls: Vec<(&'static str, StatsEvent)>,
}

/// The English strings the panel reads (`locales/en.json`); the key itself
/// for any other, as `t` answers a missing key.
pub fn stats_text(key: &str) -> &str {
    match key {
        "stats.angle" => "Angle",
        "stats.shapes" => "Shapes",
        "stats.height" => "Height",
        "stats.scene" => "Scene",
        "stats.selected" => "Selected",
        "stats.storage" => "Storage",
        "stats.fullTitle" => "Canvas & Shape properties",
        "stats.title" => "Properties",
        "stats.generalStats" => "General",
        "stats.elementProperties" => "Shape properties",
        "stats.total" => "Total",
        "stats.version" => "Version",
        "stats.versionCopy" => "Click to copy",
        "stats.versionNotAvailable" => "Version not available",
        "stats.width" => "Width",
        "element.rectangle" => "Rectangle",
        "element.diamond" => "Diamond",
        "element.ellipse" => "Ellipse",
        "element.arrow" => "Arrow",
        "element.line" => "Line",
        "element.freedraw" => "Freedraw",
        "element.text" => "Text",
        "element.image" => "Image",
        "element.group" => "Group",
        "element.frame" => "Frame",
        "element.magicframe" => "Wireframe to code",
        "element.stickynote" => "Sticky note",
        "element.embeddable" => "Web Embed",
        "element.selection" => "Selection",
        "element.iframe" => "IFrame",
        "labels.unCroppedDimension" => "Uncropped dimension",
        "labels.imageCropping" => "Image cropping",
        other => other,
    }
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

fn panels(app_state: &AppState) -> u32 {
    app_state
        .get("stats")
        .and_then(|s| s.get("panels"))
        .and_then(Value::as_u64)
        .map_or(0, |p| p as u32)
}

/// The patch a section header's `openTrigger` passes to `setAppState`
/// (`index.tsx:201-209, 250-261`): open, with `bit` of the panels flipped.
pub fn toggle_panel(app_state: &AppState, bit: u32) -> Value {
    json!({ "stats": { "open": true, "panels": panels(app_state) ^ bit } })
}

/// `shouldShowStats` (`LayerUI.tsx:305-310`) with the default UI on:
/// open, outside zen and view mode, and not under the element link
/// selector.
pub fn should_show_stats(app_state: &AppState) -> bool {
    truthy(app_state.get("stats").and_then(|s| s.get("open")))
        && !truthy(app_state.get("zenModeEnabled"))
        && !truthy(app_state.get("viewModeEnabled"))
        && app_state
            .get("openDialog")
            .and_then(|d| d.get("name"))
            .and_then(Value::as_str)
            != Some("elementLinkSelector")
}

/// `Number(text)` (ECMA-262 `StringToNumber`): whitespace trimmed, empty
/// is 0, `0x` / `0o` / `0b` integers, `Infinity`, or a decimal literal.
fn string_to_number(text: &str) -> f64 {
    let s = text.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    if s.is_empty() {
        return 0.0;
    }
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if let Some(digits) = s.strip_prefix(prefix) {
            if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                return f64::NAN;
            }
            return digits.chars().fold(0.0, |n, c| {
                n * f64::from(radix) + f64::from(c.to_digit(radix).unwrap())
            });
        }
    }
    let unsigned = s.strip_prefix(['+', '-']).unwrap_or(s);
    if unsigned == "Infinity" {
        return if s.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    // a StrDecimalLiteral: digits, one point, an exponent; Rust's parser
    // also takes "inf" and "nan", which Number does not
    let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
        Some(i) => (&unsigned[..i], Some(&unsigned[i + 1..])),
        None => (unsigned, None),
    };
    let digits_ok = mantissa.chars().filter(|&c| c == '.').count() <= 1
        && mantissa.chars().all(|c| c.is_ascii_digit() || c == '.')
        && mantissa.chars().any(|c| c.is_ascii_digit());
    let exponent_ok = exponent.is_none_or(|e| {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        !e.is_empty() && e.chars().all(|c| c.is_ascii_digit())
    });
    if !digits_ok || !exponent_ok {
        return f64::NAN;
    }
    s.parse().unwrap_or(f64::NAN)
}

/// `DragInput.handleInputValue` (`DragInput.tsx:122-173`) for a typed
/// `text` while the input shows `shown` (`None` for "Mixed"): `Err` when
/// the text is no finite number (the input goes back to the shown value),
/// `Ok(None)` when it is within [`SMALLEST_DELTA`] of the shown value, else
/// the number rounded to two places.
#[allow(clippy::result_unit_err)]
pub fn typed_value(text: &str, shown: Option<f64>) -> Result<Option<f64>, ()> {
    let parsed = string_to_number(text);
    if !parsed.is_finite() {
        return Err(());
    }
    let rounded = excali_scene::display::to_fixed(parsed, 2);
    Ok(match shown {
        Some(original) if (rounded - original).abs() < SMALLEST_DELTA => None,
        _ => Some(rounded),
    })
}

/// `CanvasGrid`'s callback for a typed value (`CanvasGrid.tsx:32-60`): no
/// change for 0, else `getNormalizedGridStep` (`scene/normalize.ts:15-17`).
pub fn canvas_grid_step(next_value: f64) -> Option<f64> {
    if next_value == 0.0 || next_value.is_nan() {
        return None;
    }
    Some(js::round(next_value).clamp(1.0, 100.0))
}

// -- values -----------------------------------------------------------------------

/// `round(value, 2)` of `@excalidraw/math` (`utils.ts:7`).
fn round2(value: f64) -> f64 {
    excali_math::round(value, 2.0, excali_math::RoundingFn::Round)
}

/// `Math.round(value * 100) / 100`.
fn hundredths(value: f64) -> f64 {
    js::round(value * 100.0) / 100.0
}

fn top_left(el: &SceneElement) -> [f64; 2] {
    let b = &el.base;
    let p: GlobalPoint = point_rotate_rads(
        point_from(b.x, b.y),
        point_from(b.x + b.width / 2.0, b.y + b.height / 2.0),
        Radians(b.angle.0),
    );
    [p.x, p.y]
}

fn degrees(el: &SceneElement) -> f64 {
    hundredths(radians_to_degrees(Radians(el.base.angle.0)).0 % 360.0)
}

fn is_in_group(el: &SceneElement) -> bool {
    !el.base.group_ids.is_empty()
}

fn is_image(el: &SceneElement) -> bool {
    el.element_type() == ElementType::Image
}

/// `isPropertyEditable(element, "angle")` (`Stats/utils.ts:45-53`).
fn angle_editable(el: &SceneElement) -> bool {
    !is_frame_like(el)
}

/// `getBaseFontSize` (`stickyNote.ts:405-412`): a sticky note label's
/// `baseFontSize`, else `fontSize`.
fn base_font_size(text: &SceneElement, map: &ElementsMap<'_>) -> Option<f64> {
    let ElementKind::Text(t) = &text.kind else {
        return None;
    };
    let in_note = t
        .container_id
        .as_deref()
        .and_then(|id| map.get(id))
        .is_some_and(|c| c.element_type() == ElementType::StickyNote);
    Some(if in_note {
        t.base_font_size.unwrap_or(t.font_size)
    } else {
        t.font_size
    })
}

/// The text a font size input edits for `el`: itself, or its bound text.
fn font_size_target<'a>(el: &'a SceneElement, map: &ElementsMap<'a>) -> Option<&'a SceneElement> {
    if el.element_type() == ElementType::Text {
        Some(el)
    } else if has_bound_text_element(el) {
        get_bound_text_element(el, map)
    } else {
        None
    }
}

/// A shown value: a number or "Mixed".
#[derive(Clone, Copy, Debug, PartialEq)]
enum Shown {
    Number(f64),
    Mixed,
}

impl Shown {
    fn of(values: &[f64]) -> Shown {
        // new Set(values).size === 1 (SameValueZero)
        match values.first() {
            Some(&first)
                if values
                    .iter()
                    .all(|&v| v == first || (v.is_nan() && first.is_nan())) =>
            {
                Shown::Number(first)
            }
            _ => Shown::Mixed,
        }
    }

    fn text(self) -> String {
        match self {
            Shown::Number(n) => number_to_string(n),
            Shown::Mixed => "Mixed".into(),
        }
    }

    fn number(self) -> Option<f64> {
        match self {
            Shown::Number(n) => Some(n),
            Shown::Mixed => None,
        }
    }
}

/// `getAtomicUnits(targetElements, appState)` (`Stats/utils.ts:235-254`):
/// each selected group's elements among the targets, then each target in
/// no group; ids in order.
fn atomic_units<'a>(targets: &[&'a SceneElement], app_state: &AppState) -> Vec<Vec<&'a str>> {
    let mut units: Vec<Vec<&str>> = app_state
        .get("selectedGroupIds")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter(|(_, v)| truthy(Some(v)))
        .map(|(gid, _)| {
            let mut ids: Vec<&str> = Vec::new();
            for el in targets {
                if el.base.group_ids.iter().any(|g| g == gid) && !ids.contains(&el.base.id.as_str())
                {
                    ids.push(el.base.id.as_str());
                }
            }
            ids
        })
        .collect();
    for el in targets.iter().filter(|el| !is_in_group(el)) {
        units.push(vec![el.base.id.as_str()]);
    }
    units
}

/// The elements of a unit the scene still has (`getElementsInAtomicUnit`,
/// `Stats/utils.ts:60-76`).
fn unit_elements<'a>(unit: &[&str], map: &ElementsMap<'a>) -> Vec<&'a SceneElement> {
    unit.iter().filter_map(|id| map.get(id)).collect()
}

// -- DOM ----------------------------------------------------------------------------

fn icon_node(name: &str) -> Node {
    let icon = icons::icon(name).unwrap_or_else(|| panic!("icons.tsx has no {name}"));
    Node::Element(
        icon.element(Theme::Light)
            .unwrap_or_else(|| panic!("{name} is a path list")),
    )
}

/// `InlineIcon` (`components/InlineIcon.tsx`) at its default size.
fn inline_icon(name: &str) -> Element {
    Element::new("span")
        .style("width", "1em")
        .style("height", "100%")
        // React's `margin: "0 0.5ex 0 0.5ex"`, as the DOM serializes it
        .style("margin", "0px 0.5ex 0px 0.5ex")
        .style("display", "inline-flex")
        .style("line-height", "0")
        .style("vertical-align", "middle")
        .style("flex", "0 0 auto")
        .child(icon_node(name))
}

/// `StatsRow` (`index.tsx:66-93`).
fn stats_row(columns: u32, heading: bool, children: Vec<Node>) -> Element {
    Element::new("div")
        .attr(
            "class",
            class_names([
                ("exc-stats__row", true),
                ("exc-stats__row--heading", heading),
            ]),
        )
        .style("grid-template-columns", format!("repeat({columns}, 1fr)"))
        .children_from(children)
}

fn cell(text: impl Into<String>) -> Node {
    Element::new("div").child(Node::text(text)).into()
}

fn pair(label: &str, value: String) -> Element {
    stats_row(2, false, vec![cell(label), cell(value)])
}

/// `StatsRows` (`index.tsx:95-107`).
fn stats_rows(children: Vec<Element>) -> Element {
    Element::new("div")
        .attr("class", "exc-stats__rows")
        .children_from(children.into_iter().map(Node::from))
}

/// `Collapsible` (`Stats/Collapsible.tsx`): the header and, when open, the
/// children in a column.
fn collapsible(label: &str, open: bool, trigger: Element, children: Vec<Node>) -> Vec<Node> {
    let header = trigger
        .style("cursor", "pointer")
        .style("display", "flex")
        .style("justify-content", "space-between")
        .style("align-items", "center")
        .child(Element::new("h3").child(Node::text(label)))
        .child(inline_icon(if open {
            "collapseUpIcon"
        } else {
            "collapseDownIcon"
        }));
    let mut out = vec![header.into()];
    if open {
        out.push(
            Element::new("div")
                .style("display", "flex")
                .style("flex-direction", "column")
                .children_from(children)
                .into(),
        );
    }
    out
}

/// A `DragInput` (`DragInput.tsx:232-409`); `None` when not editable, as
/// it renders nothing then.
struct DragInput {
    label: &'static str,
    icon: Option<&'static str>,
    property: StatsProperty,
    value: Shown,
    editable: bool,
}

fn drag_input(input: DragInput, on_event: &Option<OnStatsEvent>) -> Option<Element> {
    if !input.editable {
        return None;
    }
    let label = Element::new("div").attr("class", "drag-input-label");
    let label = match input.icon {
        Some(icon) => label.child(inline_icon(icon)),
        None => label.child(Node::text(input.label)),
    };
    let mut field = Element::new("input")
        .attr("class", "drag-input")
        .attr("autocomplete", "off")
        .attr("spellcheck", "false")
        .attr("value", input.value.text());
    if let Some(on_event) = on_event {
        field = bind_input(field, input.property, input.value, on_event.clone());
    }
    Some(
        Element::new("div")
            .attr("class", "drag-input-container")
            .attr("data-testid", input.label)
            .child(label)
            .child(field),
    )
}

/// The input's handlers (`DragInput.tsx:366-406`): a change marks an update
/// pending; Enter applies it and focuses the container; a blur applies it,
/// or restores the shown value when the input is empty; focus selects.
fn bind_input(
    field: Element,
    property: StatsProperty,
    shown: Shown,
    on_event: OnStatsEvent,
) -> Element {
    let pending = Rc::new(Cell::new(false));
    let target = |e: &web_sys::Event| {
        e.current_target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
    };
    let commit = {
        let pending = pending.clone();
        let on_event = on_event.clone();
        Rc::new(move |input: &web_sys::HtmlInputElement| {
            if !pending.replace(false) {
                return;
            }
            match typed_value(&input.value(), shown.number()) {
                Err(()) => input.set_value(&shown.text()),
                Ok(Some(value)) => on_event(StatsEvent::Input { property, value }),
                Ok(None) => {}
            }
        })
    };
    let on_change = pending.clone();
    let on_enter = commit.clone();
    let on_key_event = on_event.clone();
    field
        .on("input", move |_| on_change.set(true))
        .on("keydown", move |e| {
            let Some(input) = target(e) else { return };
            let enter = e
                .dyn_ref::<web_sys::KeyboardEvent>()
                .is_some_and(|k| k.key() == "Enter");
            if enter {
                on_enter(&input);
                on_key_event(StatsEvent::FocusContainer);
            }
        })
        .on("focus", move |e| {
            if let Some(input) = target(e) {
                input.select();
            }
        })
        .on("blur", move |e| {
            let Some(input) = target(e) else { return };
            if input.value().is_empty() {
                input.set_value(&shown.text());
            } else {
                commit(&input);
            }
        })
}

/// `Stats` (`index.tsx:51-440`).
pub fn stats_panel(props: &StatsProps<'_>, on_event: Option<OnStatsEvent>) -> StatsPanel {
    let app_state = props.app_state;
    let elements: Vec<&SceneElement> = props
        .elements
        .iter()
        .filter(|e| !e.base.is_deleted)
        .collect();
    let map = ElementsMap::new(elements.iter().copied());
    // scene.getSelectedElements({selectedElementIds, includeBoundTextElement: false})
    let selected_ids = app_state.get("selectedElementIds");
    let selected: Vec<&SceneElement> = elements
        .iter()
        .copied()
        .filter(|e| truthy(selected_ids.and_then(|ids| ids.get(&e.base.id))))
        .collect();
    let grid_mode = props
        .grid_mode_enabled
        .unwrap_or_else(|| app_state.grid_mode_enabled().unwrap_or(false));
    let panels = panels(app_state);
    let mut controls = Vec::new();
    let mut bind = |el: Element, control: &'static str, event: StatsEvent| {
        controls.push((control, event.clone()));
        match &on_event {
            Some(on_event) => {
                let on_event = on_event.clone();
                el.on("click", move |_| on_event(event.clone()))
            }
            None => el,
        }
    };

    let title = Element::new("div")
        .attr("class", "title")
        .child(Element::new("h2").child(Node::text(stats_text("stats.title"))))
        .child(
            bind(
                Element::new("div").attr("class", "close"),
                "close",
                StatsEvent::Close,
            )
            .child(icon_node("CloseIcon")),
        );

    // the general section
    let [x1, y1, x2, y2] = get_common_bounds(&elements);
    let mut general = vec![
        stats_row(1, true, vec![Node::text(stats_text("stats.scene"))]),
        pair(stats_text("stats.shapes"), elements.len().to_string()),
        pair(
            stats_text("stats.width"),
            number_to_string(js::round(x2) - js::round(x1)),
        ),
        pair(
            stats_text("stats.height"),
            number_to_string(js::round(y2) - js::round(y1)),
        ),
    ];
    if grid_mode {
        general.push(stats_row(1, true, vec![Node::text("Canvas")]));
        general.push(stats_row(
            1,
            false,
            drag_input(
                DragInput {
                    label: "Grid step",
                    icon: None,
                    property: StatsProperty::GridStep,
                    value: Shown::Number(app_state.grid_step().unwrap_or(0.0)),
                    editable: true,
                },
                &on_event,
            )
            .into_iter()
            .map(Node::from)
            .collect(),
        ));
    }
    let general_trigger = bind(
        Element::new("div"),
        "generalStats",
        StatsEvent::TogglePanel(STATS_PANEL_GENERAL_STATS),
    );
    let mut island_children = vec![title.into()];
    island_children.extend(collapsible(
        stats_text("stats.generalStats"),
        panels & STATS_PANEL_GENERAL_STATS != 0,
        general_trigger,
        vec![stats_rows(general).into()],
    ));

    // the element properties section
    if !frame_and_children_selected_together(&selected) && !selected.is_empty() {
        let rows = if selected.len() == 1 {
            single_element_rows(selected[0], app_state, &map, &on_event)
        } else {
            multiple_element_rows(&selected, app_state, &map, &on_event)
        };
        let trigger = bind(
            Element::new("div"),
            "elementProperties",
            StatsEvent::TogglePanel(STATS_PANEL_ELEMENT_PROPERTIES),
        );
        island_children.push(
            Element::new("div")
                .attr("id", "elementStats")
                .style("margin-top", "12px")
                .children_from(collapsible(
                    stats_text("stats.elementProperties"),
                    panels & STATS_PANEL_ELEMENT_PROPERTIES != 0,
                    trigger,
                    vec![stats_rows(rows).into()],
                ))
                .into(),
        );
    }

    let element = Element::new("div").attr("class", "exc-stats").child(island(
        IslandProps {
            padding: Some(3.0),
            ..Default::default()
        },
        island_children,
    ));
    StatsPanel { element, controls }
}

fn input_row(input: DragInput, on_event: &Option<OnStatsEvent>) -> Element {
    stats_row(
        1,
        false,
        drag_input(input, on_event)
            .into_iter()
            .map(Node::from)
            .collect(),
    )
}

/// One element's rows (`index.tsx:264-345`).
fn single_element_rows(
    el: &SceneElement,
    app_state: &AppState,
    map: &ElementsMap<'_>,
    on_event: &Option<OnStatsEvent>,
) -> Vec<Element> {
    let cropping_id = app_state.get("croppingElementId");
    let cropping = truthy(cropping_id);
    let crop_mode = cropping && is_image(el);
    let cropping_this =
        crop_mode && cropping_id.and_then(Value::as_str) == Some(el.base.id.as_str());
    let crop = match &el.kind {
        ElementKind::Image(image) => image.crop.as_ref(),
        _ => None,
    };
    let mut rows = Vec::new();
    if crop_mode {
        let (w, h) = get_uncropped_width_and_height(el);
        rows.push(stats_row(
            1,
            true,
            vec![Node::text(stats_text("labels.unCroppedDimension"))],
        ));
        rows.push(pair(stats_text("stats.width"), number_to_string(round2(w))));
        rows.push(pair(
            stats_text("stats.height"),
            number_to_string(round2(h)),
        ));
    }
    let type_label = if cropping {
        stats_text("labels.imageCropping").to_owned()
    } else {
        stats_text(&format!("element.{}", el.element_type().as_str())).to_owned()
    };
    rows.push(
        stats_row(1, true, vec![Node::text(type_label)])
            .attr("data-testid", "stats-element-type")
            .style("margin", "0.3125rem 0px"),
    );

    // Position (Position.tsx:175-196)
    let [mut x, mut y] = top_left(el).map(round2);
    if cropping_this && crop.is_some() {
        if let Some([cx, cy]) = get_flip_adjusted_crop_position(el, false) {
            x = round2(cx);
            y = round2(cy);
        }
    }
    // Dimension (Dimension.tsx:320-344)
    let (mut w, mut h) = (round2(el.base.width), round2(el.base.height));
    if cropping_this {
        if let Some(crop) = crop {
            let (uw, uh) = get_uncropped_width_and_height(el);
            w = round2(crop.width * (uw / crop.natural_width));
            h = round2(crop.height * (uh / crop.natural_height));
        }
    }
    for (label, property, value) in [
        ("X", StatsProperty::X, x),
        ("Y", StatsProperty::Y, y),
        ("W", StatsProperty::Width, w),
        ("H", StatsProperty::Height, h),
    ] {
        rows.push(input_row(
            DragInput {
                label,
                icon: None,
                property,
                value: Shown::Number(value),
                editable: true,
            },
            on_event,
        ));
    }
    rows.push(input_row(
        DragInput {
            label: "A",
            icon: Some("angleIcon"),
            property: StatsProperty::Angle,
            value: Shown::Number(degrees(el)),
            editable: angle_editable(el),
        },
        on_event,
    ));
    // FontSize (FontSize.tsx:85-111): nothing without text
    let font_size = font_size_target(el, map)
        .and_then(|text| base_font_size(text, map))
        .map(|size| js::round(size * 10.0) / 10.0);
    rows.push(match font_size {
        Some(size) => input_row(
            DragInput {
                label: "F",
                icon: Some("fontSizeIcon"),
                property: StatsProperty::FontSize,
                value: Shown::Number(size),
                editable: true,
            },
            on_event,
        ),
        None => stats_row(1, false, vec![]),
    });
    rows
}

/// Several elements' rows (`index.tsx:348-415`).
fn multiple_element_rows(
    selected: &[&SceneElement],
    app_state: &AppState,
    map: &ElementsMap<'_>,
    on_event: &Option<OnStatsEvent>,
) -> Vec<Element> {
    let mut rows = Vec::new();
    if elements_are_in_same_group(selected) {
        rows.push(stats_row(
            1,
            true,
            vec![Node::text(stats_text("element.group"))],
        ));
    }
    rows.push(
        pair(stats_text("stats.shapes"), selected.len().to_string())
            .style("margin", "0.3125rem 0px"),
    );
    let units: Vec<Vec<&SceneElement>> = atomic_units(selected, app_state)
        .iter()
        .map(|unit| unit_elements(unit, map))
        .filter(|unit| !unit.is_empty())
        .collect();

    // MultiPosition (MultiPosition.tsx:219-258)
    let position = |i: usize| -> Vec<f64> {
        units
            .iter()
            .map(|unit| {
                if unit.len() > 1 {
                    hundredths(get_common_bounds(unit)[i])
                } else {
                    hundredths(top_left(unit[0])[i])
                }
            })
            .collect()
    };
    // MultiDimension (MultiDimension.tsx:462-497)
    let size = |width: bool| -> Vec<f64> {
        units
            .iter()
            .map(|unit| {
                if unit.len() > 1 {
                    let [x1, y1, x2, y2] = get_common_bounds(unit);
                    hundredths(if width { x2 - x1 } else { y2 - y1 })
                } else if width {
                    hundredths(unit[0].base.width)
                } else {
                    hundredths(unit[0].base.height)
                }
            })
            .collect()
    };
    let dimension = |sizes: Vec<f64>| match Shown::of(&sizes) {
        Shown::Number(n) => Shown::Number(hundredths(n)),
        Shown::Mixed => Shown::Mixed,
    };
    let (widths, heights) = (size(true), size(false));
    let editable = !widths.is_empty();
    for (label, property, value, editable) in [
        ("X", StatsProperty::X, Shown::of(&position(0)), true),
        ("Y", StatsProperty::Y, Shown::of(&position(1)), true),
        ("W", StatsProperty::Width, dimension(widths), editable),
        ("H", StatsProperty::Height, dimension(heights), editable),
    ] {
        rows.push(input_row(
            DragInput {
                label,
                icon: None,
                property,
                value,
                editable,
            },
            on_event,
        ));
    }

    // MultiAngle (MultiAngle.tsx:102-133)
    let angles: Vec<f64> = selected
        .iter()
        .filter(|el| !is_in_group(el) && angle_editable(el))
        .map(|el| degrees(el))
        .collect();
    rows.push(input_row(
        DragInput {
            label: "A",
            icon: Some("angleIcon"),
            property: StatsProperty::Angle,
            value: Shown::of(&angles),
            editable: !angles.is_empty(),
        },
        on_event,
    ));

    // MultiFontSize (MultiFontSize.tsx:38-62, 133-164): nothing without text
    let font_sizes: Vec<f64> = selected
        .iter()
        .filter(|el| !is_in_group(el))
        .filter_map(|el| font_size_target(el, map))
        .filter_map(|text| base_font_size(text, map))
        .map(|size| js::round(size * 10.0) / 10.0)
        .collect();
    rows.push(if font_sizes.is_empty() {
        stats_row(1, false, vec![])
    } else {
        input_row(
            DragInput {
                label: "F",
                icon: Some("fontSizeIcon"),
                property: StatsProperty::FontSize,
                value: Shown::of(&font_sizes),
                editable: true,
            },
            on_event,
        )
    });
    rows
}

const STYLESHEET_ID: &str = "stats";

/// Adds [`STATS_CSS`] to the document's head once, after the primitives'
/// stylesheet.
pub fn install_stylesheet(document: &Document) -> Result<(), JsValue> {
    let selector = format!("style[data-excali-ui=\"{STYLESHEET_ID}\"]");
    if document.query_selector(&selector)?.is_some() {
        return Ok(());
    }
    let style = document.create_element("style")?;
    style.set_attribute("data-excali-ui", STYLESHEET_ID)?;
    style.set_text_content(Some(STATS_CSS));
    let head = document
        .head()
        .ok_or_else(|| JsValue::from_str("the document has no head"))?;
    let after = document.query_selector("style[data-excali-ui=\"primitives\"]")?;
    let before = match after {
        Some(p) => p.next_sibling(),
        None => head.first_child(),
    };
    head.insert_before(&style, before.as_ref())?;
    Ok(())
}
