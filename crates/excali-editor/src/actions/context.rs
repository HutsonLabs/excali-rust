//! What an action's `keyTest`, `predicate`, `label` and `checked` read:
//! the scene's elements, the app state, the host's props and the editor
//! environment (`app` upstream), plus the selection helpers the predicates
//! share (`packages/element/src/selection.ts`, `groups.ts`,
//! `typeChecks.ts`).

use std::collections::{HashMap, HashSet};

use excali_core::app_state::AppState;
use excali_core::element::{BoundElementType, Element, ElementKind, ElementType};
use serde_json::Value;

use crate::js_value::truthy;

/// `editorInterface.formFactor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FormFactor {
    Phone,
    Tablet,
    #[default]
    Desktop,
}

/// `UIOptions.canvasActions` after the component merges the host's options
/// over `DEFAULT_UI_OPTIONS.canvasActions` (`constants.ts:384-393`,
/// `index.tsx:121-147`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanvasActions {
    pub change_view_background_color: bool,
    pub clear_canvas: bool,
    /// `export`: `false` (`None`) or `{ saveFileToDisk }`.
    pub export: Option<bool>,
    pub load_scene: bool,
    pub save_to_active_file: bool,
    /// `toggleTheme`: `null` (`None`) until [`CanvasActions::normalize`]
    /// settles it.
    pub toggle_theme: Option<bool>,
    pub save_as_image: bool,
}

impl Default for CanvasActions {
    fn default() -> CanvasActions {
        CanvasActions {
            change_view_background_color: true,
            clear_canvas: true,
            export: Some(true),
            load_scene: true,
            save_to_active_file: true,
            toggle_theme: None,
            save_as_image: true,
        }
    }
}

impl CanvasActions {
    /// `index.tsx:142-147`: a `null` `toggleTheme` becomes `true` when the
    /// host does not pass `theme`, or passes `onThemeChange`.
    pub fn normalize(&mut self, theme_prop_set: bool, on_theme_change_set: bool) {
        if self.toggle_theme.is_none() && (!theme_prop_set || on_theme_change_set) {
            self.toggle_theme = Some(true);
        }
    }

    /// `name in canvasActions ? canvasActions[name] : true`, read for
    /// truthiness (`manager.tsx:102-104, 186-189`). Only five action names
    /// are keys of `canvasActions`.
    pub fn allows(&self, name: &str) -> bool {
        match name {
            "changeViewBackgroundColor" => self.change_view_background_color,
            "clearCanvas" => self.clear_canvas,
            "loadScene" => self.load_scene,
            "saveToActiveFile" => self.save_to_active_file,
            "toggleTheme" => self.toggle_theme == Some(true),
            _ => true,
        }
    }
}

/// The host props the predicates read (`ExcalidrawProps`). `None` is a
/// prop left unset (`undefined`), which hands the matching toggle to the
/// user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AppProps {
    pub canvas_actions: CanvasActions,
    pub grid_mode_enabled: Option<bool>,
    pub zen_mode_enabled: Option<bool>,
    pub view_mode_enabled: Option<bool>,
    pub objects_snap_mode_enabled: Option<bool>,
}

/// The editor environment: platform, form factor and what `App` answers
/// about interaction and navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionEnv {
    /// `isDarwin` (`editorInterface.ts:37`): CTRL_OR_CMD is `metaKey` and
    /// Alt reads "Option".
    pub is_darwin: bool,
    pub form_factor: FormFactor,
    /// `app.isInteractionEnabled()`.
    pub interaction_enabled: bool,
    /// `app.isNavigationEnabled()`.
    pub navigation_enabled: bool,
    /// `probablySupportsClipboardWriteText` (`clipboard.ts`).
    pub clipboard_write_text: bool,
    /// `probablySupportsClipboardBlob` (`clipboard.ts`).
    pub clipboard_blob: bool,
    /// `app.viewport.isLockedTransitionPending`.
    pub viewport_transition_pending: bool,
}

impl Default for ActionEnv {
    /// Not a Mac, desktop, interactive and navigable, without clipboard
    /// write support (as under jsdom).
    fn default() -> ActionEnv {
        ActionEnv {
            is_darwin: false,
            form_factor: FormFactor::Desktop,
            interaction_enabled: true,
            navigation_enabled: true,
            clipboard_write_text: false,
            clipboard_blob: false,
            viewport_transition_pending: false,
        }
    }
}

