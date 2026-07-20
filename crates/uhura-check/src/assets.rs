//! Checked local asset registries.
//!
//! The parser owns the existing `[assets.<id>]` manifest shape. The loader is
//! pure over host-supplied bytes so Editor and Play can share one captured
//! project revision without filesystem access in semantic layers.

use std::collections::BTreeMap;
use std::sync::Arc;

use uhura_base::{Ident, sha256_hex};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetManifest {
    pub assets: BTreeMap<Ident, AssetConfig>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetConfig {
    /// Path relative to the asset manifest.
    pub file: String,
    pub alt: String,
    /// Optional presentation-byte pin. Materialized sourced assets require it.
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetInput {
    pub file: String,
    pub bytes: Option<Arc<[u8]>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedAsset {
    pub file: String,
    pub bytes: Arc<[u8]>,
    pub media_type: String,
    pub alt: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CheckedAssets {
    pub assets: BTreeMap<Ident, CheckedAsset>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetIssue {
    pub path: String,
    pub message: String,
}

/// Parse the existing local asset registry.
///
/// `[sources.*]` is provenance-only and intentionally does not enter runtime
/// resources. The accepted asset fields preserve the materializer's current
/// source/hash and motif/seed forms while exposing only file, alt, and hash to
/// the checked runtime resource model.
pub fn load_asset_manifest(text: &str) -> Result<AssetManifest, Vec<AssetIssue>> {
    let table: toml::Table = match text.parse() {
        Ok(table) => table,
        Err(error) => {
            return Err(vec![AssetIssue {
                path: String::new(),
                message: format!("invalid TOML: {error}"),
            }]);
        }
    };
    let mut issues = Vec::new();
    for key in table.keys() {
        if !["assets", "sources"].contains(&key.as_str()) {
            issue(&mut issues, key, format!("unknown key `{key}`"));
        }
    }
    let Some(assets) = table.get("assets").and_then(toml::Value::as_table) else {
        issue(&mut issues, "assets", "missing required `[assets]` table");
        return Err(issues);
    };

    let mut declarations = BTreeMap::new();
    for (id, value) in assets {
        let path = format!("assets.{id}");
        let id = match Ident::new(id) {
            Ok(id) => Some(id),
            Err(_) => {
                issue(
                    &mut issues,
                    &path,
                    format!("`{id}` is not a lowercase kebab-case identifier"),
                );
                None
            }
        };
        let Some(entry) = value.as_table() else {
            issue(&mut issues, &path, "expected an asset table");
            continue;
        };
        for key in entry.keys() {
            if !["file", "alt", "sha256", "size", "source", "motif", "seed"].contains(&key.as_str())
            {
                issue(
                    &mut issues,
                    format!("{path}.{key}"),
                    format!("unknown key `{key}`"),
                );
            }
        }

        let file = required_string(entry.get("file"), &format!("{path}.file"), &mut issues)
            .and_then(|file| {
                if safe_asset_reference(&file) {
                    Some(file)
                } else {
                    issue(
                        &mut issues,
                        format!("{path}.file"),
                        "expected a safe manifest-relative path",
                    );
                    None
                }
            });
        let alt = required_string(entry.get("alt"), &format!("{path}.alt"), &mut issues).and_then(
            |alt| {
                if alt.trim().is_empty() {
                    issue(
                        &mut issues,
                        format!("{path}.alt"),
                        "alternative text must not be empty",
                    );
                    None
                } else {
                    Some(alt)
                }
            },
        );
        let sha256 = optional_sha256(entry.get("sha256"), &format!("{path}.sha256"), &mut issues);

        let source = optional_string(entry.get("source"), &format!("{path}.source"), &mut issues);
        let motif = optional_string(entry.get("motif"), &format!("{path}.motif"), &mut issues);
        let seed = match entry.get("seed") {
            None => None,
            Some(toml::Value::Integer(value)) => Some(*value),
            Some(_) => {
                issue(&mut issues, format!("{path}.seed"), "expected an integer");
                None
            }
        };
        if source.is_some() && sha256.is_none() {
            issue(
                &mut issues,
                format!("{path}.sha256"),
                "a sourced asset requires a SHA-256 pin",
            );
        }
        if motif.is_some() != seed.is_some() {
            issue(
                &mut issues,
                &path,
                "a generated asset requires both `motif` and `seed`",
            );
        }
        if source.is_some() && motif.is_some() {
            issue(
                &mut issues,
                &path,
                "an asset cannot declare both `source` and `motif`",
            );
        }
        if let Some(value) = entry.get("size")
            && !matches!(value, toml::Value::Integer(size) if *size > 0)
        {
            issue(
                &mut issues,
                format!("{path}.size"),
                "expected a positive integer",
            );
        }

        if let (Some(id), Some(file), Some(alt)) = (id, file, alt) {
            declarations.insert(id, AssetConfig { file, alt, sha256 });
        }
    }

    if issues.is_empty() {
        Ok(AssetManifest {
            assets: declarations,
        })
    } else {
        Err(issues)
    }
}

/// Validate the exact captured bytes associated with each declaration.
pub fn load_assets(
    manifest: &AssetManifest,
    inputs: &BTreeMap<Ident, AssetInput>,
) -> Result<CheckedAssets, Vec<AssetIssue>> {
    let mut issues = Vec::new();
    let mut assets = BTreeMap::new();
    for (id, declaration) in &manifest.assets {
        let path = format!("assets.{id}");
        let Some(input) = inputs.get(id) else {
            issue(
                &mut issues,
                &path,
                "declared asset has no supplied file input",
            );
            continue;
        };
        if input.file != declaration.file {
            issue(
                &mut issues,
                format!("{path}.file"),
                format!(
                    "host supplied `{}` for manifest path `{}`",
                    input.file, declaration.file
                ),
            );
        }
        let Some(bytes) = input.bytes.as_ref() else {
            issue(
                &mut issues,
                &declaration.file,
                "asset file is missing or unreadable",
            );
            continue;
        };
        let actual_hash = sha256_hex(bytes);
        if let Some(expected_hash) = &declaration.sha256
            && expected_hash != &actual_hash
        {
            issue(
                &mut issues,
                format!("{path}.sha256"),
                format!("asset hash mismatch: expected `{expected_hash}`, got `{actual_hash}`"),
            );
            continue;
        }
        assets.insert(
            id.clone(),
            CheckedAsset {
                file: declaration.file.clone(),
                bytes: Arc::clone(bytes),
                media_type: asset_media_type(&declaration.file).to_string(),
                alt: declaration.alt.clone(),
                sha256: actual_hash,
            },
        );
    }
    for id in inputs.keys() {
        if !manifest.assets.contains_key(id) {
            issue(
                &mut issues,
                format!("assets.{id}"),
                "host supplied an undeclared asset",
            );
        }
    }

    if issues.is_empty() {
        Ok(CheckedAssets { assets })
    } else {
        Err(issues)
    }
}

fn required_string(
    value: Option<&toml::Value>,
    path: &str,
    issues: &mut Vec<AssetIssue>,
) -> Option<String> {
    match value.and_then(toml::Value::as_str) {
        Some(value) => Some(value.to_string()),
        None => {
            issue(issues, path, "missing required string");
            None
        }
    }
}

fn optional_string(
    value: Option<&toml::Value>,
    path: &str,
    issues: &mut Vec<AssetIssue>,
) -> Option<String> {
    match value {
        None => None,
        Some(value) => match value.as_str() {
            Some(value) => Some(value.to_string()),
            None => {
                issue(issues, path, "expected a string");
                None
            }
        },
    }
}

fn optional_sha256(
    value: Option<&toml::Value>,
    path: &str,
    issues: &mut Vec<AssetIssue>,
) -> Option<String> {
    let value = optional_string(value, path, issues)?;
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Some(value)
    } else {
        issue(
            issues,
            path,
            "expected a lowercase 64-character SHA-256 digest",
        );
        None
    }
}

fn safe_asset_reference(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains('\0')
        && !path.contains("://")
        && !matches!(
            path.as_bytes(),
            [drive, b':', ..] if drive.is_ascii_alphabetic()
        )
        && path
            .split('/')
            .all(|segment| !segment.is_empty() && !matches!(segment, "." | ".."))
}

fn asset_media_type(file: &str) -> &'static str {
    match file
        .rsplit_once('.')
        .map(|(_, extension)| extension)
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

fn issue(issues: &mut Vec<AssetIssue>, path: impl Into<String>, message: impl Into<String>) {
    issues.push(AssetIssue {
        path: path.into(),
        message: message.into(),
    });
}
