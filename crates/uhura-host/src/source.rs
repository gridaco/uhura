//! Compatibility re-exports for Spock and existing Uhura host consumers.

pub(crate) use uhura_project::normalize_project_path;
pub use uhura_project::{
    ProjectSourceFingerprint, ProjectSourceSnapshot, capture_project_snapshot,
};
