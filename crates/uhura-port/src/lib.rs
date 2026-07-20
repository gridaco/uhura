//! Uhura's typed foreign boundary: canonical values, port contracts,
//! admission, qualified envelopes, routes, and pinned standard contracts.
#![deny(clippy::float_arithmetic)]

mod canonical;
pub mod contract;
pub mod envelope;
pub mod route;
pub mod standard;

pub use canonical::{CanonicalJson, CanonicalJsonError};
pub use contract::{
    AdmissionIssue, AdmissionIssueCode, AdmissionReport, AdmittedBinding, AdmittedPortSet,
    CheckedUiDecl, CodecDecl, ConstructorDecl, ContractCompatibility, ContractIdentity,
    ContractInstance, ContractModelError, FieldDecl, PortBinding, PortContract, PortDeclaration,
    PureHelperDecl, ResolvedCodec, SumDecl, TypeArgument, TypeRef, UiAttributeDecl, admit_bindings,
};
pub use envelope::{
    EnvelopeIssue, QualifiedPortEnvelope, QualifiedReceiveEnvelope, QualifiedSendEnvelope,
};
pub use route::{
    OPAQUE_PATH_CODEC, QUERY_VALUE_CODEC, RouteAtom, RouteConstructorDecl, RouteError,
    RouteErrorCode, RouteFieldDecl, RouteFieldKind, RouteFieldValue, RouteLocation,
    RoutePatternDecl, RouteTable, decode_opaque_path_component, decode_query_value,
    encode_opaque_path_component, encode_query_value,
};
pub use standard::{
    CANONICAL_VALUE_CODEC, OBSERVATION_CONTRACT_HASH, OBSERVATION_MODULE, PORTS_MODULE,
    REQUEST_PORT_CONTRACT_HASH, ROUTER_CONTRACT_HASH, ROUTER_MODULE, ROUTER_ROUTE_CODEC,
    SINK_PORT_CONTRACT_HASH, observation_contract, observation_instance, request_port_contract,
    request_port_instance, router_contract, router_instance, sink_port_contract,
    sink_port_instance,
};
