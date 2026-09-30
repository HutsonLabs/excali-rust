//! The styles panels' action controls: what `renderAction(name, data)`
//! renders for each action the full, compact and mobile panels
//! ([`crate::styles_panel`]) hold, each action's `PanelComponent`
//! (`packages/excalidraw/actions/actionProperties.tsx`,
//! `actionLinearEditor.tsx`, `actionAlign.tsx`, `actionDistribute.tsx`,
//! `actionZindex.tsx`, `actionGroup.tsx`, `actionDeleteSelected.tsx`,
//! `actionDuplicateSelection.tsx`, `actionLink.tsx`, `actionCropEditor.tsx`,
//! `actionHistory.tsx`) with the components they are built of
//! (`RadioSelection.tsx`, `RadioButton.tsx`, `IconButton.tsx`,
//! `Range.tsx`, `IconPicker.tsx` with `Stats/Collapsible.tsx` and
//! `InlineIcon.tsx`).
//!
//! [`render_action_panel`] builds the DOM for the context's elements and
//! app state; what the user picks comes back through
//! [`ActionPanelOptions::on_update`] as the value upstream's `updateData`
//! receives, for the host to run the action's `perform` with. The values
//! the controls show are excali-editor's `getFormValue` ports
//! ([`excali_editor::actions::form_fill_style`] and the rest). IconPicker
//! keeps its open state and the shared "more options" atom in React; the
//! host keeps them here ([`IconPickerState`]), told of changes through
//! [`ActionPanelOptions::on_icon_picker`].
//!
//! The stateful pickers are the host's: the colour pickers
//! ([`crate::styles_panel::color_action_panel`],
//! [`bucket_fill_color_panel`]) and the font picker
//! ([`font_family_panel`], rendered with [`crate::font_picker`]).
//!
//! Text is English (`locales/en.json`).

use std::rc::Rc;

use excali_core::color::{BUCKET_FILL_BACKGROUND_PICKS, DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE};
use excali_core::constants::FONT_SIZES;
use excali_core::element::{Arrowhead, ElementType, FillStyle, StrokeVariability};
use excali_editor::actions::{
    align_enabled, arrow_type_str, bucket_fill_color, distribute_enabled, form_arrow_type,
    form_arrowhead, form_fill_style, form_font_family, form_font_size, form_opacity,
    form_roughness, form_roundness, form_stroke_style, form_stroke_variability,
    form_stroke_width_key, form_text_align, form_vertical_align, get_shortcut_key, group_enabled,
    has_selected_groups, is_some_element_selected, linear_editor_target, link_panel_state,
    polygon_toggle, selected_fill_styles, stroke_width_key_str, ActionContext, ActionName,
    ArrowheadPosition, KeyLabels,
};
use excali_scene::shape::Theme;
use serde_json::{json, Value};

use crate::color_picker::{ColorPickerType, ColorTopPicksSlot, StylesPanelMode};
use crate::dom::{class_names, Element, EventData, EventResponse, Node};
use crate::font_picker::FontPickerState;
use crate::icons::{self, Icon};
use crate::primitives::{icon_button, range, IconButtonKind, IconButtonProps, RangeProps};
use crate::styles_panel::ColorActionPanel;

/// Called with an action and the value its `updateData` passes (upstream's
/// JSON value; `null` for the buttons).
pub type OnActionUpdate = Rc<dyn Fn(ActionName, Value)>;

/// Called with IconPicker's state after a change.
pub type OnIconPicker = Rc<dyn Fn(IconPickerState)>;

/// IconPicker's state (`IconPicker.tsx`): which picker is open (its label,
/// `arrowhead_start` or `arrowhead_end`; React state upstream) and the
/// "more options" atom all pickers share.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IconPickerState {
    pub open: Option<String>,
    pub more_options: bool,
}

/// What `renderAction` passes a panel component besides the context.
#[derive(Clone)]
pub struct ActionPanelOptions {
    /// The styles panel's mode (`useStylesPanelMode()`,
    /// `getStylesPanelInfo`).
    pub mode: StylesPanelMode,
    /// `appState.theme`, for the themed icons.
    pub theme: Theme,
    /// The language's direction (`getLanguage().rtl`): the arrowhead
    /// previews flip and the IconPicker's arrow keys swap.
    pub rtl: bool,
    /// `isDarwin`, for the shortcuts in the titles.
    pub is_darwin: bool,
    /// `data.cycle` (`renderAction("changeFreedrawMode", { cycle: true })`).
    pub cycle: bool,
    /// `appState.openPopup`.
    pub open_popup: Option<String>,
    /// IconPicker's state.
    pub icon_picker: IconPickerState,
    /// The history's stacks (`useEmitter(history.onHistoryChangedEmitter)`)
    /// for undo and redo.
    pub undo_stack_empty: bool,
    pub redo_stack_empty: bool,
    /// `updateData(value)`: the host runs the action's `perform` with
    /// `value`.
    pub on_update: OnActionUpdate,
    /// IconPicker's `setActive` and the "more options" atom's setter.
    pub on_icon_picker: Option<OnIconPicker>,
}

impl ActionPanelOptions {
    /// The options for `mode`: light, left to right, not a Mac, no data,
    /// no popup, the pickers closed and the history empty.
    pub fn new(mode: StylesPanelMode, on_update: OnActionUpdate) -> ActionPanelOptions {
        ActionPanelOptions {
            mode,
            theme: Theme::Light,
            rtl: false,
            is_darwin: false,
            cycle: false,
            open_popup: None,
            icon_picker: IconPickerState::default(),
            undo_stack_empty: true,
            redo_stack_empty: true,
            on_update,
            on_icon_picker: None,
        }
    }

    fn compact(&self) -> bool {
        self.mode != StylesPanelMode::Full
    }
}

/// `t(key)` in English: the strings of `locales/en.json` the panels
/// show (`crate::i18n::EN_JSON`, whose loader the panels do not pull into
/// the module); the key itself for any other, as `t` does for a missing
/// one. `tests/action_panels.rs` holds the table to `en.json`.
pub fn t(key: &str) -> String {
    match key {
        "buttons.redo" => "Redo",
        "buttons.undo" => "Undo",
        "helpDialog.cropStart" => "Crop image",
        "labels.alignBottom" => "Align bottom",
        "labels.alignLeft" => "Align left",
        "labels.alignRight" => "Align right",
        "labels.alignTop" => "Align top",
        "labels.architect" => "Architect",
        "labels.arrowhead_arrow" => "Arrow",
        "labels.arrowhead_bar" => "Bar",
        "labels.arrowhead_cardinality_exactly_one" => "Cardinality (exactly one)",
        "labels.arrowhead_cardinality_many" => "Cardinality (many)",
        "labels.arrowhead_cardinality_one" => "Cardinality (one)",
        "labels.arrowhead_cardinality_one_or_many" => "Cardinality (one or many)",
        "labels.arrowhead_cardinality_zero_or_many" => "Cardinality (zero or many)",
        "labels.arrowhead_cardinality_zero_or_one" => "Cardinality (zero or one)",
        "labels.arrowhead_circle" => "Circle",
        "labels.arrowhead_circle_outline" => "Circle (outline)",
        "labels.arrowhead_diamond" => "Diamond",
        "labels.arrowhead_diamond_outline" => "Diamond (outline)",
        "labels.arrowhead_none" => "None",
        "labels.arrowhead_triangle" => "Triangle",
        "labels.arrowhead_triangle_outline" => "Triangle (outline)",
        "labels.arrowheads" => "Arrowheads",
        "labels.arrowtype_elbowed" => "Elbow arrow",
        "labels.arrowtype_round" => "Curved arrow",
        "labels.arrowtype_sharp" => "Sharp arrow",
        "labels.arrowtypes" => "Arrow type",
        "labels.artist" => "Artist",
        "labels.background" => "Background",
        "labels.bold" => "Bold",
        "labels.bringForward" => "Bring forward",
        "labels.bringToFront" => "Bring to front",
        "labels.cardinality" => "Cardinality",
        "labels.cartoonist" => "Cartoonist",
        "labels.center" => "Center",
        "labels.centerHorizontally" => "Center horizontally",
        "labels.centerVertically" => "Center vertically",
        "labels.crossHatch" => "Cross-hatch",
        "labels.delete" => "Delete",
        "labels.distributeHorizontally" => "Distribute horizontally",
        "labels.distributeVertically" => "Distribute vertically",
        "labels.duplicateSelection" => "Duplicate",
        "labels.edges" => "Edges",
        "labels.fill" => "Fill",
        "labels.fontFamily" => "Font family",
        "labels.fontSize" => "Font size",
        "labels.group" => "Group selection",
        "labels.hachure" => "Hachure",
        "labels.large" => "Large",
        "labels.left" => "Left",
        "labels.lineEditor.edit" => "Edit line",
        "labels.lineEditor.editArrow" => "Edit arrow",
        "labels.link.create" => "Add link",
        "labels.link.edit" => "Edit link",
        "labels.link.editEmbed" => "Edit embeddable link",
        "labels.link.label" => "Link",
        "labels.link.labelEmbed" => "Link & embed",
        "labels.medium" => "Medium",
        "labels.more_options" => "More options",
        "labels.opacity" => "Opacity",
        "labels.polygon.breakPolygon" => "Break polygon",
        "labels.polygon.convertToPolygon" => "Convert to polygon",
        "labels.pressure" => "Pressure",
        "labels.pressure_constant" => "Constant",
        "labels.pressure_variable" => "Variable",
        "labels.right" => "Right",
        "labels.round" => "Round",
        "labels.sendBackward" => "Send backward",
        "labels.sendToBack" => "Send to back",
        "labels.sharp" => "Sharp",
        "labels.sloppiness" => "Sloppiness",
        "labels.small" => "Small",
        "labels.solid" => "Solid",
        "labels.strokeStyle" => "Stroke style",
        "labels.strokeStyle_dashed" => "Dashed",
        "labels.strokeStyle_dotted" => "Dotted",
        "labels.strokeStyle_solid" => "Solid",
        "labels.strokeWidth" => "Stroke width",
        "labels.textAlign" => "Text align",
        "labels.thin" => "Thin",
        "labels.ungroup" => "Ungroup selection",
        "labels.veryLarge" => "Very large",
        "labels.zigzag" => "Zigzag",
        _ => key,
    }
    .to_owned()
}

