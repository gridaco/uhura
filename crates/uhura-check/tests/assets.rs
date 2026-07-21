use std::collections::BTreeMap;
use std::sync::Arc;

use uhura_base::{Ident, sha256_hex};
use uhura_check::assets::{AssetInput, load_asset_manifest, load_assets};

fn ident(value: &str) -> Ident {
    Ident::new(value).unwrap()
}

#[test]
fn local_asset_manifest_checks_bytes_hash_media_type_and_alt() {
    let bytes = Arc::<[u8]>::from(&b"checked webp"[..]);
    let hash = sha256_hex(&bytes);
    let manifest = load_asset_manifest(&format!(
        r#"[assets.hero]
file = "media/hero.webp"
alt = "A checked hero image"
source = "library-object"
sha256 = "{hash}"

[sources.library-object]
page = "https://example.invalid/provenance-only"
"#
    ))
    .unwrap();
    let checked = load_assets(
        &manifest,
        &BTreeMap::from([(
            ident("hero"),
            AssetInput {
                file: "media/hero.webp".into(),
                bytes: Some(Arc::clone(&bytes)),
            },
        )]),
    )
    .unwrap();
    let hero = &checked.assets[&ident("hero")];
    assert_eq!(hero.bytes, bytes);
    assert_eq!(hero.sha256, hash);
    assert_eq!(hero.media_type, "image/webp");
    assert_eq!(hero.alt, "A checked hero image");
}

#[test]
fn local_asset_manifest_rejects_malformed_entries_missing_bytes_and_hash_drift() {
    let malformed = load_asset_manifest(
        r#"[assets.Bad_Name]
file = "../outside.webp"
alt = ""
source = "library-object"
sha256 = "ABC"
extra = true

[assets.dot-path]
file = "./dot.webp"
alt = "Dot path"

[assets.empty-segment]
file = "media//empty.webp"
alt = "Empty segment"
"#,
    )
    .unwrap_err();
    assert!(
        malformed
            .iter()
            .any(|issue| issue.path == "assets.Bad_Name")
    );
    assert!(
        malformed
            .iter()
            .any(|issue| issue.path == "assets.Bad_Name.file")
    );
    assert!(
        malformed
            .iter()
            .any(|issue| issue.path == "assets.Bad_Name.alt")
    );
    assert!(
        malformed
            .iter()
            .any(|issue| issue.path == "assets.Bad_Name.sha256")
    );
    assert!(
        malformed
            .iter()
            .any(|issue| issue.path == "assets.Bad_Name.extra")
    );
    assert!(
        malformed
            .iter()
            .any(|issue| issue.path == "assets.dot-path.file")
    );
    assert!(
        malformed
            .iter()
            .any(|issue| issue.path == "assets.empty-segment.file")
    );

    let manifest = load_asset_manifest(
        r#"[assets.hero]
file = "hero.webp"
alt = "Hero"
sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
"#,
    )
    .unwrap();
    let missing = load_assets(
        &manifest,
        &BTreeMap::from([(
            ident("hero"),
            AssetInput {
                file: "hero.webp".into(),
                bytes: None,
            },
        )]),
    )
    .unwrap_err();
    assert!(
        missing
            .iter()
            .any(|issue| issue.message.contains("missing or unreadable"))
    );

    let drift = load_assets(
        &manifest,
        &BTreeMap::from([(
            ident("hero"),
            AssetInput {
                file: "hero.webp".into(),
                bytes: Some(Arc::<[u8]>::from(&b"different"[..])),
            },
        )]),
    )
    .unwrap_err();
    assert!(
        drift
            .iter()
            .any(|issue| issue.message.contains("hash mismatch"))
    );
}