/// Everything an action's callbacks read. `elements` is what upstream
/// hands the callback: the scene including deleted elements for the
/// action manager, the non-deleted elements for the context menu and the
/// command palette.
#[derive(Debug, Clone, Copy)]
pub struct ActionContext<'a> {
    pub elements: &'a [Element],
    pub app_state: &'a AppState,
    pub props: &'a AppProps,
    pub env: &'a ActionEnv,
}

impl<'a> ActionContext<'a> {
    /// `appState[key]`, `None` when absent.
    pub(crate) fn get(&self, key: &str) -> Option<&'a Value> {
        self.app_state.get(key)
    }

    /// `!!appState[key]`.
    pub(crate) fn flag(&self, key: &str) -> bool {
        truthy(self.get(key))
    }

    /// `appState[key]?.[field]`.
    pub(crate) fn field(&self, key: &str, field: &str) -> Option<&'a Value> {
        self.get(key).and_then(|v| v.get(field))
    }

    /// `appState[key] === null` (strict: an absent key is `undefined`).
    pub(crate) fn is_null(&self, key: &str) -> bool {
        matches!(self.get(key), Some(Value::Null))
    }

    /// `appState.openDialog?.name`.
    pub(crate) fn open_dialog_name(&self) -> Option<&'a str> {
        self.field("openDialog", "name").and_then(Value::as_str)
    }

    /// `appState.selectedElementIds[id]` is truthy.
    pub(crate) fn is_selected_id(&self, id: &str) -> bool {
        truthy(self.field("selectedElementIds", id))
    }

    /// `appState.editingGroupId`, when a non-empty string.
    fn editing_group_id(&self) -> Option<&'a str> {
        self.get("editingGroupId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    }

    /// `appState.selectedGroupIds[id]` is truthy.
    fn is_selected_group(&self, id: &str) -> bool {
        truthy(self.field("selectedGroupIds", id))
    }

    /// The non-deleted elements.
    pub(crate) fn non_deleted(&self) -> impl Iterator<Item = &'a Element> {
        self.elements.iter().filter(|e| !e.base.is_deleted)
    }

    /// `getSelectedElements(elements, appState, { includeBoundTextElement })`
    /// (`selection.ts:161-215`); `app.scene.getSelectedElements` gives the
    /// same over the scene. Deleted elements are never selected.
    pub(crate) fn selected(&self, include_bound_text: bool) -> Vec<&'a Element> {
        self.elements
            .iter()
            .filter(|e| {
                if e.base.is_deleted {
                    return false;
                }
                if self.is_selected_id(&e.base.id) {
                    return true;
                }
                include_bound_text && container_id(e).is_some_and(|c| self.is_selected_id(c))
            })
            .collect()
    }

    /// `isSomeElementSelected(nonDeletedElements, appState)`.
    pub(crate) fn is_some_element_selected(&self) -> bool {
        self.non_deleted().any(|e| self.is_selected_id(&e.base.id))
    }

    /// `getSelectedGroupIds(appState)` (`groups.ts:234-239`).
    pub(crate) fn selected_group_ids(&self) -> Vec<&'a str> {
        match self.get("selectedGroupIds") {
            Some(Value::Object(map)) => map
                .iter()
                .filter(|(_, v)| truthy(Some(v)))
                .map(|(k, _)| k.as_str())
                .collect(),
            _ => Vec::new(),
        }
    }

    /// `getBoundTextElement(container, nonDeletedElementsMap)`
    /// (`textElement.ts:332-356`): the first text in `boundElements`, if
    /// it is a non-deleted element of the scene.
    pub(crate) fn bound_text_of(&self, container: &Element) -> Option<&'a Element> {
        let id = container
            .base
            .bound_elements
            .as_ref()?
            .iter()
            .find(|b| b.kind == BoundElementType::Text)?
            .id
            .as_str();
        self.non_deleted().find(|e| e.base.id == id)
    }

    /// The number of buckets `getSelectedElementsByGroup` sorts the
    /// selection into (`groups.ts:417-461`): each selected group (or, when
    /// a single group is selected and holds the whole selection, each of
    /// its inner groups) and each loose element; bound text goes with its
    /// container.
    pub(crate) fn selected_units(&self, selected: &[&Element]) -> usize {
        let selected_group_ids = self.selected_group_ids();
        let single_group =
            selected_group_ids.len() == 1 && selected.iter().all(|e| self.is_selected_via_group(e));
        let mut buckets: Vec<String> = Vec::new();
        for element in selected {
            if is_bound_to_container(element) {
                continue;
            }
            let group_ids = &element.base.group_ids;
            let key = match group_ids.iter().find(|g| self.is_selected_group(g)) {
                None => format!("{}_element", element.base.id),
                Some(selected_group) => {
                    let index = group_ids
                        .iter()
                        .position(|g| g == selected_group)
                        .unwrap_or(0) as isize;
                    let key_index = if single_group { index - 1 } else { index };
                    if key_index < 0 {
                        format!("{}_element", element.base.id)
                    } else {
                        format!("{}_group", group_ids[key_index as usize])
                    }
                }
            };
            if !buckets.contains(&key) {
                buckets.push(key);
            }
        }
        buckets.len()
    }

    /// `isSelectedViaGroup(appState, element)` (`groups.ts:215-232`).
    fn is_selected_via_group(&self, element: &Element) -> bool {
        let editing = self.editing_group_id();
        element
            .base
            .group_ids
            .iter()
            .filter(|g| Some(g.as_str()) != editing)
            .any(|g| self.is_selected_group(g))
    }

    /// `allElementsInSameGroup(elements, editingGroupId)`
    /// (`actionGroup.tsx:52-71`).
    pub(crate) fn all_in_same_group(&self, elements: &[&Element]) -> bool {
        if elements.len() < 2 {
            return false;
        }
        let first = &elements[0].base.group_ids;
        let editing_index = self
            .editing_group_id()
            .and_then(|g| first.iter().position(|x| x == g));
        let group_ids = match editing_index {
            Some(i) => &first[..i],
            None => &first[..],
        };
        group_ids
            .iter()
            .any(|g| elements.iter().all(|e| e.base.group_ids.contains(g)))
    }
}

