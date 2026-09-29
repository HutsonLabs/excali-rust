//! Text editing (ex-512) against upstream: the text editor overlay
//! (`packages/excalidraw/wysiwyg/textWysiwyg.tsx`), `App.startTextEditing`
//! and `handleTextWysiwyg`, and `redrawTextBoundingBox`
//! (`packages/element/src/textElement.ts:51-152`, with the sticky note fit
//! and arrow labels).
//!
//! Fixture: `tests/fixtures/text-editing.json`, upstream's own code at the
//! pinned commit (`tools/goldens/text-editing.mjs`): the App members cut out
//! of `App.tsx` over upstream's Scene, run under jsdom with the browser's
//! part (timers, key events and their default edits) replayed step by
//! step. Each session here replays the same steps through
//! [`excali_editor::text_editing`] and holds, after the start and after
//! every step, the textarea (value, selection, every style value
//! assigned), every element (all fields but the drawn `seed`,
//! `versionNonce` and `updated`), the app state, what the editor asked of
//! the app and the original container cache to upstream's.
//!
//! Text measures 10 px per UTF-16 code unit, as in upstream's tests.

use std::collections::VecDeque;

use excali_core::app_state::AppState;
use excali_core::element::Element;
use excali_core::fractional_index::{ChangeStamp, SceneElementsMap};
use excali_editor::mutate::mutate_element;
use excali_editor::session::Session;
use excali_editor::store::HistoryEnv;
use excali_editor::text_editing::{
    get_transform, max_text_width, start_text_editing, KeyDown, PasteOutcome, StartTextEditing,
    TextEditingContext, TextEditingHost, TextEditor, TextTarget, CARET_FOLLOW_PADDING,
    DEFAULT_BOUND_TEXT_LABEL_POSITION, TEXTAREA_ATTRIBUTES, TEXT_TO_CENTER_SNAP_THRESHOLD,
    TEXT_VIEWPORT_PADDING,
};
use excali_editor::text_layout::{
    get_position_after_height_change, normalize_sticky_note_font_size, OriginalContainerCache,
    TextLayouter, VerticalAnchor,
};
use excali_text::text_measurements::CharCountTextMetrics;
use serde_json::{json, Map, Value};

const NOW: f64 = 1_700_000_000_000.0;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/text-editing.json")).unwrap()
}

/// Upstream's environment in the generator: element ids from a counter
/// (`id0`, `id1`, ... in the order `randomId()` handed them out), the
/// clock fixed. Nonces and seeds are drawn and not compared.
struct Env {
    ids: VecDeque<String>,
    deltas: u32,
    nonce: f64,
}

impl ChangeStamp for Env {
    fn version_nonce(&mut self) -> f64 {
        self.nonce += 1.0;
        self.nonce
    }

    fn updated(&mut self) -> f64 {
        NOW
    }
}

