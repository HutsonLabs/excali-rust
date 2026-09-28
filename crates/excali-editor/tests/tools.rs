//! The tool registry and active-tool state machine, pinned to upstream at
//! the pinned commit:
//!
//! - `packages/excalidraw/components/Tools.tsx:69-223` (`TOOLS`,
//!   `TOGGLE_TOOLS`, `getToolLetter`, `getToolShortcut`, `findShapeByKey`,
//!   `isToolButtonDisabled`);
//! - `packages/common/src/utils.ts:272-307` (`isSelectionLikeTool`,
//!   `updateActiveTool`);
//! - `packages/excalidraw/components/App.tsx` (`isInteractionEnabled`
//!   :963, `isToolSupported` :1045, `isToolLocked` :1079, `toggleLock`
//!   :5220, `togglePenMode` :5271, the tool keys of `onKeyDown`
//!   :5768-5846, `setActiveTool` :6210, pen detection :8830 and the pen-mode
//!   pointer gate :8963);
//! - `packages/excalidraw/actions/actionDeselect.ts:18-33` and
//!   `actionFinalize.tsx:343-355` (the tool after Esc / finalize);
//! - `packages/excalidraw/tests/tool.test.tsx` (the `findShapeByKey()`
//!   cases are ported verbatim; the `setActiveTool()` and forced-tool cases
//!   are ported at the state level).

use excali_core::element::StrokeVariability;
use excali_editor::tools::{
    find_shape_by_key, get_tool_letter, get_tool_shortcut, is_selection_like_tool, is_toggle_tool,
    tool_config, update_active_tool, ActiveTool, ActiveToolUpdate, ArrowType, CursorEffect,
    Interaction, KeyHintKind, PointerType, SelectionTool, SetActiveToolOptions, ShortcutLabels,
    Tool, ToolKeyAction, ToolKeyContext, ToolKeyEvent, ToolKeyOutcome, ToolOptions, ToolRefusal,
    ToolRequest, ToolState, ToolType, TOGGLE_TOOLS, TOOLS,
};
use serde_json::json;

// ---------------------------------------------------------------------------
// TOOLS (Tools.tsx:69-160)
// ---------------------------------------------------------------------------

/// Every `TOOLS` entry in upstream's key order: type, icon export, letter
/// keys, shift, numeric key, fillable (`None` = not set), toggle.
#[allow(clippy::type_complexity)]
const EXPECTED: &[(&str, &str, &[&str], bool, Option<&str>, Option<bool>, bool)] = &[
    ("hand", "handIcon", &["h"], false, None, None, true),
    (
        "selection",
        "SelectionIcon",
        &["v"],
        false,
        Some("1"),
        Some(true),
        false,
    ),
    (
        "rectangle",
        "RectangleIcon",
        &["r"],
        false,
        Some("2"),
        Some(true),
        false,
    ),
    (
        "diamond",
        "DiamondIcon",
        &["d"],
        false,
        Some("3"),
        Some(true),
        false,
    ),
    (
        "ellipse",
        "EllipseIcon",
        &["o"],
        false,
        Some("4"),
        Some(true),
        false,
    ),
    (
        "arrow",
        "ArrowIcon",
        &["a"],
        false,
        Some("5"),
        Some(true),
        false,
    ),
    (
        "line",
        "LineIcon",
        &["l"],
        false,
        Some("6"),
        Some(true),
        false,
    ),
    (
        "freedraw",
        "FreedrawIcon",
        &["p", "x"],
        false,
        Some("7"),
        None,
        false,
    ),
    ("text", "TextIcon", &["t"], false, Some("8"), None, false),
    (
        "stickynote",
        "stickyNoteToolIcon",
        &["n"],
        false,
        None,
        None,
        false,
    ),
    ("image", "ImageIcon", &[], false, Some("9"), None, false),
    ("eraser", "EraserIcon", &["e"], false, Some("0"), None, true),
    ("frame", "frameToolIcon", &["f"], false, None, None, false),
    (
        "autoshape",
        "drawShapeToolIcon",
        &["x"],
        true,
        None,
        Some(false),
        false,
    ),
    ("embeddable", "EmbedIcon", &[], false, None, None, false),
    (
        "laser",
        "laserPointerToolIcon",
        &["k"],
        false,
        None,
        None,
        false,
    ),
    (
        "bucketfill",
        "bucketFillIcon",
        &["b"],
        false,
        None,
        None,
        false,
    ),
    ("lasso", "LassoIcon", &[], false, None, Some(false), false),
];

#[test]
fn tools_table_matches_upstream_in_order() {
    assert_eq!(TOOLS.len(), EXPECTED.len());
    for ((ty, config), &(name, icon, letters, shift, numeric, fillable, toggle)) in
        TOOLS.iter().zip(EXPECTED)
    {
        assert_eq!(ty.as_str(), name);
        assert_eq!(config.icon, icon, "{name} icon");
        assert_eq!(config.letter_key, letters, "{name} letter keys");
        assert_eq!(config.shift_key, shift, "{name} shift");
        assert_eq!(config.numeric_key, numeric, "{name} numeric key");
        assert_eq!(config.fillable, fillable, "{name} fillable");
        assert_eq!(config.toggle, toggle, "{name} toggle");
        assert_eq!(tool_config(*ty), Some(config));
    }
}

