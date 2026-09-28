//! Importing a library from a URL (research page
//! `site/content/research/data-model.md`, section 4, "Import from a URL").
//!
//! libraries.excalidraw.com sends the user back to the editor with
//! `#addLibrary=<url>&token=<editor id>`; older links put `addLibrary` in the
//! query string. Upstream's `useHandleLibrary`
//! (`packages/excalidraw/data/library.ts:674-800`) then:
//!
//! 1. reads the tokens with `parseLibraryTokensFromUrl` (`library.ts:530-543`,
//!    keys from `packages/common/src/constants.ts:376-382`):
//!    [`parse_library_tokens`];
//! 2. `decodeURIComponent`s the URL, makes it a valid one with `toValidURL`
//!    and checks it with `validateLibraryUrl` against the allow-list
//!    (`library.ts:54-58, 497-528, 726-731`): [`resolve_library_url`];
//! 3. fetches it and merges the library in, asking first unless the token is
//!    the editor's own id (`library.ts:747-763`):
//!    [`LibraryUrlTokens::should_prompt`], with the parse and merge of
//!    [`crate::library`];
//! 4. removes `addLibrary` from the address (`library.ts:765-776`):
//!    [`library_url_after_import`].
//!
//! The allow-list check is upstream's to the letter. An entry is parsed as
//! `https://<entry>` and its hostname and pathname become regular
//! expressions, `(^|\.)<hostname>$` for the library URL's hostname and
//! `^<pathname>(/+|$)` for its pathname (trailing slashes removed from the
//! entry's): a suffix match on the host at a subdomain boundary and a prefix
//! match on the path at a segment boundary. The entries are interpolated
//! unescaped, so their dots match any character (`excalidraw-com` passes)
//! and other regular expression syntax in a caller's entry takes effect or
//! throws; `crate::js_regexp` gives the same answers and V8's errors.
//! URLs are parsed by the `url` crate, an implementation of the WHATWG URL
//! Standard that `new URL` follows.

use std::fmt;

use url::{form_urlencoded, Url};

use crate::js_regexp::RegExp;
use crate::link::to_valid_url;

/// `ALLOWED_LIBRARY_URLS` (`packages/excalidraw/data/library.ts:54-58`):
/// excalidraw.com (libraries.excalidraw.com serves the catalogue) and the
/// catalogue's GitHub repository, for installing from its pull requests.
pub const ALLOWED_LIBRARY_URLS: [&str; 2] = [
    "excalidraw.com",
    "raw.githubusercontent.com/excalidraw/excalidraw-libraries",
];

/// `URL_HASH_KEYS.addLibrary` (`packages/common/src/constants.ts:380-382`).
pub const URL_HASH_KEY_ADD_LIBRARY: &str = "addLibrary";

/// `URL_QUERY_KEYS.addLibrary` (`packages/common/src/constants.ts:376-378`),
/// the legacy form.
pub const URL_QUERY_KEY_ADD_LIBRARY: &str = "addLibrary";

/// The hash key holding the id of the editor that asked for the library.
pub const URL_HASH_KEY_TOKEN: &str = "token";

/// What decides whether a library URL may be fetched: upstream's
/// `validateLibraryUrl` `validator` argument, a list of allowed URL prefixes
/// or a function (the editor's `validateLibraryUrl` option,
/// `library.ts:684`).
#[derive(Clone, Copy)]
pub enum LibraryUrlValidator<'a> {
    /// Entries of the form `host[/path]`, optionally after `http://` or
    /// `https://`.
    AllowList(&'a [&'a str]),
    /// Allows the URL when it returns true; nothing else is checked.
    Predicate(&'a dyn Fn(&str) -> bool),
}

impl Default for LibraryUrlValidator<'_> {
    /// [`ALLOWED_LIBRARY_URLS`], upstream's default argument.
    fn default() -> Self {
        Self::AllowList(&ALLOWED_LIBRARY_URLS)
    }
}

impl fmt::Debug for LibraryUrlValidator<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AllowList(list) => f.debug_tuple("AllowList").field(list).finish(),
            Self::Predicate(_) => f.write_str("Predicate(..)"),
        }
    }
}

