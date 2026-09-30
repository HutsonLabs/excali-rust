//! What the library's header menu asks of the host
//! (`LibraryMenuHeaderContent.tsx:37-190`): the file its Open picks and
//! merges, the file its Save to... writes (`saveLibraryAsJSON`,
//! `data/json.ts:147-160`), the publish dialog's saved fields
//! (EditorLocalStorage) and its submission, and the menu's dialogs
//! (`excali_ui::library_sidebar::library_dialogs`), portalled to the body.
//!
//! Open is `fileOpen` without `showOpenFilePicker`: browser-fs-access's
//! `<input type="file">` fallback, which every webview has; Save to... its
//! `<a download="library.excalidrawlib">` fallback. The submission goes to
//! the host as a cancelable `library-publish` event (`detail`: the
//! `.excalidrawlib` text as `library`, the form's text `fields` in
//! upstream's order, `respond(url)` and `reject(error)`), which the host
//! sends to a library backend of its choosing: the port never posts to
//! upstream's. Upstream's JPEG preview of the items (`generatePreviewImage`)
//! is not made.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use excali_core::library::LibraryItem;
use excali_scene::shape::Theme;
use excali_ui::dom::Node;
use excali_ui::library_sidebar::{
    library_dialogs, library_text, publish_form_fields, LibraryDialogsProps, LibraryEffect,
    LibraryMenuState, LibrarySidebarEvent, PublishLibraryData, PUBLISH_LIBRARY_STORAGE_KEY,
};
use serde_json::Value;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};

use super::{library_context, library_event, preview_node, refresh_chrome, Inner};

/// `MIME_TYPES.excalidrawlib` (`common/src/constants.ts`).
const MIME_EXCALIDRAWLIB: &str = "application/vnd.excalidrawlib+json";

/// What the dialogs are mounted from: they mount again only when it
/// changes, not as their fields are typed in (the DOM holds those).
pub(super) fn dialogs_key(state: &LibraryMenuState) -> Value {
    serde_json::json!({
        "confirm": state.confirm_open.then_some(state.selected_items.len()),
        "publish": state.publish.as_ref().map(|p| serde_json::json!({
            "items": state.selected_items,
            "errors": p.errors.iter().collect::<std::collections::BTreeMap<_, _>>(),
            "submitting": p.submitting,
        })),
        "success": state.publish_success.as_ref().map(|s| [s.url.clone(), s.author_name.clone()]),
    })
}

/// Mounts the header menu's open dialogs in the body.
pub(super) fn render_library_dialogs(weak: &Weak<RefCell<Inner>>) -> Result<(), JsValue> {
    let Some(rc) = weak.upgrade() else {
        return Ok(());
    };
    let mut inner = rc.borrow_mut();
    let key = dialogs_key(&inner.library_menu);
    if inner.library_dialogs_key.as_ref() == Some(&key) {
        return Ok(());
    }
    inner.library_dialogs_key = Some(key);
    for old in std::mem::take(&mut inner.library_dialogs) {
        old.close();
    }
    let items = inner.editor.library().to_vec();
    let pending = inner.editor.pending_library_elements();
    let previews: std::collections::HashMap<String, Node> = match &inner.library_menu.publish {
        Some(_) => items
            .iter()
            .filter(|i| inner.library_menu.selected_items.contains(&i.id))
            .map(|i| {
                (
                    i.id.clone(),
                    preview_node(inner.editor.publish_item_svg(&i.elements)),
                )
            })
            .collect(),
        None => Default::default(),
    };
    let theme = if inner
        .editor
        .app_state()
        .get("theme")
        .and_then(Value::as_str)
        == Some("dark")
    {
        Theme::Dark
    } else {
        Theme::Light
    };
    let events = weak.clone();
    let on_event = Rc::new(move |event: LibrarySidebarEvent| dispatch(&events, event))
        as Rc<dyn Fn(LibrarySidebarEvent)>;
    let dialogs = {
        let cx = library_context(&inner, &items, &pending);
        library_dialogs(LibraryDialogsProps {
            context: cx,
            state: &inner.library_menu,
            theme,
            container_id: "excali-editor".into(),
            previews: &previews,
            on_event: Some(on_event),
        })
    };
    let document = inner.document();
    for dialog in dialogs {
        let open = excali_ui::primitives::open_modal(&document, dialog)?;
        inner.library_dialogs.push(open);
    }
    Ok(())
}

/// Runs a sidebar or dialog event through the editor, then what it asks of
/// the host, and renders again.
pub(super) fn dispatch(weak: &Weak<RefCell<Inner>>, event: LibrarySidebarEvent) {
    let Some(rc) = weak.upgrade() else {
        return;
    };
    let jobs = {
        let Ok(mut inner) = rc.try_borrow_mut() else {
            return;
        };
        let jobs = library_event(&mut inner, event);
        inner.after_event();
        jobs
    };
    refresh_chrome(weak);
    for job in jobs {
        run_job(weak, job);
    }
}

