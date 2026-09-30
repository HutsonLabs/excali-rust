// The app's own commands get generated `allow-<command>` permissions, and
// with an app manifest the ACL checks them: capabilities/app.json allows them
// in the `main` window only (tests/app.rs).
fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "smoke",
            "smoke_report",
            "update_check",
            "update_install",
        ]),
    ))
    .expect("failed to run tauri-build");
}
