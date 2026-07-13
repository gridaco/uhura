//! The authored contract data (catalog, ports, manifest) must load clean,
//! hash deterministically, and reject tampering. `include_str!` keeps the
//! pure-crate discipline: no fs reads at test run time.

use uhura_check::catalog::{ChildrenModel, ElementClass, load_catalog};
use uhura_check::manifest::load_manifest;
use uhura_port::load_port_contract;

const CATALOG: &str = include_str!("../../../examples/instagram-uhura/catalog/base.toml");
const FEED: &str = include_str!("../../../examples/instagram-uhura/ports/feed.port.toml");
const COMMENTS: &str = include_str!("../../../examples/instagram-uhura/ports/comments.port.toml");
const PROFILE: &str = include_str!("../../../examples/instagram-uhura/ports/profile.port.toml");
const CREATE: &str = include_str!("../../../examples/instagram-uhura/ports/create.port.toml");
const MANIFEST: &str = include_str!("../../../examples/instagram-uhura/uhura.toml");

#[test]
fn base_catalog_loads_with_nine_elements_and_fourteen_icons() {
    let catalog = load_catalog(CATALOG).unwrap();
    assert_eq!(catalog.elements.len(), 9, "design §10: nine elements");
    assert_eq!(catalog.icons.len(), 14, "design §10: the closed icon set");

    let names: Vec<&str> = catalog.elements.keys().map(|k| k.as_str()).collect();
    assert_eq!(
        names,
        [
            "button",
            "icon",
            "image",
            "pager",
            "region",
            "scroll",
            "text",
            "text-field",
            "view"
        ]
    );

    let view = &catalog.elements[&ident("view")];
    assert_eq!(view.class, ElementClass::Layout);
    assert!(
        view.events.is_empty(),
        "a view can never become interactive"
    );

    let scroll = &catalog.elements[&ident("scroll")];
    assert!(scroll.viewport);
    assert_eq!(
        scroll.events[&ident("near-end")].threshold_percent,
        Some(100),
        "integer percentage, stated once in the catalog (§8.2)"
    );

    let pager = &catalog.elements[&ident("pager")];
    assert_eq!(pager.children, ChildrenModel::KeyedEach);

    let text_field = &catalog.elements[&ident("text-field")];
    assert_eq!(
        text_field.controlled,
        Some((ident("value"), ident("change"))),
        "controlled promotion (§10)"
    );
    assert_eq!(
        text_field.events[&ident("change")].carries.len(),
        1,
        "change carries {{ value: text }} (§4.2)"
    );
}

#[test]
fn all_four_ports_load_clean() {
    let feed = load_port_contract(FEED).unwrap();
    assert_eq!(feed.name.as_str(), "feed");
    assert_eq!(feed.types.len(), 10);
    assert_eq!(feed.projections.len(), 5);
    assert!(feed.projections[&ident("viewer")].boot);
    assert_eq!(feed.commands.len(), 5);
    assert!(
        feed.commands[&ident("reload")].payload.is_empty(),
        "ok payloads and reload's payload are empty (§9.1)"
    );

    let comments = load_port_contract(COMMENTS).unwrap();
    assert_eq!(
        comments.projections[&ident("for-post")].key,
        Some(uhura_port::TypeExpr::Id),
        "keyed projection (§9.2)"
    );

    let profile = load_port_contract(PROFILE).unwrap();
    assert_eq!(profile.types.len(), 6);
    assert_eq!(profile.projections.len(), 4);
    assert_eq!(profile.commands.len(), 3);

    let create = load_port_contract(CREATE).unwrap();
    assert_eq!(create.name.as_str(), "create");
    assert_eq!(create.projections.len(), 1);
    assert_eq!(create.commands.len(), 2);
}

