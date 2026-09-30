//! The render config the editor's static canvas draws with. Upstream's
//! `App` passes the static canvas `renderGrid: isGridModeEnabled(this)` and
//! `theme: this.state.theme` (`packages/excalidraw/components/App.tsx:
//! 2675-2690`), so in the dark theme the elements and the grid go through
//! `applyDarkModeFilter` (`element/src/renderElement.ts`,
//! `renderer/staticScene.ts:57-66`) as the background does, and the grid is
//! drawn only in grid mode (`appState.gridModeEnabled`, false by default:
//! `appState.ts:76`).

use excali_core::color::apply_dark_mode_filter;
use excali_editor::actions::ActionName;
use excali_scene::static_scene::{GRID_LINE_COLOR_BOLD, GRID_LINE_COLOR_REGULAR};
use excali_text::text_measurements::CharCountTextMetrics;
use excali_wasm::editor::Editor;
use excali_wasm::env::EditorEnv;

const SCENE: &str = include_str!("fixtures/bound.excalidraw");

fn editor(dark: bool) -> Editor<CharCountTextMetrics> {
    let env = EditorEnv::new(CharCountTextMetrics, 7, || 1.0);
    let mut ed = Editor::new(env, "https://term.hut", false);
    ed.set_viewport(1000.0, 700.0, 0.0, 0.0);
    ed.load(SCENE).expect("the fixture loads");
    ed.set_theme(dark);
    ed.take_events();
    ed
}

/// The static canvas as vectors and as the editor's cached frame (with the
/// bitmaps it makes), printed.
fn drawn(ed: &mut Editor<CharCountTextMetrics>) -> String {
    let vectors = ed.static_scene(1000.0, 700.0, 1.0);
    let frame = ed.static_frame(1000.0, 700.0, 1.0);
    format!("{vectors:?}\n{:?}\n{:?}", frame.list, frame.new_bitmaps)
}

fn has(printed: &str, color: &str) -> bool {
    printed.contains(&format!("{color:?}"))
}

#[test]
fn the_dark_theme_filters_the_elements_colours() {
    let stroke = "#1e1e1e";
    let dark = apply_dark_mode_filter(stroke, true);
    assert_ne!(dark, stroke);

    let light = drawn(&mut editor(false));
    assert!(has(&light, stroke));
    assert!(!has(&light, &dark));

    let printed = drawn(&mut editor(true));
    assert!(has(&printed, &dark), "no {dark} in the dark frame");
    assert!(!has(&printed, stroke), "{stroke} drawn unfiltered in the dark theme");
}

#[test]
fn the_grid_is_drawn_in_grid_mode_only_through_the_theme() {
    for dark in [false, true] {
        let colors = [
            apply_dark_mode_filter(GRID_LINE_COLOR_BOLD, dark),
            apply_dark_mode_filter(GRID_LINE_COLOR_REGULAR, dark),
        ];
        let mut ed = editor(dark);
        let off = drawn(&mut ed);
        for c in &colors {
            assert!(!has(&off, c), "grid colour {c} drawn with grid mode off");
        }
        ed.perform_action(ActionName::GridMode);
        assert_eq!(ed.app_state().grid_mode_enabled(), Some(true));
        let on = drawn(&mut ed);
        for c in &colors {
            assert!(has(&on, c), "no grid colour {c} in grid mode (dark {dark})");
        }
    }
}
