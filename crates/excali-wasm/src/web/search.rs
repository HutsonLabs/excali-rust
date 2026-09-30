//! The default sidebar's search tab (`SearchMenu`,
//! `excali_ui::search_menu`) in the host: the menu mounted while the
//! sidebar is open on the search tab, its events and window keys run
//! through the menu's state machine against the editor
//! ([`crate::editor::Editor::with_search_context`]), the debounced search
//! on a [`SEARCH_DEBOUNCE_MS`] timer, and the navigation to the focused
//! match ([`crate::editor::Editor::fit_bounds`]).
//!
//! `app.viewport.getOffsets()` measures the UI marked `data-viewport-ui`;
//! here it is upstream's 24 px padding on every side plus the sidebar's
//! [`RIGHT_SIDEBAR_WIDTH`] on the right while it is open.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use excali_editor::viewport::Offsets;
use excali_ui::dom::Node;
use excali_ui::keyboard::keystroke;
use excali_ui::library_sidebar::{CANVAS_SEARCH_TAB, DEFAULT_SIDEBAR_NAME};
use excali_ui::search_menu::{
    focus_search_matches, key_down, mount, run_pending, search_menu, sync, unmount, update,
    InputFocus, SearchEffect, SearchKey, SearchMenuEvent, SearchMenuProps, SearchMenuState,
    SEARCH_DEBOUNCE_MS, SEARCH_MENU_INPUT_WRAPPER,
};
use excali_ui::theme::RIGHT_SIDEBAR_WIDTH;
use serde_json::Value;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::KeyboardEvent;

use super::{is_darwin, refresh_chrome, Inner};

/// The menu's state and whether it is mounted, the debounce timer, and a
/// generation the chrome's key reads so a change renders it again.
#[derive(Default)]
pub(super) struct SearchSession {
    pub(super) state: SearchMenuState,
    mounted: bool,
    timer: Option<i32>,
    pub(super) generation: u64,
}

/// `getOffsets()` as described in the module docs.
fn offsets() -> Offsets {
    Offsets {
        top: 24.0,
        right: 24.0 + RIGHT_SIDEBAR_WIDTH,
        bottom: 24.0,
        left: 24.0,
    }
}

/// Whether the sidebar is open on the search tab.
fn on_search_tab(inner: &Inner) -> bool {
    let open = inner.editor.app_state().get("openSidebar");
    open.and_then(|o| o.get("name")).and_then(Value::as_str) == Some(DEFAULT_SIDEBAR_NAME)
        && open.and_then(|o| o.get("tab")).and_then(Value::as_str) == Some(CANVAS_SEARCH_TAB)
}

/// The search field of the mounted sidebar, if any.
fn search_input(inner: &Inner) -> Option<web_sys::HtmlInputElement> {
    inner
        .container
        .query_selector(&format!(".{SEARCH_MENU_INPUT_WRAPPER} input"))
        .ok()
        .flatten()
        .and_then(|el| el.dyn_into::<web_sys::HtmlInputElement>().ok())
}

/// The field's selection while it has the focus.
fn focused_selection(inner: &Inner) -> Option<(u32, u32)> {
    let input = search_input(inner)?;
    let active = inner.document().active_element()?;
    if !active.is_same_node(Some(&input)) {
        return None;
    }
    Some((input.selection_start().ok()??, input.selection_end().ok()??))
}

/// Applies the menu's effects to the editor; the timer is (re)started.
fn apply(weak: &Weak<RefCell<Inner>>, inner: &mut Inner, effects: Vec<SearchEffect>) {
    for effect in effects {
        match effect {
            SearchEffect::SetAppState(patch) => inner.editor.set_app_state(patch),
            SearchEffect::FocusMatches(index) => {
                let current = inner
                    .editor
                    .app_state()
                    .get("searchMatches")
                    .cloned()
                    .unwrap_or(Value::Null);
                if let Some(next) = focus_search_matches(&current, index) {
                    let mut patch = serde_json::Map::new();
                    patch.insert("searchMatches".into(), next);
                    inner.editor.set_app_state(patch);
                }
            }
            SearchEffect::SetViewport { target, fit } => {
                inner.editor.fit_bounds(target, fit, offsets());
            }
            SearchEffect::FocusInput => {
                if let Some(input) = search_input(inner) {
                    let _ = input.focus();
                    input.select();
                }
            }
            SearchEffect::ScheduleSearch => schedule(weak, inner),
        }
    }
    inner.search.generation += 1;
}

/// (Re)starts the debounce: [`run`] after [`SEARCH_DEBOUNCE_MS`].
fn schedule(weak: &Weak<RefCell<Inner>>, inner: &mut Inner) {
    let Some(window) = web_sys::window() else {
        return;
    };
    if let Some(id) = inner.search.timer.take() {
        window.clear_timeout_with_handle(id);
    }
    let weak = weak.clone();
    let run_it = Closure::once_into_js(move || run(&weak));
    inner.search.timer = window
        .set_timeout_with_callback_and_timeout_and_arguments_0(
            run_it.unchecked_ref(),
            SEARCH_DEBOUNCE_MS as i32,
        )
        .ok();
}

