//! ex-519 harness: excali-ui's full styles panel in the browser.
//!
//! [`mount_panel`] builds a scene and an app state from JSON, and mounts
//! [`excali_ui::styles_panel::shape_actions_section`] around
//! [`selected_shape_actions`] into a DOM element with
//! [`excali_ui::styles_panel::mount`]; each action's panel component is a
//! `<button data-action="…">`. [`show_panel`] answers
//! `showSelectedShapeActions`. `scripts/web/styles-panel.sh` builds it.

use excali_core::app_state::AppState;
use excali_core::element::Element;
use excali_editor::actions::{show_selected_shape_actions, ActionContext, ActionEnv, AppProps};
use excali_ui::styles_panel::{mount, selected_shape_actions, shape_actions_section};
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
    let mut render = |name: excali_editor::actions::ActionName| {
        let button = document.create_element("button")?;
        button.set_attribute("data-action", name.as_str())?;
        Ok(Some(Node::from(button)))
    };
    let panel = mount(
        &document,
        &selected_shape_actions(&ctx, rtl),
        &mut render,
        None,
    )?;
    let section = shape_actions_section(
        height,
        zen,
        container_id,
        excali_ui::styles_panel::PanelNode::Panel,
    );
    if let Some(node) = mount(&document, &section, &mut render, panel.as_ref())? {
        target.append_child(&node)?;
    }
    Ok(())
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
