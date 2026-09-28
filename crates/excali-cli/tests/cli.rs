use std::process::Command;

fn excali(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_excali"))
        .args(args)
        .output()
        .expect("binary runs")
}

#[test]
fn version_prints_the_workspace_version() {
    let out = excali(&["--version"]);
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        format!("excali {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn help_prints_usage() {
    let out = excali(&["--help"]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("usage: excali"));
}

#[test]
fn unknown_argument_is_a_usage_error() {
    let out = excali(&["--frobnicate"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--frobnicate"));
}

#[test]
fn no_arguments_is_a_usage_error() {
    let out = excali(&[]);
    assert_eq!(out.status.code(), Some(2));
}
