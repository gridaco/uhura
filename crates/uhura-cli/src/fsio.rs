//! Filesystem-backed source records consumed by CLI commands.

use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct SourceFile {
    /// Project-relative path with `/` separators.
    pub rel_path: String,
    pub abs_path: PathBuf,
    pub text: String,
}