/// The debounced search runs.
fn run(weak: &Weak<RefCell<Inner>>) {
    let Some(rc) = weak.upgrade() else {
        return;
    };
    {
        let Ok(mut inner) = rc.try_borrow_mut() else {
            return;
        };
        inner.search.timer = None;
        if !inner.search.mounted {
            return;
        }
        let mut state = std::mem::take(&mut inner.search.state);
        let effects = inner
            .editor
            .with_search_context(offsets(), |cx| run_pending(&mut state, cx));
        inner.search.state = state;
        apply(weak, &mut inner, effects);
        inner.after_event();
    }
    refresh_chrome(weak);
}

/// The menu's DOM for the sidebar's search tab, mounting or unmounting the
/// menu as the tab opens or closes. Called as the sidebar renders again,
/// before the old sidebar is removed.
pub(super) fn render(weak: &Weak<RefCell<Inner>>, inner: &mut Inner) -> Option<Node> {
    let open = inner.ui != "none" && on_search_tab(inner);
    if !open {
        if inner.search.mounted {
            inner.search.mounted = false;
            if let (Some(window), Some(id)) = (web_sys::window(), inner.search.timer.take()) {
                window.clear_timeout_with_handle(id);
            }
            let effects = unmount(&mut inner.search.state);
            apply(weak, inner, effects);
        }
        return None;
    }
    let restore = focused_selection(inner);
    let nonce = inner
        .editor
        .with_search_context(offsets(), |cx| cx.scene_nonce);
    let focus = if !inner.search.mounted {
        inner.search.mounted = true;
        let effects = mount(&mut inner.search.state, nonce);
        apply(weak, inner, effects);
        InputFocus::Select
    } else if inner.editor.take_search_focus_request() {
        InputFocus::Select
    } else {
        let effects = sync(&mut inner.search.state, nonce);
        if !effects.is_empty() {
            apply(weak, inner, effects);
        }
        restore.map_or(InputFocus::None, |(s, e)| InputFocus::Restore(s, e))
    };
    let events = weak.clone();
    let on_event = Rc::new(move |event: SearchMenuEvent| dispatch(&events, event))
        as Rc<dyn Fn(SearchMenuEvent)>;
    Some(Node::Element(search_menu(SearchMenuProps {
        state: &inner.search.state,
        focus,
        on_event: Some(on_event),
    })))
}

/// A search menu event, run through the menu and applied to the editor.
fn dispatch(weak: &Weak<RefCell<Inner>>, event: SearchMenuEvent) {
    let Some(rc) = weak.upgrade() else {
        return;
    };
    {
        let Ok(mut inner) = rc.try_borrow_mut() else {
            return;
        };
        let mut state = std::mem::take(&mut inner.search.state);
        let effects = inner
            .editor
            .with_search_context(offsets(), |cx| update(&mut state, event, cx));
        inner.search.state = state;
        apply(weak, &mut inner, effects);
        inner.after_event();
    }
    refresh_chrome(weak);
}

/// The scene may have changed: the menu's search effect runs again.
pub(super) fn scene_changed(weak: &Weak<RefCell<Inner>>) {
    let Some(rc) = weak.upgrade() else {
        return;
    };
    let Ok(mut inner) = rc.try_borrow_mut() else {
        return;
    };
    if !inner.search.mounted {
        return;
    }
    let nonce = inner
        .editor
        .with_search_context(offsets(), |cx| cx.scene_nonce);
    let effects = sync(&mut inner.search.state, nonce);
    if !effects.is_empty() {
        apply(weak, &mut inner, effects);
    }
}

/// The menu's window `keydown` listener (capture phase,
/// `SearchMenu.tsx:279-338`), while it is mounted.
pub(super) fn window_key_down(rc: &Rc<RefCell<Inner>>, event: &KeyboardEvent) {
    let weak = Rc::downgrade(rc);
    let handled = {
        let Ok(mut inner) = rc.try_borrow_mut() else {
            return;
        };
        if !inner.search.mounted {
            return;
        }
        let stroke = keystroke(event, None);
        let in_search = event
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            .and_then(|t| t.closest(".layer-ui__search").ok().flatten())
            .is_some();
        let input_focused = focused_selection(&inner).is_some();
        let app = inner.editor.app_state();
        let set = |k: &str| app.get(k).is_some_and(|v| !v.is_null());
        let (open_dialog, open_popup) = (set("openDialog"), set("openPopup"));
        let key = SearchKey {
            key: &stroke.key,
            ctrl_or_cmd: stroke.modifiers.ctrl_or_cmd(is_darwin()),
            in_search,
            input_focused,
            open_dialog,
            open_popup,
        };
        let mut state = std::mem::take(&mut inner.search.state);
        let out = inner
            .editor
            .with_search_context(offsets(), |cx| key_down(&mut state, &key, cx));
        inner.search.state = state;
        if out.prevent_default {
            event.prevent_default();
        }
        if out.stop_propagation {
            event.stop_propagation();
        }
        let handled = !out.effects.is_empty();
        apply(&weak, &mut inner, out.effects);
        if handled {
            inner.after_event();
        }
        handled
    };
    if handled {
        refresh_chrome(&weak);
    }
}
