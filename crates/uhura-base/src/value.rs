//! The deterministic value model shared by state fields, props, event
//! payloads, command payloads, projections, and traces (design §7.1).
//!
//! Deliberately absent: floats (design §7.5 — determinism by type shape),
//! unordered containers (`Record` is a `BTreeMap`).

use std::collections::BTreeMap;
use std::fmt;

/// A validated lowercase kebab-case identifier: `[a-z][a-z0-9]*(-[a-z0-9]+)*`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ident(String);

impl Ident {
    /// Validates and interns a kebab-case identifier.
    pub fn new(s: &str) -> Result<Self, IdentError> {
        if s.is_empty() || s.len() > 64 {
            return Err(IdentError(s.to_string()));
        }
        let bytes = s.as_bytes();
        if !bytes[0].is_ascii_lowercase() {
            return Err(IdentError(s.to_string()));
        }
        let mut prev_dash = false;
        for &b in &bytes[1..] {
            match b {
                b'a'..=b'z' | b'0'..=b'9' => prev_dash = false,
                b'-' if !prev_dash => prev_dash = true,
                _ => return Err(IdentError(s.to_string())),
            }
        }
        if prev_dash {
            return Err(IdentError(s.to_string()));
        }
        Ok(Ident(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Ident {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl serde::Serialize for Ident {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for Ident {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ident::new(&s).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for Ident {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Error for an invalid identifier; carries the offending text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentError(pub String);

impl fmt::Display for IdentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` is not a lowercase kebab-case identifier", self.0)
    }
}

impl std::error::Error for IdentError {}

/// The one value model. No floats exist (§7.5); all containers are ordered.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Value {
    Unit,
    Bool(bool),
    /// i64, saturating arithmetic at the language level (§4.3).
    Int(i64),
    Text(String),
    /// Opaque external identity (post id, cursor, asset id).
    Id(String),
    /// Command tag as a first-class value; minted from `U.counters`.
    /// Canonical encoding: the string `"t-<n>"` (micro-decision #3).
    Tag(u64),
    /// Absent optional / removed map entry.
    None,
    /// Present optional. Options do not nest in the language (`T?` only),
    /// so `Some` never wraps `None` in checked programs.
    Some(Box<Value>),
    List(Vec<Value>),
    Record(BTreeMap<Ident, Value>),
}

impl Value {
    /// Canonical JSON image of this value (design §7.5): `Unit` and `None`
    /// encode as `null`; `Some(v)` encodes as `v` (options never nest in
    /// checked programs); `Tag(n)` encodes as the string `"t-<n>"`; records
    /// keep their already-sorted key order.
    pub fn to_json(&self) -> serde_json::Value {
        use serde_json::Value as J;
        match self {
            Value::Unit | Value::None => J::Null,
            Value::Bool(b) => J::Bool(*b),
            Value::Int(i) => J::Number((*i).into()),
            Value::Text(s) | Value::Id(s) => J::String(s.clone()),
            Value::Tag(n) => J::String(format!("t-{n}")),
            Value::Some(v) => v.to_json(),
            Value::List(xs) => J::Array(xs.iter().map(Value::to_json).collect()),
            Value::Record(fields) => J::Object(
                fields
                    .iter()
                    .map(|(k, v)| (k.as_str().to_string(), v.to_json()))
                    .collect(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ident_accepts_kebab() {
        for ok in ["a", "post-card", "like-pending", "a1", "x-2y"] {
            assert!(Ident::new(ok).is_ok(), "{ok}");
        }
    }

    #[test]
    fn ident_rejects_non_kebab() {
        for bad in ["", "A", "1a", "-a", "a-", "a--b", "a_b", "café", "on:press"] {
            assert!(Ident::new(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn ident_rejects_over_64_chars() {
        let long = "a".repeat(65);
        assert!(Ident::new(&long).is_err());
    }

    #[test]
    fn value_json_shapes() {
        let mut rec = BTreeMap::new();
        rec.insert(Ident::new("b-key").unwrap(), Value::Int(2));
        rec.insert(Ident::new("a-key").unwrap(), Value::Tag(4));
        let v = Value::Record(rec);
        assert_eq!(v.to_json().to_string(), r#"{"a-key":"t-4","b-key":2}"#);
        assert_eq!(Value::None.to_json(), serde_json::Value::Null);
        assert_eq!(
            Value::Some(Box::new(Value::Bool(true))).to_json(),
            serde_json::Value::Bool(true)
        );
    }
}