/// `getShortcutKey(shortcut)`.
fn shortcut(s: &str, opts: &ActionPanelOptions) -> String {
    get_shortcut_key(s, opts.is_darwin, &KeyLabels::EN)
}

/// A JavaScript number as JSON: integral values as integers.
fn js_number(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 9_007_199_254_740_992.0 {
        json!(n as i64)
    } else {
        json!(n)
    }
}

fn icon_node(icon: &Icon, theme: Theme) -> Node {
    icon.element(theme)
        .unwrap_or_else(|| panic!("{} is a path list", icon.name))
        .into()
}

fn static_icon(icon: &Icon) -> Node {
    icon_node(icon, Theme::Light)
}

/// `updateData(value)` for `name`.
fn updater(opts: &ActionPanelOptions, name: ActionName) -> impl Fn(Value) + Clone + 'static {
    let on_update = opts.on_update.clone();
    move |value| on_update(name, value)
}

/// `withCaretPositionPreservation` (`hooks/useTextEditorFocus.ts:47-70`):
/// in the compact panels, while text is edited, the text editor keeps its
/// caret across the update (restored after a tick).
fn with_caret_position_preservation(compact: bool, editing: bool, f: impl FnOnce()) {
    if !(compact && editing) {
        f();
        return;
    }
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::closure::Closure;
        use wasm_bindgen::JsCast;
        let editor = || -> Option<web_sys::HtmlTextAreaElement> {
            web_sys::window()?
                .document()?
                .query_selector(".excalidraw-wysiwyg")
                .ok()??
                .dyn_into()
                .ok()
        };
        let saved = editor().map(|e| (e.selection_start(), e.selection_end()));
        f();
        let restore = Closure::once_into_js(move || {
            if let Some(e) = editor() {
                let _ = e.focus();
                if let Some((Ok(start), Ok(end))) = saved {
                    let _ = e.set_selection_start(start);
                    let _ = e.set_selection_end(end);
                }
            }
        });
        if let Some(window) = web_sys::window() {
            let _ = window
                .set_timeout_with_callback_and_timeout_and_arguments_0(restore.unchecked_ref(), 0);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    f();
}

fn editing_text(ctx: &ActionContext<'_>) -> bool {
    ctx.app_state
        .get("editingTextElement")
        .is_some_and(|v| v.is_object())
}

// -- RadioSelection ---------------------------------------------------------------------

/// One `RadioSelection` option.
struct Choice {
    value: Value,
    text: String,
    icon: Node,
    test_id: Option<&'static str>,
    /// `active`; the value's identity with the selection's otherwise.
    active: Option<bool>,
}

fn choice(value: Value, text_key: &str, icon: Node, test_id: Option<&'static str>) -> Choice {
    Choice {
        value,
        text: t(text_key),
        icon,
        test_id,
        active: None,
    }
}

/// `RadioSelection` of radio inputs (`RadioSelection.tsx:43-60`): a label
/// per option, `active` when the value is the selection's.
fn radio_selection(
    group: &str,
    choices: Vec<Choice>,
    value: &Option<Value>,
    on_change: impl Fn(Value) + Clone + 'static,
) -> Vec<Node> {
    choices
        .into_iter()
        .map(|c| {
            let checked = value.as_ref() == Some(&c.value);
            let on_change = on_change.clone();
            let picked = c.value.clone();
            Element::new("label")
                .attr_opt("class", checked.then_some("active"))
                .attr("title", c.text)
                .child(
                    Element::new("input")
                        .attr("type", "radio")
                        .attr("name", group)
                        .flag("checked", checked)
                        .attr_opt("data-testid", c.test_id)
                        .on_data("change", move |_| {
                            on_change(picked.clone());
                            EventResponse::default()
                        }),
                )
                .child(c.icon)
                .into()
        })
        .collect()
}

/// `RadioSelection type="button"`: a `RadioButton` per option
/// (`RadioButton.tsx`), clicked with the event's Alt key.
fn radio_buttons(
    choices: Vec<Choice>,
    value: &Option<Value>,
    on_click: impl Fn(Value, bool) + Clone + 'static,
) -> Vec<Node> {
    choices
        .into_iter()
        .map(|c| {
            let active = c.active.unwrap_or(value.as_ref() == Some(&c.value));
            let on_click = on_click.clone();
            let picked = c.value.clone();
            let class = class_names([("active", active)]);
            Element::new("button")
                .attr("type", "button")
                .attr("title", c.text)
                .attr_opt("data-testid", c.test_id)
                .attr_opt("class", (!class.is_empty()).then_some(class))
                .child(c.icon)
                .on_data("click", move |data| {
                    on_click(picked.clone(), data.alt_key);
                    EventResponse::default()
                })
                .into()
        })
        .collect()
}

/// `<fieldset><legend>{legend}</legend><div class="buttonList">…</div></fieldset>`.
fn button_list_fieldset(legend: Option<&str>, class: &str, children: Vec<Node>) -> Vec<Node> {
    vec![Element::new("fieldset")
        .child_opt(legend.map(|key| Element::new("legend").child(t(key))))
        .child(
            Element::new("div")
                .attr("class", class)
                .children_from(children),
        )
        .into()]
}

// -- IconButton --------------------------------------------------------------------------

/// An `IconButton type="button"` whose click calls `updateData(null)`
/// (or, for `on_click`, what it does).
fn icon_button_node(props: IconButtonProps, on_click: impl Fn() + 'static) -> Node {
    icon_button(props)
        .on_data("click", move |_| {
            on_click();
            EventResponse::default()
        })
        .into()
}

fn button_props(icon: Node, title: Option<String>, aria_label: String) -> IconButtonProps {
    IconButtonProps {
        kind: IconButtonKind::Button,
        icon: Some(icon),
        title,
        aria_label,
        ..IconButtonProps::default()
    }
}

/// `MOBILE_ACTION_BUTTON_BG` (`common/src/constants.ts:618-620`) on the
/// mobile row's buttons, unless the "…" popover holds them.
fn mobile_background(opts: &ActionPanelOptions, always: bool) -> Vec<(String, String)> {
    let on = opts.mode == StylesPanelMode::Mobile
        && (always || opts.open_popup.as_deref() != Some("compactOtherProperties"));
    if on {
        vec![("background".into(), "var(--mobile-action-button-bg)".into())]
    } else {
        Vec::new()
    }
}

// -- the property actions ---------------------------------------------------------------

/// `actionChangeFillStyle`'s panel (`actionProperties.tsx:626-697`).
fn fill_style(ctx: &ActionContext<'_>, opts: &ActionPanelOptions) -> Vec<Node> {
    let styles = selected_fill_styles(ctx);
    let all_zigzag = !styles.is_empty() && styles.iter().all(|s| *s == FillStyle::Zigzag);
    let all_hachure = styles.iter().all(|s| *s == FillStyle::Hachure);
    let hachure = Choice {
        value: json!("hachure"),
        text: format!(
            "{} ({})",
            t(if all_zigzag {
                "labels.zigzag"
            } else {
                "labels.hachure"
            }),
            shortcut("Alt-Click", opts)
        ),
        icon: static_icon(if all_zigzag {
            &icons::FillZigZagIcon
        } else {
            &icons::FillHachureIcon
        }),
        test_id: Some("fill-hachure"),
        active: all_zigzag.then_some(true),
    };
    let choices = vec![
        hachure,
        choice(
            json!("cross-hatch"),
            "labels.crossHatch",
            static_icon(&icons::FillCrossHatchIcon),
            Some("fill-cross-hatch"),
        ),
        choice(
            json!("solid"),
            "labels.solid",
            static_icon(&icons::FillSolidIcon),
            Some("fill-solid"),
        ),
    ];
    let update = updater(opts, ActionName::ChangeFillStyle);
    let buttons = radio_buttons(
        choices,
        &form_fill_style(ctx).map(|v| serde_json::to_value(v).unwrap_or(Value::Null)),
        move |value, alt| {
            // Alt-click on hachure over an all-hachure selection: zigzag
            let next = if alt && value == json!("hachure") && all_hachure {
                json!("zigzag")
            } else {
                value
            };
            update(next);
        },
    );
    button_list_fieldset(Some("labels.fill"), "buttonList", buttons)
}

/// `actionChangeStrokeWidth`'s panel (:725-770).
fn stroke_width(ctx: &ActionContext<'_>, opts: &ActionPanelOptions) -> Vec<Node> {
    let choices = vec![
        choice(
            json!("thin"),
            "labels.thin",
            static_icon(&icons::StrokeWidthBaseIcon),
            Some("strokeWidth-thin"),
        ),
        choice(
            json!("medium"),
            "labels.medium",
            static_icon(&icons::StrokeWidthBoldIcon),
            Some("strokeWidth-medium"),
        ),
        choice(
            json!("bold"),
            "labels.bold",
            static_icon(&icons::StrokeWidthExtraBoldIcon),
            Some("strokeWidth-bold"),
        ),
    ];
    let value = form_stroke_width_key(ctx).map(|k| json!(stroke_width_key_str(k)));
    let radios = radio_selection(
        "stroke-width",
        choices,
        &value,
        updater(opts, ActionName::ChangeStrokeWidth),
    );
    button_list_fieldset(Some("labels.strokeWidth"), "buttonList", radios)
}

/// `actionChangeSloppiness`'s panel (:782-822).
fn sloppiness(ctx: &ActionContext<'_>, opts: &ActionPanelOptions) -> Vec<Node> {
    let choices = vec![
        choice(
            json!(0),
            "labels.architect",
            static_icon(&icons::SloppinessArchitectIcon),
            None,
        ),
        choice(
            json!(1),
            "labels.artist",
            static_icon(&icons::SloppinessArtistIcon),
            None,
        ),
        choice(
            json!(2),
            "labels.cartoonist",
            static_icon(&icons::SloppinessCartoonistIcon),
            None,
        ),
    ];
    let value = form_roughness(ctx).map(js_number);
    let radios = radio_selection(
        "sloppiness",
        choices,
        &value,
        updater(opts, ActionName::ChangeSloppiness),
    );
    button_list_fieldset(Some("labels.sloppiness"), "buttonList", radios)
}

/// `actionChangeFreedrawMode`'s panel (:843-905): with `data.cycle` one
/// button cycling the mode, else the two radios.
fn freedraw_mode(ctx: &ActionContext<'_>, opts: &ActionPanelOptions) -> Vec<Node> {
    let variability = form_stroke_variability(ctx);
    let update = updater(opts, ActionName::ChangeFreedrawMode);
    if opts.cycle {
        let is_variable = variability == Some(StrokeVariability::Variable);
        let label = t("labels.pressure");
        let icon = static_icon(if is_variable {
            &icons::strokeVariabilityVariableIcon
        } else {
            &icons::strokeVariabilityConstantIcon
        });
        return vec![icon_button_node(
            button_props(icon, Some(label.clone()), label),
            move || update(json!(if is_variable { "constant" } else { "variable" })),
        )];
    }
    let choices = vec![
        choice(
            json!("constant"),
            "labels.pressure_constant",
            static_icon(&icons::strokeVariabilityConstantIcon),
            None,
        ),
        choice(
            json!("variable"),
            "labels.pressure_variable",
            static_icon(&icons::strokeVariabilityVariableIcon),
            None,
        ),
    ];
    let radios = radio_selection(
        "strokeOptions.variability",
        choices,
        &variability.map(|v| serde_json::to_value(v).unwrap_or(Value::Null)),
        update,
    );
    button_list_fieldset(Some("labels.pressure"), "buttonList", radios)
}

/// `actionChangeStrokeStyle`'s panel (:918-958).
fn stroke_style(ctx: &ActionContext<'_>, opts: &ActionPanelOptions) -> Vec<Node> {
    let choices = vec![
        choice(
            json!("solid"),
            "labels.strokeStyle_solid",
            static_icon(&icons::StrokeWidthBaseIcon),
            None,
        ),
        choice(
            json!("dashed"),
            "labels.strokeStyle_dashed",
            static_icon(&icons::StrokeStyleDashedIcon),
            None,
        ),
        choice(
            json!("dotted"),
            "labels.strokeStyle_dotted",
            static_icon(&icons::StrokeStyleDottedIcon),
            None,
        ),
    ];
    let radios = radio_selection(
        "strokeStyle",
        choices,
        &form_stroke_style(ctx).map(|v| serde_json::to_value(v).unwrap_or(Value::Null)),
        updater(opts, ActionName::ChangeStrokeStyle),
    );
    button_list_fieldset(Some("labels.strokeStyle"), "buttonList", radios)
}

/// `actionChangeOpacity`'s panel (:975-998): a Range over 0 to 100.
fn opacity(ctx: &ActionContext<'_>, opts: &ActionPanelOptions) -> Vec<Node> {
    let opacity = form_opacity(ctx);
    let current = ctx
        .app_state
        .get("currentItemOpacity")
        .and_then(Value::as_f64)
        .unwrap_or(100.0);
    let update = updater(opts, ActionName::ChangeOpacity);
    vec![range(RangeProps {
        label: Node::text(t("labels.opacity")),
        value: opacity.unwrap_or(current),
        has_common_value: opacity.is_some(),
        on_change: Rc::new(move |v| update(js_number(v))),
        min: 0.0,
        max: 100.0,
        step: 10.0,
        min_label: None,
        test_id: Some("opacity".into()),
    })
    .into()]
}

/// A text property's radios, updating through
/// `withCaretPositionPreservation`.
fn text_radios(
    ctx: &ActionContext<'_>,
    opts: &ActionPanelOptions,
    name: ActionName,
    group: &str,
    choices: Vec<Choice>,
    value: Option<Value>,
) -> Vec<Node> {
    let update = updater(opts, name);
    let (compact, editing) = (opts.compact(), editing_text(ctx));
    radio_selection(group, choices, &value, move |v| {
        let update = update.clone();
        with_caret_position_preservation(compact, editing, move || update(v));
    })
}

/// `actionChangeFontSize`'s panel (:1016-1090).
fn font_size(ctx: &ActionContext<'_>, opts: &ActionPanelOptions) -> Vec<Node> {
    let choices = vec![
        choice(
            js_number(FONT_SIZES.sm),
            "labels.small",
            static_icon(&icons::FontSizeSmallIcon),
            Some("fontSize-small"),
        ),
        choice(
            js_number(FONT_SIZES.md),
            "labels.medium",
            static_icon(&icons::FontSizeMediumIcon),
            Some("fontSize-medium"),
        ),
        choice(
            js_number(FONT_SIZES.lg),
            "labels.large",
            static_icon(&icons::FontSizeLargeIcon),
            Some("fontSize-large"),
        ),
        choice(
            js_number(FONT_SIZES.xl),
            "labels.veryLarge",
            static_icon(&icons::FontSizeExtraLargeIcon),
            Some("fontSize-veryLarge"),
        ),
    ];
    let value = form_font_size(ctx).map(js_number);
    let radios = text_radios(
        ctx,
        opts,
        ActionName::ChangeFontSize,
        "font-size",
        choices,
        value,
    );
    button_list_fieldset(Some("labels.fontSize"), "buttonList", radios)
}

/// `actionChangeTextAlign`'s panel (:1581-1643).
fn text_align(ctx: &ActionContext<'_>, opts: &ActionPanelOptions) -> Vec<Node> {
    let choices = vec![
        choice(
            json!("left"),
            "labels.left",
            static_icon(&icons::TextAlignLeftIcon),
            Some("align-left"),
        ),
        choice(
            json!("center"),
            "labels.center",
            static_icon(&icons::TextAlignCenterIcon),
            Some("align-horizontal-center"),
        ),
        choice(
            json!("right"),
            "labels.right",
            static_icon(&icons::TextAlignRightIcon),
            Some("align-right"),
        ),
    ];
    let value = form_text_align(ctx).map(|v| serde_json::to_value(v).unwrap_or(Value::Null));
    let radios = text_radios(
        ctx,
        opts,
        ActionName::ChangeTextAlign,
        "text-align",
        choices,
        value,
    );
    button_list_fieldset(Some("labels.textAlign"), "buttonList", radios)
}

/// `actionChangeVerticalAlign`'s panel (:1682-1745), its icons themed.
fn vertical_align(ctx: &ActionContext<'_>, opts: &ActionPanelOptions) -> Vec<Node> {
    let theme = opts.theme;
    let choices = vec![
        choice(
            json!("top"),
            "labels.alignTop",
            icon_node(&icons::TextAlignTopIcon, theme),
            Some("align-top"),
        ),
        choice(
            json!("middle"),
            "labels.centerVertically",
            icon_node(&icons::TextAlignMiddleIcon, theme),
            Some("align-middle"),
        ),
        choice(
            json!("bottom"),
            "labels.alignBottom",
            icon_node(&icons::TextAlignBottomIcon, theme),
            Some("align-bottom"),
        ),
    ];
    let value = form_vertical_align(ctx).map(|v| serde_json::to_value(v).unwrap_or(Value::Null));
    let radios = text_radios(
        ctx,
        opts,
        ActionName::ChangeVerticalAlign,
        "text-align",
        choices,
        value,
    );
    button_list_fieldset(None, "buttonList", radios)
}

/// `actionChangeRoundness`'s panel (:1779-1828), with the polygon toggle
/// after the radios (`renderAction("togglePolygon")`).
fn roundness(ctx: &ActionContext<'_>, opts: &ActionPanelOptions) -> Vec<Node> {
    let choices = vec![
        choice(
            json!("sharp"),
            "labels.sharp",
            static_icon(&icons::EdgeSharpIcon),
            None,
        ),
        choice(
            json!("round"),
            "labels.round",
            static_icon(&icons::EdgeRoundIcon),
            None,
        ),
    ];
    let value = form_roundness(ctx).map(|r| json!(r.as_str()));
    let mut children = radio_selection(
        "edges",
        choices,
        &value,
        updater(opts, ActionName::ChangeRoundness),
    );
    children.extend(toggle_polygon(ctx, opts));
    button_list_fieldset(Some("labels.edges"), "buttonList", children)
}

/// `actionChangeArrowType`'s panel (:2251-2300).
fn arrow_type(ctx: &ActionContext<'_>, opts: &ActionPanelOptions) -> Vec<Node> {
    let choices = vec![
        choice(
            json!("sharp"),
            "labels.arrowtype_sharp",
            static_icon(&icons::sharpArrowIcon),
            Some("sharp-arrow"),
        ),
        choice(
            json!("round"),
            "labels.arrowtype_round",
            static_icon(&icons::roundArrowIcon),
            Some("round-arrow"),
        ),
        choice(
            json!("elbow"),
            "labels.arrowtype_elbowed",
            static_icon(&icons::elbowArrowIcon),
            Some("elbow-arrow"),
        ),
    ];
    let value = form_arrow_type(ctx).map(|a| json!(arrow_type_str(a)));
    let radios = radio_selection(
        "arrowtypes",
        choices,
        &value,
        updater(opts, ActionName::ChangeArrowType),
    );
    button_list_fieldset(Some("labels.arrowtypes"), "buttonList", radios)
}

/// `actionChangeArrowProperties`'s panel (:2047-2056): the arrowheads and
/// the arrow type.
fn arrow_properties(ctx: &ActionContext<'_>, opts: &ActionPanelOptions) -> Vec<Node> {
    let mut children = arrowheads(ctx, opts);
    children.extend(arrow_type(ctx, opts));
    vec![Element::new("div")
        .attr("class", "selected-shape-actions")
        .children_from(children)
        .into()]
}

// -- arrowheads and IconPicker -------------------------------------------------------------

/// The flip IconPicker's arrowhead previews take (`icons.tsx`,
/// `<g transform={flip ? … : ""}>`).
const ARROWHEAD_FLIP: &str = "translate(40, 0) scale(-1, 1)";

/// An arrowhead preview icon with `flip`: the icons toggle their group's
/// mirroring transform (the cardinality ones are drawn mirrored and flip
/// back), as `icons.json` holds them without `flip`.
pub fn arrowhead_icon(icon: &Icon, flip: bool) -> Node {
    let markup = icon.svg(Theme::Light).expect("an arrowhead icon");
    if !flip {
        return icons::parse_markup(markup).into();
    }
    let mirrored = format!("transform=\"{ARROWHEAD_FLIP}\"");
    let flipped = if markup.contains(&mirrored) {
        markup.replacen(&mirrored, "transform=\"\"", 1)
    } else {
        markup.replacen("transform=\"\"", &mirrored, 1)
    };
    icons::parse_markup(&flipped).into()
}

/// A picker option (`IconPicker.tsx:21-26`).
struct PickerOption {
    value: Option<Arrowhead>,
    text: String,
    key_binding: Option<&'static str>,
    icon: &'static Icon,
}

/// A picker section: `default` renders without a label.
struct PickerSection {
    name: String,
    options: Vec<PickerOption>,
}

const DEFAULT_SECTION_NAME: &str = "default";

/// `PICKER_COLUMNS`.
const PICKER_COLUMNS: usize = 4;

/// Popover.Content's `sideOffset` for the picker (`IconPicker.tsx:238`).
pub const ICON_PICKER_SIDE_OFFSET: f64 = 12.0;

/// Popover.Content's `alignOffset` for the picker (`IconPicker.tsx:239`).
pub const ICON_PICKER_ALIGN_OFFSET: f64 = 12.0;

/// `getArrowheadOptions(flip)` (`actionProperties.tsx:1830-1941`): the
/// visible and hidden sections.
fn arrowhead_options() -> (Vec<PickerSection>, Vec<PickerSection>) {
    use Arrowhead as A;
    let opt = |value, key: &str, key_binding, icon| PickerOption {
        value,
        text: t(key),
        key_binding,
        icon,
    };
    let visible = vec![PickerSection {
        name: DEFAULT_SECTION_NAME.into(),
        options: vec![
            opt(
                None,
                "labels.arrowhead_none",
                Some("q"),
                &icons::ArrowheadNoneIcon,
            ),
            opt(
                Some(A::Arrow),
                "labels.arrowhead_arrow",
                Some("w"),
                &icons::ArrowheadArrowIcon,
            ),
            opt(
                Some(A::Triangle),
                "labels.arrowhead_triangle",
                Some("e"),
                &icons::ArrowheadTriangleIcon,
            ),
            opt(
                Some(A::TriangleOutline),
                "labels.arrowhead_triangle_outline",
                Some("r"),
                &icons::ArrowheadTriangleOutlineIcon,
            ),
        ],
    }];
    let hidden = vec![
        PickerSection {
            name: DEFAULT_SECTION_NAME.into(),
            options: vec![
                opt(
                    Some(A::Circle),
                    "labels.arrowhead_circle",
                    Some("a"),
                    &icons::ArrowheadCircleIcon,
                ),
                opt(
                    Some(A::CircleOutline),
                    "labels.arrowhead_circle_outline",
                    Some("s"),
                    &icons::ArrowheadCircleOutlineIcon,
                ),
                opt(
                    Some(A::Diamond),
                    "labels.arrowhead_diamond",
                    Some("d"),
                    &icons::ArrowheadDiamondIcon,
                ),
                opt(
                    Some(A::DiamondOutline),
                    "labels.arrowhead_diamond_outline",
                    Some("f"),
                    &icons::ArrowheadDiamondOutlineIcon,
                ),
                opt(
                    Some(A::Bar),
                    "labels.arrowhead_bar",
                    Some("z"),
                    &icons::ArrowheadBarIcon,
                ),
            ],
        },
        PickerSection {
            name: t("labels.cardinality"),
            options: vec![
                opt(
                    Some(A::CardinalityOne),
                    "labels.arrowhead_cardinality_one",
                    Some("x"),
                    &icons::ArrowheadCardinalityOneIcon,
                ),
                opt(
                    Some(A::CardinalityMany),
                    "labels.arrowhead_cardinality_many",
                    Some("c"),
                    &icons::ArrowheadCardinalityManyIcon,
                ),
                opt(
                    Some(A::CardinalityOneOrMany),
                    "labels.arrowhead_cardinality_one_or_many",
                    Some("v"),
                    &icons::ArrowheadCardinalityOneOrManyIcon,
                ),
                opt(
                    Some(A::CardinalityExactlyOne),
                    "labels.arrowhead_cardinality_exactly_one",
                    None,
                    &icons::ArrowheadCardinalityExactlyOneIcon,
                ),
                opt(
                    Some(A::CardinalityZeroOrOne),
                    "labels.arrowhead_cardinality_zero_or_one",
                    None,
                    &icons::ArrowheadCardinalityZeroOrOneIcon,
                ),
                opt(
                    Some(A::CardinalityZeroOrMany),
                    "labels.arrowhead_cardinality_zero_or_many",
                    None,
                    &icons::ArrowheadCardinalityZeroOrManyIcon,
                ),
            ],
        },
    ];
    (visible, hidden)
}

/// What a picker's key does (`IconPicker.tsx:88-181`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconPickerKeyOutcome {
    /// `onChange(value)`: the option picked, by index into all the options.
    pub pick: Option<usize>,
    /// `onClose()`.
    pub close: bool,
    pub prevent_default: bool,
    pub stop_propagation: bool,
}

