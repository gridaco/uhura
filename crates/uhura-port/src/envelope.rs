//! Qualified receive/send values at a resolved Uhura port edge.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::canonical::CanonicalJson;
use super::contract::{ConstructorDecl, PortDeclaration, SumDecl};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct QualifiedReceiveEnvelope {
    pub port: String,
    pub constructor: String,
    pub payload: CanonicalJson,
}

impl QualifiedReceiveEnvelope {
    pub fn new(
        port: impl Into<String>,
        constructor: impl Into<String>,
        payload: CanonicalJson,
    ) -> Self {
        Self {
            port: port.into(),
            constructor: constructor.into(),
            payload,
        }
    }

    pub fn validate(&self, declaration: &PortDeclaration) -> Result<(), EnvelopeIssue> {
        validate_qualified(
            "receive",
            &self.port,
            &self.constructor,
            &self.payload,
            declaration,
            &declaration.contract.receive,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct QualifiedSendEnvelope {
    pub port: String,
    pub constructor: String,
    pub payload: CanonicalJson,
}

impl QualifiedSendEnvelope {
    pub fn new(
        port: impl Into<String>,
        constructor: impl Into<String>,
        payload: CanonicalJson,
    ) -> Self {
        Self {
            port: port.into(),
            constructor: constructor.into(),
            payload,
        }
    }

    pub fn validate(&self, declaration: &PortDeclaration) -> Result<(), EnvelopeIssue> {
        validate_qualified(
            "send",
            &self.port,
            &self.constructor,
            &self.payload,
            declaration,
            &declaration.contract.send,
        )
    }
}

/// A direction-tagged stable wire envelope.
///
/// Qualification is carried as ordinary data, so `router.changed` can never
/// collide with another port's `changed` constructor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "direction", rename_all = "kebab-case")]
pub enum QualifiedPortEnvelope {
    Receive(QualifiedReceiveEnvelope),
    Send(QualifiedSendEnvelope),
}

impl QualifiedPortEnvelope {
    pub fn validate(&self, declaration: &PortDeclaration) -> Result<(), EnvelopeIssue> {
        match self {
            Self::Receive(envelope) => envelope.validate(declaration),
            Self::Send(envelope) => envelope.validate(declaration),
        }
    }
}

fn validate_qualified(
    direction: &'static str,
    port: &str,
    constructor: &str,
    payload: &CanonicalJson,
    declaration: &PortDeclaration,
    family: &SumDecl,
) -> Result<(), EnvelopeIssue> {
    if port != declaration.name {
        return Err(EnvelopeIssue::new(
            direction,
            port,
            constructor,
            format!(
                "qualified port `{port}` does not match declaration `{}`",
                declaration.name
            ),
        ));
    }
    let constructor_decl = family.constructor(constructor).ok_or_else(|| {
        EnvelopeIssue::new(
            direction,
            port,
            constructor,
            format!("`{constructor}` is not a declared {direction} constructor on port `{port}`"),
        )
    })?;
    validate_payload(direction, port, constructor_decl, payload)
}

fn validate_payload(
    direction: &'static str,
    port: &str,
    constructor: &ConstructorDecl,
    payload: &CanonicalJson,
) -> Result<(), EnvelopeIssue> {
    let object = payload.as_value().as_object().ok_or_else(|| {
        EnvelopeIssue::new(
            direction,
            port,
            &constructor.name,
            "constructor payload must be a JSON object keyed by field name",
        )
    })?;
    let expected: BTreeSet<&str> = constructor
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .collect();
    let actual: BTreeSet<&str> = object.keys().map(String::as_str).collect();
    if expected != actual {
        let missing = expected
            .difference(&actual)
            .copied()
            .collect::<Vec<_>>()
            .join(", ");
        let extra = actual
            .difference(&expected)
            .copied()
            .collect::<Vec<_>>()
            .join(", ");
        return Err(EnvelopeIssue::new(
            direction,
            port,
            &constructor.name,
            format!("payload field mismatch; missing [{missing}], extra [{extra}]"),
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct EnvelopeIssue {
    pub direction: String,
    pub port: String,
    pub constructor: String,
    pub message: String,
}

impl EnvelopeIssue {
    fn new(
        direction: impl Into<String>,
        port: impl Into<String>,
        constructor: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            direction: direction.into(),
            port: port.into(),
            constructor: constructor.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for EnvelopeIssue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} {}.{}: {}",
            self.direction, self.port, self.constructor, self.message
        )
    }
}

impl std::error::Error for EnvelopeIssue {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PortDeclaration, TypeRef, request_port_instance};

    fn canonical(value: serde_json::Value) -> CanonicalJson {
        CanonicalJson::new(value).unwrap()
    }

    fn request_port() -> PortDeclaration {
        PortDeclaration::new(
            "returns",
            request_port_instance(
                TypeRef::new("RequestId").unwrap(),
                TypeRef::new("ReturnPayload").unwrap(),
                TypeRef::new("Settlement").unwrap(),
            )
            .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn validates_receive_and_send_against_the_resolved_qualified_family() {
        let port = request_port();
        QualifiedReceiveEnvelope::new(
            "returns",
            "settled",
            canonical(serde_json::json!({
                "id": { "RequestId": 2 },
                "result": { "accepted": { "return_id": "return-900" } },
            })),
        )
        .validate(&port)
        .unwrap();
        QualifiedSendEnvelope::new(
            "returns",
            "request",
            canonical(serde_json::json!({
                "id": { "RequestId": 2 },
                "payload": { "order": "order-100" },
            })),
        )
        .validate(&port)
        .unwrap();
    }

    #[test]
    fn rejects_wrong_qualification_constructor_and_payload_shape() {
        let port = request_port();
        let wrong_port = QualifiedReceiveEnvelope::new(
            "orders",
            "settled",
            canonical(serde_json::json!({ "id": 2, "result": {} })),
        );
        assert!(wrong_port.validate(&port).is_err());

        let wrong_direction = QualifiedReceiveEnvelope::new(
            "returns",
            "request",
            canonical(serde_json::json!({ "id": 2, "payload": {} })),
        );
        assert!(wrong_direction.validate(&port).is_err());

        let extra_field = QualifiedSendEnvelope::new(
            "returns",
            "request",
            canonical(serde_json::json!({
                "id": 2,
                "payload": {},
                "ambient": true,
            })),
        );
        assert!(extra_field.validate(&port).is_err());
    }

    #[test]
    fn direction_tagged_wire_shape_round_trips() {
        let envelope = QualifiedPortEnvelope::Send(QualifiedSendEnvelope::new(
            "returns",
            "request",
            canonical(serde_json::json!({
                "id": 2,
                "payload": { "order": "order-100" },
            })),
        ));
        let json = serde_json::to_value(&envelope).unwrap();
        assert_eq!(json["direction"], "send");
        assert_eq!(json["port"], "returns");
        assert_eq!(json["constructor"], "request");
        assert_eq!(
            serde_json::from_value::<QualifiedPortEnvelope>(json).unwrap(),
            envelope
        );
    }
}
