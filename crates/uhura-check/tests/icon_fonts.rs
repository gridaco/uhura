use std::collections::BTreeMap;
use std::sync::Arc;

use uhura_base::Ident;
use uhura_check::icon_fonts::{IconFontInput, load_icon_fonts};
use uhura_check::resource_manifest::{
    IconFamilyConfig, IconsConfig, ResourceManifest, load_resource_manifest,
};

const LUCIDE_FONT: &[u8] = include_bytes!("../../../resources/icon-fonts/lucide/lucide.woff2");
const LUCIDE_UPSTREAM_CODEPOINTS: &str =
    include_str!("../../../resources/icon-fonts/lucide/codepoints.json");

fn ident(value: &str) -> Ident {
    Ident::new(value).unwrap()
}

fn local_config() -> IconsConfig {
    IconsConfig {
        default: ident("brand"),
        families: BTreeMap::from([(
            ident("brand"),
            IconFamilyConfig {
                font: "icons/brand.woff2".into(),
                glyphs: "icons/brand.json".into(),
            },
        )]),
    }
}

fn local_input(glyphs: &str) -> BTreeMap<Ident, IconFontInput> {
    BTreeMap::from([(
        ident("brand"),
        IconFontInput {
            font_path: "icons/brand.woff2".into(),
            font_bytes: Some(Arc::<[u8]>::from(LUCIDE_FONT)),
            glyphs_path: "icons/brand.json".into(),
            glyphs_text: Some(glyphs.into()),
        },
    )])
}

#[test]
fn built_in_lucide_is_the_zero_config_default() {
    let checked = load_icon_fonts(&IconsConfig::default(), &BTreeMap::new()).unwrap();
    assert_eq!(checked.default, ident("lucide"));
    let lucide = &checked.families[&ident("lucide")];
    assert_eq!(lucide.font.get(..4), Some(&b"wOF2"[..]));
    assert_eq!(lucide.glyphs.len(), 1_995);
    assert!(lucide.glyphs.contains_key(&ident("home")));
    assert!(!lucide.glyphs.contains_key(&ident("chrome")));
    assert_eq!(lucide.font_hash.len(), 64);
    assert_eq!(lucide.glyphs_hash.len(), 64);

    let upstream: BTreeMap<String, u32> = serde_json::from_str(LUCIDE_UPSTREAM_CODEPOINTS).unwrap();
    for (name, codepoint) in &lucide.glyphs {
        assert_eq!(upstream.get(name.as_str()), Some(codepoint));
    }
}

#[test]
fn local_registry_hash_is_independent_of_json_order_and_whitespace() {
    let a = load_icon_fonts(
        &local_config(),
        &local_input(r#"{"spark":57589,"star":57586}"#),
    )
    .unwrap();
    let b = load_icon_fonts(
        &local_config(),
        &local_input("{\n  \"star\": 57586,\n  \"spark\": 57589\n}"),
    )
    .unwrap();
    assert_eq!(
        a.families[&ident("brand")].glyphs_hash,
        b.families[&ident("brand")].glyphs_hash
    );
}

#[test]
fn local_registry_rejects_invalid_glyphs_and_fonts() {
    let duplicate = load_icon_fonts(
        &local_config(),
        &local_input(r#"{"spark":57589,"spark":57586}"#),
    )
    .unwrap_err();
    assert!(
        duplicate
            .iter()
            .any(|issue| issue.message.contains("duplicate glyph name"))
    );

    let mut inputs = local_input(r#"{"Bad_Name":57589,"spark":65}"#);
    inputs.get_mut(&ident("brand")).unwrap().font_bytes =
        Some(Arc::<[u8]>::from(&b"not-a-font"[..]));
    let invalid = load_icon_fonts(&local_config(), &inputs).unwrap_err();
    assert!(invalid.iter().any(|issue| issue.message.contains("WOFF2")));
    assert!(
        invalid
            .iter()
            .any(|issue| issue.message.contains("kebab-case"))
    );
    assert!(
        invalid
            .iter()
            .any(|issue| issue.message.contains("private-use"))
    );
}

#[test]
fn resource_manifest_is_closed_and_uses_direct_family_tables() {
    let manifest = load_resource_manifest(
        r#"
[assets]
manifest = "fixtures/assets/manifest.toml"

[icons]
default = "brand"

[icons.brand]
font = "icons/brand.woff2"
glyphs = "icons/brand.json"
"#,
    )
    .unwrap();
    assert_eq!(
        manifest.assets.manifest.as_deref(),
        Some("fixtures/assets/manifest.toml")
    );
    assert_eq!(manifest.icons.default, ident("brand"));
    assert_eq!(
        manifest.icons.families[&ident("brand")].font,
        "icons/brand.woff2"
    );
}

#[test]
fn empty_resource_manifest_selects_lucide_and_retired_sections_are_rejected() {
    assert_eq!(
        load_resource_manifest("").unwrap(),
        ResourceManifest::default()
    );

    let issues = load_resource_manifest(
        r#"
[app]
name = "retired"

[catalog]
path = "catalog/base.toml"

[play.default]
fixture = "standard"
"#,
    )
    .unwrap_err();
    assert_eq!(
        issues
            .iter()
            .map(|issue| issue.path.as_str())
            .collect::<Vec<_>>(),
        ["app", "catalog", "play"]
    );
}

#[test]
fn resource_manifest_rejects_unknown_fields_and_unsafe_paths() {
    let issues = load_resource_manifest(
        r#"
[assets]
manifest = "../assets.toml"
extra = true

[icons]
default = "brand"

[icons.brand]
font = "/tmp/brand.woff2"
glyphs = "icons/../brand.json"
format = "woff2"
"#,
    )
    .unwrap_err();
    let paths = issues
        .iter()
        .map(|issue| issue.path.as_str())
        .collect::<Vec<_>>();
    assert!(paths.contains(&"assets.manifest"));
    assert!(paths.contains(&"assets.extra"));
    assert!(paths.contains(&"icons.brand.font"));
    assert!(paths.contains(&"icons.brand.glyphs"));
    assert!(paths.contains(&"icons.brand.format"));
}
