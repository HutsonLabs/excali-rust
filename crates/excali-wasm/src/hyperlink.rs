//! The hyperlink popup in the editor (`components/hyperlink/Hyperlink.tsx`
//! as `App` mounts it, `App.tsx:2549-2565`).
//!
//! `actionLink` (Ctrl/Cmd+K, `actions/actionLink.tsx:20-42`) opens it in
//! the editor mode. While it shows for an element the editor keeps what
//! its input holds (React's `inputVal`, the element's link when it
//! mounted); Enter or Escape submits `normalizeLink(input) || null` and
//! shows the info popup, the remove button clears the link and closes it,
//! and the popup going away while its input shows submits what was typed
//! (the layout effect's cleanup, `Hyperlink.tsx:182-186`). Leaving the
//! selection tool closes it (`App.tsx:4399-4406`).
//!
//! Upstream writes the link with `scene.mutateElement`, which the store
//! records with the next captured update; the element schedules a capture
//! with it, so a changed link is one history entry of its own. Reduced
//! from upstream: the "Link to object" button's element link dialog
//! (`ElementLinkDialog`) is not ported, so the button does nothing; an
//! embeddable's link is set without `embeddableURLValidator`, the embed's
//! video resize and its toast.

use excali_editor::hyperlink::{
    hyperlink_panel, submitted_link, HyperlinkEvent, HyperlinkMode, HyperlinkPanel,
};
use excali_editor::mutate::bump_version;
use excali_editor::scene::Scene;
use excali_editor::tools::ToolType;
use excali_text::text_measurements::TextMetricsProvider;
use serde_json::{json, Map, Value};

use crate::editor::Editor;

/// The popup while it is mounted for an element.
#[derive(Clone, Debug, Default)]
pub(crate) struct HyperlinkMount {
    pub element_id: String,
    /// `inputVal`.
    pub input: String,
    /// The input shows (`showHyperlinkPopup === "editor"`).
    pub editing: bool,
}

impl<P: TextMetricsProvider + Clone> Editor<P> {
    /// The popup the container shows, `None` while it does not.
    pub fn hyperlink_panel(&self) -> Option<HyperlinkPanel> {
        let scene = Scene::new(self.session.elements().to_vec());
        hyperlink_panel(&scene, self.session.app_state())
    }

    /// What the popup's input holds while the popup is mounted.
    pub fn hyperlink_input(&self) -> Option<&str> {
        self.hyperlink.as_ref().map(|m| m.input.as_str())
    }

    /// What the popup's handlers report.
    pub fn hyperlink_event(&mut self, event: HyperlinkEvent) {
        let Some(id) = self.hyperlink.as_ref().map(|m| m.element_id.clone()) else {
            return;
        };
        match event {
            HyperlinkEvent::Input(value) => {
                if let Some(m) = self.hyperlink.as_mut() {
                    m.input = value;
                }
                return;
            }
            HyperlinkEvent::Submit(value) => {
                if let Some(m) = self.hyperlink.as_mut() {
                    m.input.clone_from(&value);
                }
                self.set_element_link(&id, submitted_link(&value));
                self.set_hyperlink_popup(json!(HyperlinkMode::Info.as_str()));
            }
            HyperlinkEvent::Edit => {
                self.set_hyperlink_popup(json!(HyperlinkMode::Editor.as_str()));
            }
            HyperlinkEvent::Remove => {
                self.set_element_link(&id, None);
                self.set_hyperlink_popup(Value::Bool(false));
            }
            HyperlinkEvent::LinkToElement => return,
        }
        self.report();
    }

    fn set_hyperlink_popup(&mut self, value: Value) {
        let mut patch = Map::new();
        patch.insert("showHyperlinkPopup".into(), value);
        self.session.set_state(patch);
        self.session.commit();
    }

    /// `scene.mutateElement(element, { link })`, captured when it changed.
    fn set_element_link(&mut self, id: &str, link: Option<String>) {
        let mut elements = self.session.elements().to_vec();
        let Some(element) = elements
            .iter_mut()
            .find(|e| e.base.id == id && !e.base.is_deleted)
        else {
            return;
        };
        if element.base.link == link {
            return;
        }
        element.base.link = link;
        bump_version(element, None, &mut self.session.env);
        // Ok: one element's link changed, the indices stay valid
        let _ = self.session.replace_all_elements(elements);
        self.session.store.schedule_capture();
        self.session.commit();
    }

    /// Mounts, keeps or unmounts the popup after a step, as React would:
    /// a popup that goes away (or moves to another element) while its
    /// input shows submits the input.
    pub(crate) fn sync_hyperlink(&mut self) {
        let selection = self.tools.active_tool.tool.builtin() == Some(ToolType::Selection);
        let left_selection = self.hyperlink_selection_tool && !selection;
        self.hyperlink_selection_tool = selection;
        if left_selection
            && self
                .session
                .app_state()
                .get("showHyperlinkPopup")
                .is_some_and(|v| v != &Value::Bool(false))
        {
            self.set_hyperlink_popup(Value::Bool(false));
        }
        let panel = self.hyperlink_panel();
        if let (Some(m), Some(p)) = (self.hyperlink.as_mut(), panel.as_ref()) {
            if m.element_id == p.element_id {
                m.editing = p.mode == HyperlinkMode::Editor;
                return;
            }
        }
        if let Some(m) = self.hyperlink.take() {
            if m.editing {
                self.set_element_link(&m.element_id, submitted_link(&m.input));
            }
        }
        self.hyperlink = panel.map(|p| HyperlinkMount {
            input: p.link.clone().unwrap_or_default(),
            editing: p.mode == HyperlinkMode::Editor,
            element_id: p.element_id,
        });
    }
}
