//! Keyboard handling, pinned to upstream at the pinned commit:
//!
//! - `packages/excalidraw/components/App.tsx:5585-6073` (`onKeyDown`),
//!   `:6076-6207` (`onKeyUp`), `:3296-3318` (`maybeHandlePageScrollKeyDown`),
//!   `:4542-4570` (`onCut`, `onCopy`), `:4828-4854` (the paste gate);
//! - `components/App.flowchart.ts:53-197` (the flowchart keys);
//! - `components/App.pan.ts:100-120` (`AppPan.start`);
//! - `components/CommandPalette/CommandPalette.tsx:141-186`;
//! - `components/ConvertElementTypePopup.tsx:641-672`;
//! - `packages/common/src/keys.ts:138-153`.
//!
//! Ported cases: `tests/regressionTests.test.tsx` "arrow keys",
//! `tests/scroll.test.tsx` "moving by page up/down/left/right",
//! `tests/shortcuts.test.tsx` (Ctrl+Delete opens the clear canvas
//! confirmation) and the `isBindingEnabled` toggle of
//! `tests/arrowBinding.test.tsx`.

mod support;

use excali_core::app_state::AppState;
use excali_core::element::{
    BindMode, BoundElement, BoundElementType, Element, ElementKind, FixedPointBinding, LineFields,
    LinearFields,
};
use excali_editor::actions::{ActionEnv, ActionManager, ActionName, AppProps, KeyDownOutcome};
use excali_editor::keyboard::{
    command_palette_key_down, effective_grid_size, get_conversion_type, is_arrow_key,
    is_command_palette_toggle_shortcut, normalize_caps_lock, nudge_step, on_clipboard_event,
    on_key_down, on_key_up, pan_starts, pointer_button, should_maintain_aspect_ratio,
    should_resize_from_center, should_rotate_with_discrete_angle, ClipboardEventKind,
    ClipboardOutcome, ClipboardTarget, ConversionType, ConvertDirection, CursorChange,
    EyeDropperKind, KeyEffect, KeyOutcome, KeyTarget, KeyboardEditor, KeyboardState, Keystroke,
    LinkDirection, Modifiers, PanStart,
};
use excali_editor::scene::Scene;
use excali_editor::tools::{ToolKeyOutcome, ToolState, ToolType};
use serde_json::{json, Value};
use support::{arrow, frame, image, rect, text, TestEnv};

struct Editor {
    scene: Scene,
    app_state: AppState,
    tools: ToolState,
    keyboard: KeyboardState,
    actions: ActionManager,
    props: AppProps,
    env: ActionEnv,
    binding: TestEnv,
}

impl Editor {
    fn new(elements: Vec<Element>) -> Editor {
        let mut app_state = AppState::default();
        app_state.insert("width", json!(1000));
        app_state.insert("height", json!(500));
        Editor {
            scene: Scene::new(elements),
            app_state,
            tools: ToolState::default(),
            keyboard: KeyboardState::default(),
            actions: ActionManager::new(),
            props: AppProps::default(),
            env: ActionEnv::default(),
            binding: TestEnv::with_layout(),
        }
    }

    fn select(&mut self, ids: &[&str]) {
        let map: serde_json::Map<String, Value> =
            ids.iter().map(|id| (id.to_string(), json!(true))).collect();
        self.app_state
            .insert("selectedElementIds", Value::Object(map));
    }

    fn down(&mut self, event: Keystroke) -> KeyOutcome {
        let mut ed = KeyboardEditor {
            scene: &mut self.scene,
            app_state: &mut self.app_state,
            tools: &mut self.tools,
            keyboard: &mut self.keyboard,
            actions: &self.actions,
            props: &self.props,
            env: &self.env,
        };
        on_key_down(&mut ed, &mut self.binding, &event)
    }

    fn up(&mut self, event: Keystroke) -> KeyOutcome {
        let mut ed = KeyboardEditor {
            scene: &mut self.scene,
            app_state: &mut self.app_state,
            tools: &mut self.tools,
            keyboard: &mut self.keyboard,
            actions: &self.actions,
            props: &self.props,
            env: &self.env,
        };
        on_key_up(&mut ed, &mut self.binding, &event)
    }

    fn press(&mut self, event: Keystroke) -> KeyOutcome {
        let out = self.down(event.clone());
        self.up(event);
        out
    }