impl HistoryEnv for Env {
    fn random_id(&mut self) -> String {
        self.ids.pop_front().unwrap_or_else(|| {
            self.deltas += 1;
            format!("delta-{}", self.deltas)
        })
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

/// What the case's stand-in App answered: the sidebar's insets and the
/// hit tests.
struct Host {
    sidebar: (f64, f64),
    hit_text: Option<String>,
    hit_frame: Option<String>,
}

impl TextEditingHost for Host {
    fn sidebar_insets(&self) -> (f64, f64) {
        self.sidebar
    }

    fn text_element_at(&self, _: &[Element], _: f64, _: f64) -> Option<String> {
        self.hit_text.clone()
    }

    fn top_layer_frame_at(&self, _: &[Element], _: f64, _: f64) -> Option<String> {
        self.hit_frame.clone()
    }
}

/// An element of the fixture, with the drawn keys filled in.
fn element(value: &Value) -> Element {
    let mut map = value.as_object().unwrap().clone();
    map.insert("seed".into(), json!(1));
    map.insert("versionNonce".into(), json!(0));
    map.insert("updated".into(), json!(NOW));
    Element::from_map(map).unwrap()
}

/// An element as the fixture holds it: without the drawn keys.
fn element_out(element: &Element) -> Map<String, Value> {
    let mut map = element.to_map();
    for key in ["seed", "versionNonce", "updated"] {
        map.shift_remove(key);
    }
    map
}

/// JSON equality with numbers compared as doubles, keys in any order.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

fn assert_same(context: &str, got: &Value, want: &Value) {
    if !same(got, want) {
        let (Value::Object(g), Value::Object(w)) = (got, want) else {
            panic!("{context}:\n got {got}\nwant {want}");
        };
        let mut diffs = Vec::new();
        for (k, v) in w {
            match g.get(k) {
                Some(x) if same(x, v) => {}
                other => diffs.push(format!("{k}: got {other:?}, want {v}")),
            }
        }
        for k in g.keys().filter(|k| !w.contains_key(*k)) {
            diffs.push(format!("{k}: got {:?}, want nothing", g[k]));
        }
        panic!("{context}:\n  {}", diffs.join("\n  "));
    }
}

// ---------------------------------------------------------------------------
// constants and pure rules

#[test]
fn constants_are_upstreams() {
    let c = &fixture()["constants"];
    let n = |key: &str| c[key].as_f64().unwrap();
    assert_eq!(n("TEXT_VIEWPORT_PADDING"), TEXT_VIEWPORT_PADDING);
    assert_eq!(
        n("TEXT_TO_CENTER_SNAP_THRESHOLD"),
        TEXT_TO_CENTER_SNAP_THRESHOLD
    );
    assert_eq!(n("CARET_FOLLOW_PADDING"), CARET_FOLLOW_PADDING);
    assert_eq!(
        n("DEFAULT_BOUND_TEXT_LABEL_POSITION"),
        DEFAULT_BOUND_TEXT_LABEL_POSITION
    );
}

#[test]
fn the_textarea_is_upstreams() {
    for case in fixture()["sessions"].as_array().unwrap() {
        let a = &case["attributes"];
        let name = case["name"].as_str().unwrap();
        assert_eq!(a["tagName"], "textarea", "{name}");
        assert_eq!(a["dir"], TEXTAREA_ATTRIBUTES.dir, "{name}");
        assert_eq!(a["wrap"], TEXTAREA_ATTRIBUTES.wrap, "{name}");
        assert_eq!(
            a["tabIndex"],
            json!(TEXTAREA_ATTRIBUTES.tab_index),
            "{name}"
        );
        assert_eq!(a["dataType"], TEXTAREA_ATTRIBUTES.data_type, "{name}");
        assert_eq!(a["className"], TEXTAREA_ATTRIBUTES.class_name, "{name}");
        assert_eq!(
            a["parentClassName"], TEXTAREA_ATTRIBUTES.container_class_name,
            "{name}"
        );
    }
}

/// `getTransform`: `translate(w(z-1)/2, h(z-1)/2) scale(z) rotate(deg)`.
#[test]
fn transform_formula() {
    assert_eq!(
        get_transform(60.0, 25.0, 0.3, 1.5),
        "translate(15px, 6.25px) scale(1.5) rotate(17.188733853924695deg)"
    );
    assert_eq!(
        get_transform(70.0, 25.0, 0.3, 1.6),
        "translate(21.000000000000004px, 7.500000000000001px) scale(1.6) rotate(17.188733853924695deg)"
    );
    assert_eq!(
        get_transform(50.0, 25.0, 0.0, 1.0),
        "translate(0px, 0px) scale(1) rotate(0deg)"
    );
}

#[test]
fn max_text_width_is_the_view_less_the_padding_over_the_zoom() {
    let mut state = AppState::default();
    state.insert("width", json!(300));
    state.insert("zoom", json!({"value": 2}));
    assert_eq!(
        max_text_width(&state, (0.0, 40.0)),
        (300.0 - 20.0 - 60.0) / 2.0
    );
    state.insert("width", json!(30));
    assert_eq!(max_text_width(&state, (0.0, 0.0)), f64::INFINITY);
}

#[test]
fn position_after_height_change_holds_the_anchor() {
    let mut e = element(&json!({
        "id": "s", "type": "rectangle", "x": 10, "y": 20, "width": 100, "height": 100,
        "angle": 0, "strokeColor": "#000", "backgroundColor": "transparent",
        "fillStyle": "solid", "strokeWidth": 1, "strokeStyle": "solid", "roughness": 1,
        "opacity": 100, "groupIds": [], "frameId": null, "roundness": null, "version": 1,
        "isDeleted": false, "boundElements": null, "link": null, "locked": false
    }));
    assert_eq!(
        get_position_after_height_change(&e, 140.0, VerticalAnchor::Top),
        [10.0, 20.0]
    );
    assert_eq!(
        get_position_after_height_change(&e, 140.0, VerticalAnchor::Bottom),
        [10.0, -20.0]
    );
    assert_eq!(
        get_position_after_height_change(&e, 140.0, VerticalAnchor::Center),
        [10.0, 0.0]
    );
    e.base.angle.0 = std::f64::consts::PI;
    let [x, y] = get_position_after_height_change(&e, 140.0, VerticalAnchor::Top);
    assert!(
        (x - 10.0).abs() < 1e-9 && (y - -20.0).abs() < 1e-9,
        "{x} {y}"
    );
}

#[test]
fn sticky_note_font_ceiling_is_clamped() {
    assert_eq!(normalize_sticky_note_font_size(f64::NAN), 28.0);
    assert_eq!(normalize_sticky_note_font_size(f64::INFINITY), 28.0);
    assert_eq!(normalize_sticky_note_font_size(0.5), 1.0);
    assert_eq!(normalize_sticky_note_font_size(1e20), 512.0);
    assert_eq!(normalize_sticky_note_font_size(36.0), 36.0);
}

#[test]
fn original_container_cache() {
    let mut cache = OriginalContainerCache::new();
    assert_eq!(cache.get("r"), None);
    assert_eq!(cache.update("r", 80.0), 80.0);
    assert_eq!(cache.update("r", 85.0), 85.0);
    assert_eq!(cache.get("r"), Some(85.0));
    assert_eq!(cache.to_json(), json!({"r": {"height": 85}}));
    cache.reset("r");
    assert_eq!(cache.get("r"), None);
}

// ---------------------------------------------------------------------------
// redrawTextBoundingBox

struct Stamp(f64);

impl ChangeStamp for Stamp {
    fn version_nonce(&mut self) -> f64 {
        self.0 += 1.0;
        self.0
    }

    fn updated(&mut self) -> f64 {
        NOW
    }
}

#[test]
fn redraw_text_bounding_box_matches_upstream() {
    let cases = fixture()["redraw"].as_array().unwrap().clone();
    assert!(cases.len() >= 15);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let mut elements: SceneElementsMap = case["elements"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                let e = element(v);
                (e.base.id.clone(), e)
            })
            .collect();
        let before = elements.clone();
        let mut layouter = TextLayouter::new(CharCountTextMetrics);
        let mut bound_updates = Vec::new();
        layouter
            .redraw_text_bounding_box(
                &mut Stamp(0.0),
                &mut elements,
                case["text"].as_str().unwrap(),
                case["container"].as_str(),
                &mut |_, _, id| {
                    bound_updates.push(id.to_owned());
                    Ok(())
                },
            )
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let changed: Vec<&Value> = case["changed"].as_array().unwrap().iter().collect();
        for (id, e) in &elements {
            let want = changed.iter().find(|c| c["id"] == json!(id));
            match want {
                Some(want) => assert_same(
                    &format!("{name}: {id}"),
                    &Value::Object(element_out(e)),
                    want,
                ),
                None => assert_eq!(e, &before[id], "{name}: {id} changed"),
            }
        }
        assert_same(
            &format!("{name}: container cache"),
            &layouter.container_cache.to_json(),
            &case["containerCache"],
        );
        // the arrows bound to a sticky note follow it
        let sticky = case["elements"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["type"] == "stickynote");
        assert_eq!(bound_updates.len(), usize::from(sticky), "{name}");
    }
}

