//! The locale loader: upstream's `languages`, `setLanguage` and `t`.
//!
//! Upstream counterpart: `packages/excalidraw/i18n.ts` (the language list
//! at lines 19-75, `setLanguage` at 92-109, `t` at 127-160) and the locale
//! files it imports (`packages/excalidraw/locales/*.json`), vendored
//! under `assets/locales` by `tools/goldens/i18n.mjs` (upstream's bytes,
//! with the invisible code points the authorship gate refuses written as
//! `\uXXXX` escapes, so each file parses to upstream's values).
//!
//! The crate embeds `en.json` (the fallback every lookup ends in) and
//! `percentages.json` (which languages are listed); a host serves the other
//! locale files at [`locale_path`] and hands their text to
//! [`I18n::set_language`] ([`fetch_locale`] fetches one), as upstream's
//! dynamic `import()` loads them.
//! Upstream's dev-only test languages (`__test__`) are not ported: the
//! production build does not list them.

use std::sync::OnceLock;

use serde_json::Value;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::Document;

/// `COMPLETION_THRESHOLD` (`i18n.ts:9`): a language is listed when its
/// percentage in `percentages.json` is at least this.
pub const COMPLETION_THRESHOLD: u32 = 85;

/// `locales/en.json`, the fallback language data.
pub const EN_JSON: &str = include_str!("../assets/locales/en.json");

/// `locales/percentages.json`, the completion of each locale file.
pub const PERCENTAGES_JSON: &str = include_str!("../assets/locales/percentages.json");

/// A language (`interface Language`, `i18n.ts:11-15`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Language {
    pub code: String,
    pub label: String,
    pub rtl: bool,
}

/// A language of upstream's static list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LanguageEntry {
    pub code: &'static str,
    pub label: &'static str,
    pub rtl: bool,
}

impl LanguageEntry {
    const fn new(code: &'static str, label: &'static str) -> Self {
        Self {
            code,
            label,
            rtl: false,
        }
    }

    const fn rtl(code: &'static str, label: &'static str) -> Self {
        Self {
            code,
            label,
            rtl: true,
        }
    }

    pub fn to_language(&self) -> Language {
        Language {
            code: self.code.to_owned(),
            label: self.label.to_owned(),
            rtl: self.rtl,
        }
    }
}

/// `defaultLang` (`i18n.ts:19`).
pub const DEFAULT_LANG: LanguageEntry = LanguageEntry::new("en", "English");

/// The languages after `defaultLang` that `i18n.ts:24-67` names, in its
/// order, before the threshold filter and the sort.
pub const CANDIDATES: &[LanguageEntry] = &[
    LanguageEntry::rtl("ar-SA", "العربية"),
    LanguageEntry::new("bg-BG", "Български"),
    LanguageEntry::new("ca-ES", "Català"),
    LanguageEntry::new("cs-CZ", "Česky"),
    LanguageEntry::new("de-DE", "Deutsch"),
    LanguageEntry::new("el-GR", "Ελληνικά"),
    LanguageEntry::new("es-ES", "Español"),
    LanguageEntry::new("eu-ES", "Euskara"),
    LanguageEntry::rtl("fa-IR", "فارسی"),
    LanguageEntry::new("fi-FI", "Suomi"),
    LanguageEntry::new("fr-FR", "Français"),
    LanguageEntry::new("gl-ES", "Galego"),
    LanguageEntry::rtl("he-IL", "עברית"),
    LanguageEntry::new("hi-IN", "हिन्दी"),
    LanguageEntry::new("hu-HU", "Magyar"),
    LanguageEntry::new("id-ID", "Bahasa Indonesia"),
    LanguageEntry::new("it-IT", "Italiano"),
    LanguageEntry::new("ja-JP", "日本語"),
    LanguageEntry::new("kab-KAB", "Taqbaylit"),
    LanguageEntry::new("kk-KZ", "Қазақ тілі"),
    LanguageEntry::new("ko-KR", "한국어"),
    LanguageEntry::new("ku-TR", "Kurdî"),
    LanguageEntry::new("lt-LT", "Lietuvių"),
    LanguageEntry::new("lv-LV", "Latviešu"),
    LanguageEntry::new("my-MM", "Burmese"),
    LanguageEntry::new("nb-NO", "Norsk bokmål"),
    LanguageEntry::new("nl-NL", "Nederlands"),
    LanguageEntry::new("nn-NO", "Norsk nynorsk"),
    LanguageEntry::new("oc-FR", "Occitan"),
    LanguageEntry::new("pa-IN", "ਪੰਜਾਬੀ"),
    LanguageEntry::new("pl-PL", "Polski"),
    LanguageEntry::new("pt-BR", "Português Brasileiro"),
    LanguageEntry::new("pt-PT", "Português"),
    LanguageEntry::new("ro-RO", "Română"),
    LanguageEntry::new("ru-RU", "Русский"),
    LanguageEntry::new("sk-SK", "Slovenčina"),
    LanguageEntry::new("sv-SE", "Svenska"),
    LanguageEntry::new("sl-SI", "Slovenščina"),
    LanguageEntry::new("tr-TR", "Türkçe"),
    LanguageEntry::new("uk-UA", "Українська"),
    LanguageEntry::new("zh-CN", "简体中文"),
    LanguageEntry::new("zh-TW", "繁體中文"),
    LanguageEntry::new("vi-VN", "Tiếng Việt"),
    LanguageEntry::new("mr-IN", "मराठी"),
];

