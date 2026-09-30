//! The dialogs the library's header menu opens
//! (`LibraryMenuHeaderContent.tsx:62-88,106-140,244-272`): upstream's
//! `ConfirmDialog` (`ConfirmDialog.tsx`) for Reset library and Remove,
//! `PublishLibrary` (`PublishLibrary.tsx`) for Rename or publish, and the
//! publish success dialog, each a [`dialog`] with upstream's
//! `DialogActionButton` (`DialogActionButton.tsx`) buttons.
//!
//! [`library_dialogs`] builds the open ones from the menu's state
//! ([`LibraryMenuState::confirm_open`], [`LibraryMenuState::publish`],
//! [`LibraryMenuState::publish_success`]); what the user does in them comes
//! back as [`LibrarySidebarEvent`]s for [`super::update`].

use std::collections::HashMap;
use std::rc::Rc;

use excali_core::library::{LibraryItem, LibraryItemStatus};
use excali_scene::shape::Theme;
use serde_json::{json, Map, Value};
use wasm_bindgen::JsCast;
use web_sys::{Event, HtmlInputElement, HtmlTextAreaElement};

use crate::dom::{class_names, Element, Node};
use crate::icons;
use crate::primitives::{
    dialog, icon_button, DialogProps, DialogSize, IconButtonProps, CLOSE_LABEL,
};

use super::{
    emit, icon_node, in_library_order, library_text, spinner, LibraryContext, LibraryMenuState,
    LibrarySidebarEvent, OnLibrarySidebarEvent,
};

/// `EDITOR_LS_KEYS.PUBLISH_LIBRARY` (`common/src/constants.ts:575`): where
/// the publish dialog keeps its fields between openings.
pub const PUBLISH_LIBRARY_STORAGE_KEY: &str = "publish-library-data";

/// A field of the publish dialog's form (`PublishLibraryDataParams`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublishField {
    AuthorName,
    GithubHandle,
    Name,
    Description,
    TwitterHandle,
    Website,
}

impl PublishField {
    /// In `useState`'s key order.
    pub const ALL: [PublishField; 6] = [
        PublishField::AuthorName,
        PublishField::GithubHandle,
        PublishField::Name,
        PublishField::Description,
        PublishField::TwitterHandle,
        PublishField::Website,
    ];

    /// The key, and the input's `name`.
    pub fn key(self) -> &'static str {
        match self {
            PublishField::AuthorName => "authorName",
            PublishField::GithubHandle => "githubHandle",
            PublishField::Name => "name",
            PublishField::Description => "description",
            PublishField::TwitterHandle => "twitterHandle",
            PublishField::Website => "website",
        }
    }

    pub fn from_key(key: &str) -> Option<PublishField> {
        PublishField::ALL.into_iter().find(|f| f.key() == key)
    }
}

/// The publish dialog's fields (`PublishLibraryDataParams`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PublishLibraryData {
    pub author_name: String,
    pub github_handle: String,
    pub name: String,
    pub description: String,
    pub twitter_handle: String,
    pub website: String,
}

impl PublishLibraryData {
    pub fn get(&self, field: PublishField) -> &str {
        match field {
            PublishField::AuthorName => &self.author_name,
            PublishField::GithubHandle => &self.github_handle,
            PublishField::Name => &self.name,
            PublishField::Description => &self.description,
            PublishField::TwitterHandle => &self.twitter_handle,
            PublishField::Website => &self.website,
        }
    }

    pub fn set(&mut self, field: PublishField, value: String) {
        *match field {
            PublishField::AuthorName => &mut self.author_name,
            PublishField::GithubHandle => &mut self.github_handle,
            PublishField::Name => &mut self.name,
            PublishField::Description => &mut self.description,
            PublishField::TwitterHandle => &mut self.twitter_handle,
            PublishField::Website => &mut self.website,
        } = value;
    }

