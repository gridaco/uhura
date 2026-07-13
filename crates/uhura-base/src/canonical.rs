//! The single canonical-JSON + SHA-256 choke point (design §7.5, risk #2).
//!
//! Everything that is hashed or golden-tested — checked IR, V snapshots,
//! traces — serializes through here: UTF-8, lexicographically sorted keys,
//! minimal escapes (serde_json's), integers only, compact (no whitespace),
//! no trailing newline (callers add LF where a file format wants it).

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

/// Renders a `serde_json::Value` to canonical form. Panics (debug) on any
/// non-integer number — floats are unrepresentable in the Uhura value model
/// and must never reach a hash.
pub fn to_canonical_json(v: &serde_json::Value) -> String {
    let mut out = String::new();
    write_canonical(v, &mut out);
    out
}

fn write_canonical(v: &serde_json::Value, out: &mut String) {
    use serde_json::Value as J;
    match v {
        J::Null => out.push_str("null"),
        J::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        J::Number(n) => {
            debug_assert!(
                n.is_i64() || n.is_u64(),
                "float reached canonical JSON: {n} — the value model has no floats (§7.5)"
            );
            let _ = write!(out, "{n}");
        }
        J::String(s) => {
            // serde_json's escaping is the canonical escaping.
            let _ = write!(out, "{}", J::String(s.clone()));
        }
        J::Array(xs) => {
            out.push('[');
            for (i, x) in xs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(x, out);
            }
            out.push(']');
        }
        J::Object(fields) => {
            // serde_json's default Map is BTree-backed (preserve_order is
            // banned in the workspace manifest), but sort defensively so a
            // dependency change can never silently reorder hashes.
            let mut keys: Vec<&String> = fields.keys().collect();
            keys.sort_unstable();
            out.push('{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                let _ = write!(out, "{}", serde_json::Value::String(k.clone()));
                out.push(':');
                write_canonical(&fields[k], out);
            }
            out.push('}');
        }
    }
}

/// SHA-256 of `bytes`, lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Convenience: canonical JSON of `v`, hashed.
pub fn hash_json(v: &serde_json::Value) -> String {
    sha256_hex(to_canonical_json(v).as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sorts_keys_and_compacts() {
        let v = json!({"b": [1, 2], "a": {"z": null, "m": "x"}});
        assert_eq!(
            to_canonical_json(&v),
            r#"{"a":{"m":"x","z":null},"b":[1,2]}"#
        );
    }

    #[test]
    fn escapes_via_serde() {
        let v = json!({"k": "line\n\"quote\""});
        assert_eq!(to_canonical_json(&v), r#"{"k":"line\n\"quote\""}"#);
    }

    #[test]
    fn known_sha256() {
        // sha256("{}") — pinned so the hash implementation can never drift.
        assert_eq!(
            sha256_hex(b"{}"),
            "44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a"
        );
    }

    #[test]
    #[should_panic(expected = "float reached canonical JSON")]
    #[cfg(debug_assertions)]
    fn floats_panic_in_debug() {
        let v = json!(1.5);
        let _ = to_canonical_json(&v);
    }
}