    fn el(&self, id: &str) -> &Element {
        self.scene.get(id).expect("element")
    }

    fn xy(&self, id: &str) -> (f64, f64) {
        let e = self.el(id);
        (e.base.x, e.base.y)
    }

    fn tool(&self) -> String {
        self.tools.active_tool.tool.type_name().to_owned()
    }

    fn state(&self, key: &str) -> Value {
        self.app_state.get(key).cloned().unwrap_or(Value::Null)
    }
}

fn key(k: &str) -> Keystroke {
    let code = match k {
        " " => "Space".to_owned(),
        "=" => "Equal".to_owned(),
        "-" => "Minus".to_owned(),
        "!" => "Digit1".to_owned(),
        "?" | "/" => "Slash".to_owned(),
        k if k.len() == 1 && k.chars().all(|c| c.is_ascii_alphabetic()) => {
            format!("Key{}", k.to_ascii_uppercase())
        }
        k if k.len() == 1 && k.chars().all(|c| c.is_ascii_digit()) => format!("Digit{k}"),
        k => k.to_owned(),
    };
    Keystroke::new(k, &code)
}

fn has(out: &KeyOutcome, effect: &KeyEffect) -> bool {
    out.effects.contains(effect)
}

fn performed(out: &KeyOutcome) -> Option<ActionName> {
    out.effects.iter().find_map(|e| match e {
        KeyEffect::Action(KeyDownOutcome::Perform(a)) => Some(*a),
        _ => None,
    })
}

fn line(id: &str) -> Element {
    let mut e = arrow(id, vec![[0.0, 0.0], [100.0, 100.0]]);
    e.kind = ElementKind::Line(LineFields {
        linear: LinearFields::new(vec![[0.0, 0.0], [100.0, 100.0]]),
        polygon: false,
    });
    e
}

// ---------------------------------------------------------------------------
// keys.ts

#[test]
fn modifier_helpers_read_shift_and_alt() {
    let shift = Modifiers {
        shift_key: true,
        ..Modifiers::default()
    };
    let alt = Modifiers {
        alt_key: true,
        ..Modifiers::default()
    };
    assert!(should_maintain_aspect_ratio(shift));
    assert!(should_rotate_with_discrete_angle(shift));
    assert!(!should_resize_from_center(shift));
    assert!(should_resize_from_center(alt));
    assert!(!should_maintain_aspect_ratio(alt));
    let ctrl = Modifiers {
        ctrl_key: true,
        ..Modifiers::default()
    };
    let meta = Modifiers {
        meta_key: true,
        ..Modifiers::default()
    };
    assert!(ctrl.ctrl_or_cmd(false) && !ctrl.ctrl_or_cmd(true));
    assert!(meta.ctrl_or_cmd(true) && !meta.ctrl_or_cmd(false));
    for k in ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"] {
        assert!(is_arrow_key(k));
    }
    assert!(!is_arrow_key("Left"));
}

#[test]
fn caps_lock_recases_a_lone_letter_to_match_shift() {
    assert_eq!(normalize_caps_lock(&key("R")).key, "r");
    assert_eq!(normalize_caps_lock(&key("r").shift()).key, "R");
    assert_eq!(normalize_caps_lock(&key("r")).key, "r");
    assert_eq!(normalize_caps_lock(&key("Enter")).key, "Enter");
    // with CapsLock on, R still picks the rectangle
    let mut ed = Editor::new(vec![]);
    ed.down(key("R"));
    assert_eq!(ed.tool(), "rectangle");
}

// ---------------------------------------------------------------------------
// tools

#[test]
fn tool_letters_and_digits_switch_tools() {
    let cases = [
        ("h", "hand"),
        ("v", "selection"),
        ("1", "selection"),
        ("r", "rectangle"),
        ("2", "rectangle"),
        ("d", "diamond"),
        ("3", "diamond"),
        ("o", "ellipse"),
        ("4", "ellipse"),
        ("a", "arrow"),
        ("5", "arrow"),
        ("l", "line"),
        ("6", "line"),
        ("p", "freedraw"),
        ("x", "freedraw"),
        ("7", "freedraw"),
        ("t", "text"),
        ("8", "text"),
        ("n", "stickynote"),
        ("9", "image"),
        ("e", "eraser"),
        ("0", "eraser"),
        ("f", "frame"),
        ("k", "laser"),
        ("b", "bucketfill"),
    ];
    for (k, tool) in cases {
        let mut ed = Editor::new(vec![]);
        let out = ed.down(key(k));
        assert_eq!(ed.tool(), tool, "key {k}");
        assert!(out.stop_propagation, "key {k}");
        assert_eq!(ed.state("activeTool")["type"], json!(tool), "key {k}");
    }
    let mut ed = Editor::new(vec![]);
    ed.down(key("X").shift());
    assert_eq!(ed.tool(), "autoshape");
}

