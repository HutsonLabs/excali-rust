//! The `excali` command line: arguments, exit codes and what each command
//! writes. The loading, export and library behaviour is upstream's
//! (`loadSceneOrLibraryFromBlob`, `exportCanvas`, `parseLibraryJSON`,
//! `mergeLibraryItems`, `serializeLibraryAsJSON`); the tests pin the CLI's
//! use of it against upstream's fixtures and excali-scene's canvas export
//! golden.

mod support;

use serde_json::{json, Value};
use support::*;

// Exit codes (documented in site/content/architecture/cli.md).
const OK: i32 = 0;
const INVALID: i32 = 1;
const USAGE: i32 = 2;
const IO: i32 = 3;
const EXPORT: i32 = 4;

// ---------------------------------------------------------------------------
// Arguments

#[test]
fn version_prints_the_workspace_version() {
    let out = excali(&["--version"]);
    assert_code(&out, OK);
    assert_eq!(
        stdout(&out).trim(),
        format!("excali {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn help_lists_the_commands() {
    let out = excali(&["--help"]);
    assert_code(&out, OK);
    let text = stdout(&out);
    assert!(text.starts_with("Usage: excali"), "{text}");
    for command in ["validate", "render", "export", "lib"] {
        assert!(
            text.contains(&format!("  {command} ")),
            "{command} in\n{text}"
        );
    }
    let lib = stdout(&excali(&["lib", "--help"]));
    for command in ["list", "merge"] {
        assert!(
            lib.contains(&format!("  {command} ")),
            "{command} in\n{lib}"
        );
    }
}

#[test]
fn unknown_argument_is_a_usage_error() {
    let out = excali(&["--frobnicate"]);
    assert_code(&out, USAGE);
    assert!(stderr(&out).contains("--frobnicate"));
}

#[test]
fn no_arguments_is_a_usage_error() {
    assert_code(&excali::<&str>(&[]), USAGE);
}

#[test]
fn bad_option_values_are_usage_errors() {
    let scene = upstream_fixture("smiley_embedded_v2.png");
    for bad in [
        ["--scale", "0"],
        ["--scale", "-1"],
        ["--scale", "x"],
        ["--padding", "NaN"],
    ] {
        let out = excali(&[
            "render".as_ref(),
            scene.as_os_str(),
            bad[0].as_ref(),
            bad[1].as_ref(),
        ]);
        assert_code(&out, USAGE);
    }
}

// ---------------------------------------------------------------------------
// validate

fn validate_json(path: &std::path::Path) -> (i32, Value) {
    let out = excali(&["validate".as_ref(), "--json".as_ref(), path.as_os_str()]);
    let report: Value = serde_json::from_str(&stdout(&out))
        .unwrap_or_else(|e| panic!("{e}: {}\n{}", stdout(&out), stderr(&out)));
    (out.status.code().unwrap(), report[0].clone())
}

#[test]
fn validate_reads_the_scenes_upstream_imports() {
    // export.test.tsx: "import embedded png (legacy v1)", "(v2)", and the
    // SVG equivalents: each holds one text element.
    for name in [
        "test_embedded_v1.png",
        "smiley_embedded_v2.png",
        "test_embedded_v1.svg",
        "smiley_embedded_v2.svg",
    ] {
        let (code, report) = validate_json(&upstream_fixture(name));
        assert_eq!(code, OK, "{name}: {report}");
        assert_eq!(report["kind"], "scene", "{name}");
        assert_eq!(report["elements"], 1, "{name}");
        assert_eq!(report["types"], json!({"text": 1}), "{name}");
    }
}

#[test]
fn validate_reports_the_texts_of_the_embedded_scenes() {
    let (_, v1) = validate_json(&upstream_fixture("test_embedded_v1.png"));
    assert_eq!(v1["texts"], json!(["test"]));
    let (_, v2) = validate_json(&upstream_fixture("smiley_embedded_v2.svg"));
    assert_eq!(v2["texts"], json!(["\u{1F600}"]));
}

#[test]
fn validate_reads_libraries() {
    let (code, report) = validate_json(&upstream_fixture("fixture_library.excalidrawlib"));
    assert_eq!(code, OK, "{report}");
    assert_eq!(report["kind"], "library");
    assert_eq!(report["items"], 1);
    assert!(report["elements"].as_u64().unwrap() > 0);
}

#[test]
fn an_image_without_a_scene_is_invalid() {
    // blob.ts parseFileContents: decodePngMetadata's INVALID.
    let out = excali(&[
        "validate".as_ref(),
        upstream_fixture("smiley.png").as_os_str(),
    ]);
    assert_code(&out, INVALID);
    assert!(
        stderr(&out).contains("Image doesn't contain scene"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn a_file_that_is_no_scene_is_invalid() {
    let dir = scratch("validate-invalid");
    for (name, text) in [
        ("broken.excalidraw", "{ not json"),
        ("array.excalidraw", "[]"),
        ("other.json", r#"{"type": "something-else"}"#),
        (
            "lib-v3.excalidrawlib",
            r#"{"type": "excalidrawlib", "version": 3}"#,
        ),
    ] {
        let path = dir.join(name);
        std::fs::write(&path, text).unwrap();
        let out = excali(&["validate".as_ref(), path.as_os_str()]);
        assert_code(&out, INVALID);
        assert!(
            stderr(&out).contains("Error: invalid file"),
            "{name}: {}",
            stderr(&out)
        );
    }
}

#[test]
fn a_library_whose_items_do_not_restore_is_invalid() {
    // parseLibraryJSON: `for...of` over a number throws.
    let dir = scratch("validate-bad-items");
    let path = dir.join("bad.excalidrawlib");
    std::fs::write(
        &path,
        r#"{"type": "excalidrawlib", "version": 2, "libraryItems": 7}"#,
    )
    .unwrap();
    let out = excali(&["validate".as_ref(), path.as_os_str()]);
    assert_code(&out, INVALID);
    assert!(
        stderr(&out).contains("libraryItems is not iterable"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn a_missing_file_is_an_io_error() {
    let out = excali(&["validate", "/nonexistent/scene.excalidraw"]);
    assert_code(&out, IO);
    assert!(stderr(&out).contains("/nonexistent/scene.excalidraw"));
}

#[test]
fn validate_checks_every_file_and_fails_if_any_fails() {
    let good = upstream_fixture("smiley_embedded_v2.png");
    let bad = upstream_fixture("smiley.png");
    let out = excali(&[
        "validate".as_ref(),
        "--json".as_ref(),
        good.as_os_str(),
        bad.as_os_str(),
    ]);
    assert_code(&out, INVALID);
    let report: Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(report.as_array().unwrap().len(), 2);
    assert_eq!(report[0]["kind"], "scene");
    assert_eq!(report[1]["error"], "Image doesn't contain scene");
    // The plain report names each file.
    let plain = excali(&["validate".as_ref(), good.as_os_str(), bad.as_os_str()]);
    assert!(stdout(&plain).contains("smiley_embedded_v2.png: scene, 1 element"));
    assert!(stderr(&plain).contains("smiley.png: Image doesn't contain scene"));
}

#[test]
fn gzipped_files_are_read_through() {
    let lib = repo().join("fixtures/libraries/aarondiel/logic-gates.excalidrawlib.gz");
    let (code, report) = validate_json(&lib);
    assert_eq!(code, OK, "{report}");
    assert_eq!(report["kind"], "library");
}

// ---------------------------------------------------------------------------
// render

/// The CLI arguments that make the editor's export of a golden scene.
fn golden_args(scene: &Value) -> Vec<String> {
    let mut args = Vec::new();
    let opts = &scene["opts"];
    let app = &scene["appState"];
    if let Some(scale) = app["exportScale"].as_f64() {
        args.extend(["--scale".to_owned(), scale.to_string()]);
    }
    if opts["exportBackground"] == false {
        args.push("--no-background".to_owned());
    }
    if app["exportWithDarkMode"] == true {
        args.push("--dark".to_owned());
    }
    if let Some(padding) = opts["exportPadding"].as_f64() {
        args.push(format!("--padding={padding}"));
    }
    if let Some(frame) = opts["exportingFrame"].as_str() {
        args.extend(["--frame".to_owned(), frame.to_owned()]);
    }
    args
}

fn golden_scene_file(dir: &std::path::Path, scene: &Value) -> std::path::PathBuf {
    let mut app_state = json!({});
    app_state["viewBackgroundColor"] = scene["opts"]["viewBackgroundColor"].clone();
    scene_file(
        dir,
        &format!("{}.excalidraw", scene["name"].as_str().unwrap()),
        &scene["elements"],
        &app_state,
        &scene["files"],
    )
}

#[test]
fn render_sizes_the_canvas_as_upstream_export_does() {
    // Each scene of the golden the editor's export can express, rendered
    // from a file: the PNG is upstream's canvas size.
    let golden = canvas_export_golden();
    let dir = scratch("render-golden");
    let mut checked = 0;
    for scene in golden["scenes"].as_array().unwrap() {
        if scene["sizing"]["kind"] != "exportScale"
            || scene["appState"]["frameRendering"]["name"] == false
            || scene["appState"]["frameRendering"]["enabled"] == false
        {
            continue;
        }
        let name = scene["name"].as_str().unwrap();
        let input = golden_scene_file(&dir, scene);
        let output = dir.join(format!("{name}.png"));
        let mut args: Vec<std::ffi::OsString> = vec![
            "render".into(),
            input.into(),
            "-o".into(),
            output.clone().into(),
        ];
        args.extend(golden_args(scene).into_iter().map(Into::into));
        let out = excali(&args);
        assert_code(&out, OK);
        let png = read_png(&std::fs::read(&output).unwrap());
        assert_eq!(
            (png.width, png.height),
            (
                scene["width"].as_u64().unwrap() as u32,
                scene["height"].as_u64().unwrap() as u32
            ),
            "{name}"
        );
        checked += 1;
    }
    assert_eq!(checked, 17);
}

#[test]
fn render_paints_the_background_only_with_export_background() {
    let golden = canvas_export_golden();
    let dir = scratch("render-background");
    let scene = golden["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "background-coloured")
        .unwrap();
    let input = golden_scene_file(&dir, scene);
    let with = dir.join("with.png");
    let without = dir.join("without.png");
    let dark = dir.join("dark.png");
    assert_code(
        &excali(&[
            "render".as_ref(),
            input.as_os_str(),
            "-o".as_ref(),
            with.as_os_str(),
        ]),
        OK,
    );
    assert_code(
        &excali(&[
            "render".as_ref(),
            input.as_os_str(),
            "--no-background".as_ref(),
            "-o".as_ref(),
            without.as_os_str(),
        ]),
        OK,
    );
    assert_code(
        &excali(&[
            "render".as_ref(),
            input.as_os_str(),
            "--dark".as_ref(),
            "-o".as_ref(),
            dark.as_os_str(),
        ]),
        OK,
    );
    // #fff9db
    assert_eq!(
        read_png(&std::fs::read(&with).unwrap()).pixel(0, 0),
        [255, 249, 219, 255]
    );
    assert_eq!(
        read_png(&std::fs::read(&without).unwrap()).pixel(0, 0)[3],
        0
    );
    let [r, g, b, a] = read_png(&std::fs::read(&dark).unwrap()).pixel(0, 0);
    assert_eq!(a, 255);
    assert!(r < 64 && g < 64 && b < 64, "{r} {g} {b}");
}

fn text_scene(dir: &std::path::Path) -> std::path::PathBuf {
    let text = json!({
        "id": "hello", "type": "text", "x": 0, "y": 0, "width": 200, "height": 50,
        "angle": 0, "strokeColor": "#1e1e1e", "backgroundColor": "transparent",
        "fillStyle": "solid", "strokeWidth": 2, "strokeStyle": "solid", "roughness": 1,
        "opacity": 100, "groupIds": [], "frameId": null, "index": "a0", "roundness": null,
        "seed": 1, "version": 1, "versionNonce": 1, "isDeleted": false,
        "boundElements": null, "updated": 1, "link": null, "locked": false,
        "text": "HHHH", "fontSize": 40, "fontFamily": 6, "textAlign": "left",
        "verticalAlign": "top", "containerId": null, "originalText": "HHHH",
        "autoResize": true, "lineHeight": 1.25
    });
    scene_file(
        dir,
        "text.excalidraw",
        &json!([text]),
        &json!({}),
        &json!({}),
    )
}

#[test]
fn render_draws_text_from_the_font_files() {
    let dir = scratch("render-text");
    let input = text_scene(&dir);
    let output = dir.join("text.png");
    let out = excali(&[
        "render".as_ref(),
        input.as_os_str(),
        "--no-background".as_ref(),
        "-o".as_ref(),
        output.as_os_str(),
    ]);
    assert_code(&out, OK);
    let png = read_png(&std::fs::read(&output).unwrap());
    // Padding 10 around the 200 x 50 box.
    assert_eq!((png.width, png.height), (220, 70));
    let inked = |x0: u32, x1: u32, y0: u32, y1: u32| {
        (y0..y1)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .filter(|&(x, y)| png.pixel(x, y)[3] > 0)
            .count()
    };
    // The glyphs sit on the first line: 40 px Nunito, line height 50.
    assert!(inked(10, 210, 10, 60) > 500, "{}", inked(10, 210, 10, 60));
    // Nothing is drawn in the padding.
    assert_eq!(inked(0, 220, 0, 8), 0);
    assert_eq!(inked(0, 8, 0, 70), 0);
    // The ink is the stroke colour.
    let (x, y) = (10..210)
        .flat_map(|x| (10..60).map(move |y| (x, y)))
        .find(|&(x, y)| png.pixel(x, y)[3] == 255)
        .expect("an opaque glyph pixel");
    assert_eq!(png.pixel(x, y), [0x1e, 0x1e, 0x1e, 255]);
}

#[test]
fn render_embeds_the_scene_and_reads_back() {
    let dir = scratch("render-embed");
    let input = text_scene(&dir);
    let out = excali(&[
        "render".as_ref(),
        input.as_os_str(),
        "--embed-scene".as_ref(),
    ]);
    assert_code(&out, OK);
    // exportCanvas's extension: "excalidraw.png" with the scene embedded.
    let output = dir.join("text.excalidraw.png");
    let bytes = std::fs::read(&output).unwrap();
    let text = excali_core::png::decode_png_metadata(&bytes)
        .unwrap()
        .unwrap();
    let scene: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(scene["type"], "excalidraw");
    assert_eq!(scene["source"], "https://github.com/HutsonLabs/excali-rust");
    assert_eq!(scene["elements"][0]["id"], "hello");
    assert_eq!(scene["elements"][0]["text"], "HHHH");
    // loadFromBlob reads the PNG back.
    let (code, report) = validate_json(&output);
    assert_eq!(code, OK);
    assert_eq!(report["texts"], json!(["HHHH"]));
}

#[test]
fn render_names_the_output_after_the_input() {
    let dir = scratch("render-name");
    let input = text_scene(&dir);
    assert_code(&excali(&["render".as_ref(), input.as_os_str()]), OK);
    assert!(dir.join("text.png").is_file());
    assert!(!dir.join("text.excalidraw.png").exists());
}

#[test]
fn render_writes_to_stdout() {
    let dir = scratch("render-stdout");
    let input = text_scene(&dir);
    let out = excali(&[
        "render".as_ref(),
        input.as_os_str(),
        "-o".as_ref(),
        "-".as_ref(),
    ]);
    assert_code(&out, OK);
    assert!(out.stdout.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_eq!(read_png(&out.stdout).width, 220);
}

#[test]
fn render_uses_the_source_given() {
    let dir = scratch("render-source");
    let input = text_scene(&dir);
    let output = dir.join("out.png");
    let out = excali(&[
        "render".as_ref(),
        input.as_os_str(),
        "--embed-scene".as_ref(),
        "--source".as_ref(),
        "https://example.test".as_ref(),
        "-o".as_ref(),
        output.as_os_str(),
    ]);
    assert_code(&out, OK);
    let text = excali_core::png::decode_png_metadata(&std::fs::read(&output).unwrap())
        .unwrap()
        .unwrap();
    let scene: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(scene["source"], "https://example.test");
}

#[test]
fn an_empty_scene_cannot_be_exported() {
    let dir = scratch("render-empty");
    let input = scene_file(&dir, "empty.excalidraw", &json!([]), &json!({}), &json!({}));
    for command in ["render", "export"] {
        let out = excali(&[command.as_ref(), input.as_os_str()]);
        assert_code(&out, EXPORT);
        assert!(
            stderr(&out).contains("Cannot export empty canvas."),
            "{}",
            stderr(&out)
        );
    }
}

#[test]
fn an_unknown_frame_cannot_be_exported() {
    let dir = scratch("render-frame");
    let input = text_scene(&dir);
    for command in ["render", "export"] {
        let out = excali(&[
            command.as_ref(),
            input.as_os_str(),
            "--frame".as_ref(),
            "nope".as_ref(),
        ]);
        assert_code(&out, EXPORT);
        assert!(
            stderr(&out).contains("no frame with id \"nope\""),
            "{}",
            stderr(&out)
        );
    }
}

#[test]
fn a_library_is_not_rendered() {
    let lib = upstream_fixture("fixture_library.excalidrawlib");
    let out = excali(&[
        "render".as_ref(),
        lib.as_os_str(),
        "-o".as_ref(),
        "-".as_ref(),
    ]);
    assert_code(&out, INVALID);
    assert!(stderr(&out).contains("Error: invalid file"));
}

#[test]
fn an_unwritable_output_is_an_io_error() {
    let dir = scratch("render-unwritable");
    let input = text_scene(&dir);
    let out = excali(&[
        "render".as_ref(),
        input.as_os_str(),
        "-o".as_ref(),
        "/nonexistent/dir/out.png".as_ref(),
    ]);
    assert_code(&out, IO);
}

#[test]
fn render_reads_scenes_embedded_in_images() {
    let dir = scratch("render-embedded");
    let output = dir.join("smiley.png");
    let input = upstream_fixture("smiley_embedded_v2.svg");
    let out = excali(&[
        "render".as_ref(),
        input.as_os_str(),
        "-o".as_ref(),
        output.as_os_str(),
    ]);
    assert_code(&out, OK);
    let png = read_png(&std::fs::read(&output).unwrap());
    assert!(png.width > 20 && png.height > 20);
}

// ---------------------------------------------------------------------------
// export (SVG)

#[test]
fn export_writes_the_svg_document() {
    let golden = canvas_export_golden();
    let dir = scratch("export-svg");
    let scene = &golden["scenes"][0];
    assert_eq!(scene["name"], "default");
    let input = golden_scene_file(&dir, scene);
    let out = excali(&[
        "export".as_ref(),
        input.as_os_str(),
        "-o".as_ref(),
        "-".as_ref(),
    ]);
    assert_code(&out, OK);
    let svg = stdout(&out);
    assert!(svg.starts_with(excali_svg::SVG_DOCUMENT_PREAMBLE), "{svg}");
    assert!(
        // exportToSvg keeps the fractional size the PNG canvas truncates
        // (318 x 160).
        svg.contains(r#"<svg version="1.1" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 318.3347922202819 160" width="318.3347922202819" height="160">"#),
        "{svg}"
    );
    assert!(svg.contains("<!-- svg-source:excalidraw -->"));
    assert!(
        svg.contains(
            r##"<rect x="0" y="0" width="318.3347922202819" height="160" fill="#ffffff"></rect>"##
        ),
        "{svg}"
    );
    // The scene's text is Excalifont: its face is inlined, subset.
    assert!(
        svg.contains("@font-face { font-family: Excalifont; src: url(data:font/woff2;base64,"),
        "{svg}"
    );
}

#[test]
fn export_follows_the_options() {
    let golden = canvas_export_golden();
    let dir = scratch("export-options");
    let input = golden_scene_file(&dir, &golden["scenes"][0]);
    let out = excali(&[
        "export".as_ref(),
        input.as_os_str(),
        "--no-background".as_ref(),
        "--scale".as_ref(),
        "2".as_ref(),
        "--no-fonts".as_ref(),
        "-o".as_ref(),
        "-".as_ref(),
    ]);
    assert_code(&out, OK);
    let svg = stdout(&out);
    assert!(
        svg.contains(
            r#"viewBox="0 0 318.3347922202819 160" width="636.6695844405638" height="320""#
        ),
        "{svg}"
    );
    assert!(!svg.contains("<rect x=\"0\" y=\"0\""), "{svg}");
    assert!(!svg.contains("@font-face"), "{svg}");
}

#[test]
fn export_embeds_the_scene_and_reads_back() {
    let dir = scratch("export-embed");
    let input = text_scene(&dir);
    let out = excali(&[
        "export".as_ref(),
        input.as_os_str(),
        "--embed-scene".as_ref(),
    ]);
    assert_code(&out, OK);
    let output = dir.join("text.excalidraw.svg");
    let svg = std::fs::read_to_string(&output).unwrap();
    assert!(svg.contains("<!-- payload-type:application/vnd.excalidraw+json -->"));
    let text = excali_core::svg_payload::decode_svg_base64_payload(&svg).unwrap();
    let scene: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(scene["elements"][0]["text"], "HHHH");
    // exportCanvas passes exportToSvg only the export keys, and the
    // embedded appState is cleanAppStateForExport of them: of those, only
    // viewBackgroundColor is stored in files (APP_STATE_STORAGE_CONF).
    assert_eq!(scene["appState"], json!({"viewBackgroundColor": "#ffffff"}));
    let (code, report) = validate_json(&output);
    assert_eq!(code, OK);
    assert_eq!(report["texts"], json!(["HHHH"]));
}

#[test]
fn export_names_the_output_after_the_input() {
    let dir = scratch("export-name");
    let input = text_scene(&dir);
    assert_code(&excali(&["export".as_ref(), input.as_os_str()]), OK);
    assert!(dir.join("text.svg").is_file());
}

// ---------------------------------------------------------------------------
// lib

fn two_libraries(dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    // With an index each, so restore keeps every versionNonce
    // (syncInvalidIndices bumps the elements it gives an index).
    let rect = |id: &str, nonce: u32| {
        json!({"id": id, "type": "rectangle", "x": 0, "y": 0, "width": 10, "height": 10,
               "versionNonce": nonce, "version": 1, "seed": 1, "index": format!("a{nonce}")})
    };
    let a = json!({"type": "excalidrawlib", "version": 2, "source": "t", "libraryItems": [
        {"id": "a1", "status": "published", "name": "first", "created": 1, "elements": [rect("r1", 1)]},
        {"id": "a2", "status": "unpublished", "created": 2, "elements": [rect("r2", 2), rect("r3", 3)]},
    ]});
    // b1 repeats a1's elements (ids and versionNonces): not unique.
    let b = json!({"type": "excalidrawlib", "version": 2, "source": "t", "libraryItems": [
        {"id": "b1", "status": "unpublished", "created": 3, "elements": [rect("r1", 1)]},
        {"id": "b2", "status": "unpublished", "name": "new", "created": 4, "elements": [rect("r4", 4)]},
    ]});
    let pa = dir.join("a.excalidrawlib");
    let pb = dir.join("b.excalidrawlib");
    std::fs::write(&pa, a.to_string()).unwrap();
    std::fs::write(&pb, b.to_string()).unwrap();
    (pa, pb)
}

#[test]
fn lib_list_lists_the_items() {
    let dir = scratch("lib-list");
    let (a, _) = two_libraries(&dir);
    let out = excali(&[
        "lib".as_ref(),
        "list".as_ref(),
        "--json".as_ref(),
        a.as_os_str(),
    ]);
    assert_code(&out, OK);
    let items: Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(
        items,
        json!([
            {"id": "a1", "name": "first", "status": "published", "created": 1, "elements": 1},
            {"id": "a2", "name": null, "status": "unpublished", "created": 2, "elements": 2},
        ])
    );
    let plain = stdout(&excali(&["lib".as_ref(), "list".as_ref(), a.as_os_str()]));
    let lines: Vec<&str> = plain.lines().collect();
    assert_eq!(lines, ["a1\tpublished\t1\tfirst", "a2\tunpublished\t2\t"]);
}

#[test]
fn lib_list_rejects_what_is_not_a_library() {
    let out = excali(&[
        "lib".as_ref(),
        "list".as_ref(),
        upstream_fixture("smiley_embedded_v2.svg").as_os_str(),
    ]);
    assert_code(&out, INVALID);
    assert!(stderr(&out).contains("Invalid library"), "{}", stderr(&out));
}

#[test]
fn lib_merge_imports_each_library_in_turn() {
    // mergeLibraryItems(local, other): other's unique items, then local.
    let dir = scratch("lib-merge");
    let (a, b) = two_libraries(&dir);
    let merged = dir.join("merged.excalidrawlib");
    let out = excali(&[
        "lib".as_ref(),
        "merge".as_ref(),
        a.as_os_str(),
        b.as_os_str(),
        "-o".as_ref(),
        merged.as_os_str(),
    ]);
    assert_code(&out, OK);
    let text = std::fs::read_to_string(&merged).unwrap();
    let lib: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(lib["type"], "excalidrawlib");
    assert_eq!(lib["version"], 2);
    assert_eq!(lib["source"], "https://github.com/HutsonLabs/excali-rust");
    let ids: Vec<&str> = lib["libraryItems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["b2", "a1", "a2"]);
    // serializeLibraryAsJSON: JSON.stringify(data, null, 2).
    assert!(text.starts_with("{\n  \"type\": \"excalidrawlib\",\n  \"version\": 2,\n"));
    let (code, report) = validate_json(&merged);
    assert_eq!(code, OK);
    assert_eq!(report["items"], 3);
}

#[test]
fn lib_merge_of_a_library_with_itself_changes_nothing() {
    let dir = scratch("lib-merge-self");
    let (a, _) = two_libraries(&dir);
    let out = excali(&[
        "lib".as_ref(),
        "merge".as_ref(),
        a.as_os_str(),
        a.as_os_str(),
        "-o".as_ref(),
        "-".as_ref(),
    ]);
    assert_code(&out, OK);
    let lib: Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(lib["libraryItems"].as_array().unwrap().len(), 2);
}

#[test]
fn lib_merge_writes_nothing_when_an_input_is_invalid() {
    let dir = scratch("lib-merge-invalid");
    let (a, _) = two_libraries(&dir);
    let merged = dir.join("merged.excalidrawlib");
    let out = excali(&[
        "lib".as_ref(),
        "merge".as_ref(),
        a.as_os_str(),
        upstream_fixture("smiley.png").as_os_str(),
        "-o".as_ref(),
        merged.as_os_str(),
    ]);
    assert_code(&out, INVALID);
    assert!(!merged.exists());
}

#[test]
fn lib_merge_needs_an_output() {
    let dir = scratch("lib-merge-usage");
    let (a, b) = two_libraries(&dir);
    let out = excali(&[
        "lib".as_ref(),
        "merge".as_ref(),
        a.as_os_str(),
        b.as_os_str(),
    ]);
    assert_code(&out, USAGE);
}