// ---------------------------------------------------------------------------
// editing sessions

/// The app state a record lists, as `stateOut` writes it.
fn state_out(state: &AppState, keys: &[Value]) -> Map<String, Value> {
    let mut out = Map::new();
    for key in keys {
        let key = key.as_str().unwrap();
        let value = state.get(key).cloned().unwrap_or(Value::Null);
        let value = match key {
            "zoom" => value.get("value").cloned().unwrap_or(json!(1)),
            "editingTextElement" | "newElement" | "multiElement" => {
                value.get("id").cloned().unwrap_or(Value::Null)
            }
            _ => value,
        };
        out.insert(key.to_owned(), value);
    }
    out
}

/// The app state a session starts from: the recorded keys, the zoom as
/// `{ value }`.
fn initial_state(state: &Map<String, Value>) -> Map<String, Value> {
    let mut out = Map::new();
    for (key, value) in state {
        let value = match key.as_str() {
            "zoom" => json!({ "value": value }),
            _ => value.clone(),
        };
        out.insert(key.clone(), value);
    }
    out
}

/// `startTextEditing`'s arguments for a case.
fn start_args(case: &Value) -> StartTextEditing {
    let start = &case["start"];
    let mut args = StartTextEditing::at(
        start["sceneX"].as_f64().unwrap(),
        start["sceneY"].as_f64().unwrap(),
    );
    args.container = start["container"].as_str().map(str::to_owned);
    if let Some(id) = start["textElement"].as_str() {
        args.text_element = TextTarget::Existing(id.to_owned());
    }
    if let Some(caret) = start["initialCaretSceneCoords"].as_object() {
        args.initial_caret = Some([caret["x"].as_f64().unwrap(), caret["y"].as_f64().unwrap()]);
    }
    args
}

