//! ex-519 / ex-701 harness: excali-ui's full and compact styles panels in
//! the browser.
//!
//! [`mount_panel`] builds a scene and an app state from JSON, and mounts
//! [`excali_ui::styles_panel::shape_actions_section`] around
//! [`selected_shape_actions`] into a DOM element with
//! [`excali_ui::styles_panel::mount`]; each action's panel component is a
//! `<button data-action="…">` (with `data-cycle` for the freedraw cycle
//! button). [`mount_compact_panel`] mounts the compact panel in its
//! section the same way, and a trigger's click or Escape in a popover
//! sets `openPopup` and mounts it again. [`show_panel`] answers
//! `showSelectedShapeActions`; [`form_factor`] and [`styles_panel_mode`]
//! the form factor rules. `scripts/web/styles-panel.sh` builds it.

use std::rc::Rc;

use excali_core::app_state::AppState;
use excali_core::element::Element;
use excali_editor::actions::{show_selected_shape_actions, ActionContext, ActionEnv, AppProps};
use excali_ui::editor_interface::{derive_styles_panel_mode, get_form_factor, DesktopUiMode};
use excali_ui::styles_panel::{
    compact_shape_actions, compact_shape_actions_section, mount, place_popovers,
    selected_shape_actions, shape_actions_section, MountOptions, PanelNode,
};
use serde_json::Value;
use wasm_bindgen::prelude::*;
use web_sys::{Element as DomElement, Node};

fn js_err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// A scene (`[element, ...]`) and an app state (the defaults with the
/// object's keys).
fn parse(scene: &str, app_state: &str) -> Result<(Vec<Element>, AppState), JsValue> {
    let elements = match serde_json::from_str(scene).map_err(js_err)? {
        Value::Array(list) => list
            .into_iter()
            .map(|e| match e {
                Value::Object(map) => Element::from_map(map).map_err(js_err),
                _ => Err(js_err("an element is not an object")),
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err(js_err("the scene is not an array")),
    };
    let mut state = AppState::default();
    match serde_json::from_str(app_state).map_err(js_err)? {
        Value::Object(map) => {
            for (k, v) in map {
                state.insert(k, v);
            }
        }
        _ => return Err(js_err("the app state is not an object")),
    }
    Ok((elements, state))
}

fn ctx<'a>(
    elements: &'a [Element],
    app_state: &'a AppState,
    props: &'a AppProps,
    env: &'a ActionEnv,
) -> ActionContext<'a> {
    ActionContext {
        elements,
        app_state,
        props,
        env,
    }
}

/// Mounts the panel for `scene` and `app_state` (its `height` and
/// `zenModeEnabled` size and place the island) into `target`, in the
/// container `container_id`; `rtl` is the document direction.
#[wasm_bindgen]
pub fn mount_panel(
    target: &DomElement,
    scene: &str,
    app_state: &str,
    rtl: bool,
    container_id: &str,
) -> Result<(), JsValue> {
    let (elements, state) = parse(scene, app_state)?;
    let (props, env) = (AppProps::default(), ActionEnv::default());
    let ctx = ctx(&elements, &state, &props, &env);
    let document = target
        .owner_document()
        .ok_or_else(|| js_err("the target has no document"))?;
    let height = state.get("height").and_then(Value::as_f64).unwrap_or(0.0);
    let zen = state.get("zenModeEnabled").and_then(Value::as_bool) == Some(true);
    let mut render = render_action(&document);
    let panel = mount(
        &document,
        &selected_shape_actions(&ctx, rtl),
        &mut render,
        &MountOptions::default(),
    )?;
    let section = shape_actions_section(height, zen, container_id, PanelNode::Panel);
    let options = MountOptions {
        panel: panel.as_ref(),
        ..MountOptions::default()
    };
    if let Some(node) = mount(&document, &section, &mut render, &options)? {
        target.append_child(&node)?;
    }
    Ok(())
}

