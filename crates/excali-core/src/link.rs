//! Link sanitizing (`packages/common/src/url.ts:5-37`): [`normalize_link`]
//! and [`to_valid_url`], which upstream applies to element links, embeds and
//! the URL of a library imported with `#addLibrary`
//! (`packages/excalidraw/data/library.ts:729`), with the
//! `@braintree/sanitize-url` 6.0.2 function they call ([`sanitize_url`],
//! the version upstream's `yarn.lock` resolves for `@excalidraw/common`) and
//! `escapeDoubleQuotes` (`packages/common/src/utils.ts:1134-1136`).
//!
//! Upstream's strings are UTF-16 and `sanitizeUrl` decodes `&#NNN;`
//! entities with `String.fromCharCode`, which can produce half of a
//! surrogate pair; the port works on UTF-16 code units the same way and only
//! converts back at the end, where a lone surrogate (which a Rust string
//! cannot hold) becomes U+FFFD. That is also what the URL parser does with
//! one, so the URL a caller goes on to use is the same.

use crate::js;
use crate::whatwg_url;

/// What [`sanitize_url`] and [`to_valid_url`] return for a link they reject.
pub const BLANK_URL: &str = "about:blank";

/// `ELEMENT_LINK_KEY` (`packages/common/src/constants.ts:595`): the query
/// parameter naming the element a link points at.
pub const ELEMENT_LINK_KEY: &str = "element";

/// `isElementLink(url)` (`packages/element/src/elementLink.ts:82-92`):
/// whether `url` is a link to an element of a scene on this page, that is,
/// a URL `new URL` parses whose query has an [`ELEMENT_LINK_KEY`] parameter
/// and whose `host` is `location_host` (upstream's `window.location.host`,
/// such as `excalidraw.com` or `localhost:3000`).
pub fn is_element_link(url: &str, location_host: &str) -> bool {
    whatwg_url::parse(url)
        .is_some_and(|u| u.search_params_has(ELEMENT_LINK_KEY) && u.host() == location_host)
}

/// `escapeDoubleQuotes` (`packages/common/src/utils.ts:1134-1136`): every
/// `"` as `&quot;`.
pub fn escape_double_quotes(s: &str) -> String {
    s.replace('"', "&quot;")
}

/// `normalizeLink(link)` (`packages/common/src/url.ts:5-11`): trimmed, and
/// unless that leaves it empty, [`escape_double_quotes`] then
/// [`sanitize_url`].
pub fn normalize_link(link: &str) -> String {
    let units: Vec<u16> = link.encode_utf16().collect();
    let trimmed = String::from_utf16_lossy(js::trim_units(&units));
    if trimmed.is_empty() {
        return trimmed;
    }
    sanitize_url(&escape_double_quotes(&trimmed))
}

/// `isLocalLink(link)` (`packages/common/src/url.ts:13-15`): whether
/// `link` contains `origin` (upstream's `location.origin`) or starts with
/// `/`, the links the hyperlink popup opens in the same tab.
pub fn is_local_link(link: &str, origin: &str) -> bool {
    link.contains(origin) || link.starts_with('/')
}

/// `toValidURL(link)` (`packages/common/src/url.ts:21-37`): the
/// [`normalize_link`] form of `link`, made absolute on `origin` (upstream's
/// `location.origin`, e.g. `https://excalidraw.com`) when it starts with
/// `/`, and [`BLANK_URL`] when it is then not a URL `new URL` parses.
pub fn to_valid_url(link: &str, origin: &str) -> String {
    let link = normalize_link(link);
    if link.starts_with('/') {
        return format!("{origin}{link}");
    }
    if whatwg_url::parse(&link).is_none() {
        return BLANK_URL.to_owned();
    }
    link
}

/// `sanitizeUrl(url)` from `@braintree/sanitize-url` 6.0.2
/// (`dist/index.js`): HTML character references `&#NNN;` decoded,
/// `&newline;` and `&tab;` removed, control and zero-width characters
/// removed, trimmed; then [`BLANK_URL`] when that is empty or its scheme is
/// `javascript`, `data` or `vbscript` (after any non-word characters, any
/// case, the `:` possibly written `&colon;`). Relative links (`.` or `/`
/// first) and anything else come back as they are.
pub fn sanitize_url(url: &str) -> String {
    let units: Vec<u16> = url.encode_utf16().collect();
    let units = decode_html_characters(&units);
    let units = remove_html_ctrl_entities(&units);
    let units: Vec<u16> = units
        .into_iter()
        .filter(|&u| !is_ctrl_character(u))
        .collect();
    let sanitized = js::trim_units(&units);
    if sanitized.is_empty() {
        return BLANK_URL.to_owned();
    }
    // isRelativeUrlWithoutProtocol: url[0] is "." or "/"
    if sanitized[0] == u16::from(b'.') || sanitized[0] == u16::from(b'/') {
        return String::from_utf16_lossy(sanitized);
    }
    if let Some(scheme) = url_scheme(sanitized) {
        if is_invalid_protocol(scheme) {
            return BLANK_URL.to_owned();
        }
    }
    String::from_utf16_lossy(sanitized)
}