/// `Picker`'s `handleKeyDown` over options laid out as `rows` (the
/// navigation rows: each section's options in rows of four, the hidden
/// sections only while shown), `values` being every option's value in
/// order and `value` the selected one.
pub fn icon_picker_key_handler<T: PartialEq>(
    data: &EventData,
    values: &[T],
    key_bindings: &[Option<&str>],
    rows: &[Vec<usize>],
    value: &T,
    rtl: bool,
) -> IconPickerKeyOutcome {
    let mut out = IconPickerKeyOutcome {
        pick: None,
        close: false,
        prevent_default: false,
        stop_propagation: true,
    };
    let len = values.len();
    let key = data.key.as_str();
    let lower = key.to_lowercase();
    let pressed = key_bindings
        .iter()
        .position(|k| k.is_some_and(|k| k == lower));
    let index = values.iter().position(|v| v == value);
    let modifier = data.meta_key || data.alt_key || data.ctrl_key;
    if let (false, Some(p)) = (modifier, pressed) {
        out.pick = Some(p);
        out.prevent_default = true;
    } else if key == "Tab" {
        // findIndex's -1 wraps as JavaScript's remainder does
        let i = index.map_or(-1, |i| i as isize);
        let n = len as isize;
        let next = if data.shift_key {
            (n + i - 1) % n
        } else {
            (i + 1) % n
        };
        out.pick = Some(next as usize);
    } else if matches!(key, "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown") {
        if let Some(index) = index {
            let (next_key, previous_key) = if rtl {
                ("ArrowLeft", "ArrowRight")
            } else {
                ("ArrowRight", "ArrowLeft")
            };
            let row_of = rows
                .iter()
                .position(|r| r.iter().any(|&o| values[o] == *value));
            let mut next = index;
            if key == next_key {
                next = (index + 1) % len;
            } else if key == previous_key {
                next = (len + index - 1) % len;
            } else if let Some(r) = row_of {
                let column = rows[r]
                    .iter()
                    .position(|&o| values[o] == *value)
                    .unwrap_or(0);
                let target = if key == "ArrowDown" {
                    &rows[(r + 1) % rows.len()]
                } else {
                    &rows[(rows.len() + r - 1) % rows.len()]
                };
                out.pick = Some(
                    target
                        .get(column.min(target.len().saturating_sub(1)))
                        .copied()
                        .unwrap_or(index),
                );
                out.prevent_default = true;
                return out;
            }
            out.pick = Some(next);
        }
        out.prevent_default = true;
    } else if key == "Escape" || key == "Enter" {
        out.prevent_default = true;
        out.close = true;
    }
    out
}

