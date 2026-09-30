//! The canvas interactions ex-712 left out (ex-713), driven through
//! [`Editor`] with the pointer and keys and checked against upstream at
//! the pin: snapping while dragging, drawing and resizing
//! (`snapDraggedElements`, `snapNewElement`, `snapResizingElements`),
//! frame membership (`getTopLayerFrameAtSceneCoords`,
//! `addElementsToFrame`, `updateFrameMembershipOfSelectedElements`,
//! `getElementsInNewFrame`), Alt+drag duplication
//! (`duplicateDraggedSelection`), point-by-point line and arrow drawing
//! (`multiElement`, `actionFinalize`), the text tool's drag and the
//! double-click's linear element editor.
//!
//! Where upstream has a test of the gesture, the case repeats it with its
//! numbers and cites it; otherwise the expected values follow from the
//! upstream code path cited.
//!
//! The viewport is 1000 × 1000 at the page's origin, so client
//! coordinates are scene coordinates at scroll 0 and zoom 1. Text is
//! measured with upstream's test metric (10 px per code unit).

use excali_core::element::Element;
use excali_editor::keyboard::Keystroke;
use excali_text::text_measurements::CharCountTextMetrics;
use excali_wasm::editor::{Editor, PointerInput};
use excali_wasm::env::EditorEnv;
use serde_json::{json, Value};

type Ed = Editor<CharCountTextMetrics>;

/// An element as `API.createElement` makes it (`tests/helpers/api.ts:
/// 160-360`): at `(x, y)`, `width` wide and as high, transparent unless a
/// background is given.
fn el(ty: &str, id: &str, x: f64, y: f64, width: f64, extra: Value) -> Value {
    let mut e = json!({
        "id": id, "type": ty, "x": x, "y": y, "width": width, "height": width,
        "angle": 0, "strokeColor": "#1e1e1e", "backgroundColor": "transparent",
        "fillStyle": "solid", "strokeWidth": 2, "strokeStyle": "solid", "roughness": 1,
        "opacity": 100, "groupIds": [], "frameId": null, "roundness": null, "seed": 1,
        "version": 1, "versionNonce": 1, "isDeleted": false, "boundElements": null,
        "updated": 1, "link": null, "locked": false,
    });
    if ty == "frame" {
        e["name"] = Value::Null;
    }
    for (k, v) in extra.as_object().cloned().unwrap_or_default() {
        e[k] = v;
    }
    e
}

/// A filled rectangle (hit anywhere inside).
fn filled(id: &str, x: f64, y: f64, width: f64) -> Value {
    el(
        "rectangle",
        id,
        x,
        y,
        width,
        json!({ "backgroundColor": "#ffc9c9" }),
    )
}

fn editor_with(elements: Vec<Value>) -> Ed {
    let env = EditorEnv::new(CharCountTextMetrics, 7, || 1.0);
    let mut ed = Editor::new(env, "https://term.hut", false);
    ed.set_viewport(1000.0, 1000.0, 0.0, 0.0);
    let scene = json!({
        "type": "excalidraw", "version": 2, "source": "https://excalidraw.com",
        "elements": elements,
        "appState": { "gridSize": 20, "viewBackgroundColor": "#ffffff" },
        "files": {},
    });
    ed.load(&scene.to_string()).expect("the scene loads");
    ed.take_events();
    ed
}

fn live(ed: &Ed) -> Vec<&Element> {
    ed.elements()
        .iter()
        .filter(|e| !e.base.is_deleted)
        .collect()
}

fn ids(ed: &Ed) -> Vec<String> {
    live(ed).iter().map(|e| e.base.id.clone()).collect()
}

fn get<'a>(ed: &'a Ed, id: &str) -> &'a Element {
    live(ed)
        .into_iter()
        .find(|e| e.base.id == id)
        .unwrap_or_else(|| panic!("no element {id}"))
}

fn app(ed: &Ed, key: &str) -> Value {
    ed.app_state().get(key).cloned().unwrap_or(Value::Null)
}

/// The id of the element an app state key holds (`frameToHighlight`,
/// `newElement`, `multiElement`), or `null`.
fn app_id(ed: &Ed, key: &str) -> Value {
    app(ed, key).get("id").cloned().unwrap_or(Value::Null)
}

fn tool(ed: &Ed) -> Value {
    ed.state()["activeTool"].clone()
}

/// The ids of `elementsToHighlight`.
fn highlighted(ed: &Ed) -> Value {
    match app(ed, "elementsToHighlight") {
        Value::Array(list) => Value::Array(list.iter().map(|e| e["id"].clone()).collect()),
        other => other,
    }
}

fn key(ed: &mut Ed, stroke: Keystroke) {
    ed.key_down(&stroke);
    ed.key_up(&stroke);
}

fn letter(ed: &mut Ed, k: &str) {
    let code = format!("Key{}", k.to_uppercase());
    key(ed, Keystroke::new(k, &code));
}

/// Alt+S: `actionToggleObjectsSnapMode`.
fn toggle_snapping(ed: &mut Ed) {
    key(ed, Keystroke::new("s", "KeyS").alt());
}

fn at(x: f64, y: f64) -> PointerInput {
    PointerInput::at(x, y)
}

fn alt(x: f64, y: f64) -> PointerInput {
    PointerInput {
        alt_key: true,
        ..at(x, y)
    }
}

fn ctrl(x: f64, y: f64) -> PointerInput {
    PointerInput {
        ctrl_or_cmd: true,
        ..at(x, y)
    }
}

fn click(ed: &mut Ed, p: [f64; 2]) {
    ed.pointer_down(at(p[0], p[1]));
    ed.pointer_up(at(p[0], p[1]));
}

/// `mouse.downAt(from); mouse.moveTo(to); mouse.upAt(to)`: one move.
fn drag(ed: &mut Ed, from: [f64; 2], to: [f64; 2]) {
    ed.pointer_down(at(from[0], from[1]));
    ed.pointer_move(at(to[0], to[1]));
    ed.pointer_up(at(to[0], to[1]));
}

fn xy(e: &Element) -> [f64; 2] {
    [e.base.x, e.base.y]
}

fn frame_of(ed: &Ed, id: &str) -> Option<String> {
    get(ed, id).base.frame_id.clone()
}

fn created(ed: &Ed, before: &[&str]) -> Element {
    let new: Vec<&Element> = live(ed)
        .into_iter()
        .filter(|e| !before.contains(&e.base.id.as_str()))
        .collect();
    assert_eq!(new.len(), 1, "one new element: {new:?}");
    new[0].clone()
}

// -- Alt+drag -----------------------------------------------------------------

#[test]
fn alt_drag_duplicates_the_selection() {
    // move.test.tsx:147-195, "duplicate element on move when ALT is clicked"
    let mut ed = editor_with(vec![]);
    letter(&mut ed, "r");
    ed.pointer_down(at(30.0, 20.0));
    ed.pointer_move(at(60.0, 70.0));
    ed.pointer_up(at(60.0, 70.0));
    assert_eq!(live(&ed).len(), 1);
    assert_eq!(xy(live(&ed)[0]), [30.0, 20.0]);
    let original = live(&ed)[0].base.id.clone();

    ed.pointer_down(at(50.0, 20.0));
    ed.pointer_move(alt(20.0, 40.0));
    // another move with Alt does not duplicate again
    ed.pointer_move(alt(20.0, 40.0));
    ed.pointer_move(at(10.0, 60.0));
    ed.pointer_up(at(10.0, 60.0));

    let elements = live(&ed);
    assert_eq!(elements.len(), 2);
    // the original stays where it was, the duplicate is dragged on
    assert_eq!(elements[0].base.id, original);
    assert_eq!(xy(elements[0]), [30.0, 20.0]);
    assert_eq!(xy(elements[1]), [-10.0, 60.0]);
    assert_ne!(elements[1].base.seed, elements[0].base.seed);
    // the duplicate is the selection
    let selected = app(&ed, "selectedElementIds");
    assert_eq!(selected, json!({ elements[1].base.id.clone(): true }));
    // one undo step removes the duplicate and the move
    key(&mut ed, Keystroke::new("z", "KeyZ").ctrl());
    assert_eq!(live(&ed).len(), 1);
    assert_eq!(xy(live(&ed)[0]), [30.0, 20.0]);
}