#[test]
fn toggle_tools_switch_back_when_pressed_again() {
    let mut ed = Editor::new(vec![]);
    ed.down(key("r"));
    ed.down(key("h"));
    assert_eq!(ed.tool(), "hand");
    ed.down(key("h"));
    assert_eq!(ed.tool(), "rectangle");
    ed.down(key("e"));
    ed.down(key("e"));
    assert_eq!(ed.tool(), "rectangle");
}

#[test]
fn a_again_cycles_the_arrow_type() {
    let mut ed = Editor::new(vec![]);
    ed.down(key("a"));
    assert_eq!(ed.state("currentItemArrowType"), json!("round"));
    let mut seen = vec![];
    for _ in 0..3 {
        ed.down(key("a"));
        seen.push(ed.state("currentItemArrowType"));
    }
    assert_eq!(seen, vec![json!("elbow"), json!("sharp"), json!("round")]);
    assert_eq!(ed.tool(), "arrow");
}

#[test]
fn b_again_cycles_the_bucket_fill_colour() {
    let mut ed = Editor::new(vec![]);
    ed.down(key("b"));
    let out = ed.down(key("b"));
    assert!(matches!(
        out.effects.as_slice(),
        [KeyEffect::Tool(ToolKeyOutcome::Tool {
            tool: ToolType::Bucketfill,
            action: excali_editor::tools::ToolKeyAction::CycleBucketFillColor,
            ..
        })]
    ));
    assert_eq!(ed.tool(), "bucketfill");
}

#[test]
fn q_toggles_the_tool_lock() {
    let mut ed = Editor::new(vec![]);
    ed.down(key("r"));
    ed.down(key("q"));
    assert!(ed.tools.active_tool.locked);
    assert_eq!(ed.state("activeTool")["locked"], json!(true));
    ed.down(key("q"));
    assert!(!ed.tools.active_tool.locked);
}

#[test]
fn view_mode_keeps_hand_and_laser_and_escape_returns_to_selection() {
    let mut ed = Editor::new(vec![]);
    ed.app_state.insert("viewModeEnabled", json!(true));
    ed.down(key("r"));
    assert_eq!(ed.tool(), "selection");
    ed.down(key("k"));
    assert_eq!(ed.tool(), "laser");
    ed.down(key("Escape"));
    assert_eq!(ed.tool(), "selection");
}

// ---------------------------------------------------------------------------
// App.onKeyDown's own keys

#[test]
fn question_mark_opens_help_and_ctrl_shift_e_the_image_export() {
    let mut ed = Editor::new(vec![]);
    ed.down(key("?").shift());
    assert_eq!(ed.state("openDialog"), json!({ "name": "help" }));
    let out = ed.down(key("E").ctrl().shift());
    assert!(out.prevent_default);
    assert_eq!(ed.state("openDialog"), json!({ "name": "imageExport" }));
    // on a Mac it is Cmd
    let mut mac = Editor::new(vec![]);
    mac.env.is_darwin = true;
    mac.down(key("E").meta().shift());
    assert_eq!(mac.state("openDialog"), json!({ "name": "imageExport" }));
}

/// `scroll.test.tsx` "moving by page up/down/left/right", zoomed to 1.1
/// and to 0.9.
#[test]
fn page_up_and_down_scroll_a_page() {
    for zoom in [1.1, 0.9] {
        let mut ed = Editor::new(vec![]);
        ed.app_state.insert("zoom", json!({ "value": zoom }));
        let step_y = 500.0 / zoom;
        let step_x = 1000.0 / zoom;
        let out = ed.press(key("PageUp"));
        assert!(out.prevent_default);
        assert_eq!(ed.state("scrollY"), json!(step_y));
        assert_eq!(ed.state("scrollX"), json!(0.0));
        ed.press(key("PageDown"));
        ed.press(key("PageDown"));
        assert_eq!(ed.state("scrollY"), json!(step_y - step_y - step_y));
        ed.press(key("PageUp").shift());
        assert_eq!(ed.state("scrollX"), json!(step_x));
        ed.press(key("PageDown").shift());
        ed.press(key("PageDown").shift());
        assert_eq!(ed.state("scrollX"), json!(step_x - step_x - step_x));
    }
}

