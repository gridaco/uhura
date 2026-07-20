//! Uhura's pure static semantics and lowering pass.
//!
//! The checker consumes a source-spanned project and returns the one canonical
//! machine program plus diagnostics. Filesystem and host policy stay outside
//! this crate.
#![deny(clippy::float_arithmetic)]

pub mod assets;
mod checker;
mod checker_ir;
mod diagnostic;
pub mod icon_fonts;
pub mod project_lock;
pub mod project_manifest;
pub mod resource_manifest;
mod types;
pub mod ui_catalog;
pub mod v04;
mod v04_compile;
mod v04_evidence;
mod v04_parts;
pub mod v04_provenance;
mod v04_topology;
mod v04_updates;

pub use assets::{AssetInput, CheckedAsset, CheckedAssets};
pub use checker::CheckOutput;
pub use diagnostic::{codes, error, warning};
pub use icon_fonts::{
    CheckedIconFamily, CheckedIconFonts, IconFontInput, IconTokenIssue, check_program_icon_tokens,
    icon_token_diagnostics,
};
pub use v04::{
    CapturedPackageModules as V04CapturedPackageModules,
    ResolutionMetadata as V04ResolutionMetadata, ResolvedBinding as V04ResolvedBinding,
    ResolvedDeclaration as V04ResolvedDeclaration, ResolvedProject as ResolvedV04Project,
    ResolvedSource as V04ResolvedSource, check_module as check_v04_module,
    check_package_graph_with_evidence as check_v04_package_graph_with_evidence,
    check_project_modules as check_v04_project_modules,
    check_project_modules_with_evidence as check_v04_project_modules_with_evidence,
    check_resolved_project as check_resolved_v04_project,
    check_resolved_project_with_evidence as check_resolved_v04_project_with_evidence,
    resolve_project_modules as resolve_v04_project_modules,
};
pub use v04_compile::{V04ProjectSource, compile_v04_project};
pub use v04_provenance::{V04ProvenanceBuildError, build_v04_provenance};
