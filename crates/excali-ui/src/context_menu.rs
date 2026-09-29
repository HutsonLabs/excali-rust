//! The context menu (stub).

use std::rc::Rc;

use excali_editor::actions::{ActionName, ContextMenuEntry};

use crate::dom::Element;

pub const CONTEXT_MENU_CSS: &str = "";

#[derive(Clone, Debug, PartialEq)]
pub enum ContextMenuEffect {
    Close,
    ExecuteAction(ActionName),
    PreventDefault,
}

pub type OnContextMenuEffect = Rc<dyn Fn(ContextMenuEffect)>;

#[derive(Clone)]
pub struct ContextMenuProps {
    pub top: f64,
    pub left: f64,
    pub viewport_width: f64,
    pub viewport_height: f64,
    pub on_effect: Option<OnContextMenuEffect>,
}

pub struct ContextMenu {
    pub element: Element,
    pub handlers: Vec<(&'static str, Vec<ContextMenuEffect>)>,
}

pub fn context_menu_text(key: &str) -> &str {
    key
}

pub fn context_menu(_entries: &[ContextMenuEntry], _props: ContextMenuProps) -> ContextMenu {
    ContextMenu {
        element: Element::new("div"),
        handlers: Vec::new(),
    }
}