    /// As `EditorLocalStorage.set` stores it (`JSON.stringify`).
    pub fn to_json(&self) -> Value {
        Value::Object(
            PublishField::ALL
                .into_iter()
                .map(|f| (f.key().to_owned(), json!(self.get(f))))
                .collect::<Map<_, _>>(),
        )
    }

    /// What `EditorLocalStorage.get` read (a missing or non-string field is
    /// empty).
    pub fn from_json(value: &Value) -> Option<PublishLibraryData> {
        let o = value.as_object()?;
        let mut data = PublishLibraryData::default();
        for f in PublishField::ALL {
            if let Some(v) = o.get(f.key()).and_then(Value::as_str) {
                data.set(f, v.to_owned());
            }
        }
        Some(data)
    }
}

/// `PublishLibrary`'s own state while it is open.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PublishState {
    /// `libraryData`.
    pub data: PublishLibraryData,
    /// The item names typed while the dialog's items are the library's own
    /// (upstream writes `items[index].name` on them in place, so the
    /// library keeps them): stored when the dialog closes or succeeds.
    pub library_names: HashMap<String, String>,
    /// The names the dialog shows and submits (`clonedLibItems`): after a
    /// submit that failed validation they are copies, and names typed then
    /// stay in the dialog.
    pub dialog_names: HashMap<String, String>,
    /// `clonedLibItems` are copies (a submit failed validation) until the
    /// items change.
    pub detached: bool,
    /// Each item's `error` (`publishDialog.errors.required`, or empty),
    /// once a submit failed validation.
    pub errors: HashMap<String, String>,
    /// `isSubmitting`.
    pub submitting: bool,
}

/// `publishLibSuccess`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublishSuccess {
    pub url: String,
    pub author_name: String,
}

/// `item` with the name `names` gives it.
pub(super) fn named(item: &LibraryItem, names: &HashMap<String, String>) -> LibraryItem {
    let mut item = item.clone();
    if let Some(name) = names.get(&item.id) {
        item.name = Some(name.clone());
    }
    item
}

/// `getSelectedItems(libraryItems, selectedItems)`: the selected items in
/// library order.
pub(super) fn selected_items<'a>(
    cx: &LibraryContext<'a>,
    state: &LibraryMenuState,
) -> Vec<&'a LibraryItem> {
    let ids = in_library_order(cx.items, &state.selected_items);
    cx.items.iter().filter(|i| ids.contains(&i.id)).collect()
}

/// The open dialogs' props.
#[derive(Clone)]
pub struct LibraryDialogsProps<'a> {
    pub context: LibraryContext<'a>,
    pub state: &'a LibraryMenuState,
    pub theme: Theme,
    /// The editor container's id (the dialog title's id prefix).
    pub container_id: String,
    /// The publish dialog's item previews (`exportToSvg` with a white
    /// background), by item id; an item without one has an empty box.
    pub previews: &'a HashMap<String, Node>,
    pub on_event: Option<OnLibrarySidebarEvent>,
}

/// The open dialogs, each the portal container [`dialog`] builds, in the
/// order upstream mounts them (`LibraryMenuHeaderContent.tsx:244-272`).
pub fn library_dialogs(props: LibraryDialogsProps<'_>) -> Vec<Element> {
    let state = props.state;
    let mut out = Vec::new();
    if state.confirm_open {
        out.push(remove_lib_alert(&props));
    }
    if let Some(publish) = &state.publish {
        out.push(publish_library(&props, publish));
    }
    if let Some(success) = &state.publish_success {
        out.push(publish_success(&props, success));
    }
    out
}

fn dialog_props(
    props: &LibraryDialogsProps<'_>,
    class_name: String,
    size: DialogSize,
    title: &str,
    on_close: Rc<dyn Fn()>,
) -> DialogProps {
    DialogProps {
        class_name: Some(class_name),
        size,
        title: Some(Node::text(title)),
        container_id: props.container_id.clone(),
        theme: props.theme,
        phone: props.context.phone,
        on_close_request: Some(on_close),
        close_label: CLOSE_LABEL.to_owned(),
        ..DialogProps::default()
    }
}

