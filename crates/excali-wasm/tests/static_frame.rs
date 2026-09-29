//! The editor's static canvas from cached bitmaps (ex-710): upstream draws
//! every element in the editor from a bitmap of its own
//! (`renderElement.ts:682-728`, `:963-1009`), made again only when the
//! element, the zoom or the theme changes, and dropped with the element
//! object (`elementWithCanvasCache` is a WeakMap). `Editor::static_frame`
//! gives the frame's display list with the bitmaps the host must make before
//! painting it and the ones it may free; a pan makes none, which is what
//! keeps a pan at 1,000 elements inside its budget
//! (site/content/plan/phases.md#budgets). When fonts load, upstream's
//! `Fonts.onLoaded` deletes every text element's bitmap and its container's
//! (`packages/excalidraw/fonts/Fonts.ts:106-150`, `ShapeCache.delete`).

use excali_editor::keyboard::Keystroke;
use excali_scene::display::{bitmap_id, DisplayItem, DisplayList};
use excali_text::text_measurements::CharCountTextMetrics;
use excali_wasm::editor::{Editor, PointerInput, WheelInput};
use excali_wasm::env::EditorEnv;

const SCENE: &str = include_str!("fixtures/bound.excalidraw");

fn editor() -> Editor<CharCountTextMetrics> {
    let env = EditorEnv::new(CharCountTextMetrics, 7, || 1.0);
    let mut ed = Editor::new(env, "https://term.hut", false);
    ed.set_viewport(1000.0, 700.0, 0.0, 0.0);
    ed.load(SCENE).expect("the fixture loads");
    ed.take_events();
    ed
}

fn made(ed: &mut Editor<CharCountTextMetrics>) -> Vec<String> {
    let mut ids: Vec<String> = ed
        .static_frame(1000.0, 700.0, 1.0)
        .new_bitmaps
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    ids.sort();
    ids
}

fn blit_ids(items: &[DisplayItem], out: &mut Vec<String>) {
    for item in items {
        match item {
            DisplayItem::Blit(b) => out.push(b.id.clone()),
            DisplayItem::Group(g) => blit_ids(&g.items, out),
            _ => {}
        }
    }
}

fn blits(list: &DisplayList) -> Vec<String> {
    let mut out = Vec::new();
    blit_ids(&list.items, &mut out);
    out
}

fn ids(names: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = names.iter().map(|n| bitmap_id(n)).collect();
    v.sort();
    v
}

const ALL: [&str; 6] = ["a", "b", "box", "label", "link", "linked"];

fn pan(ed: &mut Editor<CharCountTextMetrics>, dx: f64, dy: f64) {
    ed.wheel(&WheelInput {
        delta_x: dx,
        delta_y: dy,
        ctrl_key: false,
        meta_key: false,
        shift_key: false,
        buttons: 0,
    });
}

#[test]
fn the_first_frame_makes_a_bitmap_per_element_and_blits_them() {
    let mut ed = editor();
    let frame = ed.static_frame(1000.0, 700.0, 1.0);
    let mut drawn = blits(&frame.list);
    drawn.sort();
    assert_eq!(drawn, ids(&ALL));
    let mut new: Vec<String> = frame.new_bitmaps.iter().map(|(id, _)| id.clone()).collect();
    new.sort();
    assert_eq!(new, ids(&ALL));
    for (id, canvas) in &frame.new_bitmaps {
        assert!(canvas.width > 0.0 && canvas.height > 0.0, "{id}");
        assert!(!canvas.content.is_empty(), "{id}");
    }
    assert!(frame.dropped_bitmaps.is_empty());
}

#[test]
fn a_pan_makes_no_bitmap() {
    let mut ed = editor();
    made(&mut ed);
    for _ in 0..5 {
        pan(&mut ed, 7.0, -3.0);
        let frame = ed.static_frame(1000.0, 700.0, 1.0);
        assert!(frame.new_bitmaps.is_empty());
        assert_eq!(blits(&frame.list).len(), ALL.len());
    }
}

#[test]
fn a_drag_remakes_what_it_changed() {
    let mut ed = editor();
    made(&mut ed);
    // `a` (60, 300, 100 x 100) is where the arrow `link` starts: the drag
    // moves both
    ed.pointer_down(PointerInput::at(110.0, 350.0));
    ed.pointer_move(PointerInput::at(130.0, 360.0));
    ed.pointer_up(PointerInput::at(130.0, 360.0));
    assert_eq!(made(&mut ed), ids(&["a", "link"]));
    assert!(made(&mut ed).is_empty());
}

#[test]
fn a_zoom_or_theme_change_remakes_every_bitmap() {
    let mut ed = editor();
    made(&mut ed);
    ed.key_down(&Keystroke::new("=", "Equal").ctrl());
    assert_eq!(made(&mut ed), ids(&ALL));
    ed.set_theme(true);
    assert_eq!(made(&mut ed), ids(&ALL));
}

#[test]
fn loading_another_scene_drops_the_old_bitmaps() {
    let mut ed = editor();
    made(&mut ed);
    let other = SCENE.replace("\"id\": \"a\"", "\"id\": \"a2\"");
    assert_ne!(other, SCENE);
    ed.load(&other).unwrap();
    let frame = ed.static_frame(1000.0, 700.0, 1.0);
    assert_eq!(frame.dropped_bitmaps, ids(&["a"]));
    // the loaded elements are new objects: every bitmap is made again
    let mut new: Vec<String> = frame.new_bitmaps.into_iter().map(|(id, _)| id).collect();
    new.sort();
    assert_eq!(new, ids(&["a2", "b", "box", "label", "link", "linked"]));
}

#[test]
fn loaded_fonts_remake_text_and_its_container() {
    let mut ed = editor();
    made(&mut ed);
    ed.fonts_loaded();
    assert_eq!(made(&mut ed), ids(&["box", "label"]));
}