/// `en.json`, parsed.
pub fn english() -> &'static Value {
    static EN: OnceLock<Value> = OnceLock::new();
    EN.get_or_init(|| serde_json::from_str(EN_JSON).expect("en.json"))
}

/// `percentages.json`, parsed.
pub fn percentages() -> &'static Value {
    static PERCENTAGES: OnceLock<Value> = OnceLock::new();
    PERCENTAGES.get_or_init(|| serde_json::from_str(PERCENTAGES_JSON).expect("percentages.json"))
}

/// Where a host serves the locale file of `code`, relative to the assets
/// root (`locales/<code>.json`, as `i18n.ts:101` imports it).
pub fn locale_path(code: &str) -> String {
    format!("locales/{code}.json")
}

/// `languages` (`i18n.ts:21-75`) for `percentages` and `threshold`:
/// `defaultLang`, then the candidates whose percentage is at least
/// `threshold` (a code without one is dropped, as `undefined >= n` is
/// false), sorted by label as JavaScript compares strings (UTF-16 code
/// units).
pub fn languages_with(percentages: &Value, threshold: u32) -> Vec<Language> {
    let mut listed: Vec<&LanguageEntry> = CANDIDATES
        .iter()
        .filter(|l| {
            percentages
                .get(l.code)
                .and_then(Value::as_f64)
                .is_some_and(|p| p >= f64::from(threshold))
        })
        .collect();
    // `left.label > right.label ? 1 : -1` (i18n.ts:74); the labels are
    // distinct, so this is the same order as a total one
    listed.sort_by(|a, b| a.label.encode_utf16().cmp(b.label.encode_utf16()));
    std::iter::once(DEFAULT_LANG.to_language())
        .chain(listed.into_iter().map(LanguageEntry::to_language))
        .collect()
}

/// `languages` (`i18n.ts:21-75`) with upstream's percentages and threshold.
pub fn languages() -> Vec<Language> {
    languages_with(percentages(), COMPLETION_THRESHOLD)
}

/// The current language and its data (`currentLang`, `currentLangData`).
#[derive(Clone, Debug)]
pub struct I18n {
    lang: Language,
    /// `None` when the data is English's (no file, or it failed to load).
    data: Option<Value>,
}

impl Default for I18n {
    fn default() -> Self {
        Self::new()
    }
}

impl I18n {
    /// English, as upstream starts (`i18n.ts:89-90`).
    pub fn new() -> Self {
        Self {
            lang: DEFAULT_LANG.to_language(),
            data: None,
        }
    }

    /// `setLanguage` (`i18n.ts:92-109`) with the locale file's text as the
    /// host loaded it: `None` or text that is not JSON falls back to
    /// English, as upstream does when the import fails.
    pub fn set_language(&mut self, lang: Language, data: Option<&str>) {
        self.lang = lang;
        self.data = data.and_then(|text| serde_json::from_str(text).ok());
    }