fn close_request(props: &LibraryDialogsProps<'_>, event: LibrarySidebarEvent) -> Rc<dyn Fn()> {
    let on_event = props.on_event.clone();
    Rc::new(move || emit(&on_event, event.clone()))
}

/// `DialogActionButton`'s kind (`actionType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionType {
    Plain,
    Primary,
    Danger,
}

/// `DialogActionButton` (`DialogActionButton.tsx`): the label, hidden
/// behind a spinner while loading.
pub fn dialog_action_button(
    label: &str,
    action: ActionType,
    submit: bool,
    loading: bool,
    test_id: Option<&str>,
) -> Element {
    let cs = match action {
        ActionType::Plain => "",
        ActionType::Primary => "Dialog__action-button--primary",
        ActionType::Danger => "Dialog__action-button--danger",
    };
    let mut text = Element::new("div");
    if loading {
        text = text.style("visibility", "hidden");
    }
    Element::new("button")
        .attr(
            "class",
            class_names([("Dialog__action-button", true), (cs, true)]),
        )
        .attr("type", if submit { "submit" } else { "button" })
        .attr("aria-label", label)
        .attr_opt("data-testid", test_id)
        .child(text.child(label))
        .child_opt(loading.then(|| {
            Element::new("div")
                .style("position", "absolute")
                .style("inset", "0")
                .child(spinner("1em"))
        }))
}

/// `renderRemoveLibAlert` (`LibraryMenuHeaderContent.tsx:62-88`): the
/// ConfirmDialog asking to remove the selected items, or to reset the
/// library.
fn remove_lib_alert(props: &LibraryDialogsProps<'_>) -> Element {
    let count = props.state.selected_items.len();
    let (content, title) = if count > 0 {
        (
            library_text("alerts.removeItemsFromsLibrary").replace("{{count}}", &count.to_string()),
            library_text("confirmDialog.removeItemsFromLib"),
        )
    } else {
        (
            library_text("alerts.resetLibrary").to_owned(),
            library_text("confirmDialog.resetLibrary"),
        )
    };
    let button = |label: &str, action: ActionType, event: LibrarySidebarEvent| {
        let on_event = props.on_event.clone();
        dialog_action_button(label, action, false, false, None)
            .on("click", move |_| emit(&on_event, event.clone()))
    };
    let buttons = Element::new("div")
        .attr("class", "confirm-dialog-buttons")
        .child(button(
            library_text("buttons.cancel"),
            ActionType::Plain,
            LibrarySidebarEvent::ConfirmCancel,
        ))
        .child(button(
            library_text("buttons.confirm"),
            ActionType::Danger,
            LibrarySidebarEvent::ConfirmAccept,
        ));
    dialog(
        dialog_props(
            props,
            // `confirm-dialog ${className}` with no className
            "confirm-dialog ".into(),
            DialogSize::Small,
            title,
            close_request(
                props,
                LibrarySidebarEvent::DialogClose(LibraryDialog::Confirm),
            ),
        ),
        vec![Element::new("p").child(content).into(), buttons.into()],
    )
}

/// Which of the menu's dialogs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryDialog {
    Confirm,
    Publish,
    PublishSuccess,
}

/// `<Trans i18nKey=... link={(el) => <a href target rel>{el}</a>} />` for a
/// string with one `<link>…</link>` and `{{key}}`s from `values`.
fn trans(text: &str, values: &[(&str, &str)], href: &str, rel: &str) -> Vec<Node> {
    let mut text = text.to_owned();
    for (k, v) in values {
        text = text.replace(&format!("{{{{{k}}}}}"), v);
    }
    let Some((before, rest)) = text.split_once("<link>") else {
        return vec![Node::text(text)];
    };
    let (inner, after) = rest.split_once("</link>").unwrap_or((rest, ""));
    let mut out = Vec::new();
    if !before.is_empty() {
        out.push(Node::text(before));
    }
    out.push(
        Element::new("a")
            .attr("href", href)
            .attr("target", "_blank")
            .attr("rel", rel)
            .child(inner)
            .into(),
    );
    if !after.is_empty() {
        out.push(Node::text(after));
    }
    out
}

