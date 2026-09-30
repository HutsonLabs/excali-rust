//! `<excali-editor>`'s DOM side: [`EditorCore`], which the custom element
//! shim (`js/excali-editor.js`, appended to `excali_editor.js` by
//! `scripts/web/build.sh`) creates in `connectedCallback`.
//!
//! The core mounts upstream's container (`div.excalidraw
//! .excalidraw-container`, focusable, `theme--dark` in the dark theme) in
//! the host element, the layered canvases in it (`excali_ui::layers`) and
//! the desktop toolbar (`excali_ui::toolbar`) with the hint viewer
//! (`excali_ui::hints`), the welcome screen on an empty scene
//! (`excali_ui::welcome_screen`), the cursor hint after an arrow or line
//! shortcut, and the library sidebar with
//! LayerUI's trigger for it (`excali_ui::library_sidebar`), into which
//! library items dragged onto the canvas drop, with its header menu's
//! files and dialogs (`library_menu`). Keys are listened to on the
//! container, as `Excalidraw` does with `handleKeyboardGlobally` off (its
//! default); pointer presses on the interactive canvas, which captures the
//! pointer until the release. Each event goes to the [`Editor`], then the
//! static canvas is painted again and the editor's [`HostEvent`]s are
//! dispatched on the host element through the shim's `dispatch` function.
//!
//! Text is measured with a canvas context's `measureText`, as upstream's
//! `CanvasTextMetricsProvider` does (`textMeasurements.ts`).

use std::cell::RefCell;
use std::rc::{Rc, Weak};

mod library_menu;
mod search;

use excali_canvas2d::{paint, WebCanvas};
use excali_core::png::{encode_chunks, encode_text_chunk, extract_chunks};
use excali_editor::actions::{
    build_context_menu, get_context_menu_items, ActionContext, ActionName, ContextMenuKind,
    KeyLabels,
};
use excali_editor::keyboard::{
    command_palette_key_down, is_command_palette_toggle_shortcut, ClipboardEventKind,
    ClipboardOutcome, KeyEffect,
};
use excali_editor::tools::{
    ArrowType, SetActiveToolOptions, Tool, ToolKeyOutcome, ToolRequest, ToolState,
};
use excali_scene::display::FontFaceSource;
use excali_scene::shape::Theme;
use excali_svg::FontContent;
use excali_text::text_measurements::TextMetricsProvider;
use excali_ui::command_palette::{
    command_list, command_palette, hosted_app_links, library_commands, palette_commands,
    palette_key_down, palette_view, perform_command, CommandPaletteProps, PaletteCommand,
    PaletteEffect, PaletteEnv, PaletteView,
};
use excali_ui::context_menu::{
    context_menu, ContextMenuEffect, ContextMenuProps, OnContextMenuEffect,
};
use excali_ui::dom::{mount, Mounted, Node};
use excali_ui::footer::{footer, FooterControl, FooterProps, OnFooterEvent};
use excali_ui::help_dialog::{close_help_dialog, help_dialog, HelpDialogProps, Platform};
use excali_ui::hints::{
    cursor_hint, hint_viewer, position_element_beside_cursor, ContainerRect, CursorHints,
    HintContext, CURSOR_HINT_DURATION, CURSOR_HINT_FADE_DURATION, CURSOR_HINT_GAP,
};
use excali_ui::keyboard::{apply_outcome, clipboard_target, keystroke};
use excali_ui::layers::{CanvasLayers, Layer};
use excali_ui::library_sidebar::{
    default_sidebar, default_sidebar_trigger, dropped_item_ids, is_sidebar_docked_and_fits,
    update as update_library, BrowseLink, LibraryContext, LibraryEffect, LibraryMenuState,
    LibrarySidebarEvent, LibrarySidebarProps, LibraryStatus, OpenSidebar, Previews,
    SidebarTriggerProps,
};
use excali_ui::main_menu::{default_main_menu, Dispatch, MenuContext, MenuEffect, ThemeChoice};
use excali_ui::text_editor::{
    measure_caret_offset, Handled, TextEditorOverlay, TextareaEvent, TextareaHandler,
    TEXTAREA_ATTRIBUTES, TEXT_EDITOR_CSS,
};
use excali_ui::theme::{apply_container_tokens, apply_theme};
use excali_ui::toolbar::{
    activate_extra_tool, activate_tool_button, install_stylesheet, toolbar, ToolbarEvent,
    ToolbarProps,
};
use excali_ui::welcome_screen::{
    help_hint, menu_hint, toolbar_hint, welcome_screen_center, WelcomeScreenEvent,
    WelcomeScreenProps,
};
use serde_json::Value;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{
    AddEventListenerOptions, CanvasRenderingContext2d, Document, Event, HtmlCanvasElement,
    HtmlElement, KeyboardEvent, PointerEvent, WheelEvent,
};

use crate::editor::{
    library_source, Editor, ExportOptions, HostEvent, LibrarySource, PointerInput, WheelInput,
};
use crate::env::EditorEnv;

/// The element's own rules: the host is a positioned block filling its
/// parent, as the `Excalidraw` component fills its own, the container
/// fills it, and the toolbar island sits centred at the top, as upstream's
/// `App-menu_top` places it (`LayerUI.tsx`, `css/styles.scss`); a
/// dialog's portal container in the body has upstream's `.excalidraw` box
/// (`css/styles.scss:40-60`), so its modal covers the page. Every
/// `.excalidraw` has upstream's UI font and text colour (`styles.scss:
/// 41-54`), which the chrome's own rules inherit.
pub const ELEMENT_CSS: &str = "\
.excalidraw {
  --ui-font: Assistant, system-ui, BlinkMacSystemFont, -apple-system, Segoe UI,
    Roboto, Helvetica, Arial, sans-serif;
  --viewport-status-frame-border-width: 0px;
  font-family: var(--ui-font);
  color: var(--text-primary-color);
}
excali-editor {
  display: block;
  position: relative;
  overflow: hidden;
  width: 100%;
  height: 100%;
}
excali-editor > .excalidraw-container {
  position: absolute;
  inset: 0;
  outline: none;
}
excali-editor .excali-editor__top {
  position: absolute;
  top: var(--editor-container-padding, 1rem);
  left: 0;
  right: 0;
  display: flex;
  justify-content: center;
  pointer-events: none;
  z-index: 3;
}
excali-editor .excali-editor__top > * {
  pointer-events: all;
}
excali-editor .excali-editor__top-right {
  position: absolute;
  top: var(--editor-container-padding, 1rem);
  right: var(--editor-container-padding, 1rem);
  z-index: 3;
}
excali-editor .excali-editor__top-left {
  position: absolute;
  top: var(--editor-container-padding, 1rem);
  left: var(--editor-container-padding, 1rem);
  z-index: 4;
}
.excalidraw.excalidraw-modal-container {
  overflow: hidden;
  color: var(--text-primary-color);
  display: flex;
  top: 0;
  bottom: 0;
  left: 0;
  right: 0;
  height: 100%;
  width: 100%;
}
";

/// Every stylesheet the element needs, in the order it installs them:
/// the primitives' (upstream's `theme.scss` tokens with them), the
/// toolbar's, the canvases' and the element's. The release ships the same
/// text as `excali.css` (`crates/excali-wasm/excali.css`, checked by
/// `tests/stylesheet.rs`).
pub fn stylesheet() -> String {
    [
        excali_ui::primitives::PRIMITIVES_CSS,
        excali_ui::toolbar::TOOLBAR_CSS,
        excali_ui::footer::FOOTER_CSS,
        excali_ui::help_dialog::HELP_DIALOG_CSS,
        excali_ui::command_palette::COMMAND_PALETTE_CSS,
        excali_ui::hints::HINTS_CSS,
        excali_ui::welcome_screen::WELCOME_SCREEN_CSS,
        excali_ui::main_menu::MAIN_MENU_CSS,
        excali_ui::context_menu::CONTEXT_MENU_CSS,
        excali_ui::convert_popup::CONVERT_POPUP_CSS,
        excali_ui::library_sidebar::LIBRARY_SIDEBAR_CSS,
        excali_ui::search_menu::SEARCH_MENU_CSS,
        excali_ui::layers::CANVAS_LAYER_CSS,
        excali_ui::icons::ICONS_CSS,
        excali_ui::accessibility::ACCESSIBILITY_CSS,
        TEXT_EDITOR_CSS,
        ELEMENT_CSS,
    ]
    .join("\n")
}

fn install_element_stylesheet(document: &Document) -> Result<(), JsValue> {
    install_stylesheet(document)?;
    excali_ui::footer::install_stylesheet(document)?;
    excali_ui::help_dialog::install_stylesheet(document)?;
    excali_ui::command_palette::install_stylesheet(document)?;
    excali_ui::hints::install_stylesheet(document)?;
    excali_ui::welcome_screen::install_stylesheet(document)?;
    excali_ui::main_menu::install_stylesheet(document)?;
    excali_ui::context_menu::install_stylesheet(document)?;
    excali_ui::library_sidebar::install_stylesheet(document)?;
    excali_ui::search_menu::install_stylesheet(document)?;
    if document
        .query_selector("style[data-excali-ui=\"excali-editor\"]")?
        .is_some()
    {
        return Ok(());
    }
    let style = document.create_element("style")?;
    style.set_attribute("data-excali-ui", "excali-editor")?;
    style.set_text_content(Some(
        &[
            excali_ui::layers::CANVAS_LAYER_CSS,
            excali_ui::icons::ICONS_CSS,
            excali_ui::accessibility::ACCESSIBILITY_CSS,
            TEXT_EDITOR_CSS,
            ELEMENT_CSS,
        ]
        .join("\n"),
    ));
    let head = document
        .head()
        .ok_or_else(|| JsValue::from_str("the document has no head"))?;
    head.append_child(&style)?;
    Ok(())
}

/// `canvas.getContext("2d").measureText(text).width` with `font` set.
#[derive(Clone)]
pub struct CanvasMetrics {
    context: CanvasRenderingContext2d,
    font: Rc<RefCell<String>>,
}

impl CanvasMetrics {
    pub fn new(document: &Document) -> Result<CanvasMetrics, JsValue> {
        let canvas: HtmlCanvasElement = document.create_element("canvas")?.dyn_into()?;
        let context: CanvasRenderingContext2d = canvas
            .get_context("2d")?
            .ok_or_else(|| JsValue::from_str("no 2d context"))?
            .dyn_into()?;
        Ok(CanvasMetrics {
            context,
            font: Rc::default(),
        })
    }
}

impl TextMetricsProvider for CanvasMetrics {
    fn get_line_width(&self, text: &str, font: &str) -> f64 {
        let mut current = self.font.borrow_mut();
        if *current != font {
            self.context.set_font(font);
            font.clone_into(&mut current);
        }
        self.context.measure_text(text).map_or(0.0, |m| m.width())
    }
}

/// An SVG export's `@font-face` sources: the release's font files by URL
/// (upstream writes the file's URL when it cannot inline it,
/// `ExcalidrawFontFace.getContent`); the export is synchronous, so nothing
/// is fetched to inline.
struct FontUrls<'a>(&'a str);

impl FontContent for FontUrls<'_> {
    fn content(&self, face: &FontFaceSource) -> String {
        format!("{}{}", self.0, face.file)
    }
}

fn js_err(e: impl std::fmt::Display) -> JsValue {
    JsError::new(&e.to_string()).into()
}

type Listener = (
    web_sys::EventTarget,
    &'static str,
    Closure<dyn FnMut(Event)>,
);

