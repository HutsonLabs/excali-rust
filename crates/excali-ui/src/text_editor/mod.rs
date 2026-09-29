//! The text editor's `<textarea>` in the page: `excali_editor::text_editing`'s
//! [`TextEditor`] mounted in the editor's `.excalidraw-textEditorContainer`
//! box, its style and value applied, and the textarea's and window's events
//! handed back.
//!
//! Upstream counterpart: the DOM half of
//! `packages/excalidraw/wysiwyg/textWysiwyg.tsx` (the textarea, its event
//! handlers, `bindBlurEvent`, `temporarilyDisableSubmit`, `onPointerDown`,
//! the `ResizeObserver`, the editor box's scroll, and
//! `getLineCaretOffsetFromNativeLayout`) and the stylesheet rules of the
//! editor box (`css/styles.scss:8`, `:134-149`).
//!
//! - **Mount.** [`TextEditorOverlay::mount`] creates the textarea with
//!   [`TEXTAREA_ATTRIBUTES`] (`dir="auto"`, `wrap="off"`,
//!   `data-type="wysiwyg"`, class `excalidraw-wysiwyg`), applies the
//!   editor's state and appends it to the editor box. After the pointer
//!   down that opened it (a timeout) the textarea is focused without
//!   scrolling and a blur submits from then on.
//! - **Events.** Every event goes to a [`TextareaHandler`] (the app, which
//!   owns the scene and the [`TextEditor`]) as a [`TextareaEvent`] with the
//!   textarea's value and selection; the handler answers with whether to
//!   prevent the browser's default and the editor's new
//!   [`TextareaState`], which the overlay applies.
//! - **Scene updates.** When the scene changes while the editor is open
//!   (`app.scene.onUpdate`) the app relays out the editor
//!   (`TextEditor::relayout`) and hands its state to
//!   [`TextEditorOverlay::scene_updated`], which applies it and focuses the
//!   textarea again without scrolling unless the focus is in a properties
//!   popover ([`refocuses_on_scene_update`]).
//! - **Submit without blur.** A pointer down on the canvas submits on the
//!   next frame (mobile browsers do not always blur); one in the styles
//!   panel, its popovers or the zoom actions suspends the blur submit until
//!   the pointer is released outside them; the window's blur and
//!   `beforeunload` submit.
//!
//! The pure rules ([`css_property_name`], [`classify_pointer_down`],
//! [`refocuses_on_scene_update`], [`closest_caret_offset`], ...) are tested
//! natively; the DOM half in Chromium (`tests/web/text-editing`).

mod dom;

use excali_editor::session::Session;
use excali_editor::store::HistoryEnv;
use excali_editor::text_editing::{
    KeyDown, PasteOutcome, TextEditingContext, TextEditingError, TextEditingHost, TextEditor,
};
pub use excali_editor::text_editing::{TextareaAttributes, TEXTAREA_ATTRIBUTES};
use excali_editor::text_layout::TextLayouter;
use excali_text::text_measurements::TextMetricsProvider;

pub use dom::{measure_caret_offset, TextEditorOverlay};

/// The editor box's and the textarea's rules of upstream's stylesheet
/// (`css/styles.scss:8`, `:134-149`), under the editor's `.excalidraw`
/// container: the box covers the canvas area and clips, and lets the
/// pointer through to the canvas except over the textarea; the textarea's
/// stacking level.
pub const TEXT_EDITOR_CSS: &str = "\
.excalidraw {
  --zIndex-wysiwyg: 3;
}
.excalidraw .excalidraw-textEditorContainer {
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  left: 0;
  overflow: hidden;
  pointer-events: none;
}
.excalidraw .excalidraw-textEditorContainer > textarea {
  pointer-events: auto;
}
";

