//! Uhura's pure static semantics and lowering pass.
//!
//! The checker consumes a source-spanned project and returns the one canonical
//! machine program plus diagnostics. Filesystem and host policy stay outside
//! this crate.
#![deny(clippy::float_arithmetic)]

pub mod assets;
mod authoring;
mod checker;
mod checker_ir;
mod compile;
mod diagnostic;
mod evidence;
pub mod icon_fonts;
mod parts;
pub mod project_lock;
pub mod project_manifest;
pub mod provenance;
pub mod resource_manifest;
pub mod source;
mod topology;
mod types;
pub mod ui_catalog;
mod updates;

pub use assets::{AssetInput, CheckedAsset, CheckedAssets};
pub use authoring::{
    AuthoringEntry, AuthoringEntryClass, AuthoringProjection, AuthoringTarget, AuthoringTargetClass,
};
pub use checker::CheckOutput;
pub use compile::{ProjectSource, compile_project};
pub use diagnostic::{codes, error, warning};
pub use icon_fonts::{
    CheckedIconFamily, CheckedIconFonts, IconFontInput, IconTokenIssue, check_program_icon_tokens,
    icon_token_diagnostics,
};
pub use provenance::{ProvenanceBuildError, build_provenance};
pub use source::{
    CapturedPackageModules, ResolutionMetadata, ResolvedBinding, ResolvedDeclaration,
    ResolvedProject, ResolvedSource, check_module, check_package_graph_with_evidence,
    check_project_modules, check_project_modules_with_evidence, check_resolved_project,
    check_resolved_project_with_evidence, resolve_project_modules,
};