struct Inner {
    editor: Editor<CanvasMetrics>,
    host: HtmlElement,
    container: HtmlElement,
    layers: CanvasLayers,
    /// `.layer-ui__wrapper` (`LayerUI.tsx:647-660`): the welcome screen's
    /// centre, the top sections and the footer, in that order, which is
    /// the order Tab walks them.
    layer_ui: HtmlElement,
    top: HtmlElement,
    toolbar: Option<Mounted>,
    footer: Option<Mounted>,
    /// The help dialog, portalled to the body while `appState.openDialog`
    /// is `{name: "help"}`.
    help_dialog: Option<excali_ui::primitives::OpenModal>,
    /// The command palette, portalled to the body while
    /// `appState.openDialog` is `{name: "commandPalette"}`.
    palette: Option<PaletteSession>,
    /// The palette's last run command, by label (`lastUsedPaletteItem`).
    palette_last_used: Option<String>,
    /// The top-left corner (`App-menu_top__left`) and the main menu in it,
    /// with the welcome screen's menu hint under it.
    top_left: HtmlElement,
    main_menu: Option<Mounted>,
    menu_hint: Option<Mounted>,
    /// The welcome screen's centre (`WelcomeScreenCenterTunnel.Out`).
    welcome_center: Option<Mounted>,
    /// The convert element type popup, the panel it shows, and whether a
    /// click on it asked for the focus (`panelRef.current?.focus()`).
    convert_popup: Option<(Mounted, Value)>,
    focus_convert_popup: bool,
    /// The hint the toolbar last showed ([`current_hint`]).
    hint: Option<String>,
    /// The cursor hint's policy and the hint shown, with the nonce its
    /// timers check.
    cursor_hints: CursorHints,
    cursor_hint: Option<(Mounted, u32)>,
    cursor_hint_nonce: u32,
    /// The open context menu (`appState.contextMenu`).
    context_menu: Option<Mounted>,
    /// The top-right corner, where LayerUI puts the library trigger.
    top_right: HtmlElement,
    /// The library trigger and the default sidebar, and the library menu's
    /// own state.
    library_trigger: Option<Mounted>,
    sidebar: Option<Mounted>,
    library_menu: LibraryMenuState,
    /// The search tab's menu (`SearchMenu`).
    search: search::SearchSession,
    /// An animation frame of the viewport navigation is requested.
    viewport_frame_pending: bool,
    /// The header menu's open dialogs, portalled to the body, and what
    /// they were mounted from (`library_menu::dialogs_key`).
    library_dialogs: Vec<excali_ui::primitives::OpenModal>,
    library_dialogs_key: Option<Value>,
    /// Each library item's preview, by id, with the elements it shows.
    previews: std::collections::HashMap<String, (u64, String)>,
    /// The text editor's box (`.excalidraw-textEditorContainer`) and the
    /// textarea mounted in it while a text is edited.
    editor_box: HtmlElement,
    overlay: Option<TextEditorOverlay>,
    extra_tools_open: bool,
    /// What the chrome was last rendered from ([`chrome_key`]).
    chrome_key: Option<Value>,
    ui: String,
    dispatch: js_sys::Function,
    fonts_base: String,
    listeners: Vec<Listener>,
}

impl Inner {
    fn document(&self) -> Document {
        self.host
            .owner_document()
            .expect("the host is in a document")
    }

    /// The canvas size and page offset into the app state, when they moved.
    fn measure(&mut self) {
        let rect = self.container.get_bounding_client_rect();
        let (w, h) = (rect.width(), rect.height());
        if self.layers.css_size() != (w, h) {
            let scale = self.layers.device_pixel_ratio();
            self.layers.resize(w, h, scale);
        }
        let app = self.editor.app_state();
        let now = [
            app.get("width").and_then(Value::as_f64),
            app.get("height").and_then(Value::as_f64),
            app.get("offsetLeft").and_then(Value::as_f64),
            app.get("offsetTop").and_then(Value::as_f64),
        ];
        let want = [Some(w), Some(h), Some(rect.left()), Some(rect.top())];
        if now != want {
            self.editor.set_viewport(w, h, rect.left(), rect.top());
        }
    }

    fn render(&mut self) {
        let size = self.layers.backing_size(Layer::Static);
        let list = self.editor.static_scene(
            f64::from(size.width),
            f64::from(size.height),
            self.layers.scale(),
        );
        let background = self
            .editor
            .app_state()
            .view_background_color()
            .map(str::to_owned);
        self.layers.paint_static(background.as_deref(), &list);
        // the interactive canvas (renderInteractiveScene), in the
        // container's `--color-selection` (getSelectionColor)
        let selection_color = self
            .container
            .owner_document()
            .and_then(|d| d.default_view())
            .and_then(|w| w.get_computed_style(&self.container).ok().flatten())
            .and_then(|style| style.get_property_value("--color-selection").ok())
            .map(|c| c.trim().to_owned())
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| {
                excali_editor::interactive_scene::DEFAULT_SELECTION_COLOR.to_owned()
            });
        let size = self.layers.backing_size(Layer::Interactive);
        let interactive = self.editor.interactive_scene(
            f64::from(size.width),
            f64::from(size.height),
            self.layers.scale(),
            &selection_color,
        );
        self.layers.paint_interactive(&interactive);
    }

    /// Paints the scene and the chrome again and dispatches the editor's
    /// events.
    fn after_event(&mut self) {
        self.render();
        self.flush();
    }

    /// The editor's events, dispatched on the host.
    fn flush(&mut self) {
        for event in self.editor.take_events() {
            let (name, detail) = match event {
                HostEvent::Change { dirty } => ("change", serde_json::json!({ "dirty": dirty })),
                HostEvent::SaveRequest => ("save-request", serde_json::json!({})),
                HostEvent::OpenLink { href } => ("open-link", serde_json::json!({ "href": href })),
            };
            let detail = js_sys::JSON::parse(&detail.to_string()).unwrap_or(JsValue::NULL);
            let _ = self
                .dispatch
                .call2(&JsValue::NULL, &JsValue::from_str(name), &detail);
        }
    }
}

/// What the chrome shows: the tools, the extra tools menu, the zoom, the
/// history stacks and the `ui` attribute.
fn chrome_key(inner: &Inner) -> Value {
    let ed = &inner.editor;
    serde_json::json!({
        "tool": ed.tools().active_tool.to_json(),
        "locked": ed.tools().is_tool_locked(),
        "extra": inner.extra_tools_open,
        "zoom": ed.app_state().zoom().unwrap_or(1.0),
        "undo": ed.can_undo(),
        "redo": ed.can_redo(),
        "ui": inner.ui,
        "openMenu": ed.app_state().get("openMenu"),
        "openDialog": ed.app_state().get("openDialog"),
        "theme": ed.app_state().get("theme"),
        "openSidebar": ed.app_state().get("openSidebar"),
        "sidebarDocked": ed.app_state().get("defaultSidebarDockedPreference"),
        "selection": ed.app_state().get("selectedElementIds"),
        "library": excali_core::library::library_items_hash(ed.library()),
        "libraryItems": ed.library().len(),
        "libraryMenu": library_menu_key(&inner.library_menu),
        "search": inner.search.generation,
        "canFitSidebar": can_fit_sidebar(inner),
        "hint": current_hint(inner),
        "welcome": render_welcome_screen(inner),
    })
}

/// `app.scene.getSelectedElements(appState)`: the non-deleted elements
/// `selectedElementIds` holds.
fn selected_elements(inner: &Inner) -> Vec<excali_core::element::Element> {
    let ids = inner.editor.app_state().get("selectedElementIds");
    inner
        .editor
        .elements()
        .iter()
        .filter(|e| {
            !e.base.is_deleted
                && ids
                    .and_then(|ids| ids.get(&e.base.id))
                    .is_some_and(|v| v.as_bool() == Some(true))
        })
        .cloned()
        .collect()
}

/// The hint viewer's element for the editor's state (`LayerUI` renders it
/// with `isMobile` from the phone form factor, which this layout is not).
fn hint_node(inner: &Inner) -> Option<excali_ui::dom::Element> {
    let selected = selected_elements(inner);
    let ctx = HintContext {
        app_state: inner.editor.app_state(),
        selected_elements: &selected,
        is_mobile: false,
        can_fit_sidebar: can_fit_sidebar(inner),
        grid_mode_enabled: None,
        active_resize_handle: inner.editor.active_resize_handle(),
    };
    hint_viewer(&ctx, is_darwin())
}

/// The hint text the viewer shows, `None` for none: what the toolbar is
/// re-rendered on.
fn current_hint(inner: &Inner) -> Option<String> {
    hint_node(inner).map(|el| Node::Element(el).to_html())
}

/// `renderWelcomeScreen` (`App.tsx:2516-2522`): not loading (the core
/// loads synchronously), `showWelcomeScreen` (which `componentDidUpdate`
/// turns on whenever the scene has no elements, `App.tsx:4342-4344`), the
/// preferred selection tool active, not in zen mode, and no elements,
/// deleted ones included.
fn render_welcome_screen(inner: &Inner) -> bool {
    let ed = &inner.editor;
    let empty = ed.elements().is_empty();
    let show = empty
        || ed
            .app_state()
            .get("showWelcomeScreen")
            .and_then(Value::as_bool)
            == Some(true);
    let tools = ed.tools();
    let preferred =
        tools.active_tool.tool.builtin() == Some(tools.preferred_selection_tool.tool.tool_type());
    let zen = ed
        .app_state()
        .get("zenModeEnabled")
        .and_then(Value::as_bool)
        == Some(true);
    inner.ui != "none" && show && preferred && !zen && empty
}

fn library_menu_key(state: &LibraryMenuState) -> Value {
    serde_json::json!({
        "selected": state.selected_items,
        "search": state.search,
        "hovered": format!("{:?}", state.hovered),
        "menuOpen": state.menu_open,
        "tabStop": state.tab_stop,
        "rerendered": state.rerendered,
        "dialogs": library_menu::dialogs_key(state),
    })
}

/// `MQ_RIGHT_SIDEBAR_MIN_WIDTH` (`common/src/editorInterface.ts:32`).
const MQ_RIGHT_SIDEBAR_MIN_WIDTH: f64 = 1229.0;

/// `editorInterface.canFitSidebar`: the editor is wider than
/// [`MQ_RIGHT_SIDEBAR_MIN_WIDTH`] (`App.tsx:3772-3781`).
fn can_fit_sidebar(inner: &Inner) -> bool {
    inner.container.get_bounding_client_rect().width() > MQ_RIGHT_SIDEBAR_MIN_WIDTH
}

/// The textarea's events, handed to the editor.
struct TextareaBridge {
    inner: Weak<RefCell<Inner>>,
}

impl TextareaHandler for TextareaBridge {
    fn on_event(&mut self, event: TextareaEvent) -> Handled {
        let Some(rc) = self.inner.upgrade() else {
            return Handled::default();
        };
        // an event fired while the element is busy (the textarea's removal)
        let Ok(mut inner) = rc.try_borrow_mut() else {
            return Handled::default();
        };
        let handled = inner.editor.textarea_event(event);
        inner.after_event();
        let closed = handled.state.as_ref().is_some_and(|s| !s.open);
        drop(inner);
        if closed {
            // unmounted once this event is over (its listener is the
            // overlay's); the container takes the focus back
            let weak = self.inner.clone();
            let later = Closure::once_into_js(move || {
                let Some(rc) = weak.upgrade() else {
                    return;
                };
                let mut inner = rc.borrow_mut();
                if inner.editor.textarea().is_none() {
                    if let Some(overlay) = inner.overlay.take() {
                        overlay.unmount();
                    }
                }
                let _ = inner.container.focus();
                drop(inner);
                refresh_chrome(&weak);
            });
            if let Some(window) = web_sys::window() {
                let _ = window.set_timeout_with_callback(later.unchecked_ref());
            }
        }
        handled
    }
}

