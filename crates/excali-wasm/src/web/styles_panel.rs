//! LayerUI's selected shape actions in the host (`LayerUI.tsx:249-297`):
//! the full styles panel under the main menu while
//! `showSelectedShapeActions` holds, each control its action's
//! `PanelComponent` (`excali_ui::action_panels`), whose `updateData(value)`
//! runs the action's perform on the editor
//! ([`crate::editor::Editor::perform_style_action`], the buttons through
//! [`crate::editor::Editor::perform_action`]).
//!
//! The stateful pickers are hosted here with their own state, which
//! upstream keeps in atoms and component state:
//!
//! - the colour pickers (`ColorPicker.tsx`): the active section
//!   (`activeColorPickerSectionAtom`), the hex input's state
//!   (`ColorInput`), the most used custom colours sampled as the popup
//!   opens, the eye dropper (`activeEyeDropperAtom`, `EyeDropper.tsx`,
//!   LayerUI's `onChange` and `onSelect`, `LayerUI.tsx:515-575`), the top
//!   picks' drag and drop and context menu; a pick, a hotkey, the hex
//!   input and the eye dropper's release each run the colour action's
//!   perform with `{ color }` (one history entry), the top picks'
//!   customisation with `{ colorTopPicks }`;
//! - the font picker (`actionChangeFontFamily`'s panel,
//!   `actionProperties.tsx:1361-1540`): the list's search, the selection
//!   cached as the popup opens (`cachedElementsRef`), which a hover
//!   previews over and a leave or close resets to;
//! - the arrowheads' IconPicker (its open picker and "more options").
//!
//! A press outside an open popup closes it (radix's
//! `onPointerDownOutside`, then `onClose`), unless the eye dropper is on.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use excali_core::element::{ElementKind, FontFamily};
use excali_editor::actions::{
    show_selected_shape_actions, ActionContext, ActionName, ColorProperty,
};
use excali_scene::shape::Theme;
use excali_ui::action_panels::{
    bucket_fill_color_panel, font_family_panel, render_action_panel, ActionPanelOptions,
    IconPickerState, OnActionUpdate,
};
use excali_ui::color_picker::{
    change_hex_input, color_picker, color_picker_dnd, color_top_picks_update, escape,
    initial_section, picker_custom_colors, toggle_eye_dropper, toggle_popup,
    top_pick_follows_focus, ColorPickerDnd, ColorPickerEvent, ColorPickerProps, ColorPickerType,
    EscapeOutcome, EyeDropper, EyeDropperProps, EyeDropperState, HexInputState, KeyNavEffect,
    OnColorPickerEvent, Section, StylesPanelMode,
};
use excali_ui::dom::{mount_deferred, Mounted, Node, PendingHooks};
use excali_ui::font_picker::{
    font_picker, font_picker_dnd, FontListContext, FontPickerDnd, FontPickerEvent, FontPickerProps,
    FontPickerState, OnFontPickerEvent,
};
use excali_ui::layers::Layer;
use excali_ui::styles_panel::{
    color_action_panel, legend_text, selected_shape_actions, shape_actions_section,
    ColorActionPanel,
};
use serde_json::{json, Map, Value};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Event, HtmlInputElement};

use super::{document_rtl, is_darwin, refresh_chrome, set_timeout, Inner};

/// The styles panel's mounted DOM and the pickers' own state.
#[derive(Default)]
pub(super) struct StylesPanelHost {
    mounted: Option<(web_sys::Node, Vec<Mounted>)>,
    /// `activeColorPickerSectionAtom`.
    section: Option<Section>,
    /// `ColorInput`'s state while it differs from the colour shown.
    hex: Option<HexInputState>,
    /// The most used custom colours of the open colour popup, sampled as
    /// it opened.
    custom_colors: Option<(ColorPickerType, Vec<String>)>,
    /// `activeEyeDropperAtom` and the mounted dropper.
    eye_dropper: Option<EyeDropperState>,
    eye_dropper_ui: Option<EyeDropper>,
    /// A strip's context menu: whose, and where.
    top_picks_menu: Option<(ActionName, (i32, i32))>,
    color_dnd: HashMap<ActionName, ColorPickerDnd>,
    font_dnd: Option<FontPickerDnd>,
    font_top_picks_menu: Option<(i32, i32)>,
    /// The font list's search term.
    font_search: String,
    /// `cachedElementsRef`: the selection as the font popup opened.
    font_cache: Map<String, Value>,
    icon_picker: IconPickerState,
    /// Bumped whenever this state changes, so the chrome renders again.
    generation: u64,
}