/// Why a library URL was not accepted: what upstream throws, with its
/// message as the [`Display`](fmt::Display) form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryUrlError {
    /// `Error: Invalid or disallowed library URL: "<url>"`
    /// (`library.ts:527`).
    Disallowed { url: String },
    /// `TypeError: Invalid URL`: `new URL` of the library URL or of an
    /// allow-list entry failed.
    InvalidUrl,
    /// `SyntaxError`: an allow-list entry made an invalid regular
    /// expression; the message is V8's.
    InvalidRegExp { message: String },
    /// `URIError: URI malformed`: `decodeURIComponent` of the URL failed.
    UriMalformed,
}

impl LibraryUrlError {
    /// The constructor of what upstream throws: `Error`, `TypeError`,
    /// `SyntaxError` or `URIError`.
    pub fn js_error_type(&self) -> &'static str {
        match self {
            Self::Disallowed { .. } => "Error",
            Self::InvalidUrl => "TypeError",
            Self::InvalidRegExp { .. } => "SyntaxError",
            Self::UriMalformed => "URIError",
        }
    }
}

impl fmt::Display for LibraryUrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disallowed { url } => write!(f, "Invalid or disallowed library URL: \"{url}\""),
            Self::InvalidUrl => f.write_str("Invalid URL"),
            Self::InvalidRegExp { message } => f.write_str(message),
            Self::UriMalformed => f.write_str("URI malformed"),
        }
    }
}

impl std::error::Error for LibraryUrlError {}

/// `validateLibraryUrl(libraryUrl)` (`library.ts:497-528`) with the default
/// allow-list, [`ALLOWED_LIBRARY_URLS`].
pub fn validate_library_url(library_url: &str) -> Result<(), LibraryUrlError> {
    validate_library_url_with(library_url, &LibraryUrlValidator::default())
}

/// `validateLibraryUrl(libraryUrl, validator)` (`library.ts:497-528`).
///
/// With an allow-list, each entry in order: the entry is parsed as
/// `https://<entry>` (a leading `http://` or `https://` removed first), the
/// library URL is parsed, and the URL is allowed when its hostname matches
/// `(^|\.)<entry hostname>$` and then its pathname matches `^<entry
/// pathname without trailing slashes>(/+|$)`. A URL either parse rejects is
/// [`LibraryUrlError::InvalidUrl`], an entry that is not a valid pattern
/// [`LibraryUrlError::InvalidRegExp`] (the path pattern is only built when
/// the host matched), and both end the check. No entry allowing the URL, or
/// the predicate returning false, is [`LibraryUrlError::Disallowed`].
pub fn validate_library_url_with(
    library_url: &str,
    validator: &LibraryUrlValidator<'_>,
) -> Result<(), LibraryUrlError> {
    let allowed = match validator {
        LibraryUrlValidator::Predicate(predicate) => predicate(library_url),
        LibraryUrlValidator::AllowList(list) => {
            let mut allowed = false;
            for entry in list.iter() {
                if entry_allows(entry, library_url)? {
                    allowed = true;
                    break;
                }
            }
            allowed
        }
    };
    if allowed {
        Ok(())
    } else {
        Err(LibraryUrlError::Disallowed {
            url: library_url.to_owned(),
        })
    }
}

/// `new URL(url)`.
fn parse_url(url: &str) -> Result<Url, LibraryUrlError> {
    Url::parse(url).map_err(|_| LibraryUrlError::InvalidUrl)
}

/// `new RegExp(source)`.
fn regexp(source: &str) -> Result<RegExp, LibraryUrlError> {
    RegExp::new(source).map_err(|e| LibraryUrlError::InvalidRegExp { message: e.0 })
}

/// The `validator.some(...)` callback of `library.ts:511-523`.
fn entry_allows(entry: &str, library_url: &str) -> Result<bool, LibraryUrlError> {
    // allowedUrlDef.replace(/^https?:\/\//, "")
    let bare = entry
        .strip_prefix("https://")
        .or_else(|| entry.strip_prefix("http://"))
        .unwrap_or(entry);
    let allowed_url = parse_url(&format!("https://{bare}"))?;
    let url = parse_url(library_url)?;
    let host = regexp(&format!(
        r"(^|\.){}$",
        allowed_url.host_str().unwrap_or_default()
    ))?;
    if !host.test(url.host_str().unwrap_or_default()) {
        return Ok(false);
    }
    let path = regexp(&format!(
        "^{}(/+|$)",
        pathname(&allowed_url).trim_end_matches('/')
    ))?;
    Ok(path.test(&pathname(&url)))
}

