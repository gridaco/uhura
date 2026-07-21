//! uhura-base: the foundation every crate shares — the deterministic value
//! model (`Value`, `Ident`), the single canonical-JSON + SHA-256 choke point
//! (design §7.5), spans and source maps, structured diagnostics, the UHnxxx
//! code registry, and the `uhura-diagnostics/0` envelope (design §12.4).
//! No floats exist in the value model (design §7.1).
#![deny(clippy::float_arithmetic)]

mod canonical;
pub mod codes;
mod diagnostic;
mod envelope;
mod span;
mod value;

pub use canonical::{
    CanonicalJsonError, hash_json, sha256_hex, to_canonical_json, try_hash_json,
    try_to_canonical_json,
};
pub use diagnostic::{Diagnostic, Edit, Fix, Label, Severity, has_errors};
pub use envelope::{render_text, to_envelope};
pub use span::{FileId, LineCol, SourceMap, Span};
pub use value::{Ident, IdentError, Value};
