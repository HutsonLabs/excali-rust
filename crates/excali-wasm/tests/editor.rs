//! The editor behind `<excali-editor>` natively (ex-530): the host API of
//! the term.hut integration page (`load`, `save`, `getState`, `export`,
//! `importLibrary`, and the `change`, `save-request` and `open-link`
//! events), and undo after a drag laying bound text and bound arrows out
//! again through the real leaf layouts.
//!
//! The scene (`fixtures/bound.excalidraw`) was saved with a stale layout: the
//! label of `box` off-centre and the end of the arrow `link` away from `b`.
//! Dragging and undoing leaves the delta's elements where the history
//! records them, then `redrawElements` lays them out: a no-op layout would
//! bring the stale positions back. Text is measured with upstream's test
//! metric (10 px per code unit). The browser suite (`tests/web/specs/
//! editor.spec.mjs`) runs the same gestures in Chromium.

use excali_core::element::{Element, ElementKind};
use excali_editor::keyboard::Keystroke;
use excali_scene::bounds::{get_element_absolute_coords, ElementsMap};
use excali_scene::render_element::get_link_handle_from_coords;
use excali_svg::FontContent;
use excali_text::text_measurements::CharCountTextMetrics;
use excali_wasm::editor::{
    library_source, Editor, ExportOptions, HostEvent, LibrarySource, PointerInput,
};
use excali_wasm::env::EditorEnv;
use serde_json::Value;

const SCENE: &str = include_str!("fixtures/bound.excalidraw");

fn editor() -> Editor<CharCountTextMetrics> {
    let env = EditorEnv::new(CharCountTextMetrics, 7, || 1.0);
    let mut ed = Editor::new(env, "https://term.hut", false);
    ed.set_viewport(1000.0, 700.0, 0.0, 0.0);
    ed.load(SCENE).expect("the fixture loads");
    ed.take_events();
    ed
}

fn get<'a>(ed: &'a Editor<CharCountTextMetrics>, id: &str) -> &'a Element {
    ed.elements()
        .iter()
        .find(|e| e.base.id == id)
        .unwrap_or_else(|| panic!("no element {id}"))
}

fn drag(ed: &mut Editor<CharCountTextMetrics>, from: [f64; 2], to: [f64; 2]) {
    ed.pointer_down(PointerInput::at(from[0], from[1]));
    let mid = [(from[0] + to[0]) / 2.0, (from[1] + to[1]) / 2.0];
    ed.pointer_move(PointerInput::at(mid[0], mid[1]));
    ed.pointer_move(PointerInput::at(to[0], to[1]));
    ed.pointer_up(PointerInput::at(to[0], to[1]));
}

fn undo(ed: &mut Editor<CharCountTextMetrics>) {
    ed.key_down(&Keystroke::new("z", "KeyZ").ctrl());
}

fn redo(ed: &mut Editor<CharCountTextMetrics>) {
    ed.key_down(&Keystroke::new("z", "KeyZ").ctrl().shift());
}

fn center(e: &Element) -> [f64; 2] {
    [
        e.base.x + e.base.width / 2.0,
        e.base.y + e.base.height / 2.0,
    ]
}

/// The arrow's position and points.
fn arrow(e: &Element) -> (f64, f64, Vec<[f64; 2]>) {
    let points = e.kind.linear().expect("an arrow").points.to_vec();
    (e.base.x, e.base.y, points)
}

#[test]
fn load_keeps_the_stale_layout_of_the_file() {
    let ed = editor();
    let (label, boxed) = (get(&ed, "label"), get(&ed, "box"));
    assert_eq!((label.base.x, label.base.y), (70.0, 70.0));
    assert_ne!(center(label), center(boxed));
    let (_, _, points) = arrow(get(&ed, "link"));
    assert_eq!(points, vec![[0.0, 0.0], [180.0, 40.0]]);
}