#[test]
fn alt_opens_the_temporary_eyedropper_with_the_bucket_fill() {
    let mut ed = Editor::new(vec![]);
    ed.down(key("b"));
    let out = ed.down(key("Alt").alt());
    assert!(out.prevent_default);
    assert_eq!(out.effects, vec![KeyEffect::OpenTemporaryEyeDropper]);
    let up = ed.up(key("Alt"));
    assert!(has(&up, &KeyEffect::CloseTemporaryEyeDropper));
}

#[test]
fn action_shortcuts_go_to_the_action_manager() {
    let cases: Vec<(Keystroke, ActionName)> = vec![
        (key("=").ctrl(), ActionName::ZoomIn),
        (key("-").ctrl(), ActionName::ZoomOut),
        (key("0").ctrl(), ActionName::ResetZoom),
        (key("!").shift(), ActionName::ZoomToFit),
        (key("z").alt(), ActionName::ZenMode),
        (key("r").alt(), ActionName::ViewMode),
        (key("z").ctrl(), ActionName::Undo),
        (key("z").ctrl().shift(), ActionName::Redo),
        (key("y").ctrl(), ActionName::Redo),
        (key("a").ctrl(), ActionName::SelectAll),
        (key("o").ctrl(), ActionName::LoadScene),
        (key("s").ctrl(), ActionName::SaveToActiveFile),
        (key("s").ctrl().shift(), ActionName::SaveFileToDisk),
        (key("f").ctrl(), ActionName::SearchMenu),
    ];
    for (event, action) in cases {
        let mut ed = Editor::new(vec![]);
        let out = ed.down(event.clone());
        assert_eq!(performed(&out), Some(action), "{event:?}");
        assert!(out.prevent_default && out.stop_propagation);
    }
}

#[test]
fn ctrl_p_shows_the_command_palette_hint() {
    let mut ed = Editor::new(vec![]);
    let out = ed.down(key("p").ctrl());
    assert_eq!(out.effects, vec![KeyEffect::CommandPaletteHint]);
    assert!(out.prevent_default);
    assert_eq!(ed.tool(), "selection");
}

#[test]
fn ctrl_slash_and_ctrl_shift_p_toggle_the_command_palette() {
    let mut state = AppState::default();
    assert!(is_command_palette_toggle_shortcut(&key("/").ctrl(), false));
    assert!(is_command_palette_toggle_shortcut(
        &key("P").ctrl().shift(),
        false
    ));
    assert!(!is_command_palette_toggle_shortcut(
        &key("/").ctrl().alt(),
        false
    ));
    assert!(!is_command_palette_toggle_shortcut(&key("p").ctrl(), false));
    let out = command_palette_key_down(&mut state, &key("/").ctrl(), false);
    assert!(out.prevent_default && out.stop_propagation);
    assert_eq!(
        state.get("openDialog"),
        Some(&json!({ "name": "commandPalette" }))
    );
    command_palette_key_down(&mut state, &key("P").ctrl().shift(), false);
    assert_eq!(state.get("openDialog"), Some(&Value::Null));
}

#[test]
fn ctrl_v_sets_the_plain_paste_flag_and_the_paste_event_reads_it() {
    let mut ed = Editor::new(vec![]);
    let out = ed.down(key("V").ctrl().shift());
    assert!(ed.keyboard.is_plain_paste);
    assert!(has(&out, &KeyEffect::PlainPasteTimer));
    let target = ClipboardTarget {
        editor_active: true,
        writable: false,
        canvas_under_pointer: true,
    };
    assert_eq!(
        on_clipboard_event(&ed.tools, &ed.keyboard, ClipboardEventKind::Paste, target),
        ClipboardOutcome::Paste { plain: true }
    );
    ed.down(key("v").ctrl());
    assert!(!ed.keyboard.is_plain_paste);
    assert_eq!(
        on_clipboard_event(&ed.tools, &ed.keyboard, ClipboardEventKind::Copy, target),
        ClipboardOutcome::Action(ActionName::Copy)
    );
    assert_eq!(
        on_clipboard_event(&ed.tools, &ed.keyboard, ClipboardEventKind::Cut, target),
        ClipboardOutcome::Action(ActionName::Cut)
    );
    let in_field = ClipboardTarget {
        writable: true,
        ..target
    };
    assert_eq!(
        on_clipboard_event(&ed.tools, &ed.keyboard, ClipboardEventKind::Copy, in_field),
        ClipboardOutcome::Ignored
    );
    let outside = ClipboardTarget {
        editor_active: false,
        ..target
    };
    assert_eq!(
        on_clipboard_event(&ed.tools, &ed.keyboard, ClipboardEventKind::Paste, outside),
        ClipboardOutcome::Ignored
    );
}

