//! `new URL(input)` as upstream's code sees it: the `url` crate (2.5.8, an
//! implementation of the WHATWG URL Standard), with the places where it
//! answers differently from `new URL` in Node 26 (ada), the engine the
//! fixtures are recorded with, put right. Each is pinned by
//! `tools/goldens/library-url-fixtures.mjs` or
//! `tools/goldens/url-host-fixtures.mjs`:
//!
//! - `file:` URLs. The crate drops the empty path segments right after the
//!   host (`file://h//x` and `file://h/\x` have the pathname `/x`, the
//!   standard's `//x`) and the host of a URL whose path starts with a
//!   Windows drive letter (`file://h/C:/x` has no host, the standard's is
//!   `h`). The host and path of a `file:` URL are parsed here, the path
//!   with the standard's path state ([`file_host_and_path`]).
//! - A non-special URL whose authority ends in `@` with nothing after it
//!   (`x://@`) is a host-missing failure; the crate parses it.
//! - Hosts that UTS 46 rejects but that are ASCII (`xn--`, `xn--zz`: an
//!   `xn--` label that is not Punycode). ada takes an ASCII domain as it is,
//!   lower-cased, where the crate fails with an IDNA error; the host is
//!   checked here the way ada checks an ASCII one ([`ascii_domain`]).
//! - A non-special URL whose port has something other than digits before
//!   the end of the authority (`x://h:1\\`): the crate ends the port at the
//!   backslash.
//! - A space right before the `?` or `#` that ends an opaque path is `%20`
//!   in ada's pathname.
//! - Hierarchical paths of URLs that are not `file:`. The crate applies the
//!   Windows drive letter rules to every scheme, so `..` does not remove a
//!   `C:` or `c|` segment (`https://h/C:/..` has the pathname `/C:/`, the
//!   standard's `/`), and it leaves `^` as it is where the standard's path
//!   percent-encode set has `%5E`. These pathnames are parsed here too, by
//!   the path start and path states ([`hierarchical_path`]).
//! - Hosts outside ASCII (after percent-decoding) in special URLs. ada's
//!   IDNA is not the crate's UTS 46: its combining-mark and bidi tables are
//!   Unicode 13 while its mapping is IDNA 17, it applies the Bidi rule per
//!   right-to-left label, and more (see [`crate::ada_idna`]). So
//!   `https://\u{1AD3}/` is `xn--trf` and `https://a\u{10D50}/` is
//!   `xn--a-ho6i` in Node, which the crate rejects, and
//!   `https://\u{5D0}\u{1AD3}/` is not a URL, which the crate accepts. Such a
//!   domain is converted here by the port of ada's `to_ascii` and its
//!   result put in the URL in its place before the crate parses it
//!   ([`non_ascii_domain`]). `tools/goldens/url-host-fixtures.mjs` pins
//!   this against Node over every code point and seeded random URLs.

use url::{ParseError, Url};

/// What upstream reads from a parsed URL: `hostname`, `pathname`, `search`
/// and `hash`.
pub(crate) struct JsUrl {
    url: Url,
    hostname: String,
    pathname: String,
}

impl JsUrl {
    /// `url.hostname`: the serialized host, or empty.
    pub(crate) fn hostname(&self) -> &str {
        &self.hostname
    }

    /// `url.pathname`.
    pub(crate) fn pathname(&self) -> &str {
        &self.pathname
    }

    /// The query without its `?`, if there is one.
    pub(crate) fn query(&self) -> Option<&str> {
        self.url.query()
    }

    /// The fragment without its `#`, if there is one.
    pub(crate) fn fragment(&self) -> Option<&str> {
        self.url.fragment()
    }

    /// `url.host`: the hostname, and `:port` when the URL has a port other
    /// than its scheme's default.
    pub(crate) fn host(&self) -> String {
        match self.url.port() {
            Some(port) => format!("{}:{port}", self.hostname),
            None => self.hostname.clone(),
        }
    }

    /// `url.searchParams.has(name)`: whether a name of the query, read as
    /// `application/x-www-form-urlencoded`, is `name`.
    pub(crate) fn search_params_has(&self, name: &str) -> bool {
        self.url
            .query()
            .is_some_and(|q| url::form_urlencoded::parse(q.as_bytes()).any(|(key, _)| key == name))
    }