fn required() -> Element {
    Element::new("span")
        .attr("aria-hidden", "true")
        .attr("class", "required")
        .child("*")
}

/// The value of the input or textarea an event came from.
fn target_value(e: &Event) -> String {
    let Some(target) = e.target() else {
        return String::new();
    };
    if let Some(input) = target.dyn_ref::<HtmlInputElement>() {
        return input.value();
    }
    target
        .dyn_ref::<HtmlTextAreaElement>()
        .map(HtmlTextAreaElement::value)
        .unwrap_or_default()
}

/// `SingleLibraryItem` (`PublishLibrary.tsx:106-199`).
fn single_library_item(
    props: &LibraryDialogsProps<'_>,
    item: &LibraryItem,
    error: &str,
) -> Element {
    let on_remove = props.on_event.clone();
    let id = item.id.clone();
    let remove = icon_button(IconButtonProps {
        aria_label: library_text("buttons.remove").to_owned(),
        icon: Some(icon_node(&icons::CloseIcon, props.theme)),
        class_name: "single-library-item--remove".into(),
        title: Some(library_text("buttons.remove").to_owned()),
        on_click: Some(Rc::new(move |_| {
            emit(
                &on_remove,
                LibrarySidebarEvent::PublishRemoveItem(id.clone()),
            )
        })),
        ..IconButtonProps::default()
    });
    let on_input = props.on_event.clone();
    let id = item.id.clone();
    let input = Element::new("input")
        .attr("type", "text")
        .style("width", "80%")
        .style("padding", "0.2rem")
        .attr_opt("value", item.name.clone())
        .attr("placeholder", "Item name")
        .on("input", move |e| {
            emit(
                &on_input,
                LibrarySidebarEvent::PublishItemName {
                    id: id.clone(),
                    value: target_value(e),
                },
            )
        });
    let label = Element::new("label")
        .style("display", "flex")
        .style("justify-content", "space-between")
        .style("flex-direction", "column")
        .child(
            Element::new("div")
                .style("padding", "0.5em 0")
                .child(
                    Element::new("span")
                        .style("font-weight", "500")
                        .style("color", "#868e96")
                        .child(library_text("publishDialog.itemName")),
                )
                .child(required()),
        )
        .child(input);
    let mut svg = Element::new("div").attr("class", "single-library-item__svg");
    if let Some(preview) = props.previews.get(&item.id) {
        svg = svg.child(preview.clone());
    }
    Element::new("div")
        .attr("class", "single-library-item")
        .child_opt((item.status == LibraryItemStatus::Published).then(|| {
            Element::new("span")
                .attr("class", "single-library-item-status")
                .child(library_text("labels.statusPublished"))
        }))
        .child(svg)
        .child(remove)
        .child(
            Element::new("div")
                .style("display", "flex")
                .style("margin", "0.8rem 0")
                .style("width", "100%")
                .style("font-size", "14px")
                .style("font-weight", "500")
                .style("flex-direction", "column")
                .child(label)
                .child(Element::new("span").attr("class", "error").child(error)),
        )
}

/// The dialog's items: the selected ones with the names typed so far.
pub(super) fn publish_items(
    cx: &LibraryContext<'_>,
    state: &LibraryMenuState,
    publish: &PublishState,
) -> Vec<LibraryItem> {
    selected_items(cx, state)
        .into_iter()
        .map(|i| named(i, &publish.dialog_names))
        .collect()
}

