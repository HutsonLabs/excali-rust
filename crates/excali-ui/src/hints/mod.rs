//! Contextual hints: the `HintViewer` under the toolbar
//! (`components/HintViewer.tsx`, `HintViewer.scss`) and the transient
//! `CursorHint` beside the pointer (`components/CursorHint.tsx`,
//! `CursorHint.scss`, `positionElementBesideCursor.ts`), research
//! `ui-design-system.md` §3.11.
//!
//! [`get_hints`] is upstream's `getHints` over a [`HintContext`] (the app
//! state, the selected elements and the few editor facts it reads); it
//! returns the hint keys with their shortcuts, and [`hint_viewer`] renders
//! them as `div.HintViewer > span` with each shortcut in a `<kbd>`, or
//! nothing (hints off with `showHints`, the `DefaultItems` preference, or no
//! hint for the state). [`CursorHints`] is the cursor hint's policy (what to
//! show for an arrow-type cycle or an arrow/line tool shortcut, and the
//! cooldown), [`position_element_beside_cursor`] where it goes and
//! [`cursor_hint`] its DOM; the host hides it after
//! [`CURSOR_HINT_DURATION`] plus [`CURSOR_HINT_FADE_DURATION`] ms, or on a
//! pointerdown. `tests/hints.rs` holds all of it to
//! `tests/fixtures/hints.json` (`tools/goldens/hints.mjs`). The shared
//! `Tooltip` of §3.11 is [`crate::primitives::tooltip`].

use excali_core::app_state::AppState;
use excali_core::element::{Element, ElementType};
use excali_editor::actions::{get_shortcut_key, KeyLabels};
use excali_editor::tools::{ArrowType, KeyHintKind, ToolType};
use excali_scene::display::number_to_string;
use excali_scene::shape::Theme;
use serde_json::Value;

use crate::dom::{class_names, Element as DomElement, Node};
use crate::icons::{self, Icon};

/// Upstream's `HintViewer.scss` and `CursorHint.scss`, compiled (generated
/// by `tools/goldens/hints.mjs`).
pub const HINTS_CSS: &str = include_str!("hints.css");

/// The `data-excali-ui` value of the `<style>` holding [`HINTS_CSS`].
const STYLESHEET_ID: &str = "hints";

/// Adds [`HINTS_CSS`] to `document`'s head once, right after the
/// primitives' stylesheet ([`crate::primitives::install_stylesheet`], which
/// it installs first), so the host's own rules still come later and win.
pub fn install_stylesheet(document: &web_sys::Document) -> Result<(), wasm_bindgen::JsValue> {
    crate::primitives::install_stylesheet(document)?;
    let selector = format!("style[data-excali-ui=\"{STYLESHEET_ID}\"]");
    if document.query_selector(&selector)?.is_some() {
        return Ok(());
    }
    let style = document.create_element("style")?;
    style.set_attribute("data-excali-ui", STYLESHEET_ID)?;
    style.set_text_content(Some(HINTS_CSS));
    let head = document
        .head()
        .ok_or_else(|| wasm_bindgen::JsValue::from_str("the document has no head"))?;
    let primitives = document.query_selector("style[data-excali-ui=\"primitives\"]")?;
    let next = primitives.and_then(|p| p.next_sibling());
    head.insert_before(&style, next.as_ref())?;
    Ok(())
}

