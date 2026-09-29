//! The picker's keyboard map, `colorPickerKeyNavHandler`
//! (`components/ColorPicker/keyboardNavHandlers.ts`): Escape, Alt and `i`
//! for the eye dropper, Tab between sections, the q…b palette hotkeys,
//! 1–5 for the most-used custom colours, Shift+1–5 for shades, and the
//! arrows inside the active section.
//!
//! Indices are JavaScript numbers here, as upstream computes them: an
//! arrow in an empty custom list yields `NaN`, and a "shade" of a single
//! colour indexes the colour string's characters (`"#1e1e1e"[4]` is `"1"`);
//! an index that is no element yields `undefined` ([`KeyNavEffect::Change`]
//! of `None`).

use excali_core::color::{PaletteColor, PaletteEntry, COLORS_PER_ROW, COLOR_PALETTE};

use super::{get_color_name_and_shade, Section, COLOR_PICKER_HOTKEY_BINDINGS};

/// The parts of a `keydown` event the map reads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyInput {
    /// `event.key`.
    pub key: String,
    /// `event.code`.
    pub code: String,
    /// `event.shiftKey`.
    pub shift: bool,
    /// `event[KEYS.CTRL_OR_CMD]`: `metaKey` on a Mac, `ctrlKey` elsewhere.
    pub ctrl_or_cmd: bool,
}

/// What the picker's state is when a key is pressed.
#[derive(Clone, Copy, Debug)]
pub struct KeyNavState<'a> {
    /// `activeColorPickerSectionAtom`.
    pub section: Option<Section>,
    pub palette: &'a [PaletteEntry],
    /// The picked colour; `None` for a mixed selection.
    pub color: Option<&'a str>,
    /// The most-used custom colours the picker opened with.
    pub custom_colors: &'a [String],
    /// The shade the palette's hues show (Picker.tsx:124-130).
    pub active_shade: usize,
    /// Palette colours hidden from the grid (their hotkeys are dead).
    pub excluded_colors: &'a [&'a str],
}

/// A callback the map runs, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyNavEffect {
    /// `onChange(color)`; `None` is `undefined`.
    Change(Option<String>),
    /// `setActiveColorPickerSection(section)`.
    SetSection(Section),
    /// `onEyeDropperToggle(force)`.
    EyeDropperToggle(Option<bool>),
    /// `onEscape(event)`.
    Escape,
}

/// The map's result: whether it handled the key (the picker then prevents
/// the default and stops propagation) and the callbacks it ran.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyNavOutcome {
    pub handled: bool,
    pub effects: Vec<KeyNavEffect>,
}

impl KeyNavOutcome {
    fn handled(effects: Vec<KeyNavEffect>) -> KeyNavOutcome {
        KeyNavOutcome {
            handled: true,
            effects,
        }
    }
}

/// `arrowHandler` (`keyboardNavHandlers.ts:17-45`): the index an arrow key
/// moves to in a grid `COLORS_PER_ROW` wide, or `None` (`undefined`) for
/// any other key or an up move past the last row.
fn arrow_handler(key: &str, current: Option<f64>, length: f64) -> Option<f64> {
    let per_row = COLORS_PER_ROW as f64;
    let rows = (length / per_row).ceil();
    let current = current.unwrap_or(-1.0);
    match key {
        "ArrowLeft" => {
            let prev = current - 1.0;
            Some(if prev < 0.0 { length - 1.0 } else { prev })
        }
        // JavaScript's `%` is Rust's on f64 (the dividend's sign, NaN for 0)
        "ArrowRight" => Some((current + 1.0) % length),
        "ArrowDown" => {
            let next = current + per_row;
            Some(if next >= length {
                current % per_row
            } else {
                next
            })
        }
        "ArrowUp" => {
            let prev = current - per_row;
            let index = if prev < 0.0 {
                per_row * rows + prev
            } else {
                prev
            };
            (index < length).then_some(index)
        }
        _ => None,
    }
}

