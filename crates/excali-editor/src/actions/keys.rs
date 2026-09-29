//! Key events as the `keyTest`s read them (`packages/common/src/keys.ts`)
//! and the shortcut strings the menus show (`actions/shortcuts.ts`,
//! `shortcut.ts`).

/// A keydown as the `keyTest`s read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent<'a> {
    /// `event.key`.
    pub key: &'a str,
    /// `event.code`.
    pub code: &'a str,
    pub shift_key: bool,
    pub alt_key: bool,
    pub ctrl_key: bool,
    pub meta_key: bool,
    /// `isWritableElement(event.target)`: the key went to a text field.
    pub target_is_writable: bool,
}

impl<'a> KeyEvent<'a> {
    /// A keydown with no modifier held, not from a text field.
    pub fn new(key: &'a str, code: &'a str) -> KeyEvent<'a> {
        KeyEvent {
            key,
            code,
            shift_key: false,
            alt_key: false,
            ctrl_key: false,
            meta_key: false,
            target_is_writable: false,
        }
    }

    /// `event[KEYS.CTRL_OR_CMD]`: `metaKey` on a Mac, `ctrlKey` elsewhere
    /// (`keys.ts:39`).
    pub fn ctrl_or_cmd(&self, is_darwin: bool) -> bool {
        if is_darwin {
            self.meta_key
        } else {
            self.ctrl_key
        }
    }
}

/// `KeyCodeMap` (`keys.ts:94-97`): the code a key falls back to on
/// non-Latin layouts.
fn key_code_fallback(key: &str) -> Option<&'static str> {
    match key {
        "z" => Some("KeyZ"),
        "y" => Some("KeyY"),
        _ => None,
    }
}

/// `isLatinChar` (`keys.ts:99`): a single a-z letter, either case.
fn is_latin_char(key: &str) -> bool {
    let lower = key.to_lowercase();
    let mut chars = lower.chars();
    matches!((chars.next(), chars.next()), (Some(c), None) if c.is_ascii_lowercase())
}

/// `matchKey(event, key)` (`keys.ts:126-137`): the lower-cased `event.key`,
/// or on non-Latin layouts the physical code for Z and Y.
pub fn match_key(event: &KeyEvent<'_>, key: &str) -> bool {
    if key == event.key.to_lowercase() {
        return true;
    }
    key_code_fallback(key).is_some_and(|code| !is_latin_char(event.key) && event.code == code)
}

/// The localized strings shortcut labels use (`keys.*` and
/// `helpDialog.drag` in the locale files).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyLabels<'a> {
    pub ctrl: &'a str,
    pub option: &'a str,
    pub cmd: &'a str,
    pub alt: &'a str,
    pub escape: &'a str,
    pub enter: &'a str,
    pub shift: &'a str,
    pub spacebar: &'a str,
    pub delete: &'a str,
    pub drag: &'a str,
}

impl KeyLabels<'static> {
    /// `en.json` (`keys`, `helpDialog.drag`).
    pub const EN: KeyLabels<'static> = KeyLabels {
        ctrl: "Ctrl",
        option: "Option",
        cmd: "Cmd",
        alt: "Alt",
        escape: "Esc",
        enter: "Enter",
        shift: "Shift",
        spacebar: "Space",
        delete: "Delete",
        drag: "drag",
    };
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `s.replace(/\b(alt1|alt2|...)\b/i, with)` (every match with `global`):
/// at each position, the alternatives are tried in order, as a regular
/// expression would, each needing a word boundary on both sides.
fn replace_word(s: &str, alternatives: &[&str], with: &str, global: bool) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    let mut replaced = false;
    while i < s.len() {
        let at_boundary = s[..i].chars().next_back().is_none_or(|c| !is_word_char(c));
        if at_boundary && (global || !replaced) {
            let hit = alternatives.iter().find(|alt| {
                let end = i + alt.len();
                end <= s.len()
                    && s.is_char_boundary(end)
                    && s[i..end].eq_ignore_ascii_case(alt)
                    && s[end..].chars().next().is_none_or(|c| !is_word_char(c))
            });
            if let Some(alt) = hit {
                out.push_str(with);
                i += alt.len();
                replaced = true;
                continue;
            }
        }
        let c = s[i..].chars().next().unwrap_or_default();
        out.push(c);
        i += c.len_utf8();
    }
    out
}

/// `getShortcutKey(shortcut)` (`shortcut.ts:5-19`): the modifier names
/// localized, Ctrl/Cmd by platform, Alt as Option on a Mac.
pub fn get_shortcut_key(shortcut: &str, is_darwin: bool, labels: &KeyLabels<'_>) -> String {
    let s = replace_word(
        shortcut,
        &["Option", "Opt", "Alt"],
        if is_darwin { labels.option } else { labels.alt },
        false,
    );
    let s = replace_word(&s, &["Shift"], labels.shift, false);
    let s = replace_word(&s, &["Enter", "Return"], labels.enter, false);
    let s = replace_word(
        &s,
        &["Ctrl", "Cmd", "Command", "CtrlOrCmd"],
        if is_darwin { labels.cmd } else { labels.ctrl },
        true,
    );
    let s = replace_word(&s, &["Escape", "Esc"], labels.escape, false);
    let s = replace_word(&s, &["Spacebar", "Space"], labels.spacebar, false);
    replace_word(&s, &["Delete", "Del"], labels.delete, false)
}