/// The "more options" atom as the open picker leaves it: its effect opens
/// the hidden sections when its value is in them (`IconPicker.tsx:183-187`).
fn effective_more_options(opts: &ActionPanelOptions, open_value_hidden: bool) -> bool {
    opts.icon_picker.more_options || open_value_hidden
}

/// `IconPicker` (`IconPicker.tsx:285-333`) for the arrowhead at
/// `position`: the trigger showing the value's icon and, when open, the
/// picker.
fn arrowhead_picker(
    ctx: &ActionContext<'_>,
    opts: &ActionPanelOptions,
    position: ArrowheadPosition,
    more: bool,
) -> Node {
    let label: &'static str = match position {
        ArrowheadPosition::Start => "arrowhead_start",
        ArrowheadPosition::End => "arrowhead_end",
    };
    // the start previews point left: flipped unless right to left
    let flip = match position {
        ArrowheadPosition::Start => !opts.rtl,
        ArrowheadPosition::End => opts.rtl,
    };
    let value = form_arrowhead(ctx, position);
    let (visible, hidden) = arrowhead_options();
    let is_open = opts.icon_picker.open.as_deref() == Some(label);
    let selected = visible
        .iter()
        .chain(&hidden)
        .flat_map(|s| &s.options)
        .find(|o| o.value == value);
    let on_picker = opts.on_icon_picker.clone();
    let trigger = Element::new("button")
        .attr("type", "button")
        .attr("aria-haspopup", "dialog")
        .attr("aria-expanded", is_open.to_string())
        .attr("data-state", if is_open { "open" } else { "closed" })
        .attr("aria-label", label)
        .attr_opt("class", is_open.then_some("active"))
        .child_opt(selected.map(|o| arrowhead_icon(o.icon, flip)))
        .on_data("click", move |_| {
            if let Some(f) = &on_picker {
                f(IconPickerState {
                    open: (!is_open).then(|| label.to_string()),
                    more_options: more,
                });
            }
            EventResponse::default()
        });
    let mut root = Element::new("div").child(trigger);
    if is_open {
        root = root.child(picker(
            opts, label, position, flip, value, visible, hidden, more,
        ));
    }
    root.into()
}