/// Mounts the textarea when the editor opened a text editor, unmounts it
/// when the editor closed, and lays it out again after anything else
/// moved the scene or the viewport under it.
fn sync_text_editor(weak: &Weak<RefCell<Inner>>) {
    let Some(rc) = weak.upgrade() else {
        return;
    };
    let mut inner = rc.borrow_mut();
    match (inner.editor.textarea().is_some(), inner.overlay.is_some()) {
        (true, false) => {
            let Some(state) = inner.editor.text_editor_app_changed() else {
                return;
            };
            let handler: Rc<RefCell<dyn TextareaHandler>> = Rc::new(RefCell::new(TextareaBridge {
                inner: weak.clone(),
            }));
            let canvas: Option<web_sys::Element> = inner
                .layers
                .canvas(Layer::Interactive)
                .map(|c| c.clone().into());
            let Ok(overlay) =
                TextEditorOverlay::mount(&inner.editor_box, canvas.as_ref(), &state, handler)
            else {
                return;
            };
            if let Some(request) = inner.editor.caret_request() {
                let offset = measure_caret_offset(&inner.document(), &request);
                inner.editor.resolve_caret(offset);
                if let Some(state) = inner.editor.textarea() {
                    overlay.apply(&state);
                }
            }
            inner.overlay = Some(overlay);
        }
        (false, true) => {
            if let Some(overlay) = inner.overlay.take() {
                overlay.unmount();
            }
        }
        (true, true) => {
            if let Some(state) = inner.editor.text_editor_app_changed() {
                if let Some(overlay) = &inner.overlay {
                    overlay.apply(&state);
                }
            }
        }
        (false, false) => {}
    }
}

/// Re-mounts the chrome (the toolbar and the footer) when what it shows
/// changed since the last time, and keeps the text editor's textarea in
/// step with the editor.
fn refresh_chrome(weak: &Weak<RefCell<Inner>>) {
    sync_text_editor(weak);
    search::scene_changed(weak);
    let _ = render_convert_popup(weak);
    let Some(rc) = weak.upgrade() else {
        return;
    };
    let key = chrome_key(&rc.borrow());
    if rc.borrow().chrome_key.as_ref() == Some(&key) {
        return;
    }
    rc.borrow_mut().chrome_key = Some(key);
    let _ = render_toolbar(weak);
    let _ = render_footer(weak);
    let _ = render_main_menu(weak);
    let _ = render_library_sidebar(weak);
    let _ = library_menu::render_library_dialogs(weak);
    let _ = render_help_dialog(weak);
    let _ = render_command_palette(weak);
    let _ = render_welcome_center(weak);
    if let Some(rc) = weak.upgrade() {
        let _ = excali_ui::accessibility::name_controls(&rc.borrow().container);
    }
}

/// The convert element type popup (`App.tsx:2770-2774`): mounted in the
/// container while it is open with the default UI, mounted again when the
/// panel changes. A click on a type converts the selection and focuses the
/// panel, so Tab and Shift+Tab keep cycling from it; a panel that had the
/// focus keeps it across a re-mount.
fn render_convert_popup(weak: &Weak<RefCell<Inner>>) -> Result<(), JsValue> {
    let Some(rc) = weak.upgrade() else {
        return Ok(());
    };
    let mut inner = rc.borrow_mut();
    let panel = if inner.ui == "none" {
        None
    } else {
        inner.editor.convert_panel()
    };
    let key = panel.as_ref().map(|p| {
        serde_json::json!({
            "left": p.left,
            "top": p.top,
            "shapes": p.shapes.iter().map(|s| (s.kind.name(), s.checked)).collect::<Vec<_>>(),
        })
    });
    let focus_requested = std::mem::take(&mut inner.focus_convert_popup);
    if key.is_some() && inner.convert_popup.as_ref().map(|(_, k)| k) == key.as_ref() {
        if focus_requested {
            focus_popup(inner.convert_popup.as_ref().map(|(m, _)| m));
        }
        return Ok(());
    }
    let document = inner.document();
    let had_focus = inner.convert_popup.as_ref().is_some_and(|(m, _)| {
        let active = document.active_element().map(web_sys::Node::from);
        active.is_some_and(|a| m.root().contains(Some(&a)))
    });
    if let Some((old, _)) = inner.convert_popup.take() {
        old.remove();
    }
    let (Some(panel), Some(key)) = (panel, key) else {
        if had_focus {
            let _ = inner.container.focus();
        }
        return Ok(());
    };
    let events = weak.clone();
    let on_select: excali_ui::convert_popup::OnConvert = Rc::new(move |kind| {
        let Some(rc) = events.upgrade() else {
            return;
        };
        {
            let Ok(mut inner) = rc.try_borrow_mut() else {
                return;
            };
            inner.editor.convert_popup_select(kind);
            inner.focus_convert_popup = true;
            inner.after_event();
        }
        refresh_chrome(&events);
    });
    let node = Node::Element(excali_ui::convert_popup::convert_popup(
        &panel,
        Some(on_select),
    ));
    let mounted = mount(&node, &document, &inner.container)?;
    if focus_requested || had_focus {
        focus_popup(Some(&mounted));
    }
    inner.convert_popup = Some((mounted, key));
    Ok(())
}

fn focus_popup(mounted: Option<&Mounted>) {
    if let Some(el) = mounted
        .and_then(Mounted::element)
        .and_then(|e| e.dyn_into::<HtmlElement>().ok())
    {
        let _ = el.focus();
    }
}

/// Re-mounts the welcome screen's centre in the container while
/// [`render_welcome_screen`] holds; its items run their actions.
fn render_welcome_center(weak: &Weak<RefCell<Inner>>) -> Result<(), JsValue> {
    let Some(rc) = weak.upgrade() else {
        return Ok(());
    };
    let mut inner = rc.borrow_mut();
    if let Some(old) = inner.welcome_center.take() {
        old.remove();
    }
    if !render_welcome_screen(&inner) {
        return Ok(());
    }
    let events = weak.clone();
    let on_event = Rc::new(move |event: WelcomeScreenEvent| {
        let Some(rc) = events.upgrade() else {
            return;
        };
        {
            let Ok(mut inner) = rc.try_borrow_mut() else {
                return;
            };
            inner.editor.perform_action(event.action());
            inner.after_event();
        }
        refresh_chrome(&events);
    });
    let view_mode = inner
        .editor
        .app_state()
        .get("viewModeEnabled")
        .and_then(Value::as_bool)
        == Some(true);
    let node = Node::Element(welcome_screen_center(&WelcomeScreenProps {
        form_factor: excali_ui::editor_interface::FormFactor::Desktop,
        view_mode_enabled: view_mode,
        is_darwin: is_darwin(),
        on_event: Some(on_event),
    }));
    let document = inner.document();
    // WelcomeScreenCenterTunnel.Out leads the wrapper (LayerUI.tsx:655)
    let mounted = mount(&node, &document, &inner.layer_ui)?;
    let first = inner.layer_ui.first_child();
    inner
        .layer_ui
        .insert_before(mounted.root(), first.as_ref())?;
    inner.welcome_center = Some(mounted);
    Ok(())
}

/// The cursor hint for a tool key's outcome (`App.tsx:5808-5821`):
/// `cursorHints.onArrowTypeCycled` or `onToolShortcut`.
fn show_cursor_hint(weak: &Weak<RefCell<Inner>>, effects: &[KeyEffect]) {
    let Some(rc) = weak.upgrade() else {
        return;
    };
    let Some((tool, next, hint)) = effects.iter().find_map(|e| match e {
        KeyEffect::Tool(ToolKeyOutcome::Tool {
            tool,
            next_arrow_type,
            hint,
            ..
        }) => Some((*tool, *next_arrow_type, *hint)),
        _ => None,
    }) else {
        return;
    };
    let mut inner = rc.borrow_mut();
    if inner.ui == "none" {
        return;
    }
    let now = js_sys::Date::now();
    let pointer = inner.editor.last_pointer();
    let position = (pointer[0], pointer[1]);
    let arrow_type = match inner
        .editor
        .app_state()
        .get("currentItemArrowType")
        .and_then(Value::as_str)
    {
        Some("sharp") => ArrowType::Sharp,
        Some("elbow") => ArrowType::Elbow,
        _ => ArrowType::Round,
    };
    let icon = match (next, hint) {
        (Some(next), _) => inner.cursor_hints.on_arrow_type_cycled(next, now, position),
        (None, Some(source)) => inner
            .cursor_hints
            .on_tool_shortcut(tool, source, arrow_type, now, position),
        (None, None) => None,
    };
    let Some(icon) = icon else {
        return;
    };
    if let Some((old, _)) = inner.cursor_hint.take() {
        old.remove();
    }
    inner.cursor_hint_nonce = inner.cursor_hint_nonce.wrapping_add(1);
    let nonce = inner.cursor_hint_nonce;
    let document = inner.document();
    let Ok(mounted) = mount(
        &Node::Element(cursor_hint(icon, false, 0.0, 0.0)),
        &document,
        &inner.container,
    ) else {
        return;
    };
    inner.cursor_hint = Some((mounted, nonce));
    place_cursor_hint(&inner, pointer[0], pointer[1]);
    drop(inner);
    // CURSOR_HINT_DURATION, then the fade-out, then gone
    let fade = weak.clone();
    let hide = weak.clone();
    set_timeout(CURSOR_HINT_DURATION, move || {
        if let Some(rc) = fade.upgrade() {
            let inner = rc.borrow();
            if let Some((mounted, n)) = &inner.cursor_hint {
                if *n == nonce {
                    if let Some(el) = mounted.element() {
                        let _ = el.class_list().add_1("CursorHint--fade-out");
                    }
                }
            }
        }
    });
    set_timeout(
        CURSOR_HINT_DURATION + CURSOR_HINT_FADE_DURATION,
        move || {
            if let Some(rc) = hide.upgrade() {
                let mut inner = rc.borrow_mut();
                if inner.cursor_hint.as_ref().is_some_and(|(_, n)| *n == nonce) {
                    if let Some((mounted, _)) = inner.cursor_hint.take() {
                        mounted.remove();
                    }
                }
            }
        },
    );
}

/// Moves the cursor hint beside a pointer at client `(x, y)`
/// (`positionElementBesideCursor`, `CURSOR_HINT_GAP`).
fn place_cursor_hint(inner: &Inner, x: f64, y: f64) {
    let Some(el) = inner
        .cursor_hint
        .as_ref()
        .and_then(|(m, _)| m.element())
        .and_then(|e| e.dyn_into::<HtmlElement>().ok())
    else {
        return;
    };
    let rect = inner.container.get_bounding_client_rect();
    let (left, top) = position_element_beside_cursor(
        (x, y),
        (f64::from(el.offset_width()), f64::from(el.offset_height())),
        ContainerRect {
            left: rect.left(),
            top: rect.top(),
            width: rect.width(),
            height: rect.height(),
        },
        CURSOR_HINT_GAP,
    );
    let _ = el
        .style()
        .set_property("transform", &format!("translate({left}px, {top}px)"));
}

/// Removes the cursor hint (a pointerdown hides it at once).
fn hide_cursor_hint(inner: &mut Inner) {
    if let Some((mounted, _)) = inner.cursor_hint.take() {
        mounted.remove();
    }
}

/// Runs the editor's viewport navigation on animation frames until it
/// settles, as `AnimationController` (`renderer/animation.ts`) runs
/// `animateToViewport`: each frame steps it at the frame's timestamp and
/// paints.
fn animate_viewport(weak: &Weak<RefCell<Inner>>, inner: &mut Inner) {
    if inner.viewport_frame_pending || !inner.editor.is_viewport_animating() {
        return;
    }
    inner.viewport_frame_pending = true;
    request_viewport_frame(weak.clone());
}