// -- Dragging -----------------------------------------------------------------

#[test]
fn shift_drag_moves_along_one_axis() {
    // App.tsx:11070-11088: with Shift the smaller of the two offsets is 0
    let mut ed = editor_with(vec![filled("a", 100.0, 100.0, 100.0)]);
    click(&mut ed, [150.0, 150.0]);
    ed.pointer_down(at(150.0, 150.0));
    ed.pointer_move(at(190.0, 160.0).shift());
    ed.pointer_up(at(190.0, 160.0).shift());
    assert_eq!(xy(get(&ed, "a")), [140.0, 100.0]);
}

/// `b` dragged by (-97, 0): its left edge ends 3 px right of `a`'s right
/// edge.
fn drag_b_near_a(ed: &mut Ed, input: fn(f64, f64) -> PointerInput) {
    click(ed, [350.0, 350.0]);
    ed.pointer_down(at(350.0, 350.0));
    ed.pointer_move(input(253.0, 350.0));
}

fn two() -> Ed {
    editor_with(vec![
        filled("a", 100.0, 100.0, 100.0),
        filled("b", 300.0, 300.0, 100.0),
    ])
}

#[test]
fn a_drag_does_not_snap_by_default() {
    let mut ed = two();
    drag_b_near_a(&mut ed, at);
    assert_eq!(app(&ed, "snapLines"), json!([]));
    ed.pointer_up(at(253.0, 350.0));
    assert_eq!(xy(get(&ed, "b")), [203.0, 300.0]);
}

#[test]
fn a_drag_snaps_with_objects_snap_mode() {
    // snapDraggedElements (snapping.ts:692-864) through App.tsx:11190-11216:
    // b's left edge (203) is within SNAP_DISTANCE (8) of a's right edge
    // (200), so the drag moves 3 px further
    let mut ed = two();
    toggle_snapping(&mut ed);
    assert_eq!(app(&ed, "objectsSnapModeEnabled"), json!(true));
    drag_b_near_a(&mut ed, at);
    let lines = app(&ed, "snapLines");
    assert!(lines.as_array().is_some_and(|l| !l.is_empty()), "{lines}");
    assert_eq!(xy(get(&ed, "b")), [200.0, 300.0]);
    ed.pointer_up(at(253.0, 350.0));
    assert_eq!(xy(get(&ed, "b")), [200.0, 300.0]);
    // the release clears the lines (App.tsx:11546-11565)
    assert_eq!(app(&ed, "snapLines"), json!([]));
}

#[test]
fn ctrl_snaps_while_the_mode_is_off() {
    // isSnappingEnabled (snapping.ts:162-190): the mode off, Ctrl/Cmd held
    // and no grid
    let mut ed = two();
    drag_b_near_a(&mut ed, ctrl);
    ed.pointer_up(ctrl(253.0, 350.0));
    assert_eq!(xy(get(&ed, "b")), [200.0, 300.0]);
}

#[test]
fn a_shape_drawn_snaps_its_dragged_corner() {
    // maybeDragNewGenericElement (App.tsx:13468-13580) with snapNewElement
    // (snapping.ts:1302-1363): the corner (203, 350) snaps to x = 200
    let mut ed = editor_with(vec![filled("a", 100.0, 100.0, 100.0)]);
    toggle_snapping(&mut ed);
    letter(&mut ed, "r");
    drag(&mut ed, [300.0, 300.0], [203.0, 350.0]);
    let r = created(&ed, &["a"]);
    assert_eq!(
        [r.base.x, r.base.y, r.base.width, r.base.height],
        [200.0, 300.0, 100.0, 50.0]
    );
}

#[test]
fn a_shape_tool_snaps_its_origin_while_hovering() {
    // getSnapLinesAtPointer on the move before the press (App.tsx:
    // 8035-8066) gives originSnapOffset, which dragNewElement adds
    // (dragElements.ts:392-396)
    let mut ed = editor_with(vec![filled("a", 100.0, 100.0, 100.0)]);
    toggle_snapping(&mut ed);
    letter(&mut ed, "r");
    ed.pointer_move(at(203.0, 400.0));
    assert!(app(&ed, "snapLines")
        .as_array()
        .is_some_and(|l| !l.is_empty()));
    assert_eq!(app(&ed, "originSnapOffset"), json!({ "x": -3.0, "y": 0.0 }));
    drag(&mut ed, [203.0, 400.0], [253.0, 450.0]);
    let r = created(&ed, &["a"]);
    assert_eq!(
        [r.base.x, r.base.y, r.base.width, r.base.height],
        [200.0, 400.0, 50.0, 50.0]
    );
}

#[test]
fn a_resize_snaps_the_moved_corner() {
    // maybeHandleResize (App.tsx:13684-13800) with snapResizingElements
    // (snapping.ts:1160-1300): a's south-east corner at x = 297 snaps to b's
    // left edge
    let mut ed = two();
    toggle_snapping(&mut ed);
    click(&mut ed, [150.0, 150.0]);
    drag(&mut ed, [206.0, 206.0], [303.0, 250.0]);
    let a = get(&ed, "a");
    assert_eq!(
        [a.base.x, a.base.y, a.base.width, a.base.height],
        [100.0, 100.0, 200.0, 144.0]
    );
    assert_eq!(app(&ed, "snapLines"), json!([]));
}

// -- Frames -------------------------------------------------------------------

/// frame.test.tsx's `frame`: id0 at (0, 0), 150 × 150.
fn frame() -> Value {
    el("frame", "id0", 0.0, 0.0, 150.0, json!({}))
}

fn rect2() -> Value {
    el("rectangle", "id2", 200.0, 0.0, 50.0, json!({}))
}

/// frame.test.tsx's `dragElementIntoFrame`.
fn drag_element_into_frame(ed: &mut Ed, frame: [f64; 4], element: [f64; 4]) {
    click(ed, [element[0], element[1]]);
    ed.pointer_down(at(
        element[0] + element[2] / 2.0,
        element[1] + element[3] / 2.0,
    ));
    ed.pointer_move(at(frame[0] + frame[2] / 2.0, frame[1] + frame[3] / 2.0));
    let p = [frame[0] + frame[2] / 2.0, frame[1] + frame[3] / 2.0];
    ed.pointer_up(at(p[0], p[1]));
}

#[test]
fn a_dragged_element_joins_the_frame() {
    // frame.test.tsx:689-695
    let mut ed = editor_with(vec![rect2(), frame()]);
    drag_element_into_frame(&mut ed, [0.0, 0.0, 150.0, 150.0], [200.0, 0.0, 50.0, 50.0]);
    assert_eq!(frame_of(&ed, "id2").as_deref(), Some("id0"));
    assert_eq!(app_id(&ed, "frameToHighlight"), Value::Null);
}

#[test]
fn a_dragged_element_moves_from_one_frame_to_another() {
    // frame.test.tsx:697-723
    let mut ed = editor_with(vec![
        frame(),
        el(
            "rectangle",
            "frameChild",
            50.0,
            50.0,
            20.0,
            json!({ "frameId": "id0" }),
        ),
        el("frame", "otherFrame", 300.0, 0.0, 150.0, json!({})),
    ]);
    drag_element_into_frame(
        &mut ed,
        [300.0, 0.0, 150.0, 150.0],
        [50.0, 50.0, 20.0, 20.0],
    );
    assert_eq!(frame_of(&ed, "frameChild").as_deref(), Some("otherFrame"));
}