/// `Picker` (`IconPicker.tsx:71-283`): radix's popper wrapper and content
/// with the sections, the hidden ones behind "More options".
#[allow(clippy::too_many_arguments)]
fn picker(
    opts: &ActionPanelOptions,
    label: &'static str,
    position: ArrowheadPosition,
    flip: bool,
    value: Option<Arrowhead>,
    visible: Vec<PickerSection>,
    hidden: Vec<PickerSection>,
    more: bool,
) -> Node {
    let update = updater(opts, ActionName::ChangeArrowhead);
    let pick = move |v: Option<Arrowhead>| {
        update(json!({ "position": position.as_str(), "type": v.map(|a| a.as_str()) }))
    };
    let all: Vec<&PickerOption> = visible
        .iter()
        .chain(&hidden)
        .flat_map(|s| &s.options)
        .collect();
    let values: Vec<Option<Arrowhead>> = all.iter().map(|o| o.value).collect();
    let bindings: Vec<Option<&'static str>> = all.iter().map(|o| o.key_binding).collect();
    // the navigation rows: each section's options in rows of four
    let mut rows = Vec::new();
    let mut offset = 0;
    for (i, section) in visible.iter().chain(&hidden).enumerate() {
        let shown = i < visible.len() || more;
        for chunk in (0..section.options.len())
            .collect::<Vec<_>>()
            .chunks(PICKER_COLUMNS)
        {
            if shown {
                rows.push(chunk.iter().map(|o| o + offset).collect::<Vec<_>>());
            }
        }
        offset += section.options.len();
    }
    let rtl = opts.rtl;
    let on_close = opts.on_icon_picker.clone();
    let key_pick = pick.clone();
    let on_key = move |data: &EventData| {
        let out = icon_picker_key_handler(data, &values, &bindings, &rows, &value, rtl);
        if let Some(i) = out.pick {
            key_pick(values[i]);
        }
        if out.close {
            if let Some(f) = &on_close {
                f(IconPickerState {
                    open: None,
                    more_options: more,
                });
            }
        }
        EventResponse {
            prevent_default: out.prevent_default,
            stop_propagation: out.stop_propagation,
        }
    };
    let options = |section: &PickerSection| -> Element {
        Element::new("div")
            .attr("class", "picker-content")
            .children_from(
                section
                    .options
                    .iter()
                    .map(|o| {
                        let active = o.value == value;
                        let pick = pick.clone();
                        let picked = o.value;
                        let title = match o.key_binding {
                            Some(k) => format!("{} — {}", o.text, k.to_uppercase()),
                            None => o.text.clone(),
                        };
                        let mut button = Element::new("button")
                            .attr("type", "button")
                            .attr(
                                "class",
                                class_names([("picker-option", true), ("active", active)]),
                            )
                            .attr("title", title)
                            .attr(
                                "aria-label",
                                if o.text.is_empty() {
                                    "none".into()
                                } else {
                                    o.text.clone()
                                },
                            )
                            .attr_opt("aria-keyshortcuts", o.key_binding)
                            .child(arrowhead_icon(o.icon, flip))
                            .child_opt(o.key_binding.map(|k| {
                                Element::new("span")
                                    .attr("class", "picker-keybinding")
                                    .child(k)
                            }))
                            .on_data("click", move |_| {
                                pick(picked);
                                EventResponse::default()
                            });
                        if active {
                            button = button.on_mount(focus_later);
                        }
                        Node::from(button)
                    })
                    .collect::<Vec<_>>(),
            )
    };
    let sections = |list: &[PickerSection]| -> Vec<Node> {
        list.iter()
            .map(|s| {
                if s.name == DEFAULT_SECTION_NAME {
                    options(s).into()
                } else {
                    Element::new("div")
                        .attr("class", "picker-section")
                        .child(
                            Element::new("div")
                                .attr("class", "picker-section-label")
                                .child(s.name.clone()),
                        )
                        .child(options(s))
                        .into()
                }
            })
            .collect()
    };
    let mut content = Element::new("div")
        .attr("class", "picker-sections")
        .children_from(sections(&visible));
    if !hidden.is_empty() {
        let on_toggle = opts.on_icon_picker.clone();
        let open = opts.icon_picker.open.clone();
        let header = Element::new("div")
            .attr("class", "picker-collapsible")
            .style("cursor", "pointer")
            .style("display", "flex")
            .style("justify-content", "space-between")
            .style("align-items", "center")
            .child(t("labels.more_options"))
            .child(inline_icon(if more {
                &icons::collapseUpIcon
            } else {
                &icons::collapseDownIcon
            }))
            .on_data("click", move |_| {
                if let Some(f) = &on_toggle {
                    f(IconPickerState {
                        open: open.clone(),
                        more_options: !more,
                    });
                }
                EventResponse::default()
            });
        content = content.child(header);
        if more {
            content = content.child(
                Element::new("div")
                    .style("display", "flex")
                    .style("flex-direction", "column")
                    .child(
                        Element::new("div")
                            .attr("class", "picker-sections")
                            .children_from(sections(&hidden)),
                    ),
            );
        }
    }
    let popover = Element::new("div")
        .attr("class", "picker")
        .attr("role", "dialog")
        .attr("aria-modal", "true")
        .attr("aria-label", label)
        .attr("data-side", "bottom")
        .attr("data-align", "start")
        .attr("data-state", "open")
        .attr("tabindex", "-1")
        .style("z-index", "var(--zIndex-ui-styles-popup)")
        .child(content)
        .on_data("keydown", on_key);
    Element::new("div")
        .attr("data-radix-popper-content-wrapper", "")
        .child(popover)
        .into()
}