#[test]
fn magicframe_is_a_tool_type_without_a_registry_entry() {
    assert_eq!(ToolType::ALL.len(), 19);
    assert_eq!(tool_config(ToolType::Magicframe), None);
    assert_eq!(
        ToolType::from_name("magicframe"),
        Some(ToolType::Magicframe)
    );
    for ty in ToolType::ALL {
        assert_eq!(ToolType::from_name(ty.as_str()), Some(ty));
    }
    assert_eq!(ToolType::from_name("custom"), None);
    assert_eq!(ToolType::from_name("Rectangle"), None);
}

#[test]
fn every_number_key_maps_to_exactly_one_tool() {
    let digits: Vec<_> = TOOLS
        .iter()
        .filter_map(|(ty, c)| c.numeric_key.map(|k| (k, ty.as_str())))
        .collect();
    assert_eq!(
        digits,
        [
            ("1", "selection"),
            ("2", "rectangle"),
            ("3", "diamond"),
            ("4", "ellipse"),
            ("5", "arrow"),
            ("6", "line"),
            ("7", "freedraw"),
            ("8", "text"),
            ("9", "image"),
            ("0", "eraser"),
        ]
    );
}

#[test]
fn toggle_tools_are_hand_and_eraser() {
    assert_eq!(TOGGLE_TOOLS, [ToolType::Hand, ToolType::Eraser]);
    assert!(is_toggle_tool(&Tool::Builtin(ToolType::Hand)));
    assert!(is_toggle_tool(&Tool::Builtin(ToolType::Eraser)));
    assert!(!is_toggle_tool(&Tool::Builtin(ToolType::Laser)));
    assert!(!is_toggle_tool(&Tool::Custom("hand".into())));
}

#[test]
fn fillable_reads_as_truthy() {
    assert!(tool_config(ToolType::Rectangle).unwrap().is_fillable());
    assert!(!tool_config(ToolType::Freedraw).unwrap().is_fillable());
    assert!(!tool_config(ToolType::Autoshape).unwrap().is_fillable());
}

// ---------------------------------------------------------------------------
// getToolLetter / getToolShortcut (Tools.tsx:171-190)
// ---------------------------------------------------------------------------

#[test]
fn tool_letters_are_capitalised_first_letter_keys() {
    let en = ShortcutLabels::EN;
    assert_eq!(
        get_tool_letter(ToolType::Selection, &en).as_deref(),
        Some("V")
    );
    assert_eq!(
        get_tool_letter(ToolType::Rectangle, &en).as_deref(),
        Some("R")
    );
    assert_eq!(get_tool_letter(ToolType::Text, &en).as_deref(), Some("T"));
    assert_eq!(
        get_tool_letter(ToolType::Stickynote, &en).as_deref(),
        Some("N")
    );
    assert_eq!(get_tool_letter(ToolType::Eraser, &en).as_deref(), Some("E"));
    // the first of several letter keys is the one shown
    assert_eq!(
        get_tool_letter(ToolType::Freedraw, &en).as_deref(),
        Some("P")
    );
    // shift-bound: getShortcutKey("Shift+X")
    assert_eq!(
        get_tool_letter(ToolType::Autoshape, &en).as_deref(),
        Some("Shift+X")
    );
    assert_eq!(get_tool_letter(ToolType::Image, &en), None);
    assert_eq!(get_tool_letter(ToolType::Lasso, &en), None);
}

#[test]
fn tool_shortcut_hints_match_upstream() {
    let en = ShortcutLabels::EN;
    assert_eq!(get_tool_shortcut(ToolType::Rectangle, &en), "R or 2");
    assert_eq!(get_tool_shortcut(ToolType::Freedraw, &en), "P or 7");
    assert_eq!(get_tool_shortcut(ToolType::Eraser, &en), "E or 0");
    assert_eq!(get_tool_shortcut(ToolType::Hand, &en), "H");
    assert_eq!(get_tool_shortcut(ToolType::Image, &en), "9");
    assert_eq!(get_tool_shortcut(ToolType::Autoshape, &en), "Shift+X");
    // `${letter || numericKey}` with neither set
    assert_eq!(get_tool_shortcut(ToolType::Embeddable, &en), "undefined");

    let de = ShortcutLabels {
        or: "oder",
        shift: "Umschalt",
    };
    assert_eq!(get_tool_shortcut(ToolType::Rectangle, &de), "R oder 2");
    assert_eq!(get_tool_shortcut(ToolType::Autoshape, &de), "Umschalt+X");
}

// ---------------------------------------------------------------------------
// findShapeByKey (Tools.tsx:192-223; tests/tool.test.tsx "findShapeByKey()")
// ---------------------------------------------------------------------------

#[test]
fn selection_shortcuts_activate_selection_when_its_preferred() {
    let p = SelectionTool::Selection;
    assert_eq!(find_shape_by_key("v", p, false), Some(ToolType::Selection));
    assert_eq!(find_shape_by_key("1", p, false), Some(ToolType::Selection));
}

#[test]
fn selection_shortcuts_activate_lasso_when_its_preferred() {
    let p = SelectionTool::Lasso;
    assert_eq!(find_shape_by_key("v", p, false), Some(ToolType::Lasso));
    assert_eq!(find_shape_by_key("1", p, false), Some(ToolType::Lasso));
}

#[test]
fn letter_shortcuts_are_caps_lock_insensitive() {
    let p = SelectionTool::Selection;
    assert_eq!(find_shape_by_key("V", p, false), Some(ToolType::Selection));
    assert_eq!(find_shape_by_key("R", p, false), Some(ToolType::Rectangle));
    assert_eq!(find_shape_by_key("X", p, false), Some(ToolType::Freedraw));
}