/// The keydown a US keyboard sends for a typed character.
fn char_event(ch: char) -> (String, String, bool) {
    let code = if ch.is_ascii_alphabetic() {
        format!("Key{}", ch.to_ascii_uppercase())
    } else if ch.is_ascii_digit() {
        format!("Digit{ch}")
    } else {
        match ch {
            ' ' => "Space",
            '=' => "Equal",
            '-' => "Minus",
            '.' => "Period",
            ',' => "Comma",
            '[' => "BracketLeft",
            ']' => "BracketRight",
            _ => "",
        }
        .to_owned()
    };
    (ch.to_string(), code, ch.is_ascii_uppercase())
}

/// "CtrlOrCmd+Shift+Period" → key, code, shift, alt, ctrl/cmd.
fn key_event(spec: &str) -> (String, String, bool, bool, bool) {
    let parts: Vec<&str> = spec.split('+').collect();
    let (code, mods) = parts.split_last().unwrap();
    let shift = mods.contains(&"Shift");
    let keys = |plain: &str, shifted: &str| if shift { shifted } else { plain }.to_owned();
    let key = match *code {
        "Enter" | "Escape" | "Tab" | "Backspace" | "Delete" => (*code).to_owned(),
        "Equal" => keys("=", "+"),
        "Minus" => keys("-", "_"),
        "Digit0" => keys("0", ")"),
        "Period" => keys(".", ">"),
        "Comma" => keys(",", "<"),
        "BracketLeft" => keys("[", "{"),
        "BracketRight" => keys("]", "}"),
        "KeyS" => keys("s", "S"),
        other => panic!("no key {other}"),
    };
    (
        key,
        (*code).to_owned(),
        shift,
        mods.contains(&"Alt"),
        mods.contains(&"CtrlOrCmd"),
    )
}

