//! A host that themes the editor itself (ex-807).
//!
//! - A `theme` the host controls: upstream leaves
//!   `UIOptions.canvasActions.toggleTheme` `null` when the `theme` prop is
//!   passed without `onThemeChange`, and only sets it `true` otherwise
//!   (`packages/excalidraw/index.tsx:142-147`), so `isActionEnabled
//!   (actionToggleTheme)` is false: the main menu drops its theme item
//!   (`components/main-menu/DefaultItems.tsx:256`), the help dialog its
//!   shortcut row (`components/HelpDialog.tsx:310`), and Alt+Shift+D
//!   matches no action (`actions/manager.tsx:97-104`); the command
//!   palette's theme command (`components/CommandPalette/CommandPalette.tsx:
//!   422-423`) is unavailable through the action's predicate
//!   (`actions/actionCanvas.tsx:462-464`, `isCommandAvailable` at
//!   `CommandPalette.tsx:672-689`).
//! - The host's canvas colour, not upstream: painted unfiltered behind a
//!   scene on the default background and used as the outline arrowheads'
//!   fill; a scene background of its own, and every export, as before.

use excali_core::color::apply_dark_mode_filter;
use excali_editor::actions::{
    commands_from_actions, default_main_menu, palette_command_available, ActionName, KeyLabels,
    MainMenuEntry, MainMenuItem, PaletteCommandSource,
};
use excali_editor::keyboard::Keystroke;
use excali_scene::display::FontFaceSource;
use excali_svg::FontContent;
use excali_text::text_measurements::CharCountTextMetrics;

use excali_wasm::editor::{Editor, ExportOptions};
use excali_wasm::env::EditorEnv;
use serde_json::{json, Value};

const HOST: &str = "#1e1e2e";

fn arrow(end: &str) -> Value {
    json!({
        "id": "arrow", "type": "arrow", "x": 100, "y": 100, "width": 200, "height": 0,
        "angle": 0, "strokeColor": "#1e1e1e", "backgroundColor": "transparent",
        "fillStyle": "solid", "strokeWidth": 2, "strokeStyle": "solid", "roughness": 0,
        "opacity": 100, "groupIds": [], "frameId": null, "index": null, "roundness": null,
        "seed": 1, "version": 1, "versionNonce": 0, "isDeleted": false,
        "boundElements": null, "updated": 1, "link": null, "locked": false,
        "points": [[0, 0], [200, 0]], "lastCommittedPoint": null,
        "startBinding": null, "endBinding": null,
        "startArrowhead": null, "endArrowhead": end, "elbowed": false
    })
}

fn editor(background: &str, dark: bool) -> Editor<CharCountTextMetrics> {
    let env = EditorEnv::new(CharCountTextMetrics, 7, || 1.0);
    let mut ed = Editor::new(env, "https://term.hut", false);
    ed.set_viewport(1000.0, 700.0, 0.0, 0.0);
    let scene = json!({
        "type": "excalidraw", "version": 2, "source": "https://excalidraw.com",
        "elements": [arrow("triangle_outline")],
        "appState": { "viewBackgroundColor": background }, "files": {}
    });
    ed.load(&scene.to_string()).expect("the scene loads");
    ed.set_theme(dark);
    ed.take_events();
    ed
}

fn theme(ed: &Editor<CharCountTextMetrics>) -> Option<&str> {
    ed.app_state().get("theme").and_then(Value::as_str)
}

fn toggle_enabled(ed: &Editor<CharCountTextMetrics>) -> bool {
    let ctx = ed.action_context();
    ed.action_manager()
        .is_action_enabled(ActionName::ToggleTheme, &ctx)
}

fn menu_has_theme_item(ed: &Editor<CharCountTextMetrics>) -> bool {
    let ctx = ed.action_context();
    default_main_menu(ed.action_manager(), &ctx, &KeyLabels::EN)
        .iter()
        .flat_map(|entry| match entry {
            MainMenuEntry::Item(row) => vec![row.item],
            MainMenuEntry::Group { items, .. } => items.iter().map(|r| r.item).collect(),
            MainMenuEntry::Separator => vec![],
        })
        .any(|item| item == MainMenuItem::ToggleTheme)
}

/// The command palette's theme command is available.
fn palette_has_theme_command(ed: &Editor<CharCountTextMetrics>) -> bool {
    let ctx = ed.action_context();
    commands_from_actions()
        .iter()
        .filter(|c| c.source == PaletteCommandSource::Action(ActionName::ToggleTheme))
        .any(|c| palette_command_available(c, &ctx))
}

fn alt_shift_d(ed: &mut Editor<CharCountTextMetrics>) {
    ed.key_down(&Keystroke::new("D", "KeyD").alt().shift());
}

#[test]
fn an_uncontrolled_theme_keeps_its_toggle() {
    let mut ed = editor("#ffffff", false);
    assert!(toggle_enabled(&ed));
    assert!(menu_has_theme_item(&ed));
    assert!(palette_has_theme_command(&ed));
    alt_shift_d(&mut ed);
    assert_eq!(theme(&ed), Some("dark"));
}