/// The English strings of the hints (`locales/en.json` `hints.*` and
/// `keys.mmb`); the key itself for any other.
pub fn hint_text(key: &str) -> &str {
    match key {
        "hints.dismissSearch" => "{{shortcut}} to dismiss search",
        "hints.canvasPanning" => "To move canvas, hold {{shortcut_1}} or {{shortcut_2}} while dragging, or use the hand tool",
        "hints.linearElement" => "Click to start multiple points, drag for single line",
        "hints.arrowTool" => "Click to start multiple points, drag for single line. Press {{shortcut}} again to change arrow type.",
        "hints.arrowBindModifiers" => "Hold {{shortcut_1}} to disable binding, or {{shortcut_2}} to bind at a fixed point",
        "hints.freeDraw" => "Click and drag, release when you're finished",
        "hints.text" => "Tip: you can also add text by double-clicking anywhere with the selection tool",
        "hints.embeddable" => "Click-drag to create a website embed",
        "hints.stickynote" => "Click to place a note, or drag to size it",
        "hints.autoshape" => "Draw rectangles, circles, diamonds, lines and arrows",
        "hints.text_selected" => "Double-click or press {{shortcut}} to edit text",
        "hints.text_editing" => "Press {{shortcut_1}} or {{shortcut_2}} to finish editing",
        "hints.linearElementMulti" => "Click on last point or press {{shortcut_1}} or {{shortcut_2}} to finish",
        "hints.lockAngle" => "You can constrain angle by holding {{shortcut}}",
        "hints.resize" => "You can constrain proportions by holding {{shortcut_1}} while resizing,\nhold {{shortcut_2}} to resize from the center",
        "hints.resizeImage" => "You can resize freely by holding {{shortcut_1}},\nhold {{shortcut_2}} to resize from the center",
        "hints.resizeStickyNote" => "hold {{shortcut_1}} to resize freely, {{shortcut_2}} to resize from the center",
        "hints.rotate" => "You can constrain angles by holding {{shortcut}} while rotating",
        "hints.toggleArrowhead" => "Double-click to toggle the arrowhead",
        "hints.lineEditor_info" => "Hold {{shortcut_1}} and Double-click or press {{shortcut_2}} to edit points",
        "hints.lineEditor_line_info" => "Double-click or press {{shortcut}} to edit points",
        "hints.lineEditor_pointSelected" => "Press {{shortcut_1}} to remove point(s),\n{{shortcut_2}} to duplicate, or drag to move",
        "hints.lineEditor_nothingSelected" => "Select a point to edit (hold {{shortcut_1}} to select multiple),\nor hold {{shortcut_2}} and click to add new points",
        "hints.publishLibrary" => "Publish your own library",
        "hints.bindTextToElement" => "{{shortcut}} to add text",
        "hints.createFlowchart" => "{{shortcut}} to create a flowchart",
        "hints.deepBoxSelect" => "Hold {{shortcut}} to deep select, and to prevent dragging",
        "hints.eraserRevert" => "Hold {{shortcut}} to revert the elements marked for deletion",
        "hints.firefox_clipboard_write" => "This feature can likely be enabled by setting the \"dom.events.asyncClipboard.clipboardItem\" flag to \"true\". To change the browser flags in Firefox, visit the \"about:config\" page.",
        "hints.disableSnapping" => "Hold {{shortcut}} to disable snapping",
        "hints.enterCropEditor" => "Double click the image or press {{shortcut}} to crop the image",
        "hints.leaveCropEditor" => "Click outside the image or press {{shortcut_1}} or {{shortcut_2}} to finish cropping",
        "keys.mmb" => "Scroll wheel",
        other => other,
    }
}

/// What `getHints` reads besides the app state: the selected elements
/// (`app.scene.getSelectedElements(appState)`), `isMobile`,
/// `editorInterface.canFitSidebar`, the host's `gridModeEnabled` prop
/// (`isGridModeEnabled`, `snapping.ts:159-160`) and the transform handle
/// being dragged (`app.activeResizeHandle`).
#[derive(Debug, Clone, Copy)]
pub struct HintContext<'a> {
    pub app_state: &'a AppState,
    pub selected_elements: &'a [Element],
    pub is_mobile: bool,
    pub can_fit_sidebar: bool,
    /// `props.gridModeEnabled`; `None` defers to the app state's.
    pub grid_mode_enabled: Option<bool>,
    pub active_resize_handle: Option<&'a str>,
}

/// A shortcut `getTaggedShortcutKey` tags: keys joined with ` + `.
pub type Shortcut = &'static [&'static str];