fn request_viewport_frame(weak: Weak<RefCell<Inner>>) {
    if let Some(window) = web_sys::window() {
        let callback = Closure::once_into_js(move |now: f64| viewport_frame(&weak, now));
        let _ = window.request_animation_frame(callback.unchecked_ref());
    }
}

fn viewport_frame(weak: &Weak<RefCell<Inner>>, now: f64) {
    let Some(rc) = weak.upgrade() else {
        return;
    };
    {
        let Ok(mut inner) = rc.try_borrow_mut() else {
            // busy: try again on the next frame
            request_viewport_frame(weak.clone());
            return;
        };
        inner.viewport_frame_pending = false;
        if !inner.editor.is_viewport_animating() {
            return;
        }
        inner.editor.viewport_frame(now);
        inner.after_event();
        let weak = weak.clone();
        animate_viewport(&weak, &mut inner);
    }
    refresh_chrome(weak);
}

fn set_timeout(ms: f64, f: impl FnOnce() + 'static) {
    if let Some(window) = web_sys::window() {
        let callback = Closure::once_into_js(f);
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            callback.unchecked_ref(),
            ms as i32,
        );
    }
}

/// Re-mounts the toolbar for the current tools.
fn render_toolbar(weak: &Weak<RefCell<Inner>>) -> Result<(), JsValue> {
    let Some(rc) = weak.upgrade() else {
        return Ok(());
    };
    let mut inner = rc.borrow_mut();
    if let Some(old) = inner.toolbar.take() {
        old.remove();
    }
    if inner.ui == "none" {
        return Ok(());
    }
    let events = weak.clone();
    let on_event = Rc::new(move |event: ToolbarEvent| {
        let Some(rc) = events.upgrade() else {
            return;
        };
        {
            let mut inner = rc.borrow_mut();
            let tools: &mut ToolState = inner.editor.tools_mut();
            match event {
                ToolbarEvent::ToolButton { tool, pointer_type } => {
                    let _ = activate_tool_button(tools, tool, pointer_type);
                }
                ToolbarEvent::Lock => {
                    tools.toggle_lock();
                }
                ToolbarEvent::PenMode => tools.toggle_pen_mode(None),
                ToolbarEvent::ExtraTool(tool) => {
                    let _ = activate_extra_tool(tools, tool);
                    inner.extra_tools_open = false;
                }
                ToolbarEvent::ExtraToolsToggle => inner.extra_tools_open = !inner.extra_tools_open,
                ToolbarEvent::ExtraToolsClose => inner.extra_tools_open = false,
                _ => return,
            }
        }
        refresh_chrome(&events);
    }) as Rc<dyn Fn(ToolbarEvent)>;
    let node = Node::Element(toolbar(ToolbarProps {
        tools: inner.editor.tools(),
        zen_mode: false,
        collaborating: false,
        ai_enabled: false,
        diagram_to_code: false,
        extra_tools_open: inner.extra_tools_open,
        id_prefix: "excali-editor".into(),
        hint_viewer: hint_node(&inner).map(Node::Element),
        ttd_trigger: None,
        on_event: Some(on_event),
    }));
    // the shapes section's `position: relative` box, with the welcome
    // screen's toolbar hint before the toolbar (LayerUI.tsx:353-357)
    let node = Node::Element(
        excali_ui::dom::Element::new("div")
            .style("position", "relative")
            .child_opt(render_welcome_screen(&inner).then(toolbar_hint))
            .child(node),
    );
    inner.hint = current_hint(&inner);
    let document = inner.document();
    let mounted = mount(&node, &document, &inner.top)?;
    inner.toolbar = Some(mounted);
    Ok(())
}

/// A main menu item's effect, applied to the editor.
fn apply_menu_effect(inner: &mut Inner, effect: MenuEffect) {
    match effect {
        MenuEffect::ExecuteAction(ActionName::ToggleTheme) => {
            let dark = inner
                .editor
                .app_state()
                .get("theme")
                .and_then(Value::as_str)
                == Some("dark");
            let _ = apply_theme(
                &inner.container,
                if dark { Theme::Light } else { Theme::Dark },
            );
            inner.editor.set_theme(!dark);
        }
        MenuEffect::ExecuteAction(name) => inner.editor.perform_action(name),
        MenuEffect::SetAppState(patch) => inner.editor.set_app_state(patch),
        MenuEffect::ToggleLock => {
            inner.editor.tools_mut().toggle_lock();
        }
        MenuEffect::ThemeChange(choice) => {
            let dark = choice == ThemeChoice::Dark;
            let _ = apply_theme(
                &inner.container,
                if dark { Theme::Dark } else { Theme::Light },
            );
            inner.editor.set_theme(dark);
        }
        MenuEffect::Warn(message) => web_sys::console::warn_1(&JsValue::from_str(message)),
        // the host owns files, dialogs and analytics
        MenuEffect::ConfirmDialog(_)
        | MenuEffect::ConfirmOverwrite { .. }
        | MenuEffect::TrackEvent { .. }
        | MenuEffect::Select => {}
    }
}

/// Re-mounts the main menu (`MainMenu`, LayerUI's default items) in the
/// top-left corner, open while `appState.openMenu` is `"canvas"`.
fn render_main_menu(weak: &Weak<RefCell<Inner>>) -> Result<(), JsValue> {
    let Some(rc) = weak.upgrade() else {
        return Ok(());
    };
    let mut inner = rc.borrow_mut();
    if let Some(old) = inner.main_menu.take() {
        old.remove();
    }
    if let Some(old) = inner.menu_hint.take() {
        old.remove();
    }
    if inner.ui == "none" {
        return Ok(());
    }
    let events = weak.clone();
    let dispatch: Dispatch = Rc::new(move |effect: MenuEffect| {
        let Some(rc) = events.upgrade() else {
            return;
        };
        {
            let Ok(mut inner) = rc.try_borrow_mut() else {
                return;
            };
            apply_menu_effect(&mut inner, effect);
            inner.after_event();
        }
        refresh_chrome(&events);
    });
    let dark = inner
        .editor
        .app_state()
        .get("theme")
        .and_then(Value::as_str)
        == Some("dark");
    let node = {
        let cx = MenuContext::new(
            false,
            is_darwin(),
            &KeyLabels::EN,
            if dark { Theme::Dark } else { Theme::Light },
            dispatch,
        );
        let ed = &inner.editor;
        let ctx = ed.action_context();
        Node::Element(default_main_menu(&cx, ed.action_manager(), &ctx, &|_| None))
    };
    let document = inner.document();
    let mounted = mount(&node, &document, &inner.top_left)?;
    inner.main_menu = Some(mounted);
    // WelcomeScreenMenuHintTunnel.Out, under the menu (LayerUI.tsx:245)
    if render_welcome_screen(&inner) {
        let hint = mount(&Node::Element(menu_hint()), &document, &inner.top_left)?;
        inner.menu_hint = Some(hint);
    }
    Ok(())
}

/// `App.openContextMenu` for a `contextmenu` event over the canvas: which
/// menu, and where relative to the container. `None` when no menu opens:
/// the default UI is off (`isDefaultUIEnabled`), interaction is disabled,
/// or a touch (or a pen's primary button) presses while a tool other than
/// the preferred selection tool is active.
fn open_context_menu(inner: &mut Inner, event: &Event) -> Option<(ContextMenuKind, f64, f64)> {
    if inner.ui == "none" || !inner.editor.tools().is_interaction_enabled() {
        return None;
    }
    if let Some(pointer) = event.dyn_ref::<PointerEvent>() {
        let tools = inner.editor.tools();
        let pointer_type = pointer.pointer_type();
        let selection_tool = Tool::Builtin(tools.preferred_selection_tool.tool.tool_type());
        if (pointer_type == "touch" || (pointer_type == "pen" && pointer.button() != 2))
            && tools.active_tool.tool != selection_tool
        {
            return None;
        }
    }
    inner.measure();
    let (client_x, client_y) = (number(event, "clientX"), number(event, "clientY"));
    let kind = inner.editor.open_context_menu(client_x, client_y);
    inner.after_event();
    let rect = inner.container.get_bounding_client_rect();
    Some((kind, client_y - rect.top(), client_x - rect.left()))
}

/// Mounts the context menu (`ContextMenu`) at `top`, `left` in the
/// container, its rows those of `getContextMenuItems(kind)` whose
/// predicates hold now. A row closes it and runs its action; a press
/// outside it closes it.
fn render_context_menu(
    weak: &Weak<RefCell<Inner>>,
    kind: ContextMenuKind,
    top: f64,
    left: f64,
) -> Result<(), JsValue> {
    let Some(rc) = weak.upgrade() else {
        return Ok(());
    };
    let mut inner = rc.borrow_mut();
    if let Some(old) = inner.context_menu.take() {
        old.remove();
    }
    let events = weak.clone();
    let on_effect: OnContextMenuEffect = Rc::new(move |effect: ContextMenuEffect| {
        let Some(rc) = events.upgrade() else {
            return;
        };
        {
            let Ok(mut inner) = rc.try_borrow_mut() else {
                return;
            };
            match effect {
                ContextMenuEffect::Close => {
                    if let Some(menu) = inner.context_menu.take() {
                        menu.remove();
                    }
                }
                ContextMenuEffect::ExecuteAction(name) => {
                    apply_menu_effect(&mut inner, MenuEffect::ExecuteAction(name));
                    inner.after_event();
                }
                ContextMenuEffect::PreventDefault => return,
            }
        }
        refresh_chrome(&events);
    });
    let node = {
        let ed = &inner.editor;
        let live: Vec<_> = ed
            .elements()
            .iter()
            .filter(|e| !e.base.is_deleted)
            .cloned()
            .collect();
        let ctx = ActionContext {
            elements: &live,
            ..ed.action_context()
        };
        let view_mode = ed.app_state().get("viewModeEnabled") == Some(&Value::Bool(true));
        let items = get_context_menu_items(kind, view_mode, ctx.env.form_factor);
        let entries = build_context_menu(&items, &ctx, &KeyLabels::EN);
        let app = ed.app_state();
        let size = |key: &str| app.get(key).and_then(Value::as_f64).unwrap_or(0.0);
        Node::Element(
            context_menu(
                &entries,
                ContextMenuProps {
                    top,
                    left,
                    viewport_width: size("width"),
                    viewport_height: size("height"),
                    on_effect: Some(on_effect),
                },
            )
            .element,
        )
    };
    let document = inner.document();
    let mounted = mount(&node, &document, &inner.container)?;
    inner.context_menu = Some(mounted);
    Ok(())
}

/// The library sidebar's view of the editor: the app state keys it reads,
/// the library and the pending selection.
fn library_context<'a>(
    inner: &Inner,
    items: &'a [excali_core::library::LibraryItem],
    pending: &'a [excali_core::element::Element],
) -> LibraryContext<'a> {
    let app = inner.editor.app_state();
    let open_sidebar = app
        .get("openSidebar")
        .and_then(Value::as_object)
        .and_then(|o| {
            Some(OpenSidebar {
                name: o.get("name")?.as_str()?.to_owned(),
                tab: o.get("tab").and_then(Value::as_str).map(str::to_owned),
            })
        });
    LibraryContext {
        open_sidebar,
        docked_preference: app
            .get("defaultSidebarDockedPreference")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        can_fit_sidebar: can_fit_sidebar(inner),
        phone: false,
        status: LibraryStatus::Loaded,
        items,
        pending,
    }
}