#[test]
fn a_host_controlled_theme_has_no_toggle() {
    let mut ed = editor("#ffffff", false);
    ed.set_theme_controlled(true);
    assert!(!toggle_enabled(&ed), "isActionEnabled(actionToggleTheme)");
    assert!(!menu_has_theme_item(&ed), "the main menu's ToggleTheme");
    assert!(!palette_has_theme_command(&ed), "actionCanvas.tsx:462-464");
    alt_shift_d(&mut ed);
    assert_eq!(theme(&ed), Some("light"), "Alt+Shift+D is not handled");
    ed.set_theme(true);
    alt_shift_d(&mut ed);
    assert_eq!(theme(&ed), Some("dark"));

    // the attribute removed: the toggle is back
    ed.set_theme_controlled(false);
    assert!(toggle_enabled(&ed));
    assert!(menu_has_theme_item(&ed));
    assert!(palette_has_theme_command(&ed));
    alt_shift_d(&mut ed);
    assert_eq!(theme(&ed), Some("light"));
}

/// The static canvas as vectors and as the editor's cached frame, with the
/// bitmaps it makes, printed.
fn drawn(ed: &mut Editor<CharCountTextMetrics>) -> String {
    let vectors = ed.static_scene(1000.0, 700.0, 1.0);
    let frame = ed.static_frame(1000.0, 700.0, 1.0);
    format!("{vectors:?}\n{:?}\n{:?}", frame.list, frame.new_bitmaps)
}

fn has(printed: &str, color: &str) -> bool {
    printed.contains(&format!("{color:?}"))
}

/// How often `color` fills a rectangle (the background) in the vectors
/// and in the frame.
fn background_fills(printed: &str, color: &str) -> usize {
    printed
        .matches(&format!("FillRect {{ rect: Rect {{ x: 0.0, y: 0.0, width: 1000.0, height: 700.0 }}, color: Color({color:?}) }}"))
        .count()
}

#[test]
fn the_host_colour_is_painted_unfiltered_behind_the_default_background() {
    for dark in [false, true] {
        // upstream's default, in any case
        for scene in ["#ffffff", "#FFFFFF"] {
            let mut ed = editor(scene, dark);
            ed.set_host_canvas_background(Some(HOST.into()));
            assert_eq!(ed.static_background().as_deref(), Some(HOST));
            let printed = drawn(&mut ed);
            assert_eq!(
                background_fills(&printed, HOST),
                2,
                "dark {dark}: {printed}"
            );
            let filtered = apply_dark_mode_filter(HOST, true);
            assert!(
                !has(&printed, &filtered),
                "the host colour went through the filter"
            );
            let white = apply_dark_mode_filter("#ffffff", dark);
            assert_eq!(background_fills(&printed, &white), 0);
        }
    }
}

#[test]
fn outline_arrowheads_are_filled_with_the_host_colour() {
    for dark in [false, true] {
        let mut ed = editor("#ffffff", dark);
        let before = drawn(&mut ed);
        let white = apply_dark_mode_filter("#ffffff", dark);
        assert!(
            has(&before, &white),
            "the head's fill without a host colour"
        );
        ed.set_host_canvas_background(Some(HOST.into()));
        let printed = drawn(&mut ed);
        // the background twice, the head in the vectors and in its bitmap,
        // made again for the new colour
        assert!(
            printed.matches(&format!("{HOST:?}")).count() >= 4,
            "dark {dark}: {printed}"
        );
        assert!(
            !has(&printed, &white),
            "dark {dark}: the head kept the old fill"
        );
    }
}

#[test]
fn a_scene_background_of_its_own_is_kept() {
    for dark in [false, true] {
        let mut ed = editor("#ffc9c9", dark);
        ed.set_host_canvas_background(Some(HOST.into()));
        assert_eq!(ed.static_background().as_deref(), Some("#ffc9c9"));
        let printed = drawn(&mut ed);
        assert!(!has(&printed, HOST), "dark {dark}");
        let own = apply_dark_mode_filter("#ffc9c9", dark);
        assert_eq!(background_fills(&printed, &own), 2, "dark {dark}");
    }
}

#[test]
fn no_host_colour_draws_as_before() {
    for dark in [false, true] {
        let mut plain = editor("#ffffff", dark);
        let mut unset = editor("#ffffff", dark);
        unset.set_host_canvas_background(Some(HOST.into()));
        unset.set_host_canvas_background(Some("  ".into()));
        assert_eq!(unset.static_background().as_deref(), Some("#ffffff"));
        let a = plain.static_scene(1000.0, 700.0, 1.0);
        let b = unset.static_scene(1000.0, 700.0, 1.0);
        assert_eq!(a, b);
        unset.set_host_canvas_background(None);
        assert_eq!(drawn(&mut plain), drawn(&mut editor("#ffffff", dark)));
    }
}

struct Urls;

impl FontContent for Urls {
    fn content(&self, face: &FontFaceSource) -> String {
        format!("fonts/{}", face.file)
    }
}

#[test]
fn exports_keep_the_scene_background() {
    for dark in [false, true] {
        let plain = editor("#ffffff", dark);
        let mut hosted = editor("#ffffff", dark);
        hosted.set_host_canvas_background(Some(HOST.into()));
        for opts in [
            ExportOptions::default(),
            ExportOptions {
                dark: true,
                ..ExportOptions::default()
            },
        ] {
            let svg = hosted.export_svg(&opts, &Urls).unwrap();
            assert_eq!(svg, plain.export_svg(&opts, &Urls).unwrap());
            assert!(!svg.contains(HOST));
            let png = hosted.export_png(&opts).unwrap();
            assert_eq!(png, plain.export_png(&opts).unwrap());
            assert!(!format!("{png:?}").contains(HOST));
        }
        let clip = hosted.copy_as_png().unwrap();
        assert_eq!(clip, plain.copy_as_png().unwrap());
        assert!(!format!("{clip:?}").contains(HOST));
    }
}