/// Focuses the element after a tick (the active option's ref callback).
fn focus_later(el: &web_sys::Element) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::closure::Closure;
        use wasm_bindgen::JsCast;
        let el = el.clone();
        let focus = Closure::once_into_js(move || {
            if let Some(h) = el.dyn_ref::<web_sys::HtmlElement>() {
                let _ = h.focus();
            }
        });
        if let Some(window) = web_sys::window() {
            let _ = window
                .set_timeout_with_callback_and_timeout_and_arguments_0(focus.unchecked_ref(), 0);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = el;
}

/// `InlineIcon` (`components/InlineIcon.tsx`) at its default size.
fn inline_icon(icon: &Icon) -> Element {
    Element::new("span")
        .style("width", "1em")
        .style("height", "100%")
        .style("margin", "0 0.5ex 0 0.5ex")
        .style("display", "inline-flex")
        .style("line-height", "0")
        .style("vertical-align", "middle")
        .style("flex", "0 0 auto")
        .child(static_icon(icon))
}

/// `actionChangeArrowhead`'s panel (:1943-2001): the start and end
/// pickers.
fn arrowheads(ctx: &ActionContext<'_>, opts: &ActionPanelOptions) -> Vec<Node> {
    let (_, hidden) = arrowhead_options();
    let open_value_hidden = match opts.icon_picker.open.as_deref() {
        Some("arrowhead_start") => Some(ArrowheadPosition::Start),
        Some("arrowhead_end") => Some(ArrowheadPosition::End),
        _ => None,
    }
    .is_some_and(|p| {
        let v = form_arrowhead(ctx, p);
        hidden.iter().flat_map(|s| &s.options).any(|o| o.value == v)
    });
    let more = effective_more_options(opts, open_value_hidden);
    let pickers = vec![
        arrowhead_picker(ctx, opts, ArrowheadPosition::Start, more),
        arrowhead_picker(ctx, opts, ArrowheadPosition::End, more),
    ];
    button_list_fieldset(
        Some("labels.arrowheads"),
        "iconSelectList buttonList",
        pickers,
    )
}

// -- buttons -------------------------------------------------------------------------------