/// A sidebar event, run through `update` and applied to the editor; the
/// effects that wait on the user or the network are returned for
/// `library_menu` to run once the editor is released.
fn library_event(inner: &mut Inner, event: LibrarySidebarEvent) -> Vec<LibraryEffect> {
    let mut jobs = Vec::new();
    let mut events = vec![event];
    while let Some(event) = events.pop() {
        jobs.extend(library_event_once(inner, event, &mut events));
    }
    jobs
}

fn library_event_once(
    inner: &mut Inner,
    event: LibrarySidebarEvent,
    answers: &mut Vec<LibrarySidebarEvent>,
) -> Vec<LibraryEffect> {
    let items = inner.editor.library().to_vec();
    let pending = inner.editor.pending_library_elements();
    let cx = library_context(inner, &items, &pending);
    let docked_and_fits = is_sidebar_docked_and_fits(&cx);
    let mut state = std::mem::take(&mut inner.library_menu);
    let out = inner
        .editor
        .with_restore_env(|env| update_library(&mut state, event, &cx, env));
    inner.library_menu = state;
    let mut jobs = Vec::new();
    for effect in out.effects {
        if library_menu::is_job(&effect) {
            jobs.push(effect);
            continue;
        }
        match effect {
            LibraryEffect::SetAppState(patch) => inner.editor.set_app_state(patch),
            LibraryEffect::Insert(ids) => {
                inner.editor.insert_library(&ids, None, docked_and_fits);
            }
            LibraryEffect::SetLibrary(items) => inner.editor.set_library(items),
            LibraryEffect::ResetLibrary => {
                inner.editor.set_library(Vec::new());
                inner.previews.clear();
            }
            LibraryEffect::DeletePreviews(ids) => {
                for id in ids {
                    inner.previews.remove(&id);
                }
            }
            // as the publish dialog mounts
            LibraryEffect::LoadPublishData => answers.push(LibrarySidebarEvent::PublishDataLoaded(
                library_menu::load_publish_data(),
            )),
            LibraryEffect::SavePublishData(data) => library_menu::save_publish_data(data.as_ref()),
            LibraryEffect::Alert(message) => library_menu::alert(&message),
            // the host owns analytics; the focus, the files and the
            // submission are jobs
            LibraryEffect::TrackEvent(..)
            | LibraryEffect::FocusContainer
            | LibraryEffect::LoadLibrary
            | LibraryEffect::ExportLibrary(_)
            | LibraryEffect::SubmitLibrary { .. } => {}
        }
    }
    jobs
}

/// A preview's markup as a node: an `<svg>` replaced by the export's once
/// mounted.
fn preview_node(markup: String) -> Node {
    Node::Element(
        excali_ui::dom::Element::svg("svg").on_mount(move |el| el.set_outer_html(&markup)),
    )
}

/// Re-mounts LayerUI's library trigger (in the top-right corner, hidden
/// while the sidebar is docked and fits) and the default sidebar.
fn render_library_sidebar(weak: &Weak<RefCell<Inner>>) -> Result<(), JsValue> {
    let Some(rc) = weak.upgrade() else {
        return Ok(());
    };
    let mut inner = rc.borrow_mut();
    // the search tab's menu, from the field of the sidebar still mounted
    let search_menu = search::render(weak, &mut inner);
    for old in [inner.library_trigger.take(), inner.sidebar.take()]
        .into_iter()
        .flatten()
    {
        old.remove();
    }
    if inner.ui == "none" {
        return Ok(());
    }
    let items = inner.editor.library().to_vec();
    let pending = inner.editor.pending_library_elements();
    // the previews, exported again for items whose elements changed
    let mut previews = Previews::default();
    for item in &items {
        let version = u64::from(excali_core::library::hash_elements_version(&item.elements));
        let markup = match inner.previews.get(&item.id) {
            Some((v, markup)) if *v == version => markup.clone(),
            _ => {
                let markup = inner.editor.library_item_svg(&item.elements);
                inner
                    .previews
                    .insert(item.id.clone(), (version, markup.clone()));
                markup
            }
        };
        previews.items.insert(item.id.clone(), preview_node(markup));
    }
    if !pending.is_empty() {
        previews.pending = Some(preview_node(inner.editor.library_item_svg(&pending)));
    }
    let events = weak.clone();
    let on_event = Rc::new(move |event: LibrarySidebarEvent| library_menu::dispatch(&events, event))
        as Rc<dyn Fn(LibrarySidebarEvent)>;
    let dark = inner
        .editor
        .app_state()
        .get("theme")
        .and_then(Value::as_str)
        == Some("dark");
    let theme = if dark { Theme::Dark } else { Theme::Light };
    let document = inner.document();
    let (trigger, sidebar) = {
        let cx = library_context(&inner, &items, &pending);
        let open = cx.open_sidebar.as_ref().map(|o| o.name.as_str()) == Some("default");
        let trigger = (!is_sidebar_docked_and_fits(&cx)).then(|| {
            Node::Element(default_sidebar_trigger(SidebarTriggerProps {
                open,
                theme,
                on_event: Some(on_event.clone()),
            }))
        });
        let sidebar = default_sidebar(LibrarySidebarProps {
            context: cx,
            theme,
            previews: &previews,
            state: &inner.library_menu,
            browse: BrowseLink {
                app_id: "excali-editor".into(),
                library_return_url: None,
                location: web_sys::window()
                    .and_then(|w| {
                        let l = w.location();
                        Some(format!("{}{}", l.origin().ok()?, l.pathname().ok()?))
                    })
                    .unwrap_or_default(),
                window_name: web_sys::window()
                    .and_then(|w| w.name().ok())
                    .unwrap_or_default(),
            },
            id_prefix: "excali-editor-sidebar".into(),
            menu_ids: (
                "excali-editor-library-menu-trigger".into(),
                "excali-editor-library-menu".into(),
            ),
            search_menu,
            on_event: Some(on_event),
        })
        .map(Node::Element);
        (trigger, sidebar)
    };
    if let Some(node) = trigger {
        inner.library_trigger = Some(mount(&node, &document, &inner.top_right)?);
    }
    if let Some(node) = sidebar {
        inner.sidebar = Some(mount(&node, &document, &inner.container)?);
    }
    Ok(())
}

/// Re-mounts the footer (`Footer.tsx`): the zoom actions and the undo and
/// redo buttons, run through the editor.
fn render_footer(weak: &Weak<RefCell<Inner>>) -> Result<(), JsValue> {
    let Some(rc) = weak.upgrade() else {
        return Ok(());
    };
    let mut inner = rc.borrow_mut();
    if let Some(old) = inner.footer.take() {
        old.remove();
    }
    if inner.ui == "none" {
        return Ok(());
    }
    let events = weak.clone();
    let on_event = Rc::new(move |control: FooterControl| {
        let Some(rc) = events.upgrade() else {
            return;
        };
        {
            let mut inner = rc.borrow_mut();
            match control {
                FooterControl::Undo => inner.editor.undo(),
                FooterControl::Redo => inner.editor.redo(),
                other => match other.zoom_action() {
                    Some(action) => inner.editor.zoom_action(action.name()),
                    None => inner.editor.perform_action(other.action()),
                },
            }
            inner.after_event();
        }
        refresh_chrome(&events);
    }) as OnFooterEvent;
    let node = Node::Element(footer(FooterProps {
        zoom: inner.editor.app_state().zoom().unwrap_or(1.0),
        undo_stack_empty: !inner.editor.can_undo(),
        redo_stack_empty: !inner.editor.can_redo(),
        is_darwin: is_darwin(),
        render_welcome_screen: render_welcome_screen(&inner),
        welcome_screen_help_hint: Some(Node::Element(help_hint())),
        on_event: Some(on_event),
        ..FooterProps::default()
    }));
    let document = inner.document();
    let mounted = mount(&node, &document, &inner.layer_ui)?;
    inner.footer = Some(mounted);
    Ok(())
}

/// The browser facts the help dialog reads (`isDarwin`, `isWindows`,
/// `isFirefox`: `common/src/editorInterface.ts:37-43`;
/// `probablySupportsClipboardBlob`: `clipboard.ts:68-72`).
fn help_platform() -> Platform {
    let Some(window) = web_sys::window() else {
        return Platform::default();
    };
    let navigator = window.navigator();
    let platform = navigator.platform().unwrap_or_default();
    let agent = navigator.user_agent().unwrap_or_default();
    let has = |target: &JsValue, key: &str| {
        js_sys::Reflect::has(target, &JsValue::from_str(key)).unwrap_or(false)
    };
    let clipboard = js_sys::Reflect::get(&navigator, &JsValue::from_str("clipboard"))
        .unwrap_or(JsValue::UNDEFINED);
    let canvas = js_sys::Reflect::get(&window, &JsValue::from_str("HTMLCanvasElement"))
        .and_then(|c| js_sys::Reflect::get(&c, &JsValue::from_str("prototype")))
        .unwrap_or(JsValue::UNDEFINED);
    Platform {
        darwin: is_darwin(),
        windows: platform.starts_with("Win"),
        firefox: has(&window, "netscape")
            && agent.find("rv:").is_some_and(|i| i > 1)
            && agent.find("Gecko").is_some_and(|i| i > 1),
        clipboard_blob: clipboard.is_object()
            && has(&clipboard, "write")
            && has(&window, "ClipboardItem")
            && canvas.is_object()
            && has(&canvas, "toBlob"),
    }
}

/// Opens the help dialog (`HelpDialog.tsx`) when `appState.openDialog`
/// becomes `{name: "help"}` (`LayerUI.tsx:577-583`), in a modal on the
/// body, and removes it when that changes; closing it clears `openMenu`
/// and `openDialog`.
fn render_help_dialog(weak: &Weak<RefCell<Inner>>) -> Result<(), JsValue> {
    let Some(rc) = weak.upgrade() else {
        return Ok(());
    };
    let mut inner = rc.borrow_mut();
    let open = inner
        .editor
        .app_state()
        .get("openDialog")
        .and_then(|d| d.get("name"))
        .and_then(Value::as_str)
        == Some("help");
    // an open dialog stays mounted (and keeps its focus) while it stays open
    if open && inner.help_dialog.is_some() {
        return Ok(());
    }
    if let Some(old) = inner.help_dialog.take() {
        old.close();
    }
    if !open {
        return Ok(());
    }
    let dark = inner
        .editor
        .app_state()
        .get("theme")
        .and_then(Value::as_str)
        == Some("dark");
    let events = weak.clone();
    let on_close = Rc::new(move || {
        let Some(rc) = events.upgrade() else {
            return;
        };
        {
            let Ok(mut inner) = rc.try_borrow_mut() else {
                return;
            };
            let mut state = excali_core::app_state::AppState::default();
            close_help_dialog(&mut state);
            let patch = ["openMenu", "openDialog"]
                .into_iter()
                .filter_map(|k| Some((k.to_owned(), state.get(k)?.clone())))
                .collect();
            inner.editor.set_app_state(patch);
            inner.after_event();
        }
        refresh_chrome(&events);
    }) as Rc<dyn Fn()>;
    let toggle_theme_enabled = {
        let ed = &inner.editor;
        let ctx = ed.action_context();
        ed.action_manager()
            .is_action_enabled(ActionName::ToggleTheme, &ctx)
    };
    let dialog = help_dialog(HelpDialogProps {
        platform: help_platform(),
        toggle_theme_enabled,
        container_id: "excali-editor".into(),
        theme: if dark { Theme::Dark } else { Theme::Light },
        phone: false,
        on_close: Some(on_close),
    });
    let document = inner.document();
    inner.help_dialog = Some(excali_ui::primitives::open_modal(&document, dialog)?);
    Ok(())
}