/// One hint: the `hints.*` key (without the prefix) and its
/// `{{placeholder}}` shortcuts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint {
    pub key: &'static str,
    pub shortcuts: Vec<(&'static str, Shortcut)>,
}

impl Hint {
    fn plain(key: &'static str) -> Hint {
        Hint {
            key,
            shortcuts: Vec::new(),
        }
    }

    fn one(key: &'static str, shortcut: Shortcut) -> Hint {
        Hint {
            key,
            shortcuts: vec![("shortcut", shortcut)],
        }
    }

    fn two(key: &'static str, first: Shortcut, second: Shortcut) -> Hint {
        Hint {
            key,
            shortcuts: vec![("shortcut_1", first), ("shortcut_2", second)],
        }
    }

    /// `t("hints.<key>", {...})` with each shortcut as
    /// `getTaggedShortcutKey` writes it (`<kbd>Ctrl + Enter</kbd>`).
    pub fn message(&self, is_darwin: bool) -> String {
        let mut out = hint_text(&format!("hints.{}", self.key)).to_owned();
        for (name, keys) in &self.shortcuts {
            let tagged = format!(
                "<kbd>{}</kbd>",
                keys.iter()
                    .map(|k| get_shortcut_key(k, is_darwin, &KeyLabels::EN))
                    .collect::<Vec<_>>()
                    .join(" + ")
            );
            out = out.replacen(&format!("{{{{{name}}}}}"), &tagged, 1);
        }
        out
    }
}

/// `t("keys.mmb")`, the middle mouse button (`HintViewer.tsx:195`).
const MMB: Shortcut = &["Scroll wheel"];

fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

fn is_linear(el: &Element) -> bool {
    matches!(
        el.kind.element_type(),
        ElementType::Line | ElementType::Arrow
    )
}

fn point_count(el: &Element) -> usize {
    el.kind.points().map_or(0, <[_]>::len)
}

/// `isTextBindableContainer(element)` with `includeLocked` true
/// (`typeChecks.ts:240-253`).
fn is_text_bindable_container(el: &Element) -> bool {
    matches!(
        el.kind.element_type(),
        ElementType::Rectangle
            | ElementType::StickyNote
            | ElementType::Diamond
            | ElementType::Ellipse
            | ElementType::Arrow
    )
}

/// `isFlowchartNodeElement(element)` (`typeChecks.ts:286-295`).
fn is_flowchart_node(el: &Element) -> bool {
    matches!(
        el.kind.element_type(),
        ElementType::Rectangle
            | ElementType::StickyNote
            | ElementType::Ellipse
            | ElementType::Diamond
    )
}

/// `getHints` (`HintViewer.tsx:40-250`): the hints for the state, in
/// order; empty for none. Upstream returns one string or, for a flowchart
/// node, two, which [`hint_viewer`] joins.
pub fn get_hints(ctx: &HintContext<'_>) -> Vec<Hint> {
    get_hint_list(ctx).unwrap_or_default()
}

