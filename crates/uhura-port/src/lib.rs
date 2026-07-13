//! uhura-port: the Spock seam — port contract model, closed type grammar,
//! and the `uhura-provider/0` envelopes. This crate is exactly what a future
//! real Spock adapter implements against (design §9).
#![deny(clippy::float_arithmetic)]

pub mod contract;
pub mod envelope;
#[cfg(feature = "toml")]
pub mod load;
pub mod types;

pub use contract::{CommandDecl, ContractIssue, PortContract, ProjectionDecl, TypeDecl};
pub use envelope::{
    CommandEnvelope, OutcomeEnvelope, OutcomeResult, PROVIDER_PROTOCOL, ProjectionUpdate,
    ProviderMsg,
};
#[cfg(feature = "toml")]
pub use load::load_port_contract;
pub use types::TypeExpr;