/// `list[index]` for a JavaScript number: `None` (`undefined`) unless it
/// is a whole index inside the list.
fn at<T: Clone>(list: &[T], index: f64) -> Option<T> {
    if index >= 0.0 && index.fract() == 0.0 {
        list.get(index as usize).cloned()
    } else {
        None
    }
}

/// `palette[name][index]`: a shade, or for a single colour the character
/// of the string at `index`.
fn entry_at(value: PaletteColor, index: f64) -> Option<String> {
    match value {
        PaletteColor::Shades(shades) => at(&shades, index).map(str::to_owned),
        PaletteColor::Single(color) => {
            let units: Vec<u16> = color.encode_utf16().collect();
            at(&units, index).map(|u| String::from_utf16_lossy(&[u]))
        }
    }
}

/// The colour a palette entry shows at `shade`.
fn resolved(value: PaletteColor, shade: usize) -> Option<&'static str> {
    match value {
        PaletteColor::Shades(shades) => shades.get(shade).copied(),
        PaletteColor::Single(color) => Some(color),
    }
}

fn lookup(palette: &[PaletteEntry], name: &str) -> Option<PaletteColor> {
    palette.iter().find(|e| e.0 == name).map(|e| e.1)
}

/// `hotkeyHandler` (`keyboardNavHandlers.ts:64-121`).
fn hotkey_handler(event: &KeyInput, state: &KeyNavState<'_>) -> Option<KeyNavOutcome> {
    let obj = get_color_name_and_shade(state.palette, state.color);
    if let Some(obj) = obj.filter(|o| o.shade.is_some()) {
        let digit = event
            .code
            .strip_prefix("Digit")
            .and_then(|d| d.parse::<usize>().ok())
            .filter(|d| (1..=5).contains(d));
        if let (Some(digit), true) = (digit, event.shift) {
            let color =
                lookup(state.palette, obj.color_name).and_then(|v| entry_at(v, (digit - 1) as f64));
            return Some(KeyNavOutcome::handled(vec![
                KeyNavEffect::Change(color),
                KeyNavEffect::SetSection(Section::Shades),
            ]));
        }
    }

    if let Some(n) = ["1", "2", "3", "4", "5"]
        .iter()
        .position(|d| *d == event.key)
    {
        if let Some(color) = state.custom_colors.get(n) {
            return Some(KeyNavOutcome::handled(vec![
                KeyNavEffect::Change(Some(color.clone())),
                KeyNavEffect::SetSection(Section::Custom),
            ]));
        }
    }

    if let Some(index) = COLOR_PICKER_HOTKEY_BINDINGS
        .iter()
        .position(|k| *k == event.key)
    {
        let color = state
            .palette
            .get(index)
            .and_then(|e| resolved(e.1, state.active_shade));
        // an excluded (hidden) or absent entry's hotkey is dead, but still
        // handled so it does not reach the global shortcuts
        return Some(match color {
            Some(c) if !state.excluded_colors.contains(&c) => KeyNavOutcome::handled(vec![
                KeyNavEffect::Change(Some(c.to_owned())),
                KeyNavEffect::SetSection(Section::BaseColors),
            ]),
            _ => KeyNavOutcome::handled(Vec::new()),
        });
    }
    None
}