fn get_hint_list(ctx: &HintContext<'_>) -> Option<Vec<Hint>> {
    let state = ctx.app_state;
    let get = |k: &str| state.get(k);
    let field = |k: &str, f: &str| state.get(k).and_then(|v| v.get(f));
    let tool = field("activeTool", "type")
        .and_then(Value::as_str)
        .unwrap_or("");
    let multi_mode = get("multiElement") != Some(&Value::Null);
    let mouse = get("lastPointerDownWith").and_then(Value::as_str) == Some("mouse");
    let one = |key: &'static str, s: Shortcut| Some(vec![Hint::one(key, s)]);
    let two = |key: &'static str, a: Shortcut, b: Shortcut| Some(vec![Hint::two(key, a, b)]);
    let plain = |key: &'static str| Some(vec![Hint::plain(key)]);

    // DEFAULT_SIDEBAR.name, CANVAS_SEARCH_TAB
    let sidebar = get("openSidebar");
    if field("openSidebar", "name").and_then(Value::as_str) == Some("default")
        && field("openSidebar", "tab").and_then(Value::as_str) == Some("search")
        && field("searchMatches", "matches")
            .and_then(Value::as_array)
            .is_some_and(|m| !m.is_empty())
    {
        return one("dismissSearch", &["Escape"]);
    }
    if truthy(sidebar) && !ctx.can_fit_sidebar {
        return None;
    }
    if tool == "eraser" {
        return one("eraserRevert", &["Alt"]);
    }

    let selected = ctx.selected_elements;
    let first = selected.first();
    let first_type = first.map(|e| e.kind.element_type());

    if truthy(field("selectedLinearElement", "isDragging"))
        && first_type == Some(ElementType::Arrow)
    {
        return two("arrowBindModifiers", &["Ctrl"], &["Alt"]);
    }

    if tool == "arrow" || tool == "line" {
        if multi_mode {
            return two("linearElementMulti", &["Escape"], &["Enter"]);
        }
        if tool == "arrow" {
            return one("arrowTool", &["A"]);
        }
        return plain("linearElement");
    }
    match tool {
        "freedraw" => return plain("freeDraw"),
        "text" => return plain("text"),
        "embeddable" => return plain("embeddable"),
        "stickynote" => return plain("stickynote"),
        "autoshape" => return plain("autoshape"),
        _ => {}
    }

    if truthy(get("isResizing")) && mouse && selected.len() == 1 {
        let target = &selected[0];
        if is_linear(target) && point_count(target) == 2 {
            return one("lockAngle", &["Shift"]);
        }
        // a note's corners are proportional by default (Shift frees them);
        // its edges are free by default, so they get the generic hint
        if target.kind.element_type() == ElementType::StickyNote
            && ctx
                .active_resize_handle
                .is_some_and(|h| h.encode_utf16().count() == 2)
        {
            return two("resizeStickyNote", &["Shift"], &["Alt"]);
        }
        if target.kind.element_type() == ElementType::Image {
            return two("resizeImage", &["Shift"], &["Alt"]);
        }
        return two("resize", &["Shift"], &["Alt"]);
    }

    if truthy(get("isRotating")) && mouse {
        return one("rotate", &["Shift"]);
    }
    if selected.len() == 1 && first_type == Some(ElementType::Text) {
        return one("text_selected", &["Enter"]);
    }
    if truthy(get("editingTextElement")) {
        return two("text_editing", &["Escape"], &["CtrlOrCmd", "Enter"]);
    }
    if truthy(get("croppingElementId")) {
        return two("leaveCropEditor", &["Enter"], &["Escape"]);
    }
    if selected.len() == 1 && first_type == Some(ElementType::Image) {
        return one("enterCropEditor", &["Enter"]);
    }

    if tool == "selection" {
        if truthy(get("selectionElement"))
            && selected.is_empty()
            && !truthy(get("editingTextElement"))
            && !truthy(field("selectedLinearElement", "isEditing"))
        {
            return one("deepBoxSelect", &["CtrlOrCmd"]);
        }
        let grid_mode = match ctx.grid_mode_enabled {
            Some(on) => on,
            None => truthy(get("gridModeEnabled")),
        };
        if grid_mode && truthy(get("selectedElementsAreBeingDragged")) {
            return one("disableSnapping", &["CtrlOrCmd"]);
        }
        if selected.is_empty() && !ctx.is_mobile {
            return two("canvasPanning", MMB, &["Space"]);
        }
        if selected.len() == 1 {
            let el = &selected[0];
            if is_linear(el) {
                let linear_for_el = field("selectedLinearElement", "elementId")
                    .and_then(Value::as_str)
                    == Some(el.base.id.as_str());
                let hover =
                    field("selectedLinearElement", "hoverPointIndex").and_then(Value::as_f64);
                let last = point_count(el) as f64 - 1.0;
                if el.kind.element_type() == ElementType::Arrow
                    && linear_for_el
                    && (hover == Some(0.0) || hover == Some(last))
                {
                    return plain("toggleArrowhead");
                }
                if truthy(field("selectedLinearElement", "isEditing")) {
                    return if truthy(field("selectedLinearElement", "selectedPointsIndices")) {
                        two("lineEditor_pointSelected", &["Delete"], &["CtrlOrCmd", "D"])
                    } else {
                        two("lineEditor_nothingSelected", &["Shift"], &["Alt"])
                    };
                }
                return if el.kind.element_type() == ElementType::Line {
                    one("lineEditor_line_info", &["Enter"])
                } else {
                    two("lineEditor_info", &["CtrlOrCmd"], &["CtrlOrCmd", "Enter"])
                };
            }
            if !truthy(get("newElement"))
                && !truthy(get("selectedElementsAreBeingDragged"))
                && is_text_bindable_container(el)
            {
                let bind = Hint::one("bindTextToElement", &["Enter"]);
                if is_flowchart_node(el) {
                    // upstream returns the same pair whether or not the
                    // node is already in a flowchart
                    return Some(vec![
                        bind,
                        Hint::one("createFlowchart", &["CtrlOrCmd", "↑↓"]),
                    ]);
                }
                return Some(vec![bind]);
            }
        }
    }
    None
}