impl StylesPanelHost {
    fn changed(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }
}

/// The colour action a picker type is the `ColorPicker` of.
fn color_action(ty: ColorPickerType) -> ActionName {
    match ty {
        ColorPickerType::ElementStroke => ActionName::ChangeStrokeColor,
        _ => ActionName::ChangeBackgroundColor,
    }
}

/// The styles panel's colour picker props for `name` (the stroke, the
/// background or the bucket fill's).
fn color_panel(ctx: &ActionContext<'_>, name: ActionName) -> Option<ColorActionPanel> {
    match name {
        ActionName::ChangeBucketFillBackgroundColor => {
            Some(bucket_fill_color_panel(ctx, StylesPanelMode::Full))
        }
        _ => color_action_panel(ctx, name, StylesPanelMode::Full),
    }
}

/// What the styles panel shows, for the chrome's key: whether LayerUI
/// renders it, the tree, what its controls read and the pickers' state.
pub(super) fn styles_panel_key(inner: &Inner) -> Value {
    if inner.ui == "none" {
        return Value::Null;
    }
    let ed = &inner.editor;
    let ctx = ed.action_context();
    if !show_selected_shape_actions(&ctx) {
        return Value::Null;
    }
    let state = ed.app_state();
    let keys: Map<String, Value> = state
        .as_map()
        .iter()
        .filter(|(k, _)| {
            k.starts_with("currentItem")
                || matches!(
                    k.as_str(),
                    "openPopup"
                        | "colorTopPicks"
                        | "fontTopPicks"
                        | "currentHoveredFontFamily"
                        | "selectedElementIds"
                        | "selectedGroupIds"
                        | "editingGroupId"
                        | "selectedLinearElement"
                        | "activeTool"
                        | "theme"
                        | "height"
                        | "zenModeEnabled"
                        | "stylesPanelMode"
                )
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let editing = state
        .get("editingTextElement")
        .and_then(|e| e.get("id"))
        .cloned();
    let version: f64 = ctx.elements.iter().map(|e| e.base.version).sum();
    json!({
        "panel": format!("{:?}", selected_shape_actions(&ctx, document_rtl(inner))),
        "state": keys,
        "editing": editing,
        "version": version,
        "history": [ed.can_undo(), ed.can_redo()],
        "host": inner.styles_panel.generation,
    })
}

/// Runs `updateData(value)` of the action `name`.
fn update(weak: &Weak<RefCell<Inner>>, name: ActionName, value: Value) {
    let Some(rc) = weak.upgrade() else {
        return;
    };
    {
        let Ok(mut inner) = rc.try_borrow_mut() else {
            return;
        };
        run_action(&mut inner, name, &value);
        inner.after_event();
    }
    after(weak);
}

fn run_action(inner: &mut Inner, name: ActionName, value: &Value) {
    use ActionName as N;
    match name {
        N::SendToBack
        | N::SendBackward
        | N::BringForward
        | N::BringToFront
        | N::Group
        | N::Ungroup
        | N::DuplicateSelection
        | N::DeleteSelectedElements
        | N::Undo
        | N::Redo
        | N::ToggleLinearEditor => inner.editor.perform_action(name),
        _ => inner.editor.perform_style_action(name, value),
    }
}

/// After an event: the eye dropper follows its state, the chrome renders
/// again.
fn after(weak: &Weak<RefCell<Inner>>) {
    sync_eye_dropper(weak);
    refresh_chrome(weak);
}

// -- the colour pickers --------------------------------------------------------------

fn on_color_event(
    weak: &Weak<RefCell<Inner>>,
    name: ActionName,
    ty: ColorPickerType,
    event: ColorPickerEvent,
) {
    let Some(rc) = weak.upgrade() else {
        return;
    };
    let mut follow_focus = false;
    {
        let Ok(mut inner) = rc.try_borrow_mut() else {
            return;
        };
        let open = inner
            .editor
            .app_state()
            .get("openPopup")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let pick = |inner: &mut Inner, color: String| {
            run_action(inner, name, &json!({ "color": color }));
        };
        let set_popup = |inner: &mut Inner, popup: Option<String>| {
            run_action(
                inner,
                name,
                &json!({ "openPopup": popup.map_or(Value::Null, Value::String) }),
            );
        };
        match event {
            ColorPickerEvent::Trigger => {
                let next = toggle_popup(open.as_deref(), ty);
                let host = &mut inner.styles_panel;
                host.section = None;
                host.hex = None;
                host.custom_colors = None;
                host.changed();
                set_popup(&mut inner, next);
            }
            ColorPickerEvent::TopPick(color) => {
                pick(&mut inner, color);
                // the popup follows the focus to this picker, a tick later
                follow_focus = top_pick_follows_focus(open.as_deref(), ty);
            }
            ColorPickerEvent::Pick { color, section } => {
                inner.styles_panel.section = Some(section);
                inner.styles_panel.hex = None;
                inner.styles_panel.changed();
                pick(&mut inner, color);
            }
            ColorPickerEvent::Key(outcome) => {
                for effect in outcome.effects {
                    match effect {
                        KeyNavEffect::Change(Some(color)) => {
                            inner.styles_panel.hex = None;
                            pick(&mut inner, color);
                        }
                        KeyNavEffect::Change(None) => {}
                        KeyNavEffect::SetSection(section) => {
                            inner.styles_panel.section = Some(section);
                        }
                        KeyNavEffect::EyeDropperToggle(force) => {
                            let host = &mut inner.styles_panel;
                            host.eye_dropper = toggle_eye_dropper(host.eye_dropper, force, ty);
                        }
                        KeyNavEffect::Escape => match escape(inner.styles_panel.eye_dropper) {
                            EscapeOutcome::CancelEyeDropper => {
                                inner.styles_panel.eye_dropper = None;
                            }
                            EscapeOutcome::ClosePopup => {
                                close_color_popup(&mut inner, ty);
                            }
                        },
                    }
                }
                inner.styles_panel.changed();
            }
            ColorPickerEvent::HexInput(value) => {
                let change = change_hex_input(&value);
                inner.styles_panel.hex = Some(change.state);
                inner.styles_panel.changed();
                if let Some(color) = change.color {
                    pick(&mut inner, color);
                }
            }
            ColorPickerEvent::HexFocus => {
                inner.styles_panel.section = Some(Section::Hex);
                inner.styles_panel.changed();
            }
            ColorPickerEvent::HexBlur => {
                inner.styles_panel.hex = None;
                inner.styles_panel.changed();
            }
            ColorPickerEvent::EyeDropperTrigger => {
                let host = &mut inner.styles_panel;
                host.eye_dropper = toggle_eye_dropper(host.eye_dropper, None, ty);
                host.changed();
            }
            ColorPickerEvent::TopPicksChange(picks) => {
                store_top_picks(&mut inner, name, Some(&picks));
            }
            ColorPickerEvent::ResetTopPicks => {
                inner.styles_panel.top_picks_menu = None;
                inner.styles_panel.changed();
                store_top_picks(&mut inner, name, None);
            }
            ColorPickerEvent::TopPicksMenu(at) => {
                inner.styles_panel.top_picks_menu = at.map(|at| (name, at));
                inner.styles_panel.changed();
            }
            ColorPickerEvent::DragChange => inner.styles_panel.changed(),
        }
        inner.after_event();
    }
    after(weak);
    if follow_focus {
        let weak = weak.clone();
        set_timeout(0.0, move || {
            if let Some(rc) = weak.upgrade() {
                if let Ok(mut inner) = rc.try_borrow_mut() {
                    run_action(&mut inner, name, &json!({ "openPopup": ty.as_str() }));
                    inner.styles_panel.section = None;
                    inner.styles_panel.custom_colors = None;
                    inner.styles_panel.changed();
                    inner.after_event();
                }
            }
            after(&weak);
        });
    }
}

/// `updateData({ colorTopPicks })` of the picker's slot.
fn store_top_picks(inner: &mut Inner, name: ActionName, picks: Option<&[String]>) {
    let slot = {
        let ctx = inner.editor.action_context();
        color_panel(&ctx, name).map(|p| p.customizable_top_picks)
    };
    let Some(slot) = slot else {
        return;
    };
    let current = inner
        .editor
        .app_state()
        .get("colorTopPicks")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    run_action(inner, name, &color_top_picks_update(&current, slot, picks));
}

/// The popup's `onClose` (`ColorPicker.tsx:175-191`): `openPopup` cleared
/// while it is still this picker's, and the section.
fn close_color_popup(inner: &mut Inner, ty: ColorPickerType) {
    let open = inner
        .editor
        .app_state()
        .get("openPopup")
        .and_then(Value::as_str)
        == Some(ty.as_str());
    let host = &mut inner.styles_panel;
    host.section = None;
    host.hex = None;
    host.custom_colors = None;
    host.changed();
    if open {
        run_action(inner, color_action(ty), &json!({ "openPopup": null }));
    }
}

/// Mounts the eye dropper while its atom is set, removes it when not.
fn sync_eye_dropper(weak: &Weak<RefCell<Inner>>) {
    let Some(rc) = weak.upgrade() else {
        return;
    };
    let Ok(mut inner) = rc.try_borrow_mut() else {
        return;
    };
    match (
        inner.styles_panel.eye_dropper,
        inner.styles_panel.eye_dropper_ui.is_some(),
    ) {
        (None, true) => {
            // removed a tick later: its own release may be running
            if let Some(ui) = inner.styles_panel.eye_dropper_ui.take() {
                set_timeout(0.0, move || drop(ui));
            }
        }
        (Some(state), false) => {
            if let Ok(ui) = mount_eye_dropper(&inner, weak, state) {
                inner.styles_panel.eye_dropper_ui = Some(ui);
            }
        }
        _ => {}
    }
}

fn mount_eye_dropper(
    inner: &Inner,
    weak: &Weak<RefCell<Inner>>,
    state: EyeDropperState,
) -> Result<EyeDropper, JsValue> {
    let document = inner.document();
    let parent = match inner
        .container
        .query_selector(".excalidraw-eye-dropper-container")?
    {
        Some(p) => p,
        None => {
            let p = document.create_element("div")?;
            p.set_class_name("excalidraw-eye-dropper-container");
            inner.container.append_child(&p)?;
            p
        }
    };
    let canvas = inner
        .layers
        .canvas(Layer::Static)
        .ok_or_else(|| JsValue::from_str("no static canvas"))?
        .clone();
    let app = inner.editor.app_state();
    let offset = (
        app.get("offsetLeft").and_then(Value::as_f64).unwrap_or(0.0),
        app.get("offsetTop").and_then(Value::as_f64).unwrap_or(0.0),
    );
    let theme = theme_of(inner);
    let [x, y] = inner.editor.last_pointer();
    let property = match state.picker {
        ColorPickerType::ElementStroke => Some(ColorProperty::StrokeColor),
        ColorPickerType::ElementBackground => Some(ColorProperty::BackgroundColor),
        _ => None,
    };
    let name = color_action(state.picker);
    let (w1, w2, w3) = (weak.clone(), weak.clone(), weak.clone());
    EyeDropper::mount(
        &document,
        &parent,
        EyeDropperProps {
            canvas,
            offset,
            theme,
            last_pointer: (x, y),
            on_change: Rc::new(move |color: String, _alt: bool| {
                let Some(property) = property else {
                    return;
                };
                if let Some(rc) = w1.upgrade() {
                    if let Ok(mut inner) = rc.try_borrow_mut() {
                        inner.editor.preview_color(property, &color);
                        inner.after_event();
                    }
                }
            }),
            on_select: Rc::new(move |color: String| {
                if let Some(rc) = w2.upgrade() {
                    if let Ok(mut inner) = rc.try_borrow_mut() {
                        inner.styles_panel.eye_dropper = None;
                        inner.styles_panel.changed();
                        run_action(&mut inner, name, &json!({ "color": color }));
                        inner.after_event();
                    }
                }
                after(&w2);
            }),
            on_cancel: Rc::new(move || {
                if let Some(rc) = w3.upgrade() {
                    if let Ok(mut inner) = rc.try_borrow_mut() {
                        inner.styles_panel.eye_dropper = None;
                        inner.styles_panel.changed();
                    }
                }
                after(&w3);
            }),
        },
    )
}

fn theme_of(inner: &Inner) -> Theme {
    if inner
        .editor
        .app_state()
        .get("theme")
        .and_then(Value::as_str)
        == Some("dark")
    {
        Theme::Dark
    } else {
        Theme::Light
    }
}

/// The `ColorPicker` a colour action's panel component renders
/// (`ColorPicker.tsx:51-73`): open while `appState.openPopup` is its
/// type, with the pickers' state.
fn color_picker_props(
    ctx: &ActionContext<'_>,
    host: &mut StylesPanelHost,
    name: ActionName,
    panel: &ColorActionPanel,
    theme: Theme,
    on_event: OnColorPickerEvent,
) -> ColorPickerProps {
    let open_popup = ctx.app_state.get("openPopup").and_then(Value::as_str);
    // the bucket fill's picker shares the background's popup
    let open = open_popup == Some(panel.ty.as_str());
    let custom_colors = if open {
        match &host.custom_colors {
            Some((ty, colors)) if *ty == panel.ty => colors.clone(),
            _ => {
                let colors = picker_custom_colors(panel.ty, ctx.elements, panel.palette);
                host.custom_colors = Some((panel.ty, colors.clone()));
                colors
            }
        }
    } else {
        Vec::new()
    };
    let slot = panel.customizable_top_picks;
    let color_top_picks = ctx
        .app_state
        .get("colorTopPicks")
        .and_then(|p| p.get(slot.as_str()))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let color = panel.color.as_deref().unwrap_or("");
    ColorPickerProps {
        ty: panel.ty,
        color: panel.color.clone(),
        label: legend_text(panel.label).to_owned(),
        palette: Some(panel.palette),
        top_picks: Some(panel.top_picks),
        excluded_colors: panel
            .excluded_colors
            .unwrap_or_default()
            .iter()
            .map(|c| (*c).to_owned())
            .collect(),
        theme,
        open,
        mode: StylesPanelMode::Full,
        phone: false,
        is_darwin: is_darwin(),
        section: host
            .section
            .or_else(|| initial_section(panel.color.as_deref(), panel.palette, &custom_colors)),
        custom_colors,
        eye_dropper_active: host.eye_dropper.is_some_and(|e| e.picker == panel.ty),
        hex: host
            .hex
            .clone()
            .filter(|_| open)
            .unwrap_or_else(|| HexInputState::for_color(color)),
        popup_id: format!("excali-editor-{}-popup", panel.ty.as_str()),
        on_event: Some(on_event),
        customizable_top_picks: Some(slot),
        color_top_picks,
        top_picks_menu: host
            .top_picks_menu
            .filter(|(n, _)| *n == name)
            .map(|(_, at)| at),
        dnd: Some(
            host.color_dnd
                .entry(name)
                .or_insert_with(color_picker_dnd)
                .clone(),
        ),
    }
}

// -- the font picker -------------------------------------------------------------------

/// The fonts the scene uses (`app.fonts.getSceneFamilies()`).
fn scene_families(ctx: &ActionContext<'_>) -> Vec<FontFamily> {
    let mut out: Vec<FontFamily> = Vec::new();
    for e in ctx.elements.iter().filter(|e| !e.base.is_deleted) {
        if let ElementKind::Text(t) = &e.kind {
            if !out.contains(&t.font_family) {
                out.push(t.font_family);
            }
        }
    }
    out
}

/// The picker's state: the panel's, the selection's family read from the
/// cached elements while the popup is open (`actionProperties.tsx:
/// 1395-1405`), and the list's search.
fn font_state(ctx: &ActionContext<'_>, host: &StylesPanelHost) -> FontPickerState {
    let mut state = font_family_panel(ctx, StylesPanelMode::Full).state;
    if state.is_open() && !host.font_cache.is_empty() {
        let cached: Vec<excali_core::element::Element> = host
            .font_cache
            .values()
            .filter_map(|v| {
                v.as_object()
                    .and_then(|m| excali_core::element::Element::from_map(m.clone()).ok())
            })
            .collect();
        let cx = ActionContext {
            elements: &cached,
            app_state: ctx.app_state,
            props: ctx.props,
            env: ctx.env,
        };
        state.selected = font_family_panel(&cx, StylesPanelMode::Full).state.selected;
    }
    state.search = host.font_search.clone();
    state
}

fn on_font_event(weak: &Weak<RefCell<Inner>>, event: FontPickerEvent) {
    let Some(rc) = weak.upgrade() else {
        return;
    };
    {
        let Ok(mut inner) = rc.try_borrow_mut() else {
            return;
        };
        let (calls, search) = {
            let ctx = inner.editor.action_context();
            let mut state = font_state(&ctx, &inner.styles_panel);
            let cx = FontListContext {
                scene_families: scene_families(&ctx),
                show_deprecated_fonts: false,
            };
            (state.dispatch(event.clone(), &cx), state.search)
        };
        inner.styles_panel.font_search = search;
        match event {
            FontPickerEvent::TopPicksMenu(at) => inner.styles_panel.font_top_picks_menu = at,
            FontPickerEvent::Search(_) | FontPickerEvent::DragChange => {}
            _ => {}
        }
        inner.styles_panel.changed();
        for call in calls {
            font_call(&mut inner, call);
        }
        inner.after_event();
    }
    after(weak);
}

/// What the panel does with a picker callback: `setBatchedData` into
/// `updateData` (`actionProperties.tsx:1455-1535`).
fn font_call(inner: &mut Inner, call: FontPickerEvent) {
    let name = ActionName::ChangeFontFamily;
    let cache = |inner: &Inner| Value::Object(inner.styles_panel.font_cache.clone());
    match call {
        FontPickerEvent::Select(family) => {
            run_action(
                inner,
                name,
                &json!({
                    "openPopup": null,
                    "currentHoveredFontFamily": null,
                    "currentItemFontFamily": family.0,
                }),
            );
            inner.styles_panel.font_cache.clear();
        }
        FontPickerEvent::Hover(family) => {
            let value = json!({
                "currentHoveredFontFamily": family.0,
                "cachedElements": cache(inner),
                "resetContainers": true,
            });
            run_action(inner, name, &value);
        }
        FontPickerEvent::Leave => {
            let value = json!({
                "currentHoveredFontFamily": null,
                "cachedElements": cache(inner),
                "resetAll": true,
            });
            run_action(inner, name, &value);
        }
        FontPickerEvent::PopupChange(true) => {
            // open: the cache from scratch, the text being edited or the
            // selection with its bound text
            let cached: Map<String, Value> = {
                let ed = &inner.editor;
                let state = ed.app_state();
                let editing = state
                    .get("editingTextElement")
                    .filter(|e| e.get("type").and_then(Value::as_str) == Some("text"))
                    .and_then(|e| e.get("id"))
                    .and_then(Value::as_str);
                let selected = state
                    .get("selectedElementIds")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                let elements = ed.elements();
                let is_selected = |e: &excali_core::element::Element| {
                    selected
                        .get(&e.base.id)
                        .is_some_and(|v| v.as_bool() == Some(true))
                };
                elements
                    .iter()
                    .filter(|e| !e.base.is_deleted)
                    .filter(|e| match editing {
                        Some(id) => e.base.id == id,
                        None => {
                            is_selected(e)
                                || matches!(&e.kind, ElementKind::Text(t)
                                if t.container_id.as_deref().is_some_and(|c| {
                                    elements.iter().any(|p| p.base.id == c && is_selected(p))
                                }))
                        }
                    })
                    .map(|e| (e.base.id.clone(), Value::Object(e.to_map())))
                    .collect()
            };
            inner.styles_panel.font_cache = cached;
            run_action(inner, name, &json!({ "openPopup": "fontFamily" }));
        }
        FontPickerEvent::PopupChange(false) => {
            let value = json!({
                "currentHoveredFontFamily": null,
                "cachedElements": cache(inner),
                "resetAll": true,
            });
            inner.styles_panel.font_cache.clear();
            run_action(inner, name, &value);
        }
        FontPickerEvent::TriggerSelect => {
            if inner
                .editor
                .app_state()
                .get("openPopup")
                .and_then(Value::as_str)
                == Some("fontFamily")
            {
                let mut patch = Map::new();
                patch.insert("openPopup".into(), Value::Null);
                inner.editor.set_app_state(patch);
            }
        }
        FontPickerEvent::TopPicksChange(picks) => {
            let picks = picks.map_or(Value::Null, |p| {
                json!(p.iter().map(|f| f.0).collect::<Vec<_>>())
            });
            run_action(inner, name, &json!({ "fontTopPicks": picks }));
        }
        FontPickerEvent::TopPicksMenu(_)
        | FontPickerEvent::DragChange
        | FontPickerEvent::Search(_) => {}
    }
}

// -- the panel ---------------------------------------------------------------------------

/// Re-mounts LayerUI's selected shape actions under the main menu while
/// `showSelectedShapeActions` holds (`LayerUI.tsx:249-297`, :314-330).
pub(super) fn render_styles_panel(weak: &Weak<RefCell<Inner>>) -> Result<(), JsValue> {
    let Some(rc) = weak.upgrade() else {
        return Ok(());
    };
    let mut guard = rc.borrow_mut();
    let inner = &mut *guard;
    // where the focus was in the panel, to give it back after the render
    // (React keeps a focused control in place)
    let mut focus_path: Option<Vec<u32>> = None;
    if let Some((root, _)) = inner.styles_panel.mounted.take() {
        focus_path = inner
            .document()
            .active_element()
            .and_then(|active| node_path(&root, &active));
        if let Some(parent) = root.parent_node() {
            parent.remove_child(&root)?;
        }
    }
    if inner.ui == "none" {
        return Ok(());
    }
    let document = inner.document();
    let rtl = document_rtl(inner);
    let theme = theme_of(inner);
    let ed = &inner.editor;
    let host = &mut inner.styles_panel;
    let ctx = ed.action_context();
    if !show_selected_shape_actions(&ctx) {
        return Ok(());
    }
    // the pickers' own state goes with their popups
    let open_popup = ctx
        .app_state
        .get("openPopup")
        .and_then(Value::as_str)
        .map(str::to_owned);
    if ColorPickerType::from_popup(open_popup.as_deref().unwrap_or("")).is_none() {
        host.section = None;
        host.hex = None;
        host.custom_colors = None;
    }
    let state = ctx.app_state;
    let tree = shape_actions_section(
        state.get("height").and_then(Value::as_f64).unwrap_or(0.0),
        state
            .get("zenModeEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "excali-editor",
        selected_shape_actions(&ctx, rtl),
    );
    let on_update: OnActionUpdate = {
        let weak = weak.clone();
        Rc::new(move |name, value| {
            let weak = weak.clone();
            // after the handler: its render replaces the control
            set_timeout(0.0, move || update(&weak, name, value));
        })
    };
    let mut opts = ActionPanelOptions::new(StylesPanelMode::Full, on_update);
    opts.theme = theme;
    opts.rtl = rtl;
    opts.is_darwin = is_darwin();
    opts.open_popup = open_popup.clone();
    opts.icon_picker = host.icon_picker.clone();
    opts.undo_stack_empty = !ed.can_undo();
    opts.redo_stack_empty = !ed.can_redo();
    opts.on_icon_picker = Some({
        let weak = weak.clone();
        Rc::new(move |next: IconPickerState| {
            if let Some(rc) = weak.upgrade() {
                if let Ok(mut inner) = rc.try_borrow_mut() {
                    inner.styles_panel.icon_picker = next;
                    inner.styles_panel.changed();
                }
            }
            let weak = weak.clone();
            set_timeout(0.0, move || refresh_chrome(&weak));
        })
    });
    let mut mounted = Vec::new();
    let mut hooks: Vec<PendingHooks> = Vec::new();
    let mut render_action =
        |name: ActionName, cycle: bool| -> Result<Option<web_sys::Node>, JsValue> {
            let fragment = document.create_document_fragment();
            let nodes: Vec<Node> = if let Some(panel) = color_panel(&ctx, name) {
                let ty = panel.ty;
                let events = weak.clone();
                let on_event = Rc::new(move |event: ColorPickerEvent| {
                    on_color_event(&events, name, ty, event)
                }) as OnColorPickerEvent;
                let props = color_picker_props(&ctx, host, name, &panel, theme, on_event);
                let mut nodes: Vec<Node> = panel
                    .heading
                    .map(|key| {
                        excali_ui::dom::Element::new("h3")
                            .attr("aria-hidden", "true")
                            .child(Node::text(legend_text(key)))
                            .into()
                    })
                    .into_iter()
                    .collect();
                nodes.extend(color_picker(&props));
                nodes
            } else if name == ActionName::ChangeFontFamily {
                let panel = font_family_panel(&ctx, StylesPanelMode::Full);
                let events = weak.clone();
                let props = FontPickerProps {
                    state: font_state(&ctx, host),
                    mode: StylesPanelMode::Full,
                    list: FontListContext {
                        scene_families: scene_families(&ctx),
                        show_deprecated_fonts: false,
                    },
                    theme,
                    phone: false,
                    is_darwin: is_darwin(),
                    popup_id: "excali-editor-fontFamily-popup".into(),
                    on_event: Some(Rc::new(move |event: FontPickerEvent| {
                        on_font_event(&events, event)
                    }) as OnFontPickerEvent),
                    top_picks_menu: host.font_top_picks_menu,
                    dnd: Some(host.font_dnd.get_or_insert_with(font_picker_dnd).clone()),
                };
                let mut nodes: Vec<Node> = panel
                    .legend
                    .map(|l| {
                        excali_ui::dom::Element::new("legend")
                            .child(Node::text(l))
                            .into()
                    })
                    .into_iter()
                    .collect();
                nodes.extend(font_picker(&props));
                nodes
            } else {
                let mut o = opts.clone();
                o.cycle = cycle;
                match render_action_panel(&ctx, name, &o) {
                    Some(nodes) => nodes,
                    None => return Ok(None),
                }
            };
            for node in &nodes {
                let (m, h) = mount_deferred(node, &document, &fragment)?;
                mounted.push(m);
                hooks.push(h);
            }
            Ok(Some(fragment.into()))
        };
    let root = excali_ui::styles_panel::mount(
        &document,
        &tree,
        &mut render_action,
        &excali_ui::styles_panel::MountOptions::default(),
    )?;
    let Some(root) = root else {
        return Ok(());
    };
    inner.top_left.append_child(&root)?;
    for h in hooks {
        h.run();
    }
    if let Some(el) = root.dyn_ref::<web_sys::Element>() {
        excali_ui::styles_panel::place_popovers(el);
        excali_ui::action_panels::place_icon_pickers(el);
        restore_focus(&document, el, focus_path.as_deref(), &inner.container);
        // a re-render keeps the hex input's caret at the end of what was typed
        if let Ok(Some(input)) = el.query_selector("input.color-picker-input") {
            if let Ok(input) = input.dyn_into::<HtmlInputElement>() {
                if document.active_element().as_ref() == Some(input.as_ref()) {
                    let end = input.value().encode_utf16().count() as u32;
                    let _ = input.set_selection_range(end, end);
                }
            }
        }
    }
    inner.styles_panel.mounted = Some((root, mounted));
    Ok(())
}

/// The child indices from `root` down to `node`, when `node` is in it.
fn node_path(root: &web_sys::Node, node: &web_sys::Element) -> Option<Vec<u32>> {
    let mut path = Vec::new();
    let mut current: web_sys::Node = node.clone().into();
    while !current.is_same_node(Some(root)) {
        let parent = current.parent_node()?;
        let children = parent.child_nodes();
        let index = (0..children.length()).find(|&i| {
            children
                .item(i)
                .is_some_and(|c| c.is_same_node(Some(&current)))
        })?;
        path.push(index);
        current = parent;
    }
    path.reverse();
    Some(path)
}

/// After a render: a focus the render dropped (to the body) goes to the
/// control at the same place in the new panel, else to the editor's
/// container, where the keys are listened to.
fn restore_focus(
    document: &web_sys::Document,
    root: &web_sys::Element,
    path: Option<&[u32]>,
    container: &web_sys::HtmlElement,
) {
    let Some(path) = path else {
        return;
    };
    let lost = document
        .active_element()
        .is_none_or(|a| document.body().is_some_and(|b| a.is_same_node(Some(&b))));
    if !lost {
        return;
    }
    let mut node: Option<web_sys::Node> = Some(root.clone().into());
    for &i in path {
        node = node.and_then(|n| n.child_nodes().item(i));
    }
    match node.and_then(|n| n.dyn_into::<web_sys::HtmlElement>().ok()) {
        Some(el) => {
            let _ = el.focus();
            if document
                .active_element()
                .is_some_and(|a| a.is_same_node(Some(&el)))
            {
                return;
            }
            let _ = container.focus();
        }
        None => {
            let _ = container.focus();
        }
    }
}

// -- outside presses ---------------------------------------------------------------------

/// A press outside an open popup of the panel (radix's
/// `onPointerDownOutside`, `DismissableLayer`): it closes, as its
/// `onClose` does, unless the eye dropper is on or the press is on the
/// popup's own trigger (which toggles it) or the top picks' menu.
pub(super) fn pointer_down_outside(weak: &Weak<RefCell<Inner>>, event: &Event) {
    let Some(rc) = weak.upgrade() else {
        return;
    };
    let target = event
        .target()
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok());
    let inside = |selector: &str| {
        target
            .as_ref()
            .and_then(|t| t.closest(selector).ok().flatten())
            .is_some()
    };
    let changed = {
        let Ok(mut inner) = rc.try_borrow_mut() else {
            return;
        };
        let mut changed = false;
        if inner.styles_panel.icon_picker.open.is_some()
            && !inside(".picker, .picker-trigger, [data-radix-popper-content-wrapper]")
        {
            inner.styles_panel.icon_picker.open = None;
            inner.styles_panel.changed();
            changed = true;
        }
        let open = inner
            .editor
            .app_state()
            .get("openPopup")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if let Some(open) = open {
            let ours = open == "fontFamily"
                || matches!(
                    ColorPickerType::from_popup(&open),
                    Some(ColorPickerType::ElementStroke | ColorPickerType::ElementBackground)
                );
            let excluded = inside(
                "[data-radix-popper-content-wrapper], .top-picks-dnd__context-menu, \
                 .excalidraw-eye-dropper-backdrop, [aria-expanded=\"true\"]",
            );
            if ours && !excluded && inner.styles_panel.eye_dropper.is_none() {
                if let Some(ty) = ColorPickerType::from_popup(&open) {
                    close_color_popup(&mut inner, ty);
                } else {
                    font_call(&mut inner, FontPickerEvent::TriggerSelect);
                    font_call(&mut inner, FontPickerEvent::PopupChange(false));
                    inner.styles_panel.font_search.clear();
                    inner.styles_panel.changed();
                }
                inner.after_event();
                changed = true;
            }
        }
        changed
    };
    if changed {
        after(weak);
    }
}
