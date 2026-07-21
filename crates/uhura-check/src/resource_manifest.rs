//! Supplemental renderer resources declared by `uhura.toml`.
//!
//! Machine identity, presentation selection, lifetime, configuration, and
//! port adapters belong to `host.toml`. This manifest deliberately owns only
//! project resources that are checked before Editor or Play publication.

use std::collections::BTreeMap;

use uhura_base::Ident;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResourceManifest {
    pub assets: AssetsConfig,
    /// Icon registry selection plus any project-local font families. `lucide`
    /// is always available as the built-in good default.
    pub icons: IconsConfig,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetsConfig {
    /// Project-relative path to the existing asset manifest, when one is
    /// supplied. Asset realization remains owned by the host asset plane.
    pub manifest: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconsConfig {
    pub default: Ident,
    pub families: BTreeMap<Ident, IconFamilyConfig>,
}

impl Default for IconsConfig {
    fn default() -> Self {
        Self {
            default: Ident::new("lucide").expect("built-in icon family is a valid identifier"),
            families: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconFamilyConfig {
    /// Project-relative WOFF2 file.
    pub font: String,
    /// Project-relative JSON map from icon name to decimal Unicode codepoint.
    pub glyphs: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceManifestIssue {
    pub path: String,
    pub message: String,
}

/// Parse the closed `uhura.toml` resource surface.
///
/// Empty input is valid and selects the bundled Lucide family. Unknown
/// top-level sections are errors so retired app/catalog/port/fixture/play
/// configuration cannot silently survive as inert configuration.
pub fn load_resource_manifest(text: &str) -> Result<ResourceManifest, Vec<ResourceManifestIssue>> {
    let table: toml::Table = match text.parse() {
        Ok(table) => table,
        Err(error) => {
            return Err(vec![ResourceManifestIssue {
                path: String::new(),
                message: format!("invalid TOML: {error}"),
            }]);
        }
    };

    let mut issues = Vec::new();
    for key in table.keys() {
        if !["assets", "icons"].contains(&key.as_str()) {
            issue(&mut issues, key, format!("unknown key `{key}`"));
        }
    }

    let assets = parse_assets(&table, &mut issues);
    let icons = parse_icons(&table, &mut issues);
    if issues.is_empty() {
        Ok(ResourceManifest { assets, icons })
    } else {
        Err(issues)
    }
}

fn parse_assets(table: &toml::Table, issues: &mut Vec<ResourceManifestIssue>) -> AssetsConfig {
    let Some(value) = table.get("assets") else {
        return AssetsConfig::default();
    };
    let Some(assets) = value.as_table() else {
        issue(issues, "assets", "expected an `[assets]` table");
        return AssetsConfig::default();
    };
    for key in assets.keys() {
        if key != "manifest" {
            issue(
                issues,
                format!("assets.{key}"),
                format!("unknown key `{key}`"),
            );
        }
    }
    let manifest = match assets.get("manifest") {
        None => None,
        Some(value) => local_path(value, "assets.manifest", issues),
    };
    AssetsConfig { manifest }
}

fn parse_icons(table: &toml::Table, issues: &mut Vec<ResourceManifestIssue>) -> IconsConfig {
    let mut out = IconsConfig::default();
    let Some(value) = table.get("icons") else {
        return out;
    };
    let Some(icons) = value.as_table() else {
        issue(issues, "icons", "expected an `[icons]` table");
        return out;
    };

    if let Some(value) = icons.get("default") {
        match value.as_str().map(Ident::new) {
            Some(Ok(default)) => out.default = default,
            Some(Err(error)) => issue(issues, "icons.default", error.to_string()),
            None => issue(
                issues,
                "icons.default",
                "expected an icon family name string",
            ),
        }
    }

    for (name, value) in icons.iter().filter(|(name, _)| name.as_str() != "default") {
        let path = format!("icons.{name}");
        let Ok(name) = Ident::new(name) else {
            issue(
                issues,
                &path,
                format!("`{name}` is not a lowercase kebab-case identifier"),
            );
            continue;
        };
        if name.as_str() == "lucide" {
            issue(
                issues,
                &path,
                "`lucide` is built in and cannot be replaced locally",
            );
            continue;
        }
        let Some(family) = value.as_table() else {
            issue(issues, &path, "expected `{ font = ..., glyphs = ... }`");
            continue;
        };
        for key in family.keys() {
            if !["font", "glyphs"].contains(&key.as_str()) {
                issue(
                    issues,
                    format!("{path}.{key}"),
                    format!("unknown key `{key}`"),
                );
            }
        }
        let font = required_local_path(family.get("font"), &format!("{path}.font"), issues);
        let glyphs = required_local_path(family.get("glyphs"), &format!("{path}.glyphs"), issues);
        if let (Some(font), Some(glyphs)) = (font, glyphs) {
            out.families.insert(name, IconFamilyConfig { font, glyphs });
        }
    }

    if out.default.as_str() != "lucide" && !out.families.contains_key(&out.default) {
        issue(
            issues,
            "icons.default",
            format!(
                "`{}` is neither the built-in `lucide` family nor a declared local family",
                out.default
            ),
        );
    }
    out
}

fn required_local_path(
    value: Option<&toml::Value>,
    path: &str,
    issues: &mut Vec<ResourceManifestIssue>,
) -> Option<String> {
    match value {
        Some(value) => local_path(value, path, issues),
        None => {
            issue(issues, path, "missing required path string");
            None
        }
    }
}

fn local_path(
    value: &toml::Value,
    path: &str,
    issues: &mut Vec<ResourceManifestIssue>,
) -> Option<String> {
    match value.as_str() {
        Some(value) if safe_project_path(value) => Some(value.to_string()),
        Some(_) => {
            issue(issues, path, "expected a safe project-relative path");
            None
        }
        None => {
            issue(issues, path, "expected a path string");
            None
        }
    }
}

fn safe_project_path(path: &str) -> bool {
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

fn issue(
    issues: &mut Vec<ResourceManifestIssue>,
    path: impl Into<String>,
    message: impl Into<String>,
) {
    issues.push(ResourceManifestIssue {
        path: path.into(),
        message: message.into(),
    });
}
