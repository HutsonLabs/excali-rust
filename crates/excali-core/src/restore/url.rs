//! `normalizeLink` (`packages/common/src/url.ts:5-11`) and the
//! `sanitizeUrl` it calls, from `@braintree/sanitize-url` 6.0.2, the version
//! upstream pins (`packages/excalidraw/package.json:81`;
//! `dist/index.js` of that package).
//!
//! Both work on UTF-16 code units, as the JS regexes do, so an entity that
//! decodes to half a surrogate pair (`&#55357;`) gives the same string JS
//! gives. Strings come and go in the crate's sentinel form
//! ([`crate::json::to_utf16`], [`crate::json::from_utf16`]).

use crate::js;
use crate::json;

const ABOUT_BLANK: &str = "about:blank";

/// `normalizeLink(link)` for a sentinel-form string: trim; empty stays
/// empty; otherwise `sanitizeUrl(escapeDoubleQuotes(link))`
/// (`escapeDoubleQuotes`, `packages/common/src/utils.ts:1134-1136`).
pub(crate) fn normalize_link_encoded(link: &str) -> String {
    let units = json::to_utf16(link);
    let trimmed = js::trim_units(&units);
    if trimmed.is_empty() {
        return String::new();
    }
    let mut escaped = Vec::with_capacity(trimmed.len());
    for &u in trimmed {
        if u == u16::from(b'"') {
            escaped.extend("&quot;".encode_utf16());
        } else {
            escaped.push(u);
        }
    }
    json::from_utf16(&sanitize_units(&escaped))
}

/// `sanitizeUrl(url)` for a sentinel-form string.
pub(crate) fn sanitize_url_encoded(url: &str) -> String {
    json::from_utf16(&sanitize_units(&json::to_utf16(url)))
}

/// `sanitizeUrl`:
///
/// ```js
/// var sanitizedUrl = decodeHtmlCharacters(url || "")
///     .replace(/&(newline|tab);/gi, "")
///     .replace(/[\u0000-\u001F\u007F-\u009F\u2000-\u200D\uFEFF]/gim, "")
///     .trim();
/// if (!sanitizedUrl) return "about:blank";
/// if (isRelativeUrlWithoutProtocol(sanitizedUrl)) return sanitizedUrl;
/// var urlSchemeParseResults = sanitizedUrl.match(/^.+(:|&colon;)/gim);
/// if (!urlSchemeParseResults) return sanitizedUrl;
/// if (/^([^\w]*)(javascript|data|vbscript)/im.test(urlSchemeParseResults[0]))
///     return "about:blank";
/// return sanitizedUrl;
/// ```
fn sanitize_units(url: &[u16]) -> Vec<u16> {
    let decoded = decode_html_characters(url);
    let without_entities = remove_ctrl_entities(&decoded);
    let without_ctrl: Vec<u16> = without_entities
        .into_iter()
        .filter(|&u| !is_ctrl_character(u))
        .collect();
    let sanitized = js::trim_units(&without_ctrl);
    if sanitized.is_empty() {
        return ABOUT_BLANK.encode_utf16().collect();
    }
    // isRelativeUrlWithoutProtocol: first character "." or "/".
    if sanitized[0] == u16::from(b'.') || sanitized[0] == u16::from(b'/') {
        return sanitized.to_vec();
    }
    match url_scheme(sanitized) {
        Some(scheme) if is_invalid_protocol(scheme) => ABOUT_BLANK.encode_utf16().collect(),
        _ => sanitized.to_vec(),
    }
}