/// The hint text `HintViewer` shows: one hint's message, or several with
/// each one's trailing period dropped (`/\. ?$/`) joined by `, `.
pub fn hint_message(hints: &[Hint], is_darwin: bool) -> Option<String> {
    match hints {
        [] => None,
        [one] => Some(one.message(is_darwin)),
        many => Some(
            many.iter()
                .map(|h| {
                    let m = h.message(is_darwin);
                    let trimmed = m
                        .strip_suffix(". ")
                        .or_else(|| m.strip_suffix('.'))
                        .unwrap_or(&m);
                    trimmed.to_owned()
                })
                .collect::<Vec<_>>()
                .join(", "),
        ),
    }
}

/// `HintViewer` (`HintViewer.tsx:252-309`): `div.HintViewer > span` with
/// the message's `<kbd>` parts as elements; `None` with `showHints` off or
/// no hint.
pub fn hint_viewer(ctx: &HintContext<'_>, is_darwin: bool) -> Option<DomElement> {
    if !truthy(ctx.app_state.get("showHints")) {
        return None;
    }
    let message = hint_message(&get_hints(ctx), is_darwin)?;
    let mut span = DomElement::new("span");
    // hint.split(/(<kbd>[^<]+<\/kbd>)/g)
    let mut rest = message.as_str();
    loop {
        let found = rest.find("<kbd>").and_then(|start| {
            let inner = &rest[start + 5..];
            let end = inner.find('<')?;
            (end > 0 && inner[end..].starts_with("</kbd>")).then_some((start, end))
        });
        match found {
            Some((start, end)) => {
                span = span.child(Node::text(&rest[..start]));
                let key = &rest[start + 5..start + 5 + end];
                span = span.child(DomElement::new("kbd").child(Node::text(key)));
                rest = &rest[start + 5 + end + 6..];
            }
            None => {
                span = span.child(Node::text(rest));
                break;
            }
        }
    }
    Some(
        DomElement::new("div")
            .attr("class", "HintViewer")
            .child(span),
    )
}

// -- cursor hint --------------------------------------------------------------

/// How long the cursor hint stays before it fades (`CURSOR_HINT_DURATION`).
pub const CURSOR_HINT_DURATION: f64 = 700.0;
/// Its fade-out (`CURSOR_HINT_FADE_DURATION`, the `transition` of
/// `CursorHint.scss`).
pub const CURSOR_HINT_FADE_DURATION: f64 = 100.0;
/// Its distance from the pointer (`CURSOR_HINT_GAP`).
pub const CURSOR_HINT_GAP: f64 = 16.0;
/// While a hint shown this recently is fresh, letter tool shortcuts show
/// none (`CURSOR_HINT_COOLDOWN`).
pub const CURSOR_HINT_COOLDOWN: f64 = 30.0 * 1000.0;