/// The open command palette: its modal, the commands it was opened with
/// (upstream builds them once per opening), the search and what it shows.
struct PaletteSession {
    /// `None` only while it is being opened.
    modal: Option<excali_ui::primitives::OpenModal>,
    /// The list re-mounted after the search or the selection changed.
    list: Option<Mounted>,
    commands: Vec<PaletteCommand>,
    library: Vec<PaletteCommand>,
    search: String,
    view: PaletteView,
    phone: bool,
    dark: bool,
    previews: Rc<std::collections::HashMap<String, String>>,
}

impl PaletteSession {
    fn close(self) {
        if let Some(list) = self.list {
            list.remove();
        }
        if let Some(modal) = self.modal {
            modal.close();
        }
    }
}

fn is_command_palette_open(inner: &Inner) -> bool {
    inner
        .editor
        .app_state()
        .get("openDialog")
        .and_then(|d| d.get("name"))
        .and_then(Value::as_str)
        == Some("commandPalette")
}

/// The palette's props for `session`, its handlers bound to the editor.
fn palette_props(weak: &Weak<RefCell<Inner>>, session: &PaletteSession) -> CommandPaletteProps {
    let on_search = {
        let weak = weak.clone();
        Rc::new(move |search: String| {
            let Some(rc) = weak.upgrade() else {
                return;
            };
            if let Ok(mut inner) = rc.try_borrow_mut() {
                let last_used = inner.palette_last_used.clone();
                if let Some(session) = inner.palette.as_mut() {
                    session.view = palette_view(
                        &session.commands,
                        &session.library,
                        &search,
                        last_used.as_deref(),
                    );
                    session.search = search;
                }
            }
            render_palette_list(&weak);
        }) as Rc<dyn Fn(String)>
    };
    let on_hover = {
        let weak = weak.clone();
        Rc::new(move |label: String| {
            let Some(rc) = weak.upgrade() else {
                return;
            };
            let changed = rc.try_borrow_mut().is_ok_and(|mut inner| {
                inner.palette.as_mut().is_some_and(|session| {
                    let changed = session.view.current.as_deref() != Some(label.as_str());
                    session.view.current = Some(label);
                    changed
                })
            });
            if changed {
                render_palette_list(&weak);
            }
        }) as Rc<dyn Fn(String)>
    };
    let on_execute = {
        let weak = weak.clone();
        Rc::new(move |label: String| execute_palette_command(&weak, &label)) as Rc<dyn Fn(String)>
    };
    let on_close = {
        let weak = weak.clone();
        Rc::new(move || close_command_palette(&weak)) as Rc<dyn Fn()>
    };
    let previews = session.previews.clone();
    CommandPaletteProps {
        view: session.view.clone(),
        search: session.search.clone(),
        is_darwin: is_darwin(),
        phone: session.phone,
        theme: if session.dark {
            Theme::Dark
        } else {
            Theme::Light
        },
        container_id: "excali-editor".into(),
        on_search: Some(on_search),
        on_execute: Some(on_execute),
        on_hover: Some(on_hover),
        on_close: Some(on_close),
        library_preview: Some(Rc::new(move |id: &str| {
            previews.get(id).map(|markup| preview_node(markup.clone()))
        })),
    }
}

/// Opens the command palette (`CommandPalette.tsx`) when
/// `appState.openDialog` becomes `{name: "commandPalette"}`, in a modal on
/// the body, with the commands of the moment (the actions', the tools',
/// the hosted app's Links and the named library items), and removes it
/// when that changes.
fn render_command_palette(weak: &Weak<RefCell<Inner>>) -> Result<(), JsValue> {
    let Some(rc) = weak.upgrade() else {
        return Ok(());
    };
    let mut inner = rc.borrow_mut();
    let open = inner.ui != "none" && is_command_palette_open(&inner);
    // an open palette stays mounted (with its search and focus)
    if open && inner.palette.is_some() {
        return Ok(());
    }
    if let Some(old) = inner.palette.take() {
        old.close();
    }
    if !open {
        return Ok(());
    }
    let ed = &inner.editor;
    let base = ed.action_context();
    // the palette reads the non-deleted elements
    let live: Vec<excali_core::element::Element> = base
        .elements
        .iter()
        .filter(|e| !e.base.is_deleted)
        .cloned()
        .collect();
    let ctx = ActionContext {
        elements: &live,
        ..base
    };
    let phone = matches!(
        base.env.form_factor,
        excali_editor::actions::FormFactor::Phone
    );
    let env = PaletteEnv {
        is_darwin: is_darwin(),
        phone,
        ..PaletteEnv::default()
    };
    let commands = palette_commands(&ctx, &env, hosted_app_links());
    let library = library_commands(ed.library());
    let dark = ed.app_state().get("theme").and_then(Value::as_str) == Some("dark");
    let view = palette_view(&commands, &library, "", inner.palette_last_used.as_deref());
    let previews = Rc::new(
        inner
            .previews
            .iter()
            .map(|(id, (_, markup))| (id.clone(), markup.clone()))
            .collect(),
    );
    let document = inner.document();
    let mut session = PaletteSession {
        modal: None,
        list: None,
        commands,
        library,
        search: String::new(),
        view,
        phone,
        dark,
        previews,
    };
    let dialog = command_palette(palette_props(weak, &session));
    session.modal = Some(excali_ui::primitives::open_modal(&document, dialog)?);
    if let Some(container) = session.modal.as_ref().and_then(|m| m.container()) {
        excali_ui::accessibility::name_controls(&container)?;
    }
    inner.palette = Some(session);
    Ok(())
}

/// Re-mounts the open palette's list (`div.commands`) after the search or
/// the selection changed, leaving the search field (and its focus) alone.
fn render_palette_list(weak: &Weak<RefCell<Inner>>) {
    let Some(rc) = weak.upgrade() else {
        return;
    };
    let Ok(mut inner) = rc.try_borrow_mut() else {
        return;
    };
    let document = inner.document();
    let Some(session) = inner.palette.as_mut() else {
        return;
    };
    let Some(container) = session.modal.as_ref().and_then(|m| m.container()) else {
        return;
    };
    let Ok(Some(old)) = container.query_selector(".command-palette-dialog .commands") else {
        return;
    };
    let Some(parent) = old.parent_node() else {
        return;
    };
    if let Some(list) = session.list.take() {
        list.remove();
    } else {
        old.remove();
    }
    let node = Node::Element(command_list(&palette_props(weak, session)));
    if let Ok(mounted) = mount(&node, &document, &parent) {
        session.list = Some(mounted);
        let _ = excali_ui::accessibility::name_controls(&container);
    }
}

/// `closeCommandPalette`: `setAppState({openDialog: null})`.
fn close_command_palette(weak: &Weak<RefCell<Inner>>) {
    let Some(rc) = weak.upgrade() else {
        return;
    };
    {
        let Ok(mut inner) = rc.try_borrow_mut() else {
            return;
        };
        let mut patch = serde_json::Map::new();
        patch.insert("openDialog".into(), Value::Null);
        inner.editor.set_app_state(patch);
        inner.after_event();
    }
    refresh_chrome(weak);
}

/// `executeCommand` (`CommandPalette.tsx:655-672`): closes the palette,
/// runs the command and remembers it as the last used one.
fn execute_palette_command(weak: &Weak<RefCell<Inner>>, label: &str) {
    let Some(rc) = weak.upgrade() else {
        return;
    };
    {
        let Ok(mut inner) = rc.try_borrow_mut() else {
            return;
        };
        let Some(command) = inner
            .palette
            .as_ref()
            .and_then(|s| s.view.command(label))
            .cloned()
        else {
            return;
        };
        if let Some(session) = inner.palette.take() {
            session.close();
        }
        let state = inner.editor.app_state().clone();
        for effect in perform_command(&command, &state) {
            apply_palette_effect(&mut inner, effect);
        }
        inner.palette_last_used = Some(command.label);
        inner.after_event();
    }
    refresh_chrome(weak);
}

fn apply_palette_effect(inner: &mut Inner, effect: PaletteEffect) {
    match effect {
        PaletteEffect::ExecuteAction(name, _) => {
            apply_menu_effect(inner, MenuEffect::ExecuteAction(name))
        }
        PaletteEffect::SetAppState(patch) => inner.editor.set_app_state(patch),
        PaletteEffect::ConfirmDialog(name) => {
            apply_menu_effect(inner, MenuEffect::ConfirmDialog(name))
        }
        PaletteEffect::SetActiveTool(ty) => {
            let _ = inner.editor.tools_mut().set_active_tool(
                ToolRequest::new(Tool::Builtin(ty)),
                SetActiveToolOptions::default(),
            );
        }
        PaletteEffect::ToggleLock => {
            inner.editor.tools_mut().toggle_lock();
        }
        PaletteEffect::InsertLibraryItem(id) => {
            let items = inner.editor.library().to_vec();
            let fits = is_sidebar_docked_and_fits(&library_context(inner, &items, &[]));
            inner.editor.insert_library(&[id], None, fits);
        }
        PaletteEffect::OpenUrl(url) => {
            if let Some(window) = web_sys::window() {
                let _ = window.open_with_url_and_target_and_features(
                    url,
                    "_blank",
                    "noopener noreferrer",
                );
            }
        }
    }
}

/// The palette's window keydown listeners, in upstream's order: the
/// toggle (Ctrl/Cmd+/, Ctrl/Cmd+Shift+P), then, while it is open,
/// `handleKeyDown` ([`palette_key_down`]).
fn palette_window_key_down(rc: &Rc<RefCell<Inner>>, event: &KeyboardEvent) {
    let weak = Rc::downgrade(rc);
    let stroke = keystroke(event, None);
    let darwin = is_darwin();
    let toggle = is_command_palette_toggle_shortcut(&stroke, darwin);
    {
        let Ok(mut inner) = rc.try_borrow_mut() else {
            return;
        };
        if inner.ui == "none" {
            return;
        }
        if toggle {
            let mut state = inner.editor.app_state().clone();
            let out = command_palette_key_down(&mut state, &stroke, darwin);
            apply_outcome(event, &out);
            let mut patch = serde_json::Map::new();
            patch.insert(
                "openDialog".into(),
                state.get("openDialog").cloned().unwrap_or(Value::Null),
            );
            inner.editor.set_app_state(patch);
            inner.after_event();
        }
    }
    if toggle {
        refresh_chrome(&weak);
        return;
    }
    let (out, input) = {
        let Ok(inner) = rc.try_borrow() else {
            return;
        };
        let Some(session) = inner.palette.as_ref() else {
            return;
        };
        let input = session
            .modal
            .as_ref()
            .and_then(|m| m.container())
            .and_then(|c| {
                c.query_selector(".command-palette-dialog input")
                    .ok()
                    .flatten()
            });
        (
            palette_key_down(&session.view, &stroke.key, stroke.target.writable, false),
            input,
        )
    };
    if out.prevent_default {
        event.prevent_default();
    }
    if out.stop_propagation {
        event.stop_propagation();
    }
    if out.focus_input {
        if let Some(input) = input.and_then(|i| i.dyn_into::<HtmlElement>().ok()) {
            let _ = input.focus();
        }
    }
    if let Some(current) = out.current {
        if let Ok(mut inner) = rc.try_borrow_mut() {
            if let Some(session) = inner.palette.as_mut() {
                session.view.current = Some(current);
            }
        }
        render_palette_list(&weak);
    }
    if out.execute {
        let label = rc
            .borrow()
            .palette
            .as_ref()
            .and_then(|s| s.view.current.clone());
        if let (Some(label), Some(window)) = (label, web_sys::window()) {
            // upstream runs it on a timeout, once the Enter is handled
            let run = Closure::once_into_js(move || execute_palette_command(&weak, &label));
            let _ = window
                .set_timeout_with_callback_and_timeout_and_arguments_0(run.unchecked_ref(), 0);
        }
    }
}

