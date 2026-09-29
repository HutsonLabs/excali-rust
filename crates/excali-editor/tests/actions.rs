//! The actions registry, pinned to upstream at the pinned commit:
//!
//! - `packages/excalidraw/actions/types.ts:45-144` (the 99 `ActionName`s)
//!   and every `register()` call under `packages/excalidraw/actions/`
//!   (label, keywords, icon, `keyPriority`, `viewMode`, `navigation`,
//!   `trackEvent`, `predicate`, `keyTest`, `PanelComponent`, `checked`);
//! - `actions/manager.tsx:92-243` (`handleKeyDown`, `executeAction`,
//!   `renderAction`, `isActionEnabled`) and `trackAction` (:22-51);
//! - `actions/shortcuts.ts` (`getShortcutFromShortcutName`) and
//!   `shortcut.ts` (`getShortcutKey`);
//! - `components/App.tsx:13835-13936` (`getContextMenuItems`) and
//!   `components/ContextMenu.tsx:37-120` (filtering and separators); the
//!   cases of `tests/contextmenu.test.tsx` and
//!   `actions/actionElementLock.test.tsx` are ported;
//! - `components/CommandPalette/CommandPalette.tsx:87-114, 290-420, 672-689`
//!   (categories and the commands built from actions);
//! - `components/Actions.tsx:63-217` (the full styles panel);
//! - `components/LayerUI.tsx:111-136` and
//!   `components/main-menu/DefaultItems.tsx` (the default main menu).
//!
//! See `site/content/research/ui-design-system.md` sections 3 and 5.

use std::collections::BTreeSet;

use excali_core::app_state::AppState;
use excali_core::element::{
    ArrowFields, BoundElement, BoundElementType, Element, ElementBase, ElementKind, FontFamily,
    FrameFields, ImageFields, LineFields, LinearFields, TextFields,
};
use excali_editor::actions::{
    build_context_menu, commands_from_actions, default_main_menu, full_styles_panel,
    get_context_menu_items, get_shortcut_from_shortcut_name, get_shortcut_key,
    palette_command_available, render_styles_panel, track_action, ActionContext, ActionEnv,
    ActionIcon, ActionLabel, ActionManager, ActionName, ActionSource, AppProps, CanvasActions,
    ContextMenuEntry, ContextMenuItem, ContextMenuKind, FormFactor, KeyDownOutcome, KeyEvent,
    KeyLabels, MainMenuEntry, MainMenuItem, PaletteCategory, PaletteCommandSource, PanelFieldset,
    PanelGate, SHORTCUT_NAMES,
};
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn element(kind: ElementKind, id: &str) -> Element {
    let mut base = ElementBase::new(id, 0.0, 0.0, 1.0, 1.0);
    base.width = 100.0;
    base.height = 100.0;
    Element::new(base, kind)
}

fn rect(id: &str) -> Element {
    element(ElementKind::Rectangle, id)
}

fn ellipse(id: &str) -> Element {
    element(ElementKind::Ellipse, id)
}

fn text(id: &str) -> Element {
    element(
        ElementKind::Text(TextFields::new("hello", FontFamily::default(), 1.25)),
        id,
    )
}

fn bound_text(id: &str, container: &str) -> Element {
    let mut t = TextFields::new("label", FontFamily::default(), 1.25);
    t.container_id = Some(container.to_owned());
    element(ElementKind::Text(t), id)
}

fn with_bound_text(mut container: Element, text_id: &str) -> Element {
    container.base.bound_elements = Some(vec![BoundElement {
        id: text_id.to_owned(),
        kind: BoundElementType::Text,
    }]);
    container
}

fn arrow(id: &str, elbowed: bool) -> Element {
    element(
        ElementKind::Arrow(ArrowFields::new(
            LinearFields::new(vec![[0.0, 0.0], [100.0, 100.0]]),
            elbowed,
        )),
        id,
    )
}

fn line(id: &str, points: usize, polygon: bool) -> Element {
    let pts = (0..points).map(|i| [i as f64 * 10.0, 0.0]).collect();
    element(
        ElementKind::Line(LineFields {
            linear: LinearFields::new(pts),
            polygon,
        }),
        id,
    )
}

fn image(id: &str) -> Element {
    element(ElementKind::Image(ImageFields::default()), id)
}

fn frame(id: &str) -> Element {
    element(ElementKind::Frame(FrameFields { name: None }), id)
}

fn embeddable(id: &str) -> Element {
    element(ElementKind::Embeddable, id)
}

fn grouped(mut e: Element, groups: &[&str]) -> Element {
    e.base.group_ids = groups.iter().map(|g| g.to_string()).collect();
    e
}

fn locked(mut e: Element) -> Element {
    e.base.locked = true;
    e
}

fn in_frame(mut e: Element, frame_id: &str) -> Element {
    e.base.frame_id = Some(frame_id.to_owned());
    e
}

fn deleted(mut e: Element) -> Element {
    e.base.is_deleted = true;
    e
}

/// The default app state with `patch`'s keys set.
fn state(patch: Value) -> AppState {
    let mut s = AppState::default();
    if let Value::Object(map) = patch {
        for (k, v) in map {
            s.insert(k, v);
        }
    }
    s
}

/// `selectedElementIds` for `ids`.
fn selecting(ids: &[&str]) -> Value {
    let map: serde_json::Map<String, Value> =
        ids.iter().map(|id| (id.to_string(), json!(true))).collect();
    Value::Object(map)
}

struct Fixture {
    elements: Vec<Element>,
    app_state: AppState,
    props: AppProps,
    env: ActionEnv,
}

impl Fixture {
    fn new(elements: Vec<Element>, patch: Value) -> Fixture {
        Fixture {
            elements,
            app_state: state(patch),
            props: AppProps::default(),
            env: ActionEnv::default(),
        }
    }

    fn selected(elements: Vec<Element>, ids: &[&str]) -> Fixture {
        Fixture::new(elements, json!({ "selectedElementIds": selecting(ids) }))
    }

    fn ctx(&self) -> ActionContext<'_> {
        ActionContext {
            elements: &self.elements,
            app_state: &self.app_state,
            props: &self.props,
            env: &self.env,
        }
    }

    fn enabled(&self, name: ActionName) -> bool {
        ActionManager::new().is_action_enabled(name, &self.ctx())
    }

    fn key_down(&self, event: &KeyEvent<'_>) -> KeyDownOutcome {
        ActionManager::new().handle_key_down(event, &self.ctx())
    }
}

/// A keydown. `mods` is a `+`-separated list of `ctrl`, `meta`, `shift`
/// and `alt`.
fn ev<'a>(key: &'a str, code: &'a str, mods: &str) -> KeyEvent<'a> {
    let has = |m: &str| mods.split('+').any(|x| x == m);
    KeyEvent {
        key,
        code,
        shift_key: has("shift"),
        alt_key: has("alt"),
        ctrl_key: has("ctrl"),
        meta_key: has("meta"),
        target_is_writable: false,
    }
}

fn n(name: &str) -> ActionName {
    ActionName::from_name(name).unwrap_or_else(|| panic!("unknown action {name}"))
}

// ---------------------------------------------------------------------------
// ActionName (types.ts:45-144)
// ---------------------------------------------------------------------------

/// The `ActionName` union, in declaration order.
const UPSTREAM_NAMES: [&str; 99] = [
    "copy",
    "cut",
    "paste",
    "copyAsPng",
    "copyAsSvg",
    "copyText",
    "sendBackward",
    "bringForward",
    "sendToBack",
    "bringToFront",
    "copyStyles",
    "selectAll",
    "pasteStyles",
    "gridMode",
    "zenMode",
    "objectsSnapMode",
    "arrowBinding",
    "midpointSnapping",
    "stats",
    "changeStrokeColor",
    "changeBackgroundColor",
    "changeBucketFillBackgroundColor",
    "changeFillStyle",
    "changeStrokeWidth",
    "changeSloppiness",
    "changeFreedrawMode",
    "changeStrokeStyle",
    "changeArrowhead",
    "changeArrowType",
    "changeArrowProperties",
    "changeOpacity",
    "changeFontSize",
    "undo",
    "redo",
    "finalize",
    "changeProjectName",
    "changeExportBackground",
    "changeExportEmbedScene",
    "changeExportScale",
    "saveToActiveFile",
    "saveFileToDisk",
    "loadScene",
    "duplicateSelection",
    "deleteSelectedElements",
    "changeViewBackgroundColor",
    "clearCanvas",
    "zoomIn",
    "zoomOut",
    "resetZoom",
    "zoomToFit",
    "zoomToFitSelection",
    "zoomToFitSelectionInViewport",
    "changeFontFamily",
    "changeTextAlign",
    "changeVerticalAlign",
    "toggleFullScreen",
    "toggleShortcuts",
    "group",
    "ungroup",
    "goToCollaborator",
    "addToLibrary",
    "changeRoundness",
    "alignTop",
    "alignBottom",
    "alignLeft",
    "alignRight",
    "alignVerticallyCentered",
    "alignHorizontallyCentered",
    "distributeHorizontally",
    "distributeVertically",
    "flipHorizontal",
    "flipVertical",
    "deselect",
    "viewMode",
    "exportWithDarkMode",
    "toggleTheme",
    "increaseFontSize",
    "decreaseFontSize",
    "unbindText",
    "hyperlink",
    "bindText",
    "unlockAllElements",
    "toggleElementLock",
    "toggleLinearEditor",
    "selectAllElementsInFrame",
    "removeAllElementsFromFrame",
    "updateFrameRendering",
    "createContainerFromText",
    "wrapTextInContainer",
    "commandPalette",
    "autoResize",
    "elementStats",
    "searchMenu",
    "copyElementLink",
    "linkToElement",
    "cropEditor",
    "wrapSelectionInFrame",
    "toggleShapeSwitch",
    "togglePolygon",
];

#[test]
fn action_names_are_the_upstream_union_in_order() {
    let names: Vec<&str> = ActionName::ALL.iter().map(|a| a.as_str()).collect();
    assert_eq!(names, UPSTREAM_NAMES);
    for name in UPSTREAM_NAMES {
        assert_eq!(ActionName::from_name(name).map(|a| a.as_str()), Some(name));
    }
    assert_eq!(ActionName::from_name("CopyAsPng"), None);
    assert_eq!(ActionName::from_name("unknown"), None);
}

#[test]
fn ninety_five_actions_are_registered() {
    let registered: Vec<ActionName> = ActionName::ALL
        .into_iter()
        .filter(|a| a.spec().is_registered())
        .collect();
    assert_eq!(registered.len(), 95);
    let unregistered: Vec<&str> = ActionName::ALL
        .into_iter()
        .filter(|a| !a.spec().is_registered())
        .map(|a| a.as_str())
        .collect();
    assert_eq!(
        unregistered,
        [
            "toggleFullScreen",
            "createContainerFromText",
            "commandPalette",
            "elementStats"
        ]
    );
    for a in ActionName::ALL {
        let spec = a.spec();
        assert_eq!(spec.name, a);
        if !spec.is_registered() {
            assert!(spec.label.is_none());
            assert!(spec.key_test.is_none());
            assert!(spec.predicate.is_none());
            assert!(!spec.has_panel);
        }
    }
    let manager = ActionManager::new();
    assert_eq!(manager.registered().count(), 95);
    assert!(manager.is_registered(ActionName::Undo));
    assert!(!manager.is_registered(ActionName::CommandPalette));
}

// ---------------------------------------------------------------------------
// Every register() call (actions/*.ts{,x})
// ---------------------------------------------------------------------------

struct Row {
    name: &'static str,
    file: &'static str,
    line: u32,
    /// `None`: the label is a function of the state.
    label: Option<&'static str>,
    keywords: &'static [&'static str],
    /// A static icon export; `None` for no icon or a function of the state.
    icon: Option<&'static str>,
    view_mode: Option<bool>,
    navigation: bool,
    key_priority: i32,
    track: Option<(&'static str, Option<&'static str>)>,
    predicate: bool,
    key_test: bool,
    panel: bool,
    checked: bool,
}