/// The container's bounding client rect.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContainerRect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

fn position_axis(cursor: f64, size: f64, container: f64, gap: f64) -> f64 {
    // flip to the other side of the cursor when overflowing the container
    let position = if cursor + gap + size > container {
        cursor - gap - size
    } else {
        cursor + gap
    };
    // clamp(position, 0, max(0, container - size))
    position.max(0.0).min((container - size).max(0.0))
}

/// `positionElementBesideCursor` (`positionElementBesideCursor.ts`): the
/// container-local `(left, top)` of an element of `size` beside a cursor
/// at client `cursor`, each axis flipped when it would overflow.
pub fn position_element_beside_cursor(
    cursor: (f64, f64),
    size: (f64, f64),
    container: ContainerRect,
    gap: f64,
) -> (f64, f64) {
    (
        position_axis(cursor.0 - container.left, size.0, container.width, gap),
        position_axis(cursor.1 - container.top, size.1, container.height, gap),
    )
}

/// `CursorHints` (`CursorHint.tsx:47-101`): what the cursor hint shows for
/// the keyboard events the app reports. `now` is `Date.now()` and
/// `last_position` the viewport's last pointer position, `(0, 0)` until
/// the first pointermove (no hint then).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CursorHints {
    last_shown_at: f64,
}

/// `getArrowTypeIcon`.
fn arrow_type_icon(arrow_type: ArrowType) -> &'static Icon {
    match arrow_type {
        ArrowType::Elbow => &icons::elbowArrowIcon,
        ArrowType::Round => &icons::roundArrowIcon,
        ArrowType::Sharp => &icons::sharpArrowIcon,
    }
}

impl CursorHints {
    fn show(
        &mut self,
        icon: &'static Icon,
        now: f64,
        last_position: (f64, f64),
    ) -> Option<&'static Icon> {
        if last_position == (0.0, 0.0) {
            return None;
        }
        self.last_shown_at = now;
        Some(icon)
    }

    fn is_on_cooldown(&self, now: f64) -> bool {
        now - self.last_shown_at < CURSOR_HINT_COOLDOWN
    }

    /// The arrow type cycled with its shortcut: always hinted.
    pub fn on_arrow_type_cycled(
        &mut self,
        arrow_type: ArrowType,
        now: f64,
        last_position: (f64, f64),
    ) -> Option<&'static Icon> {
        self.show(arrow_type_icon(arrow_type), now, last_position)
    }

    /// The arrow or line tool picked with its shortcut: digits always
    /// hint, letters only off cooldown. `arrow_type` is
    /// `currentItemArrowType`.
    pub fn on_tool_shortcut(
        &mut self,
        tool: ToolType,
        source: KeyHintKind,
        arrow_type: ArrowType,
        now: f64,
        last_position: (f64, f64),
    ) -> Option<&'static Icon> {
        if source == KeyHintKind::Digit || !self.is_on_cooldown(now) {
            let icon = if tool == ToolType::Line {
                &icons::LineIcon
            } else {
                arrow_type_icon(arrow_type)
            };
            return self.show(icon, now, last_position);
        }
        None
    }
}

/// `CursorHint` (`CursorHint.tsx:110-209`): `div.CursorHint` holding the
/// icon at `translate(left, top)`, with `CursorHint--fade-out` once
/// [`CURSOR_HINT_DURATION`] has passed.
pub fn cursor_hint(icon: &Icon, fading: bool, left: f64, top: f64) -> DomElement {
    let mut el = DomElement::new("div")
        .attr(
            "class",
            class_names([("CursorHint", true), ("CursorHint--fade-out", fading)]),
        )
        .attr("data-testid", "cursor-hint")
        .style(
            "transform",
            format!(
                "translate({}px, {}px)",
                number_to_string(left),
                number_to_string(top)
            ),
        );
    if let Some(svg) = icon.element(Theme::Light) {
        el = el.child(svg);
    }
    el
}