#[test]
fn a_dragged_element_is_layered_above_the_highest_frame_child() {
    // frame.test.tsx:725-748
    let mut ed = editor_with(vec![
        frame(),
        el(
            "rectangle",
            "frameChild",
            10.0,
            10.0,
            20.0,
            json!({ "frameId": "id0" }),
        ),
        rect2(),
    ]);
    drag_element_into_frame(&mut ed, [0.0, 0.0, 150.0, 150.0], [200.0, 0.0, 50.0, 50.0]);
    assert_eq!(frame_of(&ed, "id2").as_deref(), Some("id0"));
    assert_eq!(ids(&ed), ["id0", "frameChild", "id2"]);
}

#[test]
fn a_drag_out_of_the_frame_leaves_it() {
    // updateFrameMembershipOfSelectedElements (frame.ts:697-745) on the
    // release (App.tsx:12094-12181): no frame under the pointer
    let mut ed = editor_with(vec![
        frame(),
        el(
            "rectangle",
            "frameChild",
            50.0,
            50.0,
            20.0,
            json!({ "frameId": "id0" }),
        ),
    ]);
    drag_element_into_frame(
        &mut ed,
        [400.0, 400.0, 100.0, 100.0],
        [50.0, 50.0, 20.0, 20.0],
    );
    assert_eq!(frame_of(&ed, "frameChild"), None);
}

#[test]
fn no_drag_into_a_frame_behind_a_non_frame_element() {
    // frame.test.tsx:964-982
    let mut ed = editor_with(vec![frame(), filled("cover", 10.0, 10.0, 80.0), rect2()]);
    click(&mut ed, [200.0, 0.0]);
    drag(&mut ed, [225.0, 25.0], [20.0, 20.0]);
    assert_eq!(frame_of(&ed, "id2"), None);
}

#[test]
fn a_drag_into_a_frame_over_a_non_frame_element() {
    // frame.test.tsx:984-1002
    let mut ed = editor_with(vec![filled("cover", 10.0, 10.0, 80.0), rect2(), frame()]);
    click(&mut ed, [200.0, 0.0]);
    drag(&mut ed, [225.0, 25.0], [20.0, 20.0]);
    assert_eq!(frame_of(&ed, "id2").as_deref(), Some("id0"));
}

#[test]
fn a_frame_child_dragged_under_a_cover_stays_in_its_frame() {
    // frame.test.tsx:1004-1036
    let mut ed = editor_with(vec![
        el(
            "rectangle",
            "frameChild",
            100.0,
            20.0,
            20.0,
            json!({ "frameId": "id0" }),
        ),
        frame(),
        filled("cover", 10.0, 10.0, 80.0),
    ]);
    click(&mut ed, [100.0, 20.0]);
    ed.pointer_down(at(110.0, 30.0));
    ed.pointer_move(at(20.0, 20.0));
    assert_eq!(app_id(&ed, "frameToHighlight"), json!("id0"));
    ed.pointer_up(at(20.0, 20.0));
    assert_eq!(frame_of(&ed, "frameChild").as_deref(), Some("id0"));
}

#[test]
fn a_new_element_joins_the_frame_over_a_non_frame_element() {
    // frame.test.tsx:189-212
    let mut ed = editor_with(vec![filled("cover", 10.0, 10.0, 80.0), frame()]);
    letter(&mut ed, "r");
    drag(&mut ed, [20.0, 20.0], [40.0, 40.0]);
    let r = created(&ed, &["cover", "id0"]);
    assert_eq!(r.base.frame_id.as_deref(), Some("id0"));
}

#[test]
fn a_new_element_behind_a_non_frame_element_stays_out() {
    // frame.test.tsx:159-187
    let mut ed = editor_with(vec![frame(), filled("cover", 10.0, 10.0, 80.0)]);
    letter(&mut ed, "r");
    drag(&mut ed, [20.0, 20.0], [40.0, 40.0]);
    let r = created(&ed, &["cover", "id0"]);
    assert_eq!(r.base.frame_id, None);
    assert_eq!(ids(&ed), ["id0".to_owned(), "cover".into(), r.base.id]);
}

#[test]
fn a_new_element_goes_behind_a_locked_frame() {
    // frame.test.tsx:270-293
    let mut ed = editor_with(vec![
        frame(),
        el(
            "frame",
            "lockedFrame",
            10.0,
            10.0,
            80.0,
            json!({ "locked": true }),
        ),
    ]);
    letter(&mut ed, "r");
    drag(&mut ed, [20.0, 20.0], [40.0, 40.0]);
    let r = created(&ed, &["id0", "lockedFrame"]);
    assert_eq!(r.base.frame_id.as_deref(), Some("id0"));
}

#[test]
fn a_new_frame_child_is_inserted_below_its_frame() {
    // frame.test.tsx:295-337
    let mut ed = editor_with(vec![
        el(
            "rectangle",
            "frameChildUnderCursor",
            10.0,
            10.0,
            80.0,
            json!({ "backgroundColor": "#ffc9c9", "frameId": "id0" }),
        ),
        el(
            "rectangle",
            "otherFrameChild",
            100.0,
            20.0,
            20.0,
            json!({ "frameId": "id0" }),
        ),
        frame(),
    ]);
    letter(&mut ed, "r");
    drag(&mut ed, [20.0, 20.0], [40.0, 40.0]);
    let r = created(&ed, &["frameChildUnderCursor", "otherFrameChild", "id0"]);
    assert_eq!(r.base.frame_id.as_deref(), Some("id0"));
    assert_eq!(
        ids(&ed),
        [
            "frameChildUnderCursor".to_owned(),
            "otherFrameChild".into(),
            r.base.id,
            "id0".into()
        ]
    );
}

#[test]
fn the_target_frame_is_highlighted_while_drawing() {
    // frame.test.tsx:214-239
    let mut ed = editor_with(vec![frame()]);
    letter(&mut ed, "r");
    ed.pointer_move(at(20.0, 20.0));
    assert_eq!(app_id(&ed, "frameToHighlight"), json!("id0"));
    ed.pointer_move(at(200.0, 200.0));
    assert_eq!(app_id(&ed, "frameToHighlight"), Value::Null);
    ed.pointer_down(at(20.0, 20.0));
    ed.pointer_move(at(40.0, 40.0));
    assert_eq!(app_id(&ed, "frameToHighlight"), json!("id0"));
    ed.pointer_up(at(40.0, 40.0));
    assert_eq!(app_id(&ed, "frameToHighlight"), Value::Null);
}

#[test]
fn a_new_frame_takes_in_the_elements_inside_it() {
    // onPointerUpFromPointerDownHandler (App.tsx:12022-12036):
    // getElementsInNewFrame (frame.ts:380-393) then addElementsToFrame; the
    // elements wholly inside, while drawing highlighted
    // (App.tsx:13574-13588, getElementsInResizingFrame)
    let mut ed = editor_with(vec![
        filled("inside", 100.0, 100.0, 50.0),
        filled("across", 250.0, 100.0, 100.0),
    ]);
    letter(&mut ed, "f");
    ed.pointer_down(at(50.0, 50.0));
    ed.pointer_move(at(300.0, 300.0));
    assert_eq!(highlighted(&ed), json!(["inside"]));
    ed.pointer_up(at(300.0, 300.0));
    let f = created(&ed, &["inside", "across"]);
    assert_eq!(frame_of(&ed, "inside"), Some(f.base.id.clone()));
    assert_eq!(frame_of(&ed, "across"), None);
    assert_eq!(highlighted(&ed), Value::Null);
}