#[test]
fn text_fields_keep_their_keys() {
    let field = KeyTarget {
        writable: true,
        input_like: true,
        editor_focused: false,
    };
    let mut ed = Editor::new(vec![]);
    let out = ed.down(key("r").with_target(field));
    assert_eq!(ed.tool(), "selection");
    assert!(out.effects.is_empty());
    // no browser zoom inside a field
    let out = ed.down(Keystroke::new("=", "Equal").ctrl().with_target(field));
    assert!(out.prevent_default);
    assert!(out.effects.is_empty());
    // arrows move between buttons of an input-like target
    ed.scene = Scene::new(vec![rect("r", 10.0, 10.0)]);
    ed.select(&["r"]);
    ed.down(key("ArrowLeft").with_target(KeyTarget {
        input_like: true,
        ..KeyTarget::default()
    }));
    assert_eq!(ed.xy("r"), (10.0, 10.0));
}

/// `regressionTests.test.tsx` "arrow keys".
#[test]
fn arrow_keys_nudge_by_one() {
    let mut ed = Editor::new(vec![rect("r", 10.0, 10.0)]);
    ed.select(&["r"]);
    for k in [
        "ArrowLeft",
        "ArrowLeft",
        "ArrowRight",
        "ArrowUp",
        "ArrowUp",
        "ArrowDown",
    ] {
        let out = ed.press(key(k));
        assert!(out.prevent_default);
        assert!(has(&out, &KeyEffect::SceneUpdated));
    }
    assert_eq!(ed.xy("r"), (9.0, 9.0));
}

#[test]
fn shift_nudges_by_five_and_the_grid_sets_the_step() {
    assert_eq!(nudge_step(None, false), 1.0);
    assert_eq!(nudge_step(None, true), 5.0);
    assert_eq!(nudge_step(Some(20.0), false), 20.0);
    assert_eq!(nudge_step(Some(20.0), true), 1.0);
    assert_eq!(nudge_step(Some(0.0), true), 5.0);

    let mut ed = Editor::new(vec![rect("r", 0.0, 0.0)]);
    ed.select(&["r"]);
    ed.press(key("ArrowRight").shift());
    assert_eq!(ed.xy("r"), (5.0, 0.0));
    ed.app_state.insert("gridModeEnabled", json!(true));
    assert_eq!(effective_grid_size(&ed.app_state, &ed.props), Some(20.0));
    ed.press(key("ArrowDown"));
    assert_eq!(ed.xy("r"), (5.0, 20.0));
    ed.press(key("ArrowDown").shift());
    assert_eq!(ed.xy("r"), (5.0, 21.0));
    // the host's gridModeEnabled prop wins
    ed.props.grid_mode_enabled = Some(false);
    assert_eq!(effective_grid_size(&ed.app_state, &ed.props), None);
}

#[test]
fn nudges_take_bound_text_and_frame_children_along() {
    let mut container = rect("box", 0.0, 0.0);
    container.base.bound_elements = Some(vec![BoundElement {
        id: "label".into(),
        kind: BoundElementType::Text,
    }]);
    let mut label = text("label", "hi", 10.0, 10.0);
    if let ElementKind::Text(t) = &mut label.kind {
        t.container_id = Some("box".into());
    }
    let mut child = rect("child", 30.0, 30.0);
    child.base.frame_id = Some("frame".into());
    let mut ed = Editor::new(vec![
        container,
        label,
        frame("frame", 20.0, 200.0),
        child,
        rect("other", 500.0, 500.0),
    ]);
    ed.select(&["box", "frame"]);
    ed.press(key("ArrowRight"));
    assert_eq!(ed.xy("box"), (1.0, 0.0));
    assert_eq!(ed.xy("label"), (11.0, 10.0));
    assert_eq!(ed.xy("frame"), (21.0, 20.0));
    assert_eq!(ed.xy("child"), (31.0, 30.0));
    assert_eq!(ed.xy("other"), (500.0, 500.0));
}