fn len16(s: &str) -> usize {
    s.encode_utf16().count()
}

fn byte_at(s: &str, i: usize) -> usize {
    s.char_indices()
        .scan(0, |units, (b, c)| {
            let at = *units;
            *units += c.len_utf16();
            Some((at, b))
        })
        .find(|(at, _)| *at >= i)
        .map_or(s.len(), |(_, b)| b)
}

/// The browser replacing the selection with `text`, the caret after it.
fn insert(value: &str, selection: (usize, usize), text: &str) -> (String, (usize, usize)) {
    let (a, b) = (byte_at(value, selection.0), byte_at(value, selection.1));
    let next = format!("{}{}{}", &value[..a], text, &value[b..]);
    let caret = selection.0 + len16(text);
    (next, (caret, caret))
}

/// The browser's Backspace (-1) and Delete (1) on ASCII text.
fn delete_by(
    value: &str,
    (start, end): (usize, usize),
    direction: i32,
) -> Option<(String, (usize, usize))> {
    if start != end {
        return Some(insert(value, (start, end), ""));
    }
    if direction < 0 && start > 0 {
        return Some(insert(value, (start - 1, end), ""));
    }
    if direction > 0 && end < len16(value) {
        let (next, _) = insert(value, (start, end + 1), "");
        return Some((next, (start, start)));
    }
    None
}

struct Replay {
    session: Session<Env>,
    layouter: TextLayouter<CharCountTextMetrics>,
    host: Host,
    editor: Option<TextEditor>,
}