/// `PublishLibrary` (`PublishLibrary.tsx:201-541`).
fn publish_library(props: &LibraryDialogsProps<'_>, publish: &PublishState) -> Element {
    let items = publish_items(&props.context, props.state, publish);
    let on_close = close_request(
        props,
        LibrarySidebarEvent::DialogClose(LibraryDialog::Publish),
    );
    let body: Node = if items.is_empty() {
        Element::new("p")
            .style("padding", "1em")
            .style("text-align", "center")
            .style("font-weight", "500")
            .child(library_text("publishDialog.atleastOneLibItem"))
            .into()
    } else {
        publish_form(props, publish, &items).into()
    };
    dialog(
        dialog_props(
            props,
            "publish-library".into(),
            DialogSize::Default,
            library_text("publishDialog.title"),
            on_close,
        ),
        vec![body],
    )
}

fn publish_form(
    props: &LibraryDialogsProps<'_>,
    publish: &PublishState,
    items: &[LibraryItem],
) -> Element {
    let note = |tag: &str, class: &str, children: Vec<Node>| {
        Element::new(tag)
            .attr("class", class)
            .children_from(children)
    };
    let contains_published = items
        .iter()
        .any(|i| i.status == LibraryItemStatus::Published);
    let selected = Element::new("div")
        .attr("class", "selected-library-items")
        .children_from(items.iter().map(|item| {
            let error = publish.errors.get(&item.id).map_or("", String::as_str);
            Element::new("div")
                .attr("class", "single-library-item-wrapper")
                .child(single_library_item(props, item, error))
                .into()
        }));
    let field = |field: PublishField, input: Element| {
        let on_event = props.on_event.clone();
        input.attr("name", field.key()).on("input", move |e| {
            emit(
                &on_event,
                LibrarySidebarEvent::PublishInput {
                    field,
                    value: target_value(e),
                },
            )
        })
    };
    let text_input = |f: PublishField, placeholder: &str| {
        field(
            f,
            Element::new("input")
                .attr("type", "text")
                .attr("value", publish.data.get(f))
                .attr("placeholder", library_text(placeholder)),
        )
    };
    let required_label = |label: &str| {
        Element::new("div")
            .child(Element::new("span").child(library_text(label)))
            .child(required())
    };
    let description = publish.data.get(PublishField::Description);
    let fields = Element::new("div")
        .attr("class", "publish-library__fields")
        .child(
            Element::new("label")
                .child(required_label("publishDialog.libraryName"))
                .child(
                    text_input(PublishField::Name, "publishDialog.placeholder.libraryName")
                        .attr("required", ""),
                ),
        )
        .child(
            Element::new("label")
                .style("align-items", "flex-start")
                .child(required_label("publishDialog.libraryDesc"))
                .child(field(
                    PublishField::Description,
                    Element::new("textarea")
                        .attr("rows", "4")
                        .attr("required", "")
                        .attr(
                            "placeholder",
                            library_text("publishDialog.placeholder.libraryDesc"),
                        )
                        .child_opt((!description.is_empty()).then(|| description.to_owned())),
                )),
        )
        .child(
            Element::new("label")
                .child(required_label("publishDialog.authorName"))
                .child(
                    text_input(
                        PublishField::AuthorName,
                        "publishDialog.placeholder.authorName",
                    )
                    .attr("required", ""),
                ),
        )
        .child(
            Element::new("label")
                .child(Element::new("span").child(library_text("publishDialog.githubUsername")))
                .child(text_input(
                    PublishField::GithubHandle,
                    "publishDialog.placeholder.githubHandle",
                )),
        )
        .child(
            Element::new("label")
                .child(Element::new("span").child(library_text("publishDialog.twitterUsername")))
                .child(text_input(
                    PublishField::TwitterHandle,
                    "publishDialog.placeholder.twitterHandle",
                )),
        )
        .child(
            Element::new("label")
                .child(Element::new("span").child(library_text("publishDialog.website")))
                .child(
                    text_input(PublishField::Website, "publishDialog.placeholder.website")
                        .attr("pattern", "https?://.+")
                        .attr("title", library_text("publishDialog.errors.website")),
                ),
        )
        .child(note(
            "span",
            "publish-library-note",
            trans(
                library_text("publishDialog.noteLicense"),
                &[],
                "https://github.com/excalidraw/excalidraw-libraries/blob/main/LICENSE",
                "noopener noreferrer",
            ),
        ));
    let on_save = props.on_event.clone();
    let buttons = Element::new("div")
        .attr("class", "publish-library__buttons")
        .child(
            dialog_action_button(
                library_text("buttons.saveLibNames"),
                ActionType::Plain,
                false,
                false,
                Some("cancel-clear-canvas-button"),
            )
            .on("click", move |_| {
                emit(&on_save, LibrarySidebarEvent::PublishSaveNames)
            }),
        )
        .child(dialog_action_button(
            library_text("buttons.submit"),
            ActionType::Primary,
            true,
            publish.submitting,
            None,
        ));
    let on_submit = props.on_event.clone();
    Element::new("form")
        .on("submit", move |e| {
            e.prevent_default();
            emit(&on_submit, LibrarySidebarEvent::PublishSubmit)
        })
        .child(note(
            "div",
            "publish-library-note",
            trans(
                library_text("publishDialog.noteDescription"),
                &[],
                "https://libraries.excalidraw.com",
                "noopener",
            ),
        ))
        .child(note(
            "span",
            "publish-library-note",
            trans(
                library_text("publishDialog.noteGuidelines"),
                &[],
                "https://github.com/excalidraw/excalidraw-libraries#guidelines",
                "noopener noreferrer",
            ),
        ))
        .child(note(
            "div",
            "publish-library-note",
            vec![Node::text(library_text("publishDialog.noteItems"))],
        ))
        .child_opt(contains_published.then(|| {
            note(
                "span",
                "publish-library-note publish-library-warning",
                vec![Node::text(library_text("publishDialog.republishWarning"))],
            )
        }))
        .child(selected)
        .child(fields)
        .child(buttons)
}

