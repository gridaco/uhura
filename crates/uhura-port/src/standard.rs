//! Pinned Uhura standard port declarations.

use super::canonical::CanonicalJson;
use super::contract::{
    CheckedUiDecl, CodecDecl, ConstructorDecl, ContractIdentity, ContractInstance,
    ContractModelError, FieldDecl, PortContract, PureHelperDecl, SumDecl, TypeRef, UiAttributeDecl,
};
use super::route::{OPAQUE_PATH_CODEC, QUERY_VALUE_CODEC, RouteTable};

pub const OBSERVATION_MODULE: &str = "uhura.observation";
pub const PORTS_MODULE: &str = "uhura.ports";
pub const ROUTER_MODULE: &str = "uhura.web_router";
pub const CANONICAL_VALUE_CODEC: &str = "uhura.canonical-value@1";
pub const ROUTER_ROUTE_CODEC: &str = "uhura.web-router.routes@1";
pub const OBSERVATION_CONTRACT_HASH: &str =
    "5fab03e1b0d8df90d6f3be7232483cadd649f617a395158ad88fc285c9a52a95";
pub const REQUEST_PORT_CONTRACT_HASH: &str =
    "fd0e728a878b6bd3fa934c5732fc1f9d4d7690ee7c3bba6900525a320b8970b0";
pub const SINK_PORT_CONTRACT_HASH: &str =
    "ec64470339e44b6c3d8ab1741044960cf2a064221e0e24611842bb26d1ec2cab";
pub const ROUTER_CONTRACT_HASH: &str =
    "14c5b3966e609addca5699af4dcec9a65cbcd6ae400b42bed920565a62dc84ee";

pub fn observation_contract() -> PortContract {
    PortContract {
        identity: identity(OBSERVATION_MODULE, "Observation"),
        type_parameters: vec!["T".to_string()],
        configuration: ty("Unit"),
        receive: SumDecl::constructors(vec![ConstructorDecl::new(
            "observed",
            vec![FieldDecl::new("value", ty("T"))],
        )]),
        send: SumDecl::never(),
        pure_helpers: Vec::new(),
        codecs: vec![CodecDecl {
            name: "value".to_string(),
            target: ty("T"),
            semantic_id: CANONICAL_VALUE_CODEC.to_string(),
            configuration_scoped: false,
        }],
        checked_ui: Vec::new(),
    }
}

pub fn observation_instance(value_type: TypeRef) -> Result<ContractInstance, ContractModelError> {
    observation_contract().instantiate(vec![value_type], CanonicalJson::unit())
}

pub fn request_port_contract() -> PortContract {
    PortContract {
        identity: identity(PORTS_MODULE, "RequestPort"),
        type_parameters: vec![
            "Id".to_string(),
            "Payload".to_string(),
            "Settlement".to_string(),
        ],
        configuration: ty("Unit"),
        receive: SumDecl::constructors(vec![ConstructorDecl::new(
            "settled",
            vec![
                FieldDecl::new("id", ty("Id")),
                FieldDecl::new("result", ty("Settlement")),
            ],
        )]),
        send: SumDecl::constructors(vec![ConstructorDecl::new(
            "request",
            vec![
                FieldDecl::new("id", ty("Id")),
                FieldDecl::new("payload", ty("Payload")),
            ],
        )]),
        pure_helpers: Vec::new(),
        codecs: vec![
            canonical_codec("id", "Id"),
            canonical_codec("payload", "Payload"),
            canonical_codec("settlement", "Settlement"),
        ],
        checked_ui: Vec::new(),
    }
}

pub fn request_port_instance(
    id_type: TypeRef,
    payload_type: TypeRef,
    settlement_type: TypeRef,
) -> Result<ContractInstance, ContractModelError> {
    request_port_contract().instantiate(
        vec![id_type, payload_type, settlement_type],
        CanonicalJson::unit(),
    )
}

pub fn sink_port_contract() -> PortContract {
    PortContract {
        identity: identity(PORTS_MODULE, "SinkPort"),
        type_parameters: vec!["T".to_string()],
        configuration: ty("Unit"),
        receive: SumDecl::never(),
        send: SumDecl::constructors(vec![ConstructorDecl::new(
            "send",
            vec![FieldDecl::new("value", ty("T"))],
        )]),
        pure_helpers: Vec::new(),
        codecs: vec![canonical_codec("value", "T")],
        checked_ui: Vec::new(),
    }
}

pub fn sink_port_instance(value_type: TypeRef) -> Result<ContractInstance, ContractModelError> {
    sink_port_contract().instantiate(vec![value_type], CanonicalJson::unit())
}

