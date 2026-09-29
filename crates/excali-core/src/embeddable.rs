//! What an embeddable's link embeds: `getEmbedLink(link)`
//! (`packages/element/src/embeddable.ts:171-400`), with the link parsing it
//! does (`parseYouTubeLikeTimestamp`, `parseGoogleDriveVideoLink`,
//! `:59-131`).
//!
//! Upstream matches the link against a list of regular expressions in
//! order (YouTube, Vimeo, Google Drive, Figma, Val Town, Microsoft Forms,
//! Twitter, Reddit, GitHub Gist) and rewrites it into the URL an
//! `<iframe>` loads, or, for Twitter, Reddit and gists, a `srcdoc`
//! document. Each expression is matched here by hand with the same
//! backtracking outcome (the comments give the source). The result is what
//! the SVG export reads (`renderer/staticSvgScene.ts:359-400`): the link
//! and the kind of embed. The sandbox flags and the `srcdoc` builders are
//! the editor's and are not part of it.

use crate::json::number_to_string;
use crate::whatwg_url;

/// `IframeData["type"]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmbedType {
    Video,
    Generic,
    /// A document written into the frame's `srcdoc`: Twitter posts, Reddit
    /// posts and gists.
    Document,
}

/// What `getEmbedLink` returns for a link.
#[derive(Clone, Debug, PartialEq)]
pub struct EmbedLink {
    /// The URL the `<iframe>` loads; `None` for a [`EmbedType::Document`].
    pub link: Option<String>,
    pub kind: EmbedType,
    /// `intrinsicSize`: `{ w, h }`.
    pub intrinsic_size: [f64; 2],
}

/// `getEmbedLink(link)`: `None` for an empty link.
pub fn get_embed_link(link: &str) -> Option<EmbedLink> {
    if link.is_empty() {
        return None;
    }
    let chars: Vec<char> = link.chars().collect();
    let original_link = link;
    let generic_size = [560.0, 840.0];

    if let Some((kind, id)) = match_youtube(&chars) {
        let start_time = parse_youtube_like_timestamp(original_link);
        let time = if start_time > 0.0 {
            format!("&start={}", number_to_string(start_time))
        } else {
            String::new()
        };
        let is_portrait = link.contains("shorts");
        let link = match kind {
            Some("playlist?list=") | Some("embed/videoseries?list=") => {
                format!("https://www.youtube.com/embed/videoseries?list={id}&enablejsapi=1{time}")
            }
            _ => format!("https://www.youtube.com/embed/{id}?enablejsapi=1{time}"),
        };
        let size = if is_portrait {
            [315.0, 560.0]
        } else {
            [560.0, 315.0]
        };
        return Some(EmbedLink {
            link: Some(link),
            kind: EmbedType::Video,
            intrinsic_size: size,
        });
    }

    if let Some(target) = match_vimeo(&chars) {
        return Some(EmbedLink {
            link: Some(format!("https://player.vimeo.com/video/{target}?api=1")),
            kind: EmbedType::Video,
            intrinsic_size: [560.0, 315.0],
        });
    }

    if let Some(drive) = parse_google_drive_video_link(link) {
        let mut search = url::form_urlencoded::Serializer::new(String::new());
        if let Some(key) = &drive.resource_key {
            search.append_pair("resourcekey", key);
        }
        if let Some(t) = drive.timestamp {
            search.append_pair("t", &number_to_string(t));
        }
        let search = search.finish();
        let query = if search.is_empty() {
            String::new()
        } else {
            format!("?{search}")
        };
        return Some(EmbedLink {
            link: Some(format!(
                "https://drive.google.com/file/d/{}/preview{query}",
                drive.file_id
            )),
            kind: EmbedType::Video,
            intrinsic_size: [560.0, 315.0],
        });
    }

    // RE_FIGMA = /^https:\/\/(?:www\.)?figma\.com/
    if let Some(rest) = strip(&chars, 0, "https://") {
        let rest = strip(&chars, rest, "www.").unwrap_or(rest);
        if strip(&chars, rest, "figma.com").is_some() {
            return Some(EmbedLink {
                link: Some(format!(
                    "https://www.figma.com/embed?embed_host=share&url={}",
                    encode_uri_component(link)
                )),
                kind: EmbedType::Generic,
                intrinsic_size: [550.0, 550.0],
            });
        }
    }

    if let Some((matched, is_embed)) = match_valtown(&chars) {
        let link = if is_embed {
            matched
        } else {
            // valLink[0].replace("/v", "/embed"): the first "/v"
            matched.replacen("/v", "/embed", 1)
        };
        return Some(EmbedLink {
            link: Some(link),
            kind: EmbedType::Generic,
            intrinsic_size: generic_size,
        });
    }

    let mut link = link.to_owned();
    // RE_MSFORMS = /^(?:https?:\/\/)?forms\.microsoft\.com\//
    let forms = {
        let start = strip(&chars, 0, "https://")
            .or_else(|| strip(&chars, 0, "http://"))
            .unwrap_or(0);
        strip(&chars, start, "forms.microsoft.com/").is_some()
    };
    if forms && !link.contains("embed=true") {
        link.push_str(if link.contains('?') {
            "&embed=true"
        } else {
            "?embed=true"
        });
    }
    let chars: Vec<char> = link.chars().collect();

    if is_twitter(&chars) || is_reddit(&chars) {
        return Some(EmbedLink {
            link: None,
            kind: EmbedType::Document,
            intrinsic_size: [480.0, 480.0],
        });
    }
    if is_gist(&chars) {
        return Some(EmbedLink {
            link: None,
            kind: EmbedType::Document,
            intrinsic_size: [550.0, 720.0],
        });
    }

    Some(EmbedLink {
        link: Some(link),
        kind: EmbedType::Generic,
        intrinsic_size: generic_size,
    })
}