#[test]
fn a_resized_frame_takes_in_what_it_now_covers() {
    // the resize branch of onPointerUpFromPointerDownHandler
    // (App.tsx:12200-12226): replaceAllElementsInFrame with
    // getElementsInResizingFrame
    let mut ed = editor_with(vec![frame(), filled("r", 200.0, 20.0, 50.0)]);
    // select the frame by its outline
    click(&mut ed, [0.0, 75.0]);
    assert_eq!(app(&ed, "selectedElementIds"), json!({ "id0": true }));
    // its south-east handle spans (152, 152)-(160, 160)
    drag(&mut ed, [156.0, 156.0], [306.0, 156.0]);
    assert_eq!(get(&ed, "id0").base.width, 300.0);
    assert_eq!(frame_of(&ed, "r").as_deref(), Some("id0"));
}

// -- Point by point -----------------------------------------------------------

fn enter(ed: &mut Ed) {
    key(ed, Keystroke::new("Enter", "Enter"));
}

fn escape(ed: &mut Ed) {
    key(ed, Keystroke::new("Escape", "Escape"));
}

fn points(e: &Element) -> Vec<[f64; 2]> {
    e.kind.points().expect("a linear element").to_vec()
}

fn multi_points(ed: &Ed) -> usize {
    app(ed, "multiElement")["points"]
        .as_array()
        .map_or(0, Vec::len)
}

/// multiPointCreate.test.tsx's gesture: a click, a move, a click, a move,
/// a click, Enter.
fn three_clicks(ed: &mut Ed, k: &str) {
    letter(ed, k);
    // the first point on the press, multi-point mode on the release
    ed.pointer_down(at(30.0, 30.0));
    ed.pointer_up(at(30.0, 30.0));
    ed.pointer_move(at(50.0, 60.0));
    // a second point
    ed.pointer_down(at(50.0, 60.0));
    ed.pointer_up(at(50.0, 60.0));
    ed.pointer_move(at(100.0, 140.0));
    // done
    ed.pointer_down(at(100.0, 140.0));
    ed.pointer_up(at(100.0, 140.0));
    enter(ed);
}

#[test]
fn an_arrow_drawn_point_by_point() {
    // multiPointCreate.test.tsx:88-128
    let mut ed = editor_with(vec![]);
    three_clicks(&mut ed, "a");
    let elements = live(&ed);
    assert_eq!(elements.len(), 1);
    let arrow = elements[0];
    assert_eq!(arrow.element_type().as_str(), "arrow");
    assert_eq!(xy(arrow), [30.0, 30.0]);
    assert_eq!(points(arrow), [[0.0, 0.0], [20.0, 30.0], [70.0, 110.0]]);
    // actionFinalize (actionFinalize.tsx:343-418): the tool reverts and the
    // arrow is selected
    assert_eq!(tool(&ed), "selection");
    assert_eq!(app(&ed, "multiElement"), Value::Null);
    assert_eq!(app(&ed, "newElement"), Value::Null);
    assert_eq!(
        app(&ed, "selectedElementIds"),
        json!({ arrow.base.id.clone(): true })
    );
}

#[test]
fn a_line_drawn_point_by_point() {
    // multiPointCreate.test.tsx:129-168
    let mut ed = editor_with(vec![]);
    three_clicks(&mut ed, "l");
    let elements = live(&ed);
    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0].element_type().as_str(), "line");
    assert_eq!(xy(elements[0]), [30.0, 30.0]);
    assert_eq!(
        points(elements[0]),
        [[0.0, 0.0], [20.0, 30.0], [70.0, 110.0]]
    );
}

#[test]
fn no_tool_switch_while_drawing_point_by_point() {
    // multiPointCreate.test.tsx:170-205
    let mut ed = editor_with(vec![]);
    letter(&mut ed, "l");
    ed.pointer_down(at(30.0, 30.0));
    ed.pointer_up(at(30.0, 30.0));
    ed.pointer_move(at(50.0, 60.0));
    letter(&mut ed, "e");
    assert_eq!(tool(&ed), "line");
    ed.pointer_down(at(50.0, 60.0));
    ed.pointer_up(at(50.0, 60.0));
    ed.pointer_move(at(100.0, 140.0));
    ed.pointer_down(at(100.0, 140.0));
    ed.pointer_up(at(100.0, 140.0));
    enter(&mut ed);
    assert_eq!(live(&ed).len(), 1);
}

#[test]
fn escape_drops_the_point_not_yet_placed() {
    // actionFinalize.tsx:238-289: the last point, following the pointer
    // since the last click, is not the last committed one
    let mut ed = editor_with(vec![]);
    letter(&mut ed, "a");
    ed.pointer_down(at(30.0, 30.0));
    ed.pointer_up(at(30.0, 30.0));
    ed.pointer_move(at(50.0, 60.0));
    ed.pointer_down(at(50.0, 60.0));
    ed.pointer_up(at(50.0, 60.0));
    ed.pointer_move(at(100.0, 140.0));
    assert_eq!(multi_points(&ed), 3);
    escape(&mut ed);
    let arrow = live(&ed)[0];
    assert_eq!(points(arrow), [[0.0, 0.0], [20.0, 30.0]]);
    assert_eq!(tool(&ed), "selection");
    assert_eq!(app(&ed, "multiElement"), Value::Null);
}

#[test]
fn a_click_on_the_last_point_finishes() {
    // handleLinearElementOnPointerDown (App.tsx:10292-10317): a press within
    // LINE_CONFIRM_THRESHOLD of the last committed point finalizes; the
    // release reverts the tool (App.tsx:12594-12620)
    let mut ed = editor_with(vec![]);
    letter(&mut ed, "a");
    ed.pointer_down(at(100.0, 100.0));
    ed.pointer_up(at(100.0, 100.0));
    ed.pointer_move(at(200.0, 100.0));
    ed.pointer_down(at(200.0, 100.0));
    ed.pointer_up(at(200.0, 100.0));
    ed.pointer_down(at(202.0, 101.0));
    assert_eq!(app(&ed, "multiElement"), Value::Null);
    ed.pointer_up(at(202.0, 101.0));
    let arrow = live(&ed)[0];
    assert_eq!(points(arrow), [[0.0, 0.0], [100.0, 0.0]]);
    assert_eq!(tool(&ed), "selection");
    assert_eq!(
        app(&ed, "selectedElementIds"),
        json!({ arrow.base.id.clone(): true })
    );
}

#[test]
fn moving_back_to_the_last_point_removes_the_next() {
    // App.tsx:8199-8235: back within the commit zone of the last committed
    // point, the point following the pointer goes
    let mut ed = editor_with(vec![]);
    letter(&mut ed, "l");
    ed.pointer_down(at(100.0, 100.0));
    ed.pointer_up(at(100.0, 100.0));
    ed.pointer_move(at(200.0, 100.0));
    ed.pointer_down(at(200.0, 100.0));
    ed.pointer_up(at(200.0, 100.0));
    ed.pointer_move(at(300.0, 100.0));
    assert_eq!(multi_points(&ed), 3);
    ed.pointer_move(at(202.0, 101.0));
    assert_eq!(multi_points(&ed), 2);
    ed.pointer_move(at(300.0, 150.0));
    assert_eq!(multi_points(&ed), 3);
}

#[test]
fn a_line_closed_on_its_first_point_is_a_polygon() {
    // handleLinearElementOnPointerDown (App.tsx:10218-10240) and
    // actionFinalize (actionFinalize.tsx:303-330): the last point snaps to
    // the first and the line becomes a polygon
    let mut ed = editor_with(vec![]);
    letter(&mut ed, "l");
    for p in [[100.0, 100.0], [200.0, 100.0], [200.0, 200.0]] {
        ed.pointer_move(at(p[0], p[1]));
        ed.pointer_down(at(p[0], p[1]));
        ed.pointer_up(at(p[0], p[1]));
    }
    ed.pointer_move(at(103.0, 102.0));
    ed.pointer_down(at(103.0, 102.0));
    ed.pointer_up(at(103.0, 102.0));
    let line = live(&ed)[0];
    assert_eq!(
        points(line),
        [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 0.0]]
    );
    assert_eq!(json!(line.to_map())["polygon"], json!(true));
    assert_eq!(tool(&ed), "selection");
}