    /// `getLanguage` (`i18n.ts:111`).
    pub fn language(&self) -> &Language {
        &self.lang
    }

    /// The document's `dir` for the language (`i18n.ts:94`).
    pub fn dir(&self) -> &'static str {
        if self.lang.rtl {
            "rtl"
        } else {
            "ltr"
        }
    }

    /// The attributes `setLanguage` writes on `document.documentElement`
    /// (`i18n.ts:94-95`).
    pub fn document_attributes(&self) -> [(&'static str, &str); 2] {
        [("dir", self.dir()), ("lang", &self.lang.code)]
    }

    /// Writes [`document_attributes`](Self::document_attributes) on
    /// `document`'s root element.
    pub fn apply_to_document(&self, document: &Document) {
        if let Some(root) = document.document_element() {
            for (name, value) in self.document_attributes() {
                let _ = root.set_attribute(name, value);
            }
        }
    }

    /// `t` (`i18n.ts:127-160`) returning `None` where upstream finds no
    /// translation: the language's string if not empty, else English's if
    /// not empty, else `fallback`; then each replacement fills the first
    /// `{{key}}` in order, with `String.prototype.replace`'s `$`
    /// substitutions.
    pub fn try_t(
        &self,
        path: &str,
        replacement: &[(&str, &str)],
        fallback: Option<&str>,
    ) -> Option<String> {
        let parts: Vec<&str> = path.split('.').collect();
        let data = self.data.as_ref().unwrap_or_else(|| english());
        let mut translation = find_parts(data, &parts)
            .filter(|s| !s.is_empty())
            .or_else(|| find_parts(english(), &parts).filter(|s| !s.is_empty()))
            .or(fallback)?
            .to_owned();
        for (key, value) in replacement {
            translation = replace_first(&translation, &format!("{{{{{key}}}}}"), value);
        }
        Some(translation)
    }

    /// `t` with replacements and a fallback; a missing translation is
    /// `""`, as upstream's production build returns.
    pub fn t_with(
        &self,
        path: &str,
        replacement: &[(&str, &str)],
        fallback: Option<&str>,
    ) -> String {
        self.try_t(path, replacement, fallback).unwrap_or_default()
    }

    /// `t(path)`.
    pub fn t(&self, path: &str) -> String {
        self.t_with(path, &[], None)
    }
}

/// `findPartsForData` (`i18n.ts:113-125`).
fn find_parts<'a>(mut data: &'a Value, parts: &[&str]) -> Option<&'a str> {
    for part in parts {
        data = data.as_object()?.get(*part)?;
    }
    data.as_str()
}

/// `source.replace(pattern, replacement)` with a string pattern: the first
/// match only, `$$`, `$&`, `` $` `` and `$'` expanded, any other `$`
/// literal (a string pattern has no captures).
fn replace_first(source: &str, pattern: &str, replacement: &str) -> String {
    let Some(at) = source.find(pattern) else {
        return source.to_owned();
    };
    let (before, after) = (&source[..at], &source[at + pattern.len()..]);
    let mut out = String::with_capacity(source.len() + replacement.len());
    out.push_str(before);
    let mut chars = replacement.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('$') => out.push('$'),
            Some('&') => out.push_str(pattern),
            Some('`') => out.push_str(before),
            Some('\'') => out.push_str(after),
            _ => {
                out.push('$');
                continue;
            }
        }
        chars.next();
    }
    out.push_str(after);
    out
}

/// Fetches the text of the locale file of `code` under `base` (the assets
/// root a host serves `assets/` at), as `i18n.ts:101` imports it; `None`
/// when the request fails or is not OK, for [`I18n::set_language`] to fall
/// back to English.
pub async fn fetch_locale(window: &web_sys::Window, base: &str, code: &str) -> Option<String> {
    let url = format!("{}/{}", base.trim_end_matches('/'), locale_path(code));
    let response = JsFuture::from(window.fetch_with_str(&url)).await.ok()?;
    let response: web_sys::Response = response.dyn_into().ok()?;
    if !response.ok() {
        return None;
    }
    JsFuture::from(response.text().ok()?)
        .await
        .ok()?
        .as_string()
}
