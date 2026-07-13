//! uhura-check: the whole front half as a pure function over in-memory
//! inputs — routes from file paths, resolution, the catalog-as-data model +
//! meta-schema (module `catalog`), port linking, typecheck, markup/style
//! rules, example resolution (replay folds uhura-core's step_u), and
//! lowering to the checked IR (design §12.1). The CLI walks the filesystem;
//! this crate never does.
#![deny(clippy::float_arithmetic)]

pub mod catalog;
pub mod examples;
pub mod fixture;
pub mod infer;
pub mod lower;
pub mod manifest;
pub mod markup;
pub mod pipeline;
pub mod preview;
pub mod replay;
pub mod resolve;
pub mod style;
pub mod types;

pub use pipeline::{CheckInput, CheckOutput, LockStatus, SourceInput, check};