/// `url.pathname`. The URL Standard's path percent-encode set has U+005E
/// (`^`), which `new URL` in Node 26 (ada) encodes as `%5E` and the `url`
/// crate (2.5.8) leaves as it is; a path that is not opaque gets it here.
/// It matters to the allow-list: `example.com/[^a-c]` is the class
/// `[%5Ea-c]` upstream, not a negated one.
fn pathname(url: &Url) -> String {
    if url.cannot_be_a_base() {
        url.path().to_owned()
    } else {
        url.path().replace('^', "%5E")
    }
}

/// What [`parse_library_tokens`] reads from the address: the library URL
/// (not yet decoded or checked) and the `token` of the hash, which
/// libraries.excalidraw.com sets to the id of the editor that sent the user
/// there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryUrlTokens {
    pub library_url: String,
    pub id_token: Option<String>,
}

impl LibraryUrlTokens {
    /// `idToken !== excalidrawAPI.id` (`library.ts:747`): the import asks the
    /// user first unless it was requested by this editor (`editor_id`).
    pub fn should_prompt(&self, editor_id: &str) -> bool {
        self.id_token.as_deref() != Some(editor_id)
    }
}

/// `new URLSearchParams(init)` for a string: one leading `?` removed, then
/// the `application/x-www-form-urlencoded` parser (`+` is a space, percent
/// escapes decoded as UTF-8 with replacement characters).
fn search_params(init: &str) -> Vec<(String, String)> {
    let init = init.strip_prefix('?').unwrap_or(init);
    form_urlencoded::parse(init.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

/// `params.get(name)`: the first value, if any.
fn get<'a>(params: &'a [(String, String)], name: &str) -> Option<&'a str> {
    params
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

/// `hash.slice(1)`: without its first character (the `#`).
fn after_first(s: &str) -> &str {
    let mut chars = s.chars();
    chars.next();
    chars.as_str()
}

/// `parseLibraryTokensFromUrl()` (`library.ts:530-543`) for an address whose
/// `location.search` is `search` and `location.hash` is `hash` (each empty
/// or starting with `?` and `#`).
///
/// The library URL is the first `addLibrary` value of the hash, or when that
/// is missing or empty, of the query (the legacy form); the token is the
/// first `token` value of the hash, whichever form the URL came in.
/// `None` when neither has a non-empty `addLibrary`.
pub fn parse_library_tokens(search: &str, hash: &str) -> Option<LibraryUrlTokens> {
    let hash_params = search_params(after_first(hash));
    let library_url = get(&hash_params, URL_HASH_KEY_ADD_LIBRARY)
        .filter(|url| !url.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            get(&search_params(search), URL_QUERY_KEY_ADD_LIBRARY)
                .filter(|url| !url.is_empty())
                .map(str::to_owned)
        })?;
    Some(LibraryUrlTokens {
        library_url,
        id_token: get(&hash_params, URL_HASH_KEY_TOKEN).map(str::to_owned),
    })
}

/// `location.search` and `location.hash` of `url`: `?query` and
/// `#fragment`, or empty when either is absent or empty.
fn search_and_hash(url: &Url) -> (String, String) {
    let part = |prefix: char, value: Option<&str>| match value {
        Some(value) if !value.is_empty() => format!("{prefix}{value}"),
        _ => String::new(),
    };
    (part('?', url.query()), part('#', url.fragment()))
}

/// [`parse_library_tokens`] for the address `href` (`location.href`);
/// `None` also when `href` is not a URL.
pub fn parse_library_tokens_from_url(href: &str) -> Option<LibraryUrlTokens> {
    let url = Url::parse(href).ok()?;
    let (search, hash) = search_and_hash(&url);
    parse_library_tokens(&search, &hash)
}