/// The editor of one `<excali-editor>`.
#[wasm_bindgen]
pub struct EditorCore {
    inner: Rc<RefCell<Inner>>,
}

fn listen(
    inner: &Rc<RefCell<Inner>>,
    target: &web_sys::EventTarget,
    name: &'static str,
    handler: impl FnMut(&Rc<RefCell<Inner>>, Event) + 'static,
) -> Result<(), JsValue> {
    let weak = Rc::downgrade(inner);
    let mut handler = handler;
    let closure = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
        if let Some(rc) = weak.upgrade() {
            handler(&rc, event);
        }
    });
    target.add_event_listener_with_callback(name, closure.as_ref().unchecked_ref())?;
    inner
        .borrow_mut()
        .listeners
        .push((target.clone(), name, closure));
    Ok(())
}

/// [`listen`] with `{ passive: false }`, so the handler may prevent the
/// default (a wheel over the canvas).
fn listen_active(
    inner: &Rc<RefCell<Inner>>,
    target: &web_sys::EventTarget,
    name: &'static str,
    handler: impl FnMut(&Rc<RefCell<Inner>>, Event) + 'static,
) -> Result<(), JsValue> {
    let weak = Rc::downgrade(inner);
    let mut handler = handler;
    let closure = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
        if let Some(rc) = weak.upgrade() {
            handler(&rc, event);
        }
    });
    let options = AddEventListenerOptions::new();
    options.set_passive(false);
    target.add_event_listener_with_callback_and_add_event_listener_options(
        name,
        closure.as_ref().unchecked_ref(),
        &options,
    )?;
    inner
        .borrow_mut()
        .listeners
        .push((target.clone(), name, closure));
    Ok(())
}

/// [`listen`] in the capture phase.
fn listen_capture(
    inner: &Rc<RefCell<Inner>>,
    target: &web_sys::EventTarget,
    name: &'static str,
    handler: impl FnMut(&Rc<RefCell<Inner>>, Event) + 'static,
) -> Result<(), JsValue> {
    let weak = Rc::downgrade(inner);
    let mut handler = handler;
    let closure = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
        if let Some(rc) = weak.upgrade() {
            handler(&rc, event);
        }
    });
    let options = AddEventListenerOptions::new();
    options.set_capture(true);
    target.add_event_listener_with_callback_and_add_event_listener_options(
        name,
        closure.as_ref().unchecked_ref(),
        &options,
    )?;
    inner
        .borrow_mut()
        .listeners
        .push((target.clone(), name, closure));
    Ok(())
}

