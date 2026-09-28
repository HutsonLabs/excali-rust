//! `isElementLink(url)` (`packages/element/src/elementLink.ts:82-92`),
//! which picks the link icon the static scene draws
//! (`renderer/staticScene.ts:226-228`): `new URL(url)`, then
//! `searchParams.has("element")` and `host === window.location.host`.
//! Expectations are Node 26's `new URL` on each input.

use excali_core::link::{is_element_link, ELEMENT_LINK_KEY};

#[test]
fn element_links() {
    assert_eq!(ELEMENT_LINK_KEY, "element");
    let cases = [
        (
            "https://excalidraw.com/?element=abc",
            "excalidraw.com",
            true,
        ),
        // the fragment is not the query
        (
            "https://excalidraw.com/#json=1&element=abc",
            "excalidraw.com",
            false,
        ),
        ("https://example.com/?element=abc", "excalidraw.com", false),
        (
            "https://excalidraw.com/?elements=abc",
            "excalidraw.com",
            false,
        ),
        (
            "https://excalidraw.com/?a=1&element",
            "excalidraw.com",
            true,
        ),
        // names are percent-decoded, `+` is a space
        (
            "https://excalidraw.com/?el%65ment=x",
            "excalidraw.com",
            true,
        ),
        (
            "https://excalidraw.com/?element+=x",
            "excalidraw.com",
            false,
        ),
        (
            "https://excalidraw.com/?+element=x",
            "excalidraw.com",
            false,
        ),
        // hosts are lower-cased, default ports dropped, others kept
        ("https://EXCALIDRAW.com/?element=x", "excalidraw.com", true),
        (
            "https://excalidraw.com:443/?element=x",
            "excalidraw.com",
            true,
        ),
        ("http://localhost:3000/?element=x", "localhost:3000", true),
        ("http://localhost:3000/?element=x", "localhost", false),
        // not a URL
        ("/?element=x", "excalidraw.com", false),
        ("not a url", "excalidraw.com", false),
    ];
    for (url, host, expected) in cases {
        assert_eq!(is_element_link(url, host), expected, "{url} on {host}");
    }
}
