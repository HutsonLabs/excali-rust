//! The scene font selection the web runtime exposes as `sceneFontFiles`
//! (ex-307), natively: a `.excalidraw` document in, the font files its
//! non-deleted text needs out. The browser suite (`tests/web`) checks the
//! same list against the requests Chromium makes.

use excali_wasm::{scene_elements, scene_font_file_list};

fn text(id: &str, family: u32, text: &str, deleted: bool) -> String {
    format!(
        r##"{{"id":"{id}","type":"text","x":0,"y":0,"width":10,"height":25,"angle":0,
        "strokeColor":"#1e1e1e","backgroundColor":"transparent","fillStyle":"solid",
        "strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"groupIds":[],
        "frameId":null,"index":null,"roundness":null,"seed":1,"version":1,"versionNonce":0,
        "isDeleted":{deleted},"boundElements":null,"updated":1,"link":null,"locked":false,
        "text":{t},"fontSize":20,"fontFamily":{family},"textAlign":"left",
        "verticalAlign":"top","containerId":null,"originalText":{t},"autoResize":true,
        "lineHeight":1.25}}"##,
        t = serde_json_string(text),
    )
}

fn serde_json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn doc(elements: &[String]) -> String {
    format!(
        r#"{{"type":"excalidraw","version":2,"source":"test","elements":[{}],"appState":{{}},"files":{{}}}}"#,
        elements.join(",")
    )
}

#[test]
fn a_latin_excalifont_scene_needs_one_file() {
    let files = scene_font_file_list(&doc(&[text("a", 5, "Hello", false)])).unwrap();
    assert_eq!(
        files,
        ["Excalifont/Excalifont-Regular-a88b72a24fb54c9f94e3b5fdaa7481c9.woff2"]
    );
}

#[test]
fn deleted_text_and_local_families_need_nothing() {
    let files = scene_font_file_list(&doc(&[
        text("a", 5, "你好", true),
        text("b", 2, "Helvetica", false),
    ]))
    .unwrap();
    assert!(files.is_empty(), "{files:?}");
}

#[test]
fn cjk_in_excalifont_adds_only_the_xiaolai_files_holding_it() {
    let files = scene_font_file_list(&doc(&[text("a", 5, "你", false)])).unwrap();
    assert_eq!(files.len(), 1, "{files:?}");
    assert!(files[0].starts_with("Xiaolai/"));
}

#[test]
fn several_families_list_each_file_once_in_load_order() {
    let files = scene_font_file_list(&doc(&[
        text("a", 8, "code", false),
        text("b", 5, "hand", false),
        text("c", 8, "more code", false),
        text("d", 9, "Liberation", false),
    ]))
    .unwrap();
    assert_eq!(
        files,
        [
            "ComicShanns/ComicShanns-Regular-279a7b317d12eb88de06167bd672b4b4.woff2",
            "Excalifont/Excalifont-Regular-a88b72a24fb54c9f94e3b5fdaa7481c9.woff2",
            "Liberation/LiberationSans-Regular.ttf",
        ]
    );
}

#[test]
fn documents_without_elements_and_invalid_json() {
    assert!(scene_font_file_list(r#"{"type":"excalidraw","version":2}"#)
        .unwrap()
        .is_empty());
    assert!(scene_elements("not json").is_err());
    assert!(scene_font_file_list("[]").is_err());
}