fn is_word_unit(u: u16) -> bool {
    u < 0x80 && (u as u8).is_ascii_alphanumeric() || u == u16::from(b'_')
}

/// `str.replace(/&#(\w+)(^\w|;)?/g, (match, dec) => String.fromCharCode(dec))`.
/// The `^` of the optional group can only match at the start of the input,
/// never after `&#` and a word character, so the group is an optional `;`.
fn decode_html_characters(units: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(units.len());
    let mut i = 0;
    while i < units.len() {
        if units[i] == u16::from(b'&') && units.get(i + 1) == Some(&u16::from(b'#')) {
            let digits = units[i + 2..]
                .iter()
                .take_while(|&&u| is_word_unit(u))
                .count();
            if digits > 0 {
                let end = i + 2 + digits;
                // ASCII only, so one byte per unit.
                let dec: String = units[i + 2..end].iter().map(|&u| u as u8 as char).collect();
                out.push(js::to_uint16(js::string_to_number(&dec)));
                i = if units.get(end) == Some(&u16::from(b';')) {
                    end + 1
                } else {
                    end
                };
                continue;
            }
        }
        out.push(units[i]);
        i += 1;
    }
    out
}

/// `str.replace(/&(newline|tab);/gi, "")`: ASCII case-insensitive (without
/// the `u` flag, `i` never maps a non-ASCII character to an ASCII one).
fn remove_html_ctrl_entities(units: &[u16]) -> Vec<u16> {
    let matches_at = |i: usize| {
        ["&newline;", "&tab;"].into_iter().find_map(|entity| {
            let len = entity.len();
            let candidate = units.get(i..i + len)?;
            candidate
                .iter()
                .zip(entity.bytes())
                .all(|(&u, b)| u < 0x80 && (u as u8).eq_ignore_ascii_case(&b))
                .then_some(len)
        })
    };
    let mut out = Vec::with_capacity(units.len());
    let mut i = 0;
    while i < units.len() {
        if let Some(len) = matches_at(i) {
            i += len;
        } else {
            out.push(units[i]);
            i += 1;
        }
    }
    out
}

/// `/[\u0000-\u001F\u007F-\u009F\u2000-\u200D\uFEFF]/`
fn is_ctrl_character(u: u16) -> bool {
    matches!(u, 0x0000..=0x001F | 0x007F..=0x009F | 0x2000..=0x200D | 0xFEFF)
}

fn is_line_terminator(u: u16) -> bool {
    matches!(u, 0x000A | 0x000D | 0x2028 | 0x2029)
}

/// The first match of `/^.+(:|&colon;)/gim`: on the first line (with `m`,
/// `^` is any line start, and `.` stops at line terminators) holding a `:`
/// or `&colon;` after its first character, the line up to and including the
/// last one (`.+` is greedy).
fn url_scheme(units: &[u16]) -> Option<&[u16]> {
    let colon = u16::from(b':');
    let entity: Vec<u16> = "&colon;".encode_utf16().collect();
    let mut start = 0;
    loop {
        let end = units[start..]
            .iter()
            .position(|&u| is_line_terminator(u))
            .map_or(units.len(), |i| start + i);
        let line = &units[start..end];
        let found = (1..line.len()).rev().find_map(|k| {
            if line[k] == colon {
                Some(k + 1)
            } else if line[k..].starts_with(&entity) {
                Some(k + entity.len())
            } else {
                None
            }
        });
        if let Some(len) = found {
            return Some(&line[..len]);
        }
        if end == units.len() {
            return None;
        }
        start = end + 1;
    }
}

/// `/^([^\w]*)(javascript|data|vbscript)/im.test(scheme)`; the scheme is one
/// line, so `^` is its start.
fn is_invalid_protocol(scheme: &[u16]) -> bool {
    let rest = &scheme[scheme.iter().take_while(|&&u| !is_word_unit(u)).count()..];
    ["javascript", "data", "vbscript"].into_iter().any(|name| {
        rest.len() >= name.len()
            && rest
                .iter()
                .zip(name.bytes())
                .all(|(&u, b)| u < 0x80 && (u as u8).eq_ignore_ascii_case(&b))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn scheme_is_the_first_line_with_a_colon_up_to_its_last() {
        let scheme = |s: &str| url_scheme(&units(s)).map(String::from_utf16_lossy);
        assert_eq!(scheme("https://a.b/c:d").as_deref(), Some("https://a.b/c:"));
        assert_eq!(scheme("a&colon;b").as_deref(), Some("a&colon;"));
        assert_eq!(scheme(":"), None);
        assert_eq!(scheme("::").as_deref(), Some("::"));
        assert_eq!(scheme("&colon;"), None);
        assert_eq!(scheme("x\u{2029}data:y").as_deref(), Some("data:"));
        assert_eq!(scheme("x\u{2028}y"), None);
    }

    #[test]
    fn lone_surrogates_become_replacement_characters() {
        assert_eq!(sanitize_url("a&#55357;b"), "a\u{fffd}b");
        assert_eq!(sanitize_url("&#55357;&#56832;"), "\u{1f600}");
    }
}
