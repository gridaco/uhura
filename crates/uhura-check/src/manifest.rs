//! The `uhura.toml` app manifest (design §3): entry route, catalog pin,
//! port bindings, fixtures, play profiles. Parsed from text; the CLI reads
//! the file.

use std::collections::BTreeMap;

use uhura_base::Ident;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub app_name: Ident,
    /// The entry route name; validated against the route table.
    pub entry: Ident,
    /// Corpus-relative path to the catalog TOML.
    pub catalog_path: String,
    /// Port name → corpus-relative contract path. The name must equal the
    /// contract's own `[port] name` (link rule L1).
    pub ports: BTreeMap<Ident, String>,
    /// Fixture name → corpus-relative data path.
    pub fixtures: BTreeMap<Ident, String>,
    /// Corpus-relative path to the asset manifest, if any.
    pub assets_manifest: Option<String>,
    /// Play profile name → (fixture, script).
    pub play: BTreeMap<Ident, PlayProfile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayProfile {
    pub fixture: Ident,
    pub script: Ident,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestIssue {
    pub path: String,
    pub message: String,
}

pub fn load_manifest(text: &str) -> Result<Manifest, Vec<ManifestIssue>> {
    let mut issues: Vec<ManifestIssue> = Vec::new();
    let push = |issues: &mut Vec<ManifestIssue>, path: &str, message: String| {
        issues.push(ManifestIssue {
            path: path.to_string(),
            message,
        });
    };

    let table: toml::Table = match text.parse() {
        Ok(t) => t,
        Err(e) => {
            return Err(vec![ManifestIssue {
                path: String::new(),
                message: format!("invalid TOML: {e}"),
            }]);
        }
    };
    for key in table.keys() {
        if !["app", "catalog", "ports", "fixtures", "assets", "play"].contains(&key.as_str()) {
            push(&mut issues, key, format!("unknown key `{key}`"));
        }
    }

    let ident_at = |issues: &mut Vec<ManifestIssue>, path: &str, v: Option<&toml::Value>| match v
        .and_then(toml::Value::as_str)
    {
        Some(s) => match Ident::new(s) {
            Ok(i) => Some(i),
            Err(e) => {
                issues.push(ManifestIssue {
                    path: path.to_string(),
                    message: e.to_string(),
                });
                None
            }
        },
        None => {
            issues.push(ManifestIssue {
                path: path.to_string(),
                message: "missing required string".into(),
            });
            None
        }
    };

    let app = table.get("app").and_then(toml::Value::as_table);
    let app_name = app.and_then(|t| ident_at(&mut issues, "app.name", t.get("name")));
    let entry = app.and_then(|t| ident_at(&mut issues, "app.entry", t.get("entry")));
    if app.is_none() {
        push(&mut issues, "app", "missing `[app]` section".into());
    }

    let catalog_path = table
        .get("catalog")
        .and_then(toml::Value::as_table)
        .and_then(|t| t.get("path"))
        .and_then(toml::Value::as_str)
        .map(ToString::to_string);
    if catalog_path.is_none() {
        push(&mut issues, "catalog.path", "missing catalog path".into());
    }

    let string_map = |issues: &mut Vec<ManifestIssue>, section: &str| {
        let mut out = BTreeMap::new();
        if let Some(toml::Value::Table(t)) = table.get(section) {
            for (name, v) in t {
                let path = format!("{section}.{name}");
                match (Ident::new(name), v.as_str()) {
                    (Ok(ident), Some(s)) => {
                        out.insert(ident, s.to_string());
                    }
                    (Err(e), _) => issues.push(ManifestIssue {
                        path,
                        message: e.to_string(),
                    }),
                    (_, None) => issues.push(ManifestIssue {
                        path,
                        message: "expected a path string".into(),
                    }),
                }
            }
        }
        out
    };
    let ports = string_map(&mut issues, "ports");
    let fixtures = string_map(&mut issues, "fixtures");

    let assets_manifest = table
        .get("assets")
        .and_then(toml::Value::as_table)
        .and_then(|t| t.get("manifest"))
        .and_then(toml::Value::as_str)
        .map(ToString::to_string);

    let mut play = BTreeMap::new();
    if let Some(toml::Value::Table(t)) = table.get("play") {
        for (name, v) in t {
            let path = format!("play.{name}");
            let Ok(profile_name) = Ident::new(name) else {
                push(&mut issues, &path, format!("`{name}` is not kebab-case"));
                continue;
            };
            let Some(profile) = v.as_table() else {
                push(
                    &mut issues,
                    &path,
                    "expected `{ fixture = …, script = … }`".into(),
                );
                continue;
            };
            let fixture = ident_at(
                &mut issues,
                &format!("{path}.fixture"),
                profile.get("fixture"),
            );
            let script = ident_at(
                &mut issues,
                &format!("{path}.script"),
                profile.get("script"),
            );
            if let (Some(fixture), Some(script)) = (fixture, script) {
                play.insert(profile_name, PlayProfile { fixture, script });
            }
        }
    }

    for (name, profile) in &play {
        if !fixtures.contains_key(&profile.fixture) {
            push(
                &mut issues,
                &format!("play.{name}.fixture"),
                format!("`{}` is not a declared fixture", profile.fixture),
            );
        }
    }

    match (app_name, entry, catalog_path) {
        (Some(app_name), Some(entry), Some(catalog_path)) if issues.is_empty() => Ok(Manifest {
            app_name,
            entry,
            catalog_path,
            ports,
            fixtures,
            assets_manifest,
            play,
        }),
        _ => Err(issues),
    }
}