/// `renderPublishSuccess` (`LibraryMenuHeaderContent.tsx:106-140`).
fn publish_success(props: &LibraryDialogsProps<'_>, success: &PublishSuccess) -> Element {
    let on_close = props.on_event.clone();
    let close = icon_button(IconButtonProps {
        title: Some(library_text("buttons.close").to_owned()),
        aria_label: library_text("buttons.close").to_owned(),
        label: Some(library_text("buttons.close").to_owned()),
        test_id: Some("publish-library-success-close".into()),
        class_name: "publish-library-success-close".into(),
        on_click: Some(Rc::new(move |_| {
            emit(&on_close, LibrarySidebarEvent::PublishSuccessClose)
        })),
        ..IconButtonProps::default()
    });
    dialog(
        dialog_props(
            props,
            "publish-library-success".into(),
            DialogSize::Small,
            library_text("publishSuccessDialog.title"),
            close_request(
                props,
                LibrarySidebarEvent::DialogClose(LibraryDialog::PublishSuccess),
            ),
        ),
        vec![
            Element::new("p")
                .children_from(trans(
                    library_text("publishSuccessDialog.content"),
                    &[("authorName", &success.author_name)],
                    &success.url,
                    "noopener noreferrer",
                ))
                .into(),
            close.into(),
        ],
    )
}

/// The text fields of the submission's form data after the library file
/// and the preview image (`PublishLibrary.tsx:293-303`), in order.
pub fn publish_form_fields(data: &PublishLibraryData) -> Vec<(&'static str, String)> {
    vec![
        ("title", data.name.clone()),
        ("authorName", data.author_name.clone()),
        ("githubHandle", data.github_handle.clone()),
        ("name", data.name.clone()),
        ("description", data.description.clone()),
        ("twitterHandle", data.twitter_handle.clone()),
        ("website", data.website.clone()),
    ]
}