#[test]
fn matches_shift_bound_tools_only_when_shift_is_held() {
    let p = SelectionTool::Selection;
    assert_eq!(find_shape_by_key("X", p, true), Some(ToolType::Autoshape));
    assert_eq!(find_shape_by_key("x", p, true), Some(ToolType::Autoshape));
    // Pressing "X" while CapsLock is active (no shift) stays freedraw
    assert_eq!(find_shape_by_key("X", p, false), Some(ToolType::Freedraw));
}

#[test]
fn does_not_match_plain_bound_tools_when_shift_is_held() {
    let p = SelectionTool::Selection;
    assert_eq!(find_shape_by_key("R", p, true), None);
    assert_eq!(find_shape_by_key("V", p, true), None);
    // digits with shift (e.g. Shift+1 zoom-to-fit) are not tool keys
    assert_eq!(find_shape_by_key("1", p, true), None);
}

#[test]
fn every_letter_and_number_key_in_both_cases() {
    let p = SelectionTool::Selection;
    let cases = [
        ("h", ToolType::Hand),
        ("v", ToolType::Selection),
        ("r", ToolType::Rectangle),
        ("d", ToolType::Diamond),
        ("o", ToolType::Ellipse),
        ("a", ToolType::Arrow),
        ("l", ToolType::Line),
        ("p", ToolType::Freedraw),
        ("x", ToolType::Freedraw),
        ("t", ToolType::Text),
        ("n", ToolType::Stickynote),
        ("e", ToolType::Eraser),
        ("f", ToolType::Frame),
        ("k", ToolType::Laser),
        ("b", ToolType::Bucketfill),
        ("1", ToolType::Selection),
        ("2", ToolType::Rectangle),
        ("3", ToolType::Diamond),
        ("4", ToolType::Ellipse),
        ("5", ToolType::Arrow),
        ("6", ToolType::Line),
        ("7", ToolType::Freedraw),
        ("8", ToolType::Text),
        ("9", ToolType::Image),
        ("0", ToolType::Eraser),
    ];
    for (key, ty) in cases {
        assert_eq!(find_shape_by_key(key, p, false), Some(ty), "{key}");
        let upper = key.to_uppercase();
        assert_eq!(find_shape_by_key(&upper, p, false), Some(ty), "{upper}");
    }
    for key in ["q", "Q", "s", "g", "i", "z", "?", " ", "Escape", "", "10"] {
        assert_eq!(find_shape_by_key(key, p, false), None, "{key:?}");
    }
}

// ---------------------------------------------------------------------------
// updateActiveTool (utils.ts:276-307)
// ---------------------------------------------------------------------------

fn builtin(ty: ToolType) -> Tool {
    Tool::Builtin(ty)
}

#[test]
fn default_active_tool_matches_upstream_json() {
    let tool = ActiveTool::default();
    assert_eq!(
        tool.to_json(),
        json!({"type":"selection","customType":null,"locked":false,"fromSelection":false,"lastActiveTool":null})
    );
    assert_eq!(ActiveTool::from_json(&tool.to_json()), Some(tool));
}

#[test]
fn active_tool_json_round_trips_nested_and_custom_tools() {
    let value = json!({
        "type": "custom",
        "customType": "comment",
        "locked": true,
        "fromSelection": false,
        "lastActiveTool": {"type":"rectangle","customType":null,"locked":false,"fromSelection":true,"lastActiveTool":null},
    });
    let tool = ActiveTool::from_json(&value).unwrap();
    assert_eq!(tool.tool, Tool::Custom("comment".into()));
    assert!(tool.locked);
    let last = tool.last_active_tool.as_deref().unwrap();
    assert_eq!(last.tool, builtin(ToolType::Rectangle));
    assert!(last.from_selection);
    assert_eq!(tool.to_json(), value);
    assert_eq!(ActiveTool::from_json(&json!({"type": "nope"})), None);
}

#[test]
fn update_active_tool_keeps_lock_and_last_tool_and_resets_from_selection() {
    let current = ActiveTool {
        tool: builtin(ToolType::Rectangle),
        last_active_tool: Some(Box::new(ActiveTool::default())),
        locked: true,
        from_selection: true,
    };
    let next = update_active_tool(&current, ActiveToolUpdate::to(builtin(ToolType::Ellipse)));
    assert_eq!(next.tool, builtin(ToolType::Ellipse));
    assert!(next.locked);
    assert!(!next.from_selection);
    assert_eq!(next.last_active_tool, current.last_active_tool);

    let next = update_active_tool(
        &current,
        ActiveToolUpdate {
            locked: Some(false),
            from_selection: Some(true),
            last_active_tool: Some(None),
            ..ActiveToolUpdate::to(builtin(ToolType::Diamond))
        },
    );
    assert!(!next.locked);
    assert!(next.from_selection);
    assert_eq!(next.last_active_tool, None);
}

#[test]
fn update_active_tool_to_custom_keeps_from_selection_and_last_tool() {
    let current = ActiveTool {
        tool: builtin(ToolType::Rectangle),
        last_active_tool: Some(Box::new(ActiveTool::default())),
        locked: false,
        from_selection: true,
    };
    let next = update_active_tool(
        &current,
        ActiveToolUpdate {
            last_active_tool: Some(None),
            ..ActiveToolUpdate::to(Tool::Custom("pin".into()))
        },
    );
    assert_eq!(next.tool, Tool::Custom("pin".into()));
    assert!(next.from_selection);
    assert_eq!(next.last_active_tool, current.last_active_tool);
}

#[test]
fn selection_like_tools() {
    assert!(is_selection_like_tool(&builtin(ToolType::Selection)));
    assert!(is_selection_like_tool(&builtin(ToolType::Lasso)));
    assert!(!is_selection_like_tool(&builtin(ToolType::Hand)));
    assert!(!is_selection_like_tool(&Tool::Custom("selection".into())));
}

