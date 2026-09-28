//! Why a command failed, and the exit code that says so.

use std::fmt;

/// The exit codes (`site/content/architecture/cli.md`).
pub mod code {
    /// Every file loaded, everything was written.
    pub const OK: u8 = 0;
    /// A file is not what the command reads (not a scene or library, an
    /// image without a scene, a library whose items do not restore).
    pub const INVALID: u8 = 1;
    /// The arguments are wrong (clap's usage errors exit with 2 as well).
    pub const USAGE: u8 = 2;
    /// A file could not be read or written.
    pub const IO: u8 = 3;
    /// The scene loaded but cannot be exported: no elements, a canvas too
    /// big to encode, or no frame with the id given.
    pub const EXPORT: u8 = 4;
}

/// A failure, with the message printed for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    Invalid(String),
    Usage(String),
    Io(String),
    Export(String),
}

impl Failure {
    pub fn code(&self) -> u8 {
        match self {
            Failure::Invalid(_) => code::INVALID,
            Failure::Usage(_) => code::USAGE,
            Failure::Io(_) => code::IO,
            Failure::Export(_) => code::EXPORT,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Failure::Invalid(m) | Failure::Usage(m) | Failure::Io(m) | Failure::Export(m) => m,
        }
    }

    /// An I/O failure on `path`.
    pub fn io(path: &std::path::Path, e: &std::io::Error) -> Failure {
        Failure::Io(format!("{}: {e}", path.display()))
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for Failure {}
