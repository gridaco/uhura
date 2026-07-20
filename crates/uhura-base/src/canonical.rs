//! The single canonical-JSON + SHA-256 choke point (design §7.5, risk #2).
//!
//! Everything that is hashed or golden-tested — checked IR, V snapshots,
//! traces — serializes through here: UTF-8, lexicographically sorted keys,
//! minimal escapes (serde_json's), integers only, compact (no whitespace),
//! no trailing newline (callers add LF where a file format wants it).

use std::fmt::{self, Write as _};

use sha2::{Digest, Sha256};

/// A value that cannot be represented by Uhura's integer-only canonical JSON.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalJsonError {
    pub path: String,
    pub message: String,
}

impl fmt::Display for CanonicalJsonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for CanonicalJsonError {}

/// Renders a `serde_json::Value` to canonical form, rejecting floating-point
/// numbers at every depth in every build profile.
pub fn try_to_canonical_json(v: &serde_json::Value) -> Result<String, CanonicalJsonError> {
    let mut out = String::new();
    write_canonical(v, &mut out, "$")?;
    Ok(out)
}

/// Infallible convenience for values already admitted by a checked Uhura
/// boundary. User-controlled JSON must use [`try_to_canonical_json`].
pub fn to_canonical_json(v: &serde_json::Value) -> String {
    try_to_canonical_json(v).expect("checked Uhura JSON must contain integers only")
}

fn write_canonical(
    v: &serde_json::Value,
    out: &mut String,
    path: &str,
) -> Result<(), CanonicalJsonError> {
    use serde_json::Value as J;
    match v {
        J::Null => out.push_str("null"),
        J::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        J::Number(n) => {
            if !n.is_i64() && !n.is_u64() {
                return Err(CanonicalJsonError {
                    path: path.to_string(),
                    message: format!(
                        "floating-point JSON number `{n}` is not canonical Uhura data"
                    ),
                });
            }
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
                write_canonical(x, out, &format!("{path}[{i}]"))?;
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
                write_canonical(&fields[k], out, &format!("{path}.{k}"))?;
            }
            out.push('}');
        }
    }
    Ok(())
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

/// Convenience for already-admitted JSON: canonical JSON of `v`, hashed.
/// User-controlled values must use [`try_hash_json`].
pub fn hash_json(v: &serde_json::Value) -> String {
    sha256_hex(to_canonical_json(v).as_bytes())
}

/// Fallible canonical JSON hash for user-controlled values.
pub fn try_hash_json(v: &serde_json::Value) -> Result<String, CanonicalJsonError> {
    Ok(sha256_hex(try_to_canonical_json(v)?.as_bytes()))
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
    fn rejects_float_recursively_in_every_build_profile() {
        let error = try_to_canonical_json(&json!({
            "ok": [1, 2],
            "nested": [{ "ratio": 1.5 }],
        }))
        .unwrap_err();
        assert_eq!(error.path, "$.nested[0].ratio");
        assert_eq!(
            error.message,
            "floating-point JSON number `1.5` is not canonical Uhura data"
        );
    }

    #[test]
    fn fallible_hash_rejects_the_same_float_tree() {
        assert_eq!(
            try_hash_json(&json!({ "nested": [1, { "ratio": 0.25 }] }))
                .unwrap_err()
                .path,
            "$.nested[1].ratio"
        );
    }
}
