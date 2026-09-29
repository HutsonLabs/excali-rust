//! The text editor's DOM half (ex-512), its pure rules: CSSOM names to CSS
//! properties for every style value upstream assigns, the platform's
//! Ctrl/Cmd, which pointer downs submit or suspend the blur submit
//! (`textWysiwyg.tsx:920-1054`), caret boundaries and the closest caret
//! (`:117-205`), and the editor box's stylesheet rules
//! (`css/styles.scss:8`, `:134-149`).
//!
//! The browser half (`tests/web/text-editing`,
//! `scripts/web/text-editing.sh`) types into the mounted textarea in
//! Chromium and holds the elements to upstream's fixture.

use excali_ui::text_editor::{
    caret_boundary_offsets, classify_pointer_down, closest_caret_offset, css_property_name,
    is_darwin, rearms_on_pointer_up, refocuses_on_scene_update, PointerDownAction,
    PointerDownTarget, TEXTAREA_ATTRIBUTES, TEXT_EDITOR_CSS,
};
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../excali-editor/tests/fixtures/text-editing.json"
    ))
    .unwrap()
}

#[test]
fn every_style_upstream_assigns_has_its_css_property() {
    let mut names: Vec<String> = Vec::new();
    for case in fixture()["sessions"].as_array().unwrap() {
        for record in case["records"].as_array().unwrap() {
            for key in record["style"].as_object().unwrap().keys() {
                if !names.contains(key) {
                    names.push(key.clone());
                }
            }
        }
    }
    let properties: Vec<String> = names.iter().map(|n| css_property_name(n)).collect();
    let expected = [
        ("position", "position"),
        ("display", "display"),
        ("minHeight", "min-height"),
        ("backfaceVisibility", "backface-visibility"),
        ("margin", "margin"),
        ("padding", "padding"),
        ("border", "border"),
        ("outline", "outline"),
        ("resize", "resize"),
        ("background", "background"),
        ("overflow", "overflow"),
        ("zIndex", "z-index"),
        ("wordBreak", "word-break"),
        ("whiteSpace", "white-space"),
        ("overflowWrap", "overflow-wrap"),
        ("boxSizing", "box-sizing"),
        ("font", "font"),
        ("lineHeight", "line-height"),
        ("width", "width"),
        ("height", "height"),
        ("left", "left"),
        ("top", "top"),
        ("transformOrigin", "transform-origin"),
        ("transform", "transform"),
        ("textAlign", "text-align"),
        ("verticalAlign", "vertical-align"),
        ("color", "color"),
        ("opacity", "opacity"),
    ];
    assert_eq!(names.len(), expected.len(), "{names:?}");
    for (cssom, property) in expected {
        let i = names.iter().position(|n| n == cssom).unwrap();
        assert_eq!(properties[i], property);
    }
}

#[test]
fn textarea_attributes() {
    assert_eq!(TEXTAREA_ATTRIBUTES.dir, "auto");
    assert_eq!(TEXTAREA_ATTRIBUTES.wrap, "off");
    assert_eq!(TEXTAREA_ATTRIBUTES.data_type, "wysiwyg");
    assert_eq!(TEXTAREA_ATTRIBUTES.class_name, "excalidraw-wysiwyg");
    assert_eq!(TEXTAREA_ATTRIBUTES.tab_index, 0);
    assert_eq!(
        TEXTAREA_ATTRIBUTES.container_class_name,
        "excalidraw-textEditorContainer"
    );
}

#[test]
fn stylesheet_rules() {
    assert!(TEXT_EDITOR_CSS.contains("--zIndex-wysiwyg: 3;"));
    for rule in [
        "position: absolute;",
        "top: 0;",
        "right: 0;",
        "bottom: 0;",
        "left: 0;",
        "overflow: hidden;",
        "pointer-events: none;",
    ] {
        assert!(TEXT_EDITOR_CSS.contains(rule), "{rule}");
    }
    assert!(TEXT_EDITOR_CSS
        .contains(".excalidraw-textEditorContainer > textarea {\n  pointer-events: auto;"));
}

#[test]
fn ctrl_or_cmd_is_cmd_on_apple_platforms() {
    for p in ["MacIntel", "iPhone", "iPad", "iPod touch"] {
        assert!(is_darwin(p), "{p}");
    }
    for p in ["Win32", "Linux x86_64", "Linux aarch64", ""] {
        assert!(!is_darwin(p), "{p}");
    }
}