/// `actionTogglePolygon`'s panel (`actionLinearEditor.tsx:170-205`).
fn toggle_polygon(ctx: &ActionContext<'_>, opts: &ActionPanelOptions) -> Vec<Node> {
    let Some(all_polygon) = polygon_toggle(ctx) else {
        return Vec::new();
    };
    let label = t(if all_polygon {
        "labels.polygon.breakPolygon"
    } else {
        "labels.polygon.convertToPolygon"
    });
    let update = updater(opts, ActionName::TogglePolygon);
    vec![icon_button_node(
        IconButtonProps {
            kind: IconButtonKind::Toggle {
                checked: all_polygon,
            },
            icon: Some(static_icon(&icons::polygonIcon)),
            title: Some(label.clone()),
            aria_label: label,
            style: vec![("margin-left".into(), "auto".into())],
            ..IconButtonProps::default()
        },
        move || update(Value::Null),
    )]
}

/// `actionToggleLinearEditor`'s panel (`actionLinearEditor.tsx:80-102`).
fn toggle_linear_editor(ctx: &ActionContext<'_>, opts: &ActionPanelOptions) -> Vec<Node> {
    let Some(element) = linear_editor_target(ctx) else {
        return Vec::new();
    };
    let label = t(if element.element_type() == ElementType::Arrow {
        "labels.lineEditor.editArrow"
    } else {
        "labels.lineEditor.edit"
    });
    let update = updater(opts, ActionName::ToggleLinearEditor);
    vec![icon_button_node(
        button_props(
            static_icon(&icons::lineEditorIcon),
            Some(label.clone()),
            label,
        ),
        move || update(Value::Null),
    )]
}

/// An align or distribute button: hidden unless `enabled`, visible with a
/// selection.
fn align_button(
    ctx: &ActionContext<'_>,
    opts: &ActionPanelOptions,
    name: ActionName,
    enabled: bool,
) -> Vec<Node> {
    let (icon, key, keys): (&Icon, &str, Option<&str>) = match name {
        ActionName::AlignTop => (
            &icons::AlignTopIcon,
            "labels.alignTop",
            Some("CtrlOrCmd+Shift+Up"),
        ),
        ActionName::AlignBottom => (
            &icons::AlignBottomIcon,
            "labels.alignBottom",
            Some("CtrlOrCmd+Shift+Down"),
        ),
        ActionName::AlignLeft => (
            &icons::AlignLeftIcon,
            "labels.alignLeft",
            Some("CtrlOrCmd+Shift+Left"),
        ),
        ActionName::AlignRight => (
            &icons::AlignRightIcon,
            "labels.alignRight",
            Some("CtrlOrCmd+Shift+Right"),
        ),
        ActionName::AlignVerticallyCentered => (
            &icons::CenterVerticallyIcon,
            "labels.centerVertically",
            None,
        ),
        ActionName::AlignHorizontallyCentered => (
            &icons::CenterHorizontallyIcon,
            "labels.centerHorizontally",
            None,
        ),
        ActionName::DistributeHorizontally => (
            &icons::DistributeHorizontallyIcon,
            "labels.distributeHorizontally",
            Some("Alt+H"),
        ),
        _ => (
            &icons::DistributeVerticallyIcon,
            "labels.distributeVertically",
            Some("Alt+V"),
        ),
    };
    let label = t(key);
    let title = match keys {
        Some(keys) => format!("{label} — {}", shortcut(keys, opts)),
        None => label.clone(),
    };
    let update = updater(opts, name);
    vec![icon_button_node(
        IconButtonProps {
            hidden: !enabled,
            visible: is_some_element_selected(ctx),
            ..button_props(static_icon(icon), Some(title), label)
        },
        move || update(Value::Null),
    )]
}

/// A layer button (`actionZindex.tsx`): a plain `zIndexButton`.
fn z_index_button(opts: &ActionPanelOptions, name: ActionName) -> Vec<Node> {
    let darwin = opts.is_darwin;
    let (icon, key, keys) = match name {
        ActionName::SendBackward => (
            &icons::SendBackwardIcon,
            "labels.sendBackward",
            "CtrlOrCmd+[",
        ),
        ActionName::BringForward => (
            &icons::BringForwardIcon,
            "labels.bringForward",
            "CtrlOrCmd+]",
        ),
        ActionName::SendToBack => (
            &icons::SendToBackIcon,
            "labels.sendToBack",
            if darwin {
                "CtrlOrCmd+Alt+["
            } else {
                "CtrlOrCmd+Shift+["
            },
        ),
        _ => (
            &icons::BringToFrontIcon,
            "labels.bringToFront",
            if darwin {
                "CtrlOrCmd+Alt+]"
            } else {
                "CtrlOrCmd+Shift+]"
            },
        ),
    };
    let update = updater(opts, name);
    vec![Element::new("button")
        .attr("type", "button")
        .attr("class", "zIndexButton")
        .attr("title", format!("{} — {}", t(key), shortcut(keys, opts)))
        .child(static_icon(icon))
        .on_data("click", move |_| {
            update(Value::Null);
            EventResponse::default()
        })
        .into()]
}

/// `actionGroup`'s and `actionUngroup`'s panels (`actionGroup.tsx`).
fn group_button(ctx: &ActionContext<'_>, opts: &ActionPanelOptions, name: ActionName) -> Vec<Node> {
    let (icon, key, keys, hidden) = if name == ActionName::Group {
        (
            &icons::GroupIcon,
            "labels.group",
            "CtrlOrCmd+G",
            !group_enabled(ctx),
        )
    } else {
        (
            &icons::UngroupIcon,
            "labels.ungroup",
            "CtrlOrCmd+Shift+G",
            !has_selected_groups(ctx),
        )
    };
    let label = t(key);
    let title = format!("{label} — {}", shortcut(keys, opts));
    let update = updater(opts, name);
    vec![icon_button_node(
        IconButtonProps {
            hidden,
            visible: is_some_element_selected(ctx),
            ..button_props(icon_node(icon, opts.theme), Some(title), label)
        },
        move || update(Value::Null),
    )]
}

/// `actionDuplicateSelection`'s and `actionDeleteSelected`'s panels:
/// disabled without a selection, on the mobile row's background.
fn selection_button(
    ctx: &ActionContext<'_>,
    opts: &ActionPanelOptions,
    name: ActionName,
) -> Vec<Node> {
    let (icon, label, title) = if name == ActionName::DuplicateSelection {
        let label = t("labels.duplicateSelection");
        let title = format!("{label} — {}", shortcut("CtrlOrCmd+D", opts));
        (&icons::DuplicateIcon, label, title)
    } else {
        let label = t("labels.delete");
        (&icons::TrashIcon, label.clone(), label)
    };
    let update = updater(opts, name);
    vec![icon_button_node(
        IconButtonProps {
            disabled: !is_some_element_selected(ctx),
            style: mobile_background(opts, false),
            ..button_props(static_icon(icon), Some(title), label)
        },
        move || update(Value::Null),
    )]
}

/// `actionLink`'s panel (`actionLink.tsx:45-66`).
fn hyperlink(ctx: &ActionContext<'_>, opts: &ActionPanelOptions) -> Vec<Node> {
    let state = link_panel_state(ctx);
    let update = updater(opts, ActionName::Hyperlink);
    vec![icon_button_node(
        IconButtonProps {
            kind: IconButtonKind::Toggle {
                checked: state.checked,
            },
            icon: Some(static_icon(&icons::LinkIcon)),
            aria_label: t(state.label),
            title: Some(format!(
                "{} - {}",
                t(state.title),
                shortcut("CtrlOrCmd+K", opts)
            )),
            ..IconButtonProps::default()
        },
        move || update(Value::Null),
    )]
}

/// `actionToggleCropEditor`'s panel (`actionCropEditor.tsx:46-58`).
fn crop_editor(opts: &ActionPanelOptions) -> Vec<Node> {
    let label = t("helpDialog.cropStart");
    let update = updater(opts, ActionName::CropEditor);
    vec![icon_button_node(
        button_props(static_icon(&icons::cropIcon), Some(label.clone()), label),
        move || update(Value::Null),
    )]
}

