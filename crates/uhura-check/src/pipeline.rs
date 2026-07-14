//! The check pipeline driver (§12.2 order): parse → routes/resolve →
//! catalog pin → port link + lock → typecheck (stores) → markup rules →
//! style checks → examples legality → lower (zero-error gated). Pure over
//! in-memory inputs; the CLI does every file read and write.

use std::collections::{BTreeMap, BTreeSet};

use uhura_base::{Diagnostic, Ident, SourceMap, Span, codes, has_errors};
use uhura_port::PortContract;
use uhura_syntax::{Parsed, SourceKind, parse};

use crate::catalog::{Catalog, load_catalog};
use crate::examples::check_examples;
use crate::infer::check_store;
use crate::lower::{Lowered, lower};
use crate::manifest::Manifest;
use crate::markup::{check_markup, interactive_content_memo};
use crate::metadata::{AuthoringProjection, collect_authoring};
use crate::resolve::{ParsedSource, resolve};
use crate::style::{check_class_existence, check_style_block, compile_stylesheet, theme_classes};
use crate::types::PortTypes;

pub struct SourceInput {
    pub rel_path: String,
    pub text: String,
    pub kind: SourceKind,
}

pub struct CheckInput {
    pub manifest: Manifest,
    pub manifest_rel_path: String,
    pub manifest_text: String,
    /// (corpus-relative path, text) — `None` text = unreadable/missing.
    pub catalog_file: (String, Option<String>),
    /// Port name → (rel path, text).
    pub port_files: BTreeMap<Ident, (String, Option<String>)>,
    pub sources: Vec<SourceInput>,
    pub theme_css: Option<(String, String)>,
    /// Fixture name → (rel path, text) — from the manifest's `[fixtures]`.
    pub fixture_files: BTreeMap<Ident, (String, Option<String>)>,
    /// Existing `uhura.lock` content, if any.
    pub lock_text: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockStatus {
    /// No lock existed — the CLI writes the computed one (micro-decision #6).
    Absent,
    Match,
    /// Pins differ — diagnosed as an error.
    Drift,
}

pub struct CheckOutput {
    pub diagnostics: Vec<Diagnostic>,
    pub source_map: SourceMap,
    /// Present iff the pipeline finished with zero errors.
    pub lowered: Option<Lowered>,
    /// Resolved example previews (empty unless the check came up clean).
    pub previews: Vec<crate::preview::ResolvedPreview>,
    /// theme.css + `<style>` blocks in path order (ships beside the IR).
    pub stylesheet: String,
    /// The canonical lock content for this input.
    pub lock_computed: String,
    pub lock_status: LockStatus,
    /// Checked docs/annotations and their source targets. Available even when
    /// unrelated diagnostics gate runtime artifacts.
    pub authoring: AuthoringProjection,
}

pub fn check(input: &CheckInput) -> CheckOutput {
    let mut sm = SourceMap::new();
    let mut diags: Vec<Diagnostic> = Vec::new();

    let manifest_file = sm.add(input.manifest_rel_path.clone(), input.manifest_text.clone());

    // ── catalog load + pin ─────────────────────────────────────────────
    let (catalog_path, catalog_text) = &input.catalog_file;
    let catalog_file = sm.add(
        catalog_path.clone(),
        catalog_text.clone().unwrap_or_default(),
    );
    let catalog: Option<Catalog> = match catalog_text {
        None => {
            diags.push(Diagnostic::error(
                codes::INVALID_CATALOG.0,
                codes::INVALID_CATALOG.1,
                format!("catalog `{catalog_path}` is missing or unreadable"),
                Span::new(manifest_file, 0, 0),
            ));
            None
        }
        Some(text) => match load_catalog(text) {
            Ok(c) => Some(c),
            Err(issues) => {
                for issue in issues {
                    diags.push(Diagnostic::error(
                        codes::INVALID_CATALOG.0,
                        codes::INVALID_CATALOG.1,
                        format!("{}: {}", issue.path, issue.message),
                        Span::new(catalog_file, 0, 0),
                    ));
                }
                None
            }
        },
    };

    // ── port contracts (link L1: manifest name == contract name) ──────
    let mut ports: BTreeMap<Ident, (PortContract, PortTypes)> = BTreeMap::new();
    for (declared_name, (rel_path, text)) in &input.port_files {
        let port_file = sm.add(rel_path.clone(), text.clone().unwrap_or_default());
        let Some(text) = text else {
            diags.push(Diagnostic::error(
                codes::INVALID_PORT_CONTRACT.0,
                codes::INVALID_PORT_CONTRACT.1,
                format!("port contract `{rel_path}` is missing or unreadable"),
                Span::new(manifest_file, 0, 0),
            ));
            continue;
        };
        match uhura_port::load_port_contract(text) {
            Err(issues) => {
                for issue in issues {
                    diags.push(Diagnostic::error(
                        codes::INVALID_PORT_CONTRACT.0,
                        codes::INVALID_PORT_CONTRACT.1,
                        format!("{}: {}", issue.path, issue.message),
                        Span::new(port_file, 0, 0),
                    ));
                }
            }
            Ok(contract) => {
                if contract.name != *declared_name {
                    diags.push(Diagnostic::error(
                        codes::PORT_NAME_MISMATCH.0,
                        codes::PORT_NAME_MISMATCH.1,
                        format!(
                            "the manifest binds `{declared_name}` but the contract says \
                             `[port] name = \"{}\"`",
                            contract.name
                        ),
                        Span::new(port_file, 0, 0),
                    ));
                }
                let types = PortTypes::build(&contract);
                ports.insert(declared_name.clone(), (contract, types));
            }
        }
    }

    // ── lock (computed from what loaded; compared before source work) ──
    let lock_computed = render_lock(catalog.as_ref(), &ports);
    let lock_status = match &input.lock_text {
        None => LockStatus::Absent,
        Some(existing) => {
            if lock_pins(existing) == lock_pins(&lock_computed) {
                LockStatus::Match
            } else {
                diags.push(
                    Diagnostic::error(
                        codes::LOCK_DRIFT.0,
                        codes::LOCK_DRIFT.1,
                        "contract pins drifted from uhura.lock — a contract or catalog changed \
                         shape (§9.1: drift is a link error, never silent)"
                            .to_string(),
                        Span::new(manifest_file, 0, 0),
                    )
                    .with_note(
                        "if the change is intentional, delete uhura.lock and re-run `uhura check` \
                         to re-pin"
                            .to_string(),
                    ),
                );
                LockStatus::Drift
            }
        }
    };

    // ── parse every source ─────────────────────────────────────────────
    let mut sources: Vec<ParsedSource> = Vec::new();
    for src in &input.sources {
        let file = sm.add(src.rel_path.clone(), src.text.clone());
        let out = parse(file, &src.text, src.kind);
        diags.extend(out.diagnostics);
        sources.push(ParsedSource {
            file,
            rel_path: src.rel_path.clone(),
            kind: src.kind,
            parsed: out.parsed,
        });
    }

    // ── resolve ────────────────────────────────────────────────────────
    let mut resolved = resolve(
        &sources,
        ports,
        &input.manifest.entry,
        manifest_file,
        &mut diags,
    );

    // ── typecheck stores; fill machine-event signatures ────────────────
    let mut all_events = Vec::new();
    for (kind, name, env) in resolved
        .pages
        .iter()
        .map(|(n, e)| (0u8, n.clone(), e))
        .chain(resolved.components.iter().map(|(n, e)| (1, n.clone(), e)))
        .chain(resolved.surfaces.iter().map(|(n, e)| (2, n.clone(), e)))
    {
        let Parsed::Module(ast) = &sources[env.source].parsed else {
            continue;
        };
        if let Some(store) = &ast.store {
            let events = check_store(env, &resolved, store, &mut diags);
            all_events.push((kind, name, events));
        }
    }
    for (kind, name, events) in all_events {
        let env = match kind {
            0 => resolved.pages.get_mut(&name),
            1 => resolved.components.get_mut(&name),
            _ => resolved.surfaces.get_mut(&name),
        };
        if let Some(env) = env {
            env.events = events;
        }
    }

    // ── markup + style (need the catalog) ──────────────────────────────
    let mut stylesheet = String::new();
    if let Some(catalog) = &catalog {
        let memo = interactive_content_memo(&resolved, &sources, catalog);

        let mut class_refs: Vec<(String, Span)> = Vec::new();
        let mut defined_classes: BTreeSet<String> = input
            .theme_css
            .as_ref()
            .map(|(path, css)| {
                let theme_file = sm.add(path.clone(), css.clone());
                theme_classes(theme_file, css)
            })
            .unwrap_or_default();
        let mut style_blocks: Vec<(String, String)> = Vec::new();

        for env in resolved
            .pages
            .values()
            .chain(resolved.components.values())
            .chain(resolved.surfaces.values())
        {
            let src = &sources[env.source];
            let Parsed::Module(ast) = &src.parsed else {
                continue;
            };
            let facts = check_markup(
                env,
                &resolved,
                catalog,
                &memo,
                &ast.markup,
                Span::new(env.file, 0, 0),
                &mut diags,
            );
            class_refs.extend(facts.class_refs);
            if let Some(style) = &ast.style {
                defined_classes.extend(check_style_block(
                    env.kind.name().as_str(),
                    style,
                    &mut diags,
                ));
                style_blocks.push((src.rel_path.clone(), style.raw.clone()));
            }
        }

        check_class_existence(&class_refs, &defined_classes, &mut diags);
        style_blocks.sort_by(|a, b| a.0.cmp(&b.0));
        stylesheet = compile_stylesheet(
            input.theme_css.as_ref().map(|(_, css)| css.as_str()),
            &style_blocks,
        );
    }

    // ── examples clause legality ───────────────────────────────────────
    let env_by_source: BTreeMap<usize, &crate::resolve::DefEnv> = resolved
        .pages
        .values()
        .chain(resolved.components.values())
        .chain(resolved.surfaces.values())
        .map(|env| (env.source, env))
        .collect();
    for (examples_idx, subject_idx) in &resolved.example_subjects {
        let Parsed::Examples(file) = &sources[*examples_idx].parsed else {
            continue;
        };
        let Some(subject) = env_by_source.get(subject_idx) else {
            continue; // subject failed resolution; already diagnosed
        };
        check_examples(
            file,
            subject,
            &resolved,
            &input.manifest,
            Span::new(sources[*examples_idx].file, 0, 0),
            &mut diags,
        );
    }

    // ── checked authoring metadata (separate from runtime IR) ─────────
    let authoring = collect_authoring(&sources, &resolved, catalog.as_ref(), &sm, &mut diags);
    if let Some(message) = authoring.template_origin_error() {
        diags.push(Diagnostic::error(
            codes::TEMPLATE_ORIGIN_COVERAGE.0,
            codes::TEMPLATE_ORIGIN_COVERAGE.1,
            message,
            Span::new(manifest_file, 0, 0),
        ));
    }

    // ── lower (zero-error gated) ───────────────────────────────────────
    let lowered = match (&catalog, has_errors(&diags)) {
        (Some(catalog), false) => {
            let lowered = lower(&input.manifest, &resolved, catalog, &sources);
            match lowered.with_template_origins(authoring.template_origins.clone()) {
                Ok(lowered) => Some(lowered),
                Err(message) => {
                    diags.push(Diagnostic::error(
                        codes::TEMPLATE_ORIGIN_COVERAGE.0,
                        codes::TEMPLATE_ORIGIN_COVERAGE.1,
                        message,
                        Span::new(manifest_file, 0, 0),
                    ));
                    None
                }
            }
        }
        _ => None,
    };

    // ── fixtures + pinned example resolution (§6.2) ────────────────────
    let mut previews = Vec::new();
    if let Some(lowered) = &lowered {
        let mut fixtures: BTreeMap<Ident, crate::fixture::FixtureData> = BTreeMap::new();
        let mut fixtures_ok = true;
        for (name, (rel_path, text)) in &input.fixture_files {
            let fixture_file = sm.add(rel_path.clone(), text.clone().unwrap_or_default());
            let Some(text) = text else {
                diags.push(Diagnostic::error(
                    codes::INVALID_FIXTURE.0,
                    codes::INVALID_FIXTURE.1,
                    format!("fixture `{rel_path}` is missing or unreadable"),
                    Span::new(manifest_file, 0, 0),
                ));
                fixtures_ok = false;
                continue;
            };
            match crate::fixture::load_fixture(text) {
                Ok(data) => {
                    fixtures.insert(name.clone(), data);
                }
                Err(issues) => {
                    for issue in issues {
                        diags.push(Diagnostic::error(
                            codes::INVALID_FIXTURE.0,
                            codes::INVALID_FIXTURE.1,
                            format!("{}: {}", issue.path, issue.message),
                            Span::new(fixture_file, 0, 0),
                        ));
                    }
                    fixtures_ok = false;
                }
            }
        }
        if fixtures_ok {
            previews = crate::preview::resolve_previews(
                &lowered.program,
                &resolved,
                &sources,
                &fixtures,
                &authoring,
                &mut diags,
            );
        }
    }
    // Late errors (fixtures, pins) gate the artifacts like early ones.
    let lowered = if has_errors(&diags) { None } else { lowered };
    let previews = if has_errors(&diags) {
        Vec::new()
    } else {
        previews
    };

    CheckOutput {
        diagnostics: diags,
        source_map: sm,
        lowered,
        previews,
        stylesheet,
        lock_computed,
        lock_status,
        authoring: authoring.projection,
    }
}

/// The canonical lock text: one pin line per contract, sorted.
fn render_lock(
    catalog: Option<&Catalog>,
    ports: &BTreeMap<Ident, (PortContract, PortTypes)>,
) -> String {
    let mut out = String::from(
        "# uhura.lock — canonical contract pins (§9.1). `uhura check` writes this\n\
         # file when absent and errors on drift; delete it to re-pin intentionally.\n",
    );
    if let Some(c) = catalog {
        out.push_str(&format!(
            "catalog {} {} sha256:{}\n",
            c.name,
            c.version,
            c.canonical_hash()
        ));
    }
    for (name, (contract, _)) in ports {
        out.push_str(&format!(
            "port {name} {} sha256:{}\n",
            contract.version,
            contract.canonical_hash()
        ));
    }
    out
}

/// Pin lines only — comments and blank lines never count as drift.
fn lock_pins(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
}