// -- The linear element editor ------------------------------------------------

/// linearElementEditor.test.tsx's `createTwoPointerLinearElement`
/// (`:108-128`): (20, 20) to (60, 20), roughness 0, selected by a click on
/// its start.
fn two_pointer(ty: &str) -> Ed {
    let mut ed = editor_with(vec![el(
        ty,
        "l",
        20.0,
        20.0,
        40.0,
        json!({ "height": 0, "roughness": 0, "points": [[0, 0], [40, 0]] }),
    )]);
    click(&mut ed, [20.0, 20.0]);
    ed
}

fn double_click(ed: &mut Ed, p: [f64; 2]) {
    double_click_with(ed, at(p[0], p[1]));
}

fn double_click_with(ed: &mut Ed, input: PointerInput) {
    ed.pointer_down(input);
    ed.pointer_up(input);
    ed.pointer_down(input);
    ed.pointer_up(input);
    ed.double_click(input);
}

fn editing(ed: &Ed) -> Value {
    app(ed, "selectedLinearElement")["isEditing"].clone()
}

fn line_points(ed: &Ed) -> Vec<[f64; 2]> {
    points(get(ed, "l"))
}

/// `mouse.downAt(from); mouse.moveTo(to); mouse.upAt(to)` with `input`'s
/// modifiers.
fn drag_with(ed: &mut Ed, from: [f64; 2], to: [f64; 2], input: PointerInput) {
    let p = |q: [f64; 2]| PointerInput {
        client_x: q[0],
        client_y: q[1],
        ..input
    };
    ed.pointer_down(p(from));
    ed.pointer_move(p(to));
    ed.pointer_up(p(to));
}

#[test]
fn a_double_click_on_a_line_opens_its_editor() {
    // linearElementEditor.test.tsx:411-418
    let mut ed = two_pointer("line");
    assert_eq!(editing(&ed), json!(false));
    double_click(&mut ed, [20.0, 20.0]);
    assert_eq!(editing(&ed), json!(true));
    assert_eq!(app(&ed, "selectedLinearElement")["elementId"], "l");
}

#[test]
fn a_double_click_on_an_arrow_does_not() {
    // linearElementEditor.test.tsx:420-427
    let mut ed = two_pointer("arrow");
    double_click(&mut ed, [40.0, 20.0]);
    assert_eq!(editing(&ed), json!(false));
}

#[test]
fn ctrl_double_click_opens_an_arrow_editor() {
    // linearElementEditor.test.tsx:389-398
    let mut ed = two_pointer("arrow");
    double_click_with(&mut ed, ctrl(20.0, 20.0));
    assert_eq!(editing(&ed), json!(true));
}

#[test]
fn escape_and_a_click_elsewhere_close_the_editor() {
    // actionFinalize's keyTest (actionFinalize.tsx:421-424); a press off the
    // element (App.tsx:9764-9782)
    let mut ed = two_pointer("line");
    double_click(&mut ed, [20.0, 20.0]);
    escape(&mut ed);
    assert_eq!(editing(&ed), json!(false));
    double_click(&mut ed, [20.0, 20.0]);
    assert_eq!(editing(&ed), json!(true));
    click(&mut ed, [500.0, 500.0]);
    assert_eq!(editing(&ed), json!(false));
    assert_eq!(app(&ed, "selectedElementIds"), json!({ "l": true }));
}

#[test]
fn a_midpoint_dragged_adds_a_point() {
    // linearElementEditor.test.tsx:214-245
    let mut ed = two_pointer("line");
    drag(&mut ed, [40.0, 20.0], [90.0, 70.0]);
    assert_eq!(line_points(&ed), [[0.0, 0.0], [70.0, 50.0], [40.0, 0.0]]);
}

#[test]
fn a_midpoint_adds_a_point_only_past_the_threshold() {
    // linearElementEditor.test.tsx:192-212
    let mut ed = two_pointer("line");
    click(&mut ed, [40.0, 20.0]);
    drag(&mut ed, [40.0, 20.0], [41.0, 21.0]);
    assert_eq!(line_points(&ed).len(), 2);
    assert_eq!(xy(get(&ed, "l")), [20.0, 20.0]);
    drag(&mut ed, [40.0, 20.0], [90.0, 70.0]);
    assert_eq!(xy(get(&ed, "l")), [20.0, 20.0]);
    assert_eq!(line_points(&ed).len(), 3);
}

#[test]
fn in_the_editor_a_midpoint_adds_a_point_at_once() {
    // linearElementEditor.test.tsx:481-498
    let mut ed = two_pointer("line");
    double_click(&mut ed, [20.0, 20.0]);
    click(&mut ed, [40.0, 20.0]);
    assert_eq!(line_points(&ed).len(), 2);
    drag(&mut ed, [40.0, 20.0], [41.0, 21.0]);
    assert_eq!(xy(get(&ed, "l")), [20.0, 20.0]);
    assert_eq!(line_points(&ed).len(), 3);
}

#[test]
fn an_endpoint_dragged_moves() {
    // handlePointDragging (linearElementEditor.ts:471-719): the point under
    // the press follows the pointer (createPointAt)
    let mut ed = two_pointer("line");
    drag(&mut ed, [60.0, 20.0], [80.0, 60.0]);
    assert_eq!(line_points(&ed), [[0.0, 0.0], [60.0, 40.0]]);
    assert_eq!(xy(get(&ed, "l")), [20.0, 20.0]);
    // the start moves the element
    drag(&mut ed, [20.0, 20.0], [30.0, 10.0]);
    assert_eq!(xy(get(&ed, "l")), [30.0, 10.0]);
    assert_eq!(line_points(&ed), [[0.0, 0.0], [50.0, 50.0]]);
}

#[test]
fn shift_keeps_the_angle_of_a_dragged_endpoint() {
    // linearElementEditor.test.tsx:1976-2025
    let mut ed = two_pointer("line");
    double_click(&mut ed, [20.0, 20.0]);
    drag_with(&mut ed, [60.0, 20.0], [64.0, 24.0], at(0.0, 0.0).shift());
    let p = line_points(&ed);
    let angle = js_atan2(p[1][1] - p[0][1], p[1][0] - p[0][0]);
    assert!(angle.abs() < 0.01, "{p:?}");
}

fn js_atan2(y: f64, x: f64) -> f64 {
    excali_math::js::atan2(y, x)
}

#[test]
fn alt_click_in_the_editor_adds_a_point() {
    // LinearElementEditor.handlePointerDown (linearElementEditor.ts:
    // 1094-1134): with Alt the pointer becomes the last point
    let mut ed = two_pointer("line");
    double_click(&mut ed, [20.0, 20.0]);
    ed.pointer_down(alt(100.0, 50.0));
    ed.pointer_up(alt(100.0, 50.0));
    assert_eq!(line_points(&ed), [[0.0, 0.0], [40.0, 0.0], [80.0, 30.0]]);
}

#[test]
fn an_end_dragged_onto_the_start_closes_the_line() {
    // LinearElementEditor.handlePointerUp (linearElementEditor.ts:738-772)
    let mut ed = editor_with(vec![el(
        "line",
        "l",
        100.0,
        100.0,
        100.0,
        json!({ "roughness": 0, "points": [[0, 0], [100, 0], [100, 100]] }),
    )]);
    click(&mut ed, [100.0, 100.0]);
    double_click(&mut ed, [100.0, 100.0]);
    assert_eq!(editing(&ed), json!(true));
    drag(&mut ed, [200.0, 200.0], [103.0, 102.0]);
    let l = get(&ed, "l");
    assert_eq!(points(l).last(), Some(&[0.0, 0.0]));
    assert_eq!(json!(l.to_map())["polygon"], json!(true));
}