#[test]
fn save_writes_the_file_as_serialize_as_json_does() {
    let mut ed = editor();
    let text = ed.save();
    assert!(text.starts_with("{\n  \"type\": \"excalidraw\",\n  \"version\": 2,\n  \"source\": \"https://term.hut\",\n  \"elements\": ["));
    let saved: Value = serde_json::from_str(&text).unwrap();
    let ids: Vec<&str> = saved["elements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["box", "label", "a", "b", "link", "linked"]);
    // cleanAppStateForExport keeps the grid size and background only
    assert_eq!(saved["appState"]["gridSize"], 20);
    assert_eq!(saved["appState"]["viewBackgroundColor"], "#ffffff");
    assert!(saved["appState"].get("selectedElementIds").is_none());
    // loading what was saved and saving again gives the same text
    ed.load(&text).unwrap();
    assert_eq!(ed.save(), text);
}

#[test]
fn load_fails_with_one_sentence() {
    let mut ed = editor();
    for (text, expected) in [
        ("not json", "The text is not valid JSON ("),
        (
            r#"{"type":"excalidrawlib","libraryItems":[]}"#,
            "The JSON is not an Excalidraw scene.",
        ),
    ] {
        let err = ed.load(text).unwrap_err();
        assert!(err.starts_with(expected), "{err}");
        assert!(err.ends_with('.') && !err.contains('\n'), "{err}");
    }
    // a failed load keeps the scene
    assert_eq!(ed.elements().len(), 6);
}

#[test]
fn get_state_reports_the_documented_keys() {
    let mut ed = editor();
    let state = ed.state();
    assert_eq!(
        state,
        serde_json::json!({
            "dirty": false,
            "elementCount": 6,
            "zoom": 1.0,
            "selectionCount": 0,
            "activeTool": "selection",
        })
    );
    ed.pointer_down(PointerInput::at(160.0, 110.0));
    ed.pointer_up(PointerInput::at(160.0, 110.0));
    assert_eq!(ed.state()["selectionCount"], 1);
    ed.key_down(&Keystroke::new("r", "KeyR"));
    assert_eq!(ed.state()["activeTool"], "rectangle");
}

#[test]
fn change_events_follow_the_dirty_state() {
    let mut ed = editor();
    ed.load(SCENE).unwrap();
    assert_eq!(ed.take_events(), [HostEvent::Change { dirty: false }]);
    drag(&mut ed, [160.0, 110.0], [200.0, 110.0]);
    let events = ed.take_events();
    assert!(
        events.contains(&HostEvent::Change { dirty: true }),
        "{events:?}"
    );
    assert_eq!(events.last(), Some(&HostEvent::Change { dirty: true }));
    assert!(ed.state()["dirty"].as_bool().unwrap());
    ed.save();
    assert_eq!(ed.take_events(), [HostEvent::Change { dirty: false }]);
    assert!(!ed.dirty());
}

#[test]
fn cmd_s_asks_the_host_to_save() {
    let mut ed = editor();
    let out = ed.key_down(&Keystroke::new("s", "KeyS").ctrl());
    assert!(out.prevent_default);
    assert_eq!(ed.take_events(), [HostEvent::SaveRequest]);
    // Shift+Ctrl+S is upstream's "save as", not the host's save
    ed.key_down(&Keystroke::new("S", "KeyS").ctrl().shift());
    assert!(!ed.take_events().contains(&HostEvent::SaveRequest));

    let env = EditorEnv::new(CharCountTextMetrics, 7, || 1.0);
    let mut mac = Editor::new(env, "https://term.hut", true);
    mac.key_down(&Keystroke::new("s", "KeyS").meta());
    assert_eq!(mac.take_events(), [HostEvent::SaveRequest]);
}

#[test]
fn undoing_a_container_move_lays_its_label_out_again() {
    let mut ed = editor();
    drag(&mut ed, [160.0, 110.0], [200.0, 110.0]);
    assert_eq!(get(&ed, "box").base.x, 100.0);
    // the drag moves the label with its container, stale offset and all
    assert_eq!(
        (get(&ed, "label").base.x, get(&ed, "label").base.y),
        (110.0, 70.0)
    );

    undo(&mut ed);
    let (boxed, label) = (get(&ed, "box"), get(&ed, "label"));
    assert_eq!((boxed.base.x, boxed.base.y), (60.0, 60.0));
    // redrawTextBoundingBox: re-measured (5 code units, 20 px, 1.25) and
    // centred in the container
    assert_eq!((label.base.width, label.base.height), (50.0, 25.0));
    assert_eq!(center(label), center(boxed));
    assert_eq!((label.base.x, label.base.y), (135.0, 97.5));
    if let ElementKind::Text(t) = &label.kind {
        assert_eq!(t.text, "label");
    }

    redo(&mut ed);
    let (boxed, label) = (get(&ed, "box"), get(&ed, "label"));
    assert_eq!(boxed.base.x, 100.0);
    assert_eq!(center(label), center(boxed));
}

#[test]
fn undoing_a_bound_rectangle_move_re_routes_its_arrow() {
    let mut ed = editor();
    let stale = arrow(get(&ed, "link"));
    drag(&mut ed, [450.0, 350.0], [490.0, 350.0]);
    assert_eq!(get(&ed, "b").base.x, 440.0);
    let moved = arrow(get(&ed, "link"));
    assert_ne!(moved, stale);
    drag(&mut ed, [490.0, 350.0], [450.0, 350.0]);
    assert_eq!(get(&ed, "b").base.x, 400.0);
    // the arrow laid out against b where the file has it
    let laid_out = arrow(get(&ed, "link"));
    assert_ne!(laid_out, stale);

    undo(&mut ed);
    assert_eq!(get(&ed, "b").base.x, 440.0);
    assert_eq!(arrow(get(&ed, "link")), moved);
    undo(&mut ed);
    assert_eq!(get(&ed, "b").base.x, 400.0);
    // the history restored the stale arrow, then updateBoundElements
    // routed it to b again
    assert_eq!(arrow(get(&ed, "link")), laid_out);

    redo(&mut ed);
    assert_eq!(arrow(get(&ed, "link")), moved);
}

#[test]
fn pressing_a_link_icon_asks_the_host_to_open_it() {
    let mut ed = editor();
    let linked = get(&ed, "linked").clone();
    let map = ElementsMap::new(std::iter::once(&linked));
    let [x1, y1, x2, y2, ..] = get_element_absolute_coords(&linked, &map, false);
    let [lx, ly, lw, lh] = get_link_handle_from_coords([x1, y1, x2, y2], 0.0, 1.0);
    let (x, y) = (lx + lw / 2.0, ly + lh / 2.0);
    ed.pointer_down(PointerInput::at(x, y));
    ed.pointer_up(PointerInput::at(x, y));
    assert_eq!(
        ed.take_events(),
        [HostEvent::OpenLink {
            href: "https://example.com/docs".into()
        }]
    );
    assert_eq!(ed.state()["selectionCount"], 0);
    // released elsewhere: nothing
    ed.pointer_down(PointerInput::at(x, y));
    ed.pointer_up(PointerInput::at(x + 100.0, y + 100.0));
    assert!(ed.take_events().is_empty());
}

const LIBRARY: &str = r##"{"type":"excalidrawlib","version":2,"source":"https://excalidraw.com","libraryItems":[
{"id":"item-1","status":"published","created":1,"name":"one","elements":[
{"id":"r1","type":"rectangle","x":0,"y":0,"width":10,"height":10,"angle":0,"strokeColor":"#1e1e1e","backgroundColor":"transparent","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"groupIds":[],"frameId":null,"index":"a0","roundness":null,"seed":1,"version":1,"versionNonce":1,"isDeleted":false,"boundElements":null,"updated":1,"link":null,"locked":false}]}]}"##;

#[test]
fn import_library_merges_as_update_library_does() {
    let mut ed = editor();
    assert_eq!(ed.import_library(LIBRARY, true), Ok(1));
    // the same item again is not added twice (mergeLibraryItems)
    assert_eq!(ed.import_library(LIBRARY, true), Ok(1));
    assert_eq!(ed.library()[0].name.as_deref(), Some("one"));
    let written: Value = serde_json::from_str(&ed.library_json()).unwrap();
    assert_eq!(written["type"], "excalidrawlib");
    assert_eq!(written["libraryItems"][0]["id"], "item-1");
    // without merge the import replaces the library
    assert_eq!(ed.import_library(LIBRARY, false), Ok(1));
    let err = ed.import_library("{}", true).unwrap_err();
    assert!(
        err.starts_with("The library could not be imported"),
        "{err}"
    );
    assert_eq!(ed.library().len(), 1);
}

#[test]
fn library_urls_are_held_to_upstreams_allow_list() {
    assert_eq!(library_source(LIBRARY), Ok(LibrarySource::Text));
    let url = "https://libraries.excalidraw.com/libraries/x/y.excalidrawlib";
    assert_eq!(library_source(url), Ok(LibrarySource::Url(url.into())));
    let link = format!(
        "https://libraries.excalidraw.com/?target=_excalidraw#addLibrary={}&token=abc",
        url.replace(':', "%3A").replace('/', "%2F")
    );
    assert_eq!(library_source(&link), Ok(LibrarySource::Url(url.into())));
    let err = library_source("https://example.com/x.excalidrawlib").unwrap_err();
    assert_eq!(
        err,
        "Invalid or disallowed library URL: \"https://example.com/x.excalidrawlib\"."
    );
}

/// Fonts by URL, as the browser build writes them.
struct Urls;

impl FontContent for Urls {
    fn content(&self, face: &excali_scene::display::FontFaceSource) -> String {
        format!("fonts/{}", face.file)
    }
}

#[test]
fn export_svg_and_png() {
    let ed = editor();
    let svg = ed.export_svg(&ExportOptions::default(), &Urls).unwrap();
    assert!(svg.contains("<svg version=\"1.1\""), "{svg}");
    assert!(!svg.contains("payload-type:application/vnd.excalidraw+json"));
    assert!(svg.contains("fonts/"), "the label's face by URL");
    let embed = ExportOptions {
        embed_scene: true,
        ..ExportOptions::default()
    };
    let svg = ed.export_svg(&embed, &Urls).unwrap();
    assert!(svg.contains("payload-type:application/vnd.excalidraw+json"));

    let png = ed.export_png(&ExportOptions::default()).unwrap();
    assert!(png.payload.is_none());
    let scaled = ed
        .export_png(&ExportOptions {
            scale: 2.0,
            embed_scene: true,
            ..ExportOptions::default()
        })
        .unwrap();
    assert!(scaled.payload.is_some());
    assert_eq!(scaled.width, png.width * 2);

    let env = EditorEnv::new(CharCountTextMetrics, 7, || 1.0);
    let empty = Editor::new(env, "https://term.hut", false);
    assert_eq!(
        empty.export_png(&ExportOptions::default()).unwrap_err(),
        "Cannot export empty canvas."
    );
}

#[test]
fn export_options_read_the_documented_keys() {
    let opts = ExportOptions::from_json(&serde_json::json!({
        "scale": 2, "background": false, "embedScene": true, "dark": true, "padding": 4,
    }));
    assert_eq!(
        opts,
        ExportOptions {
            scale: 2.0,
            background: false,
            dark: true,
            embed_scene: true,
            padding: Some(4.0),
        }
    );
    assert_eq!(
        ExportOptions::from_json(&Value::Null),
        ExportOptions::default()
    );
}

#[test]
fn the_context_menu_is_the_elements_over_an_element_or_the_selection() {
    use excali_editor::actions::ContextMenuKind;
    let selected = |ed: &Editor<CharCountTextMetrics>| {
        let mut ids: Vec<String> = ed
            .app_state()
            .get("selectedElementIds")
            .and_then(Value::as_object)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        ids.sort();
        ids
    };
    let mut ed = editor();
    // App.openContextMenu: nothing under the pointer is the canvas menu
    assert_eq!(ed.open_context_menu(900.0, 100.0), ContextMenuKind::Canvas);
    assert!(selected(&ed).is_empty());
    // an element not selected becomes the selection
    let a = center(get(&ed, "a"));
    assert_eq!(ed.open_context_menu(a[0], a[1]), ContextMenuKind::Element);
    assert_eq!(selected(&ed), ["a"]);
    // a selected element keeps the selection
    let b = center(get(&ed, "b"));
    ed.pointer_down(PointerInput::at(b[0], b[1]).shift());
    ed.pointer_up(PointerInput::at(b[0], b[1]).shift());
    assert_eq!(selected(&ed), ["a", "b"]);
    assert_eq!(ed.open_context_menu(b[0], b[1]), ContextMenuKind::Element);
    assert_eq!(selected(&ed), ["a", "b"]);
    // inside the two's common bounds, over neither: still the element menu
    // (isHittingCommonBoundingBoxOfSelectedElements), the selection kept
    assert_eq!(ed.open_context_menu(330.0, 310.0), ContextMenuKind::Element);
    assert_eq!(selected(&ed), ["a", "b"]);
    // outside them, past the padding: the canvas menu
    assert_eq!(ed.open_context_menu(330.0, 285.0), ContextMenuKind::Canvas);
}