#[test]
fn nudges_leave_arrows_bound_outside_the_selection_and_move_bound_arrows() {
    let binding = |id: &str| FixedPointBinding {
        element_id: id.into(),
        fixed_point: [0.5, 0.5],
        mode: BindMode::Orbit,
    };
    let mut target = rect("target", 200.0, 0.0);
    target.base.bound_elements = Some(vec![BoundElement {
        id: "link".into(),
        kind: BoundElementType::Arrow,
    }]);
    let mut link = arrow("link", vec![[0.0, 50.0], [150.0, 50.0]]);
    if let ElementKind::Arrow(a) = &mut link.kind {
        a.linear.end_binding = Some(binding("target"));
    }
    let mut ed = Editor::new(vec![rect("free", 0.0, 0.0), target, link]);

    // the arrow is bound to an element outside the selection: it stays
    ed.select(&["free", "link"]);
    ed.down(key("ArrowDown"));
    assert_eq!(ed.xy("free"), (0.0, 1.0));
    assert_eq!(ed.xy("link"), (0.0, 0.0));

    // moving the target lays its bound arrow out again
    let before = ed.el("link").clone();
    ed.select(&["target"]);
    ed.down(key("ArrowDown"));
    assert_eq!(ed.xy("target"), (200.0, 1.0));
    assert_ne!(ed.el("link").base.version, before.base.version);
}

#[test]
fn enter_starts_text_editing_enters_frames_and_edits_lines() {
    let mut ed = Editor::new(vec![rect("r", 0.0, 0.0)]);
    ed.select(&["r"]);
    let out = ed.down(key("Enter"));
    assert!(out.prevent_default);
    assert!(has(
        &out,
        &KeyEffect::StartTextEditing {
            scene_x: 50.0,
            scene_y: 50.0,
            container: Some("r".into()),
        }
    ));

    let mut ed = Editor::new(vec![text("t", "hello", 10.0, 20.0)]);
    ed.select(&["t"]);
    let (w, h) = (ed.el("t").base.width, ed.el("t").base.height);
    let out = ed.down(key("Enter"));
    assert!(has(
        &out,
        &KeyEffect::StartTextEditing {
            scene_x: 10.0 + w / 2.0,
            scene_y: 20.0 + h / 2.0,
            container: None,
        }
    ));

    let mut ed = Editor::new(vec![frame("f", 0.0, 100.0)]);
    ed.select(&["f"]);
    ed.down(key("Enter"));
    assert_eq!(ed.state("editingFrame"), json!("f"));

    // Enter on a line, Ctrl+Enter on an arrow: the line editor
    let mut ed = Editor::new(vec![line("l"), arrow("a", vec![[0.0, 0.0], [10.0, 0.0]])]);
    ed.select(&["l"]);
    let out = ed.down(key("Enter"));
    assert!(has(
        &out,
        &KeyEffect::ExecuteAction(ActionName::ToggleLinearEditor)
    ));
    ed.select(&["a"]);
    let out = ed.down(key("Enter"));
    assert!(!has(
        &out,
        &KeyEffect::ExecuteAction(ActionName::ToggleLinearEditor)
    ));
    let out = ed.down(key("Enter").ctrl());
    assert!(has(&out, &KeyEffect::ScheduleCapture));
    assert!(has(
        &out,
        &KeyEffect::ExecuteAction(ActionName::ToggleLinearEditor)
    ));
}

#[test]
fn enter_on_an_image_starts_cropping_and_escape_finishes() {
    let mut ed = Editor::new(vec![image("img")]);
    ed.select(&["img"]);
    let out = ed.down(key("Enter"));
    assert_eq!(
        out.effects,
        vec![KeyEffect::StartImageCropping {
            element_id: "img".into()
        }]
    );
    ed.app_state.insert("croppingElementId", json!("img"));
    let out = ed.down(key("Escape"));
    assert_eq!(out.effects, vec![KeyEffect::FinishImageCropping]);
}