/// The position after `prefix` when `s` has it at `at`.
fn strip(s: &[char], at: usize, prefix: &str) -> Option<usize> {
    let mut i = at;
    for c in prefix.chars() {
        if s.get(i) != Some(&c) {
            return None;
        }
        i += 1;
    }
    Some(i)
}

/// The end of the longest run of `pred` characters from `at`.
fn run(s: &[char], at: usize, pred: impl Fn(char) -> bool) -> usize {
    let mut i = at;
    while i < s.len() && pred(s[i]) {
        i += 1;
    }
    i
}

/// JavaScript's `\s`: white space and line terminators.
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// `.`: anything but a line terminator.
fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `\w`.
fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_id_char(c: char) -> bool {
    is_word(c) || c == '-'
}

/// `(?:http(?:s)?:\/\/)?(?:www\.)?` at the start: where the rest begins.
/// Taking the longest prefix is the backtracking outcome, as what follows
/// never starts with `h` or `w`.
fn http_www(s: &[char]) -> usize {
    let at = strip(s, 0, "https://")
        .or_else(|| strip(s, 0, "http://"))
        .unwrap_or(0);
    strip(s, at, "www.").unwrap_or(at)
}

/// `RE_YOUTUBE`: `^(?:http(?:s)?:\/\/)?(?:www\.)?youtu(?:be\.com|\.be)\/
/// (embed\/|watch\?v=|shorts\/|live\/|playlist\?list=|
/// embed\/videoseries\?list=)?([a-zA-Z0-9_-]+)`: the first group (the
/// first alternative after which an id follows) and the id.
fn match_youtube(s: &[char]) -> Option<(Option<&'static str>, String)> {
    let at = http_www(s);
    let at = strip(s, at, "youtu")?;
    let at = strip(s, at, "be.com").or_else(|| strip(s, at, ".be"))?;
    let at = strip(s, at, "/")?;
    let id = |from: usize| {
        let end = run(s, from, is_id_char);
        (end > from).then(|| s[from..end].iter().collect::<String>())
    };
    for alternative in [
        "embed/",
        "watch?v=",
        "shorts/",
        "live/",
        "playlist?list=",
        "embed/videoseries?list=",
    ] {
        if let Some(after) = strip(s, at, alternative) {
            if let Some(id) = id(after) {
                return Some((Some(alternative), id));
            }
        }
    }
    id(at).map(|id| (None, id))
}

/// `RE_VIMEO`: `^(?:http(?:s)?:\/\/)?(?:(?:w){3}\.)?(?:player\.)?vimeo\.com
/// \/(?:video\/)?([^?\s]+)(?:\?.*)?$`: the target.
fn match_vimeo(s: &[char]) -> Option<String> {
    let at = http_www(s);
    let at = strip(s, at, "player.").unwrap_or(at);
    let at = strip(s, at, "vimeo.com/")?;
    let tail = |from: usize| {
        let end = run(s, from, |c| c != '?' && !is_js_space(c));
        if end == from {
            return None;
        }
        // (?:\?.*)?$
        let rest = &s[end..];
        let ok =
            rest.is_empty() || (rest[0] == '?' && !rest.iter().any(|c| is_line_terminator(*c)));
        ok.then(|| s[from..end].iter().collect::<String>())
    };
    strip(s, at, "video/").and_then(tail).or_else(|| tail(at))
}