/// A number property of an event: `clientX` and `clientY` are doubles,
/// fractional in Chromium, where web-sys reads them as integers.
fn number(event: &Event, key: &str) -> f64 {
    js_sys::Reflect::get(event, &JsValue::from_str(key))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

/// A pointer event as the editor reads it; Cmd is the modifier on a Mac.
fn pointer_input(event: &PointerEvent) -> PointerInput {
    PointerInput {
        client_x: number(event, "clientX"),
        client_y: number(event, "clientY"),
        button: event.button(),
        shift_key: event.shift_key(),
        alt_key: event.alt_key(),
        ctrl_or_cmd: if is_darwin() {
            event.meta_key()
        } else {
            event.ctrl_key()
        },
    }
}

fn is_darwin() -> bool {
    web_sys::window()
        .and_then(|w| w.navigator().platform().ok())
        .is_some_and(|p| p.to_uppercase().contains("MAC") || p.contains("iP"))
}

#[wasm_bindgen]
impl EditorCore {
    /// Mounts the editor in `host` (the `<excali-editor>`); `dispatch(type,
    /// detail)` dispatches an event on it; `fonts_base` is the release's
    /// `fonts/` URL.
    #[wasm_bindgen(constructor)]
    pub fn new(
        host: HtmlElement,
        dispatch: js_sys::Function,
        fonts_base: String,
    ) -> Result<EditorCore, JsValue> {
        let document = host
            .owner_document()
            .ok_or_else(|| JsValue::from_str("the host has no document"))?;
        install_element_stylesheet(&document)?;
        let container: HtmlElement = document.create_element("div")?.dyn_into()?;
        container.set_class_name("excalidraw excalidraw-container");
        container.set_tab_index(0);
        apply_container_tokens(&container)?;
        host.append_child(&container)?;
        let layers = CanvasLayers::mount(&container)?;
        let editor_box: HtmlElement = document.create_element("div")?.dyn_into()?;
        editor_box.set_class_name(TEXTAREA_ATTRIBUTES.container_class_name);
        container.append_child(&editor_box)?;
        // App-menu_top's sections in upstream's order (LayerUI.tsx:314-430):
        // the left one with the main menu, the toolbar, the right one
        let layer_ui: HtmlElement = document.create_element("div")?.dyn_into()?;
        layer_ui.set_class_name("layer-ui__wrapper");
        container.append_child(&layer_ui)?;
        let top_left: HtmlElement = document.create_element("div")?.dyn_into()?;
        top_left.set_class_name("excali-editor__top-left");
        layer_ui.append_child(&top_left)?;
        let top: HtmlElement = document.create_element("div")?.dyn_into()?;
        top.set_class_name("excali-editor__top");
        layer_ui.append_child(&top)?;
        let top_right: HtmlElement = document.create_element("div")?.dyn_into()?;
        top_right.set_class_name("excali-editor__top-right");
        layer_ui.append_child(&top_right)?;

        let source = web_sys::window()
            .and_then(|w| w.location().origin().ok())
            .unwrap_or_default();
        let env = EditorEnv::new(
            CanvasMetrics::new(&document)?,
            (js_sys::Math::random() * 9_007_199_254_740_991.0) as u64,
            js_sys::Date::now,
        )
        .with_time_zone(|time| -js_sys::Date::new(&JsValue::from_f64(time)).get_timezone_offset());
        let editor = Editor::new(env, &source, is_darwin());
        let inner = Rc::new(RefCell::new(Inner {
            editor,
            host,
            container: container.clone(),
            layers,
            layer_ui,
            top,
            toolbar: None,
            footer: None,
            help_dialog: None,
            palette: None,
            palette_last_used: None,
            top_left,
            main_menu: None,
            menu_hint: None,
            welcome_center: None,
            convert_popup: None,
            focus_convert_popup: false,
            hint: None,
            cursor_hints: CursorHints::default(),
            cursor_hint: None,
            cursor_hint_nonce: 0,
            context_menu: None,
            top_right,
            library_trigger: None,
            sidebar: None,
            library_menu: LibraryMenuState::default(),
            search: search::SearchSession::default(),
            viewport_frame_pending: false,
            library_dialogs: Vec::new(),
            library_dialogs_key: None,
            previews: std::collections::HashMap::new(),
            editor_box,
            overlay: None,
            extra_tools_open: false,
            chrome_key: None,
            ui: "full".into(),
            dispatch,
            fonts_base,
            listeners: Vec::new(),
        }));

        let target: &web_sys::EventTarget = container.as_ref();
        listen(&inner, target, "keydown", |rc, event| {
            let Ok(event) = event.dyn_into::<KeyboardEvent>() else {
                return;
            };
            let mut inner = rc.borrow_mut();
            let stroke = keystroke(&event, Some(&inner.container));
            let out = inner.editor.key_down(&stroke);
            apply_outcome(&event, &out);
            inner.after_event();
            drop(inner);
            show_cursor_hint(&Rc::downgrade(rc), &out.effects);
            refresh_chrome(&Rc::downgrade(rc));
        })?;
        // the command palette's window listeners (capture phase,
        // CommandPalette.tsx:158-186 and 810-819): the toggle, then the
        // open palette's keys
        if let Some(window) = web_sys::window() {
            let window: web_sys::EventTarget = window.into();
            listen_capture(&inner, &window, "keydown", |rc, event| {
                let Ok(event) = event.dyn_into::<KeyboardEvent>() else {
                    return;
                };
                palette_window_key_down(rc, &event);
            })?;
            // the search menu's (SearchMenu.tsx:279-338), while it is
            // mounted
            listen_capture(&inner, &window, "keydown", |rc, event| {
                let Ok(event) = event.dyn_into::<KeyboardEvent>() else {
                    return;
                };
                search::window_key_down(rc, &event);
            })?;
        }
        // library items dragged from the sidebar (`App.handleAppOnDrop`,
        // `App.tsx:13194-13240`): allowed over the editor, inserted where
        // they drop
        listen(&inner, target, "dragover", |_, event| {
            let Some(drag) = event.dyn_ref::<web_sys::DragEvent>() else {
                return;
            };
            let carries_items = drag.data_transfer().is_some_and(|t| {
                let types = t.types();
                (0..types.length()).any(|i| {
                    types.get(i).as_string().as_deref()
                        == Some(excali_core::library::MIME_TYPE_EXCALIDRAWLIB_IDS)
                })
            });
            if carries_items {
                event.prevent_default();
            }
        })?;
        listen(&inner, target, "drop", |rc, event| {
            let Some(drag) = event.dyn_ref::<web_sys::DragEvent>() else {
                return;
            };
            let Some(ids) = drag
                .data_transfer()
                .and_then(|t| {
                    t.get_data(excali_core::library::MIME_TYPE_EXCALIDRAWLIB_IDS)
                        .ok()
                })
                .filter(|d| !d.is_empty())
                .and_then(|d| dropped_item_ids(&d))
            else {
                return;
            };
            event.prevent_default();
            {
                let mut inner = rc.borrow_mut();
                let items = inner.editor.library().to_vec();
                let fits = is_sidebar_docked_and_fits(&library_context(&inner, &items, &[]));
                let client = [f64::from(drag.client_x()), f64::from(drag.client_y())];
                inner.editor.insert_library(&ids, Some(client), fits);
                inner.after_event();
            }
            refresh_chrome(&Rc::downgrade(rc));
        })?;
        listen(&inner, target, "keyup", |rc, event| {
            let Ok(event) = event.dyn_into::<KeyboardEvent>() else {
                return;
            };
            let mut inner = rc.borrow_mut();
            let stroke = keystroke(&event, Some(&inner.container));
            let out = inner.editor.key_up(&stroke);
            apply_outcome(&event, &out);
            inner.after_event();
            drop(inner);
            refresh_chrome(&Rc::downgrade(rc));
        })?;
        let interactive: web_sys::EventTarget = inner
            .borrow()
            .layers
            .canvas(Layer::Interactive)
            .ok_or_else(|| JsValue::from_str("no interactive canvas"))?
            .clone()
            .into();
        listen(&inner, &interactive, "pointerdown", |rc, event| {
            let Ok(event) = event.dyn_into::<PointerEvent>() else {
                return;
            };
            let mut inner = rc.borrow_mut();
            let _ = inner.container.focus();
            if let Some(target) = event
                .target()
                .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            {
                let _ = target.set_pointer_capture(event.pointer_id());
            }
            inner.measure();
            // the cursor hint gets out of the way at once (CursorHint.tsx:175-181)
            hide_cursor_hint(&mut inner);
            inner.editor.pointer_down(pointer_input(&event));
            inner.after_event();
            drop(inner);
            refresh_chrome(&Rc::downgrade(rc));
        })?;
        listen(&inner, &interactive, "pointermove", |rc, event| {
            let Ok(event) = event.dyn_into::<PointerEvent>() else {
                return;
            };
            let mut inner = rc.borrow_mut();
            inner.editor.pointer_move(pointer_input(&event));
            inner.after_event();
            place_cursor_hint(
                &inner,
                f64::from(event.client_x()),
                f64::from(event.client_y()),
            );
            // a hover can change the hint (toggleArrowhead)
            let stale = inner.hint != current_hint(&inner);
            drop(inner);
            if stale {
                refresh_chrome(&Rc::downgrade(rc));
            }
        })?;
        for name in ["pointerup", "pointercancel"] {
            listen(&inner, &interactive, name, |rc, event| {
                let Ok(event) = event.dyn_into::<PointerEvent>() else {
                    return;
                };
                let mut inner = rc.borrow_mut();
                inner.editor.pointer_up(pointer_input(&event));
                inner.after_event();
                drop(inner);
                refresh_chrome(&Rc::downgrade(rc));
            })?;
        }
        listen_active(&inner, &interactive, "wheel", |rc, event| {
            let Ok(event) = event.dyn_into::<WheelEvent>() else {
                return;
            };
            let mut inner = rc.borrow_mut();
            let prevent = inner.editor.wheel(&WheelInput {
                delta_x: event.delta_x(),
                delta_y: event.delta_y(),
                ctrl_key: event.ctrl_key(),
                meta_key: event.meta_key(),
                shift_key: event.shift_key(),
                buttons: event.buttons(),
            });
            if prevent {
                event.prevent_default();
            }
            inner.after_event();
            drop(inner);
            refresh_chrome(&Rc::downgrade(rc));
        })?;
        // the canvas's context menu is the editor's (`handleCanvasContextMenu`
        // prevents the browser's and opens it, `openContextMenu`)
        listen(&inner, &interactive, "contextmenu", |rc, event| {
            event.prevent_default();
            let opened = {
                let mut inner = rc.borrow_mut();
                open_context_menu(&mut inner, &event)
            };
            if let Some((kind, top, left)) = opened {
                let weak = Rc::downgrade(rc);
                refresh_chrome(&weak);
                let _ = render_context_menu(&weak, kind, top, left);
            }
        })?;
        // copy, cut and paste reach the document (`App.onCopy`, `onCut`,
        // `pasteFromClipboard`)
        let document_target: web_sys::EventTarget = document.clone().into();
        for (name, kind) in [
            ("copy", ClipboardEventKind::Copy),
            ("cut", ClipboardEventKind::Cut),
            ("paste", ClipboardEventKind::Paste),
        ] {
            listen(&inner, &document_target, name, move |rc, event| {
                let Some(data) = event
                    .dyn_ref::<web_sys::ClipboardEvent>()
                    .and_then(web_sys::ClipboardEvent::clipboard_data)
                else {
                    return;
                };
                let mut inner = rc.borrow_mut();
                let pointer = inner.editor.last_pointer();
                let target = clipboard_target(
                    &event,
                    Some(inner.container.as_ref()),
                    (pointer[0], pointer[1]),
                    kind == ClipboardEventKind::Paste,
                );
                match inner.editor.clipboard_outcome(kind, target) {
                    ClipboardOutcome::Ignored => return,
                    ClipboardOutcome::Action(ActionName::Cut) => {
                        if let Some(text) = inner.editor.cut() {
                            let _ = data.set_data("text/plain", &text);
                        }
                    }
                    ClipboardOutcome::Action(_) => {
                        if let Some(text) = inner.editor.copy() {
                            let _ = data.set_data("text/plain", &text);
                        }
                    }
                    ClipboardOutcome::Paste { plain } => {
                        let text = data.get_data("text/plain").unwrap_or_default();
                        inner.editor.paste(&text, plain);
                    }
                }
                event.prevent_default();
                event.stop_propagation();
                inner.after_event();
                drop(inner);
                refresh_chrome(&Rc::downgrade(rc));
            })?;
        }
        listen(&inner, &interactive, "dblclick", |rc, event| {
            let Ok(event) = event.dyn_into::<web_sys::MouseEvent>() else {
                return;
            };
            let mut inner = rc.borrow_mut();
            inner.editor.double_click(PointerInput {
                client_x: number(&event, "clientX"),
                client_y: number(&event, "clientY"),
                button: event.button(),
                shift_key: event.shift_key(),
                alt_key: event.alt_key(),
                ctrl_or_cmd: if is_darwin() {
                    event.meta_key()
                } else {
                    event.ctrl_key()
                },
            });
            inner.after_event();
            drop(inner);
            refresh_chrome(&Rc::downgrade(rc));
        })?;

        {
            let mut i = inner.borrow_mut();
            i.measure();
            i.render();
        }
        refresh_chrome(&Rc::downgrade(&inner));
        Ok(EditorCore { inner })
    }

    /// The canvases and viewport after the host's size changed.
    pub fn resize(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.measure();
        inner.render();
    }

    /// `theme`: `"light"`, `"dark"` or `"system"` (the page's
    /// `prefers-color-scheme`).
    #[wasm_bindgen(js_name = setTheme)]
    pub fn set_theme(&self, theme: &str) -> Result<(), JsValue> {
        let dark = match theme {
            "dark" => true,
            "system" => web_sys::window()
                .and_then(|w| w.match_media("(prefers-color-scheme: dark)").ok().flatten())
                .is_some_and(|m| m.matches()),
            _ => false,
        };
        let mut inner = self.inner.borrow_mut();
        apply_theme(
            &inner.container,
            if dark { Theme::Dark } else { Theme::Light },
        )?;
        inner.editor.set_theme(dark);
        inner.render();
        Ok(())
    }

    /// `ui`: `"full"`, `"compact"`, `"mobile"` or `"auto"` show the
    /// desktop toolbar (the compact and mobile layouts are Phase 7),
    /// `"none"` hides it.
    #[wasm_bindgen(js_name = setUi)]
    pub fn set_ui(&self, ui: &str) -> Result<(), JsValue> {
        ui.clone_into(&mut self.inner.borrow_mut().ui);
        refresh_chrome(&Rc::downgrade(&self.inner));
        Ok(())
    }

    /// `load(text)`. Throws with a one-sentence reason.
    pub fn load(&self, text: &str) -> Result<(), JsValue> {
        let mut inner = self.inner.borrow_mut();
        inner.editor.load(text).map_err(js_err)?;
        inner.measure();
        inner.render();
        inner.flush();
        drop(inner);
        refresh_chrome(&Rc::downgrade(&self.inner));
        Ok(())
    }

    /// The scene as a `.excalidraw` document, for `loadSceneFonts`.
    #[wasm_bindgen(js_name = sceneJson)]
    pub fn scene_json(&self) -> String {
        self.inner.borrow_mut().editor.scene_text()
    }

    /// Paints the scene again (after its fonts loaded).
    pub fn repaint(&self) {
        self.inner.borrow_mut().render();
    }

    /// `save()`.
    pub fn save(&self) -> String {
        let mut inner = self.inner.borrow_mut();
        let text = inner.editor.save();
        inner.flush();
        text
    }

    /// `getState()`.
    #[wasm_bindgen(js_name = getState)]
    pub fn get_state(&self) -> Result<JsValue, JsValue> {
        js_sys::JSON::parse(&self.inner.borrow().editor.state().to_string())
    }

    /// `export("svg", options)`: the SVG file's text.
    #[wasm_bindgen(js_name = exportSvg)]
    pub fn export_svg(&self, options: JsValue) -> Result<String, JsValue> {
        let opts = export_options(&options)?;
        let inner = self.inner.borrow();
        inner
            .editor
            .export_svg(&opts, &FontUrls(&inner.fonts_base))
            .map_err(js_err)
    }

    /// `export("png", options)`: the PNG file's bytes, the scene embedded
    /// in a `tEXt` chunk with `embedScene` (`encodePngMetadata`).
    #[wasm_bindgen(js_name = exportPng)]
    pub fn export_png(&self, options: JsValue) -> Result<Vec<u8>, JsValue> {
        let opts = export_options(&options)?;
        let inner = self.inner.borrow();
        let doc = inner.editor.export_png(&opts).map_err(js_err)?;
        let document = inner.document();
        let canvas: HtmlCanvasElement = document.create_element("canvas")?.dyn_into()?;
        canvas.set_width(doc.width);
        canvas.set_height(doc.height);
        let context: CanvasRenderingContext2d = canvas
            .get_context("2d")?
            .ok_or_else(|| JsValue::from_str("no 2d context"))?
            .dyn_into()?;
        paint(&doc.list, &mut WebCanvas::new(context));
        let url = canvas.to_data_url_with_type("image/png")?;
        let data = url
            .split_once(',')
            .map(|(_, d)| d)
            .ok_or_else(|| js_err("the canvas gave no data URL"))?;
        let bytes: Vec<u8> = excali_core::encode::atob(data)
            .map_err(js_err)?
            .chars()
            .map(|c| c as u8)
            .collect();
        let Some(payload) = doc.payload else {
            return Ok(bytes);
        };
        let mut chunks = extract_chunks(&bytes).map_err(js_err)?;
        let chunk = encode_text_chunk(&payload.keyword, &payload.text).map_err(js_err)?;
        let last = chunks.len() - 1;
        chunks.insert(last, chunk);
        Ok(encode_chunks(&chunks))
    }

    /// Whether `input` of `importLibrary` is a URL: the URL to fetch
    /// (allow-listed, a `#addLibrary=` link resolved), or `undefined` for
    /// library text. Throws for a URL upstream's list refuses.
    #[wasm_bindgen(js_name = libraryUrl)]
    pub fn library_url(&self, input: &str) -> Result<Option<String>, JsValue> {
        match library_source(input).map_err(js_err)? {
            LibrarySource::Text => Ok(None),
            LibrarySource::Url(url) => Ok(Some(url)),
        }
    }

    /// `importLibrary(text, { merge })`: the library's item count.
    #[wasm_bindgen(js_name = importLibrary)]
    pub fn import_library(&self, text: &str, merge: bool) -> Result<u32, JsValue> {
        let n = self
            .inner
            .borrow_mut()
            .editor
            .import_library(text, merge)
            .map_err(js_err)?;
        Ok(u32::try_from(n).unwrap_or(u32::MAX))
    }

    /// The personal library as a `.excalidrawlib` file.
    #[wasm_bindgen(js_name = libraryJson)]
    pub fn library_json(&self) -> String {
        self.inner.borrow().editor.library_json()
    }

    /// Removes the editor from the host and its listeners.
    pub fn destroy(&self) {
        let mut inner = self.inner.borrow_mut();
        for (target, name, closure) in inner.listeners.drain(..) {
            let _ =
                target.remove_event_listener_with_callback(name, closure.as_ref().unchecked_ref());
            // the capture-phase ones (listen_capture)
            let _ = target.remove_event_listener_with_callback_and_bool(
                name,
                closure.as_ref().unchecked_ref(),
                true,
            );
        }
        if let Some(overlay) = inner.overlay.take() {
            overlay.unmount();
        }
        if let Some(dialog) = inner.help_dialog.take() {
            dialog.close();
        }
        for dialog in std::mem::take(&mut inner.library_dialogs) {
            dialog.close();
        }
        if let Some(palette) = inner.palette.take() {
            palette.close();
        }
        for mounted in [
            inner.toolbar.take(),
            inner.footer.take(),
            inner.main_menu.take(),
            inner.context_menu.take(),
            inner.library_trigger.take(),
            inner.sidebar.take(),
        ]
        .into_iter()
        .flatten()
        {
            mounted.remove();
        }
        inner.container.remove();
    }
}

fn export_options(options: &JsValue) -> Result<ExportOptions, JsValue> {
    if options.is_undefined() || options.is_null() {
        return Ok(ExportOptions::default());
    }
    let text: String = js_sys::JSON::stringify(options)?.into();
    let value: Value = serde_json::from_str(&text).map_err(js_err)?;
    Ok(ExportOptions::from_json(&value))
}