/// `colorPickerKeyNavHandler` (`keyboardNavHandlers.ts:145-318`).
pub fn color_picker_key_nav_handler(event: &KeyInput, state: &KeyNavState<'_>) -> KeyNavOutcome {
    if event.ctrl_or_cmd {
        return KeyNavOutcome::default();
    }
    match event.key.as_str() {
        "Escape" => return KeyNavOutcome::handled(vec![KeyNavEffect::Escape]),
        // `key`, so Alt combinations are left alone
        "Alt" => return KeyNavOutcome::handled(vec![KeyNavEffect::EyeDropperToggle(Some(true))]),
        "i" => return KeyNavOutcome::handled(vec![KeyNavEffect::EyeDropperToggle(None)]),
        _ => {}
    }

    let obj = get_color_name_and_shade(state.palette, state.color);

    if event.key == "Tab" {
        let sections: Vec<Section> = [
            (Section::Custom, !state.custom_colors.is_empty()),
            (Section::BaseColors, true),
            (Section::Shades, obj.is_some_and(|o| o.shade.is_some())),
            (Section::Hex, true),
        ]
        .into_iter()
        .filter_map(|(s, on)| on.then_some(s))
        .collect();
        let active = state
            .section
            .and_then(|s| sections.iter().position(|x| *x == s))
            .map_or(-1, |i| i as i64);
        let len = sections.len() as i64;
        let next = active + if event.shift { -1 } else { 1 };
        let next = if next > len - 1 {
            0
        } else if next < 0 {
            len - 1
        } else {
            next
        };
        let next = sections[next as usize];
        let mut effects = vec![KeyNavEffect::SetSection(next)];
        match next {
            Section::Custom => {
                effects.push(KeyNavEffect::Change(state.custom_colors.first().cloned()));
            }
            Section::BaseColors => {
                let in_palette = state.palette.iter().any(|(_, value)| match value {
                    PaletteColor::Shades(shades) => {
                        state.color.is_some_and(|c| shades.contains(&c))
                    }
                    PaletteColor::Single(c) => state.color == Some(*c),
                });
                if !in_palette {
                    effects.push(KeyNavEffect::Change(Some(COLOR_PALETTE.black.to_owned())));
                }
            }
            _ => {}
        }
        return KeyNavOutcome::handled(effects);
    }

    if let Some(outcome) = hotkey_handler(event, state) {
        return outcome;
    }

    if state.section == Some(Section::Shades) {
        if let Some(obj) = obj {
            let shade = obj.shade.map(|s| s as f64);
            if let Some(next) = arrow_handler(&event.key, shade, COLORS_PER_ROW as f64) {
                let color = lookup(state.palette, obj.color_name).and_then(|v| entry_at(v, next));
                return KeyNavOutcome::handled(vec![KeyNavEffect::Change(color)]);
            }
        }
    }

    if state.section == Some(Section::BaseColors) {
        if let Some(obj) = obj {
            let names: Vec<&str> = state.palette.iter().map(|e| e.0).collect();
            let length = names.len() as f64;
            let current = names
                .iter()
                .position(|n| *n == obj.color_name)
                .map_or(-1.0, |i| i as f64);
            let mut next = arrow_handler(&event.key, Some(current), length);
            // step over excluded (hidden) entries in the arrow's direction
            let mut guard = 0;
            while let Some(index) = next {
                guard += 1;
                if guard > names.len() {
                    break;
                }
                let color =
                    at(state.palette, index).and_then(|e| resolved(e.1, state.active_shade));
                if !color.is_some_and(|c| state.excluded_colors.contains(&c)) {
                    break;
                }
                next = arrow_handler(&event.key, Some(index), length);
            }
            if let Some(index) = next {
                let color = at(state.palette, index)
                    .and_then(|e| resolved(e.1, state.active_shade))
                    .map(str::to_owned);
                return KeyNavOutcome::handled(vec![KeyNavEffect::Change(color)]);
            }
        }
    }

    if state.section == Some(Section::Custom) {
        let current = match state.color {
            Some(c) => state
                .custom_colors
                .iter()
                .position(|x| x == c)
                .map_or(-1.0, |i| i as f64),
            None => 0.0,
        };
        let length = state.custom_colors.len() as f64;
        if let Some(next) = arrow_handler(&event.key, Some(current), length) {
            return KeyNavOutcome::handled(vec![KeyNavEffect::Change(at(
                state.custom_colors,
                next,
            ))]);
        }
    }

    KeyNavOutcome::default()
}