/// The undo and redo panels (`actionHistory.tsx:81-150`): disabled with
/// an empty stack; `updateData(event)` (the event, which `perform`
/// ignores, is `null` here).
fn history_button(opts: &ActionPanelOptions, name: ActionName) -> Vec<Node> {
    let (icon, key, test_id, empty) = if name == ActionName::Undo {
        (
            &icons::UndoIcon,
            "buttons.undo",
            "button-undo",
            opts.undo_stack_empty,
        )
    } else {
        (
            &icons::RedoIcon,
            "buttons.redo",
            "button-redo",
            opts.redo_stack_empty,
        )
    };
    let update = updater(opts, name);
    vec![icon_button_node(
        IconButtonProps {
            disabled: empty,
            test_id: Some(test_id.into()),
            style: mobile_background(opts, true),
            ..button_props(static_icon(icon), None, t(key))
        },
        move || update(Value::Null),
    )]
}

// -- the entry points ------------------------------------------------------------------------

/// What `renderAction(name, data)` renders for the styles panels' action
/// `name`: its `PanelComponent`'s nodes (empty when it renders `null`,
/// e.g. the polygon toggle without polygons; nested `renderAction` calls
/// rendered in place); `None` when the host renders it itself (the colour
/// actions, the bucket fill colour and the font family, stateful pickers:
/// see [`crate::styles_panel::color_action_panel`],
/// [`bucket_fill_color_panel`] and [`font_family_panel`]) or it has no
/// panel component in the styles panels.
pub fn render_action_panel(
    ctx: &ActionContext<'_>,
    name: ActionName,
    opts: &ActionPanelOptions,
) -> Option<Vec<Node>> {
    use ActionName as N;
    Some(match name {
        N::ChangeFillStyle => fill_style(ctx, opts),
        N::ChangeStrokeWidth => stroke_width(ctx, opts),
        N::ChangeSloppiness => sloppiness(ctx, opts),
        N::ChangeFreedrawMode => freedraw_mode(ctx, opts),
        N::ChangeStrokeStyle => stroke_style(ctx, opts),
        N::ChangeOpacity => opacity(ctx, opts),
        N::ChangeFontSize => font_size(ctx, opts),
        N::ChangeTextAlign => text_align(ctx, opts),
        N::ChangeVerticalAlign => vertical_align(ctx, opts),
        N::ChangeRoundness => roundness(ctx, opts),
        N::TogglePolygon => toggle_polygon(ctx, opts),
        N::ChangeArrowhead => arrowheads(ctx, opts),
        N::ChangeArrowProperties => arrow_properties(ctx, opts),
        N::ChangeArrowType => arrow_type(ctx, opts),
        N::ToggleLinearEditor => toggle_linear_editor(ctx, opts),
        N::AlignTop
        | N::AlignBottom
        | N::AlignLeft
        | N::AlignRight
        | N::AlignVerticallyCentered
        | N::AlignHorizontallyCentered => align_button(ctx, opts, name, align_enabled(ctx)),
        N::DistributeHorizontally | N::DistributeVertically => {
            align_button(ctx, opts, name, distribute_enabled(ctx))
        }
        N::SendToBack | N::SendBackward | N::BringForward | N::BringToFront => {
            z_index_button(opts, name)
        }
        N::Group | N::Ungroup => group_button(ctx, opts, name),
        N::DuplicateSelection | N::DeleteSelectedElements => selection_button(ctx, opts, name),
        N::Hyperlink => hyperlink(ctx, opts),
        N::CropEditor => crop_editor(opts),
        N::Undo | N::Redo => history_button(opts, name),
        _ => return None,
    })
}

/// What `actionChangeFontFamily`'s panel (`actionProperties.tsx:1361-1540`)
/// renders: the legend in the full panel and the `FontPicker`, whose
/// props these are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontFamilyPanel {
    /// The legend's text (`labels.fontFamily`) in the full panel.
    pub legend: Option<String>,
    /// `selectedFontFamily` (the selection's `getFormValue`, as with the
    /// popup closed), `hoveredFontFamily`
    /// (`appState.currentHoveredFontFamily`), `topPicks`
    /// (`appState.fontTopPicks`) and `appState.openPopup`.
    pub state: FontPickerState,
    /// `isOpened`: `appState.openPopup === "fontFamily"`.
    pub is_opened: bool,
    /// `compactMode`: outside the full panel.
    pub compact_mode: bool,
}

/// [`FontFamilyPanel`] for the selection in `mode`; render it with
/// [`crate::font_picker::font_picker`].
pub fn font_family_panel(ctx: &ActionContext<'_>, mode: StylesPanelMode) -> FontFamilyPanel {
    let state = FontPickerState::from_app_state(ctx.app_state.as_map(), form_font_family(ctx));
    FontFamilyPanel {
        legend: (mode == StylesPanelMode::Full).then(|| t("labels.fontFamily")),
        is_opened: state.open_popup.as_deref() == Some("fontFamily"),
        compact_mode: mode != StylesPanelMode::Full,
        state,
    }
}

/// `COLOR_PALETTE.transparent`, hidden from the bucket fill's picker.
const BUCKET_FILL_EXCLUDED_COLORS: &[&str] = &["transparent"];

/// What `actionChangeBucketFillBackgroundColor`'s panel
/// (`actionProperties.tsx:560-601`) renders: in the full panel the
/// heading, then the `ColorPicker` with the bucket fill's picks, its
/// `bucketFill` slot, `transparent` hidden and the tool's colour.
pub fn bucket_fill_color_panel(ctx: &ActionContext<'_>, mode: StylesPanelMode) -> ColorActionPanel {
    ColorActionPanel {
        heading: (mode == StylesPanelMode::Full).then_some("labels.background"),
        ty: ColorPickerType::ElementBackground,
        label: "labels.background",
        color: Some(bucket_fill_color(ctx)),
        palette: &DEFAULT_ELEMENT_BACKGROUND_COLOR_PALETTE,
        top_picks: BUCKET_FILL_BACKGROUND_PICKS,
        customizable_top_picks: ColorTopPicksSlot::BucketFill,
        excluded_colors: Some(BUCKET_FILL_EXCLUDED_COLORS),
    }
}

/// Places each open IconPicker popover under `root` as radix's
/// `Popover.Content` does (`IconPicker.tsx:251-259`: `side="bottom"`,
/// `align="start"`, `sideOffset={12}`, `alignOffset={12}`): below its
/// trigger, from the trigger's left edge, flipped above when it would
/// leave the editor. Call once the panel is in the document.
pub fn place_icon_pickers(root: &web_sys::Element) {
    use wasm_bindgen::JsCast;
    let Ok(pickers) = root.query_selector_all("[data-radix-popper-content-wrapper] > .picker")
    else {
        return;
    };
    for i in 0..pickers.length() {
        let Some(picker) = pickers
            .item(i)
            .and_then(|n| n.dyn_into::<web_sys::Element>().ok())
        else {
            continue;
        };
        let Some(wrapper) = picker
            .parent_element()
            .and_then(|w| w.dyn_into::<web_sys::HtmlElement>().ok())
        else {
            continue;
        };
        let label = picker.get_attribute("aria-label").unwrap_or_default();
        let selector = format!("button[aria-label=\"{label}\"][aria-expanded=\"true\"]");
        let Ok(Some(trigger)) = root.query_selector(&selector) else {
            continue;
        };
        let t = trigger.get_bounding_client_rect();
        let c = picker.get_bounding_client_rect();
        let bottom_edge = wrapper
            .closest(".excalidraw")
            .ok()
            .flatten()
            .map_or(f64::INFINITY, |e| e.get_bounding_client_rect().bottom());
        let x = t.left() + ICON_PICKER_ALIGN_OFFSET;
        let below = t.bottom() + ICON_PICKER_SIDE_OFFSET;
        let (side, y) = if below + c.height() > bottom_edge {
            ("top", t.top() - ICON_PICKER_SIDE_OFFSET - c.height())
        } else {
            ("bottom", below)
        };
        let _ = picker.set_attribute("data-side", side);
        let style = wrapper.style();
        let _ = style.set_property("position", "fixed");
        let _ = style.set_property("left", "0px");
        let _ = style.set_property("top", "0px");
        let _ = style.set_property("transform", &format!("translate({x}px, {y}px)"));
        let _ = style.set_property("min-width", "max-content");
        let _ = style.set_property("z-index", "var(--zIndex-ui-styles-popup)");
    }
}