// ---------------------------------------------------------------------------
// setActiveTool (App.tsx:6210-6335; tests/tool.test.tsx "setActiveTool()")
// ---------------------------------------------------------------------------

fn set(state: &mut ToolState, ty: ToolType) -> ToolSwitchResult {
    state.set_active_tool(
        ToolRequest::new(builtin(ty)),
        SetActiveToolOptions::default(),
    )
}

type ToolSwitchResult = Result<excali_editor::tools::ToolSwitch, ToolRefusal>;

#[test]
fn sets_the_active_tool_type() {
    let mut state = ToolState::default();
    assert_eq!(state.active_tool.tool, builtin(ToolType::Selection));
    set(&mut state, ToolType::Rectangle).unwrap();
    assert_eq!(state.active_tool.tool, builtin(ToolType::Rectangle));
    // drawing reverts to the preferred selection tool unless locked
    assert!(!state.is_tool_locked());
    assert_eq!(
        state.tool_after_finalize().tool,
        builtin(ToolType::Selection)
    );
}

#[test]
fn supports_tool_locking() {
    let mut state = ToolState::default();
    state
        .set_active_tool(
            ToolRequest {
                locked: Some(true),
                ..ToolRequest::new(builtin(ToolType::Rectangle))
            },
            SetActiveToolOptions::default(),
        )
        .unwrap();
    assert_eq!(state.active_tool.tool, builtin(ToolType::Rectangle));
    assert!(state.is_tool_locked());
}

#[test]
fn sets_a_custom_tool() {
    let mut state = ToolState::default();
    state
        .set_active_tool(
            ToolRequest::new(Tool::Custom("comment".into())),
            SetActiveToolOptions::default(),
        )
        .unwrap();
    assert_eq!(state.active_tool.tool, Tool::Custom("comment".into()));
    assert_eq!(state.active_tool.tool.type_name(), "custom");
    assert_eq!(state.active_tool.tool.custom_type(), Some("comment"));
}