impl Replay {
    fn ctx(&mut self) -> TextEditingContext<'_, Env, CharCountTextMetrics> {
        TextEditingContext {
            session: &mut self.session,
            layouter: &mut self.layouter,
            host: &mut self.host,
        }
    }

    /// A keydown as the browser delivers it: to the editor, then its
    /// default edit and `input` unless prevented or the editor closed.
    fn keydown(&mut self, key: &str, code: &str, shift: bool, alt: bool, ctrl_or_cmd: bool) {
        let mut editor = self.editor.take().expect("an open editor");
        let outcome = editor
            .keydown(
                &mut self.ctx(),
                &KeyDown {
                    key,
                    code,
                    shift_key: shift,
                    alt_key: alt,
                    ctrl_or_cmd,
                    is_composing: false,
                    key_code: 0,
                },
            )
            .unwrap();
        if !outcome.prevent_default && editor.is_open() && !ctrl_or_cmd && !alt {
            let (value, selection) = (editor.value().to_owned(), editor.selection());
            let edit = match key {
                "Enter" => Some(insert(&value, selection, "\n")),
                "Backspace" => delete_by(&value, selection, -1),
                "Delete" => delete_by(&value, selection, 1),
                k if k.chars().count() == 1 => Some(insert(&value, selection, k)),
                _ => None,
            };
            if let Some((value, selection)) = edit {
                editor.input(&mut self.ctx(), &value, selection).unwrap();
            }
        }
        self.editor = Some(editor);
    }

    fn step(&mut self, step: &Value) {
        if let Some(text) = step["type"].as_str() {
            for ch in text.chars() {
                let (key, code, shift) = char_event(ch);
                self.keydown(&key, &code, shift, false, false);
            }
        } else if let Some(spec) = step["press"].as_str() {
            let (key, code, shift, alt, ctrl) = key_event(spec);
            self.keydown(&key, &code, shift, alt, ctrl);
        } else if let Some(text) = step["insertText"].as_str() {
            let mut editor = self.editor.take().unwrap();
            let (value, selection) = insert(editor.value(), editor.selection(), text);
            editor.input(&mut self.ctx(), &value, selection).unwrap();
            self.editor = Some(editor);
        } else if let Some(select) = step["select"].as_array() {
            let editor = self.editor.as_mut().unwrap();
            editor.set_selection(
                select[0].as_u64().unwrap() as usize,
                select[1].as_u64().unwrap() as usize,
            );
        } else if step["blur"] == json!(true) {
            let mut editor = self.editor.take().unwrap();
            editor.submit(&mut self.ctx()).unwrap();
            self.editor = Some(editor);
        } else if let Some(paste) = step["paste"].as_object() {
            let types: Vec<&str> = paste["types"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t.as_str().unwrap())
                .collect();
            let text = paste["text"].as_str().unwrap();
            let mut editor = self.editor.take().unwrap();
            let outcome = editor.paste(&mut self.ctx(), &types, Some(text)).unwrap();
            // the browser pastes, after the handler (and its microtasks)
            if outcome == PasteOutcome::Browser && !text.is_empty() {
                let (value, selection) = insert(editor.value(), editor.selection(), text);
                editor.input(&mut self.ctx(), &value, selection).unwrap();
            }
            self.editor = Some(editor);
        } else if let Some(scroll) = step["boxScroll"].as_array() {
            let mut editor = self.editor.take().unwrap();
            editor
                .editor_box_scrolled(
                    &mut self.ctx(),
                    scroll[0].as_f64().unwrap(),
                    scroll[1].as_f64().unwrap(),
                )
                .unwrap();
            self.editor = Some(editor);
        } else if let Some(theme) = step["theme"].as_str() {
            self.session.set_state(obj(json!({ "theme": theme })));
            let mut editor = self.editor.take().unwrap();
            editor.app_changed(&mut self.ctx()).unwrap();
            self.editor = Some(editor);
        } else if let Some(size) = step["resize"].as_array() {
            self.session
                .set_state(obj(json!({ "width": size[0], "height": size[1] })));
            let mut editor = self.editor.take().unwrap();
            editor.relayout(&mut self.ctx()).unwrap();
            self.editor = Some(editor);
        } else if let Some(mutate) = step["mutate"].as_array() {
            let id = mutate[0].as_str().unwrap().to_owned();
            let updates = mutate[1].as_object().unwrap().clone();
            self.session.edit_elements(|map, env| {
                let mut e = map[&id].clone();
                mutate_element(&mut e, map, updates, env).unwrap();
                map.insert(id.clone(), e);
            });
            let mut editor = self.editor.take().unwrap();
            editor.relayout(&mut self.ctx()).unwrap();
            self.editor = Some(editor);
        } else {
            panic!("unknown step {step}");
        }
    }
}

impl Replay {
    /// A session on the case's scene and app state, and `startTextEditing`
    /// with the case's arguments; a caret the case asks for is measured at
    /// the start of its line (jsdom has no layout: every caret position
    /// measures 0), and the editor focused (`bindBlurEvent`'s timeout).
    fn start(case: &Value) -> Replay {
        let mut replay = Replay::new(case);
        let name = case["name"].as_str().unwrap();
        let args = start_args(case);
        replay.editor =
            start_text_editing(&mut replay.ctx(), &args).unwrap_or_else(|e| panic!("{name}: {e}"));
        if let Some(editor) = replay.editor.as_mut() {
            if editor.caret_request().is_some() {
                editor.resolve_caret(Some(0));
            }
            editor.focused();
        }
        replay
    }

    /// The case's scene and app state, and its host, with no editor yet.
    fn new(case: &Value) -> Replay {
        let initial = &case["initial"];
        let ids: VecDeque<String> = case["ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        let env = Env {
            ids,
            deltas: 0,
            nonce: 0.0,
        };
        let elements: Vec<Element> = initial["elements"]
            .as_array()
            .unwrap()
            .iter()
            .map(element)
            .collect();
        let mut session = Session::new(env, AppState::default());
        session
            .initialize_scene(
                elements,
                initial_state(initial["state"].as_object().unwrap()),
            )
            .unwrap();
        let sidebar = &case["sidebar"];
        let replay = Replay {
            session,
            layouter: TextLayouter::new(CharCountTextMetrics),
            host: Host {
                sidebar: (
                    sidebar["left"].as_f64().unwrap(),
                    sidebar["right"].as_f64().unwrap(),
                ),
                hit_text: case["hitText"].as_str().map(str::to_owned),
                hit_frame: case["hitFrame"].as_str().map(str::to_owned),
            },
            editor: None,
        };

        replay
    }
}

