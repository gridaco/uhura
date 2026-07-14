//! uhura-core: the checked IR ("uhura-ir/0", module `ir`), the semantic view
//! protocol ("uhura-view/0", module `view`), and the machine — step-u and
//! eval_view. Pure and I/O-free by construction: the dependency closure is
//! exactly {uhura-base, uhura-port}, enforced by uhura-tests/tests/purity.rs
//! (design §7, §8.1, §12.1). No clocks, randomness, floats, or HashMap.
#![deny(clippy::float_arithmetic)]

pub mod decode;
pub mod eval;
pub mod event;
pub mod inspect;
pub mod ir;
pub mod state;
pub mod step;
pub mod template;
pub mod trace;
pub mod view;