/// `\w` without the `u` flag: `[A-Za-z0-9_]`.
fn is_word(u: u16) -> bool {
    u8::try_from(u).is_ok_and(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// `str.replace(/&#(\w+)(^\w|;)?/g, (match, dec) => String.fromCharCode(dec))`.
///
/// `(^\w|;)?`: without the `m` flag `^` only matches at the start of the
/// input, which never follows `&#\w+`, so this is an optional `;`.
/// `String.fromCharCode(dec)` is `ToUint16(Number(dec))`: `"106"` is `j`,
/// `"0x6A"` too, `"x6A"` is NaN and so U+0000.
fn decode_html_characters(units: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(units.len());
    let mut i = 0;
    while i < units.len() {
        let starts = units[i] == u16::from(b'&')
            && units.get(i + 1) == Some(&u16::from(b'#'))
            && units.get(i + 2).is_some_and(|&u| is_word(u));
        if !starts {
            out.push(units[i]);
            i += 1;
            continue;
        }
        let digits_start = i + 2;
        let mut end = digits_start;
        while end < units.len() && is_word(units[end]) {
            end += 1;
        }
        // Word characters are ASCII.
        let dec: String = units[digits_start..end]
            .iter()
            .map(|&u| char::from(u as u8))
            .collect();
        out.push(js::to_uint16(js::string_to_number(&dec)));
        if units.get(end) == Some(&u16::from(b';')) {
            end += 1;
        }
        i = end;
    }
    out
}

/// True when `units[at..]` starts with the ASCII `literal`, ignoring ASCII
/// case (a non-unicode `i` regex never folds non-ASCII onto ASCII).
fn starts_with_ignore_case(units: &[u16], at: usize, literal: &str) -> bool {
    let lit = literal.as_bytes();
    units.len() >= at + lit.len()
        && units[at..at + lit.len()]
            .iter()
            .zip(lit)
            .all(|(&u, &b)| u8::try_from(u).is_ok_and(|c| c.eq_ignore_ascii_case(&b)))
}

/// `.replace(/&(newline|tab);/gi, "")`: one left-to-right pass, removed
/// text is not rescanned.
fn remove_ctrl_entities(units: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(units.len());
    let mut i = 0;
    while i < units.len() {
        if starts_with_ignore_case(units, i, "&newline;") {
            i += "&newline;".len();
        } else if starts_with_ignore_case(units, i, "&tab;") {
            i += "&tab;".len();
        } else {
            out.push(units[i]);
            i += 1;
        }
    }
    out
}

/// `[\u0000-\u001F\u007F-\u009F\u2000-\u200D\uFEFF]` (the `i` flag adds
/// nothing: none of these has a case mapping).
fn is_ctrl_character(u: u16) -> bool {
    matches!(u, 0x0000..=0x001F | 0x007F..=0x009F | 0x2000..=0x200D | 0xFEFF)
}

/// `LineTerminator`: where `.` stops and, with the `m` flag, `^` matches.
fn is_line_terminator(u: u16) -> bool {
    matches!(u, 0x0A | 0x0D | 0x2028 | 0x2029)
}

/// The first match of `/^.+(:|&colon;)/gim`: at the first line start (the
/// input start or just after a line terminator) where one exists, the
/// longest run of at least one non-terminator followed by `:` or
/// `&colon;` (any case), as greedy `.+` backtracks to it.
fn url_scheme(units: &[u16]) -> Option<&[u16]> {
    let line_starts = std::iter::once(0).chain(
        units
            .iter()
            .enumerate()
            .filter(|(_, &u)| is_line_terminator(u))
            .map(|(i, _)| i + 1),
    );
    for start in line_starts {
        let line_end = units[start..]
            .iter()
            .position(|&u| is_line_terminator(u))
            .map_or(units.len(), |p| start + p);
        for at in (start + 1..line_end).rev() {
            if units[at] == u16::from(b':') {
                return Some(&units[start..=at]);
            }
            if starts_with_ignore_case(units, at, "&colon;") {
                return Some(&units[start..at + "&colon;".len()]);
            }
        }
    }
    None
}

/// `/^([^\w]*)(javascript|data|vbscript)/im.test(scheme)`. The scheme has
/// no line terminator, so `^` is its start; `[^\w]*` takes every leading
/// non-word unit (the keyword starts with a word character).
fn is_invalid_protocol(scheme: &[u16]) -> bool {
    let at = scheme
        .iter()
        .position(|&u| is_word(u))
        .unwrap_or(scheme.len());
    ["javascript", "data", "vbscript"]
        .iter()
        .any(|p| starts_with_ignore_case(scheme, at, p))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn scheme_is_the_longest_prefix_up_to_a_colon_on_the_first_line_with_one() {
        let s = |x: &str| url_scheme(&units(x)).map(String::from_utf16_lossy);
        assert_eq!(s("a:b:c").as_deref(), Some("a:b:"));
        assert_eq!(s("a&colon;b").as_deref(), Some("a&colon;"));
        assert_eq!(s("a:b&COLON;c").as_deref(), Some("a:b&COLON;"));
        assert_eq!(s(":x").as_deref(), None);
        assert_eq!(s("::").as_deref(), Some("::"));
        assert_eq!(s("ab").as_deref(), None);
        assert_eq!(s("ab\u{2028}c:d").as_deref(), Some("c:"));
        assert_eq!(s("a:b\u{2029}c:d").as_deref(), Some("a:"));
    }

    #[test]
    fn html_entities_decode_through_to_uint16() {
        let d = |x: &str| String::from_utf16_lossy(&decode_html_characters(&units(x)));
        assert_eq!(d("&#106;&#97"), "ja");
        assert_eq!(d("&#0x6A;"), "j");
        assert_eq!(d("&#x6A;"), "\0");
        assert_eq!(d("&&#65;&#;&#"), "&A&#;&#");
        assert_eq!(d("&#65601;"), "A");
    }

    #[test]
    fn invalid_protocols() {
        assert!(is_invalid_protocol(&units(" %javascript:")));
        assert!(is_invalid_protocol(&units("DATA:")));
        assert!(!is_invalid_protocol(&units("xjavascript:")));
        assert!(!is_invalid_protocol(&units("_javascript:")));
        assert!(!is_invalid_protocol(&units("https:")));
    }
}