#[test]
fn pointer_downs_outside_the_editor() {
    let t = PointerDownTarget::default;
    assert_eq!(
        classify_pointer_down(&PointerDownTarget {
            canvas: true,
            ..t()
        }),
        PointerDownAction::SubmitNextFrame
    );
    assert_eq!(classify_pointer_down(&t()), PointerDownAction::Nothing);
    // the middle button pans, from the textarea itself too
    assert_eq!(
        classify_pointer_down(&PointerDownTarget {
            button: 1,
            textarea: true,
            ..t()
        }),
        PointerDownAction::Pan { on_textarea: true }
    );
    assert_eq!(
        classify_pointer_down(&PointerDownTarget {
            button: 1,
            canvas: true,
            ..t()
        }),
        PointerDownAction::Pan { on_textarea: false }
    );
    // styles panel, popovers and zoom actions keep the editor open
    for target in [
        PointerDownTarget {
            in_actions_menu: true,
            canvas: true,
            ..t()
        },
        PointerDownTarget {
            properties_trigger: true,
            ..t()
        },
        PointerDownTarget {
            in_properties_content: true,
            writable: true,
            ..t()
        },
    ] {
        assert_eq!(
            classify_pointer_down(&target),
            PointerDownAction::SuspendSubmit,
            "{target:?}"
        );
        assert!(!rearms_on_pointer_up(&target), "{target:?}");
    }
    // a writable field in the menu (a colour's hex input) does not
    let field = PointerDownTarget {
        in_actions_menu: true,
        writable: true,
        ..t()
    };
    assert_eq!(classify_pointer_down(&field), PointerDownAction::Nothing);
    assert!(rearms_on_pointer_up(&PointerDownTarget {
        canvas: true,
        ..t()
    }));
}

#[test]
fn a_scene_update_refocuses_the_textarea_unless_a_properties_popover_has_the_focus() {
    let t = PointerDownTarget::default;
    // nothing focused (the body), a button, the canvas, the textarea itself
    for active in [
        t(),
        PointerDownTarget {
            canvas: true,
            ..t()
        },
        PointerDownTarget {
            textarea: true,
            writable: true,
            ..t()
        },
        // the styles panel and a properties trigger: the focus comes back
        // (unlike the pointer up, which keeps the blur submit suspended)
        PointerDownTarget {
            in_actions_menu: true,
            ..t()
        },
        PointerDownTarget {
            properties_trigger: true,
            ..t()
        },
    ] {
        assert!(refocuses_on_scene_update(&active), "{active:?}");
    }
    // `activeElement.closest(".properties-content")`: a popover is open
    for active in [
        PointerDownTarget {
            in_properties_content: true,
            ..t()
        },
        PointerDownTarget {
            in_properties_content: true,
            writable: true,
            ..t()
        },
        PointerDownTarget {
            in_properties_content: true,
            in_actions_menu: true,
            ..t()
        },
    ] {
        assert!(!refocuses_on_scene_update(&active), "{active:?}");
    }
}

#[test]
fn caret_boundaries_are_code_points() {
    assert_eq!(caret_boundary_offsets("ab"), vec![0, 1, 2]);
    // an astral character is two UTF-16 units and one boundary
    assert_eq!(caret_boundary_offsets("a\u{1F600}b"), vec![0, 1, 3, 4]);
    assert_eq!(caret_boundary_offsets(""), vec![0]);
}

#[test]
fn closest_caret_is_measured_from_the_line_start() {
    let offsets = [0, 1, 2, 3];
    // a line laid out at x = 100: carets at 100, 110, 120, 130
    let positions = [100.0, 110.0, 120.0, 130.0];
    assert_eq!(closest_caret_offset(&offsets, &positions, 0.0), 0);
    assert_eq!(closest_caret_offset(&offsets, &positions, 14.0), 1);
    assert_eq!(closest_caret_offset(&offsets, &positions, 16.0), 2);
    // a tie goes to the first
    assert_eq!(closest_caret_offset(&offsets, &positions, 15.0), 1);
    assert_eq!(closest_caret_offset(&offsets, &positions, 500.0), 3);
    // right to left: the leftmost caret is the line's end
    let rtl = [130.0, 120.0, 110.0, 100.0];
    assert_eq!(closest_caret_offset(&offsets, &rtl, 0.0), 3);
    assert_eq!(closest_caret_offset(&offsets, &rtl, 29.0), 0);
}

// ---------------------------------------------------------------------------
// the app side: textarea events to the editor

use excali_core::app_state::AppState;
use excali_core::element::Element;
use excali_core::fractional_index::{ChangeStamp, SceneElementsMap};
use excali_editor::session::Session;
use excali_editor::store::HistoryEnv;
use excali_editor::text_editing::{
    start_text_editing, NoHost, StartTextEditing, TextEditingContext, TextTarget,
};
use excali_editor::text_layout::TextLayouter;
use excali_text::text_measurements::TextMetricsProvider;
use excali_ui::text_editor::{
    Handled, TextEditingApp, TextareaEvent, TextareaHandler, TextareaKey, TextareaState,
};

