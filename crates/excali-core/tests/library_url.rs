//! Importing a library from a URL (`site/content/research/data-model.md`,
//! section 4, "Import from a URL"): the allow-list check
//! `validateLibraryUrl` and the `#addLibrary` token parser
//! `parseLibraryTokensFromUrl` (`packages/excalidraw/data/library.ts:54-58,
//! 497-543`), the link sanitizers `normalizeLink` and `toValidURL`
//! (`packages/common/src/url.ts:5-37`) and the first steps of
//! `importLibraryFromURL` (`library.ts:726-731`), against upstream's own
//! functions at the pinned commit, recorded by
//! `tools/goldens/library-url-fixtures.mjs` in `tests/fixtures/library-url.json`.

use excali_core::library_url::{
    decode_uri_component, library_url_after_import, parse_library_tokens,
    parse_library_tokens_from_url, resolve_library_url, resolve_library_url_with,
    validate_library_url, validate_library_url_with, LibraryUrlError, LibraryUrlTokens,
    LibraryUrlValidator, ALLOWED_LIBRARY_URLS, URL_HASH_KEY_ADD_LIBRARY, URL_QUERY_KEY_ADD_LIBRARY,
};
use excali_core::link::{escape_double_quotes, normalize_link, sanitize_url, to_valid_url};
use serde_json::Value;

const FIXTURE: &str = include_str!("fixtures/library-url.json");

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).expect("library-url.json parses")
}

fn cases<'a>(fixture: &'a Value, table: &str) -> &'a [Value] {
    let cases = fixture[table].as_array().expect(table);
    assert!(!cases.is_empty(), "{table} is empty");
    cases
}

fn str_of<'a>(case: &'a Value, key: &str) -> &'a str {
    case[key]
        .as_str()
        .unwrap_or_else(|| panic!("{}: {key}", case["id"]))
}

fn allow_list(case: &Value) -> Option<Vec<&str>> {
    case.get("allowList").map(|list| {
        list.as_array()
            .expect("allowList")
            .iter()
            .map(|entry| entry.as_str().expect("entry"))
            .collect()
    })
}

/// The recorded outcome (`ok` or the thrown message and constructor) matches.
fn assert_outcome(case: &Value, result: &Result<(), LibraryUrlError>) {
    let id = &case["id"];
    match (case.get("error"), result) {
        (None, Ok(())) => assert_eq!(case["ok"], true, "{id}"),
        (Some(error), Err(actual)) => {
            assert_eq!(actual.to_string(), error.as_str().unwrap(), "{id}");
            assert_eq!(actual.js_error_type(), str_of(case, "errorType"), "{id}");
        }
        (expected, actual) => panic!("{id}: expected {expected:?}, got {actual:?}"),
    }
}