/// Each action's panel component: `<button data-action>`, with
/// `data-cycle` when the panel asks for the cycle button.
fn render_action(
    document: &web_sys::Document,
) -> impl FnMut(excali_editor::actions::ActionName, bool) -> Result<Option<Node>, JsValue> + '_ {
    move |name, cycle| {
        let button = document.create_element("button")?;
        button.set_attribute("data-action", name.as_str())?;
        if cycle {
            button.set_attribute("data-cycle", "")?;
        }
        Ok(Some(Node::from(button)))
    }
}

/// Mounts the compact panel for `scene` and `app_state` into `target`
/// (emptied first), as [`mount_panel`] does the full one, and places its
/// open popover. A trigger's click, or Escape in the popover, mounts it
/// again with the `openPopup` it sets.
#[wasm_bindgen]
pub fn mount_compact_panel(
    target: &DomElement,
    scene: &str,
    app_state: &str,
    rtl: bool,
    container_id: &str,
) -> Result<(), JsValue> {
    let (elements, state) = parse(scene, app_state)?;
    let (props, env) = (AppProps::default(), ActionEnv::default());
    let ctx = ctx(&elements, &state, &props, &env);
    let document = target
        .owner_document()
        .ok_or_else(|| js_err("the target has no document"))?;
    let height = state.get("height").and_then(Value::as_f64).unwrap_or(0.0);
    let zen = state.get("zenModeEnabled").and_then(Value::as_bool) == Some(true);
    let on_popup = {
        let (target, scene, container_id) =
            (target.clone(), scene.to_string(), container_id.to_string());
        let current: Value = serde_json::from_str(app_state).map_err(js_err)?;
        Rc::new(
            move |popup: Option<excali_ui::styles_panel::CompactPopup>| {
                let mut next = current.clone();
                next["openPopup"] = popup.map_or(Value::Null, |p| Value::from(p.as_str()));
                let state = next.to_string();
                if let Err(e) = mount_compact_panel(&target, &scene, &state, rtl, &container_id) {
                    web_sys::console::error_1(&e);
                }
            },
        ) as excali_ui::styles_panel::OnPopup
    };
    let options = MountOptions {
        on_popup: Some(on_popup),
        ..MountOptions::default()
    };
    let mut render = render_action(&document);
    let panel = mount(
        &document,
        &compact_shape_actions(&ctx, rtl),
        &mut render,
        &options,
    )?;
    let section = compact_shape_actions_section(height, zen, container_id, PanelNode::Panel);
    let options = MountOptions {
        panel: panel.as_ref(),
        ..options
    };
    target.set_inner_html("");
    if let Some(node) = mount(&document, &section, &mut render, &options)? {
        target.append_child(&node)?;
    }
    place_popovers(target);
    Ok(())
}

/// `getFormFactor(width, height)`.
#[wasm_bindgen]
pub fn form_factor(width: f64, height: f64) -> String {
    get_form_factor(width, height).as_str().into()
}

/// `deriveStylesPanelMode` for an editor of `width` × `height` whose
/// desktop UI mode is `desktop_ui_mode` ("compact" or "full").
#[wasm_bindgen]
pub fn styles_panel_mode(
    width: f64,
    height: f64,
    desktop_ui_mode: &str,
) -> Result<String, JsValue> {
    let desktop =
        DesktopUiMode::parse(desktop_ui_mode).ok_or_else(|| js_err("no such desktop UI mode"))?;
    Ok(
        derive_styles_panel_mode(get_form_factor(width, height), desktop)
            .as_str()
            .into(),
    )
}

/// `showSelectedShapeActions` for `scene` and `app_state`.
#[wasm_bindgen]
pub fn show_panel(scene: &str, app_state: &str) -> Result<bool, JsValue> {
    let (elements, state) = parse(scene, app_state)?;
    let (props, env) = (AppProps::default(), ActionEnv::default());
    Ok(show_selected_shape_actions(&ctx(
        &elements, &state, &props, &env,
    )))
}