// -- Elbow arrows ---------------------------------------------------------------

#[test]
fn an_elbow_segment_moved_and_reset_by_a_double_click() {
    // elbowArrow.test.tsx:89-121
    let mut ed = editor_with(vec![]);
    letter(&mut ed, "a");
    let mut patch = serde_json::Map::new();
    patch.insert("currentItemArrowType".into(), json!("elbow"));
    ed.set_app_state(patch);
    ed.pointer_move(at(0.0, 0.0));
    click(&mut ed, [0.0, 0.0]);
    ed.pointer_move(at(250.0, 200.0));
    click(&mut ed, [250.0, 200.0]);
    ed.pointer_move(at(125.0, 100.0));
    ed.pointer_down(at(125.0, 100.0));
    ed.pointer_move(at(130.0, 100.0));
    ed.pointer_up(at(130.0, 100.0));
    let arrow = live(&ed)[0].clone();
    let close = |got: Vec<[f64; 2]>, want: [[f64; 2]; 4]| {
        assert_eq!(got.len(), 4, "{got:?}");
        for (g, w) in got.iter().zip(want) {
            assert!(
                (g[0] - w[0]).abs() < 1.0 && (g[1] - w[1]).abs() < 1.0,
                "{got:?}"
            );
        }
    };
    assert_eq!(
        app(&ed, "selectedElementIds"),
        json!({ arrow.base.id.clone(): true })
    );
    close(
        points(&arrow),
        [[0.0, 0.0], [130.0, 0.0], [130.0, 200.0], [250.0, 200.0]],
    );
    double_click(&mut ed, [130.0, 100.0]);
    close(
        points(live(&ed)[0]),
        [[0.0, 0.0], [125.0, 0.0], [125.0, 200.0], [250.0, 200.0]],
    );
}

// -- Binding highlight --------------------------------------------------------

fn suggested(ed: &Ed) -> Value {
    app(ed, "suggestedBinding")["element"]["id"].clone()
}

#[test]
fn an_arrow_drawn_over_a_shape_suggests_its_binding() {
    // pointDraggingUpdates (linearElementEditor.ts:2436-2466, 2560-2590):
    // the element the dragged end would bind to is the highlight
    // (interactiveScene.ts:1705-1720); the release clears it
    let mut ed = editor_with(vec![filled("a", 100.0, 100.0, 100.0)]);
    letter(&mut ed, "a");
    ed.pointer_down(at(400.0, 150.0));
    ed.pointer_move(at(300.0, 150.0));
    assert_eq!(app(&ed, "suggestedBinding"), Value::Null);
    ed.pointer_move(at(150.0, 150.0));
    assert_eq!(suggested(&ed), "a");
    ed.pointer_up(at(150.0, 150.0));
    assert_eq!(app(&ed, "suggestedBinding"), Value::Null);
}

#[test]
fn a_point_by_point_arrow_suggests_its_binding() {
    let mut ed = editor_with(vec![filled("a", 100.0, 100.0, 100.0)]);
    letter(&mut ed, "a");
    ed.pointer_down(at(400.0, 150.0));
    ed.pointer_up(at(400.0, 150.0));
    ed.pointer_move(at(150.0, 150.0));
    assert_eq!(suggested(&ed), "a");
}

#[test]
fn a_dragged_arrow_end_suggests_its_binding() {
    let mut ed = editor_with(vec![
        filled("a", 100.0, 100.0, 100.0),
        el(
            "arrow",
            "l",
            300.0,
            150.0,
            100.0,
            json!({ "height": 0, "points": [[0, 0], [100, 0]] }),
        ),
    ]);
    click(&mut ed, [350.0, 150.0]);
    ed.pointer_down(at(300.0, 150.0));
    ed.pointer_move(at(150.0, 150.0));
    assert_eq!(suggested(&ed), "a");
    ed.pointer_up(at(150.0, 150.0));
    assert_eq!(app(&ed, "suggestedBinding"), Value::Null);
    // the end bound where it was dropped
    let arrow = json!(get(&ed, "l").to_map());
    assert_eq!(arrow["startBinding"]["elementId"], "a");
}

// -- What the interactive canvas reads ------------------------------------------

#[test]
fn the_selection_box_is_the_selection_element() {
    // createGenericElementOnPointerDown("selection") and
    // maybeDragNewGenericElement (App.tsx:10536-10600, 13474-13497): the box
    // spans the press and the pointer; the release drops it
    let mut ed = editor_with(vec![filled("a", 100.0, 100.0, 50.0)]);
    ed.pointer_down(at(300.0, 300.0));
    ed.pointer_move(at(250.0, 380.0));
    let s = app(&ed, "selectionElement");
    assert_eq!(s["type"], "selection");
    assert_eq!(
        [&s["x"], &s["y"], &s["width"], &s["height"]],
        [&json!(250.0), &json!(300.0), &json!(50.0), &json!(80.0)]
    );
    ed.pointer_up(at(250.0, 380.0));
    assert_eq!(app(&ed, "selectionElement"), Value::Null);
}

#[test]
fn hovering_a_selected_line_marks_its_point_and_midpoint() {
    // App.tsx:8557-8645: hoverPointIndex, segmentMidPointHoveredCoords
    let mut ed = two_pointer("line");
    ed.pointer_move(at(61.0, 21.0));
    assert_eq!(app(&ed, "selectedLinearElement")["hoverPointIndex"], 1);
    ed.pointer_move(at(40.0, 21.0));
    let l = app(&ed, "selectedLinearElement");
    assert_eq!(l["hoverPointIndex"], -1);
    assert_eq!(l["segmentMidPointHoveredCoords"], json!([40.0, 20.0]));
    ed.pointer_move(at(300.0, 300.0));
    assert_eq!(
        app(&ed, "selectedLinearElement")["segmentMidPointHoveredCoords"],
        Value::Null
    );
}

#[test]
fn hovering_a_shape_with_the_arrow_tool_suggests_it() {
    // App.tsx:8110-8147
    let mut ed = editor_with(vec![filled("a", 100.0, 100.0, 100.0)]);
    letter(&mut ed, "a");
    ed.pointer_move(at(150.0, 150.0));
    assert_eq!(suggested(&ed), "a");
    ed.pointer_move(at(400.0, 400.0));
    assert_eq!(app(&ed, "suggestedBinding"), Value::Null);
}

// -- Locked elements ------------------------------------------------------------

fn locked(id: &str) -> Value {
    el(
        "rectangle",
        id,
        0.0,
        0.0,
        100.0,
        json!({ "backgroundColor": "red", "locked": true }),
    )
}

#[test]
fn a_click_does_not_select_a_locked_element() {
    // elementLocking.test.tsx:23-34; the release marks it (activeLockedId,
    // App.tsx:11578-11614), which the interactive canvas outlines
    let mut ed = editor_with(vec![locked("l")]);
    click(&mut ed, [50.0, 50.0]);
    assert_eq!(app(&ed, "selectedElementIds"), json!({}));
    assert_eq!(app(&ed, "activeLockedId"), "l");
    click(&mut ed, [500.0, 500.0]);
    assert_eq!(app(&ed, "activeLockedId"), Value::Null);
}

#[test]
fn a_locked_element_is_not_dragged() {
    // elementLocking.test.tsx:52-66
    let mut ed = editor_with(vec![locked("l")]);
    drag(&mut ed, [50.0, 50.0], [100.0, 100.0]);
    assert_eq!(xy(get(&ed, "l")), [0.0, 0.0]);
}

