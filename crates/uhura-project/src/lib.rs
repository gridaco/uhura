//! Canonical Uhura project capture and admission.
//!
//! This crate is the filesystem-to-language boundary shared by the CLI, host,
//! Editor, and framework consumers. It captures one immutable revision,
//! validates its explicit project contract, resolves exact locked packages,
//! and hands the pure checker one admitted source inventory.

mod resolve;
mod source;
mod web_app;

pub use resolve::{
    AdmittedSource, AdmittedSourceKind, ProjectRejection, ResolvedApplication, ResolvedModule,
    ResolvedProfile, ResolvedProject, ResolvedRoute, ResolvedUiRole, ResolvedUiSubject,
    ResolvedWebApplication, resolve_project, selected_web_app_router_port,
};
pub use source::{
    ProjectSourceFingerprint, ProjectSourceSnapshot, capture_project_snapshot,
    normalize_project_path,
};

pub const RESOLVED_APPLICATION_PROTOCOL: &str = "uhura-resolved-application/0";
