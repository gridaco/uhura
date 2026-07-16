use std::collections::BTreeMap;
use std::sync::Arc;

use uhura_base::{Ident, Severity};
use uhura_check::icon_fonts::{IconFontInput, load_icon_fonts};
use uhura_check::manifest::{IconFamilyConfig, IconsConfig, Manifest, load_manifest};
use uhura_check::{CheckInput, SourceInput, check};
use uhura_core::ir::{ExprIr, NodeIr};
use uhura_syntax::SourceKind;

const CATALOG: &str = include_str!("../../../examples/instagram/client/catalog/base.toml");
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
    let removed: Vec<&str> = upstream
        .keys()
        .filter(|name| !lucide.glyphs.keys().any(|glyph| glyph.as_str() == *name))
        .map(String::as_str)
        .collect();
    assert_eq!(
        removed,
        [
            "chrome",
            "chromium",
            "codepen",
            "codesandbox",
            "dribbble",
            "facebook",
            "figma",
            "framer",
            "github",
            "gitlab",
            "instagram",
            "linkedin",
            "pocket",
            "rail-symbol",
            "slack",
            "trello",
            "twitch",
            "twitter",
            "youtube",
        ]
    );
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
fn local_registry_rejects_duplicate_names_non_pua_values_and_bad_woff2() {
    let duplicate = load_icon_fonts(
        &local_config(),
        &local_input(r#"{"spark":57589,"spark":57586}"#),
    )
    .unwrap_err();
    assert!(
        duplicate
            .iter()
            .any(|issue| issue.message.contains("duplicate glyph name")),
        "{duplicate:?}"
    );

    let mut inputs = local_input(r#"{"Bad_Name":57589,"spark":65}"#);
    inputs.get_mut(&ident("brand")).unwrap().font_bytes =
        Some(Arc::<[u8]>::from(&b"not-a-font"[..]));
    let invalid = load_icon_fonts(&local_config(), &inputs).unwrap_err();
    assert!(
        invalid.iter().any(|issue| issue.message.contains("WOFF2")),
        "{invalid:?}"
    );
    assert!(
        invalid
            .iter()
            .any(|issue| issue.message.contains("kebab-case")),
        "{invalid:?}"
    );
    assert!(
        invalid
            .iter()
            .any(|issue| issue.message.contains("private-use")),
        "{invalid:?}"
    );

    let absent = load_icon_fonts(&local_config(), &local_input(r#"{"ghost":57461}"#)).unwrap_err();
    assert!(
        absent.iter().any(|issue| issue
            .message
            .contains("absent from the font `cmap` or resolves to `.notdef`")),
        "{absent:?}"
    );
}

#[test]
fn manifest_uses_direct_family_tables() {
    let manifest = load_manifest(
        r#"
[app]
name = "icons-test"
entry = "home"

[catalog]
path = "catalog/base.toml"

[icons]
default = "brand"

[icons.brand]
font = "icons/brand.woff2"
glyphs = "icons/brand.json"
"#,
    )
    .unwrap();
    assert_eq!(manifest.icons.default, ident("brand"));
    assert_eq!(
        manifest.icons.families[&ident("brand")].font,
        "icons/brand.woff2"
    );
}

fn check_source(
    source: &str,
    icons: IconsConfig,
    icon_font_files: BTreeMap<Ident, IconFontInput>,
) -> uhura_check::CheckOutput {
    check(&CheckInput {
        manifest: Manifest {
            app_name: ident("icons-test"),
            entry: ident("home"),
            catalog_path: "catalog/base.toml".into(),
            icons,
            ports: BTreeMap::new(),
            fixtures: BTreeMap::new(),
            assets_manifest: None,
            play: BTreeMap::new(),
        },
        manifest_rel_path: "uhura.toml".into(),
        manifest_text: "# constructed test manifest".into(),
        catalog_file: ("catalog/base.toml".into(), Some(CATALOG.into())),
        icon_font_files,
        port_files: BTreeMap::new(),
        sources: vec![SourceInput {
            rel_path: "app/home/page.uhura".into(),
            text: source.into(),
            kind: SourceKind::Module,
        }],
        theme_css: None,
        fixture_files: BTreeMap::new(),
        lock_text: None,
    })
}

#[test]
fn omitted_family_checks_against_default_and_is_normalized_in_ir() {
    let output = check_source(
        "page\n\n<icon name=\"home\" />\n",
        IconsConfig::default(),
        BTreeMap::new(),
    );
    let errors: Vec<_> = output
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    assert!(output.lock_computed.contains("icon-glyphs lucide sha256:"));
    assert!(output.lock_computed.contains("icon-font lucide sha256:"));

    let root = &output.lowered.unwrap().program.pages[&ident("home")].root;
    let NodeIr::Element(root) = root else {
        panic!("icon should lower as an element")
    };
    let family = root
        .props
        .iter()
        .find(|prop| prop.name == ident("family"))
        .expect("default family is explicit in checked IR");
    assert_eq!(family.value, ExprIr::Text("lucide".into()));
}

#[test]
fn icon_name_is_checked_within_the_selected_family() {
    let valid = check_source(
        "page\n\n<icon family=\"brand\" name=\"spark\" />\n",
        local_config(),
        local_input(r#"{"spark":57589}"#),
    );
    assert!(
        valid
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity != Severity::Error),
        "{:?}",
        valid.diagnostics
    );

    let unknown = check_source(
        "page\n\n<icon family=\"brand\" name=\"missing\" />\n",
        local_config(),
        local_input(r#"{"spark":57589}"#),
    );
    assert!(
        unknown
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "UH5017" && diagnostic.message.contains("brand")),
        "{:?}",
        unknown.diagnostics
    );

    let unknown_family = check_source(
        "page\n\n<icon family=\"missing\" name=\"spark\" />\n",
        local_config(),
        local_input(r#"{"spark":57589}"#),
    );
    assert!(
        unknown_family
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "UH5021"),
        "{:?}",
        unknown_family.diagnostics
    );
}

#[test]
fn icon_name_expressions_use_the_selected_registry_without_giant_enum_diagnostics() {
    let valid = check_source(
        "page\n\n<icon family=\"brand\" name={if true then \"spark\" else \"star\"} />\n",
        local_config(),
        local_input(r#"{"spark":57589,"star":57586}"#),
    );
    assert!(
        valid
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity != Severity::Error),
        "{:?}",
        valid.diagnostics
    );

    let invalid = check_source(
        "page\n\n<icon family=\"brand\" name={if true then \"spark\" else \"missing\"} />\n",
        local_config(),
        local_input(r#"{"spark":57589,"star":57586}"#),
    );
    let diagnostic = invalid
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "UH5017")
        .expect("the invalid branch should name the missing icon");
    assert!(diagnostic.message.contains("`missing`"));
    assert!(diagnostic.message.len() < 200, "{diagnostic:?}");

    let unconstrained = check_source(
        "page\n\n<icon family=\"brand\" name={\"spa\" ++ \"rk\"} />\n",
        local_config(),
        local_input(r#"{"spark":57589}"#),
    );
    assert!(unconstrained.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "UH3001"
            && diagnostic.message == "expected an icon name in family `brand`, got text"
    }));
}