#[test]
fn a_locked_element_covers_the_element_below() {
    // elementLocking.test.tsx:68-100 and :111-133
    let mut ed = editor_with(vec![filled("r", 0.0, 0.0, 100.0), locked("l")]);
    click(&mut ed, [50.0, 50.0]);
    assert_eq!(app(&ed, "selectedElementIds"), json!({}));
    assert_eq!(app(&ed, "activeLockedId"), "l");
    drag(&mut ed, [50.0, 50.0], [100.0, 100.0]);
    assert_eq!(xy(get(&ed, "l")), [0.0, 0.0]);
    assert_eq!(xy(get(&ed, "r")), [0.0, 0.0]);
}

// -- Cropping -------------------------------------------------------------------

/// A PNG's signature and header: 400 × 200 pixels.
const PNG_400_200: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAZAAAADICAYAAADGFbfi";

/// cropElement.test.tsx's scene (`:40-52`): a 200 × 100 image at (0, 0),
/// selected; its file 400 × 200.
fn image_scene() -> Ed {
    let env = EditorEnv::new(CharCountTextMetrics, 7, || 1.0);
    let mut ed = Editor::new(env, "https://term.hut", false);
    ed.set_viewport(1000.0, 1000.0, 0.0, 0.0);
    let image = el(
        "image",
        "img",
        0.0,
        0.0,
        200.0,
        json!({
            "height": 100, "fileId": "f1", "status": "saved", "scale": [1, 1], "crop": null,
        }),
    );
    let scene = json!({
        "type": "excalidraw", "version": 2, "source": "https://excalidraw.com",
        "elements": [image],
        "appState": { "gridSize": 20, "viewBackgroundColor": "#ffffff" },
        "files": { "f1": {
            "mimeType": "image/png", "id": "f1", "dataURL": PNG_400_200, "created": 1,
        } },
    });
    ed.load(&scene.to_string()).expect("the scene loads");
    click(&mut ed, [100.0, 50.0]);
    assert_eq!(app(&ed, "selectedElementIds"), json!({ "img": true }));
    ed
}

/// The centre of an image's south-east handle (`getTransformHandles`, no
/// margin for images).
fn se_handle_center(image: &Element) -> [f64; 2] {
    use excali_editor::tools::PointerType;
    use excali_editor::transform_handles::{
        get_omit_sides_for_editor_interface, get_transform_handles, EditorInterface,
    };
    let map = excali_scene::bounds::ElementsMap::new(std::iter::once(image));
    let handles = get_transform_handles(
        image,
        1.0,
        &map,
        PointerType::Mouse,
        &get_omit_sides_for_editor_interface(&EditorInterface::desktop()),
    );
    let [x, y, w, h] = handles.se.expect("a south-east handle");
    [x + w / 2.0, y + h / 2.0]
}

fn cropping(ed: &Ed) -> Value {
    app(ed, "croppingElementId")
}

#[test]
fn a_double_click_on_an_image_crops_it() {
    // cropElement.test.tsx:88-95
    let mut ed = image_scene();
    assert_eq!(cropping(&ed), Value::Null);
    double_click(&mut ed, [100.0, 50.0]);
    assert_eq!(cropping(&ed), "img");
}

#[test]
fn enter_crops_the_selected_image_and_escape_ends() {
    // cropElement.test.tsx:97-103, 113-119
    let mut ed = image_scene();
    enter(&mut ed);
    assert_eq!(cropping(&ed), "img");
    escape(&mut ed);
    assert_eq!(cropping(&ed), Value::Null);
}

#[test]
fn a_click_outside_ends_cropping() {
    // cropElement.test.tsx:105-111
    let mut ed = image_scene();
    enter(&mut ed);
    click(&mut ed, [-20.0, -20.0]);
    assert_eq!(cropping(&ed), Value::Null);
}

#[test]
fn a_crop_handle_dragged_crops() {
    // maybeHandleCrop (App.tsx:13590-13680): cropElement with the pointer
    // less the press's offset from the handle's corner
    use excali_editor::crop::crop_element;
    use excali_editor::transform_handles::{TransformHandleDirection, TransformHandleType};
    let mut ed = image_scene();
    enter(&mut ed);
    let before = get(&ed, "img").clone();
    let se = se_handle_center(&before);
    // the press is at the handle's centre, the corner is where the pointer
    // less that offset is: here the corner (200, 100) moved by (-50, -30)
    drag(&mut ed, se, [se[0] - 50.0, se[1] - 30.0]);
    let map = excali_scene::bounds::ElementsMap::new(std::iter::once(&before));
    let want = crop_element(
        &before,
        &map,
        TransformHandleType::Resize(TransformHandleDirection::Se),
        400.0,
        200.0,
        150.0,
        70.0,
        None,
    );
    let got = get(&ed, "img");
    assert_eq!(
        [got.base.x, got.base.y, got.base.width, got.base.height],
        [want.x, want.y, want.width, want.height]
    );
    assert_eq!(
        json!(got.to_map())["crop"]["width"],
        json!(want.crop.as_ref().unwrap().width)
    );
    assert!(got.base.width < 200.0);
    // still cropping, one undo step
    assert_eq!(cropping(&ed), "img");
    key(&mut ed, Keystroke::new("z", "KeyZ").ctrl());
    assert_eq!(get(&ed, "img").base.width, 200.0);
}

#[test]
fn a_drag_inside_a_cropped_image_moves_the_crop() {
    // App.tsx:11095-11184: the drag moves the crop over the image, in the
    // image's pixels (natural / uncropped size), clamped to the image
    let mut ed = image_scene();
    enter(&mut ed);
    let se = se_handle_center(get(&ed, "img"));
    drag(&mut ed, se, [se[0] - 50.0, se[1] - 30.0]);
    let crop = json!(get(&ed, "img").to_map())["crop"].clone();
    let (x0, w) = (crop["x"].as_f64().unwrap(), crop["width"].as_f64().unwrap());
    assert_eq!(x0, 0.0);
    // right by 10 px of a 200 px wide uncropped image of 400 px: the crop
    // goes 20 px left, clamped at 0; left by 10 moves it 20 px right
    drag(&mut ed, [50.0, 30.0], [40.0, 30.0]);
    let crop = json!(get(&ed, "img").to_map())["crop"].clone();
    assert_eq!(crop["x"], json!(20.0f64.min(400.0 - w)));
    assert_eq!(xy(get(&ed, "img")), [0.0, 0.0]);
}

// -- The text tool's drag and arrow endpoint labels ------------------------------------

fn type_and_submit(ed: &mut Ed, value: &str) {
    use excali_ui::text_editor::TextareaEvent;
    let n = value.encode_utf16().count();
    ed.textarea_event(TextareaEvent::Input {
        value: value.into(),
        selection: (n, n),
    });
    ed.textarea_event(TextareaEvent::Submit);
}

/// `mouse.downAt(from); 4 × mouse.moveTo; mouse.upAt(to)` along y.
fn drag_steps(ed: &mut Ed, from: [f64; 2], to_x: f64, input: PointerInput) {
    let p = |x: f64| PointerInput {
        client_x: x,
        client_y: from[1],
        ..input
    };
    ed.pointer_move(p(from[0]));
    ed.pointer_down(p(from[0]));
    for i in 1..=4 {
        ed.pointer_move(p(from[0] + (to_x - from[0]) * f64::from(i) / 4.0));
    }
    ed.pointer_up(p(to_x));
}

fn the_text(ed: &Ed) -> Value {
    let texts: Vec<Value> = live(ed)
        .iter()
        .map(|e| json!(e.to_map()))
        .filter(|e| e["type"] == "text")
        .collect();
    assert_eq!(texts.len(), 1, "{texts:?}");
    texts[0].clone()
}

