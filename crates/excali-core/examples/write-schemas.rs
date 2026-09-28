//! Regenerate the published JSON Schemas (`site/static/schema/`) from the
//! model: `cargo run -p excali-core --example write-schemas`. The test
//! `published_files_match_the_model` (`tests/schema.rs`) fails until the
//! files are regenerated after a model change. File access lives here, not
//! in `excali_core::schema`, because the crate itself does no I/O (ADR-008).

use std::path::Path;
use std::process::ExitCode;

use excali_core::schema;

fn write(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for p in schema::published() {
        std::fs::write(dir.join(p.file_name), schema::to_json(&(p.schema)()))?;
        println!("wrote site/static/schema/{}", p.file_name);
    }
    Ok(())
}

fn main() -> ExitCode {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../site/static/schema");
    match write(&dir) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("write-schemas: {}: {e}", dir.display());
            ExitCode::FAILURE
        }
    }
}