#[test]
fn editing_sessions_match_upstream() {
    let f = fixture();
    let keys = f["stateKeys"].as_array().unwrap().clone();
    let sessions = f["sessions"].as_array().unwrap().clone();
    assert!(sessions.len() >= 25);
    for case in sessions {
        let name = case["name"].as_str().unwrap();
        let initial = &case["initial"];
        let mut replay = Replay::start(&case);
        let mut scene: Vec<Map<String, Value>> = initial["elements"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e.as_object().unwrap().clone())
            .collect();
        let mut state = initial["state"].as_object().unwrap().clone();
        let mut style = Map::new();
        for (i, record) in case["records"].as_array().unwrap().iter().enumerate() {
            if i > 0 {
                replay.step(&record["step"]);
            }
            let at = format!("{name}, record {i} ({})", record["step"]);
            // what upstream recorded, applied to what came before
            for changed in record["changed"].as_array().unwrap() {
                let id = &changed["id"];
                match scene.iter_mut().find(|e| e["id"] == *id) {
                    Some(e) => *e = changed.as_object().unwrap().clone(),
                    None => scene.push(changed.as_object().unwrap().clone()),
                }
            }
            if let Some(order) = record["order"].as_array() {
                scene.sort_by_key(|e| order.iter().position(|id| *id == e["id"]));
            }
            for (k, v) in record["state"].as_object().unwrap() {
                state.insert(k.clone(), v.clone());
            }
            for (k, v) in record["style"].as_object().unwrap() {
                style.insert(k.clone(), v.clone());
            }

            let editor = replay.editor.as_mut().expect("an editor");
            assert_eq!(
                editor.is_open(),
                record["open"] == json!(true),
                "{at}: open"
            );
            assert_eq!(json!(editor.value()), record["value"], "{at}: value");
            let (s, e) = editor.selection();
            assert_eq!(json!([s, e]), record["selection"], "{at}: selection");
            let got_style: Map<String, Value> = editor
                .style()
                .iter()
                .map(|(k, v)| (k.clone(), json!(v)))
                .collect();
            assert_same(
                &format!("{at}: style"),
                &Value::Object(got_style),
                &Value::Object(style.clone()),
            );
            let calls: Vec<String> = editor.take_calls().iter().map(|c| c.name()).collect();
            assert_eq!(json!(calls), record["calls"], "{at}: calls");

            let got_ids: Vec<&str> = replay
                .session
                .elements()
                .iter()
                .map(|e| e.base.id.as_str())
                .collect();
            let want_ids: Vec<&str> = scene.iter().map(|e| e["id"].as_str().unwrap()).collect();
            assert_eq!(got_ids, want_ids, "{at}: scene order");
            for (got, want) in replay.session.elements().iter().zip(&scene) {
                assert_same(
                    &format!("{at}: element {}", got.base.id),
                    &Value::Object(element_out(got)),
                    &Value::Object(want.clone()),
                );
            }
            assert_same(
                &format!("{at}: app state"),
                &Value::Object(state_out(replay.session.app_state(), &keys)),
                &Value::Object(state.clone()),
            );
            assert_same(
                &format!("{at}: container cache"),
                &replay.layouter.container_cache.to_json(),
                &record["containerCache"],
            );
        }
    }
}