#[test]
fn a_text_tool_drag_sets_the_text_width() {
    // textWysiwyg.test.tsx:285-330 (dragNewTextElement,
    // dragElements.ts:227-292): the drag from the press sizes the text, which
    // no longer grows by itself
    for (from_x, to_x, y) in [
        (220.0, 380.0, 230.0),
        (380.0, 220.0, 230.0),
        (350.0, 510.0, 300.0),
        (350.0, 190.0, 300.0),
        (365.0, 525.0, 300.0),
    ] {
        let mut ed = editor_with(vec![el(
            "rectangle",
            "c",
            100.0,
            100.0,
            500.0,
            json!({ "height": 400 }),
        )]);
        letter(&mut ed, "t");
        drag_steps(&mut ed, [from_x, y], to_x, at(0.0, 0.0));
        type_and_submit(
            &mut ed,
            "A label long enough to wrap within the dragged width",
        );
        let text = the_text(&ed);
        assert_eq!(text["autoResize"], json!(false), "{from_x}->{to_x}");
        assert_eq!(text["width"], json!(160.0), "{from_x}->{to_x}");
        assert_eq!(text["x"], json!(f64::min(from_x, to_x)), "{from_x}->{to_x}");
        assert_eq!(text["y"], json!(y), "{from_x}->{to_x}");
        assert_eq!(text["containerId"], Value::Null);
        assert!(text["text"].as_str().unwrap().contains('\n'));
        // upstream's `null` (API.setElements does not restore); loading
        // restores it to `[]`: either way nothing is bound
        let bound = json!(get(&ed, "c").to_map())["boundElements"].clone();
        assert!(
            bound.is_null() || bound == json!([]),
            "{from_x}->{to_x}: {bound}"
        );
    }
}

#[test]
fn a_ctrl_text_tool_drag_from_a_centre_is_free_text() {
    // textWysiwyg.test.tsx:452-484
    let mut ed = editor_with(vec![el(
        "rectangle",
        "c",
        100.0,
        100.0,
        500.0,
        json!({ "height": 400, "backgroundColor": "#a5d8ff" }),
    )]);
    letter(&mut ed, "t");
    drag_steps(&mut ed, [350.0, 300.0], 430.0, ctrl(0.0, 0.0));
    type_and_submit(&mut ed, "Hello");
    let text = the_text(&ed);
    assert_eq!(text["autoResize"], json!(false));
    assert_eq!(text["width"], json!(80.0));
    assert_eq!(text["x"], json!(350.0));
    assert_eq!(text["containerId"], Value::Null);
}

/// arrowEndpointTextBinding.test.tsx's `createArrow` (`:14-32`).
fn arrow_to(id: &str, from: [f64; 2], to: [f64; 2]) -> Value {
    el(
        "arrow",
        id,
        from[0],
        from[1],
        (to[0] - from[0]).abs(),
        json!({
            "height": (to[1] - from[1]).abs(),
            "points": [[0, 0], [to[0] - from[0], to[1] - from[1]]],
            "endArrowhead": "arrow", "startArrowhead": null,
            "startBinding": null, "endBinding": null, "elbowed": false,
        }),
    )
}

/// arrowEndpointTextBinding.test.tsx's `bindTextAt` (`:73-80`).
fn bind_text_at(ed: &mut Ed, at_: [f64; 2], value: &str) {
    letter(ed, "t");
    ed.pointer_move(at(at_[0], at_[1]));
    click(ed, at_);
    type_and_submit(ed, value);
}

#[test]
fn a_text_tool_click_on_an_arrow_end_labels_it() {
    // arrowEndpointTextBinding.test.tsx:334-385
    for (from, to, fixed, align, valign) in [
        (
            [100.0, 300.0],
            [100.0, 100.0],
            [0.5001, 1.0],
            "center",
            "bottom",
        ),
        (
            [100.0, 100.0],
            [300.0, 100.0],
            [0.0, 0.5001],
            "left",
            "middle",
        ),
        (
            [100.0, 100.0],
            [100.0, 300.0],
            [0.5001, 0.0],
            "center",
            "top",
        ),
        (
            [300.0, 100.0],
            [100.0, 100.0],
            [1.0, 0.5001],
            "right",
            "middle",
        ),
    ] {
        let mut ed = editor_with(vec![arrow_to("arrow", from, to)]);
        bind_text_at(&mut ed, to, "label");
        let text = the_text(&ed);
        let arrow = json!(get(&ed, "arrow").to_map());
        assert_eq!(
            arrow["endBinding"],
            json!({ "elementId": text["id"], "fixedPoint": fixed, "mode": "orbit" }),
            "{from:?}->{to:?}"
        );
        assert_eq!(text["textAlign"], align);
        assert_eq!(text["verticalAlign"], valign);
        assert_eq!(text["containerId"], Value::Null);
        assert_eq!(
            text["boundElements"],
            json!([{ "id": "arrow", "type": "arrow" }])
        );
    }
}

#[test]
fn a_text_tool_click_on_an_arrow_start_labels_the_start() {
    // arrowEndpointTextBinding.test.tsx:387-401
    let mut ed = editor_with(vec![arrow_to("arrow", [100.0, 100.0], [300.0, 100.0])]);
    bind_text_at(&mut ed, [100.0, 100.0], "label");
    let text = the_text(&ed);
    let arrow = json!(get(&ed, "arrow").to_map());
    assert_eq!(arrow["startBinding"]["elementId"], text["id"]);
    assert_eq!(arrow["endBinding"], Value::Null);
    assert_eq!(arrow["startBinding"]["fixedPoint"], json!([1.0, 0.5001]));
    assert_eq!(text["textAlign"], "right");
}

#[test]
fn an_endpoint_label_is_sized_by_the_drag() {
    // arrowEndpointTextBinding.test.tsx:479-548
    let close = |a: &Value, b: f64| (a.as_f64().unwrap() - b).abs() < 0.5;
    // left-bound: the left edge stays
    let mut ed = editor_with(vec![arrow_to("arrow", [100.0, 100.0], [300.0, 100.0])]);
    letter(&mut ed, "t");
    drag_steps(&mut ed, [300.0, 100.0], 520.0, at(0.0, 0.0));
    type_and_submit(&mut ed, "a label long enough to wrap");
    let t = the_text(&ed);
    assert_eq!(t["autoResize"], json!(false));
    assert_eq!(t["textAlign"], "left");
    assert!(close(&t["x"], 306.0) && close(&t["width"], 214.0), "{t}");
    // right-bound: the right edge stays
    let mut ed = editor_with(vec![arrow_to("arrow", [500.0, 100.0], [300.0, 100.0])]);
    letter(&mut ed, "t");
    drag_steps(&mut ed, [300.0, 100.0], 80.0, at(0.0, 0.0));
    type_and_submit(&mut ed, "a label long enough to wrap");
    let t = the_text(&ed);
    assert_eq!(t["textAlign"], "right");
    let right = t["x"].as_f64().unwrap() + t["width"].as_f64().unwrap();
    assert!((right - 294.0).abs() < 0.5, "{t}");
    // back over the arrow: no drag at all
    let mut ed = editor_with(vec![arrow_to("arrow", [100.0, 100.0], [300.0, 100.0])]);
    letter(&mut ed, "t");
    drag_steps(&mut ed, [300.0, 100.0], 150.0, at(0.0, 0.0));
    type_and_submit(&mut ed, "label");
    let t = the_text(&ed);
    assert_eq!(t["autoResize"], json!(true));
    assert!(close(&t["x"], 306.0), "{t}");
}

#[test]
fn one_undo_removes_an_endpoint_label_and_its_binding() {
    // arrowEndpointTextBinding.test.tsx:704-731
    let mut ed = editor_with(vec![arrow_to("arrow", [100.0, 300.0], [100.0, 100.0])]);
    bind_text_at(&mut ed, [100.0, 100.0], "bound");
    let text_id = json!(get(&ed, "arrow").to_map())["endBinding"]["elementId"].clone();
    assert!(text_id.is_string());
    key(&mut ed, Keystroke::new("z", "KeyZ").ctrl());
    assert!(live(&ed).iter().all(|e| json!(e.base.id) != text_id));
    assert_eq!(json!(get(&ed, "arrow").to_map())["endBinding"], Value::Null);
}
