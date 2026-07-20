use std::path::Path;

#[test]
fn rust_catalogue_and_browser_adapter_contract_are_identical() {
    let contract_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resources/ui-catalog/0.4.json");
    let contract: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&contract_path)
            .unwrap_or_else(|error| panic!("{}: {error}", contract_path.display())),
    )
    .expect("0.4 UI catalogue contract is JSON");

    assert_eq!(contract["protocol"], "uhura-ui-catalog/0");
    assert_eq!(contract["language"], "0.4");
    assert_eq!(
        contract["primitiveAdapters"],
        serde_json::json!(uhura_check::ui_catalog::primitive_adapter_ids()),
        "the checked Rust catalogue is the authority for required browser adapters"
    );
}