    /// `url.searchParams.get(name)`: the value of the query's first pair
    /// named `name`, read as `application/x-www-form-urlencoded`.
    pub(crate) fn search_params_get(&self, name: &str) -> Option<String> {
        url::form_urlencoded::parse(self.url.query()?.as_bytes())
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
    }
}

/// `new URL(input)`: `None` where it throws `TypeError: Invalid URL`.
pub(crate) fn parse(input: &str) -> Option<JsUrl> {
    let mut input = preprocess(input);
    let (scheme, rest) = scheme_and_rest(&input)?;
    if is_special(&scheme) {
        if let Some((start, end)) = host_span(&scheme, &input, rest) {
            // A domain outside ASCII: ada's IDNA gives its ASCII form, which
            // then stands in the URL for it, so everything after (IPv4,
            // `localhost` in `file:`) is what ada does with that result.
            if !percent_decode(&input[start..end]).is_ascii() {
                let ascii = non_ascii_domain(&input[start..end])?;
                input.replace_range(start..end, &ascii);
            }
        }
    }
    match Url::parse(&input) {
        Ok(url) => {
            if authority_fails(&scheme, &input[rest..]) {
                return None;
            }
            build(url, &scheme, &input[rest..], None)
        }
        Err(ParseError::IdnaError) => {
            let (start, end) = host_span(&scheme, &input, rest)?;
            let host = ascii_domain(&input[start..end])?;
            // The same URL with a host the crate takes, for the rest of it.
            let replaced = format!("{}h{}", &input[..start], &input[end..]);
            let url = Url::parse(&replaced).ok()?;
            build(url, &scheme, &replaced[rest..], Some(host))
        }
        Err(_) => None,
    }
}

fn build(url: Url, scheme: &str, rest: &str, host: Option<String>) -> Option<JsUrl> {
    let (hostname, pathname) = if scheme == "file" {
        let (buffer, pathname) = file_host_and_path(rest);
        let hostname = match (host, buffer) {
            (Some(host), _) => host,
            (None, None) => String::new(),
            (None, Some(buffer)) => file_host(&buffer)?,
        };
        (hostname, pathname)
    } else {
        let hostname = host.unwrap_or_else(|| url.host_str().unwrap_or_default().to_owned());
        let pathname = if url.cannot_be_a_base() {
            opaque_path(&url)
        } else {
            hierarchical_path(scheme, rest)
        };
        (hostname, pathname)
    };
    Some(JsUrl {
        url,
        hostname,
        pathname,
    })
}

/// An opaque path (`mailto:x`): a space right before the `?` or `#` that
/// ends it is `%20`, where the crate keeps it.
fn opaque_path(url: &Url) -> String {
    let path = url.path();
    match path.strip_suffix(' ') {
        Some(head) if url.query().is_some() || url.fragment().is_some() => format!("{head}%20"),
        _ => path.to_owned(),
    }
}

/// The standard's first steps: leading and trailing C0 controls and spaces
/// removed, then every tab and newline.
fn preprocess(input: &str) -> String {
    input
        .trim_matches(|c: char| c <= ' ')
        .chars()
        .filter(|&c| !matches!(c, '\t' | '\n' | '\r'))
        .collect()
}

/// The lower-cased scheme and the byte index after its `:`, or `None` when
/// the input does not start with one (not a URL without a base).
fn scheme_and_rest(input: &str) -> Option<(String, usize)> {
    let colon = input.find(':')?;
    let scheme = &input[..colon];
    let mut chars = scheme.chars();
    let first = chars.next()?;
    if !first.is_ascii_alphabetic()
        || !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return None;
    }
    Some((scheme.to_ascii_lowercase(), colon + 1))
}

fn is_special(scheme: &str) -> bool {
    matches!(scheme, "ftp" | "file" | "http" | "https" | "ws" | "wss")
}

fn is_slash(c: char, special: bool) -> bool {
    c == '/' || special && c == '\\'
}

/// The authority of a URL with one, after `scheme:`: past any slashes and
/// backslashes for a special scheme, past `//` for another; up to the first
/// `/`, `?` or `#` (or `\` for a special scheme). Not for `file:`.
fn authority<'a>(scheme: &str, rest: &'a str) -> Option<&'a str> {
    let special = is_special(scheme);
    let start = if special {
        rest.trim_start_matches(['/', '\\'])
    } else {
        rest.strip_prefix("//")?
    };
    let end = start
        .find(|c| is_slash(c, special) || c == '?' || c == '#')
        .unwrap_or(start.len());
    Some(&start[..end])
}