/// Read off each `register({...})` (and `createUndoAction` /
/// `createRedoAction`) at the pinned commit; `line` is the `name:` line.
#[rustfmt::skip]
const ROWS: &[Row] = &[
    Row { name: "addToLibrary", file: "actionAddToLibrary.ts", line: 11, label: Some("labels.addToLibrary"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: false, key_test: false, panel: false, checked: false },
    Row { name: "alignTop", file: "actionAlign.tsx", line: 80, label: Some("labels.alignTop"), keywords: &[], icon: Some("AlignTopIcon"), view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: true, key_test: true, panel: true, checked: false },
    Row { name: "alignBottom", file: "actionAlign.tsx", line: 114, label: Some("labels.alignBottom"), keywords: &[], icon: Some("AlignBottomIcon"), view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: true, key_test: true, panel: true, checked: false },
    Row { name: "alignLeft", file: "actionAlign.tsx", line: 148, label: Some("labels.alignLeft"), keywords: &[], icon: Some("AlignLeftIcon"), view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: true, key_test: true, panel: true, checked: false },
    Row { name: "alignRight", file: "actionAlign.tsx", line: 182, label: Some("labels.alignRight"), keywords: &[], icon: Some("AlignRightIcon"), view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: true, key_test: true, panel: true, checked: false },
    Row { name: "alignVerticallyCentered", file: "actionAlign.tsx", line: 216, label: Some("labels.centerVertically"), keywords: &[], icon: Some("CenterVerticallyIcon"), view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: true, key_test: false, panel: true, checked: false },
    Row { name: "alignHorizontallyCentered", file: "actionAlign.tsx", line: 246, label: Some("labels.centerHorizontally"), keywords: &[], icon: Some("CenterHorizontallyIcon"), view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: true, key_test: false, panel: true, checked: false },
    Row { name: "unbindText", file: "actionBoundText.tsx", line: 61, label: Some("labels.unbindText"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: true, key_test: false, panel: false, checked: false },
    Row { name: "bindText", file: "actionBoundText.tsx", line: 125, label: Some("labels.bindText"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: true, key_test: false, panel: false, checked: false },
    Row { name: "wrapTextInContainer", file: "actionBoundText.tsx", line: 259, label: Some("labels.createContainerFromText"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: true, key_test: false, panel: false, checked: false },
    Row { name: "changeViewBackgroundColor", file: "actionCanvas.tsx", line: 48, label: Some("labels.canvasBackground"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: None, predicate: true, key_test: false, panel: true, checked: false },
    Row { name: "clearCanvas", file: "actionCanvas.tsx", line: 85, label: Some("labels.clearCanvas"), keywords: &[], icon: Some("TrashIcon"), view_mode: None, navigation: false, key_priority: 0, track: Some(("canvas", None)), predicate: true, key_test: false, panel: false, checked: false },
    Row { name: "zoomIn", file: "actionCanvas.tsx", line: 130, label: Some("buttons.zoomIn"), keywords: &[], icon: Some("ZoomInIcon"), view_mode: Some(true), navigation: true, key_priority: 0, track: Some(("canvas", None)), predicate: true, key_test: true, panel: true, checked: false },
    Row { name: "zoomOut", file: "actionCanvas.tsx", line: 182, label: Some("buttons.zoomOut"), keywords: &[], icon: Some("ZoomOutIcon"), view_mode: Some(true), navigation: true, key_priority: 0, track: Some(("canvas", None)), predicate: true, key_test: true, panel: true, checked: false },
    Row { name: "resetZoom", file: "actionCanvas.tsx", line: 234, label: Some("buttons.resetZoom"), keywords: &[], icon: Some("ZoomResetIcon"), view_mode: Some(true), navigation: true, key_priority: 0, track: Some(("canvas", None)), predicate: true, key_test: true, panel: true, checked: false },
    Row { name: "zoomToFitSelectionInViewport", file: "actionCanvas.tsx", line: 308, label: Some("labels.zoomToFitViewport"), keywords: &[], icon: Some("zoomAreaIcon"), view_mode: Some(true), navigation: true, key_priority: 0, track: Some(("canvas", None)), predicate: true, key_test: true, panel: false, checked: false },
    Row { name: "zoomToFitSelection", file: "actionCanvas.tsx", line: 351, label: Some("helpDialog.zoomToSelection"), keywords: &[], icon: Some("zoomAreaIcon"), view_mode: Some(true), navigation: true, key_priority: 0, track: Some(("canvas", None)), predicate: true, key_test: true, panel: false, checked: false },
    Row { name: "zoomToFit", file: "actionCanvas.tsx", line: 393, label: Some("helpDialog.zoomToFit"), keywords: &[], icon: Some("zoomAreaIcon"), view_mode: Some(true), navigation: true, key_priority: 0, track: Some(("canvas", None)), predicate: true, key_test: true, panel: false, checked: false },
    Row { name: "toggleTheme", file: "actionCanvas.tsx", line: 429, label: None, keywords: &["toggle", "dark", "light", "mode", "theme"], icon: None, view_mode: Some(true), navigation: false, key_priority: 0, track: Some(("canvas", None)), predicate: true, key_test: true, panel: false, checked: false },
    Row { name: "copy", file: "actionClipboard.tsx", line: 24, label: Some("labels.copy"), keywords: &[], icon: Some("DuplicateIcon"), view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: false, key_test: false, panel: false, checked: false },
    Row { name: "paste", file: "actionClipboard.tsx", line: 56, label: Some("labels.paste"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: false, key_test: false, panel: false, checked: false },
    Row { name: "cut", file: "actionClipboard.tsx", line: 113, label: Some("labels.cut"), keywords: &[], icon: Some("cutIcon"), view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: false, key_test: true, panel: false, checked: false },
    Row { name: "copyAsSvg", file: "actionClipboard.tsx", line: 125, label: Some("labels.copyAsSvg"), keywords: &["svg", "clipboard", "copy"], icon: Some("svgIcon"), view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: true, key_test: false, panel: false, checked: false },
    Row { name: "copyAsPng", file: "actionClipboard.tsx", line: 193, label: Some("labels.copyAsPng"), keywords: &["png", "clipboard", "copy"], icon: Some("pngIcon"), view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: true, key_test: true, panel: false, checked: false },
    Row { name: "copyText", file: "actionClipboard.tsx", line: 255, label: Some("labels.copyText"), keywords: &["text", "clipboard", "copy"], icon: None, view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: true, key_test: false, panel: false, checked: false },
    Row { name: "cropEditor", file: "actionCropEditor.tsx", line: 14, label: Some("helpDialog.cropStart"), keywords: &["image", "crop"], icon: Some("cropIcon"), view_mode: Some(true), navigation: false, key_priority: 0, track: Some(("menu", None)), predicate: true, key_test: false, panel: true, checked: false },
    Row { name: "deleteSelectedElements", file: "actionDeleteSelected.tsx", line: 209, label: Some("labels.delete"), keywords: &[], icon: Some("TrashIcon"), view_mode: None, navigation: false, key_priority: 0, track: Some(("element", Some("delete"))), predicate: false, key_test: true, panel: true, checked: false },
    Row { name: "deselect", file: "actionDeselect.ts", line: 65, label: Some(""), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: None, predicate: false, key_test: true, panel: false, checked: false },
    Row { name: "distributeHorizontally", file: "actionDistribute.tsx", line: 74, label: Some("labels.distributeHorizontally"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: false, key_test: true, panel: true, checked: false },
    Row { name: "distributeVertically", file: "actionDistribute.tsx", line: 105, label: Some("labels.distributeVertically"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: false, key_test: true, panel: true, checked: false },
    Row { name: "duplicateSelection", file: "actionDuplicateSelection.tsx", line: 35, label: Some("labels.duplicateSelection"), keywords: &[], icon: Some("DuplicateIcon"), view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: false, key_test: true, panel: true, checked: false },
    Row { name: "copyElementLink", file: "actionElementLink.ts", line: 17, label: Some("labels.copyElementLink"), keywords: &[], icon: Some("copyIcon"), view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: true, key_test: false, panel: false, checked: false },
    Row { name: "linkToElement", file: "actionElementLink.ts", line: 74, label: Some("labels.linkToElement"), keywords: &[], icon: Some("elementLinkIcon"), view_mode: None, navigation: false, key_priority: 0, track: None, predicate: true, key_test: false, panel: false, checked: false },
    Row { name: "toggleElementLock", file: "actionElementLock.ts", line: 26, label: None, keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: true, key_test: true, panel: false, checked: false },
    Row { name: "unlockAllElements", file: "actionElementLock.ts", line: 161, label: Some("labels.elementLock.unlockAll"), keywords: &[], icon: Some("UnlockedIcon"), view_mode: Some(false), navigation: false, key_priority: 0, track: Some(("canvas", None)), predicate: true, key_test: false, panel: false, checked: false },
    Row { name: "changeProjectName", file: "actionExport.tsx", line: 38, label: Some("labels.fileTitle"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: None, predicate: false, key_test: false, panel: true, checked: false },
    Row { name: "changeExportScale", file: "actionExport.tsx", line: 58, label: Some("imageExportDialog.scale"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: Some(("export", Some("scale"))), predicate: false, key_test: false, panel: false, checked: false },
    Row { name: "changeExportBackground", file: "actionExport.tsx", line: 72, label: Some("imageExportDialog.label.withBackground"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: Some(("export", Some("toggleBackground"))), predicate: false, key_test: false, panel: true, checked: false },
    Row { name: "changeExportEmbedScene", file: "actionExport.tsx", line: 94, label: Some("imageExportDialog.tooltip.embedScene"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: Some(("export", Some("embedScene"))), predicate: false, key_test: false, panel: true, checked: false },
    Row { name: "saveToActiveFile", file: "actionExport.tsx", line: 254, label: Some("buttons.save"), keywords: &[], icon: Some("ExportIcon"), view_mode: None, navigation: false, key_priority: 0, track: Some(("export", None)), predicate: true, key_test: true, panel: false, checked: false },
    Row { name: "saveFileToDisk", file: "actionExport.tsx", line: 329, label: Some("exportDialog.disk_title"), keywords: &[], icon: Some("ExportIcon"), view_mode: Some(true), navigation: false, key_priority: 0, track: Some(("export", None)), predicate: false, key_test: true, panel: true, checked: false },
    Row { name: "loadScene", file: "actionExport.tsx", line: 394, label: Some("buttons.load"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: Some(("export", None)), predicate: true, key_test: true, panel: false, checked: false },
    Row { name: "exportWithDarkMode", file: "actionExport.tsx", line: 434, label: Some("imageExportDialog.label.darkMode"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: Some(("export", Some("toggleTheme"))), predicate: false, key_test: false, panel: true, checked: false },
    Row { name: "finalize", file: "actionFinalize.tsx", line: 54, label: Some(""), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: None, predicate: false, key_test: true, panel: true, checked: false },
    Row { name: "flipHorizontal", file: "actionFlip.ts", line: 30, label: Some("labels.flipHorizontal"), keywords: &[], icon: Some("flipHorizontal"), view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: false, key_test: true, panel: false, checked: false },
    Row { name: "flipVertical", file: "actionFlip.ts", line: 55, label: Some("labels.flipVertical"), keywords: &[], icon: Some("flipVertical"), view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: false, key_test: true, panel: false, checked: false },
    Row { name: "selectAllElementsInFrame", file: "actionFrame.ts", line: 37, label: Some("labels.selectAllElementsInFrame"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: Some(("canvas", None)), predicate: true, key_test: false, panel: false, checked: false },
    Row { name: "removeAllElementsFromFrame", file: "actionFrame.ts", line: 74, label: Some("labels.removeAllElementsFromFrame"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: Some(("history", None)), predicate: true, key_test: false, panel: false, checked: false },
    Row { name: "updateFrameRendering", file: "actionFrame.ts", line: 105, label: Some("labels.updateFrameRendering"), keywords: &[], icon: None, view_mode: Some(true), navigation: false, key_priority: 0, track: Some(("canvas", None)), predicate: false, key_test: false, panel: false, checked: true },
    Row { name: "wrapSelectionInFrame", file: "actionFrame.ts", line: 126, label: Some("labels.wrapSelectionInFrame"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: true, key_test: false, panel: false, checked: false },
    Row { name: "group", file: "actionGroup.tsx", line: 87, label: Some("labels.group"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: true, key_test: true, panel: true, checked: false },
    Row { name: "ungroup", file: "actionGroup.tsx", line: 218, label: Some("labels.ungroup"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: true, key_test: true, panel: true, checked: false },
    Row { name: "undo", file: "actionHistory.tsx", line: 70, label: Some("buttons.undo"), keywords: &[], icon: Some("UndoIcon"), view_mode: Some(false), navigation: false, key_priority: 0, track: Some(("history", None)), predicate: false, key_test: true, panel: true, checked: false },
    Row { name: "redo", file: "actionHistory.tsx", line: 109, label: Some("buttons.redo"), keywords: &[], icon: Some("RedoIcon"), view_mode: Some(false), navigation: false, key_priority: 0, track: Some(("history", None)), predicate: false, key_test: true, panel: true, checked: false },
    Row { name: "toggleLinearEditor", file: "actionLinearEditor.tsx", line: 28, label: None, keywords: &["line"], icon: None, view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: true, key_test: false, panel: true, checked: false },
    Row { name: "togglePolygon", file: "actionLinearEditor.tsx", line: 107, label: None, keywords: &["loop"], icon: Some("polygonIcon"), view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: true, key_test: false, panel: true, checked: false },
    Row { name: "hyperlink", file: "actionLink.tsx", line: 20, label: None, keywords: &[], icon: Some("LinkIcon"), view_mode: None, navigation: false, key_priority: 0, track: Some(("hyperlink", Some("click"))), predicate: true, key_test: true, panel: true, checked: false },
    Row { name: "toggleShortcuts", file: "actionMenu.tsx", line: 10, label: Some("welcomeScreen.defaults.helpHint"), keywords: &[], icon: Some("HelpIconThin"), view_mode: Some(true), navigation: false, key_priority: 0, track: Some(("menu", Some("toggleHelpDialog"))), predicate: false, key_test: true, panel: false, checked: false },
    Row { name: "goToCollaborator", file: "actionNavigate.tsx", line: 22, label: Some("Go to a collaborator"), keywords: &[], icon: None, view_mode: Some(true), navigation: false, key_priority: 0, track: Some(("collab", None)), predicate: false, key_test: false, panel: true, checked: false },
    Row { name: "changeStrokeColor", file: "actionProperties.tsx", line: 361, label: Some("labels.stroke"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: None, predicate: false, key_test: false, panel: true, checked: false },
    Row { name: "changeBackgroundColor", file: "actionProperties.tsx", line: 440, label: Some("labels.changeBackground"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: None, predicate: false, key_test: false, panel: true, checked: false },
    Row { name: "changeBucketFillBackgroundColor", file: "actionProperties.tsx", line: 556, label: Some("labels.changeBackground"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: None, predicate: false, key_test: false, panel: true, checked: false },
    Row { name: "changeFillStyle", file: "actionProperties.tsx", line: 607, label: Some("labels.fill"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: None, predicate: false, key_test: false, panel: true, checked: false },
    Row { name: "changeStrokeWidth", file: "actionProperties.tsx", line: 709, label: Some("labels.strokeWidth"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: None, predicate: false, key_test: false, panel: true, checked: false },
    Row { name: "changeSloppiness", file: "actionProperties.tsx", line: 767, label: Some("labels.sloppiness"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: None, predicate: false, key_test: false, panel: true, checked: false },
    Row { name: "changeFreedrawMode", file: "actionProperties.tsx", line: 821, label: Some("labels.pressure"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: None, predicate: false, key_test: false, panel: true, checked: false },
    Row { name: "changeStrokeStyle", file: "actionProperties.tsx", line: 904, label: Some("labels.strokeStyle"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: None, predicate: false, key_test: false, panel: true, checked: false },
    Row { name: "changeOpacity", file: "actionProperties.tsx", line: 957, label: Some("labels.opacity"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: None, predicate: false, key_test: false, panel: true, checked: false },
    Row { name: "changeFontSize", file: "actionProperties.tsx", line: 1001, label: Some("labels.fontSize"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: None, predicate: false, key_test: false, panel: true, checked: false },
    Row { name: "decreaseFontSize", file: "actionProperties.tsx", line: 1096, label: Some("labels.decreaseFontSize"), keywords: &[], icon: Some("fontSizeIcon"), view_mode: None, navigation: false, key_priority: 0, track: None, predicate: false, key_test: true, panel: false, checked: false },
    Row { name: "increaseFontSize", file: "actionProperties.tsx", line: 1121, label: Some("labels.increaseFontSize"), keywords: &[], icon: Some("fontSizeIcon"), view_mode: None, navigation: false, key_priority: 0, track: None, predicate: false, key_test: true, panel: false, checked: false },
    Row { name: "changeFontFamily", file: "actionProperties.tsx", line: 1164, label: Some("labels.fontFamily"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: None, predicate: false, key_test: false, panel: true, checked: false },
    Row { name: "changeTextAlign", file: "actionProperties.tsx", line: 1548, label: Some("Change text alignment"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: None, predicate: false, key_test: false, panel: true, checked: false },
    Row { name: "changeVerticalAlign", file: "actionProperties.tsx", line: 1649, label: Some("Change vertical alignment"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: false, key_test: false, panel: true, checked: false },
    Row { name: "changeRoundness", file: "actionProperties.tsx", line: 1749, label: Some("Change edge roundness"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: None, predicate: false, key_test: false, panel: true, checked: false },
    Row { name: "changeArrowhead", file: "actionProperties.tsx", line: 1948, label: Some("Change arrowheads"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: None, predicate: false, key_test: false, panel: true, checked: false },
    Row { name: "changeArrowProperties", file: "actionProperties.tsx", line: 2039, label: Some("Change arrow properties"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: None, predicate: false, key_test: false, panel: true, checked: false },
    Row { name: "changeArrowType", file: "actionProperties.tsx", line: 2058, label: Some("Change arrow types"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: None, predicate: false, key_test: false, panel: true, checked: false },
    Row { name: "selectAll", file: "actionSelectAll.ts", line: 22, label: Some("labels.selectAll"), keywords: &[], icon: Some("selectAllIcon"), view_mode: Some(false), navigation: false, key_priority: 0, track: Some(("canvas", None)), predicate: false, key_test: true, panel: false, checked: false },
    Row { name: "copyStyles", file: "actionStyles.ts", line: 52, label: Some("labels.copyStyles"), keywords: &[], icon: Some("paintIcon"), view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: false, key_test: true, panel: false, checked: false },
    Row { name: "pasteStyles", file: "actionStyles.ts", line: 83, label: Some("labels.pasteStyles"), keywords: &[], icon: Some("paintIcon"), view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: false, key_test: true, panel: false, checked: false },
    Row { name: "autoResize", file: "actionTextAutoResize.ts", line: 23, label: Some("labels.autoResize"), keywords: &[], icon: None, view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: true, key_test: false, panel: false, checked: false },
    Row { name: "arrowBinding", file: "actionToggleArrowBinding.tsx", line: 6, label: Some("labels.arrowBinding"), keywords: &[], icon: None, view_mode: Some(false), navigation: false, key_priority: 0, track: Some(("canvas", None)), predicate: false, key_test: false, panel: false, checked: true },
    Row { name: "gridMode", file: "actionToggleGridMode.tsx", line: 10, label: Some("labels.toggleGrid"), keywords: &["snap"], icon: Some("gridIcon"), view_mode: Some(true), navigation: false, key_priority: 0, track: Some(("canvas", None)), predicate: true, key_test: true, panel: false, checked: true },
    Row { name: "midpointSnapping", file: "actionToggleMidpointSnapping.tsx", line: 6, label: Some("labels.midpointSnapping"), keywords: &[], icon: None, view_mode: Some(false), navigation: false, key_priority: 0, track: Some(("canvas", None)), predicate: false, key_test: false, panel: false, checked: true },
    Row { name: "objectsSnapMode", file: "actionToggleObjectsSnapMode.tsx", line: 10, label: Some("buttons.objectsSnapMode"), keywords: &[], icon: Some("magnetIcon"), view_mode: Some(false), navigation: false, key_priority: 0, track: Some(("canvas", None)), predicate: true, key_test: true, panel: false, checked: true },
    Row { name: "searchMenu", file: "actionToggleSearchMenu.ts", line: 15, label: Some("search.title"), keywords: &["search", "find"], icon: Some("searchIcon"), view_mode: Some(true), navigation: false, key_priority: 0, track: Some(("search_menu", Some("toggle"))), predicate: true, key_test: true, panel: false, checked: true },
    Row { name: "toggleShapeSwitch", file: "actionToggleShapeSwitch.tsx", line: 14, label: Some("labels.shapeSwitch"), keywords: &["change", "switch", "swap"], icon: None, view_mode: Some(true), navigation: false, key_priority: 0, track: Some(("shape_switch", Some("toggle"))), predicate: true, key_test: false, panel: false, checked: true },
    Row { name: "stats", file: "actionToggleStats.tsx", line: 10, label: Some("stats.fullTitle"), keywords: &["edit", "attributes", "customize"], icon: Some("abacusIcon"), view_mode: Some(true), navigation: false, key_priority: 0, track: Some(("menu", None)), predicate: false, key_test: true, panel: false, checked: true },
    Row { name: "viewMode", file: "actionToggleViewMode.tsx", line: 10, label: Some("labels.viewMode"), keywords: &[], icon: Some("eyeIcon"), view_mode: Some(true), navigation: false, key_priority: 0, track: Some(("canvas", None)), predicate: true, key_test: true, panel: false, checked: true },
    Row { name: "zenMode", file: "actionToggleZenMode.tsx", line: 10, label: Some("buttons.zenMode"), keywords: &[], icon: Some("coffeeIcon"), view_mode: Some(true), navigation: false, key_priority: 0, track: Some(("canvas", None)), predicate: true, key_test: true, panel: false, checked: true },
    Row { name: "sendBackward", file: "actionZindex.tsx", line: 24, label: Some("labels.sendBackward"), keywords: &["move down", "zindex", "layer"], icon: Some("SendBackwardIcon"), view_mode: None, navigation: false, key_priority: 40, track: Some(("element", None)), predicate: false, key_test: true, panel: true, checked: false },
    Row { name: "bringForward", file: "actionZindex.tsx", line: 54, label: Some("labels.bringForward"), keywords: &["move up", "zindex", "layer"], icon: Some("BringForwardIcon"), view_mode: None, navigation: false, key_priority: 40, track: Some(("element", None)), predicate: false, key_test: true, panel: true, checked: false },
    Row { name: "sendToBack", file: "actionZindex.tsx", line: 84, label: Some("labels.sendToBack"), keywords: &["move down", "zindex", "layer"], icon: Some("SendToBackIcon"), view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: false, key_test: true, panel: true, checked: false },
    Row { name: "bringToFront", file: "actionZindex.tsx", line: 121, label: Some("labels.bringToFront"), keywords: &["move up", "zindex", "layer"], icon: Some("BringToFrontIcon"), view_mode: None, navigation: false, key_priority: 0, track: Some(("element", None)), predicate: false, key_test: true, panel: true, checked: false },
];

/// Actions whose icon is a function of the state (or a component taking
/// the theme); the rest have a static icon or none.
const DYNAMIC_ICONS: [&str; 4] = ["toggleTheme", "toggleElementLock", "group", "ungroup"];

#[test]
fn every_registered_action_matches_its_register_call() {
    assert_eq!(ROWS.len(), 95);
    let names: BTreeSet<&str> = ROWS.iter().map(|r| r.name).collect();
    assert_eq!(names.len(), 95);
    for row in ROWS {
        let spec = n(row.name).spec();
        let ctx_note = row.name;
        assert!(spec.is_registered(), "{ctx_note}");
        assert_eq!(spec.source, Some((row.file, row.line)), "{ctx_note}");
        match (row.label, &spec.label) {
            (Some(key), Some(ActionLabel::Key(k))) => assert_eq!(key, *k, "{ctx_note}"),
            (None, Some(ActionLabel::Dynamic(_))) => {}
            (expected, got) => panic!("{ctx_note}: label {expected:?} vs {got:?}"),
        }
        assert_eq!(spec.keywords, row.keywords, "{ctx_note}");
        match (row.icon, &spec.icon) {
            (Some(icon), ActionIcon::Static(i)) => assert_eq!(icon, *i, "{ctx_note}"),
            (None, ActionIcon::None) => assert!(!DYNAMIC_ICONS.contains(&row.name)),
            (None, ActionIcon::Static(_) | ActionIcon::Dynamic(_)) => {
                assert!(DYNAMIC_ICONS.contains(&row.name), "{ctx_note}")
            }
            (expected, got) => panic!("{ctx_note}: icon {expected:?} vs {got:?}"),
        }
        assert_eq!(spec.view_mode, row.view_mode, "{ctx_note}");
        assert_eq!(spec.navigation, row.navigation, "{ctx_note}");
        assert_eq!(spec.key_priority, row.key_priority, "{ctx_note}");
        assert_eq!(
            spec.track_event.as_ref().map(|t| (t.category, t.action)),
            row.track,
            "{ctx_note}"
        );
        assert_eq!(spec.predicate.is_some(), row.predicate, "{ctx_note}");
        assert_eq!(spec.key_test.is_some(), row.key_test, "{ctx_note}");
        assert_eq!(spec.has_panel, row.panel, "{ctx_note}");
        assert_eq!(spec.checked.is_some(), row.checked, "{ctx_note}");
    }
}

// ---------------------------------------------------------------------------
// keyTest: the research table (ui-design-system.md section 5)
// ---------------------------------------------------------------------------

/// A shortcut from the research table as the browser reports it: `key`,
/// `code` and the modifiers, with `cmd` standing for the platform's
/// CTRL_OR_CMD (metaKey on a Mac, ctrlKey elsewhere).
struct Shortcut {
    action: &'static str,
    key: &'static str,
    code: &'static str,
    mods: &'static str,
}

#[rustfmt::skip]
const SHORTCUTS: &[Shortcut] = &[
    Shortcut { action: "alignTop", key: "ArrowUp", code: "ArrowUp", mods: "cmd+shift" },
    Shortcut { action: "alignBottom", key: "ArrowDown", code: "ArrowDown", mods: "cmd+shift" },
    Shortcut { action: "alignLeft", key: "ArrowLeft", code: "ArrowLeft", mods: "cmd+shift" },
    Shortcut { action: "alignRight", key: "ArrowRight", code: "ArrowRight", mods: "cmd+shift" },
    Shortcut { action: "zoomIn", key: "=", code: "Equal", mods: "cmd" },
    Shortcut { action: "zoomIn", key: "+", code: "Equal", mods: "shift" },
    Shortcut { action: "zoomIn", key: "+", code: "NumpadAdd", mods: "cmd" },
    Shortcut { action: "zoomOut", key: "-", code: "Minus", mods: "cmd" },
    Shortcut { action: "zoomOut", key: "_", code: "Minus", mods: "shift" },
    Shortcut { action: "zoomOut", key: "-", code: "NumpadSubtract", mods: "cmd" },
    Shortcut { action: "resetZoom", key: "0", code: "Digit0", mods: "cmd" },
    Shortcut { action: "resetZoom", key: "0", code: "Numpad0", mods: "cmd" },
    Shortcut { action: "zoomToFitSelectionInViewport", key: "@", code: "Digit2", mods: "shift" },
    Shortcut { action: "zoomToFitSelection", key: "#", code: "Digit3", mods: "shift" },
    Shortcut { action: "zoomToFit", key: "!", code: "Digit1", mods: "shift" },
    Shortcut { action: "toggleTheme", key: "D", code: "KeyD", mods: "alt+shift" },
    Shortcut { action: "cut", key: "x", code: "KeyX", mods: "cmd" },
    Shortcut { action: "copyAsPng", key: "C", code: "KeyC", mods: "alt+shift" },
    Shortcut { action: "deleteSelectedElements", key: "Backspace", code: "Backspace", mods: "" },
    Shortcut { action: "deleteSelectedElements", key: "Delete", code: "Delete", mods: "" },
    Shortcut { action: "deselect", key: "Escape", code: "Escape", mods: "" },
    Shortcut { action: "distributeHorizontally", key: "h", code: "KeyH", mods: "alt" },
    Shortcut { action: "distributeVertically", key: "v", code: "KeyV", mods: "alt" },
    Shortcut { action: "duplicateSelection", key: "d", code: "KeyD", mods: "cmd" },
    Shortcut { action: "toggleElementLock", key: "L", code: "KeyL", mods: "cmd+shift" },
    Shortcut { action: "saveToActiveFile", key: "s", code: "KeyS", mods: "cmd" },
    Shortcut { action: "saveFileToDisk", key: "S", code: "KeyS", mods: "cmd+shift" },
    Shortcut { action: "loadScene", key: "o", code: "KeyO", mods: "cmd" },
    Shortcut { action: "finalize", key: "Enter", code: "Enter", mods: "" },
    Shortcut { action: "flipHorizontal", key: "H", code: "KeyH", mods: "shift" },
    Shortcut { action: "flipVertical", key: "V", code: "KeyV", mods: "shift" },
    Shortcut { action: "group", key: "g", code: "KeyG", mods: "cmd" },
    Shortcut { action: "ungroup", key: "G", code: "KeyG", mods: "cmd+shift" },
    Shortcut { action: "undo", key: "z", code: "KeyZ", mods: "cmd" },
    Shortcut { action: "redo", key: "Z", code: "KeyZ", mods: "cmd+shift" },
    Shortcut { action: "redo", key: "y", code: "KeyY", mods: "cmd" },
    Shortcut { action: "hyperlink", key: "k", code: "KeyK", mods: "cmd" },
    Shortcut { action: "toggleShortcuts", key: "?", code: "Slash", mods: "shift" },
    Shortcut { action: "decreaseFontSize", key: "<", code: "Comma", mods: "cmd+shift" },
    Shortcut { action: "decreaseFontSize", key: ",", code: "Comma", mods: "cmd+shift" },
    Shortcut { action: "increaseFontSize", key: ">", code: "Period", mods: "cmd+shift" },
    Shortcut { action: "increaseFontSize", key: ".", code: "Period", mods: "cmd+shift" },
    Shortcut { action: "selectAll", key: "a", code: "KeyA", mods: "cmd" },
    Shortcut { action: "copyStyles", key: "c", code: "KeyC", mods: "cmd+alt" },
    Shortcut { action: "pasteStyles", key: "v", code: "KeyV", mods: "cmd+alt" },
    Shortcut { action: "gridMode", key: "'", code: "Quote", mods: "cmd" },
    Shortcut { action: "objectsSnapMode", key: "s", code: "KeyS", mods: "alt" },
    Shortcut { action: "searchMenu", key: "f", code: "KeyF", mods: "cmd" },
    Shortcut { action: "stats", key: "/", code: "Slash", mods: "alt" },
    Shortcut { action: "viewMode", key: "r", code: "KeyR", mods: "alt" },
    Shortcut { action: "zenMode", key: "z", code: "KeyZ", mods: "alt" },
    Shortcut { action: "sendBackward", key: "[", code: "BracketLeft", mods: "cmd" },
    Shortcut { action: "bringForward", key: "]", code: "BracketRight", mods: "cmd" },
];

/// Resolve `cmd` for the platform.
fn platform_mods(mods: &str, darwin: bool) -> String {
    mods.split('+')
        .map(|m| match m {
            "cmd" if darwin => "meta",
            "cmd" => "ctrl",
            other => other,
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// A state under which the action's `keyTest` can pass: a selection for
/// the lock toggle and deselect, a multi-point element for finalize.
fn fixture_for(action: &str, darwin: bool) -> Fixture {
    let mut f = match action {
        "toggleElementLock" | "deselect" => Fixture::selected(vec![rect("a")], &["a"]),
        "finalize" => Fixture::new(vec![arrow("m", false)], json!({ "multiElement": {"id": "m"} })),
        _ => Fixture::new(vec![], json!({})),
    };
    f.env.is_darwin = darwin;
    f.props.canvas_actions.toggle_theme = Some(true);
    f
}

#[test]
fn every_research_table_shortcut_runs_its_action() {
    for darwin in [false, true] {
        for s in SHORTCUTS {
            let mods = platform_mods(s.mods, darwin);
            let event = ev(s.key, s.code, &mods);
            let f = fixture_for(s.action, darwin);
            let action = n(s.action);
            let key_test = action.spec().key_test.expect(s.action);
            assert!(
                key_test(&event, &f.ctx()),
                "{} {} ({mods}) darwin={darwin}",
                s.action,
                s.code
            );
            assert_eq!(
                f.key_down(&event),
                KeyDownOutcome::Perform(action),
                "{} {} ({mods}) darwin={darwin}",
                s.action,
                s.code
            );
        }
    }
}

#[test]
fn the_shortcut_table_covers_every_key_test() {
    let with_key_test: BTreeSet<&str> = ActionName::ALL
        .into_iter()
        .filter(|a| a.spec().key_test.is_some())
        .map(|a| a.as_str())
        .collect();
    let mut covered: BTreeSet<&str> = SHORTCUTS.iter().map(|s| s.action).collect();
    // Their shortcuts differ by platform; see `z_order_extremes_by_platform`.
    covered.insert("sendToBack");
    covered.insert("bringToFront");
    assert_eq!(with_key_test, covered);
    // copy and paste run from the native copy/paste events, not a keyTest.
    assert!(ActionName::Copy.spec().key_test.is_none());
    assert!(ActionName::Paste.spec().key_test.is_none());
}

#[test]
fn z_order_extremes_by_platform() {
    // Elsewhere: Ctrl+Shift+[ / ].
    let f = fixture_for("sendToBack", false);
    let back = ev("{", "BracketLeft", "ctrl+shift");
    let front = ev("}", "BracketRight", "ctrl+shift");
    assert_eq!(f.key_down(&back), KeyDownOutcome::Perform(ActionName::SendToBack));
    assert_eq!(f.key_down(&front), KeyDownOutcome::Perform(ActionName::BringToFront));
    // Ctrl+Alt+[ is not a shortcut off a Mac.
    let alt = ev("[", "BracketLeft", "ctrl+alt");
    assert!(!(ActionName::SendToBack.spec().key_test.unwrap())(&alt, &f.ctx()));

    // On a Mac: Cmd+Alt+[ / ]. sendBackward's keyTest (Cmd, no Shift,
    // BracketLeft) passes too, and handleKeyDown gives up when more than one
    // action matches (manager.tsx:108-113) whatever the keyPriority.
    let f = fixture_for("sendToBack", true);
    let back = ev("[", "BracketLeft", "meta+alt");
    assert!((ActionName::SendToBack.spec().key_test.unwrap())(&back, &f.ctx()));
    assert!((ActionName::SendBackward.spec().key_test.unwrap())(&back, &f.ctx()));
    assert_eq!(
        f.key_down(&back),
        KeyDownOutcome::Ambiguous(vec![ActionName::SendBackward, ActionName::SendToBack])
    );
    let front = ev("]", "BracketRight", "meta+alt");
    assert!((ActionName::BringToFront.spec().key_test.unwrap())(&front, &f.ctx()));
    assert_eq!(
        f.key_down(&front),
        KeyDownOutcome::Ambiguous(vec![ActionName::BringForward, ActionName::BringToFront])
    );
    // Cmd+Shift+[ is not sendToBack on a Mac.
    let shift = ev("{", "BracketLeft", "meta+shift");
    assert!(!(ActionName::SendToBack.spec().key_test.unwrap())(&shift, &f.ctx()));
}

#[test]
fn ctrl_or_cmd_is_meta_on_a_mac_and_ctrl_elsewhere() {
    let f = fixture_for("selectAll", false);
    assert_eq!(f.key_down(&ev("a", "KeyA", "meta")), KeyDownOutcome::Unhandled);
    let f = fixture_for("selectAll", true);
    assert_eq!(f.key_down(&ev("a", "KeyA", "ctrl")), KeyDownOutcome::Unhandled);
    assert_eq!(
        f.key_down(&ev("a", "KeyA", "meta")),
        KeyDownOutcome::Perform(ActionName::SelectAll)
    );
}

fn passes(action: ActionName, event: &KeyEvent<'_>, f: &Fixture) -> bool {
    (action.spec().key_test.unwrap())(event, &f.ctx())
}

#[test]
fn key_tests_reject_near_misses() {
    let f = fixture_for("", false);
    let cases: &[(ActionName, KeyEvent<'_>)] = &[
        // Delete only without Ctrl: Ctrl+Delete is the clear-canvas confirm.
        (ActionName::DeleteSelectedElements, ev("Delete", "Delete", "ctrl")),
        (ActionName::DeleteSelectedElements, ev("Backspace", "Backspace", "ctrl")),
        (ActionName::Group, ev("G", "KeyG", "ctrl+shift")),
        (ActionName::Ungroup, ev("g", "KeyG", "ctrl")),
        (ActionName::Undo, ev("Z", "KeyZ", "ctrl+shift")),
        (ActionName::Redo, ev("Y", "KeyY", "ctrl+shift")),
        (ActionName::SaveToActiveFile, ev("S", "KeyS", "ctrl+shift")),
        // saveToActiveFile compares key exactly: an upper-case S is not it.
        (ActionName::SaveToActiveFile, ev("S", "KeyS", "ctrl")),
        (ActionName::FlipVertical, ev("V", "KeyV", "ctrl+shift")),
        (ActionName::ToggleTheme, ev("D", "KeyD", "ctrl+alt+shift")),
        (ActionName::ZoomToFit, ev("!", "Digit1", "shift+alt")),
        (ActionName::ZoomToFit, ev("!", "Digit1", "shift+ctrl")),
        (ActionName::ZoomIn, ev("=", "Equal", "")),
        (ActionName::GridMode, ev("'", "Quote", "")),
        (ActionName::ObjectsSnapMode, ev("s", "KeyS", "alt+ctrl")),
        (ActionName::Stats, ev("/", "Slash", "alt+ctrl")),
        (ActionName::ViewMode, ev("r", "KeyR", "alt+ctrl")),
        (ActionName::ZenMode, ev("z", "KeyZ", "alt+ctrl")),
        (ActionName::DistributeHorizontally, ev("h", "KeyH", "alt+ctrl")),
        (ActionName::SendBackward, ev("{", "BracketLeft", "ctrl+shift")),
        (ActionName::BringForward, ev("}", "BracketRight", "ctrl+shift")),
        (ActionName::Cut, ev("X", "KeyX", "ctrl")),
        (ActionName::CopyAsPng, ev("C", "KeyC", "shift")),
        (ActionName::IncreaseFontSize, ev(">", "Period", "ctrl")),
        (ActionName::AlignTop, ev("ArrowUp", "ArrowUp", "ctrl")),
        (ActionName::ToggleShortcuts, ev("/", "Slash", "")),
        // A selection is required for the lock toggle.
        (ActionName::ToggleElementLock, ev("L", "KeyL", "ctrl+shift")),
        // Nothing to deselect and the preferred tool is active.
        (ActionName::Deselect, ev("Escape", "Escape", "")),
        // No multi-point element and no line editor.
        (ActionName::Finalize, ev("Escape", "Escape", "")),
        (ActionName::Finalize, ev("Enter", "Enter", "")),
    ];
    for (action, event) in cases {
        assert!(!passes(*action, event, &f), "{} {:?}", action.as_str(), event);
    }
    // saveFileToDisk lower-cases the key; toggleElementLock too.
    assert!(passes(ActionName::SaveFileToDisk, &ev("s", "KeyS", "ctrl+shift"), &f));
}

#[test]
fn undo_and_redo_match_non_latin_layouts_by_code() {
    let f = fixture_for("", false);
    // Russian: Ctrl+я on the Z key.
    assert!(passes(ActionName::Undo, &ev("я", "KeyZ", "ctrl"), &f));
    assert!(passes(ActionName::Redo, &ev("Я", "KeyZ", "ctrl+shift"), &f));
    assert!(passes(ActionName::Redo, &ev("н", "KeyY", "ctrl"), &f));
    // A Latin key wins over the code: Czech z sits on KeyY.
    assert!(passes(ActionName::Undo, &ev("z", "KeyY", "ctrl"), &f));
    assert!(!passes(ActionName::Redo, &ev("z", "KeyY", "ctrl"), &f));
    // French: w on KeyZ is not undo.
    assert!(!passes(ActionName::Undo, &ev("w", "KeyZ", "ctrl"), &f));
}

#[test]
fn deselect_key_test_follows_the_editor_state() {
    let esc = ev("Escape", "Escape", "");
    let pass = |f: &Fixture| passes(ActionName::Deselect, &esc, f);
    assert!(pass(&Fixture::selected(vec![rect("a")], &["a"])));
    // A deleted selected element is not a selection.
    assert!(!pass(&Fixture::selected(vec![deleted(rect("a"))], &["a"])));
    // A tool other than the preferred selection tool is active.
    assert!(pass(&Fixture::new(
        vec![],
        json!({"activeTool": {"type": "rectangle", "customType": null, "locked": false, "fromSelection": false, "lastActiveTool": null}})
    )));
    // The preferred tool is lasso and lasso is active.
    assert!(!pass(&Fixture::new(
        vec![],
        json!({
            "activeTool": {"type": "lasso", "customType": null, "locked": false, "fromSelection": false, "lastActiveTool": null},
            "preferredSelectionTool": {"type": "lasso", "initialized": true}
        })
    )));
    assert!(pass(&Fixture::new(vec![], json!({"editingGroupId": "g"}))));
    assert!(pass(&Fixture::new(vec![], json!({"activeEmbeddable": {"element": {}, "state": "active"}}))));
    assert!(pass(&Fixture::new(vec![], json!({"selectedLinearElement": {"isEditing": false}}))));
    // Not while drawing, in a multi-point element or line editing.
    let selected = json!({"selectedElementIds": selecting(&["a"])});
    let mut busy = Fixture::new(vec![rect("a")], selected.clone());
    busy.app_state.insert("newElement", json!({"id": "n"}));
    assert!(!pass(&busy));
    let mut busy = Fixture::new(vec![rect("a")], selected.clone());
    busy.app_state.insert("multiElement", json!({"id": "m"}));
    assert!(!pass(&busy));
    let mut busy = Fixture::new(vec![rect("a")], selected);
    busy.app_state.insert("selectedLinearElement", json!({"isEditing": true}));
    assert!(!pass(&busy));
    // Not from a text field.
    let f = Fixture::selected(vec![rect("a")], &["a"]);
    let mut writable = esc;
    writable.target_is_writable = true;
    assert!(!passes(ActionName::Deselect, &writable, &f));
}

#[test]
fn finalize_key_test_follows_the_editor_state() {
    let esc = ev("Escape", "Escape", "");
    let enter = ev("Enter", "Enter", "");
    let editing = Fixture::new(vec![], json!({"selectedLinearElement": {"isEditing": true}}));
    assert!(passes(ActionName::Finalize, &esc, &editing));
    assert!(!passes(ActionName::Finalize, &enter, &editing));
    let multi = Fixture::new(vec![], json!({"multiElement": {"id": "m"}}));
    assert!(passes(ActionName::Finalize, &esc, &multi));
    assert!(passes(ActionName::Finalize, &enter, &multi));
    // Line editing with Esc finalizes and does not deselect.
    let mut both = Fixture::selected(vec![line("l", 3, false)], &["l"]);
    both.app_state
        .insert("selectedLinearElement", json!({"isEditing": true}));
    assert_eq!(both.key_down(&esc), KeyDownOutcome::Perform(ActionName::Finalize));
}

#[test]
fn element_lock_key_test_needs_a_selection_without_bound_text() {
    let ev_l = ev("L", "KeyL", "ctrl+shift");
    let f = Fixture::selected(vec![rect("a")], &["a"]);
    assert!(passes(ActionName::ToggleElementLock, &ev_l, &f));
    assert!(passes(ActionName::ToggleElementLock, &ev("l", "KeyL", "ctrl+shift"), &f));
    // Only a bound text id selected: excluded (includeBoundTextElement false)
    // unless selected itself; it is, so it counts.
    let f = Fixture::selected(
        vec![with_bound_text(rect("c"), "t"), bound_text("t", "c")],
        &["t"],
    );
    assert!(passes(ActionName::ToggleElementLock, &ev_l, &f));
    let f = Fixture::new(vec![rect("a")], json!({}));
    assert!(!passes(ActionName::ToggleElementLock, &ev_l, &f));
}

// ---------------------------------------------------------------------------
// ActionManager (manager.tsx)
// ---------------------------------------------------------------------------

#[test]
fn handle_key_down_respects_view_mode() {
    let mut f = Fixture::new(vec![rect("a")], json!({"viewModeEnabled": true}));
    f.props.canvas_actions.toggle_theme = Some(true);
    // viewMode: false
    assert_eq!(f.key_down(&ev("a", "KeyA", "ctrl")), KeyDownOutcome::Unhandled);
    assert_eq!(f.key_down(&ev("z", "KeyZ", "ctrl")), KeyDownOutcome::Unhandled);
    // viewMode unset
    assert_eq!(f.key_down(&ev("k", "KeyK", "ctrl")), KeyDownOutcome::Unhandled);
    // viewMode: true
    assert_eq!(
        f.key_down(&ev("=", "Equal", "ctrl")),
        KeyDownOutcome::Perform(ActionName::ZoomIn)
    );
    assert_eq!(
        f.key_down(&ev("'", "Quote", "ctrl")),
        KeyDownOutcome::Perform(ActionName::GridMode)
    );
    assert_eq!(
        f.key_down(&ev("r", "KeyR", "alt")),
        KeyDownOutcome::Perform(ActionName::ViewMode)
    );
    assert_eq!(
        f.key_down(&ev("D", "KeyD", "alt+shift")),
        KeyDownOutcome::Perform(ActionName::ToggleTheme)
    );
}

#[test]
fn handle_key_down_respects_canvas_actions() {
    // toggleTheme defaults to null in canvasActions: not in the running.
    let f = Fixture::new(vec![], json!({}));
    assert_eq!(f.props.canvas_actions.toggle_theme, None);
    assert_eq!(f.key_down(&ev("D", "KeyD", "alt+shift")), KeyDownOutcome::Unhandled);
    let mut f = Fixture::new(vec![], json!({}));
    f.props.canvas_actions.toggle_theme = Some(false);
    assert_eq!(f.key_down(&ev("D", "KeyD", "alt+shift")), KeyDownOutcome::Unhandled);
    f.props.canvas_actions.toggle_theme = Some(true);
    assert_eq!(
        f.key_down(&ev("D", "KeyD", "alt+shift")),
        KeyDownOutcome::Perform(ActionName::ToggleTheme)
    );
    f.props.canvas_actions.load_scene = false;
    assert_eq!(f.key_down(&ev("o", "KeyO", "ctrl")), KeyDownOutcome::Unhandled);
    f.props.canvas_actions.save_to_active_file = false;
    assert_eq!(f.key_down(&ev("s", "KeyS", "ctrl")), KeyDownOutcome::Unhandled);
}

#[test]
fn canvas_actions_normalize_toggle_theme_like_the_component() {
    // index.tsx:142-147: null becomes true when the host does not control
    // the theme, or listens to theme changes.
    let mut c = CanvasActions::default();
    c.normalize(false, false);
    assert_eq!(c.toggle_theme, Some(true));
    let mut c = CanvasActions::default();
    c.normalize(true, true);
    assert_eq!(c.toggle_theme, Some(true));
    let mut c = CanvasActions::default();
    c.normalize(true, false);
    assert_eq!(c.toggle_theme, None);
    let mut c = CanvasActions {
        toggle_theme: Some(false),
        ..CanvasActions::default()
    };
    c.normalize(false, false);
    assert_eq!(c.toggle_theme, Some(false));
    // DEFAULT_UI_OPTIONS.canvasActions (constants.ts:384-393).
    let d = CanvasActions::default();
    assert!(d.change_view_background_color);
    assert!(d.clear_canvas);
    assert_eq!(d.export, Some(true));
    assert!(d.load_scene);
    assert!(d.save_to_active_file);
    assert!(d.save_as_image);
}

#[test]
fn handle_key_down_outside_the_interactive_editor() {
    let mut f = Fixture::selected(vec![rect("a")], &["a"]);
    f.env.interaction_enabled = false;
    // Navigation actions only.
    assert_eq!(f.key_down(&ev("a", "KeyA", "ctrl")), KeyDownOutcome::Unhandled);
    assert_eq!(
        f.key_down(&ev("=", "Equal", "ctrl")),
        KeyDownOutcome::Perform(ActionName::ZoomIn)
    );
    f.env.navigation_enabled = false;
    assert_eq!(f.key_down(&ev("=", "Equal", "ctrl")), KeyDownOutcome::Unhandled);
}

#[test]
fn handle_key_down_swallows_navigation_during_a_viewport_transition() {
    let mut f = Fixture::new(vec![], json!({}));
    f.env.viewport_transition_pending = true;
    assert_eq!(
        f.key_down(&ev("=", "Equal", "ctrl")),
        KeyDownOutcome::Swallowed(ActionName::ZoomIn)
    );
    assert_eq!(
        f.key_down(&ev("a", "KeyA", "ctrl")),
        KeyDownOutcome::Perform(ActionName::SelectAll)
    );
}

#[test]
fn handle_key_down_gives_up_on_conflicting_shortcuts() {
    let f = Fixture::new(vec![], json!({}));
    // Shift+Alt+H: flipHorizontal (Shift+H) and distributeHorizontally (Alt+H).
    match f.key_down(&ev("H", "KeyH", "shift+alt")) {
        KeyDownOutcome::Ambiguous(names) => {
            let set: BTreeSet<&str> = names.iter().map(|a| a.as_str()).collect();
            assert_eq!(set, BTreeSet::from(["flipHorizontal", "distributeHorizontally"]));
        }
        other => panic!("{other:?}"),
    }
    // Ctrl+Alt+Shift+C: copyAsPng and copyStyles.
    assert!(matches!(
        f.key_down(&ev("C", "KeyC", "ctrl+alt+shift")),
        KeyDownOutcome::Ambiguous(_)
    ));
}

#[test]
fn only_registered_actions_take_keys() {
    let f = Fixture::new(vec![], json!({}));
    let mut manager = ActionManager::empty();
    assert_eq!(
        manager.handle_key_down(&ev("a", "KeyA", "ctrl"), &f.ctx()),
        KeyDownOutcome::Unhandled
    );
    manager.register(ActionName::SelectAll);
    assert_eq!(
        manager.handle_key_down(&ev("a", "KeyA", "ctrl"), &f.ctx()),
        KeyDownOutcome::Perform(ActionName::SelectAll)
    );
    // Registering twice keeps one entry (actions is a record by name).
    manager.register(ActionName::SelectAll);
    assert_eq!(manager.registered().count(), 1);
}

#[test]
fn execute_action_gate() {
    let manager = ActionManager::new();
    let mut f = Fixture::new(vec![], json!({}));
    for source in [
        ActionSource::Ui,
        ActionSource::Keyboard,
        ActionSource::ContextMenu,
        ActionSource::Api,
        ActionSource::CommandPalette,
    ] {
        assert!(manager.can_execute(ActionName::SelectAll, source, &f.ctx()));
    }
    f.env.interaction_enabled = false;
    assert!(manager.can_execute(ActionName::SelectAll, ActionSource::Api, &f.ctx()));
    assert!(!manager.can_execute(ActionName::SelectAll, ActionSource::ContextMenu, &f.ctx()));
    assert!(manager.can_execute(ActionName::ZoomIn, ActionSource::Ui, &f.ctx()));
    f.env.navigation_enabled = false;
    assert!(!manager.can_execute(ActionName::ZoomIn, ActionSource::Ui, &f.ctx()));
    let mut f = Fixture::new(vec![], json!({}));
    f.env.viewport_transition_pending = true;
    assert!(!manager.can_execute(ActionName::ZoomIn, ActionSource::Api, &f.ctx()));
    assert!(manager.can_execute(ActionName::SelectAll, ActionSource::Api, &f.ctx()));
}

#[test]
fn render_action_gate() {
    let manager = ActionManager::new();
    let mut f = Fixture::new(vec![], json!({}));
    assert!(manager.can_render(ActionName::ChangeViewBackgroundColor, &f.ctx()));
    assert!(manager.can_render(ActionName::ChangeStrokeColor, &f.ctx()));
    assert!(!manager.can_render(ActionName::Cut, &f.ctx()));
    assert!(!manager.can_render(ActionName::CommandPalette, &f.ctx()));
    f.props.canvas_actions.change_view_background_color = false;
    assert!(!manager.can_render(ActionName::ChangeViewBackgroundColor, &f.ctx()));
    assert!(!ActionManager::empty().can_render(ActionName::ChangeStrokeColor, &f.ctx()));
}

#[test]
fn track_action_reports_category_action_and_source() {
    let f = Fixture::new(vec![], json!({}));
    let spec = ActionName::DeleteSelectedElements.spec();
    let tracked = track_action(spec, ActionSource::Keyboard, &f.ctx()).unwrap();
    assert_eq!(tracked.category, "element");
    assert_eq!(tracked.action, "delete");
    assert_eq!(tracked.label, "keyboard (desktop)");
    let tracked = track_action(ActionName::Group.spec(), ActionSource::ContextMenu, &f.ctx()).unwrap();
    assert_eq!(tracked.action, "group");
    assert_eq!(tracked.label, "contextMenu (desktop)");
    // trackEvent: false
    assert!(track_action(ActionName::Finalize.spec(), ActionSource::Ui, &f.ctx()).is_none());
    // trackEvent.predicate: gridMode tracks only while the grid is on
    // (the state before the toggle).
    assert!(track_action(ActionName::GridMode.spec(), ActionSource::Ui, &f.ctx()).is_none());
    let on = Fixture::new(vec![], json!({"gridModeEnabled": true}));
    assert!(track_action(ActionName::GridMode.spec(), ActionSource::Ui, &on.ctx()).is_some());
    assert!(track_action(ActionName::ZenMode.spec(), ActionSource::Ui, &f.ctx()).is_some());
    assert!(track_action(ActionName::ArrowBinding.spec(), ActionSource::Ui, &f.ctx()).is_none());
    assert!(track_action(ActionName::MidpointSnapping.spec(), ActionSource::Ui, &f.ctx()).is_none());
    assert!(track_action(ActionName::ObjectsSnapMode.spec(), ActionSource::Ui, &f.ctx()).is_some());
    assert!(track_action(ActionName::ViewMode.spec(), ActionSource::Ui, &f.ctx()).is_some());
    assert!(track_action(ActionName::SearchMenu.spec(), ActionSource::Ui, &f.ctx()).is_none());
    let mut phone = Fixture::new(vec![], json!({}));
    phone.env.form_factor = FormFactor::Phone;
    let tracked = track_action(ActionName::Undo.spec(), ActionSource::Ui, &phone.ctx()).unwrap();
    assert_eq!(tracked.label, "ui (mobile)");
    let tracked =
        track_action(ActionName::Hyperlink.spec(), ActionSource::CommandPalette, &f.ctx()).unwrap();
    assert_eq!((tracked.category, tracked.action.as_str()), ("hyperlink", "click"));
    assert_eq!(tracked.label, "commandPalette (desktop)");
}

// ---------------------------------------------------------------------------
// Predicates
// ---------------------------------------------------------------------------

#[test]
fn actions_without_a_predicate_are_always_enabled() {
    let f = Fixture::new(vec![], json!({}));
    for a in [ActionName::Cut, ActionName::SelectAll, ActionName::Undo, ActionName::ChangeStrokeColor] {
        assert!(f.enabled(a), "{}", a.as_str());
    }
}

#[test]
fn align_needs_two_selected_groups_and_no_frame() {
    let two = Fixture::selected(vec![rect("a"), rect("b")], &["a", "b"]);
    let one = Fixture::selected(vec![rect("a"), rect("b")], &["a"]);
    for a in [
        ActionName::AlignTop,
        ActionName::AlignBottom,
        ActionName::AlignLeft,
        ActionName::AlignRight,
        ActionName::AlignVerticallyCentered,
        ActionName::AlignHorizontallyCentered,
    ] {
        assert!(two.enabled(a), "{}", a.as_str());
        assert!(!one.enabled(a), "{}", a.as_str());
    }
    // A lone selected group aligns its members: with no inner group each
    // element is its own unit (groups.ts:441-448).
    let group = Fixture::new(
        vec![grouped(rect("a"), &["g"]), grouped(rect("b"), &["g"])],
        json!({"selectedElementIds": selecting(&["a", "b"]), "selectedGroupIds": {"g": true}}),
    );
    assert!(group.enabled(ActionName::AlignLeft));
    // Two members of one selected group next to a second selected group:
    // the groups are the units.
    let groups = Fixture::new(
        vec![
            grouped(rect("a"), &["g"]),
            grouped(rect("b"), &["g"]),
            grouped(rect("c"), &["h"]),
        ],
        json!({"selectedElementIds": selecting(&["a", "b", "c"]), "selectedGroupIds": {"g": true, "h": true}}),
    );
    assert!(groups.enabled(ActionName::AlignLeft));
    // Distribute has no predicate; its buttons hide on their own
    // (enableActionGroup, actionDistribute.tsx:35-46).
    assert!(groups.enabled(ActionName::DistributeHorizontally));
    let one_group = Fixture::new(
        vec![grouped(rect("a"), &["g"]), grouped(rect("b"), &["g"])],
        json!({"selectedElementIds": selecting(&["a", "b"]), "selectedGroupIds": {"g": true, "h": true}}),
    );
    // Two selected group ids (one unused): not the single-group case, so
    // both members fall in the g bucket.
    assert!(!one_group.enabled(ActionName::AlignLeft));
    // One selected group: its inner groups are the units.
    let nested = Fixture::new(
        vec![grouped(rect("a"), &["i1", "g"]), grouped(rect("b"), &["i2", "g"])],
        json!({"selectedElementIds": selecting(&["a", "b"]), "selectedGroupIds": {"g": true}}),
    );
    assert!(nested.enabled(ActionName::AlignLeft));
    // A group plus a loose element.
    let mixed = Fixture::new(
        vec![grouped(rect("a"), &["g"]), grouped(rect("b"), &["g"]), rect("c")],
        json!({"selectedElementIds": selecting(&["a", "b", "c"]), "selectedGroupIds": {"g": true}}),
    );
    assert!(mixed.enabled(ActionName::AlignLeft));
    // Bound text is appended to its container's bucket.
    let with_text = Fixture::selected(
        vec![with_bound_text(rect("c"), "t"), bound_text("t", "c")],
        &["c", "t"],
    );
    assert!(!with_text.enabled(ActionName::AlignLeft));
    let framed = Fixture::selected(vec![rect("a"), frame("f")], &["a", "f"]);
    assert!(!framed.enabled(ActionName::AlignLeft));
}

#[test]
fn bound_text_predicates() {
    let container = with_bound_text(rect("c"), "t");
    let label = bound_text("t", "c");
    let f = Fixture::selected(vec![container.clone(), label.clone()], &["c"]);
    assert!(f.enabled(ActionName::UnbindText));
    assert!(!f.enabled(ActionName::BindText));
    let f = Fixture::selected(vec![rect("r"), text("t")], &["r", "t"]);
    assert!(f.enabled(ActionName::BindText));
    assert!(!f.enabled(ActionName::UnbindText));
    assert!(f.enabled(ActionName::WrapTextInContainer));
    // An arrow is a text container too.
    let f = Fixture::selected(vec![text("t"), arrow("a", false)], &["t", "a"]);
    assert!(f.enabled(ActionName::BindText));
    // A container that already has a label.
    let f = Fixture::selected(vec![container.clone(), label.clone(), text("x")], &["c", "x"]);
    assert!(!f.enabled(ActionName::BindText));
    // A label whose text element is gone does not count.
    let f = Fixture::selected(vec![container.clone(), text("x")], &["c", "x"]);
    assert!(f.enabled(ActionName::BindText));
    let f = Fixture::selected(vec![rect("r"), ellipse("e")], &["r", "e"]);
    assert!(!f.enabled(ActionName::BindText));
    let f = Fixture::selected(vec![rect("r"), text("a"), text("b")], &["r", "a", "b"]);
    assert!(!f.enabled(ActionName::BindText));
    // wrapTextInContainer: some unbound text.
    let f = Fixture::selected(vec![container, label], &["t"]);
    assert!(!f.enabled(ActionName::WrapTextInContainer));
    let f = Fixture::new(vec![text("t")], json!({}));
    assert!(!f.enabled(ActionName::WrapTextInContainer));
}

#[test]
fn canvas_predicates() {
    let mut f = Fixture::new(vec![], json!({}));
    assert!(f.enabled(ActionName::ChangeViewBackgroundColor));
    assert!(f.enabled(ActionName::ClearCanvas));
    assert!(f.enabled(ActionName::LoadScene));
    assert!(!f.enabled(ActionName::SaveToActiveFile));
    assert!(!f.enabled(ActionName::ToggleTheme));
    f.props.canvas_actions.toggle_theme = Some(true);
    assert!(f.enabled(ActionName::ToggleTheme));
    f.app_state.insert("fileHandle", json!({}));
    assert!(f.enabled(ActionName::SaveToActiveFile));
    f.app_state.insert("viewModeEnabled", json!(true));
    assert!(!f.enabled(ActionName::ChangeViewBackgroundColor));
    assert!(!f.enabled(ActionName::ClearCanvas));
    assert!(!f.enabled(ActionName::LoadScene));
    assert!(!f.enabled(ActionName::SaveToActiveFile));
    assert!(f.enabled(ActionName::ToggleTheme));
    let mut f = Fixture::new(vec![], json!({"openDialog": {"name": "elementLinkSelector"}}));
    assert!(!f.enabled(ActionName::ClearCanvas));
    f.app_state.insert("openDialog", json!({"name": "help"}));
    assert!(f.enabled(ActionName::ClearCanvas));
    f.props.canvas_actions.clear_canvas = false;
    f.props.canvas_actions.change_view_background_color = false;
    f.props.canvas_actions.load_scene = false;
    assert!(!f.enabled(ActionName::ClearCanvas));
    assert!(!f.enabled(ActionName::ChangeViewBackgroundColor));
    assert!(!f.enabled(ActionName::LoadScene));
}

#[test]
fn zoom_actions_need_navigation() {
    let mut f = Fixture::new(vec![], json!({}));
    let zooms = [
        ActionName::ZoomIn,
        ActionName::ZoomOut,
        ActionName::ResetZoom,
        ActionName::ZoomToFit,
        ActionName::ZoomToFitSelection,
        ActionName::ZoomToFitSelectionInViewport,
    ];
    for a in zooms {
        assert!(f.enabled(a));
    }
    f.env.navigation_enabled = false;
    for a in zooms {
        assert!(!f.enabled(a));
    }
}

#[test]
fn clipboard_predicates() {
    let mut f = Fixture::selected(vec![text("t")], &["t"]);
    assert!(!f.enabled(ActionName::CopyAsPng));
    assert!(!f.enabled(ActionName::CopyAsSvg));
    assert!(!f.enabled(ActionName::CopyText));
    f.env.clipboard_blob = true;
    f.env.clipboard_write_text = true;
    assert!(f.enabled(ActionName::CopyAsPng));
    assert!(f.enabled(ActionName::CopyAsSvg));
    assert!(f.enabled(ActionName::CopyText));
    // Bound text counts for copyText through its selected container.
    f.elements = vec![with_bound_text(rect("c"), "t"), bound_text("t", "c")];
    f.app_state.insert("selectedElementIds", selecting(&["c"]));
    assert!(f.enabled(ActionName::CopyText));
    f.app_state.insert("selectedElementIds", json!({}));
    assert!(!f.enabled(ActionName::CopyText));
    // Any element, selected or not, enables the image copies.
    assert!(f.enabled(ActionName::CopyAsPng));
    f.elements.clear();
    assert!(!f.enabled(ActionName::CopyAsPng));
    assert!(!f.enabled(ActionName::CopyAsSvg));
}

#[test]
fn crop_editor_needs_one_image_and_no_crop_in_progress() {
    assert!(Fixture::selected(vec![image("i")], &["i"]).enabled(ActionName::CropEditor));
    assert!(!Fixture::selected(vec![image("i"), image("j")], &["i", "j"]).enabled(ActionName::CropEditor));
    assert!(!Fixture::selected(vec![rect("r")], &["r"]).enabled(ActionName::CropEditor));
    let f = Fixture::new(
        vec![image("i")],
        json!({"selectedElementIds": selecting(&["i"]), "croppingElementId": "i"}),
    );
    assert!(!f.enabled(ActionName::CropEditor));
}

#[test]
fn element_link_predicates() {
    let one = Fixture::selected(vec![rect("a")], &["a"]);
    assert!(one.enabled(ActionName::CopyElementLink));
    assert!(one.enabled(ActionName::LinkToElement));
    assert!(one.enabled(ActionName::Hyperlink));
    let two = Fixture::selected(vec![rect("a"), rect("b")], &["a", "b"]);
    assert!(!two.enabled(ActionName::CopyElementLink));
    assert!(!two.enabled(ActionName::LinkToElement));
    assert!(!two.enabled(ActionName::Hyperlink));
    let group = Fixture::selected(
        vec![grouped(rect("a"), &["g"]), grouped(rect("b"), &["g", "h"])],
        &["a", "b"],
    );
    assert!(group.enabled(ActionName::CopyElementLink));
    assert!(!group.enabled(ActionName::LinkToElement));
    let none = Fixture::new(vec![rect("a")], json!({}));
    assert!(!none.enabled(ActionName::CopyElementLink));
    let dialog = Fixture::new(
        vec![rect("a")],
        json!({"selectedElementIds": selecting(&["a"]), "openDialog": {"name": "elementLinkSelector"}}),
    );
    assert!(!dialog.enabled(ActionName::LinkToElement));
    assert!(dialog.enabled(ActionName::CopyElementLink));
}

#[test]
fn element_lock_predicates() {
    let f = Fixture::selected(vec![rect("a")], &["a"]);
    assert!(f.enabled(ActionName::ToggleElementLock));
    assert!(!f.enabled(ActionName::UnlockAllElements));
    // A locked element inside a frame cannot be toggled from here.
    let f = Fixture::selected(vec![frame("f"), in_frame(locked(rect("a")), "f")], &["a"]);
    assert!(!f.enabled(ActionName::ToggleElementLock));
    let f = Fixture::selected(vec![frame("f"), in_frame(rect("a"), "f")], &["a"]);
    assert!(f.enabled(ActionName::ToggleElementLock));
    let f = Fixture::new(vec![locked(rect("a")), rect("b")], json!({}));
    assert!(!f.enabled(ActionName::ToggleElementLock));
    assert!(f.enabled(ActionName::UnlockAllElements));
    let f = Fixture::new(vec![rect("a")], json!({}));
    assert!(!f.enabled(ActionName::UnlockAllElements));
    // A selection hides unlock-all.
    let f = Fixture::selected(vec![locked(rect("a")), rect("b")], &["b"]);
    assert!(!f.enabled(ActionName::UnlockAllElements));
}

#[test]
fn frame_predicates() {
    let f = Fixture::selected(vec![frame("f"), in_frame(rect("a"), "f")], &["f"]);
    assert!(f.enabled(ActionName::SelectAllElementsInFrame));
    assert!(f.enabled(ActionName::RemoveAllElementsFromFrame));
    assert!(!f.enabled(ActionName::WrapSelectionInFrame));
    let f = Fixture::selected(vec![element(ElementKind::MagicFrame(FrameFields { name: None }), "m")], &["m"]);
    assert!(f.enabled(ActionName::SelectAllElementsInFrame));
    let f = Fixture::selected(vec![rect("a"), rect("b")], &["a", "b"]);
    assert!(!f.enabled(ActionName::SelectAllElementsInFrame));
    assert!(f.enabled(ActionName::WrapSelectionInFrame));
    let f = Fixture::new(vec![rect("a")], json!({}));
    assert!(!f.enabled(ActionName::WrapSelectionInFrame));
}

#[test]
fn group_predicates() {
    let f = Fixture::selected(vec![rect("a"), rect("b")], &["a", "b"]);
    assert!(f.enabled(ActionName::Group));
    assert!(!f.enabled(ActionName::Ungroup));
    let f = Fixture::selected(vec![rect("a")], &["a"]);
    assert!(!f.enabled(ActionName::Group));
    // Already one group.
    let f = Fixture::new(
        vec![grouped(rect("a"), &["g"]), grouped(rect("b"), &["g"])],
        json!({"selectedElementIds": selecting(&["a", "b"]), "selectedGroupIds": {"g": true}}),
    );
    assert!(!f.enabled(ActionName::Group));
    assert!(f.enabled(ActionName::Ungroup));
    // Inside the group being edited, its elements can be grouped again.
    let f = Fixture::new(
        vec![grouped(rect("a"), &["g"]), grouped(rect("b"), &["g"])],
        json!({"selectedElementIds": selecting(&["a", "b"]), "editingGroupId": "g"}),
    );
    assert!(f.enabled(ActionName::Group));
    // A frame with one of its children.
    let f = Fixture::selected(vec![frame("f"), in_frame(rect("a"), "f")], &["f", "a"]);
    assert!(!f.enabled(ActionName::Group));
    // selectedGroupIds entries set to false do not count.
    let f = Fixture::new(vec![], json!({"selectedGroupIds": {"g": false}}));
    assert!(!f.enabled(ActionName::Ungroup));
}

#[test]
fn linear_editor_and_polygon_predicates() {
    let f = Fixture::selected(vec![line("l", 3, false)], &["l"]);
    assert!(f.enabled(ActionName::ToggleLinearEditor));
    assert!(!f.enabled(ActionName::TogglePolygon));
    let f = Fixture::selected(vec![arrow("a", false)], &["a"]);
    assert!(f.enabled(ActionName::ToggleLinearEditor));
    let f = Fixture::selected(vec![arrow("a", true)], &["a"]);
    assert!(!f.enabled(ActionName::ToggleLinearEditor));
    let f = Fixture::new(
        vec![line("l", 3, false)],
        json!({"selectedElementIds": selecting(&["l"]), "selectedLinearElement": {"isEditing": true}}),
    );
    assert!(!f.enabled(ActionName::ToggleLinearEditor));
    let f = Fixture::selected(vec![line("l", 4, false), line("m", 5, true)], &["l", "m"]);
    assert!(f.enabled(ActionName::TogglePolygon));
    let f = Fixture::selected(vec![line("l", 4, false), arrow("a", false)], &["l", "a"]);
    assert!(!f.enabled(ActionName::TogglePolygon));
    let f = Fixture::new(vec![line("l", 4, false)], json!({}));
    assert!(!f.enabled(ActionName::TogglePolygon));
}

#[test]
fn auto_resize_needs_one_fixed_width_text() {
    let mut t = text("t");
    if let ElementKind::Text(fields) = &mut t.kind {
        fields.auto_resize = false;
    }
    assert!(Fixture::selected(vec![t], &["t"]).enabled(ActionName::AutoResize));
    assert!(!Fixture::selected(vec![text("t")], &["t"]).enabled(ActionName::AutoResize));
    assert!(!Fixture::selected(vec![rect("r")], &["r"]).enabled(ActionName::AutoResize));
}

#[test]
fn host_controlled_toggles() {
    let mut f = Fixture::new(vec![], json!({}));
    for a in [
        ActionName::GridMode,
        ActionName::ObjectsSnapMode,
        ActionName::SearchMenu,
        ActionName::ViewMode,
        ActionName::ZenMode,
    ] {
        assert!(f.enabled(a), "{}", a.as_str());
    }
    f.props.grid_mode_enabled = Some(false);
    assert!(!f.enabled(ActionName::GridMode));
    // searchMenu reads the grid prop (actionToggleSearchMenu.ts:54-56).
    assert!(!f.enabled(ActionName::SearchMenu));
    f.props.objects_snap_mode_enabled = Some(true);
    assert!(!f.enabled(ActionName::ObjectsSnapMode));
    f.props.view_mode_enabled = Some(false);
    assert!(!f.enabled(ActionName::ViewMode));
    f.props.zen_mode_enabled = Some(true);
    assert!(!f.enabled(ActionName::ZenMode));

    let mut f = Fixture::new(vec![], json!({}));
    f.env.interaction_enabled = false;
    assert!(!f.enabled(ActionName::ViewMode));
    f.env.form_factor = FormFactor::Phone;
    assert!(!f.enabled(ActionName::ZenMode));
    f.env.form_factor = FormFactor::Tablet;
    assert!(f.enabled(ActionName::ZenMode));
}

#[test]
fn shape_switch_needs_a_convertible_element() {
    assert!(!Fixture::new(vec![], json!({})).enabled(ActionName::ToggleShapeSwitch));
    assert!(Fixture::new(vec![rect("r")], json!({})).enabled(ActionName::ToggleShapeSwitch));
    assert!(Fixture::new(vec![line("l", 2, false)], json!({})).enabled(ActionName::ToggleShapeSwitch));
    assert!(Fixture::new(vec![arrow("a", false)], json!({})).enabled(ActionName::ToggleShapeSwitch));
    assert!(!Fixture::new(vec![text("t"), image("i")], json!({})).enabled(ActionName::ToggleShapeSwitch));
    // An arrow with a label or a binding is not convertible.
    let labelled = with_bound_text(arrow("a", false), "t");
    assert!(!Fixture::new(vec![labelled], json!({})).enabled(ActionName::ToggleShapeSwitch));
    let mut bound = arrow("a", false);
    if let ElementKind::Arrow(fields) = &mut bound.kind {
        fields.linear.start_binding = Some(serde_json::from_value(json!({
            "elementId": "r", "fixedPoint": [0.5, 0.5], "mode": "orbit"
        }))
        .expect("binding"));
    }
    assert!(!Fixture::new(vec![bound], json!({})).enabled(ActionName::ToggleShapeSwitch));
}

// ---------------------------------------------------------------------------
// Labels, icons, checked
// ---------------------------------------------------------------------------

fn label(name: ActionName, f: &Fixture) -> &'static str {
    name.spec().label_key(&f.ctx()).expect("label")
}

#[test]
fn dynamic_labels() {
    let light = Fixture::new(vec![], json!({}));
    let dark = Fixture::new(vec![], json!({"theme": "dark"}));
    assert_eq!(label(ActionName::ToggleTheme, &light), "buttons.darkMode");
    assert_eq!(label(ActionName::ToggleTheme, &dark), "buttons.lightMode");

    let unlocked = Fixture::selected(vec![rect("a")], &["a"]);
    let mixed = Fixture::selected(vec![rect("a"), locked(rect("b"))], &["a", "b"]);
    assert_eq!(label(ActionName::ToggleElementLock, &unlocked), "labels.elementLock.lock");
    assert_eq!(label(ActionName::ToggleElementLock, &mixed), "labels.elementLock.unlock");

    let arrow_sel = Fixture::selected(vec![arrow("a", false)], &["a"]);
    let line_sel = Fixture::selected(vec![line("l", 3, false)], &["l"]);
    assert_eq!(label(ActionName::ToggleLinearEditor, &arrow_sel), "labels.lineEditor.editArrow");
    assert_eq!(label(ActionName::ToggleLinearEditor, &line_sel), "labels.lineEditor.edit");

    let polygons = Fixture::selected(vec![line("l", 4, true)], &["l"]);
    assert_eq!(label(ActionName::TogglePolygon, &polygons), "labels.polygon.breakPolygon");
    assert_eq!(label(ActionName::TogglePolygon, &line_sel), "labels.polygon.convertToPolygon");

    let mut linked = rect("a");
    linked.base.link = Some("https://example.com".to_owned());
    assert_eq!(
        label(ActionName::Hyperlink, &Fixture::selected(vec![linked], &["a"])),
        "labels.link.edit"
    );
    assert_eq!(label(ActionName::Hyperlink, &unlocked), "labels.link.create");
    assert_eq!(
        label(ActionName::Hyperlink, &Fixture::selected(vec![embeddable("e")], &["e"])),
        "labels.link.editEmbed"
    );
    assert_eq!(label(ActionName::Cut, &light), "labels.cut");
    assert_eq!(ActionName::CommandPalette.spec().label_key(&light.ctx()), None);
}

#[test]
fn dynamic_icons() {
    let light = Fixture::new(vec![], json!({}));
    let dark = Fixture::new(vec![], json!({"theme": "dark"}));
    let icon = |a: ActionName, f: &Fixture| a.spec().icon_name(&f.ctx());
    assert_eq!(icon(ActionName::ToggleTheme, &light), Some("MoonIcon"));
    assert_eq!(icon(ActionName::ToggleTheme, &dark), Some("SunIcon"));
    let unlocked = Fixture::selected(vec![rect("a")], &["a"]);
    let all_locked = Fixture::selected(vec![locked(rect("a"))], &["a"]);
    assert_eq!(icon(ActionName::ToggleElementLock, &unlocked), Some("LockedIcon"));
    assert_eq!(icon(ActionName::ToggleElementLock, &all_locked), Some("UnlockedIcon"));
    assert_eq!(icon(ActionName::Group, &light), Some("GroupIcon"));
    assert_eq!(icon(ActionName::Ungroup, &light), Some("UngroupIcon"));
    assert_eq!(icon(ActionName::ToggleShapeSwitch, &light), None);
    assert_eq!(icon(ActionName::AutoResize, &light), None);
    assert_eq!(icon(ActionName::Cut, &light), Some("cutIcon"));
}

#[test]
fn checked_reads_the_app_state() {
    let checked = |a: ActionName, patch: Value| (a.spec().checked.unwrap())(&state(patch));
    assert!(checked(ActionName::ArrowBinding, json!({})));
    assert!(!checked(ActionName::ArrowBinding, json!({"bindingPreference": "disabled"})));
    assert!(!checked(ActionName::GridMode, json!({})));
    assert!(checked(ActionName::GridMode, json!({"gridModeEnabled": true})));
    assert!(checked(ActionName::MidpointSnapping, json!({})));
    assert!(checked(ActionName::ObjectsSnapMode, json!({"objectsSnapModeEnabled": true})));
    // searchMenu and toggleShapeSwitch carry the grid toggle's `checked`.
    assert!(checked(ActionName::SearchMenu, json!({"gridModeEnabled": true})));
    assert!(checked(ActionName::ToggleShapeSwitch, json!({"gridModeEnabled": true})));
    assert!(!checked(ActionName::Stats, json!({})));
    assert!(checked(ActionName::Stats, json!({"stats": {"open": true, "panels": 3}})));
    assert!(checked(ActionName::ViewMode, json!({"viewModeEnabled": true})));
    assert!(checked(ActionName::ZenMode, json!({"zenModeEnabled": true})));
    assert!(checked(ActionName::UpdateFrameRendering, json!({})));
    assert!(!checked(
        ActionName::UpdateFrameRendering,
        json!({"frameRendering": {"enabled": false, "clip": true, "name": true, "outline": true}})
    ));
}

// ---------------------------------------------------------------------------
// Shortcut labels (shortcuts.ts, shortcut.ts)
// ---------------------------------------------------------------------------

#[test]
fn get_shortcut_key_localizes_modifiers() {
    let en = KeyLabels::EN;
    assert_eq!(get_shortcut_key("CtrlOrCmd+Shift+Up", false, &en), "Ctrl+Shift+Up");
    assert_eq!(get_shortcut_key("CtrlOrCmd+Shift+Up", true, &en), "Cmd+Shift+Up");
    assert_eq!(get_shortcut_key("Alt+H", false, &en), "Alt+H");
    assert_eq!(get_shortcut_key("Alt+H", true, &en), "Option+H");
    assert_eq!(get_shortcut_key("Option+H", false, &en), "Alt+H");
    assert_eq!(get_shortcut_key("Return", false, &en), "Enter");
    assert_eq!(get_shortcut_key("Escape", false, &en), "Esc");
    assert_eq!(get_shortcut_key("Spacebar", false, &en), "Space");
    assert_eq!(get_shortcut_key("Del", false, &en), "Delete");
    assert_eq!(get_shortcut_key("Command+Q", false, &en), "Ctrl+Q");
    // Case-insensitive, whole words only.
    assert_eq!(get_shortcut_key("ctrl+shift+x", false, &en), "Ctrl+Shift+x");
    assert_eq!(get_shortcut_key("Shifty+Alternate", false, &en), "Shifty+Alternate");
    let loud = KeyLabels {
        ctrl: "CTRL",
        cmd: "CMD",
        alt: "ALT",
        option: "OPT",
        shift: "SHIFT",
        enter: "ENTER",
        escape: "ESC",
        spacebar: "SPACE",
        delete: "DEL",
        drag: "DRAG",
    };
    // Only the first Alt and Shift are replaced; every Ctrl/Cmd is.
    assert_eq!(get_shortcut_key("Alt+Alt", false, &loud), "ALT+Alt");
    assert_eq!(get_shortcut_key("Shift+Shift", false, &loud), "SHIFT+Shift");
    assert_eq!(get_shortcut_key("Ctrl+Cmd", false, &loud), "CTRL+CTRL");
    assert_eq!(get_shortcut_key("Ctrl+Cmd", true, &loud), "CMD+CMD");
}

#[test]
fn shortcut_names_and_labels() {
    let en = KeyLabels::EN;
    #[rustfmt::skip]
    let expected: &[(&str, &str, &str)] = &[
        ("toggleTheme", "Shift+Alt+D", "Shift+Option+D"),
        ("saveScene", "Ctrl+S", "Cmd+S"),
        ("loadScene", "Ctrl+O", "Cmd+O"),
        ("clearCanvas", "Ctrl+Delete", "Cmd+Delete"),
        ("imageExport", "Ctrl+Shift+E", "Cmd+Shift+E"),
        ("commandPalette", "Ctrl+/", "Cmd+/"),
        ("cut", "Ctrl+X", "Cmd+X"),
        ("copy", "Ctrl+C", "Cmd+C"),
        ("paste", "Ctrl+V", "Cmd+V"),
        ("copyStyles", "Ctrl+Alt+C", "Cmd+Option+C"),
        ("pasteStyles", "Ctrl+Alt+V", "Cmd+Option+V"),
        ("selectAll", "Ctrl+A", "Cmd+A"),
        ("deleteSelectedElements", "Delete", "Delete"),
        ("duplicateSelection", "Ctrl+D", "Cmd+D"),
        ("sendBackward", "Ctrl+[", "Cmd+["),
        ("bringForward", "Ctrl+]", "Cmd+]"),
        ("sendToBack", "Ctrl+Shift+[", "Cmd+Option+["),
        ("bringToFront", "Ctrl+Shift+]", "Cmd+Option+]"),
        ("copyAsPng", "Shift+Alt+C", "Shift+Option+C"),
        ("group", "Ctrl+G", "Cmd+G"),
        ("ungroup", "Ctrl+Shift+G", "Cmd+Shift+G"),
        ("gridMode", "Ctrl+'", "Cmd+'"),
        ("zenMode", "Alt+Z", "Option+Z"),
        ("objectsSnapMode", "Alt+S", "Option+S"),
        ("stats", "Alt+/", "Option+/"),
        ("addToLibrary", "", ""),
        ("flipHorizontal", "Shift+H", "Shift+H"),
        ("flipVertical", "Shift+V", "Shift+V"),
        ("viewMode", "Alt+R", "Option+R"),
        ("hyperlink", "Ctrl+K", "Cmd+K"),
        ("toggleElementLock", "Ctrl+Shift+L", "Cmd+Shift+L"),
        ("resetZoom", "Ctrl+0", "Cmd+0"),
        ("zoomOut", "Ctrl+-", "Cmd+-"),
        ("zoomIn", "Ctrl++", "Cmd++"),
        ("zoomToFitSelection", "Shift+3", "Shift+3"),
        ("zoomToFit", "Shift+1", "Shift+1"),
        ("zoomToFitSelectionInViewport", "Shift+2", "Shift+2"),
        ("saveFileToDisk", "Ctrl+S", "Cmd+S"),
        ("saveToActiveFile", "Ctrl+S", "Cmd+S"),
        ("toggleShortcuts", "?", "?"),
        ("searchMenu", "Ctrl+F", "Cmd+F"),
        ("wrapSelectionInFrame", "", ""),
        ("toolLock", "Q", "Q"),
    ];
    assert_eq!(SHORTCUT_NAMES.len(), expected.len());
    let names: BTreeSet<&str> = SHORTCUT_NAMES.iter().copied().collect();
    for (name, other, mac) in expected {
        assert!(names.contains(name), "{name}");
        assert_eq!(get_shortcut_from_shortcut_name(name, 0, false, &en), *other, "{name}");
        assert_eq!(get_shortcut_from_shortcut_name(name, 0, true, &en), *mac, "{name}");
    }
    // A second shortcut when there is one, else the first.
    assert_eq!(get_shortcut_from_shortcut_name("commandPalette", 1, false, &en), "Ctrl+Shift+P");
    assert_eq!(get_shortcut_from_shortcut_name("duplicateSelection", 1, false, &en), "Alt+drag");
    assert_eq!(get_shortcut_from_shortcut_name("duplicateSelection", 1, true, &en), "Option+drag");
    assert_eq!(get_shortcut_from_shortcut_name("group", 1, false, &en), "Ctrl+G");
    // Names outside the map have no shortcut.
    assert_eq!(get_shortcut_from_shortcut_name("unbindText", 0, false, &en), "");
    assert_eq!(get_shortcut_from_shortcut_name("nope", 0, false, &en), "");
}

// ---------------------------------------------------------------------------
// Context menu (App.tsx:13835-13936, ContextMenu.tsx)
// ---------------------------------------------------------------------------

fn item_names(entries: &[ContextMenuEntry]) -> Vec<&'static str> {
    entries
        .iter()
        .filter_map(|e| match e {
            ContextMenuEntry::Item(item) => Some(item.name.as_str()),
            ContextMenuEntry::Separator => None,
        })
        .collect()
}

fn menu(kind: ContextMenuKind, f: &Fixture) -> Vec<ContextMenuEntry> {
    let view_mode = f.app_state.get("viewModeEnabled") == Some(&json!(true));
    let items = get_context_menu_items(kind, view_mode, f.env.form_factor);
    build_context_menu(&items, &f.ctx(), &KeyLabels::EN)
}

#[test]
fn context_menu_item_lists() {
    use ContextMenuItem::{Action as A, Separator as S};
    let canvas = get_context_menu_items(ContextMenuKind::Canvas, false, FormFactor::Desktop);
    assert_eq!(
        canvas,
        vec![
            A(n("paste")),
            S,
            A(n("copyAsPng")),
            A(n("copyAsSvg")),
            A(n("copyText")),
            S,
            A(n("selectAll")),
            A(n("unlockAllElements")),
            S,
            A(n("gridMode")),
            A(n("objectsSnapMode")),
            A(n("arrowBinding")),
            A(n("midpointSnapping")),
            A(n("zenMode")),
            A(n("viewMode")),
            A(n("stats")),
        ]
    );
    assert_eq!(
        get_context_menu_items(ContextMenuKind::Canvas, true, FormFactor::Desktop),
        vec![
            A(n("copyAsPng")),
            A(n("copyAsSvg")),
            A(n("gridMode")),
            A(n("zenMode")),
            A(n("viewMode")),
            A(n("stats")),
        ]
    );
    assert_eq!(
        get_context_menu_items(ContextMenuKind::Element, true, FormFactor::Desktop),
        vec![A(n("copy")), A(n("copyAsPng")), A(n("copyAsSvg")), A(n("copyText"))]
    );
    let names = |items: Vec<ContextMenuItem>| -> Vec<&'static str> {
        items
            .into_iter()
            .map(|i| match i {
                A(a) => a.as_str(),
                S => "|",
            })
            .collect()
    };
    assert_eq!(
        names(get_context_menu_items(ContextMenuKind::Element, false, FormFactor::Desktop)),
        [
            "|", "cut", "copy", "paste", "|", "selectAllElementsInFrame",
            "removeAllElementsFromFrame", "wrapSelectionInFrame", "|", "cropEditor", "|",
            "copyAsPng", "copyAsSvg", "copyText", "|", "copyStyles", "pasteStyles", "|",
            "group", "autoResize", "unbindText", "bindText", "wrapTextInContainer", "ungroup",
            "|", "addToLibrary", "|", "sendBackward", "bringForward", "sendToBack",
            "bringToFront", "|", "flipHorizontal", "flipVertical", "|", "toggleLinearEditor",
            "|", "hyperlink", "copyElementLink", "|", "duplicateSelection",
            "toggleElementLock", "|", "deleteSelectedElements",
        ]
    );
    // z-order only on desktop.
    for ff in [FormFactor::Phone, FormFactor::Tablet] {
        let items = names(get_context_menu_items(ContextMenuKind::Element, false, ff));
        assert!(!items.contains(&"sendBackward"));
        assert!(!items.contains(&"bringToFront"));
        assert!(items.contains(&"addToLibrary"));
    }
}

/// contextmenu.test.tsx: "shows context menu for canvas".
#[test]
fn context_menu_for_canvas() {
    let f = Fixture::new(vec![], json!({}));
    let entries = menu(ContextMenuKind::Canvas, &f);
    assert_eq!(
        item_names(&entries),
        [
            "paste",
            "selectAll",
            "gridMode",
            "objectsSnapMode",
            "arrowBinding",
            "midpointSnapping",
            "zenMode",
            "viewMode",
            "stats"
        ]
    );
    // Separators never lead or repeat.
    let shape: Vec<bool> = entries
        .iter()
        .map(|e| matches!(e, ContextMenuEntry::Separator))
        .collect();
    assert_eq!(
        shape,
        [false, true, false, true, false, false, false, false, false, false, false]
    );
    let ContextMenuEntry::Item(paste) = &entries[0] else {
        panic!()
    };
    assert_eq!(paste.label, "labels.paste");
    assert_eq!(paste.shortcut, "Ctrl+V");
    assert!(!paste.dangerous);
    let checked: Vec<&str> = entries
        .iter()
        .filter_map(|e| match e {
            ContextMenuEntry::Item(i) if i.checked => Some(i.name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(checked, ["arrowBinding", "midpointSnapping"]);
}

/// contextmenu.test.tsx: "shows context menu for element".
#[test]
fn context_menu_for_one_element() {
    let f = Fixture::selected(vec![rect("a")], &["a"]);
    let got: BTreeSet<&str> = item_names(&menu(ContextMenuKind::Element, &f)).into_iter().collect();
    let expected = BTreeSet::from([
        "cut",
        "copy",
        "paste",
        "wrapSelectionInFrame",
        "copyStyles",
        "pasteStyles",
        "deleteSelectedElements",
        "addToLibrary",
        "flipHorizontal",
        "flipVertical",
        "sendBackward",
        "bringForward",
        "sendToBack",
        "bringToFront",
        "duplicateSelection",
        "hyperlink",
        "copyElementLink",
        "toggleElementLock",
    ]);
    assert_eq!(got, expected);
    assert_eq!(item_names(&menu(ContextMenuKind::Element, &f)).len(), 18);
    let entries = menu(ContextMenuKind::Element, &f);
    let delete = entries
        .iter()
        .find_map(|e| match e {
            ContextMenuEntry::Item(i) if i.name == ActionName::DeleteSelectedElements => Some(i),
            _ => None,
        })
        .unwrap();
    assert!(delete.dangerous);
    assert_eq!(delete.label, "labels.delete");
    assert_eq!(delete.shortcut, "Delete");
    let lock = entries
        .iter()
        .find_map(|e| match e {
            ContextMenuEntry::Item(i) if i.name == ActionName::ToggleElementLock => Some(i),
            _ => None,
        })
        .unwrap();
    assert_eq!(lock.label, "labels.elementLock.lock");
    assert_eq!(lock.shortcut, "Ctrl+Shift+L");
    // The element menu starts with a separator in the list, never on screen.
    assert!(matches!(entries[0], ContextMenuEntry::Item(_)));
}

/// contextmenu.test.tsx: "shows 'Group selection' in context menu for
/// multiple selected elements".
#[test]
fn context_menu_for_two_elements() {
    let f = Fixture::selected(vec![rect("a"), rect("b")], &["a", "b"]);
    let got: Vec<&str> = item_names(&menu(ContextMenuKind::Element, &f));
    let expected = BTreeSet::from([
        "cut",
        "copy",
        "paste",
        "wrapSelectionInFrame",
        "copyStyles",
        "pasteStyles",
        "deleteSelectedElements",
        "group",
        "addToLibrary",
        "flipHorizontal",
        "flipVertical",
        "sendBackward",
        "bringForward",
        "sendToBack",
        "bringToFront",
        "duplicateSelection",
        "toggleElementLock",
    ]);
    assert_eq!(got.len(), expected.len());
    assert_eq!(got.into_iter().collect::<BTreeSet<_>>(), expected);
}

/// contextmenu.test.tsx: "shows 'Ungroup selection' in context menu for
/// group inside selected elements".
#[test]
fn context_menu_for_a_group() {
    let f = Fixture::new(
        vec![grouped(rect("a"), &["g"]), grouped(rect("b"), &["g"])],
        json!({"selectedElementIds": selecting(&["a", "b"]), "selectedGroupIds": {"g": true}}),
    );
    let got: Vec<&str> = item_names(&menu(ContextMenuKind::Element, &f));
    let expected = BTreeSet::from([
        "cut",
        "copy",
        "paste",
        "wrapSelectionInFrame",
        "copyStyles",
        "pasteStyles",
        "deleteSelectedElements",
        "copyElementLink",
        "ungroup",
        "addToLibrary",
        "flipHorizontal",
        "flipVertical",
        "sendBackward",
        "bringForward",
        "sendToBack",
        "bringToFront",
        "duplicateSelection",
        "toggleElementLock",
    ]);
    assert_eq!(got.len(), expected.len());
    assert_eq!(got.into_iter().collect::<BTreeSet<_>>(), expected);
}

/// actionElementLock.test.tsx: unlock-all shows only with locked elements.
#[test]
fn context_menu_unlock_all() {
    let f = Fixture::new(vec![], json!({}));
    assert!(!item_names(&menu(ContextMenuKind::Canvas, &f)).contains(&"unlockAllElements"));
    let f = Fixture::new(vec![locked(rect("a")), locked(rect("b"))], json!({}));
    let entries = menu(ContextMenuKind::Canvas, &f);
    assert!(item_names(&entries).contains(&"unlockAllElements"));
    let unlock = entries
        .iter()
        .find_map(|e| match e {
            ContextMenuEntry::Item(i) if i.name == ActionName::UnlockAllElements => Some(i),
            _ => None,
        })
        .unwrap();
    assert_eq!(unlock.label, "labels.elementLock.unlockAll");
    assert_eq!(unlock.shortcut, "");
}

#[test]
fn context_menu_in_view_mode_and_with_clipboard() {
    let mut f = Fixture::new(vec![rect("a")], json!({"viewModeEnabled": true}));
    f.env.clipboard_blob = true;
    f.env.clipboard_write_text = true;
    assert_eq!(
        item_names(&menu(ContextMenuKind::Canvas, &f)),
        ["copyAsPng", "copyAsSvg", "gridMode", "zenMode", "viewMode", "stats"]
    );
    let entries = menu(ContextMenuKind::Canvas, &f);
    let ContextMenuEntry::Item(view) = &entries[4] else {
        panic!()
    };
    assert!(view.checked);
    assert_eq!(view.shortcut, "Alt+R");
    let ContextMenuEntry::Item(png) = &entries[0] else {
        panic!()
    };
    assert_eq!(png.shortcut, "Shift+Alt+C");
    // copyText needs a selected text.
    assert_eq!(item_names(&menu(ContextMenuKind::Element, &f)), ["copy", "copyAsPng", "copyAsSvg"]);
}

#[test]
fn context_menu_keeps_a_trailing_separator_like_upstream() {
    // ContextMenu.tsx only drops a separator after nothing or after
    // another separator; one at the end stays.
    let f = Fixture::new(vec![], json!({}));
    let items = [
        ContextMenuItem::Action(ActionName::Paste),
        ContextMenuItem::Separator,
        ContextMenuItem::Action(ActionName::CropEditor),
    ];
    let entries = build_context_menu(&items, &f.ctx(), &KeyLabels::EN);
    assert_eq!(entries.len(), 2);
    assert!(matches!(entries[1], ContextMenuEntry::Separator));
}

// ---------------------------------------------------------------------------
// Command palette (CommandPalette.tsx)
// ---------------------------------------------------------------------------

#[test]
fn palette_categories_and_order() {
    let cats = [
        (PaletteCategory::App, "App", 1),
        (PaletteCategory::Export, "Export", 2),
        (PaletteCategory::Editor, "Editor", 3),
        (PaletteCategory::Tools, "Tools", 4),
        (PaletteCategory::Elements, "Elements", 5),
        (PaletteCategory::Links, "Links", 6),
        (PaletteCategory::Library, "Library", 10),
    ];
    for (cat, name, order) in cats {
        assert_eq!(cat.as_str(), name);
        assert_eq!(cat.order(), order);
    }
}

#[test]
fn palette_commands_from_actions() {
    let commands = commands_from_actions();
    let listed: Vec<(&str, &str)> = commands
        .iter()
        .map(|c| {
            let name = match c.source {
                PaletteCommandSource::Action(a) => a.as_str(),
                PaletteCommandSource::ClearCanvas => "<clearCanvas>",
                PaletteCommandSource::ImageExport => "<imageExport>",
            };
            (name, c.category.as_str())
        })
        .collect();
    let mut expected: Vec<(&str, &str)> = [
        "group", "ungroup", "cut", "copy", "deleteSelectedElements", "wrapSelectionInFrame",
        "copyStyles", "pasteStyles", "bringToFront", "bringForward", "sendBackward",
        "sendToBack", "alignTop", "alignBottom", "alignLeft", "alignRight",
        "alignVerticallyCentered", "alignHorizontallyCentered", "duplicateSelection",
        "flipHorizontal", "flipVertical", "zoomToFitSelection", "zoomToFitSelectionInViewport",
        "increaseFontSize", "decreaseFontSize", "toggleLinearEditor", "cropEditor",
        "togglePolygon", "hyperlink", "copyElementLink", "linkToElement",
    ]
    .into_iter()
    .map(|a| (a, "Elements"))
    .collect();
    expected.extend(
        [
            "undo", "redo", "zoomIn", "zoomOut", "resetZoom", "zoomToFit", "zenMode",
            "viewMode", "gridMode", "objectsSnapMode", "toggleShortcuts", "selectAll",
            "toggleElementLock", "unlockAllElements", "stats",
        ]
        .into_iter()
        .map(|a| (a, "Editor")),
    );
    expected.push(("<clearCanvas>", "Editor"));
    expected.push(("<imageExport>", "Export"));
    expected.extend(
        ["saveToActiveFile", "saveFileToDisk", "copyAsPng", "copyAsSvg"]
            .into_iter()
            .map(|a| (a, "Export")),
    );
    expected.push(("toggleTheme", "App"));
    assert_eq!(listed, expected);
}

#[test]
fn palette_command_details() {
    let commands = commands_from_actions();
    let find = |source: PaletteCommandSource| commands.iter().find(|c| c.source == source).unwrap();
    let f = Fixture::new(vec![], json!({}));
    let clear = find(PaletteCommandSource::ClearCanvas);
    assert_eq!(clear.label_key(&f.ctx()), "labels.clearCanvas");
    assert_eq!(clear.shortcut(false, &KeyLabels::EN), "Ctrl+Delete");
    assert_eq!(clear.keywords(), ["delete", "destroy"]);
    assert_eq!(clear.view_mode(), Some(false));
    let image = find(PaletteCommandSource::ImageExport);
    assert_eq!(image.label_key(&f.ctx()), "buttons.exportImage");
    assert_eq!(image.shortcut(true, &KeyLabels::EN), "Cmd+Shift+E");
    assert_eq!(
        image.keywords(),
        ["export", "image", "png", "jpeg", "svg", "clipboard", "picture"]
    );
    assert_eq!(image.view_mode(), None);
    let cut = find(PaletteCommandSource::Action(ActionName::Cut));
    assert_eq!(cut.label_key(&f.ctx()), "labels.cut");
    assert_eq!(cut.shortcut(false, &KeyLabels::EN), "Ctrl+X");
    let theme = find(PaletteCommandSource::Action(ActionName::ToggleTheme));
    assert_eq!(theme.label_key(&f.ctx()), "buttons.darkMode");
    assert_eq!(theme.keywords(), ["toggle", "dark", "light", "mode", "theme"]);
    assert_eq!(theme.view_mode(), Some(true));
    let polygon = find(PaletteCommandSource::Action(ActionName::TogglePolygon));
    assert_eq!(polygon.keywords(), ["loop"]);
    assert_eq!(polygon.shortcut(false, &KeyLabels::EN), "");
}

#[test]
fn palette_availability() {
    let commands = commands_from_actions();
    let find = |source: PaletteCommandSource| commands.iter().find(|c| c.source == source).unwrap();
    let cut = find(PaletteCommandSource::Action(ActionName::Cut));
    let group = find(PaletteCommandSource::Action(ActionName::Group));
    let undo = find(PaletteCommandSource::Action(ActionName::Undo));
    let zoom_in = find(PaletteCommandSource::Action(ActionName::ZoomIn));
    let clear = find(PaletteCommandSource::ClearCanvas);
    let image = find(PaletteCommandSource::ImageExport);
    let select_all = find(PaletteCommandSource::Action(ActionName::SelectAll));
    let hyperlink = find(PaletteCommandSource::Action(ActionName::Hyperlink));

    // Elements commands without a predicate need a selection.
    let empty = Fixture::new(vec![rect("a")], json!({}));
    assert!(!palette_command_available(cut, &empty.ctx()));
    let one = Fixture::selected(vec![rect("a")], &["a"]);
    assert!(palette_command_available(cut, &one.ctx()));
    // Elements commands with a predicate use it.
    assert!(!palette_command_available(group, &one.ctx()));
    assert!(palette_command_available(hyperlink, &one.ctx()));
    // Editor commands without a predicate are always there.
    assert!(palette_command_available(select_all, &empty.ctx()));
    assert!(palette_command_available(undo, &empty.ctx()));
    assert!(palette_command_available(clear, &empty.ctx()));
    assert!(palette_command_available(image, &empty.ctx()));
    // viewMode: false hides a command in view mode; unset does not.
    let view = Fixture::selected(vec![rect("a")], &["a"]);
    let mut view = view;
    view.app_state.insert("viewModeEnabled", json!(true));
    assert!(!palette_command_available(undo, &view.ctx()));
    assert!(!palette_command_available(clear, &view.ctx()));
    assert!(!palette_command_available(select_all, &view.ctx()));
    assert!(palette_command_available(cut, &view.ctx()));
    assert!(palette_command_available(image, &view.ctx()));
    assert!(palette_command_available(zoom_in, &view.ctx()));
}

// ---------------------------------------------------------------------------
// Styles panel, full mode (Actions.tsx:63-217)
// ---------------------------------------------------------------------------

#[test]
fn full_styles_panel_order() {
    use PanelGate as G;
    let panel = full_styles_panel(false, false);
    let got: Vec<(&str, Vec<PanelGate>, PanelFieldset)> = panel
        .iter()
        .map(|c| (c.action.as_str(), c.gates.to_vec(), c.fieldset))
        .collect();
    let expected: Vec<(&str, Vec<PanelGate>, PanelFieldset)> = vec![
        ("changeStrokeColor", vec![G::StrokeColor], PanelFieldset::None),
        ("changeBackgroundColor", vec![G::BackgroundColor], PanelFieldset::None),
        ("changeFillStyle", vec![G::Fill], PanelFieldset::None),
        ("changeStrokeWidth", vec![G::StrokeWidth], PanelFieldset::None),
        ("changeStrokeStyle", vec![G::StrokeStyle], PanelFieldset::None),
        ("changeFreedrawMode", vec![G::FreedrawMode], PanelFieldset::None),
        ("changeSloppiness", vec![G::Sloppiness], PanelFieldset::None),
        ("changeRoundness", vec![G::Roundness], PanelFieldset::None),
        ("changeArrowType", vec![G::ArrowType], PanelFieldset::None),
        ("changeFontFamily", vec![G::Text], PanelFieldset::FontFamily),
        ("changeFontSize", vec![G::Text], PanelFieldset::None),
        ("changeTextAlign", vec![G::Text, G::TextAlign], PanelFieldset::None),
        ("changeVerticalAlign", vec![G::VerticalAlign], PanelFieldset::None),
        ("changeArrowhead", vec![G::Arrowheads], PanelFieldset::None),
        ("changeOpacity", vec![G::Opacity], PanelFieldset::None),
        ("sendToBack", vec![G::Layers], PanelFieldset::Layers),
        ("sendBackward", vec![G::Layers], PanelFieldset::Layers),
        ("bringForward", vec![G::Layers], PanelFieldset::Layers),
        ("bringToFront", vec![G::Layers], PanelFieldset::Layers),
        ("alignLeft", vec![G::Align], PanelFieldset::AlignHorizontal),
        ("alignHorizontallyCentered", vec![G::Align], PanelFieldset::AlignHorizontal),
        ("alignRight", vec![G::Align], PanelFieldset::AlignHorizontal),
        ("distributeHorizontally", vec![G::Align, G::Distribute], PanelFieldset::AlignHorizontal),
        ("alignTop", vec![G::Align], PanelFieldset::AlignVertical),
        ("alignVerticallyCentered", vec![G::Align], PanelFieldset::AlignVertical),
        ("alignBottom", vec![G::Align], PanelFieldset::AlignVertical),
        ("distributeVertically", vec![G::Align, G::Distribute], PanelFieldset::AlignVertical),
        ("duplicateSelection", vec![G::ShowExtraActions], PanelFieldset::Actions),
        ("deleteSelectedElements", vec![G::ShowExtraActions], PanelFieldset::Actions),
        ("group", vec![G::ShowExtraActions], PanelFieldset::Actions),
        ("ungroup", vec![G::ShowExtraActions], PanelFieldset::Actions),
        ("hyperlink", vec![G::ShowExtraActions, G::Link], PanelFieldset::Actions),
        ("cropEditor", vec![G::ShowExtraActions, G::CropEditor], PanelFieldset::Actions),
        ("toggleLinearEditor", vec![G::ShowExtraActions, G::LineEditor], PanelFieldset::Actions),
    ];
    assert_eq!(got, expected);
    // RTL mirrors the horizontal align row only.
    let rtl: Vec<&str> = full_styles_panel(false, true)
        .iter()
        .filter(|c| c.fieldset == PanelFieldset::AlignHorizontal)
        .map(|c| c.action.as_str())
        .collect();
    assert_eq!(rtl, ["alignRight", "alignHorizontallyCentered", "alignLeft", "distributeHorizontally"]);
    // The bucket fill tool: colour, fill style and opacity, unconditionally.
    let bucket: Vec<(&str, usize)> = full_styles_panel(true, false)
        .iter()
        .map(|c| (c.action.as_str(), c.gates.len()))
        .collect();
    assert_eq!(
        bucket,
        [("changeBucketFillBackgroundColor", 0), ("changeFillStyle", 0), ("changeOpacity", 0)]
    );
}

#[test]
fn styles_panel_renders_through_the_registry() {
    let f = Fixture::new(vec![], json!({}));
    let manager = ActionManager::new();
    let panel = full_styles_panel(false, false);
    let all = render_styles_panel(&panel, |_| true, &manager, &f.ctx());
    assert_eq!(all.len(), panel.len());
    let no_distribute =
        render_styles_panel(&panel, |g| g != PanelGate::Distribute, &manager, &f.ctx());
    assert!(!no_distribute.contains(&ActionName::DistributeHorizontally));
    assert!(no_distribute.contains(&ActionName::AlignLeft));
    let text_only = render_styles_panel(
        &panel,
        |g| matches!(g, PanelGate::Text | PanelGate::Opacity),
        &manager,
        &f.ctx(),
    );
    assert_eq!(
        text_only,
        [ActionName::ChangeFontFamily, ActionName::ChangeFontSize, ActionName::ChangeOpacity]
    );
    // An unregistered action renders nothing.
    let mut partial = ActionManager::empty();
    partial.register(ActionName::ChangeOpacity);
    assert_eq!(
        render_styles_panel(&panel, |_| true, &partial, &f.ctx()),
        [ActionName::ChangeOpacity]
    );
}

// ---------------------------------------------------------------------------
// Main menu (LayerUI.tsx:111-136, DefaultItems.tsx)
// ---------------------------------------------------------------------------

fn menu_items(entries: &[MainMenuEntry]) -> Vec<String> {
    entries
        .iter()
        .map(|e| match e {
            MainMenuEntry::Item(i) => format!("{:?}", i.item),
            MainMenuEntry::Separator => "|".to_owned(),
            MainMenuEntry::Group { title, items } => format!(
                "{title}[{}]",
                items.iter().map(|i| format!("{:?}", i.item)).collect::<Vec<_>>().join(",")
            ),
        })
        .collect()
}

#[test]
fn default_main_menu_items() {
    let manager = ActionManager::new();
    let f = Fixture::new(vec![], json!({}));
    let entries = default_main_menu(&manager, &f.ctx(), &KeyLabels::EN);
    assert_eq!(
        menu_items(&entries),
        [
            "LoadScene",
            "Export",
            "SaveAsImage",
            "SearchMenu",
            "Help",
            "ClearCanvas",
            "|",
            "Excalidraw links[Socials]",
            "|",
            "ChangeCanvasBackground"
        ]
    );
    let details: Vec<(MainMenuItem, Option<ActionName>, &str, String)> = entries
        .iter()
        .filter_map(|e| match e {
            MainMenuEntry::Item(i) => Some((i.item, i.action, i.label, i.shortcut.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        details,
        [
            (MainMenuItem::LoadScene, Some(ActionName::LoadScene), "buttons.load", "Ctrl+O".to_owned()),
            (MainMenuItem::Export, None, "buttons.export", String::new()),
            (MainMenuItem::SaveAsImage, None, "buttons.exportImage", "Ctrl+Shift+E".to_owned()),
            (MainMenuItem::SearchMenu, Some(ActionName::SearchMenu), "search.title", "Ctrl+F".to_owned()),
            (MainMenuItem::Help, Some(ActionName::ToggleShortcuts), "helpDialog.title", "?".to_owned()),
            (MainMenuItem::ClearCanvas, Some(ActionName::ClearCanvas), "buttons.clearReset", String::new()),
            (MainMenuItem::ChangeCanvasBackground, Some(ActionName::ChangeViewBackgroundColor), "labels.canvasBackground", String::new()),
        ]
    );

    // With a file handle and the theme toggle.
    let mut f = Fixture::new(vec![], json!({"fileHandle": {}}));
    f.props.canvas_actions.toggle_theme = Some(true);
    let entries = default_main_menu(&manager, &f.ctx(), &KeyLabels::EN);
    assert_eq!(
        menu_items(&entries),
        [
            "LoadScene",
            "SaveToActiveFile",
            "Export",
            "SaveAsImage",
            "SearchMenu",
            "Help",
            "ClearCanvas",
            "|",
            "Excalidraw links[Socials]",
            "|",
            "ToggleTheme",
            "ChangeCanvasBackground"
        ]
    );
    let save = entries
        .iter()
        .find_map(|e| match e {
            MainMenuEntry::Item(i) if i.item == MainMenuItem::SaveToActiveFile => Some(i),
            _ => None,
        })
        .unwrap();
    assert_eq!(save.shortcut, "Ctrl+S");
    assert_eq!(save.label, "buttons.save");
    let theme = entries
        .iter()
        .find_map(|e| match e {
            MainMenuEntry::Item(i) if i.item == MainMenuItem::ToggleTheme => Some(i),
            _ => None,
        })
        .unwrap();
    assert_eq!(theme.label, "buttons.darkMode");
    assert_eq!(theme.shortcut, "Shift+Alt+D");

    // View mode hides load, clear and the background picker; host options
    // hide export and save-as-image.
    let mut f = Fixture::new(vec![], json!({"viewModeEnabled": true}));
    f.props.canvas_actions.export = None;
    f.props.canvas_actions.save_as_image = false;
    let entries = default_main_menu(&manager, &f.ctx(), &KeyLabels::EN);
    assert_eq!(
        menu_items(&entries),
        ["SearchMenu", "Help", "|", "Excalidraw links[Socials]", "|"]
    );
}