#[test]
fn switching_to_a_drawing_tool_clears_the_selection_and_resets_state() {
    let mut state = ToolState::default();
    let switch = set(&mut state, ToolType::Rectangle).unwrap();
    assert!(switch.clear_selection);
    assert!(!switch.keep_selected_linear_element);
    assert!(switch.clear_suggested_binding);
    assert!(!switch.open_image_picker);
    assert!(!switch.schedule_capture);
    assert!(!switch.finalize_pending_gesture);
    assert_eq!(switch.cursor, CursorEffect::ForTool);

    // keepSelection only applies to lasso
    let switch = state
        .set_active_tool(
            ToolRequest::new(builtin(ToolType::Ellipse)),
            SetActiveToolOptions {
                keep_selection: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(switch.clear_selection);
}

#[test]
fn selection_and_lasso_keep_the_selection() {
    let mut state = ToolState::default();
    let switch = set(&mut state, ToolType::Selection).unwrap();
    assert!(!switch.clear_selection);
    assert!(switch.keep_selected_linear_element);

    let switch = set(&mut state, ToolType::Lasso).unwrap();
    assert!(switch.clear_selection);
    assert!(switch.keep_selected_linear_element);

    let switch = state
        .set_active_tool(
            ToolRequest::new(builtin(ToolType::Lasso)),
            SetActiveToolOptions {
                keep_selection: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(!switch.clear_selection);
}

#[test]
fn per_tool_side_effects() {
    let mut state = ToolState::default();
    assert_eq!(
        set(&mut state, ToolType::Hand).unwrap().cursor,
        CursorEffect::Grab
    );
    let switch = set(&mut state, ToolType::Image).unwrap();
    assert!(switch.open_image_picker);
    assert!(
        set(&mut state, ToolType::Freedraw)
            .unwrap()
            .schedule_capture
    );
    // linear tools keep the suggested binding
    assert!(
        !set(&mut state, ToolType::Arrow)
            .unwrap()
            .clear_suggested_binding
    );
    assert!(
        !set(&mut state, ToolType::Line)
            .unwrap()
            .clear_suggested_binding
    );

    // the tool cursor is left alone while space-panning, the grab cursor is not
    state.space_held = true;
    assert_eq!(
        set(&mut state, ToolType::Diamond).unwrap().cursor,
        CursorEffect::Unchanged
    );
    assert_eq!(
        set(&mut state, ToolType::Hand).unwrap().cursor,
        CursorEffect::Grab
    );

    state.space_held = false;
    state.pending_draw_shape = true;
    assert!(
        set(&mut state, ToolType::Selection)
            .unwrap()
            .finalize_pending_gesture
    );
}

#[test]
fn toggle_tools_record_and_restore_the_previous_tool() {
    let mut state = ToolState::default();
    set(&mut state, ToolType::Rectangle).unwrap();
    // activating a toggle tool records the currently active tool
    set(&mut state, ToolType::Eraser).unwrap();
    assert_eq!(state.active_tool.tool, builtin(ToolType::Eraser));
    let last = state.active_tool.last_active_tool.as_deref().unwrap();
    assert_eq!(last.tool, builtin(ToolType::Rectangle));

    // activation is idempotent by default
    set(&mut state, ToolType::Eraser).unwrap();
    assert_eq!(state.active_tool.tool, builtin(ToolType::Eraser));

    // with `toggle`, re-activating switches back
    state
        .set_active_tool(
            ToolRequest::new(builtin(ToolType::Eraser)),
            SetActiveToolOptions {
                toggle: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(state.active_tool.tool, builtin(ToolType::Rectangle));
    assert_eq!(state.active_tool.last_active_tool, None);

    // with no recorded tool it falls back to the preferred selection tool
    state.preferred_selection_tool.tool = SelectionTool::Lasso;
    state.active_tool = ActiveTool {
        tool: builtin(ToolType::Hand),
        ..ActiveTool::default()
    };
    state
        .set_active_tool(
            ToolRequest::new(builtin(ToolType::Hand)),
            SetActiveToolOptions {
                toggle: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(state.active_tool.tool, builtin(ToolType::Lasso));
}

#[test]
fn toggle_is_ignored_for_non_toggle_tools() {
    let mut state = ToolState::default();
    set(&mut state, ToolType::Rectangle).unwrap();
    state
        .set_active_tool(
            ToolRequest::new(builtin(ToolType::Rectangle)),
            SetActiveToolOptions {
                toggle: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(state.active_tool.tool, builtin(ToolType::Rectangle));
}

#[test]
fn esc_and_finalize_return_to_the_recorded_or_preferred_tool() {
    let mut state = ToolState::default();
    set(&mut state, ToolType::Text).unwrap();
    set(&mut state, ToolType::Hand).unwrap();
    let next = state.tool_after_finalize();
    assert_eq!(next.tool, builtin(ToolType::Text));
    assert_eq!(next.last_active_tool, None);

    set(&mut state, ToolType::Rectangle).unwrap();
    state.preferred_selection_tool.tool = SelectionTool::Lasso;
    assert_eq!(state.tool_after_finalize().tool, builtin(ToolType::Lasso));
}

// ---------------------------------------------------------------------------
// isToolSupported / forced tools (App.tsx:963-1081, 3519-3525;
// tests/tool.test.tsx "props.activeTool (forced tool)")
// ---------------------------------------------------------------------------

#[test]
fn image_tool_can_be_disabled_via_ui_options() {
    let mut state = ToolState {
        options: ToolOptions {
            image_tool: false,
            ..ToolOptions::default()
        },
        ..ToolState::default()
    };
    assert!(!state.is_tool_supported(&builtin(ToolType::Image)));
    assert!(state.is_tool_supported(&builtin(ToolType::Frame)));
    assert_eq!(
        set(&mut state, ToolType::Image),
        Err(ToolRefusal::Unsupported)
    );
    assert_eq!(state.active_tool.tool, builtin(ToolType::Selection));
}

#[test]
fn non_interactive_editor_supports_only_opted_in_tools() {
    let mut state = ToolState {
        options: ToolOptions {
            interaction: Interaction::Disabled,
            ..ToolOptions::default()
        },
        ..ToolState::default()
    };
    assert!(!state.is_interaction_enabled());
    for ty in ToolType::ALL {
        assert!(!state.is_tool_supported(&builtin(ty)), "{ty:?}");
    }
    assert!(!state.is_tool_supported(&Tool::Custom("x".into())));

    state.options.interaction = Interaction::Restricted {
        laser: true,
        custom: false,
    };
    assert!(!state.is_interaction_enabled());
    assert!(state.is_tool_supported(&builtin(ToolType::Laser)));
    assert!(!state.is_tool_supported(&builtin(ToolType::Hand)));
    assert!(!state.is_tool_supported(&Tool::Custom("x".into())));
    state.options.interaction = Interaction::Restricted {
        laser: false,
        custom: true,
    };
    assert!(state.is_tool_supported(&Tool::Custom("x".into())));
}

#[test]
fn forced_tool_behaves_as_locked_without_touching_the_padlock() {
    let mut state = ToolState::default();
    state.force_tool(Some(builtin(ToolType::Rectangle)));
    assert_eq!(state.active_tool.tool, builtin(ToolType::Rectangle));
    assert!(!state.active_tool.locked);
    assert!(state.is_tool_locked());
}

#[test]
fn forced_tool_refuses_other_activations_and_the_lock() {
    let mut state = ToolState::default();
    state.force_tool(Some(builtin(ToolType::Selection)));
    assert_eq!(
        set(&mut state, ToolType::Freedraw),
        Err(ToolRefusal::Forced)
    );
    assert_eq!(state.active_tool.tool, builtin(ToolType::Selection));
    // re-activating the forced tool is allowed
    assert!(set(&mut state, ToolType::Selection).is_ok());

    assert!(!state.toggle_lock());
    assert!(!state.active_tool.locked);

    assert!(state.is_tool_button_disabled("freedraw"));
    assert!(!state.is_tool_button_disabled("selection"));
}

#[test]
fn forcing_does_not_clobber_the_padlock_preference() {
    let mut state = ToolState::default();
    assert!(state.toggle_lock());
    assert!(state.active_tool.locked);
    state.force_tool(Some(builtin(ToolType::Freedraw)));
    assert_eq!(state.active_tool.tool, builtin(ToolType::Freedraw));
    assert!(state.active_tool.locked);
    state.force_tool(None);
    assert!(state.active_tool.locked);
    assert!(!state.is_tool_button_disabled("rectangle"));
    // unset: the current tool stays, switching works again
    assert_eq!(state.active_tool.tool, builtin(ToolType::Freedraw));
    set(&mut state, ToolType::Rectangle).unwrap();
    assert_eq!(state.active_tool.tool, builtin(ToolType::Rectangle));
}

#[test]
fn forced_custom_tools_compare_by_custom_type() {
    let mut state = ToolState::default();
    state.force_tool(Some(Tool::Custom("comment".into())));
    assert_eq!(state.active_tool.tool, Tool::Custom("comment".into()));
    state.force_tool(Some(Tool::Custom("pin".into())));
    assert_eq!(state.active_tool.tool.custom_type(), Some("pin"));
    assert_eq!(
        state.set_active_tool(
            ToolRequest::new(Tool::Custom("comment".into())),
            SetActiveToolOptions::default()
        ),
        Err(ToolRefusal::Forced)
    );
}

#[test]
fn forced_tools_snap_back_after_a_bypassing_write() {
    let mut state = ToolState::default();
    state.force_tool(Some(builtin(ToolType::Laser)));
    state.active_tool = update_active_tool(
        &state.active_tool,
        ActiveToolUpdate::to(builtin(ToolType::Selection)),
    );
    state.sync_forced_tool();
    assert_eq!(state.active_tool.tool, builtin(ToolType::Laser));
}

#[test]
fn the_image_tool_cannot_be_forced() {
    let mut state = ToolState::default();
    state.force_tool(Some(builtin(ToolType::Image)));
    assert_eq!(state.active_tool.tool, builtin(ToolType::Selection));
}

#[test]
fn forcing_a_non_activatable_tool_resolves_to_selection_until_activatable() {
    let mut state = ToolState {
        options: ToolOptions {
            interaction: Interaction::Disabled,
            ..ToolOptions::default()
        },
        ..ToolState::default()
    };
    state.force_tool(Some(builtin(ToolType::Laser)));
    assert_eq!(state.active_tool.tool, builtin(ToolType::Selection));

    state.options.interaction = Interaction::Restricted {
        laser: true,
        custom: false,
    };
    state.sync_options();
    assert_eq!(state.active_tool.tool, builtin(ToolType::Laser));
}

/// `tool.test.tsx:303-331` "composes with interaction.enabled.tools
/// (presenter → viewer)": the viewer's forced selection applies although
/// selection itself is unsupported while inert, through `componentDidUpdate`'s
/// non-interactive reset (`App.tsx:3486-3499`).
#[test]
fn forced_tools_compose_with_interaction_enabled_tools_presenter_to_viewer() {
    let presenter = Interaction::Restricted {
        laser: true,
        custom: false,
    };
    let mut state = ToolState {
        options: ToolOptions {
            interaction: presenter,
            ..ToolOptions::default()
        },
        ..ToolState::default()
    };
    // presenter: forced laser while otherwise non-interactive
    state.force_tool(Some(builtin(ToolType::Laser)));
    assert_eq!(state.active_tool.tool, builtin(ToolType::Laser));
    assert!(!state.is_interaction_enabled());
    assert!(state.is_tool_supported(&builtin(ToolType::Laser)));

    // viewer: fully inert, tool resolves to the default selection
    state.options.interaction = Interaction::Disabled;
    state.force_tool(Some(builtin(ToolType::Selection)));
    assert_eq!(state.active_tool.tool, builtin(ToolType::Selection));

    // back to presenter
    state.options.interaction = presenter;
    state.force_tool(Some(builtin(ToolType::Laser)));
    assert_eq!(state.active_tool.tool, builtin(ToolType::Laser));
}

/// `App.tsx:3486-3499`: while non-interactive, an unsupported active tool
/// resets to selection through `updateActiveTool`, with no forced tool.
#[test]
fn a_stale_tool_resets_to_selection_when_interaction_is_disabled() {
    let mut state = ToolState::default();
    set(&mut state, ToolType::Rectangle).unwrap();
    state.options.interaction = Interaction::Disabled;
    state.sync_options();
    assert_eq!(state.active_tool.tool, builtin(ToolType::Selection));

    // restricted to the laser: the rectangle is stale too, the laser is not
    let mut state = ToolState::default();
    set(&mut state, ToolType::Rectangle).unwrap();
    state.options.interaction = Interaction::Restricted {
        laser: true,
        custom: false,
    };
    state.sync_options();
    assert_eq!(state.active_tool.tool, builtin(ToolType::Selection));

    let mut state = ToolState::default();
    set(&mut state, ToolType::Laser).unwrap();
    state.options.interaction = Interaction::Restricted {
        laser: true,
        custom: false,
    };
    state.sync_options();
    assert_eq!(state.active_tool.tool, builtin(ToolType::Laser));

    // interactive: nothing to reset
    let mut state = ToolState::default();
    set(&mut state, ToolType::Rectangle).unwrap();
    state.sync_options();
    assert_eq!(state.active_tool.tool, builtin(ToolType::Rectangle));
}

// ---------------------------------------------------------------------------
// toggleLock (App.tsx:5220-5245)
// ---------------------------------------------------------------------------

#[test]
fn toggle_lock_flips_the_padlock_and_unlocking_returns_to_selection() {
    let mut state = ToolState::default();
    set(&mut state, ToolType::Rectangle).unwrap();
    state.active_tool.from_selection = true;
    assert!(state.toggle_lock());
    assert!(state.active_tool.locked);
    assert_eq!(state.active_tool.tool, builtin(ToolType::Rectangle));
    assert!(state.active_tool.from_selection);

    state.preferred_selection_tool.tool = SelectionTool::Lasso;
    assert!(state.toggle_lock());
    assert!(!state.active_tool.locked);
    assert_eq!(state.active_tool.tool, builtin(ToolType::Lasso));
    assert!(!state.active_tool.from_selection);
}

// ---------------------------------------------------------------------------
// togglePenMode, pen detection and the pen-mode pointer gate
// (App.tsx:5271-5281, 8830-8837, 8963-8969, 9341-9343)
// ---------------------------------------------------------------------------

#[test]
fn toggle_pen_mode_flips_or_forces_and_marks_the_pen_detected() {
    let mut state = ToolState::default();
    assert!(!state.pen_mode);
    assert!(!state.pen_detected);
    assert_eq!(state.stroke_variability, StrokeVariability::Constant);

    state.toggle_pen_mode(None);
    assert!(state.pen_mode);
    assert!(state.pen_detected);
    // the first toggle switches strokes to variable width
    assert_eq!(state.stroke_variability, StrokeVariability::Variable);

    state.stroke_variability = StrokeVariability::Constant;
    state.toggle_pen_mode(None);
    assert!(!state.pen_mode);
    assert_eq!(state.stroke_variability, StrokeVariability::Constant);

    state.toggle_pen_mode(Some(false));
    assert!(!state.pen_mode);
    state.toggle_pen_mode(Some(true));
    assert!(state.pen_mode);
}

#[test]
fn a_pen_pointer_turns_pen_mode_on_once() {
    let mut state = ToolState::default();
    assert!(!state.detect_pen(PointerType::Mouse));
    assert!(!state.detect_pen(PointerType::Touch));
    assert!(!state.pen_mode);
    assert!(state.detect_pen(PointerType::Pen));
    assert!(state.pen_mode && state.pen_detected);
    assert_eq!(state.stroke_variability, StrokeVariability::Variable);

    // the user can switch it off again; later pens don't re-enable it
    state.toggle_pen_mode(Some(false));
    assert!(!state.detect_pen(PointerType::Pen));
    assert!(!state.pen_mode);
}

#[test]
fn pen_mode_ignores_touch_except_for_selection_text_and_image() {
    let mut state = ToolState::default();
    state.toggle_pen_mode(Some(true));
    for ty in [
        ToolType::Selection,
        ToolType::Lasso,
        ToolType::Text,
        ToolType::Image,
    ] {
        state.active_tool.tool = builtin(ty);
        assert!(state.allows_pointer_down(PointerType::Touch), "{ty:?}");
    }
    state.active_tool.tool = builtin(ToolType::Freedraw);
    assert!(!state.allows_pointer_down(PointerType::Touch));
    assert!(state.allows_pointer_down(PointerType::Pen));
    assert!(state.allows_pointer_down(PointerType::Mouse));
    // two-finger pinch doesn't zoom while drawing with a pen
    assert!(state.pinch_zoom_locked());

    state.toggle_pen_mode(Some(false));
    assert!(state.allows_pointer_down(PointerType::Touch));
    assert!(!state.pinch_zoom_locked());
}

// ---------------------------------------------------------------------------
// Tool keys in onKeyDown (App.tsx:5768-5846)
// ---------------------------------------------------------------------------

fn key(k: &str) -> ToolKeyEvent<'_> {
    ToolKeyEvent {
        key: k,
        ..ToolKeyEvent::default()
    }
}

fn switched(outcome: &ToolKeyOutcome) -> bool {
    matches!(
        outcome,
        ToolKeyOutcome::Tool {
            action: ToolKeyAction::SetTool(Ok(_)),
            ..
        }
    )
}

#[test]
fn letter_and_number_keys_switch_tools() {
    let mut state = ToolState::default();
    let ctx = ToolKeyContext::default();
    assert!(switched(&state.handle_tool_key(key("r"), &ctx)));
    assert_eq!(state.active_tool.tool, builtin(ToolType::Rectangle));
    assert!(switched(&state.handle_tool_key(key("4"), &ctx)));
    assert_eq!(state.active_tool.tool, builtin(ToolType::Ellipse));
    // CapsLock
    assert!(switched(&state.handle_tool_key(key("D"), &ctx)));
    assert_eq!(state.active_tool.tool, builtin(ToolType::Diamond));
    // Shift+X
    let shift_x = ToolKeyEvent {
        key: "X",
        shift: true,
        ..ToolKeyEvent::default()
    };
    assert!(switched(&state.handle_tool_key(shift_x, &ctx)));
    assert_eq!(state.active_tool.tool, builtin(ToolType::Autoshape));
}

#[test]
fn modifier_keys_and_in_progress_gestures_block_tool_keys() {
    let mut state = ToolState::default();
    for event in [
        ToolKeyEvent {
            key: "r",
            ctrl: true,
            ..ToolKeyEvent::default()
        },
        ToolKeyEvent {
            key: "r",
            alt: true,
            ..ToolKeyEvent::default()
        },
        ToolKeyEvent {
            key: "r",
            meta: true,
            ..ToolKeyEvent::default()
        },
    ] {
        assert_eq!(
            state.handle_tool_key(event, &ToolKeyContext::default()),
            ToolKeyOutcome::NotHandled
        );
    }
    let busy = ToolKeyContext {
        gesture_in_progress: true,
        ..ToolKeyContext::default()
    };
    assert_eq!(
        state.handle_tool_key(key("r"), &busy),
        ToolKeyOutcome::NotHandled
    );
    let host_view_mode = ToolKeyContext {
        prevent_tool_switching: true,
        ..ToolKeyContext::default()
    };
    assert_eq!(
        state.handle_tool_key(key("r"), &host_view_mode),
        ToolKeyOutcome::NotHandled
    );
    assert_eq!(state.active_tool.tool, builtin(ToolType::Selection));
}

#[test]
fn toggle_tool_keys_switch_back() {
    let mut state = ToolState::default();
    let ctx = ToolKeyContext::default();
    state.handle_tool_key(key("r"), &ctx);
    state.handle_tool_key(key("h"), &ctx);
    assert_eq!(state.active_tool.tool, builtin(ToolType::Hand));
    state.handle_tool_key(key("h"), &ctx);
    assert_eq!(state.active_tool.tool, builtin(ToolType::Rectangle));
    state.handle_tool_key(key("0"), &ctx);
    assert_eq!(state.active_tool.tool, builtin(ToolType::Eraser));
    state.handle_tool_key(key("E"), &ctx);
    assert_eq!(state.active_tool.tool, builtin(ToolType::Rectangle));
}

#[test]
fn arrow_key_again_cycles_the_arrow_type() {
    let mut state = ToolState::default();
    let mut ctx = ToolKeyContext::default();
    let outcome = state.handle_tool_key(key("a"), &ctx);
    match outcome {
        ToolKeyOutcome::Tool {
            tool,
            next_arrow_type,
            hint,
            ..
        } => {
            assert_eq!(tool, ToolType::Arrow);
            assert_eq!(next_arrow_type, None);
            assert_eq!(hint, Some(KeyHintKind::Letter));
        }
        other => panic!("{other:?}"),
    }
    for (current, next) in [
        (ArrowType::Sharp, ArrowType::Round),
        (ArrowType::Round, ArrowType::Elbow),
        (ArrowType::Elbow, ArrowType::Sharp),
    ] {
        ctx.arrow_type = current;
        match state.handle_tool_key(key("5"), &ctx) {
            ToolKeyOutcome::Tool {
                next_arrow_type,
                hint,
                action,
                ..
            } => {
                assert_eq!(next_arrow_type, Some(next));
                assert_eq!(hint, None);
                assert!(matches!(action, ToolKeyAction::SetTool(Ok(_))));
            }
            other => panic!("{other:?}"),
        }
    }
    match state.handle_tool_key(key("6"), &ctx) {
        ToolKeyOutcome::Tool { hint, .. } => assert_eq!(hint, Some(KeyHintKind::Digit)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn bucket_fill_key_again_cycles_the_color() {
    let mut state = ToolState::default();
    let ctx = ToolKeyContext::default();
    assert!(switched(&state.handle_tool_key(key("b"), &ctx)));
    match state.handle_tool_key(key("b"), &ctx) {
        ToolKeyOutcome::Tool { action, .. } => {
            assert_eq!(action, ToolKeyAction::CycleBucketFillColor)
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(state.active_tool.tool, builtin(ToolType::Bucketfill));
}

#[test]
fn selection_key_leaves_the_laser_for_the_preferred_selection_tool() {
    let mut state = ToolState::default();
    state.preferred_selection_tool.tool = SelectionTool::Lasso;
    let ctx = ToolKeyContext::default();
    state.handle_tool_key(key("k"), &ctx);
    assert_eq!(state.active_tool.tool, builtin(ToolType::Laser));
    state.handle_tool_key(key("v"), &ctx);
    assert_eq!(state.active_tool.tool, builtin(ToolType::Lasso));
}

#[test]
fn q_toggles_the_tool_lock() {
    let mut state = ToolState::default();
    let ctx = ToolKeyContext::default();
    assert_eq!(
        state.handle_tool_key(key("q"), &ctx),
        ToolKeyOutcome::ToggledLock { changed: true }
    );
    assert!(state.active_tool.locked);
    // Q is not CapsLock-insensitive upstream (`event.key === KEYS.Q`)
    assert_eq!(
        state.handle_tool_key(key("Q"), &ctx),
        ToolKeyOutcome::NotHandled
    );
    state.force_tool(Some(builtin(ToolType::Rectangle)));
    assert_eq!(
        state.handle_tool_key(key("q"), &ctx),
        ToolKeyOutcome::ToggledLock { changed: false }
    );
    assert!(state.active_tool.locked);
}

#[test]
fn view_mode_allows_only_laser_and_hand() {
    let mut state = ToolState::default();
    let ctx = ToolKeyContext {
        view_mode_enabled: true,
        ..ToolKeyContext::default()
    };
    assert_eq!(
        state.handle_tool_key(key("r"), &ctx),
        ToolKeyOutcome::Ignored
    );
    assert_eq!(state.active_tool.tool, builtin(ToolType::Selection));
    assert!(switched(&state.handle_tool_key(key("k"), &ctx)));
    assert_eq!(state.active_tool.tool, builtin(ToolType::Laser));
    assert!(switched(&state.handle_tool_key(key("h"), &ctx)));
    assert_eq!(state.active_tool.tool, builtin(ToolType::Hand));
    // a key with no tool is ignored too (the view-mode `return` runs first)
    assert_eq!(
        state.handle_tool_key(key("q"), &ctx),
        ToolKeyOutcome::Ignored
    );

    // Escape in view mode returns to selection
    match state.handle_tool_key(key("Escape"), &ctx) {
        ToolKeyOutcome::ViewModeEscape(Ok(_)) => {}
        other => panic!("{other:?}"),
    }
    assert_eq!(state.active_tool.tool, builtin(ToolType::Selection));
}

#[test]
fn forced_tool_refuses_key_switches_without_side_effects() {
    let mut state = ToolState::default();
    state.force_tool(Some(builtin(ToolType::Selection)));
    match state.handle_tool_key(key("r"), &ToolKeyContext::default()) {
        ToolKeyOutcome::Tool { action, .. } => {
            assert_eq!(action, ToolKeyAction::SetTool(Err(ToolRefusal::Forced)))
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(state.active_tool.tool, builtin(ToolType::Selection));
}