/// Failures of the authority, host and port states the crate lets through
/// for a non-special URL: an `@` with nothing after the last one
/// (host-missing), and a port with something other than digits before the
/// end of the authority (port-invalid; the crate ends a port at `\` in any
/// URL, the standard only in a special one).
fn authority_fails(scheme: &str, rest: &str) -> bool {
    if scheme == "file" {
        return false;
    }
    let Some(authority) = authority(scheme, rest) else {
        return false;
    };
    let host_and_port = match authority.rsplit_once('@') {
        Some((_, "")) => return true,
        Some((_, host_and_port)) => host_and_port,
        None => authority,
    };
    let mut inside_brackets = false;
    for (i, c) in host_and_port.char_indices() {
        match c {
            '[' => inside_brackets = true,
            ']' => inside_brackets = false,
            ':' if !inside_brackets => {
                return !host_and_port[i + 1..].bytes().all(|b| b.is_ascii_digit());
            }
            _ => {}
        }
    }
    false
}

/// The byte range of the host in `input` of a special URL (its authority
/// after `rest`): after the last `@` of the authority, up to a `:`; for
/// `file:` the file host state's buffer after `//`.
fn host_span(scheme: &str, input: &str, rest: usize) -> Option<(usize, usize)> {
    let after = &input[rest..];
    let (start, host) = if scheme == "file" {
        let mut chars = after.chars();
        if !(chars.next().is_some_and(|c| is_slash(c, true))
            && chars.next().is_some_and(|c| is_slash(c, true)))
        {
            return None;
        }
        let host = &after[2..];
        let end = host
            .find(|c| is_slash(c, true) || c == '?' || c == '#')
            .unwrap_or(host.len());
        (rest + 2, &host[..end])
    } else {
        let authority = authority(scheme, after)?;
        let offset = rest + (authority.as_ptr() as usize - after.as_ptr() as usize);
        let (skip, host) = match authority.rfind('@') {
            Some(at) => (at + 1, &authority[at + 1..]),
            None => (0, authority),
        };
        let host = &host[..host.find(':').unwrap_or(host.len())];
        (offset + skip, host)
    };
    Some((start, start + host.len()))
}

/// ada's host for a domain that is ASCII once percent-decoded: lower-cased,
/// and rejected when empty, when it has a forbidden domain code point, or
/// when it ends in a number (an IPv4 address; one with a label UTS 46 turns
/// down has a label that is not a number, so it is not one).
fn ascii_domain(raw: &str) -> Option<String> {
    let domain = String::from_utf8(percent_decode(raw)).ok()?;
    if domain.is_empty() || !domain.is_ascii() {
        return None;
    }
    let domain = domain.to_ascii_lowercase();
    if domain.chars().any(is_forbidden_domain_code_point) || ends_in_a_number(&domain) {
        return None;
    }
    Some(domain)
}

/// ada's `unicode::to_ascii` for a domain outside ASCII once
/// percent-decoded: UTF-8 (strictly; ada's decoder refuses what Rust's
/// does), then [`ada_idna::to_ascii`](crate::ada_idna::to_ascii); `None`
/// for an empty result or one with a forbidden domain code point.
fn non_ascii_domain(raw: &str) -> Option<String> {
    let domain = String::from_utf8(percent_decode(raw)).ok()?;
    let ascii = crate::ada_idna::to_ascii(&domain)?;
    if ascii.is_empty() || ascii.chars().any(is_forbidden_domain_code_point) {
        return None;
    }
    Some(ascii)
}