struct TenPx;

impl TextMetricsProvider for TenPx {
    fn get_line_width(&self, text: &str, _: &str) -> f64 {
        text.encode_utf16().count() as f64 * 10.0
    }
}

#[derive(Default)]
struct Env(f64);

impl ChangeStamp for Env {
    fn version_nonce(&mut self) -> f64 {
        self.0 += 1.0;
        self.0
    }

    fn updated(&mut self) -> f64 {
        1.0
    }
}

impl HistoryEnv for Env {
    fn random_id(&mut self) -> String {
        format!("id{}", self.0)
    }

    fn redraw_text_bounding_box(
        &mut self,
        _: &mut SceneElementsMap,
        _: &str,
        _: &str,
    ) -> Result<(), String> {
        Ok(())
    }

    fn update_bound_elements(
        &mut self,
        _: &mut SceneElementsMap,
        _: &str,
        _: &SceneElementsMap,
    ) -> Result<(), String> {
        Ok(())
    }
}

fn app_on(case: &str) -> TextEditingApp<Env, TenPx, NoHost> {
    let f = fixture();
    let case = f["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == case)
        .unwrap()
        .clone();
    let elements: Vec<Element> = case["initial"]["elements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            let mut m = v.as_object().unwrap().clone();
            m.insert("seed".into(), 1.into());
            m.insert("versionNonce".into(), 0.into());
            m.insert("updated".into(), 1.into());
            Element::from_map(m).unwrap()
        })
        .collect();
    let mut session = Session::new(Env::default(), AppState::default());
    session
        .initialize_scene(elements, serde_json::Map::new())
        .unwrap();
    let mut app = TextEditingApp::new(session, TextLayouter::new(TenPx), NoHost);
    let mut args = StartTextEditing::at(110.0, 110.0);
    args.text_element = TextTarget::Existing("t".into());
    let editor = start_text_editing(
        &mut TextEditingContext {
            session: &mut app.session,
            layouter: &mut app.layouter,
            host: &mut app.host,
        },
        &args,
    )
    .unwrap();
    app.editor = editor;
    app
}

fn key(key: &str, code: &str) -> TextareaKey {
    TextareaKey {
        key: key.into(),
        code: code.into(),
        shift_key: false,
        alt_key: false,
        ctrl_or_cmd: false,
        is_composing: false,
        key_code: 0,
    }
}

fn state(h: &Handled) -> TextareaState {
    h.state.clone().unwrap()
}

#[test]
fn textarea_events_drive_the_editor() {
    let mut app = app_on("free-text-typed");

    let h = app.on_event(TextareaEvent::Focused);
    assert_eq!(state(&h).selection, (0, 5));

    // Tab indents the line the caret is on (the selection is synced first)
    let h = app.on_event(TextareaEvent::KeyDown {
        key: key("Tab", "Tab"),
        selection: (2, 2),
    });
    assert!(h.prevent_default);
    assert_eq!(state(&h).value, "    hello");
    assert_eq!(state(&h).selection, (6, 6));

    // a printable key is the browser's; its input writes the text
    let h = app.on_event(TextareaEvent::KeyDown {
        key: key("x", "KeyX"),
        selection: (6, 6),
    });
    assert!(!h.prevent_default);
    let h = app.on_event(TextareaEvent::Input {
        value: "    hexllo".into(),
        selection: (7, 7),
    });
    assert_eq!(state(&h).value, "    hexllo");
    assert!(state(&h).open);
    let text = app
        .session
        .elements()
        .iter()
        .find(|e| e.base.id == "t")
        .unwrap();
    assert_eq!(text.base.width, 100.0);

    // a middle-button pan is the app's
    let h = app.on_event(TextareaEvent::PanStart {
        client_x: 1.0,
        client_y: 2.0,
    });
    assert_eq!(h.state, None);

    // Escape submits: the textarea goes, the text is selected
    let h = app.on_event(TextareaEvent::KeyDown {
        key: key("Escape", "Escape"),
        selection: (7, 7),
    });
    assert!(h.prevent_default);
    assert!(!state(&h).open);
    assert_eq!(
        app.calls,
        vec!["panStart", "scheduleCapture", "focusContainer"]
    );
    assert_eq!(
        app.session.app_state().get("selectedElementIds"),
        Some(&serde_json::json!({"t": true}))
    );
    assert!(app.errors.is_empty());

    // a closed editor ignores what comes after
    let h = app.on_event(TextareaEvent::Submit);
    assert!(!state(&h).open);
    assert_eq!(app.calls.len(), 3);
}