/// `RE_VALTOWN`: `^https:\/\/(?:www\.)?val\.town\/(v|embed)\/[a-zA-Z_$]
/// [0-9a-zA-Z_$]+\.[a-zA-Z_$][0-9a-zA-Z_$]+`: the match and whether the
/// group is `embed`.
fn match_valtown(s: &[char]) -> Option<(String, bool)> {
    let at = strip(s, 0, "https://")?;
    let at = strip(s, at, "www.").unwrap_or(at);
    let at = strip(s, at, "val.town/")?;
    let (at, is_embed) = match strip(s, at, "v") {
        Some(after) => (after, false),
        None => (strip(s, at, "embed")?, true),
    };
    let at = strip(s, at, "/")?;
    let ident_start = |c: char| c.is_ascii_alphabetic() || c == '_' || c == '$';
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '$';
    let identifier = |from: usize| {
        if !s.get(from).is_some_and(|c| ident_start(*c)) {
            return None;
        }
        let end = run(s, from + 1, ident);
        (end > from + 1).then_some(end)
    };
    let at = identifier(at)?;
    let at = strip(s, at, ".")?;
    let end = identifier(at)?;
    Some((s[..end].iter().collect(), is_embed))
}

/// `RE_TWITTER.test(link)`: `(?:https?:\/\/)?(?:(?:w){3}\.)?(?:twitter|x)
/// \.com\/[^/]+\/status\/(\d+)`, anywhere in the link. The optional
/// prefixes cannot make a match where there is none without them.
fn is_twitter(s: &[char]) -> bool {
    (0..s.len()).any(|i| {
        let Some(at) = strip(s, i, "twitter").or_else(|| strip(s, i, "x")) else {
            return false;
        };
        let Some(at) = strip(s, at, ".com/") else {
            return false;
        };
        let end = run(s, at, |c| c != '/');
        if end == at {
            return false;
        }
        let Some(at) = strip(s, end, "/status/") else {
            return false;
        };
        run(s, at, |c| c.is_ascii_digit()) > at
    })
}

/// `RE_REDDIT.test(link)`: `^(?:http(?:s)?:\/\/)?(?:www\.)?reddit\.com\/r\/
/// ([a-zA-Z0-9_]+)\/comments\/([a-zA-Z0-9_]+)\/([a-zA-Z0-9_]+)\/?
/// (?:\?[^#\s]*)?(?:#[^\s]*)?$`.
fn is_reddit(s: &[char]) -> bool {
    let word = |from: usize| {
        let end = run(s, from, is_word);
        (end > from).then_some(end)
    };
    let check = || -> Option<()> {
        let at = http_www(s);
        let at = strip(s, at, "reddit.com/r/")?;
        let at = strip(s, word(at)?, "/comments/")?;
        let at = strip(s, word(at)?, "/")?;
        let mut at = word(at)?;
        at = strip(s, at, "/").unwrap_or(at);
        if let Some(after) = strip(s, at, "?") {
            at = run(s, after, |c| c != '#' && !is_js_space(c));
        }
        if let Some(after) = strip(s, at, "#") {
            at = run(s, after, |c| !is_js_space(c));
        }
        (at == s.len()).then_some(())
    };
    check().is_some()
}

/// `RE_GH_GIST.test(link)`: `^https:\/\/gist\.github\.com\/([\w_-]+)\/
/// ([\w_-]+)`.
fn is_gist(s: &[char]) -> bool {
    let Some(at) = strip(s, 0, "https://gist.github.com/") else {
        return false;
    };
    let end = run(s, at, is_id_char);
    end > at && strip(s, end, "/").is_some_and(|at| run(s, at, is_id_char) > at)
}

/// `parseInt(digits, 10)` of a run of ASCII digits.
fn parse_digits(digits: &str) -> f64 {
    digits.parse().unwrap_or(0.0)
}

