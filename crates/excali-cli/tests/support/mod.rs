//! Running the `excali` binary and reading what it writes.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{json, Value};

pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// An upstream test fixture (`fixtures/upstream/packages/excalidraw/tests/fixtures`).
pub fn upstream_fixture(name: &str) -> PathBuf {
    repo()
        .join("fixtures/upstream/packages/excalidraw/tests/fixtures")
        .join(name)
}

/// A fresh directory for one test's files.
pub fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("excali-cli")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn excali<S: AsRef<std::ffi::OsStr>>(args: &[S]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_excali"))
        .args(args)
        .env_remove("EXCALIDRAW_EXPORT_SOURCE")
        .env("SOURCE_DATE_EPOCH", "1700000000")
        .stdin(Stdio::null())
        .output()
        .expect("binary runs")
}

pub fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

pub fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Assert the exit code, showing the output when it is not the one wanted.
#[track_caller]
pub fn assert_code(out: &Output, code: i32) {
    assert_eq!(
        out.status.code(),
        Some(code),
        "stdout:\n{}\nstderr:\n{}",
        stdout(out),
        stderr(out)
    );
}

/// Write a `.excalidraw` file.
pub fn scene_file(dir: &Path, name: &str, elements: &Value, app_state: &Value, files: &Value) -> PathBuf {
    let path = dir.join(name);
    let doc = json!({
        "type": "excalidraw",
        "version": 2,
        "source": "https://excalidraw.com",
        "elements": elements,
        "appState": app_state,
        "files": files,
    });
    std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
    path
}

/// A decoded PNG: size and 8-bit RGBA pixels.
pub struct Png {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Png {
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2], self.rgba[i + 3]]
    }
}

pub fn read_png(bytes: &[u8]) -> Png {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().expect("a PNG");
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    buf.truncate(info.buffer_size());
    Png {
        width: info.width,
        height: info.height,
        rgba: buf,
    }
}

/// The canvas export golden of excali-scene (upstream's `exportToCanvas`,
/// `tools/goldens/png-export.mjs`).
pub fn canvas_export_golden() -> Value {
    let path = repo().join("crates/excali-scene/tests/fixtures/canvas-export.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}