fn obj(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

/// The fixture's session named `name`.
fn case(name: &str) -> Value {
    fixture()["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("no session {name}"))
        .clone()
}

/// A paste into a label widens the textarea to the wrapped result before
/// the browser pastes (`textWysiwyg.tsx:652-682`); the paste's input then
/// restyles it, which is all the fixture sees.
#[test]
fn a_paste_into_a_label_widens_the_editor_first() {
    let mut replay = Replay::start(&case("plain-text-pasted-into-a-label"));
    let mut editor = replay.editor.take().unwrap();
    editor.set_selection(5, 5);
    let outcome = editor
        .paste(
            &mut replay.ctx(),
            &["text/plain"],
            Some(" pasted\twith a tab"),
        )
        .unwrap();
    assert_eq!(outcome, PasteOutcome::Browser);
    // "label pasted        with a tab" wrapped at 150: "label pasted" is
    // the widest line
    assert_eq!(editor.style()["width"], "120px");
    // a free text is not widened
    let mut replay = Replay::start(&case("plain-text-pasted-into-a-free-text"));
    let mut editor = replay.editor.take().unwrap();
    let width = editor.style()["width"].clone();
    editor
        .paste(
            &mut replay.ctx(),
            &["text/plain"],
            Some("a long pasted line"),
        )
        .unwrap();
    assert_eq!(editor.style()["width"], width);
    // excalidraw data without text pastes nothing and prevents the paste
    let outcome = editor
        .paste(
            &mut replay.ctx(),
            &["application/vnd.excalidraw+json", "text/plain"],
            Some("{}"),
        )
        .unwrap();
    assert_eq!(outcome, PasteOutcome::Prevented);
}

/// `getCaretIndexFromInitialSceneCoords` up to the page's measurement: the
/// wrapped line under the point (clamped to the text's lines) and the
/// point's distance from where that line starts, in the line's direction.
#[test]
fn caret_request_names_the_line_under_the_point() {
    let c = case("caret-placed-at-the-point");
    let mut replay = Replay::new(&c);
    let editor = start_text_editing(&mut replay.ctx(), &start_args(&c))
        .unwrap()
        .unwrap();
    let request = editor.caret_request().unwrap();
    assert_eq!(request.line_text, "two");
    assert_eq!(request.line_start, 4);
    assert_eq!(request.target_x, 12.0);
    assert_eq!(request.direction, "ltr");
    assert_eq!(request.line_height_px, 25.0);
    assert_eq!(
        request.font,
        "20px Excalifont, Xiaolai, sans-serif, Segoe UI Emoji"
    );
    // no text is selected while the caret waits to be placed
    assert_eq!(editor.selection(), (13, 13));

    // a rotated, centred label: the point turned back about its centre
    let c = case("caret-placed-in-a-rotated-label");
    let mut replay = Replay::new(&c);
    let mut editor = start_text_editing(&mut replay.ctx(), &start_args(&c))
        .unwrap()
        .unwrap();
    let request = editor.caret_request().unwrap().clone();
    assert_eq!(request.line_text, "turn");
    assert_eq!(request.line_start, 0);
    // the page measured 3 code units in
    editor.resolve_caret(Some(3));
    assert_eq!(editor.caret_request(), None);
    editor.focused();
    assert_eq!(editor.selection(), (3, 3));
}

#[test]
fn caret_request_reads_the_hard_line_direction() {
    let c = case("caret-placed-at-the-point");
    let mut replay = Replay::new(&c);
    replay.session.edit_elements(|map, env| {
        let mut t = map["t"].clone();
        mutate_element(
            &mut t,
            map,
            obj(json!({"originalText": "one\n\u{5e9}\u{5dc}\u{5d5}\n", "text": "one\n\u{5e9}\u{5dc}\u{5d5}\n"})),
            env,
        )
        .unwrap();
        map.insert("t".into(), t);
    });
    let editor = start_text_editing(&mut replay.ctx(), &start_args(&c))
        .unwrap()
        .unwrap();
    assert_eq!(editor.caret_request().unwrap().direction, "rtl");
}
