//! The shortcut actions in the editor (ex-541): Esc, Shift+H and Shift+V,
//! Ctrl/Cmd+Shift+L, Ctrl/Cmd+Alt+C and V, Alt+R and Alt+Shift+D run
//! upstream's perform (`excali_editor::edit_actions::perform_shortcut_action`)
//! from `key_down` and `perform_action`, writing the result back with its
//! `captureUpdate`.

use excali_core::element::Element;
use excali_editor::actions::ActionName;
use excali_editor::keyboard::Keystroke;
use excali_text::text_measurements::CharCountTextMetrics;
use excali_wasm::editor::Editor;
use excali_wasm::env::EditorEnv;
use serde_json::{json, Map, Value};

fn rectangle(id: &str, x: f64, y: f64) -> Value {
    json!({
        "id": id, "type": "rectangle", "x": x, "y": y, "width": 100, "height": 100,
        "angle": 0, "strokeColor": "#1e1e1e", "backgroundColor": "transparent",
        "fillStyle": "solid", "strokeWidth": 2, "strokeStyle": "solid", "roughness": 1,
        "opacity": 100, "groupIds": [], "frameId": null, "index": null, "roundness": null,
        "seed": 1, "version": 1, "versionNonce": 0, "isDeleted": false,
        "boundElements": null, "updated": 1, "link": null, "locked": false
    })
}

fn editor(elements: Vec<Value>) -> Editor<CharCountTextMetrics> {
    let env = EditorEnv::new(CharCountTextMetrics, 7, || 1.0);
    let mut ed = Editor::new(env, "https://term.hut", false);
    ed.set_viewport(1000.0, 700.0, 0.0, 0.0);
    let scene = json!({
        "type": "excalidraw", "version": 2, "source": "https://excalidraw.com",
        "elements": elements, "appState": {}, "files": {}
    });
    ed.load(&scene.to_string()).expect("the scene loads");
    ed.take_events();
    ed
}

fn select(ed: &mut Editor<CharCountTextMetrics>, ids: &[&str]) {
    let selected: Map<String, Value> = ids
        .iter()
        .map(|id| ((*id).to_owned(), json!(true)))
        .collect();
    let mut patch = Map::new();
    patch.insert("selectedElementIds".into(), Value::Object(selected));
    ed.set_app_state(patch);
}

fn get<'a>(ed: &'a Editor<CharCountTextMetrics>, id: &str) -> &'a Element {
    ed.elements()
        .iter()
        .find(|e| e.base.id == id)
        .unwrap_or_else(|| panic!("no element {id}"))
}

fn state<'a>(ed: &'a Editor<CharCountTextMetrics>, key: &str) -> Option<&'a Value> {
    ed.app_state().get(key)
}

#[test]
fn escape_deselects_and_leaves_the_tool() {
    let mut ed = editor(vec![rectangle("a", 100.0, 100.0)]);
    select(&mut ed, &["a"]);
    ed.key_down(&Keystroke::new("Escape", "Escape"));
    assert_eq!(state(&ed, "selectedElementIds"), Some(&json!({})));
    // a drawing tool goes back to the selection tool
    ed.key_down(&Keystroke::new("r", "KeyR"));
    assert_eq!(ed.tools().active_tool.tool.type_name(), "rectangle");
    ed.key_down(&Keystroke::new("Escape", "Escape"));
    assert_eq!(ed.tools().active_tool.tool.type_name(), "selection");
}

#[test]
fn shift_h_and_shift_v_flip_the_selection() {
    let mut ed = editor(vec![rectangle("a", 0.0, 0.0), rectangle("b", 200.0, 200.0)]);
    select(&mut ed, &["a", "b"]);
    ed.key_down(&Keystroke::new("H", "KeyH").shift());
    assert_eq!((get(&ed, "a").base.x, get(&ed, "b").base.x), (200.0, 0.0));
    ed.key_down(&Keystroke::new("V", "KeyV").shift());
    assert_eq!((get(&ed, "a").base.y, get(&ed, "b").base.y), (200.0, 0.0));
    // each flip is one history entry
    ed.undo();
    assert_eq!((get(&ed, "a").base.y, get(&ed, "b").base.y), (0.0, 200.0));
    ed.undo();
    assert_eq!((get(&ed, "a").base.x, get(&ed, "b").base.x), (0.0, 200.0));
    assert!(!ed.can_undo());
}

#[test]
fn ctrl_shift_l_locks_and_unlocks() {
    let mut ed = editor(vec![rectangle("a", 0.0, 0.0), rectangle("b", 200.0, 0.0)]);
    select(&mut ed, &["a", "b"]);
    ed.key_down(&Keystroke::new("L", "KeyL").ctrl().shift());
    assert!(get(&ed, "a").base.locked && get(&ed, "b").base.locked);
    let group = get(&ed, "a")
        .base
        .group_ids
        .last()
        .cloned()
        .expect("a lock group");
    assert_eq!(get(&ed, "b").base.group_ids, [group.clone()]);
    assert_eq!(state(&ed, "selectedElementIds"), Some(&json!({})));
    assert_eq!(state(&ed, "activeLockedId"), Some(&json!(group)));
    assert_eq!(
        state(&ed, "lockedMultiSelections"),
        Some(&json!({ group.clone(): true }))
    );
    // unlocking from the menu drops the lock group
    select(&mut ed, &["a", "b"]);
    ed.perform_action(ActionName::ToggleElementLock);
    assert!(!get(&ed, "a").base.locked && get(&ed, "a").base.group_ids.is_empty());
}

#[test]
fn ctrl_alt_c_and_v_copy_and_paste_styles() {
    let mut styled = rectangle("a", 0.0, 0.0);
    styled["strokeColor"] = json!("#e03131");
    styled["strokeStyle"] = json!("dashed");
    styled["opacity"] = json!(60);
    let mut ed = editor(vec![styled, rectangle("b", 200.0, 0.0)]);
    select(&mut ed, &["a"]);
    ed.key_down(&Keystroke::new("c", "KeyC").ctrl().alt());
    assert_eq!(
        state(&ed, "toast"),
        Some(&json!({ "message": "Copied styles." }))
    );
    assert!(!ed.can_undo(), "copying is not captured");
    select(&mut ed, &["b"]);
    ed.key_down(&Keystroke::new("v", "KeyV").ctrl().alt());
    let b = get(&ed, "b");
    assert_eq!(b.base.stroke_color, "#e03131");
    assert_eq!(b.base.opacity, 60.0);
    assert_eq!(b.to_map()["strokeStyle"], json!("dashed"));
    ed.undo();
    assert_eq!(get(&ed, "b").base.stroke_color, "#1e1e1e");
}

#[test]
fn alt_r_toggles_view_mode_and_alt_shift_d_the_theme() {
    let mut ed = editor(vec![rectangle("a", 0.0, 0.0)]);
    ed.key_down(&Keystroke::new("r", "KeyR").alt());
    assert_eq!(state(&ed, "viewModeEnabled"), Some(&json!(true)));
    ed.key_down(&Keystroke::new("r", "KeyR").alt());
    assert_eq!(state(&ed, "viewModeEnabled"), Some(&json!(false)));
    ed.key_down(&Keystroke::new("D", "KeyD").alt().shift());
    assert_eq!(state(&ed, "theme"), Some(&json!("dark")));
    ed.key_down(&Keystroke::new("D", "KeyD").alt().shift());
    assert_eq!(state(&ed, "theme"), Some(&json!("light")));
    assert!(!ed.can_undo(), "neither is captured");
}