#[test]
fn space_holds_the_pan_and_releases_it() {
    let mut ed = Editor::new(vec![]);
    let out = ed.down(key(" "));
    assert!(ed.tools.space_held);
    assert!(out.prevent_default);
    assert!(has(&out, &KeyEffect::Cursor(CursorChange::Grab)));
    let pan = PanStart {
        button: pointer_button::MAIN,
        pointer_count: 1,
        interaction_enabled: true,
        navigation_enabled: true,
        ..PanStart::default()
    };
    assert!(pan_starts(&ed.tools, pan));
    let up = ed.up(key(" "));
    assert!(!ed.tools.space_held);
    assert!(has(&up, &KeyEffect::Cursor(CursorChange::Reset)));
    assert!(!pan_starts(&ed.tools, pan));
    // the wheel button pans without Space
    assert!(pan_starts(
        &ed.tools,
        PanStart {
            button: pointer_button::WHEEL,
            ..pan
        }
    ));
    // not while a pointer is already down
    ed.keyboard.pointers_down = 1;
    ed.down(key(" "));
    assert!(!ed.tools.space_held);
}

#[test]
fn space_released_with_a_drawing_tool_clears_the_selection() {
    let mut ed = Editor::new(vec![rect("r", 0.0, 0.0)]);
    ed.down(key("r"));
    ed.select(&["r"]);
    ed.down(key(" "));
    let up = ed.up(key(" "));
    assert!(has(&up, &KeyEffect::Cursor(CursorChange::ApplyForTool)));
    assert_eq!(ed.state("selectedElementIds"), json!({}));
}

#[test]
fn s_and_g_open_the_colour_pickers_and_shift_f_the_font_picker() {
    let mut ed = Editor::new(vec![rect("r", 0.0, 0.0), text("t", "a", 0.0, 0.0)]);
    // nothing selected with the selection tool: nothing opens
    ed.down(key("s"));
    assert_eq!(ed.state("openPopup"), Value::Null);
    ed.select(&["r"]);
    let out = ed.down(key("s"));
    assert!(out.stop_propagation);
    assert_eq!(ed.state("openPopup"), json!("elementStroke"));
    ed.down(key("g"));
    assert_eq!(ed.state("openPopup"), json!("elementBackground"));
    ed.app_state.insert("openPopup", Value::Null);
    ed.down(key("F").shift());
    assert_eq!(ed.state("openPopup"), Value::Null);
    ed.select(&["t"]);
    let out = ed.down(key("F").shift());
    assert!(out.prevent_default);
    assert_eq!(ed.state("openPopup"), json!("fontFamily"));
    // g on a text: no background
    ed.app_state.insert("openPopup", Value::Null);
    ed.down(key("g"));
    assert_eq!(ed.state("openPopup"), Value::Null);
}

#[test]
fn eyedropper_keys() {
    let cases = [
        (key("i"), EyeDropperKind::Background),
        (key("S").shift(), EyeDropperKind::Stroke),
        (key("G").shift(), EyeDropperKind::Background),
    ];
    for (event, kind) in cases {
        let mut ed = Editor::new(vec![]);
        let out = ed.down(event.clone());
        assert!(has(&out, &KeyEffect::OpenEyeDropper(kind)), "{event:?}");
    }
}

/// `shortcuts.test.tsx`: Ctrl+Delete asks to clear the canvas.
#[test]
fn ctrl_backspace_and_ctrl_delete_ask_to_clear_the_canvas() {
    for k in ["Delete", "Backspace"] {
        let mut ed = Editor::new(vec![rect("r", 0.0, 0.0)]);
        ed.down(key(k).ctrl());
        assert_eq!(
            ed.keyboard.active_confirm_dialog.as_deref(),
            Some("clearCanvas")
        );
    }
    // Delete alone deletes the selection
    let mut ed = Editor::new(vec![rect("r", 0.0, 0.0)]);
    ed.select(&["r"]);
    let out = ed.down(key("Delete"));
    assert_eq!(performed(&out), Some(ActionName::DeleteSelectedElements));
}