/// `parseYouTubeLikeTimestamp(url)` (`embeddable.ts:59-86`): the `t` or
/// `start` query parameter in seconds (`90`, or `1h2m3s` and its parts),
/// 0 when there is none or it reads as neither.
pub fn parse_youtube_like_timestamp(url: &str) -> f64 {
    let absolute = if url.starts_with("http") {
        url.to_owned()
    } else {
        format!("https://{url}")
    };
    let time_param = match whatwg_url::parse(&absolute) {
        Some(parsed) => parsed
            .search_params_get("t")
            .filter(|t| !t.is_empty())
            .or_else(|| parsed.search_params_get("start")),
        // url.match(/[?&#](?:t|start)=([^&#\s]+)/)?.[1]
        None => {
            let s: Vec<char> = url.chars().collect();
            (0..s.len()).find_map(|i| {
                if !matches!(s[i], '?' | '&' | '#') {
                    return None;
                }
                let at = strip(&s, i + 1, "t=").or_else(|| strip(&s, i + 1, "start="))?;
                let end = run(&s, at, |c| !matches!(c, '&' | '#') && !is_js_space(c));
                (end > at).then(|| s[at..end].iter().collect::<String>())
            })
        }
    };
    let Some(time_param) = time_param.filter(|t| !t.is_empty()) else {
        return 0.0;
    };
    if time_param.chars().all(|c| c.is_ascii_digit()) {
        return parse_digits(&time_param);
    }
    // /^(?:(\d+)h)?(?:(\d+)m)?(?:(\d+)s)?$/
    let s: Vec<char> = time_param.chars().collect();
    let mut at = 0;
    let mut part = |unit: char| {
        let end = run(&s, at, |c| c.is_ascii_digit());
        if end > at && s.get(end) == Some(&unit) {
            let value = parse_digits(&s[at..end].iter().collect::<String>());
            at = end + 1;
            value
        } else {
            0.0
        }
    };
    let hours = part('h');
    let minutes = part('m');
    let seconds = part('s');
    if at != s.len() {
        return 0.0;
    }
    hours * 3600.0 + minutes * 60.0 + seconds
}

/// A Google Drive video link (`parseGoogleDriveVideoLink`,
/// `embeddable.ts:88-131`).
struct DriveVideo {
    file_id: String,
    resource_key: Option<String>,
    timestamp: Option<f64>,
}

fn is_drive_id(s: &str) -> bool {
    !s.is_empty() && s.chars().all(is_id_char)
}

fn parse_google_drive_video_link(url: &str) -> Option<DriveVideo> {
    let absolute = if url.starts_with("http") {
        url.to_owned()
    } else {
        format!("https://{url}")
    };
    let parsed = whatwg_url::parse(&absolute)?;
    let hostname = parsed.hostname();
    if hostname.strip_prefix("www.").unwrap_or(hostname) != "drive.google.com" {
        return None;
    }
    let pathname = parsed.pathname();
    // /^\/file\/d\/([^/]+)(?:\/|$)/
    let from_path = pathname
        .strip_prefix("/file/d/")
        .map(|rest| rest.split('/').next().unwrap_or(""))
        .filter(|id| !id.is_empty());
    let file_id = match from_path {
        Some(id) => Some(id.to_owned()),
        None if pathname == "/open" || pathname == "/uc" => parsed.search_params_get("id"),
        None => None,
    }?;
    if !is_drive_id(&file_id) {
        return None;
    }
    let resource_key = parsed
        .search_params_get("resourcekey")
        .filter(|k| is_drive_id(k));
    // parseYouTubeLikeTimestamp(urlObj.toString()): the same query
    let timestamp = parse_youtube_like_timestamp(&absolute);
    Some(DriveVideo {
        file_id,
        resource_key,
        timestamp: (timestamp > 0.0).then_some(timestamp),
    })
}