/// The effects [`library_event`] leaves until the chrome has rendered
/// again: the ones that wait on the user or the network, and the focus
/// (upstream's sidebar stays mounted; a sidebar mounted again focuses its
/// search field).
pub(super) fn is_job(effect: &LibraryEffect) -> bool {
    matches!(
        effect,
        LibraryEffect::FocusContainer
            | LibraryEffect::LoadLibrary
            | LibraryEffect::ExportLibrary(_)
            | LibraryEffect::SubmitLibrary { .. }
    )
}

fn run_job(weak: &Weak<RefCell<Inner>>, job: LibraryEffect) {
    let result = match job {
        LibraryEffect::FocusContainer => match weak.upgrade() {
            Some(rc) => rc.borrow().container.focus(),
            None => Ok(()),
        },
        LibraryEffect::LoadLibrary => open_library_file(weak),
        LibraryEffect::ExportLibrary(ids) => save_library_file(weak, &ids),
        LibraryEffect::SubmitLibrary { items, data } => submit_library(weak, &items, &data),
        _ => Ok(()),
    };
    if let Err(e) = result {
        web_sys::console::warn_1(&e);
    }
}

/// Sets `appState.errorMessage`.
fn set_error(weak: &Weak<RefCell<Inner>>, message: &str) {
    let Some(rc) = weak.upgrade() else {
        return;
    };
    {
        let Ok(mut inner) = rc.try_borrow_mut() else {
            return;
        };
        let patch = [("errorMessage".to_owned(), Value::String(message.to_owned()))]
            .into_iter()
            .collect();
        inner.editor.set_app_state(patch);
        inner.after_event();
    }
    refresh_chrome(weak);
}

/// `onLibraryImport`: `library.updateLibrary({ libraryItems: fileOpen(...),
/// merge: true, openLibraryMenu: true })`, `errors.importLibraryError` when
/// the file is no library; a cancelled pick does nothing.
fn open_library_file(weak: &Weak<RefCell<Inner>>) -> Result<(), JsValue> {
    let Some(rc) = weak.upgrade() else {
        return Ok(());
    };
    let document = rc.borrow().document();
    let input: web_sys::HtmlInputElement = document.create_element("input")?.unchecked_into();
    input.set_type("file");
    let events = weak.clone();
    let on_change = Closure::once_into_js(move |e: web_sys::Event| {
        let Some(file) = e
            .target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
            .and_then(|i| i.files())
            .and_then(|f| f.get(0))
        else {
            return;
        };
        wasm_bindgen_futures::spawn_local(async move {
            let text = wasm_bindgen_futures::JsFuture::from(file.text())
                .await
                .ok()
                .and_then(|t| t.as_string());
            let loaded = text.and_then(|text| {
                let rc = events.upgrade()?;
                let mut inner = rc.try_borrow_mut().ok()?;
                // updateLibrary opens the library tab first
                let patch = [(
                    "openSidebar".to_owned(),
                    serde_json::json!({"name": "default", "tab": "library"}),
                )]
                .into_iter()
                .collect();
                inner.editor.set_app_state(patch);
                let ok = inner.editor.import_library(&text, true).is_ok();
                inner.after_event();
                Some(ok)
            });
            refresh_chrome(&events);
            if loaded != Some(true) {
                set_error(&events, library_text("errors.importLibraryError"));
            }
        });
    });
    input.add_event_listener_with_callback("change", on_change.unchecked_ref())?;
    input.click();
    Ok(())
}

/// `onLibraryExport` and `saveLibraryAsJSON`: these items as
/// `library.excalidrawlib`.
fn save_library_file(weak: &Weak<RefCell<Inner>>, ids: &[String]) -> Result<(), JsValue> {
    let Some(rc) = weak.upgrade() else {
        return Ok(());
    };
    let (document, text) = {
        let inner = rc.borrow();
        let items: Vec<LibraryItem> = inner
            .editor
            .library()
            .iter()
            .filter(|i| ids.contains(&i.id))
            .cloned()
            .collect();
        (inner.document(), inner.editor.library_items_json(&items))
    };
    let parts = js_sys::Array::of1(&JsValue::from_str(&text));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(MIME_EXCALIDRAWLIB);
    let saved = web_sys::Blob::new_with_str_sequence_and_options(&parts, &options)
        .and_then(|blob| web_sys::Url::create_object_url_with_blob(&blob))
        .and_then(|url| {
            let a: web_sys::HtmlAnchorElement = document.create_element("a")?.unchecked_into();
            a.set_href(&url);
            a.set_download("library.excalidrawlib");
            a.click();
            // browser-fs-access revokes the url after a moment
            let revoke = Closure::once_into_js(move || {
                let _ = web_sys::Url::revoke_object_url(&url);
            });
            web_sys::window()
                .ok_or_else(|| JsValue::from_str("no window"))?
                .set_timeout_with_callback_and_timeout_and_arguments_0(
                    revoke.unchecked_ref(),
                    30_000,
                )?;
            Ok(())
        });
    if let Err(e) = saved {
        let message = js_sys::Reflect::get(&e, &JsValue::from_str("message"))
            .ok()
            .and_then(|m| m.as_string())
            .unwrap_or_else(|| "Couldn't save the library".to_owned());
        set_error(weak, &message);
    }
    Ok(())
}

