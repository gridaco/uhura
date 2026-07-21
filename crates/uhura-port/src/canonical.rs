//! Canonical JSON at the Uhura machine host boundary.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use uhura_base::{hash_json, to_canonical_json};

/// JSON whose complete tree is valid for deterministic Uhura boundary use.
///
/// Floating-point JSON numbers are rejected. Uhura numerics cross this seam
/// through their separately tagged canonical representations, never through a
/// host `f64`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalJson(Value);

impl CanonicalJson {
    pub fn new(value: Value) -> Result<Self, CanonicalJsonError> {
        validate_value(&value, "$")?;
        Ok(Self(value))
    }

    pub fn from_serializable<T: Serialize>(value: &T) -> Result<Self, CanonicalJsonError> {
        let value = serde_json::to_value(value).map_err(|error| CanonicalJsonError {
            path: "$".to_string(),
            message: error.to_string(),
        })?;
        Self::new(value)
    }

    /// The JSON bridge representation of Uhura's single `Unit` value.
    pub fn unit() -> Self {
        Self(Value::Null)
    }

    pub fn as_value(&self) -> &Value {
        &self.0
    }

    pub fn into_value(self) -> Value {
        self.0
    }

    pub fn canonical_text(&self) -> String {
        to_canonical_json(&self.0)
    }

    pub fn hash(&self) -> String {
        hash_json(&self.0)
    }
}

impl Default for CanonicalJson {
    fn default() -> Self {
        Self::unit()
    }
}

impl Serialize for CanonicalJson {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CanonicalJson {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
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

fn validate_value(value: &Value, path: &str) -> Result<(), CanonicalJsonError> {
    match value {
        Value::Number(number) if !number.is_i64() && !number.is_u64() => Err(CanonicalJsonError {
            path: path.to_string(),
            message: "floating-point JSON is not canonical Uhura data".to_string(),
        }),
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                validate_value(item, &format!("{path}[{index}]"))?;
            }
            Ok(())
        }
        Value::Object(fields) => {
            for (name, field) in fields {
                validate_value(field, &format!("{path}.{name}"))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_float_anywhere_in_the_tree() {
        let error = CanonicalJson::new(serde_json::json!({
            "ok": [1, 2],
            "bad": { "ratio": 0.5 },
        }))
        .unwrap_err();
        assert_eq!(error.path, "$.bad.ratio");
    }

    #[test]
    fn serde_deserialization_applies_the_same_check() {
        assert!(serde_json::from_str::<CanonicalJson>(r#"{"n":1}"#).is_ok());
        assert!(serde_json::from_str::<CanonicalJson>(r#"{"n":1.0}"#).is_err());
    }
}