pub fn router_contract() -> PortContract {
    PortContract {
        identity: identity(ROUTER_MODULE, "Router"),
        type_parameters: vec!["Location".to_string()],
        configuration: ty("Routes<Location>"),
        receive: SumDecl::constructors(vec![ConstructorDecl::new(
            "changed",
            vec![FieldDecl::new("location", ty("Location"))],
        )]),
        send: SumDecl::constructors(vec![
            ConstructorDecl::new("push", vec![FieldDecl::new("location", ty("Location"))]),
            ConstructorDecl::new("replace", vec![FieldDecl::new("location", ty("Location"))]),
            ConstructorDecl::new("back", Vec::new()),
        ]),
        pure_helpers: vec![PureHelperDecl {
            name: "routes".to_string(),
            signature: "Record<RoutePattern>->Routes<Location>".to_string(),
            semantic_id: format!(
                "{ROUTER_ROUTE_CODEC};path={OPAQUE_PATH_CODEC};query={QUERY_VALUE_CODEC}"
            ),
        }],
        codecs: vec![CodecDecl {
            name: "location".to_string(),
            target: ty("Location"),
            semantic_id: format!(
                "{ROUTER_ROUTE_CODEC};path={OPAQUE_PATH_CODEC};query={QUERY_VALUE_CODEC}"
            ),
            configuration_scoped: true,
        }],
        checked_ui: vec![CheckedUiDecl {
            name: "Link".to_string(),
            attributes: vec![
                UiAttributeDecl {
                    name: "routes".to_string(),
                    ty: ty("Routes<Location>"),
                    required: true,
                    default: None,
                },
                UiAttributeDecl {
                    name: "to".to_string(),
                    ty: ty("Location"),
                    required: true,
                    default: None,
                },
                UiAttributeDecl {
                    name: "disabled".to_string(),
                    ty: ty("Bool"),
                    required: false,
                    default: Some(
                        CanonicalJson::new(serde_json::Value::Bool(false))
                            .expect("Bool is canonical JSON"),
                    ),
                },
            ],
            events: SumDecl::constructors(vec![ConstructorDecl::new("follow", Vec::new())]),
        }],
    }
}

pub fn router_instance(
    location_type: TypeRef,
    routes: &RouteTable,
) -> Result<ContractInstance, ContractModelError> {
    if routes.location_type() != &location_type {
        return Err(ContractModelError::new(
            "router.configuration",
            format!(
                "Routes<{}> cannot configure Router<{}>",
                routes.location_type(),
                location_type
            ),
        ));
    }
    let configuration = CanonicalJson::from_serializable(routes)
        .map_err(|error| ContractModelError::new("router.configuration", error.to_string()))?;
    router_contract().instantiate(vec![location_type], configuration)
}

fn canonical_codec(name: &str, target: &str) -> CodecDecl {
    CodecDecl {
        name: name.to_string(),
        target: ty(target),
        semantic_id: CANONICAL_VALUE_CODEC.to_string(),
        configuration_scoped: false,
    }
}

fn identity(module: &str, name: &str) -> ContractIdentity {
    ContractIdentity::new(module, 1, name).expect("pinned standard contract identity is valid")
}

fn ty(value: &str) -> TypeRef {
    TypeRef::new(value).expect("pinned standard contract type is canonical")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RouteConstructorDecl, RouteFieldDecl, RouteFieldKind, RoutePatternDecl};

    #[test]
    fn pinned_contract_sums_have_the_exact_b3_shapes() {
        let observation = observation_contract();
        assert_eq!(observation.receive.constructors[0].name, "observed");
        assert!(observation.send.is_never());

        let request = request_port_contract();
        assert_eq!(
            request.receive.constructors[0]
                .fields
                .iter()
                .map(|field| field.name.as_str())
                .collect::<Vec<_>>(),
            ["id", "result"]
        );
        assert_eq!(
            request.send.constructors[0]
                .fields
                .iter()
                .map(|field| field.name.as_str())
                .collect::<Vec<_>>(),
            ["id", "payload"]
        );

        let sink = sink_port_contract();
        assert!(sink.receive.is_never());
        assert_eq!(sink.send.constructors[0].name, "send");

        let router = router_contract();
        assert_eq!(
            router
                .send
                .constructors
                .iter()
                .map(|constructor| constructor.name.as_str())
                .collect::<Vec<_>>(),
            ["push", "replace", "back"]
        );
        assert_eq!(router.checked_ui[0].name, "Link");
        assert_eq!(router.checked_ui[0].events.constructors[0].name, "follow");
    }

    #[test]
    fn pinned_contract_content_hashes_are_stable() {
        assert_eq!(
            observation_contract().content_hash(),
            OBSERVATION_CONTRACT_HASH
        );
        assert_eq!(
            request_port_contract().content_hash(),
            REQUEST_PORT_CONTRACT_HASH
        );
        assert_eq!(sink_port_contract().content_hash(), SINK_PORT_CONTRACT_HASH);
        assert_eq!(router_contract().content_hash(), ROUTER_CONTRACT_HASH);
    }

    #[test]
    fn router_codec_identity_is_scoped_to_the_checked_routes_value() {
        let location_type = ty("app@1::Location");
        let routes = RouteTable::compile(
            location_type.clone(),
            vec![RouteConstructorDecl::new(
                "page",
                vec![RouteFieldDecl::new("slug", RouteFieldKind::Text)],
            )],
            vec![RoutePatternDecl::new("page", "/pages/{slug}")],
        )
        .unwrap();
        let instance = router_instance(location_type, &routes).unwrap();
        assert_eq!(
            instance.configuration_type.as_str(),
            "Routes<app@1::Location>"
        );
        assert_eq!(
            instance.codecs[0].configuration_hash,
            Some(instance.configuration.hash())
        );

        // Configuration is serialized as data, not an ambient host lookup.
        let object = instance.configuration.as_value().as_object().unwrap();
        assert_eq!(
            object["patterns"][0]["pattern"],
            serde_json::Value::String("/pages/{slug}".to_string())
        );
        assert_eq!(
            object["constructors"][0]["fields"][0]["name"],
            serde_json::Value::String("slug".to_string())
        );
    }

    #[test]
    fn standard_instances_reject_wrong_generic_arity_or_route_type() {
        assert!(
            observation_contract()
                .instantiate(Vec::new(), CanonicalJson::unit())
                .is_err()
        );

        let routes = RouteTable::compile(
            ty("FirstLocation"),
            vec![RouteConstructorDecl::new("home", Vec::new())],
            vec![RoutePatternDecl::new("home", "/")],
        )
        .unwrap();
        assert!(router_instance(ty("OtherLocation"), &routes).is_err());
    }
}
