//! The command-line page (`site/content/architecture/cli.md`) documents
//! every command, every option and every exit code the binary has.

use clap::CommandFactory;
use excali_cli::cli::Cli;
use excali_cli::error::code;

fn page() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../site/content/architecture/cli.md"
    );
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn every_command_and_option_is_documented() {
    let page = page();
    let root = Cli::command();
    let mut checked = 0;
    let mut visit = vec![(String::from("excali"), root)];
    while let Some((name, command)) = visit.pop() {
        for arg in command.get_arguments() {
            if let Some(long) = arg.get_long() {
                if long == "help" || long == "version" {
                    continue;
                }
                assert!(page.contains(&format!("--{long}")), "{name} --{long}");
                checked += 1;
            }
        }
        for sub in command.get_subcommands() {
            if sub.get_name() == "help" {
                continue;
            }
            let full = format!("{name} {}", sub.get_name());
            if sub.get_subcommands().next().is_none() {
                assert!(page.contains(&format!("`{full} ")), "{full}");
                checked += 1;
            }
            visit.push((full, sub.clone()));
        }
    }
    assert!(checked >= 15, "{checked}");
}

#[test]
fn every_exit_code_is_documented() {
    let page = page();
    for n in [code::OK, code::INVALID, code::USAGE, code::IO, code::EXPORT] {
        assert!(page.contains(&format!("\n| {n} | ")), "exit code {n}");
    }
}
