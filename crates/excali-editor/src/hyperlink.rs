//! The hyperlink popup (`components/hyperlink/Hyperlink.tsx`): whether it
//! shows and where, and what its input submits. `actionLink`
//! (`actions/actionLink.tsx:20-42`, Ctrl/Cmd+K) opens it in the editor
//! mode by setting `showHyperlinkPopup: "editor"`; Enter or Escape in its
//! input submits the link and switches it to `"info"`, which shows the
//! link with the edit and remove buttons.
//!
//! The DOM is `excali_ui::hyperlink`'s; the host keeps what the input
//! holds while the popup stays mounted (React's `inputVal` state).

use excali_core::app_state::AppState;
use excali_core::element::ElementType;
use excali_core::link::normalize_link;
use excali_scene::bounds::get_element_absolute_coords;
use serde_json::Value;

use crate::keyboard::get_selected_elements;
use crate::scene::Scene;
use crate::viewport::{scene_coords_to_viewport_coords, ViewportState};

/// `POPUP_WIDTH` (`Hyperlink.tsx:55`).
pub const POPUP_WIDTH: f64 = 380.0;
/// `POPUP_HEIGHT` (`Hyperlink.tsx:56`).
pub const POPUP_HEIGHT: f64 = 42.0;
/// `POPUP_PADDING` (`Hyperlink.tsx:57`).
pub const POPUP_PADDING: f64 = 5.0;
/// `SPACE_BOTTOM` (`Hyperlink.tsx:58`): the popup's top above the
/// element's.
pub const SPACE_BOTTOM: f64 = 85.0;

/// `appState.showHyperlinkPopup` while the popup shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HyperlinkMode {
    /// `"editor"`: the input.
    Editor,
    /// `"info"`: the link (or "No link is set") and the edit button.
    Info,
}

impl HyperlinkMode {
    /// The `showHyperlinkPopup` value.
    pub const fn as_str(self) -> &'static str {
        match self {
            HyperlinkMode::Editor => "editor",
            HyperlinkMode::Info => "info",
        }
    }
}

/// What the popup does, as its handlers report it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HyperlinkEvent {
    /// The input's `onChange`: its value.
    Input(String),
    /// Enter or Escape in the input (`Hyperlink.tsx:266-279`): the value
    /// to submit, then the info popup.
    Submit(String),
    /// The edit button (`onEdit`, `:235-238`).
    Edit,
    /// The "Link to object" button (`:325-335`).
    LinkToElement,
    /// The remove button (`handleRemove`, `:229-233`).
    Remove,
}

/// The popup for the selection (`App.tsx:2549-2565` and the early return
/// of `Hyperlink.tsx:240-249`).
#[derive(Clone, Debug, PartialEq)]
pub struct HyperlinkPanel {
    pub element_id: String,
    pub mode: HyperlinkMode,
    /// `getCoordsForPopover`: the popup's left and top in the container.
    pub left: f64,
    pub top: f64,
    /// `element.link`.
    pub link: Option<String>,
    /// `isEmbeddableElement(element)`: no remove button.
    pub embeddable: bool,
}

fn truthy(app_state: &AppState, key: &str) -> bool {
    match app_state.get(key) {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Number(n)) => n.as_f64().is_some_and(|n| n != 0.0),
        Some(_) => true,
    }
}

/// The popup when one element is selected, `showHyperlinkPopup` is set,
/// the element link dialog is closed (`App.tsx:2549-2553`), and no context
/// menu, drag, resize, rotation, open menu or view mode hides it
/// (`Hyperlink.tsx:240-249`).
pub fn hyperlink_panel(scene: &Scene, app_state: &AppState) -> Option<HyperlinkPanel> {
    let mode = match app_state.get("showHyperlinkPopup").and_then(Value::as_str) {
        Some("editor") => HyperlinkMode::Editor,
        Some("info") => HyperlinkMode::Info,
        _ => return None,
    };
    let dialog = app_state
        .get("openDialog")
        .and_then(|d| d.get("name"))
        .and_then(Value::as_str);
    if dialog == Some("elementLinkSelector") {
        return None;
    }
    let hidden = [
        "contextMenu",
        "selectedElementsAreBeingDragged",
        "resizingElement",
        "isRotating",
        "openMenu",
        "viewModeEnabled",
    ];
    if hidden.iter().any(|k| truthy(app_state, k)) {
        return None;
    }
    let selected = get_selected_elements(scene, app_state, false, false);
    let [element] = selected.as_slice() else {
        return None;
    };
    let (left, top) = coords_for_popover(scene, element, app_state);
    Some(HyperlinkPanel {
        element_id: element.base.id.clone(),
        mode,
        left,
        top,
        link: element.base.link.clone(),
        embeddable: element.element_type() == ElementType::Embeddable,
    })
}

/// `getCoordsForPopover` (`Hyperlink.tsx:356-369`): centred on the
/// element's unrotated box, [`SPACE_BOTTOM`] above its top, in the
/// container's coordinates.
fn coords_for_popover(
    scene: &Scene,
    element: &excali_core::element::Element,
    app_state: &AppState,
) -> (f64, f64) {
    let map = scene.elements_map();
    let [x1, y1, ..] = get_element_absolute_coords(element, &map, false);
    let viewport = ViewportState::from_app_state(app_state);
    let (x, y) = scene_coords_to_viewport_coords(x1 + element.base.width / 2.0, y1, &viewport);
    (
        x - viewport.offset_left - POPUP_WIDTH / 2.0,
        y - viewport.offset_top - SPACE_BOTTOM,
    )
}

/// The link `handleSubmit` writes for the input's `value`
/// (`Hyperlink.tsx:99-104`): `normalizeLink(value) || null`.
pub fn submitted_link(value: &str) -> Option<String> {
    Some(normalize_link(value)).filter(|l| !l.is_empty())
}
