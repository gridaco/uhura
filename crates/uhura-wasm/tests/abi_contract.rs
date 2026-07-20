//! Frozen public Wasm protocol surface.

use uhura_wasm::{BROWSER_PROTOCOL, RUNTIME_SNAPSHOT_PROTOCOL, Session, protocols};

#[test]
fn protocol_set_is_exact_and_canonical() {
    assert_eq!(BROWSER_PROTOCOL, "uhura-browser/3");
    assert_eq!(RUNTIME_SNAPSHOT_PROTOCOL, "uhura-runtime-snapshot/0");
    assert_eq!(
        protocols(),
        r#"{"browser":"uhura-browser/3","checkpoint":"uhura-checkpoint/0","genesisReceipt":"uhura-genesis-receipt/0","ingressRecord":"uhura-ingress-record/0","ir":"uhura-ir/1","reactionReceipt":"uhura-reaction-receipt/0","runtimeSnapshot":"uhura-runtime-snapshot/0","view":"uhura-view/1"}"#
    );
}

#[test]
fn session_is_the_single_public_runtime() {
    fn accepts_session(_: Option<Session>) {}
    accepts_session(None);
}
