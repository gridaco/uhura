//! Lossless Wasm boundary for the canonical Uhura machine runtime.
//!
//! [`Session`] is a thin owner around `uhura-core`: it admits one checked
//! machine instance, advances it deterministically, and optionally projects
//! its declared web presentation. Hosts own adapters, clocks, networking, and
//! DOM mutation.

use serde_json::json;
use uhura_base::to_canonical_json;
use uhura_core::{
    CHECKPOINT_PROTOCOL, GENESIS_RECEIPT_PROTOCOL, INGRESS_RECORD_PROTOCOL, IR_PROTOCOL,
    REACTION_RECEIPT_PROTOCOL, VIEW_PROTOCOL,
};
use wasm_bindgen::prelude::*;

mod session;

pub use session::{BROWSER_PROTOCOL, Session};

/// Protocol versions spoken by this build.
///
/// The browser shell hard-asserts this complete set before admitting a
/// session. Adding or changing a value is an explicit protocol revision.
#[wasm_bindgen]
pub fn protocols() -> String {
    to_canonical_json(&json!({
        "browser": BROWSER_PROTOCOL,
        "checkpoint": CHECKPOINT_PROTOCOL,
        "genesisReceipt": GENESIS_RECEIPT_PROTOCOL,
        "ingressRecord": INGRESS_RECORD_PROTOCOL,
        "ir": IR_PROTOCOL,
        "reactionReceipt": REACTION_RECEIPT_PROTOCOL,
        "view": VIEW_PROTOCOL,
    }))
}