/// `encodeURIComponent(s)`: every UTF-8 byte of `s` percent-encoded except
/// ASCII letters, digits and `-_.!~*'()`.
fn encode_uri_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(s: &str) -> Option<String> {
        get_embed_link(s).and_then(|l| l.link)
    }

    fn kind(s: &str) -> EmbedType {
        get_embed_link(s).unwrap().kind
    }

    #[test]
    fn youtube() {
        assert_eq!(
            link("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=1m30s").as_deref(),
            Some("https://www.youtube.com/embed/dQw4w9WgXcQ?enablejsapi=1&start=90")
        );
        assert_eq!(
            link("youtube.com/shorts/xyz_12-3?start=42").as_deref(),
            Some("https://www.youtube.com/embed/xyz_12-3?enablejsapi=1&start=42")
        );
        assert_eq!(
            get_embed_link("youtube.com/shorts/x")
                .unwrap()
                .intrinsic_size,
            [315.0, 560.0]
        );
        assert_eq!(
            link("https://youtube.com/playlist?list=PL123").as_deref(),
            Some("https://www.youtube.com/embed/videoseries?list=PL123&enablejsapi=1")
        );
        // "embed/" comes first: the videoseries alternative is never taken
        assert_eq!(
            link("https://youtube.com/embed/videoseries?list=PL1").as_deref(),
            Some("https://www.youtube.com/embed/videoseries?enablejsapi=1")
        );
        assert_eq!(
            link("https://youtu.be/abc?t=5").as_deref(),
            Some("https://www.youtube.com/embed/abc?enablejsapi=1&start=5")
        );
        // no alternative with an id after it: the path segment is the id
        assert_eq!(
            link("https://youtube.com/watch?x=1").as_deref(),
            Some("https://www.youtube.com/embed/watch?enablejsapi=1")
        );
        assert_eq!(kind("https://youtu.be/abc"), EmbedType::Video);
    }

    #[test]
    fn vimeo_drive_figma_valtown_forms() {
        assert_eq!(
            link("https://vimeo.com/12345").as_deref(),
            Some("https://player.vimeo.com/video/12345?api=1")
        );
        assert_eq!(
            link("https://www.player.vimeo.com/video/7?x=1").as_deref(),
            Some("https://player.vimeo.com/video/7?api=1")
        );
        assert_eq!(
            link("https://drive.google.com/file/d/abc_DEF-123/view?resourcekey=key-1&t=90")
                .as_deref(),
            Some("https://drive.google.com/file/d/abc_DEF-123/preview?resourcekey=key-1&t=90")
        );
        assert_eq!(
            link("drive.google.com/open?id=X1").as_deref(),
            Some("https://drive.google.com/file/d/X1/preview")
        );
        assert_eq!(
            link("https://www.figma.com/file/abc").as_deref(),
            Some("https://www.figma.com/embed?embed_host=share&url=https%3A%2F%2Fwww.figma.com%2Ffile%2Fabc")
        );
        assert_eq!(
            link("https://www.val.town/v/user.fn").as_deref(),
            Some("https://www.val.town/embed/user.fn")
        );
        // the first "/v" is in "//val"
        assert_eq!(
            link("https://val.town/v/user.fn").as_deref(),
            Some("https://embedal.town/v/user.fn")
        );
        assert_eq!(
            link("https://forms.microsoft.com/r/abc").as_deref(),
            Some("https://forms.microsoft.com/r/abc?embed=true")
        );
        assert_eq!(
            link("https://forms.microsoft.com/r/abc?x=1").as_deref(),
            Some("https://forms.microsoft.com/r/abc?x=1&embed=true")
        );
    }

    #[test]
    fn documents_and_generic_links() {
        assert_eq!(
            kind("https://twitter.com/excalidraw/status/1234567890"),
            EmbedType::Document
        );
        assert_eq!(kind("see x.com/a/status/1 please"), EmbedType::Document);
        assert_eq!(
            kind("https://www.reddit.com/r/excalidraw/comments/abc123/some_title/"),
            EmbedType::Document
        );
        assert_eq!(
            kind("https://gist.github.com/user/0123abcd"),
            EmbedType::Document
        );
        assert_eq!(
            link("https://example.com/page?x=1").as_deref(),
            Some("https://example.com/page?x=1")
        );
        assert_eq!(link("about:blank").as_deref(), Some("about:blank"));
        assert_eq!(get_embed_link(""), None);
    }

    #[test]
    fn timestamps() {
        assert_eq!(parse_youtube_like_timestamp("youtu.be/x?t=1h2m3s"), 3723.0);
        assert_eq!(parse_youtube_like_timestamp("youtu.be/x?t=&start=7"), 7.0);
        assert_eq!(parse_youtube_like_timestamp("youtu.be/x?t=2x"), 0.0);
        assert_eq!(parse_youtube_like_timestamp("youtu.be/x"), 0.0);
        // not a URL: the query read by the fallback expression
        assert_eq!(parse_youtube_like_timestamp("http://[?t=12"), 12.0);
    }
}