fn percent_decode(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if let Some(hex) = bytes.get(i + 1..i + 3) {
                if hex.iter().all(u8::is_ascii_hexdigit) {
                    let hex = std::str::from_utf8(hex).unwrap_or_default();
                    if let Ok(byte) = u8::from_str_radix(hex, 16) {
                        out.push(byte);
                        i += 3;
                        continue;
                    }
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

/// The URL Standard's forbidden domain code points.
fn is_forbidden_domain_code_point(c: char) -> bool {
    c <= '\u{1f}'
        || matches!(
            c,
            ' ' | '#'
                | '%'
                | '/'
                | ':'
                | '<'
                | '>'
                | '?'
                | '@'
                | '['
                | '\\'
                | ']'
                | '^'
                | '|'
                | '\u{7f}'
        )
}

/// The URL Standard's "ends in a number": the last label (the one before a
/// trailing dot, if any) is all digits or a `0x` hexadecimal number.
fn ends_in_a_number(domain: &str) -> bool {
    let domain = domain.strip_suffix('.').unwrap_or(domain);
    let last = domain.rsplit('.').next().unwrap_or_default();
    if !last.is_empty() && last.bytes().all(|b| b.is_ascii_digit()) {
        return true;
    }
    last.strip_prefix("0x")
        .is_some_and(|hex| hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// The host of a `file:` URL from the file host state's `buffer`: `""` for
/// `localhost`, the parsed host otherwise; `None` when it is not a host.
fn file_host(buffer: &str) -> Option<String> {
    let url = Url::parse(&format!("file://{buffer}/")).ok()?;
    Some(url.host_str().unwrap_or_default().to_owned())
}

fn is_windows_drive_letter(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 2 && b[0].is_ascii_alphabetic() && matches!(b[1], b':' | b'|')
}

fn is_normalized_windows_drive_letter(s: &str) -> bool {
    is_windows_drive_letter(s) && s.as_bytes()[1] == b':'
}

fn is_single_dot(s: &str) -> bool {
    s == "." || s.eq_ignore_ascii_case("%2e")
}

fn is_double_dot(s: &str) -> bool {
    matches!(
        s.to_ascii_lowercase().as_str(),
        ".." | ".%2e" | "%2e." | "%2e%2e"
    )
}

/// The path percent-encode set, without `?` and `#`, which end a path.
fn in_path_percent_encode_set(c: char) -> bool {
    c <= '\u{1f}' || c > '~' || matches!(c, ' ' | '"' | '<' | '>' | '^' | '`' | '{' | '}')
}

/// The host buffer and pathname of the `file:` URL whose text after
/// `file:` is `rest` (already preprocessed), by the standard's file, file
/// slash, file host, path start and path states with no base URL. The
/// buffer is `None` for an empty host.
fn file_host_and_path(rest: &str) -> (Option<String>, String) {
    let chars: Vec<char> = rest.chars().collect();
    let slash = |i: usize| chars.get(i).is_some_and(|&c| is_slash(c, true));
    let mut host = None;
    let mut buffer = String::new();
    let mut i = 0;
    if slash(0) && slash(1) {
        // file host state
        i = 2;
        let start = i;
        while chars
            .get(i)
            .is_some_and(|&c| !is_slash(c, true) && c != '?' && c != '#')
        {
            i += 1;
        }
        let host_buffer: String = chars[start..i].iter().collect();
        if is_windows_drive_letter(&host_buffer) {
            // The Windows drive letter quirk: the buffer is the path's.
            buffer = host_buffer;
        } else {
            if !host_buffer.is_empty() {
                host = Some(host_buffer);
            }
            // path start state
            if slash(i) {
                i += 1;
            }
        }
    } else if slash(0) {
        // file slash state, then the path state from the next code point
        i = 1;
    }
    (host, path_state(&chars, i, buffer, "file"))
}

/// The pathname of a URL with a hierarchical path whose scheme is not
/// `file` and whose text after `scheme:` is `rest` (already preprocessed):
/// the path start and path states from the end of the authority, or from
/// after the `/` of a non-special `scheme:/path` with no authority. The
/// crate keeps a `C:` or `c|` segment that `..` should remove in these
/// URLs (`https://h/C:/..` has the pathname `/`), since it applies the
/// Windows drive letter rules to every scheme; the standard applies them to
/// `file:` only.
fn hierarchical_path(scheme: &str, rest: &str) -> String {
    let special = is_special(scheme);
    let Some(authority) = authority(scheme, rest) else {
        // `scheme:/path`: the path or authority state saw one `/`, and the
        // path state starts after it.
        let chars: Vec<char> = rest.chars().skip(1).collect();
        return path_state(&chars, 0, String::new(), scheme);
    };
    let start = authority.as_ptr() as usize - rest.as_ptr() as usize + authority.len();
    let chars: Vec<char> = rest[start..].chars().collect();
    let mut i = 0;
    // path start state
    match chars.first() {
        Some(&c) if is_slash(c, special) => i = 1,
        None | Some('?' | '#') if !special => return String::new(),
        _ => {}
    }
    path_state(&chars, i, String::new(), scheme)
}

/// The standard's path state from `chars[i]` with `buffer` so far, to the
/// end of the path; the pathname. `\\` is a slash in a special URL, and the
/// Windows drive letter rules are for `file:` only.
fn path_state(chars: &[char], mut i: usize, mut buffer: String, scheme: &str) -> String {
    let special = is_special(scheme);
    let file = scheme == "file";
    let mut path: Vec<String> = Vec::new();
    loop {
        let c = chars.get(i).copied();
        match c {
            None | Some('?' | '#') | Some('/') => {}
            Some('\\') if special => {}
            Some(c) => {
                if in_path_percent_encode_set(c) {
                    let mut utf8 = [0; 4];
                    for byte in c.encode_utf8(&mut utf8).bytes() {
                        buffer.push_str(&format!("%{byte:02X}"));
                    }
                } else {
                    buffer.push(c);
                }
                i += 1;
                continue;
            }
        }
        let at_slash = matches!(c, Some('/' | '\\'));
        if is_double_dot(&buffer) {
            let keep = file && path.len() == 1 && is_normalized_windows_drive_letter(&path[0]);
            if !keep {
                path.pop();
            }
            if !at_slash {
                path.push(String::new());
            }
        } else if is_single_dot(&buffer) {
            if !at_slash {
                path.push(String::new());
            }
        } else {
            if file && path.is_empty() && is_windows_drive_letter(&buffer) {
                buffer.replace_range(1..2, ":");
            }
            path.push(std::mem::take(&mut buffer));
        }
        buffer.clear();
        if !at_slash {
            break;
        }
        i += 1;
    }
    path.iter().map(|segment| format!("/{segment}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(input: &str) -> Option<(String, String)> {
        parse(input).map(|url| (url.hostname().to_owned(), url.pathname().to_owned()))
    }

    fn some(host: &str, path: &str) -> Option<(String, String)> {
        Some((host.to_owned(), path.to_owned()))
    }

    // Each expectation is what `new URL` gives in Node 26.
    #[test]
    fn file_urls_keep_empty_segments_and_their_host() {
        assert_eq!(parts("file://h/\\x"), some("h", "//x"));
        assert_eq!(parts("file://h//x"), some("h", "//x"));
        assert_eq!(parts("file://h\\\\x"), some("h", "//x"));
        assert_eq!(parts("file://h/\\/x"), some("h", "///x"));
        assert_eq!(parts("file:///\\x"), some("", "//x"));
        assert_eq!(parts("file:/\\x"), some("x", "/"));
        assert_eq!(parts("file://h/\\../x"), some("h", "/x"));
        assert_eq!(parts("file://h/C:/\\x"), some("h", "/C://x"));
        assert_eq!(parts("file://h/\\C:/x"), some("h", "//C:/x"));
        assert_eq!(parts("file://h/c|/x"), some("h", "/c:/x"));
        assert_eq!(parts("file://h/C:/../.."), some("h", "/C:/"));
        assert_eq!(parts("file://C|/x"), some("", "/C:/x"));
        assert_eq!(parts("file:C:/x"), some("", "/C:/x"));
        assert_eq!(parts("file:/C|\\..\\y"), some("", "/C:/y"));
        assert_eq!(parts("file://h/%2e%2E/x"), some("h", "/x"));
        assert_eq!(parts("file://h/a/%2e/b"), some("h", "/a/b"));
        assert_eq!(parts("file://LOCALHOST/x"), some("", "/x"));
        assert_eq!(parts("file:"), some("", "/"));
        assert_eq!(
            parts("file://h/ ^`{}\u{e9}/x"),
            some("h", "/%20%5E%60%7B%7D%C3%A9/x")
        );
        assert_eq!(parts("file://a@b/x"), None);
        assert_eq!(parts("file://h:80/x"), None);
    }

    #[test]
    fn credentials_without_a_host_are_not_a_url() {
        assert_eq!(parts("x://@"), None);
        assert_eq!(parts("x://a@b@"), None);
        assert_eq!(parts("web+a://u@"), None);
        assert_eq!(parts("x://@h"), some("h", ""));
        assert_eq!(parts("x:///@"), some("", "/@"));
        assert_eq!(parts("x://h:1\\"), None);
        assert_eq!(parts("x://h:1a"), None);
        assert_eq!(parts("x://[::1]:80/"), some("[::1]", "/"));
        assert_eq!(parts("x://h:/p"), some("h", "/p"));
    }

    #[test]
    fn a_space_ending_an_opaque_path_is_encoded() {
        assert_eq!(parts("x:a  #f"), some("", "a %20"));
        assert_eq!(parts("x:a ?q"), some("", "a%20"));
        assert_eq!(parts("x:a b"), some("", "a b"));
    }

    #[test]
    fn ascii_hosts_uts46_turns_down() {
        assert_eq!(parts("https://xn--/x"), some("xn--", "/x"));
        assert_eq!(parts("https://XN--A/x"), some("xn--a", "/x"));
        assert_eq!(parts("https://xn--%41.b/x"), some("xn--a.b", "/x"));
        assert_eq!(parts("https://u@xn--:8080/p?q#f"), some("xn--", "/p"));
        assert_eq!(parts("https:xn--/x"), some("xn--", "/x"));
        assert_eq!(parts("https:\\\\xn--\\x"), some("xn--", "/x"));
        assert_eq!(parts("file://xn--/C:/x"), some("xn--", "/C:/x"));
        assert_eq!(parts("file:\\\\xn--\\\\a"), some("xn--", "//a"));
        assert_eq!(parts("x://xn--/y"), some("xn--", "/y"));
        assert_eq!(parts("https://xn--ls8h/x"), some("xn--ls8h", "/x"));
        assert_eq!(parts("https://xn--%C3%A4/x"), None);
        assert_eq!(parts("https://xn--%FF/x"), None);
        assert_eq!(parts("https://xn--zz.\u{e4}/x"), None);
        assert_eq!(parts("https://xn--0.1.2.3/x"), None);
        assert_eq!(parts("https://xn--%2F/x"), None);
        assert_eq!(parts("https://xn--:99999/x"), None);
        let url = parse("https://u@xn--:8080/p?q#f").unwrap();
        assert_eq!((url.query(), url.fragment()), (Some("q"), Some("f")));
    }

    #[test]
    fn drive_letters_are_ordinary_segments_outside_file_urls() {
        assert_eq!(
            parts("https://excalidraw.com/C:/.."),
            some("excalidraw.com", "/")
        );
        assert_eq!(
            parts("https://excalidraw.com/c|/%2e%2e/"),
            some("excalidraw.com", "/")
        );
        assert_eq!(parts("http://h/C:/.."), some("h", "/"));
        assert_eq!(parts("ws://h/c|/../x"), some("h", "/x"));
        assert_eq!(parts("wss://h/./C:/../"), some("h", "/"));
        assert_eq!(parts("x://h/c|/.."), some("h", "/"));
        assert_eq!(parts("x:/C:/.."), some("", "/"));
        assert_eq!(parts("x:/c|/.."), some("", "/"));
        assert_eq!(parts("ftp:,\u{df}\\c|/%2E%2e/"), some("xn--,-qfa", "/"));
        assert_eq!(parts("https://h/c|/x"), some("h", "/c|/x"));
        assert_eq!(parts("x://h/C|/x"), some("h", "/C|/x"));
        assert_eq!(parts("https://h/C:"), some("h", "/C:"));
        assert_eq!(parts("http://h/c|"), some("h", "/c|"));
        assert_eq!(
            parts("https://raw.githubusercontent.com/C:/../excalidraw/x"),
            some("raw.githubusercontent.com", "/excalidraw/x")
        );
    }

    #[test]
    fn paths_outside_file_urls_follow_the_path_state() {
        assert_eq!(parts("x://h"), some("h", ""));
        assert_eq!(parts("x://h?q"), some("h", ""));
        assert_eq!(parts("x://h#f"), some("h", ""));
        assert_eq!(parts("x://h/"), some("h", "/"));
        assert_eq!(parts("x:/"), some("", "/"));
        assert_eq!(parts("x:"), some("", ""));
        assert_eq!(parts("https://h"), some("h", "/"));
        assert_eq!(parts("https://h?q"), some("h", "/"));
        assert_eq!(parts("https:h\\\\a\\\\..\\\\b"), some("h", "//a//b"));
        assert_eq!(parts("x://h/a\\..\\b"), some("h", "/a\\..\\b"));
        assert_eq!(parts("x:/.//p"), some("", "//p"));
        assert_eq!(parts("x:/..//p"), some("", "//p"));
        assert_eq!(parts("x:/a/../.."), some("", "/"));
        assert_eq!(parts("https://h/a/./b/%2E/c/.%2e"), some("h", "/a/b/"));
        assert_eq!(
            parts("x://h/ ^`{}\u{e9}|\"<>"),
            some("h", "/%20%5E%60%7B%7D%C3%A9|%22%3C%3E")
        );
    }

    /// Hosts outside ASCII go through ada's IDNA, not the crate's UTS 46:
    /// ada's validity tables are Unicode 13 (its mapping and normalization
    /// Unicode 17) and it applies the Bidi rule to each label that is
    /// right-to-left on its own. So it accepts a label starting with a mark
    /// from Unicode 14 or later, or mixing `a` with a right-to-left letter
    /// from then, and rejects a Hebrew letter followed by U+1AD3 (an NSM in
    /// Unicode 17, unknown to ada's table). Each expectation is Node 26.10.
    #[test]
    fn non_ascii_hosts_as_ada_parses_them() {
        // A combining mark ada does not know starting a label.
        assert_eq!(parts("https://\u{1AD3}/"), some("xn--trf", "/"));
        assert_eq!(parts("file://\u{1AD3}/"), some("xn--trf", "/"));
        assert_eq!(parts("https://\u{0C3C}/"), some("xn--3pc", "/"));
        assert_eq!(parts("https://\u{1E6E3}/"), some("xn--uw5h", "/"));
        assert_eq!(
            parts("https://\u{1AD3}.excalidraw.com/"),
            some("xn--trf.excalidraw.com", "/")
        );
        // A right-to-left letter ada does not know after a left-to-right one.
        assert_eq!(parts("https://a\u{10D50}/"), some("xn--a-ho6i", "/"));
        assert_eq!(parts("https://a\u{0870}/"), some("xn--a-fld", "/"));
        // The same through an `xn--` label: the soft hyphen is dropped by
        // the mapping, and the Punycode decodes to U+1AD3 followed by ASCII.
        assert_eq!(
            parts("ws:\u{ad}XN--A_xn--LOCALHOSTxn--ls8h"),
            some("xn--a_xn--localhostxn--ls8h", "/")
        );
        // A mark ada does not know is not an NSM to it: a right-to-left
        // label with one is rejected.
        assert_eq!(parts("https://\u{05D0}\u{1AD3}/"), None);
        assert_eq!(parts("https://\u{05D0}\u{1AD3}.excalidraw.com/x"), None);
        assert_eq!(parts("https://\u{05D0}\u{0301}/"), some("xn--lsa15l", "/"));
        // The Bidi rule for each right-to-left label, not the whole domain.
        assert_eq!(parts("https://1.\u{05D0}/"), some("1.xn--4db", "/"));
        assert_eq!(parts("https://1\u{05D0}/"), None);
        // Marks and letters ada knows.
        assert_eq!(parts("https://a\u{1C8A}/"), some("xn--a-hzl", "/"));
        assert_eq!(parts("https://a\u{1AD3}/"), some("xn--a-e9k", "/"));
        assert_eq!(parts("https://\u{0301}/"), None);
        assert_eq!(parts("https://a\u{05D0}/"), None);
        // ASCII as written.
        assert_eq!(parts("https://xn--trf/"), some("xn--trf", "/"));
        assert_eq!(
            parts("https://xn--a_xn--localhostxn--ls8h/"),
            some("xn--a_xn--localhostxn--ls8h", "/")
        );
        assert_eq!(parts("https://xn--ls8h\u{ad}/"), some("xn--ls8h", "/"));
        assert_eq!(parts("https://xn--zz\u{ad}/"), None);
        // ada leaves a Hangul LV syllable and a trailing jamo apart.
        assert_eq!(
            parts("https://\u{AC00}\u{11A8}/"),
            some("xn--rud7310f", "/")
        );
        assert_eq!(
            parts("https://\u{1100}\u{1161}\u{11A8}/"),
            some("xn--p39a", "/")
        );
    }

    /// `tests/fixtures/url-hosts.json` (tools/goldens/url-host-fixtures.mjs):
    /// `new URL` in Node for hosts outside ASCII, case by case, over every
    /// code point on its own and after `a`, and over seeded random URLs.
    mod node {
        use super::parse;
        use crate::encode::checksum::crc32;
        use serde_json::Value;

        fn fixture() -> Value {
            serde_json::from_str(include_str!("../tests/fixtures/url-hosts.json")).unwrap()
        }

        fn crc_of_lines(lines: &[String]) -> u32 {
            let mut text = String::new();
            for line in lines {
                text.push_str(line);
                text.push('\n');
            }
            crc32(0, text.as_bytes())
        }

        #[test]
        fn cases() {
            let fixture = fixture();
            let mut wrong = Vec::new();
            for case in fixture["cases"].as_array().unwrap() {
                let input = case["input"].as_str().unwrap();
                let want = if case["error"] == true {
                    None
                } else {
                    Some((
                        case["hostname"].as_str().unwrap().to_owned(),
                        case["pathname"].as_str().unwrap().to_owned(),
                    ))
                };
                let got = parse(input).map(|u| (u.hostname().to_owned(), u.pathname().to_owned()));
                if got != want {
                    wrong.push(format!("{input:?}: port {got:?}, node {want:?}"));
                }
            }
            assert!(
                wrong.is_empty(),
                "{} cases differ:\n{}",
                wrong.len(),
                wrong.join("\n")
            );
        }

        #[test]
        fn every_code_point() {
            let fixture = fixture();
            let sweep = &fixture["sweep"];
            let size = sweep["blockSize"].as_u64().unwrap() as u32;
            let forms: Vec<&str> = sweep["forms"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| f.as_str().unwrap())
                .collect();
            let mut wrong = Vec::new();
            for block in sweep["blocks"].as_array().unwrap() {
                let start = block["start"].as_u64().unwrap() as u32;
                let mut lines = Vec::new();
                let mut first = None;
                for cp in start.max(0x80)..start + size {
                    let Some(c) = char::from_u32(cp) else {
                        continue;
                    };
                    for form in &forms {
                        let input = form.replace("{}", &c.to_string());
                        let line =
                            parse(&input).map_or("!".to_owned(), |u| u.hostname().to_owned());
                        first.get_or_insert(cp);
                        lines.push(line);
                    }
                }
                let crc = crc_of_lines(&lines);
                if lines.len() as u64 != block["inputs"].as_u64().unwrap()
                    || u64::from(crc) != block["crc32"].as_u64().unwrap()
                {
                    wrong.push(format!("U+{start:04X}.."));
                }
            }
            assert!(
                wrong.is_empty(),
                "blocks that differ from Node: {}",
                wrong.join(" ")
            );
        }

        /// `randomInputs` in the generator.
        fn random_inputs(random: &Value) -> Vec<String> {
            let strings = |key: &str| -> Vec<String> {
                random[key]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|t| t.as_str().unwrap().to_owned())
                    .collect()
            };
            let (schemes, tokens) = (strings("schemes"), strings("tokens"));
            let mut state = random["seed"].as_u64().unwrap();
            let mut pick = |n: usize| {
                state = state * 48271 % 2_147_483_647;
                ((state as f64 / 2_147_483_647.0) * n as f64).floor() as usize
            };
            let max = random["maxTokens"].as_u64().unwrap() as usize;
            (0..random["count"].as_u64().unwrap())
                .map(|_| {
                    let n = 1 + pick(max);
                    let mut s = schemes[pick(schemes.len())].clone();
                    for _ in 0..n {
                        s.push_str(&tokens[pick(tokens.len())]);
                    }
                    s.push_str("/p");
                    s
                })
                .collect()
        }

        #[test]
        fn seeded_random_urls() {
            let fixture = fixture();
            let random = &fixture["random"];
            let inputs = random_inputs(random);
            let size = random["chunkSize"].as_u64().unwrap() as usize;
            let chunks = random["chunks"].as_array().unwrap();
            assert_eq!(chunks.len(), inputs.len().div_ceil(size));
            let mut accepted = 0;
            let mut wrong = Vec::new();
            for (i, (chunk, want)) in inputs.chunks(size).zip(chunks).enumerate() {
                let lines: Vec<String> = chunk
                    .iter()
                    .map(|input| match parse(input) {
                        Some(u) => {
                            accepted += 1;
                            format!("{}\t{}", u.hostname(), u.pathname())
                        }
                        None => "!".to_owned(),
                    })
                    .collect();
                if u64::from(crc_of_lines(&lines)) != want.as_u64().unwrap() {
                    wrong.push(format!("{}..{}", i * size, (i + 1) * size));
                }
            }
            assert!(
                wrong.is_empty(),
                "chunks that differ from Node: {}",
                wrong.join(" ")
            );
            assert_eq!(accepted, random["accepted"].as_u64().unwrap());
        }
    }

    #[test]
    fn caret_in_a_path_is_encoded() {
        assert_eq!(parts("https://h/^"), some("h", "/%5E"));
        assert_eq!(parts("x:^"), some("", "^"));
    }
}