/// `arrowBinding.test.tsx`: Ctrl held turns binding off, releasing it
/// restores the preference.
#[test]
fn ctrl_held_disables_binding() {
    let mut ed = Editor::new(vec![]);
    ed.app_state.insert("bindingPreference", json!("enabled"));
    ed.app_state.insert("isBindingEnabled", json!(true));
    let out = ed.down(key("Control").ctrl());
    assert_eq!(ed.state("isBindingEnabled"), json!(false));
    assert!(has(&out, &KeyEffect::TextToolRefresh));
    let up = ed.up(key("Control"));
    assert_eq!(ed.state("isBindingEnabled"), json!(true));
    assert!(has(&up, &KeyEffect::TextToolRefresh));
    // a repeat does not toggle again
    ed.app_state.insert("isBindingEnabled", json!(true));
    let mut repeat = key("Control").ctrl();
    repeat.repeat = true;
    ed.down(repeat);
    assert_eq!(ed.state("isBindingEnabled"), json!(true));
}

#[test]
fn tab_opens_the_convert_panel_and_cycles_the_type() {
    let focused = KeyTarget {
        editor_focused: true,
        ..KeyTarget::default()
    };
    let mut ed = Editor::new(vec![rect("r", 0.0, 0.0)]);
    ed.select(&["r"]);
    let out = ed.down(key("Tab").with_target(focused));
    assert!(out.prevent_default);
    assert!(ed.keyboard.convert_popup_open);
    assert!(out.effects.is_empty());
    let out = ed.down(key("Tab").shift().with_target(focused));
    assert_eq!(
        out.effects,
        vec![KeyEffect::ConvertElementType {
            conversion: Some(ConversionType::Generic),
            direction: ConvertDirection::Left,
        }]
    );
    ed.down(key("Escape"));
    assert!(!ed.keyboard.convert_popup_open);

    let l = line("l");
    let bound = {
        let mut a = arrow("a", vec![[0.0, 0.0], [1.0, 1.0]]);
        if let ElementKind::Arrow(f) = &mut a.kind {
            f.linear.start_binding = Some(FixedPointBinding {
                element_id: "r".into(),
                fixed_point: [0.5, 0.5],
                mode: BindMode::Orbit,
            });
        }
        a
    };
    assert_eq!(get_conversion_type(&[&l]), Some(ConversionType::Linear));
    assert_eq!(get_conversion_type(&[&bound]), None);
    assert_eq!(get_conversion_type(&[]), None);
}

#[test]
fn flowchart_keys() {
    let mut ed = Editor::new(vec![rect("r", 0.0, 0.0)]);
    ed.select(&["r"]);
    let out = ed.down(key("ArrowRight").ctrl());
    assert!(out.prevent_default);
    assert_eq!(
        out.effects,
        vec![KeyEffect::FlowchartCreate {
            start: Some("r".into()),
            direction: LinkDirection::Right,
        }]
    );
    // the element did not move
    assert_eq!(ed.xy("r"), (0.0, 0.0));
    let up = ed.up(key("Control"));
    assert!(has(&up, &KeyEffect::FlowchartCommit));

    let out = ed.down(key("ArrowDown").ctrl());
    assert!(has(
        &out,
        &KeyEffect::FlowchartCreate {
            start: Some("r".into()),
            direction: LinkDirection::Down,
        }
    ));
    let out = ed.down(key("Escape"));
    assert_eq!(out.effects, vec![KeyEffect::FlowchartCanceled]);

    let out = ed.down(key("ArrowUp").alt());
    assert_eq!(
        out.effects,
        vec![KeyEffect::FlowchartNavigate {
            from: "r".into(),
            direction: LinkDirection::Up,
        }]
    );
    ed.keyboard.flowchart.is_exploring = true;
    let up = ed.up(key("Alt"));
    assert!(has(&up, &KeyEffect::FlowchartNavigationEnded));
}

#[test]
fn arrow_key_release_rebinds_selected_arrows() {
    let mut ed = Editor::new(vec![arrow("a", vec![[0.0, 0.0], [10.0, 0.0]])]);
    ed.select(&["a"]);
    ed.app_state
        .insert("suggestedBinding", json!({ "id": "x" }));
    ed.press(key("ArrowRight"));
    assert_eq!(ed.xy("a"), (1.0, 0.0));
    assert_eq!(ed.state("suggestedBinding"), Value::Null);
}

#[test]
fn nothing_happens_while_interaction_is_disabled() {
    let mut ed = Editor::new(vec![]);
    ed.tools.options.interaction = excali_editor::tools::Interaction::Disabled;
    let out = ed.down(key("r"));
    assert!(out.effects.is_empty());
    assert_eq!(ed.tool(), "selection");
}