/// The CSS property a CSSOM name assigns: `zIndex` is `z-index`,
/// `backfaceVisibility` `backface-visibility`.
pub fn css_property_name(cssom: &str) -> String {
    let mut out = String::with_capacity(cssom.len() + 4);
    for c in cssom.chars() {
        if c.is_ascii_uppercase() {
            out.push('-');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// `isDarwin` (`packages/common/src/editorInterface.ts:37`): the platform
/// names a Mac or an iOS device, where Cmd stands for Ctrl
/// (`KEYS.CTRL_OR_CMD` is `metaKey`).
pub fn is_darwin(platform: &str) -> bool {
    ["Mac", "iPod", "iPhone", "iPad"]
        .iter()
        .any(|name| platform.contains(name))
}

/// The textarea as the editor last left it.
#[derive(Debug, Clone, PartialEq)]
pub struct TextareaState {
    pub value: String,
    /// `[selectionStart, selectionEnd]` in UTF-16 code units.
    pub selection: (usize, usize),
    /// Every style property assigned, by its CSSOM name, in first
    /// assignment order.
    pub style: Vec<(String, String)>,
    /// The editor box's `left` and `right`, px.
    pub editor_box_insets: (f64, f64),
    /// The editor is still open; a closed editor's textarea is removed.
    pub open: bool,
}

impl TextareaState {
    /// The state of `editor`.
    pub fn of(editor: &TextEditor) -> TextareaState {
        TextareaState {
            value: editor.value().to_owned(),
            selection: editor.selection(),
            style: editor
                .style()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            editor_box_insets: editor.editor_box_insets(),
            open: editor.is_open(),
        }
    }
}

/// A keydown, as the editor reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextareaKey {
    pub key: String,
    pub code: String,
    pub shift_key: bool,
    pub alt_key: bool,
    /// `event[KEYS.CTRL_OR_CMD]`.
    pub ctrl_or_cmd: bool,
    pub is_composing: bool,
    pub key_code: u32,
}

/// What happened to the textarea, for the app to hand to its
/// [`TextEditor`]. `selection` is the textarea's at the time.
#[derive(Debug, Clone, PartialEq)]
pub enum TextareaEvent {
    /// Focused after the pointer down that opened it: a pending caret
    /// goes in (`TextEditor::focused`).
    Focused,
    /// `input`: `TextEditor::input`.
    Input {
        value: String,
        selection: (usize, usize),
    },
    /// The selection moved (`selectionchange`): the caret or selection the
    /// editor reads (upstream reads the textarea's whenever it needs it).
    Select { selection: (usize, usize) },
    /// `keydown`: `TextEditor::keydown` (after the selection is synced).
    KeyDown {
        key: TextareaKey,
        selection: (usize, usize),
    },
    /// `paste`: `TextEditor::paste`, with the clipboard's MIME types and
    /// its `text/plain` string.
    Paste {
        types: Vec<String>,
        text: Option<String>,
        selection: (usize, usize),
    },
    /// Submit (`TextEditor::submit`): the textarea blurred while a blur
    /// submits, a pointer down on the canvas, the window blurred while the
    /// blur submit was suspended, or the page unloading.
    Submit,
    /// The editor box scrolled to reveal the caret (the overlay put it
    /// back): `TextEditor::editor_box_scrolled`.
    EditorBoxScrolled { left: f64, top: f64 },
    /// The canvas was resized: `TextEditor::relayout`.
    Resized,
    /// A middle-button press on the textarea starts a pan (`app.pan.start`).
    PanStart { client_x: f64, client_y: f64 },
}

/// The app's answer to a [`TextareaEvent`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Handled {
    /// `event.preventDefault()`.
    pub prevent_default: bool,
    /// The editor's state after the event (`None` when nothing changed).
    pub state: Option<TextareaState>,
}

/// The app side of the textarea.
pub trait TextareaHandler {
    fn on_event(&mut self, event: TextareaEvent) -> Handled;
}

/// The app's side of the textarea: the scene with its store and history,
/// text layout, the host, and the open [`TextEditor`]; each
/// [`TextareaEvent`] goes to the editor as upstream's handler does.
pub struct TextEditingApp<E: HistoryEnv, P: TextMetricsProvider, H: TextEditingHost> {
    pub session: Session<E>,
    pub layouter: TextLayouter<P>,
    pub host: H,
    pub editor: Option<TextEditor>,
    /// What the editor asked of the app (`AppCall::name`), and a middle
    /// button pan start (`panStart`), in order.
    pub calls: Vec<String>,
    /// Errors the editor reported (upstream throws), in order.
    pub errors: Vec<String>,
}

impl<E: HistoryEnv, P: TextMetricsProvider, H: TextEditingHost> TextEditingApp<E, P, H> {
    pub fn new(session: Session<E>, layouter: TextLayouter<P>, host: H) -> Self {
        TextEditingApp {
            session,
            layouter,
            host,
            editor: None,
            calls: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Runs `f` on the open editor with the editing context, then records
    /// the editor's calls; the editor's state after, `None` without one.
    pub fn with_editor<R>(
        &mut self,
        f: impl FnOnce(
            &mut TextEditor,
            &mut TextEditingContext<'_, E, P>,
        ) -> Result<R, TextEditingError>,
    ) -> Option<(R, TextareaState)> {
        let editor = self.editor.as_mut()?;
        let mut ctx = TextEditingContext {
            session: &mut self.session,
            layouter: &mut self.layouter,
            host: &mut self.host,
        };
        let result = f(editor, &mut ctx);
        self.calls
            .extend(editor.take_calls().iter().map(|c| c.name()));
        let state = TextareaState::of(editor);
        match result {
            Ok(r) => Some((r, state)),
            Err(e) => {
                self.errors.push(e.to_string());
                None
            }
        }
    }
}

impl<E: HistoryEnv, P: TextMetricsProvider, H: TextEditingHost> TextareaHandler
    for TextEditingApp<E, P, H>
{
    fn on_event(&mut self, event: TextareaEvent) -> Handled {
        let answer = match event {
            TextareaEvent::Focused => self.with_editor(|editor, _| {
                editor.focused();
                Ok(false)
            }),
            TextareaEvent::Select { selection } => self.with_editor(|editor, _| {
                editor.set_selection(selection.0, selection.1);
                Ok(false)
            }),
            TextareaEvent::Input { value, selection } => {
                self.with_editor(|editor, ctx| editor.input(ctx, &value, selection).map(|_| false))
            }
            TextareaEvent::KeyDown { key, selection } => self.with_editor(|editor, ctx| {
                editor.set_selection(selection.0, selection.1);
                let outcome = editor.keydown(
                    ctx,
                    &KeyDown {
                        key: &key.key,
                        code: &key.code,
                        shift_key: key.shift_key,
                        alt_key: key.alt_key,
                        ctrl_or_cmd: key.ctrl_or_cmd,
                        is_composing: key.is_composing,
                        key_code: key.key_code,
                    },
                )?;
                Ok(outcome.prevent_default)
            }),
            TextareaEvent::Paste {
                types,
                text,
                selection,
            } => self.with_editor(|editor, ctx| {
                editor.set_selection(selection.0, selection.1);
                let types: Vec<&str> = types.iter().map(String::as_str).collect();
                let outcome = editor.paste(ctx, &types, text.as_deref())?;
                Ok(outcome == PasteOutcome::Prevented)
            }),
            TextareaEvent::Submit => {
                self.with_editor(|editor, ctx| editor.submit(ctx).map(|_| false))
            }
            TextareaEvent::EditorBoxScrolled { left, top } => self.with_editor(|editor, ctx| {
                editor.editor_box_scrolled(ctx, left, top).map(|_| false)
            }),
            TextareaEvent::Resized => {
                self.with_editor(|editor, ctx| editor.relayout(ctx).map(|_| false))
            }
            TextareaEvent::PanStart { .. } => {
                self.calls.push("panStart".into());
                None
            }
        };
        match answer {
            Some((prevent_default, state)) => Handled {
                prevent_default,
                state: Some(state),
            },
            None => Handled::default(),
        }
    }
}

/// Where a pointer down outside the textarea landed, as `onPointerDown`
/// reads its target (`textWysiwyg.tsx:996-1054`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PointerDownTarget {
    /// `event.button`.
    pub button: i16,
    /// The target is a `<textarea>`.
    pub textarea: bool,
    /// The target is a `<canvas>`.
    pub canvas: bool,
    /// The target has the class `properties-trigger`.
    pub properties_trigger: bool,
    /// The target is inside `.properties-content`.
    pub in_properties_content: bool,
    /// The target is inside the shape actions menu (`.App-menu__left`),
    /// the zoom actions (`.zoom-actions`) or the compact shape actions
    /// island.
    pub in_actions_menu: bool,
    /// `isWritableElement(target)`.
    pub writable: bool,
}

/// What a pointer down outside the editor does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerDownAction {
    /// The middle button: suspend the blur submit; on the textarea itself
    /// also prevent the default and start a pan.
    Pan { on_textarea: bool },
    /// A styles panel or zoom control: suspend the blur submit until the
    /// pointer is released.
    SuspendSubmit,
    /// The canvas: submit on the next frame.
    SubmitNextFrame,
    /// Anything else: the blur submits, if it comes.
    Nothing,
}

/// `onPointerDown(event)` (`textWysiwyg.tsx:996-1054`).
pub fn classify_pointer_down(target: &PointerDownTarget) -> PointerDownAction {
    // POINTER_BUTTON.WHEEL
    if target.button == 1 {
        return PointerDownAction::Pan {
            on_textarea: target.textarea,
        };
    }
    if (target.in_actions_menu && !target.writable)
        || target.properties_trigger
        || target.in_properties_content
    {
        return PointerDownAction::SuspendSubmit;
    }
    if target.canvas {
        return PointerDownAction::SubmitNextFrame;
    }
    PointerDownAction::Nothing
}

/// `bindBlurEvent(event)` after a suspended submit (`textWysiwyg.tsx:920-948`):
/// whether the pointer up re-arms the blur submit (and refocuses the
/// textarea). A release inside the actions menu, on a properties trigger
/// or in a properties popover keeps it suspended.
pub fn rearms_on_pointer_up(target: &PointerDownTarget) -> bool {
    !(target.in_actions_menu || target.properties_trigger || target.in_properties_content)
}

/// The scene update's popup check (`textWysiwyg.tsx:1053-1061`): after a
/// scene change the textarea takes the focus back from `active` (the
/// document's focused element) unless it is inside a properties popover
/// (`ownerDocument.activeElement?.closest(".properties-content")`). Focus
/// in the shape actions menu or on a properties trigger is taken back.
pub fn refocuses_on_scene_update(active: &PointerDownTarget) -> bool {
    !active.in_properties_content
}

/// `getCaretBoundaryOffsets(text)` (`textWysiwyg.tsx:117-126`): the UTF-16
/// offset of every code point boundary.
pub fn caret_boundary_offsets(text: &str) -> Vec<usize> {
    let mut offsets = vec![0];
    let mut offset = 0;
    for c in text.chars() {
        offset += c.len_utf16();
        offsets.push(offset);
    }
    offsets
}

/// The closest caret boundary to `target_x` (the end of
/// `getLineCaretOffsetFromNativeLayout`, `textWysiwyg.tsx:190-204`):
/// `positions[i]` is the caret's left edge at `offsets[i]`, measured from
/// the line's leftmost caret position; the first of equally close
/// boundaries wins.
pub fn closest_caret_offset(offsets: &[usize], positions: &[f64], target_x: f64) -> usize {
    let left_edge = positions.iter().copied().fold(f64::INFINITY, f64::min);
    let mut closest = offsets.first().copied().unwrap_or(0);
    let mut closest_distance = f64::INFINITY;
    for (offset, position) in offsets.iter().zip(positions) {
        let distance = (position - left_edge - target_x).abs();
        if distance < closest_distance {
            closest_distance = distance;
            closest = *offset;
        }
    }
    closest
}
