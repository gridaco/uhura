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
    /// Play profile name → fixture/script test double plus an optional
    /// browser-only provider module for `uhura dev`.
    pub play: BTreeMap<Ident, PlayProfile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayProfile {
    pub fixture: Ident,
    pub script: Ident,
    /// A live provider used only by the play shell. The fixture and script
    /// remain required because checks, previews, and traces keep using them.
    pub provider: Option<PlayProvider>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayProvider {
    /// Corpus-relative ES module path. `uhura dev` reads the bytes into its
    /// last-good build and exposes them at a content-addressed `/provider.js` URL.
    pub module: String,
    /// Opaque string settings passed to the provider module in `/play.json`.
    pub config: BTreeMap<String, String>,
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
                    "expected `{ fixture = …, script = …, provider = …? }`".into(),
                );
                continue;
            };
            for key in profile.keys() {
                if !["fixture", "script", "provider"].contains(&key.as_str()) {
                    push(
                        &mut issues,
                        &format!("{path}.{key}"),
                        format!("unknown key `{key}`"),
                    );
                }
            }
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
            let provider = parse_play_provider(&mut issues, &path, profile.get("provider"));
            if let (Some(fixture), Some(script)) = (fixture, script) {
                play.insert(
                    profile_name,
                    PlayProfile {
                        fixture,
                        script,
                        provider,
                    },
                );
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

fn parse_play_provider(
    issues: &mut Vec<ManifestIssue>,
    profile_path: &str,
    value: Option<&toml::Value>,
) -> Option<PlayProvider> {
    let value = value?;
    let path = format!("{profile_path}.provider");
    let Some(table) = value.as_table() else {
        issues.push(ManifestIssue {
            path,
            message: "expected a `{ module = …, config = { … } }` table".into(),
        });
        return None;
    };
    for key in table.keys() {
        if !["module", "config"].contains(&key.as_str()) {
            issues.push(ManifestIssue {
                path: format!("{path}.{key}"),
                message: format!("unknown key `{key}`"),
            });
        }
    }

    let module = match table.get("module").and_then(toml::Value::as_str) {
        Some(module) if safe_corpus_path(module) => Some(module.to_string()),
        Some(_) => {
            issues.push(ManifestIssue {
                path: format!("{path}.module"),
                message: "expected a safe corpus-relative module path".into(),
            });
            None
        }
        None => {
            issues.push(ManifestIssue {
                path: format!("{path}.module"),
                message: "missing required string".into(),
            });
            None
        }
    };

    let mut config = BTreeMap::new();
    match table.get("config") {
        None => {}
        Some(toml::Value::Table(entries)) => {
            for (key, value) in entries {
                match value.as_str() {
                    Some(value) => {
                        config.insert(key.clone(), value.to_string());
                    }
                    None => issues.push(ManifestIssue {
                        path: format!("{path}.config.{key}"),
                        message: "provider config values must be strings".into(),
                    }),
                }
            }
        }
        Some(_) => issues.push(ManifestIssue {
            path: format!("{path}.config"),
            message: "expected a table of string values".into(),
        }),
    }

    module.map(|module| PlayProvider { module, config })
}

fn safe_corpus_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && path
            .split('/')
            .all(|segment| !segment.is_empty() && !matches!(segment, "." | ".."))
}