/// The `ShortcutName`s (`shortcuts.ts:10-52`): action names with a menu
/// shortcut, and five that are not actions (`saveScene`, `imageExport`,
/// `commandPalette`, `searchMenu`, `toolLock`).
pub const SHORTCUT_NAMES: [&str; 43] = [
    "toggleTheme",
    "saveScene",
    "loadScene",
    "clearCanvas",
    "imageExport",
    "commandPalette",
    "cut",
    "copy",
    "paste",
    "copyStyles",
    "pasteStyles",
    "selectAll",
    "deleteSelectedElements",
    "duplicateSelection",
    "sendBackward",
    "bringForward",
    "sendToBack",
    "bringToFront",
    "copyAsPng",
    "group",
    "ungroup",
    "gridMode",
    "zenMode",
    "objectsSnapMode",
    "stats",
    "addToLibrary",
    "flipHorizontal",
    "flipVertical",
    "viewMode",
    "hyperlink",
    "toggleElementLock",
    "resetZoom",
    "zoomOut",
    "zoomIn",
    "zoomToFitSelection",
    "zoomToFit",
    "zoomToFitSelectionInViewport",
    "saveFileToDisk",
    "saveToActiveFile",
    "toggleShortcuts",
    "searchMenu",
    "wrapSelectionInFrame",
    "toolLock",
];

/// `shortcutMap[name]` before localization (`shortcuts.ts:58-110`); `None`
/// for names outside the map.
fn raw_shortcuts(name: &str, is_darwin: bool) -> Option<Vec<&'static str>> {
    let one = |s: &'static str| Some(vec![s]);
    match name {
        "toggleTheme" => one("Shift+Alt+D"),
        "saveScene" => one("CtrlOrCmd+S"),
        "loadScene" => one("CtrlOrCmd+O"),
        "clearCanvas" => one("CtrlOrCmd+Delete"),
        "imageExport" => one("CtrlOrCmd+Shift+E"),
        "commandPalette" => Some(vec!["CtrlOrCmd+/", "CtrlOrCmd+Shift+P"]),
        "cut" => one("CtrlOrCmd+X"),
        "copy" => one("CtrlOrCmd+C"),
        "paste" => one("CtrlOrCmd+V"),
        "copyStyles" => one("CtrlOrCmd+Alt+C"),
        "pasteStyles" => one("CtrlOrCmd+Alt+V"),
        "selectAll" => one("CtrlOrCmd+A"),
        "deleteSelectedElements" => one("Delete"),
        // The second is `Alt+${t("helpDialog.drag")}`; see
        // `get_shortcut_from_shortcut_name`.
        "duplicateSelection" => Some(vec!["CtrlOrCmd+D", "Alt+"]),
        "sendBackward" => one("CtrlOrCmd+["),
        "bringForward" => one("CtrlOrCmd+]"),
        "sendToBack" if is_darwin => one("CtrlOrCmd+Alt+["),
        "sendToBack" => one("CtrlOrCmd+Shift+["),
        "bringToFront" if is_darwin => one("CtrlOrCmd+Alt+]"),
        "bringToFront" => one("CtrlOrCmd+Shift+]"),
        "copyAsPng" => one("Shift+Alt+C"),
        "group" => one("CtrlOrCmd+G"),
        "ungroup" => one("CtrlOrCmd+Shift+G"),
        "gridMode" => one("CtrlOrCmd+'"),
        "zenMode" => one("Alt+Z"),
        "objectsSnapMode" => one("Alt+S"),
        "stats" => one("Alt+/"),
        "addToLibrary" | "wrapSelectionInFrame" => Some(Vec::new()),
        "flipHorizontal" => one("Shift+H"),
        "flipVertical" => one("Shift+V"),
        "viewMode" => one("Alt+R"),
        "hyperlink" => one("CtrlOrCmd+K"),
        "toggleElementLock" => one("CtrlOrCmd+Shift+L"),
        "resetZoom" => one("CtrlOrCmd+0"),
        "zoomOut" => one("CtrlOrCmd+-"),
        "zoomIn" => one("CtrlOrCmd++"),
        "zoomToFitSelection" => one("Shift+3"),
        "zoomToFit" => one("Shift+1"),
        "zoomToFitSelectionInViewport" => one("Shift+2"),
        "saveFileToDisk" | "saveToActiveFile" => one("CtrlOrCmd+S"),
        "toggleShortcuts" => one("?"),
        "searchMenu" => one("CtrlOrCmd+F"),
        "toolLock" => one("Q"),
        _ => None,
    }
}

/// `getShortcutFromShortcutName(name, idx)` (`shortcuts.ts:112-118`): the
/// `idx`th shortcut, else the first, else `""`. Any name is accepted, as
/// the context menu and palette pass action names that may be outside the
/// map.
pub fn get_shortcut_from_shortcut_name(
    name: &str,
    idx: usize,
    is_darwin: bool,
    labels: &KeyLabels<'_>,
) -> String {
    let Some(shortcuts) = raw_shortcuts(name, is_darwin) else {
        return String::new();
    };
    let Some(raw) = shortcuts.get(idx).or(shortcuts.first()) else {
        return String::new();
    };
    if name == "duplicateSelection" && *raw == "Alt+" {
        return get_shortcut_key(&format!("Alt+{}", labels.drag), is_darwin, labels);
    }
    get_shortcut_key(raw, is_darwin, labels)
}