/// `element.containerId` when the element is text with a container
/// (`isBoundToContainer`, `typeChecks.ts:307-316`: any non-null id).
pub(crate) fn container_id(element: &Element) -> Option<&str> {
    match &element.kind {
        ElementKind::Text(t) => t.container_id.as_deref(),
        _ => None,
    }
}

/// `isBoundToContainer`.
pub(crate) fn is_bound_to_container(element: &Element) -> bool {
    container_id(element).is_some()
}

/// `isTextBindableContainer(element)` with locked elements included
/// (`typeChecks.ts:240-253`).
pub fn is_text_bindable_container(element: &Element) -> bool {
    element.element_type().is_text_container()
}

/// `hasBoundTextElement` (`typeChecks.ts:297-305`).
pub fn has_bound_text_element(element: &Element) -> bool {
    is_text_bindable_container(element)
        && element
            .base
            .bound_elements
            .as_ref()
            .is_some_and(|b| b.iter().any(|b| b.kind == BoundElementType::Text))
}

/// `isFrameLikeElement`.
pub(crate) fn is_frame_like(element: &Element) -> bool {
    element.element_type().is_frame_like()
}

pub(crate) fn is_type(element: &Element, ty: ElementType) -> bool {
    element.element_type() == ty
}

/// `elementsAreInSameGroup` (`groups.ts:376-391`): some group holds every
/// element.
pub fn elements_are_in_same_group(elements: &[&Element]) -> bool {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    let mut max = 0;
    for element in elements {
        for group in &element.base.group_ids {
            let c = counts.entry(group.as_str()).or_insert(0);
            *c += 1;
            max = max.max(*c);
        }
    }
    max == elements.len()
}

/// `canCreateLinkFromElements` (`elementLink.ts:68-80`).
pub(crate) fn can_create_link_from_elements(selected: &[&Element]) -> bool {
    selected.len() == 1 || (selected.len() > 1 && elements_are_in_same_group(selected))
}

/// `frameAndChildrenSelectedTogether` (`frame.ts:1000-1011`).
pub fn frame_and_children_selected_together(selected: &[&Element]) -> bool {
    let ids: HashSet<&str> = selected.iter().map(|e| e.base.id.as_str()).collect();
    selected.len() > 1
        && selected.iter().any(|e| {
            e.base
                .frame_id
                .as_deref()
                .is_some_and(|f| !f.is_empty() && ids.contains(f))
        })
}