#[test]
fn allow_list_is_upstreams() {
    let fixture = fixture();
    let recorded: Vec<&str> = fixture["allowedLibraryUrls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(ALLOWED_LIBRARY_URLS.as_slice(), recorded.as_slice());
    assert_eq!(
        ALLOWED_LIBRARY_URLS,
        [
            "excalidraw.com",
            "raw.githubusercontent.com/excalidraw/excalidraw-libraries"
        ]
    );
    // packages/common/src/constants.ts:376-382
    assert_eq!(URL_HASH_KEY_ADD_LIBRARY, "addLibrary");
    assert_eq!(URL_QUERY_KEY_ADD_LIBRARY, "addLibrary");
}

#[test]
fn validate_library_url_matches_upstream() {
    let fixture = fixture();
    let cases = cases(&fixture, "validate");
    for case in cases {
        let url = str_of(case, "url");
        let result = match allow_list(case) {
            None => validate_library_url(url),
            Some(list) => validate_library_url_with(url, &LibraryUrlValidator::AllowList(&list)),
        };
        assert_outcome(case, &result);
    }
    // Both outcomes and every kind of error are covered.
    for kind in ["Error", "TypeError", "SyntaxError"] {
        assert!(
            cases.iter().any(|c| c["errorType"] == kind),
            "no {kind} case"
        );
    }
    assert!(cases.iter().filter(|c| c["ok"] == true).count() > 20);
}

#[test]
fn acceptance_default_allow_list() {
    // excalidraw.com: host suffix on a subdomain boundary, any path.
    for ok in [
        "https://excalidraw.com",
        "https://libraries.excalidraw.com/libraries/a.excalidrawlib",
        "http://a.b.excalidraw.com/x",
    ] {
        assert_eq!(validate_library_url(ok), Ok(()), "{ok}");
    }
    // raw.githubusercontent.com: host suffix, path prefix on a segment boundary.
    for ok in [
        "https://raw.githubusercontent.com/excalidraw/excalidraw-libraries",
        "https://raw.githubusercontent.com/excalidraw/excalidraw-libraries/main/libraries/a.excalidrawlib",
        "https://x.raw.githubusercontent.com/excalidraw/excalidraw-libraries//a",
    ] {
        assert_eq!(validate_library_url(ok), Ok(()), "{ok}");
    }
    for rejected in [
        "https://notexcalidraw.com/x",
        "https://excalidraw.com.evil.com/x",
        "https://evil.com/excalidraw.com",
        "https://raw.githubusercontent.com/excalidraw/excalidraw-libraries-evil/x",
        "https://raw.githubusercontent.com/excalidraw/excalidraw/x",
        "https://raw.githubusercontent.com/excalidraw/excalidraw-libraries/../../evil/x",
        "about:blank",
    ] {
        assert_eq!(
            validate_library_url(rejected),
            Err(LibraryUrlError::Disallowed {
                url: rejected.to_owned()
            }),
            "{rejected}"
        );
        assert_eq!(
            validate_library_url(rejected).unwrap_err().to_string(),
            format!("Invalid or disallowed library URL: \"{rejected}\"")
        );
    }
    assert_eq!(
        validate_library_url("excalidraw.com/x"),
        Err(LibraryUrlError::InvalidUrl)
    );
}

/// Where the `url` crate parses differently from `new URL` (Node 26, ada),
/// the port answers as upstream does (each case is also in the fixture).
#[test]
fn url_parsing_is_new_urls() {
    let disallowed = |url: &str| {
        Err(LibraryUrlError::Disallowed {
            url: url.to_owned(),
        })
    };
    // file: keeps the empty segment a backslash or slash leaves after the
    // host, so the path is not the allowed prefix.
    for url in [
        "file://raw.githubusercontent.com/\\excalidraw/excalidraw-libraries/x",
        "file://raw.githubusercontent.com//excalidraw/excalidraw-libraries/x",
    ] {
        assert_eq!(validate_library_url(url), disallowed(url), "{url}");
    }
    let list = ["excalidraw.com/x"];
    let url = "file://excalidraw.com/\\x";
    assert_eq!(
        validate_library_url_with(url, &LibraryUrlValidator::AllowList(&list)),
        disallowed(url)
    );
    assert_eq!(
        validate_library_url("file://raw.githubusercontent.com/excalidraw/excalidraw-libraries/x"),
        Ok(())
    );
    // and keeps its host next to a Windows drive letter.
    assert_eq!(validate_library_url("file://excalidraw.com/C:/x"), Ok(()));

    // An ASCII host UTS 46 turns down (`xn--`) is a host, as in ada: the URL
    // is disallowed, not invalid, and allowed under an allowed domain.
    for scheme in ["https", "http", "ftp", "ws", "file", "web+lib"] {
        let url = format!("{scheme}://xn--/excalidraw/excalidraw-libraries/x");
        let result = validate_library_url(&url);
        assert_eq!(result, disallowed(&url), "{url}");
        assert_eq!(result.unwrap_err().js_error_type(), "Error");
    }
    assert_eq!(
        validate_library_url("https://xn--.excalidraw.com/x"),
        Ok(())
    );
    assert_eq!(
        validate_library_url("https://xn--%C3%A4.excalidraw.com/x"),
        Err(LibraryUrlError::InvalidUrl)
    );

    // Credentials with no host: `new URL` fails, so toValidURL is
    // about:blank and the import names it.
    for link in ["x://@", "web+a://u@"] {
        assert_eq!(to_valid_url(link, "https://excalidraw.com"), "about:blank");
        assert_eq!(
            resolve_library_url(link, "https://excalidraw.com"),
            Err(LibraryUrlError::Disallowed {
                url: "about:blank".to_owned()
            }),
            "{link}"
        );
    }
}

#[test]
fn validate_library_url_with_a_predicate() {
    // library.ts:497-507: a function validator decides alone.
    let seen = std::cell::RefCell::new(Vec::new());
    let only_example = |url: &str| {
        seen.borrow_mut().push(url.to_owned());
        url.starts_with("https://example.com/")
    };
    let validator = LibraryUrlValidator::Predicate(&only_example);
    assert_eq!(
        validate_library_url_with("https://example.com/a", &validator),
        Ok(())
    );
    assert_eq!(
        validate_library_url_with("https://excalidraw.com/a", &validator),
        Err(LibraryUrlError::Disallowed {
            url: "https://excalidraw.com/a".into()
        })
    );
    // Not even parsed: an invalid URL is the predicate's call.
    assert_eq!(
        validate_library_url_with("not a url", &LibraryUrlValidator::Predicate(&|_| true)),
        Ok(())
    );
    assert_eq!(
        *seen.borrow(),
        ["https://example.com/a", "https://excalidraw.com/a"]
    );
    assert_eq!(
        validate_library_url_with("https://excalidraw.com/a", &LibraryUrlValidator::default()),
        Ok(())
    );
}

#[test]
fn parse_library_tokens_matches_upstream() {
    let fixture = fixture();
    let cases = cases(&fixture, "tokens");
    for case in cases {
        let id = &case["id"];
        let expected = match &case["result"] {
            Value::Null => None,
            result => Some(LibraryUrlTokens {
                library_url: str_of(result, "libraryUrl").to_owned(),
                id_token: result["idToken"].as_str().map(str::to_owned),
            }),
        };
        let (search, hash) = (str_of(case, "search"), str_of(case, "hash"));
        assert_eq!(parse_library_tokens(search, hash), expected, "{id}");
        assert_eq!(
            parse_library_tokens_from_url(str_of(case, "href")),
            expected,
            "{id} (href)"
        );
    }
    assert!(cases.iter().any(|c| c["result"].is_null()));
    assert!(cases
        .iter()
        .any(|c| c["result"]["idToken"].is_null() && !c["result"].is_null()));
}

#[test]
fn acceptance_hash_and_legacy_query() {
    let tokens = |library_url: &str, id_token: Option<&str>| {
        Some(LibraryUrlTokens {
            library_url: library_url.into(),
            id_token: id_token.map(Into::into),
        })
    };
    assert_eq!(
        parse_library_tokens_from_url(
            "https://excalidraw.com/#addLibrary=https%3A%2F%2Flibraries.excalidraw.com%2Fa.excalidrawlib&token=abc"
        ),
        tokens("https://libraries.excalidraw.com/a.excalidrawlib", Some("abc"))
    );
    // legacy: the query string, the token still only from the hash
    assert_eq!(
        parse_library_tokens("?addLibrary=u&token=q", ""),
        tokens("u", None)
    );
    assert_eq!(
        parse_library_tokens("?addLibrary=u", "#token=t"),
        tokens("u", Some("t"))
    );
    // the hash wins; an empty hash value falls back to the query
    assert_eq!(
        parse_library_tokens("?addLibrary=q", "#addLibrary=h"),
        tokens("h", None)
    );
    assert_eq!(
        parse_library_tokens("?addLibrary=q", "#addLibrary="),
        tokens("q", None)
    );
    assert_eq!(parse_library_tokens("", "#token=t"), None);
    assert_eq!(parse_library_tokens("", ""), None);
    assert_eq!(parse_library_tokens_from_url("not a url"), None);
}

#[test]
fn should_prompt_unless_the_token_is_the_editors_id() {
    // library.ts:747: `idToken !== excalidrawAPI.id`
    let with = |id_token: Option<&str>| LibraryUrlTokens {
        library_url: "u".into(),
        id_token: id_token.map(Into::into),
    };
    assert!(!with(Some("editor-1")).should_prompt("editor-1"));
    assert!(with(Some("editor-2")).should_prompt("editor-1"));
    assert!(with(None).should_prompt("editor-1"));
    assert!(with(Some("")).should_prompt("editor-1"));
}

#[test]
fn url_after_import_matches_upstream() {
    // library.ts:765-776, the `finally` block: the `addLibrary` key removed
    // from the hash when the hash contains the text, else from the query.
    // Expected values are what URLSearchParams gives in Node 26.
    let table: [(&str, &str, Option<&str>); 9] = [
        ("", "#addLibrary=u&token=t", Some("#token=t")),
        ("", "#addLibrary=u", Some("#")),
        ("?addLibrary=u&x=1", "#token=t", Some("?x=1")),
        ("?a=1", "#room=r", None),
        (
            "",
            "#fooaddLibraryBar=1&a=b%20c",
            Some("#fooaddLibraryBar=1&a=b+c"),
        ),
        (
            "",
            "#token=a+b%20c&addLibrary=1&addLibrary=2&k",
            Some("#token=a+b+c&k="),
        ),
        ("?addLibrary=q", "#addLibrary=h", Some("#")),
        ("", "#?addLibrary=u&z=%C3%A9*~", Some("#z=%C3%A9*%7E")),
        ("", "", None),
    ];
    for (search, hash, expected) in table {
        assert_eq!(
            library_url_after_import(search, hash).as_deref(),
            expected,
            "{search} {hash}"
        );
    }
}

#[test]
fn normalize_link_matches_upstream() {
    let fixture = fixture();
    for case in cases(&fixture, "normalizeLink") {
        assert_eq!(
            normalize_link(str_of(case, "input")),
            str_of(case, "output"),
            "{}",
            case["id"]
        );
    }
}

#[test]
fn sanitize_url_basics() {
    // @braintree/sanitize-url 6.0.2 on its own, and escapeDoubleQuotes
    // (packages/common/src/utils.ts:1134-1136).
    assert_eq!(sanitize_url(""), "about:blank");
    assert_eq!(sanitize_url("  "), "about:blank");
    assert_eq!(sanitize_url(" https://a.b/ "), "https://a.b/");
    assert_eq!(sanitize_url("javascript:x"), "about:blank");
    // `&#x;` decodes to U+0000 (Number("x") is NaN), which is then removed.
    assert_eq!(sanitize_url("&#x;"), "about:blank");
    assert_eq!(sanitize_url("&#;"), "&#;");
    assert_eq!(escape_double_quotes(r#"a"b""#), "a&quot;b&quot;");
}

#[test]
fn to_valid_url_matches_upstream() {
    let fixture = fixture();
    for case in cases(&fixture, "toValidURL") {
        assert_eq!(
            to_valid_url(str_of(case, "input"), str_of(case, "origin")),
            str_of(case, "output"),
            "{}",
            case["id"]
        );
    }
}

#[test]
fn import_steps_match_upstream() {
    let fixture = fixture();
    let cases = cases(&fixture, "import");
    for case in cases {
        let id = &case["id"];
        let (input, origin) = (str_of(case, "input"), str_of(case, "origin"));
        let result = resolve_library_url(input, origin);
        match (case.get("url"), &result) {
            (Some(url), Ok(actual)) => assert_eq!(actual, url.as_str().unwrap(), "{id}"),
            (None, Err(error)) => {
                assert_eq!(error.to_string(), str_of(case, "error"), "{id}");
                assert_eq!(error.js_error_type(), str_of(case, "errorType"), "{id}");
            }
            (expected, actual) => panic!("{id}: expected {expected:?}, got {actual:?}"),
        }
        assert_eq!(
            resolve_library_url_with(input, origin, &LibraryUrlValidator::default()),
            result,
            "{id}"
        );
    }
    assert!(cases.iter().any(|c| c["errorType"] == "URIError"));
    // A caller's validator replaces the allow-list.
    assert_eq!(
        resolve_library_url_with(
            "https%3A%2F%2Fexample.com%2Fa",
            "https://app.test",
            &LibraryUrlValidator::AllowList(&["example.com"])
        ),
        Ok("https://example.com/a".to_owned())
    );
}

#[test]
fn decode_uri_component_is_javascripts() {
    assert_eq!(
        decode_uri_component("a%20b%2Fc+d").as_deref(),
        Ok("a b/c+d")
    );
    assert_eq!(decode_uri_component("%e2%82%ac").as_deref(), Ok("\u{20ac}"));
    assert_eq!(
        decode_uri_component("%F0%9F%98%80").as_deref(),
        Ok("\u{1f600}")
    );
    for malformed in [
        "%",
        "%4",
        "%g0",
        "%80",
        "%C3",
        "%C3%28",
        "%E0%80%80",
        "%ED%BF%BF",
        "%F5%80%80%80",
        "%FF",
    ] {
        assert_eq!(
            decode_uri_component(malformed),
            Err(LibraryUrlError::UriMalformed),
            "{malformed}"
        );
    }
    assert_eq!(LibraryUrlError::UriMalformed.to_string(), "URI malformed");
    assert_eq!(LibraryUrlError::UriMalformed.js_error_type(), "URIError");
}