/// `String(error)`.
fn js_string(value: &JsValue) -> String {
    js_sys::Function::new_with_args("e", "return String(e)")
        .call1(&JsValue::NULL, value)
        .ok()
        .and_then(|s| s.as_string())
        .unwrap_or_default()
}

/// PublishLibrary's `onSubmit` fetch, as the host's `library-publish`.
fn submit_library(
    weak: &Weak<RefCell<Inner>>,
    items: &[LibraryItem],
    data: &PublishLibraryData,
) -> Result<(), JsValue> {
    let Some(rc) = weak.upgrade() else {
        return Ok(());
    };
    let (text, dispatch) = {
        let inner = rc.borrow();
        (
            inner.editor.library_items_json(items),
            inner.dispatch.clone(),
        )
    };
    let answered = Rc::new(std::cell::Cell::new(false));
    let fields = js_sys::Array::new();
    for (name, value) in publish_form_fields(data) {
        fields.push(&js_sys::Array::of2(
            &JsValue::from_str(name),
            &JsValue::from_str(&value),
        ));
    }
    let respond = {
        let events = weak.clone();
        let answered = answered.clone();
        Closure::once_into_js(move |url: JsValue| {
            if answered.replace(true) {
                return;
            }
            let url = url.as_string().unwrap_or_else(|| js_string(&url));
            dispatch_later(&events, LibrarySidebarEvent::PublishSucceeded { url });
        })
    };
    let reject = {
        let events = weak.clone();
        let answered = answered.clone();
        Closure::once_into_js(move |error: JsValue| {
            if answered.replace(true) {
                return;
            }
            dispatch_later(
                &events,
                LibrarySidebarEvent::PublishFailed(js_string(&error)),
            );
        })
    };
    let detail = js_sys::Object::new();
    js_sys::Reflect::set(&detail, &"library".into(), &JsValue::from_str(&text))?;
    js_sys::Reflect::set(&detail, &"fields".into(), &fields)?;
    js_sys::Reflect::set(&detail, &"respond".into(), &respond)?;
    js_sys::Reflect::set(&detail, &"reject".into(), &reject)?;
    // the shim's dispatch returns dispatchEvent's result: false once the
    // host called preventDefault()
    let not_handled = dispatch
        .call2(
            &JsValue::NULL,
            &JsValue::from_str("library-publish"),
            &detail,
        )?
        .as_bool()
        .unwrap_or(true);
    if not_handled && !answered.replace(true) {
        dispatch_later(
            weak,
            LibrarySidebarEvent::PublishFailed(
                "Error: The host did not handle library-publish.".to_owned(),
            ),
        );
    }
    Ok(())
}

/// [`dispatch`] after the current task: the host may answer inside the
/// event it handles, while the editor is still borrowed.
fn dispatch_later(weak: &Weak<RefCell<Inner>>, event: LibrarySidebarEvent) {
    let events = weak.clone();
    let run = Closure::once_into_js(move || dispatch(&events, event));
    if let Some(window) = web_sys::window() {
        let _ = window.set_timeout_with_callback(run.unchecked_ref());
    }
}

/// `EditorLocalStorage.get(EDITOR_LS_KEYS.PUBLISH_LIBRARY)`.
pub(super) fn load_publish_data() -> Option<PublishLibraryData> {
    let storage = web_sys::window()?.local_storage().ok()??;
    let text = storage.get_item(PUBLISH_LIBRARY_STORAGE_KEY).ok()??;
    PublishLibraryData::from_json(&serde_json::from_str(&text).ok()?)
}

/// `EditorLocalStorage.set` (or `.delete` for `None`).
pub(super) fn save_publish_data(data: Option<&PublishLibraryData>) {
    let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) else {
        return;
    };
    let _ = match data {
        Some(data) => storage.set_item(PUBLISH_LIBRARY_STORAGE_KEY, &data.to_json().to_string()),
        None => storage.remove_item(PUBLISH_LIBRARY_STORAGE_KEY),
    };
}

/// `window.alert(message)`.
pub(super) fn alert(message: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.alert_with_message(message);
    }
}