/// `decodeURIComponent(s)` (ECMA-262 section 19.2.6.3): every `%XX`
/// sequence decoded as UTF-8. A `%` without two hex digits after it, or
/// bytes that are not the UTF-8 of one code point (a stray continuation
/// byte, a truncated, overlong or surrogate sequence, beyond U+10FFFF), is
/// [`LibraryUrlError::UriMalformed`].
pub fn decode_uri_component(s: &str) -> Result<String, LibraryUrlError> {
    let bytes = s.as_bytes();
    let byte_at = |i: usize| -> Result<u8, LibraryUrlError> {
        if bytes.get(i) != Some(&b'%') {
            return Err(LibraryUrlError::UriMalformed);
        }
        let hex = bytes
            .get(i + 1..i + 3)
            .ok_or(LibraryUrlError::UriMalformed)?;
        let hex = std::str::from_utf8(hex).map_err(|_| LibraryUrlError::UriMalformed)?;
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(LibraryUrlError::UriMalformed);
        }
        u8::from_str_radix(hex, 16).map_err(|_| LibraryUrlError::UriMalformed)
    };
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        let first = byte_at(i)?;
        i += 3;
        let n = first.leading_ones() as usize;
        if first < 0x80 {
            out.push(first);
            continue;
        }
        if n == 1 || n > 4 {
            return Err(LibraryUrlError::UriMalformed);
        }
        let mut sequence = vec![first];
        for _ in 1..n {
            let next = byte_at(i)?;
            if next & 0xC0 != 0x80 {
                return Err(LibraryUrlError::UriMalformed);
            }
            sequence.push(next);
            i += 3;
        }
        // Rejects overlong forms, surrogates and values above U+10FFFF.
        std::str::from_utf8(&sequence).map_err(|_| LibraryUrlError::UriMalformed)?;
        out.extend_from_slice(&sequence);
    }
    // Only whole code points were added to valid UTF-8.
    String::from_utf8(out).map_err(|_| LibraryUrlError::UriMalformed)
}

/// The URL `importLibraryFromURL` fetches for the `addLibrary` value
/// `library_url` (`library.ts:726-731`): [`decode_uri_component`], then
/// [`to_valid_url`] on `origin` (the editor's `location.origin`), then
/// [`validate_library_url`] with the default allow-list.
pub fn resolve_library_url(library_url: &str, origin: &str) -> Result<String, LibraryUrlError> {
    resolve_library_url_with(library_url, origin, &LibraryUrlValidator::default())
}

/// [`resolve_library_url`] with the editor's own validator (the
/// `validateLibraryUrl` option of `useHandleLibrary`, `library.ts:731`).
pub fn resolve_library_url_with(
    library_url: &str,
    origin: &str,
    validator: &LibraryUrlValidator<'_>,
) -> Result<String, LibraryUrlError> {
    let decoded = decode_uri_component(library_url)?;
    let url = to_valid_url(&decoded, origin);
    validate_library_url_with(&url, validator)?;
    Ok(url)
}

/// The address upstream moves to once an import from `#addLibrary` is over,
/// successful or not (`library.ts:765-776`, with `history.replaceState`):
/// when `hash` contains the text `addLibrary`, `#` and the hash's parameters
/// without any `addLibrary`; otherwise when `search` does, `?` and the
/// query's parameters without it; otherwise `None` (nothing to replace).
/// Parameters are written back as `URLSearchParams.toString()` does (`+`
/// for spaces, other characters percent-encoded).
pub fn library_url_after_import(search: &str, hash: &str) -> Option<String> {
    let without = |init: &str, key: &str| {
        let params = search_params(init);
        form_urlencoded::Serializer::new(String::new())
            .extend_pairs(params.iter().filter(|(k, _)| k != key))
            .finish()
    };
    if hash.contains(URL_HASH_KEY_ADD_LIBRARY) {
        Some(format!(
            "#{}",
            without(after_first(hash), URL_HASH_KEY_ADD_LIBRARY)
        ))
    } else if search.contains(URL_QUERY_KEY_ADD_LIBRARY) {
        Some(format!("?{}", without(search, URL_QUERY_KEY_ADD_LIBRARY)))
    } else {
        None
    }
}