#[test]
fn contract_hashes_are_deterministic_and_tamper_evident() {
    let a = load_port_contract(FEED).unwrap().canonical_hash();
    let b = load_port_contract(FEED).unwrap().canonical_hash();
    assert_eq!(a, b);
    assert_eq!(a.len(), 64);

    let tampered = FEED.replace("like-count = \"int\"", "like-count = \"text\"");
    let c = load_port_contract(&tampered).unwrap().canonical_hash();
    assert_ne!(a, c, "a shape change must change the pin (§9.1)");

    // Comment/whitespace churn must NOT change the pin.
    let reformatted = FEED.replace("# provider-formatted; core has no clock (§9.1)\n", "");
    let d = load_port_contract(&reformatted).unwrap().canonical_hash();
    assert_eq!(a, d, "the pin covers the canonical form, not the bytes");

    let catalog_a = load_catalog(CATALOG).unwrap().canonical_hash();
    let catalog_b = load_catalog(CATALOG).unwrap().canonical_hash();
    assert_eq!(catalog_a, catalog_b);
}

#[test]
fn manifest_loads_and_binds_all_four_ports() {
    let manifest = load_manifest(MANIFEST).unwrap();
    assert_eq!(manifest.entry.as_str(), "feed");
    assert_eq!(manifest.ports.len(), 4);
    assert_eq!(manifest.catalog_path, "catalog/base.toml");
    let play = &manifest.play[&ident("default")];
    assert_eq!(play.script.as_str(), "demo");
    let provider = play.provider.as_ref().expect("live play provider");
    assert_eq!(provider.module, "providers/spock.js");
    assert_eq!(
        provider.config["graphql_url"],
        "http://127.0.0.1:4000/graphql/v1"
    );
    assert_eq!(
        provider.config["rpc_url"],
        "http://127.0.0.1:4000/rest/v1/rpc"
    );
    assert_eq!(
        provider.config["storage_url"],
        "http://127.0.0.1:4000/storage/v1"
    );
    assert_eq!(
        provider.config["actor"],
        "10000000-0000-4000-8000-000000000001"
    );
}

#[test]
fn manifest_rejects_unsafe_provider_modules_and_non_string_config() {
    let unsafe_module = MANIFEST.replace(
        "module = \"providers/spock.js\"",
        "module = \"../outside.js\"",
    );
    let issues = load_manifest(&unsafe_module).unwrap_err();
    assert!(
        issues.iter().any(|issue| {
            issue.path == "play.default.provider.module"
                && issue.message.contains("corpus-relative")
        }),
        "{issues:?}"
    );

    let non_string = MANIFEST.replace(
        "graphql_url = \"http://127.0.0.1:4000/graphql/v1\"",
        "graphql_url = 4000",
    );
    let issues = load_manifest(&non_string).unwrap_err();
    assert!(
        issues.iter().any(|issue| {
            issue.path == "play.default.provider.config.graphql_url"
                && issue.message.contains("must be strings")
        }),
        "{issues:?}"
    );
}

#[test]
fn meta_schema_rejects_input_events_on_layout() {
    let bad = CATALOG.replace(
        "[elements.scroll.events.near-end]\nkind = \"observe\"",
        "[elements.scroll.events.near-end]\nkind = \"input\"",
    );
    let issues = load_catalog(&bad).unwrap_err();
    assert!(
        issues.iter().any(|i| i
            .message
            .contains("input events are declarable only on interactive")),
        "{issues:?}"
    );
}

#[test]
fn contract_validation_rejects_dangling_and_recursive_types() {
    let dangling = FEED.replace("avatar = \"image-ref\"", "avatar = \"portrait-ref\"");
    let issues = load_port_contract(&dangling).unwrap_err();
    assert!(
        issues
            .iter()
            .any(|i| i.message.contains("`portrait-ref` is not declared")),
        "{issues:?}"
    );

    let recursive = FEED.replace(
        "[types.slide.fields]\nid = \"id\"",
        "[types.slide.fields]\nagain = \"slide\"\nid = \"id\"",
    );
    let issues = load_port_contract(&recursive).unwrap_err();
    assert!(
        issues.iter().any(|i| i.message.contains("recursive")),
        "{issues:?}"
    );
}

fn ident(s: &str) -> uhura_base::Ident {
    uhura_base::Ident::new(s).unwrap()
}
