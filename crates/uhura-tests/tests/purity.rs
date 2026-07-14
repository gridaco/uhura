//! Purity is a failing test, not a convention (design §12.1, §9.6).
//!
//! Asserts, via `cargo metadata`, that:
//! 1. uhura-core's transitive normal-dependency closure is exactly
//!    {uhura-base, uhura-port} — the foundation and the seam, nothing else;
//! 2. uhura-core can never reach uhura-fixture (Spock-replaceability: the
//!    core has no provider-identity input);
//! 3. no I/O-capable external crate leaks into the closures of the pure
//!    crates (core, check, view, ir, value, diag, port, syntax).

use std::collections::{BTreeMap, BTreeSet};

use cargo_metadata::{DependencyKind, Metadata, MetadataCommand};

fn workspace_metadata() -> Metadata {
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml");
    MetadataCommand::new()
        .manifest_path(manifest)
        .exec()
        .expect("cargo metadata")
}

/// Transitive closure of normal (non-dev, non-build) dependencies of `root`,
/// as package names. Excludes `root` itself.
fn normal_dep_closure(meta: &Metadata, root: &str) -> BTreeSet<String> {
    let resolve = meta.resolve.as_ref().expect("resolve graph");
    let name_of: BTreeMap<_, _> = meta
        .packages
        .iter()
        .map(|p| (p.id.clone(), p.name.clone()))
        .collect();
    let nodes: BTreeMap<_, _> = resolve.nodes.iter().map(|n| (n.id.clone(), n)).collect();

    let root_id = meta
        .packages
        .iter()
        .find(|p| p.name == root)
        .unwrap_or_else(|| panic!("package {root} not found"))
        .id
        .clone();

    let mut seen = BTreeSet::new();
    let mut stack = vec![root_id];
    while let Some(id) = stack.pop() {
        let Some(node) = nodes.get(&id) else { continue };
        for dep in &node.deps {
            let normal = dep.dep_kinds.is_empty()
                || dep
                    .dep_kinds
                    .iter()
                    .any(|k| k.kind == DependencyKind::Normal);
            if !normal {
                continue;
            }
            let name = name_of[&dep.pkg].clone();
            if seen.insert(name) {
                stack.push(dep.pkg.clone());
            }
        }
    }
    seen
}

fn workspace_names(meta: &Metadata) -> BTreeSet<String> {
    let members: BTreeSet<_> = meta.workspace_members.iter().cloned().collect();
    meta.packages
        .iter()
        .filter(|p| members.contains(&p.id))
        .map(|p| p.name.clone())
        .collect()
}

/// External crates that must never appear in a pure crate's closure. This is
/// the meaningful boundary: filesystem/network/clock/UI capability.
const DENIED_EXTERNALS: &[&str] = &[
    "tokio",
    "async-std",
    "notify",
    "tiny_http",
    "hyper",
    "reqwest",
    "clap",
    "wasm-bindgen",
    "image",
    "rand",
    "chrono",
    "time",
    "ureq",
    "mio",
];

#[test]
fn core_closure_is_exactly_the_seam() {
    let meta = workspace_metadata();
    let ws = workspace_names(&meta);
    let closure = normal_dep_closure(&meta, "uhura-core");

    let ws_in_closure: BTreeSet<_> = closure
        .iter()
        .filter(|n| ws.contains(*n))
        .cloned()
        .collect();
    let expected: BTreeSet<String> = ["uhura-base", "uhura-port"]
        .into_iter()
        .map(String::from)
        .collect();

    assert_eq!(
        ws_in_closure, expected,
        "uhura-core's workspace dependency closure changed — the design \
         (§12.1) fixes it to exactly {{base, port}}"
    );
    assert!(
        !closure.contains("uhura-fixture"),
        "uhura-core must never reach uhura-fixture (§9.6 Spock-replaceability)"
    );
}

/// The port crate's `toml` feature must be OFF on core's edge (§12.1).
///
/// `cargo metadata`'s resolve graph is workspace-feature-unified (uhura-check
/// legitimately enables uhura-port/toml), so it cannot answer this question.
/// `cargo tree -p uhura-core` resolves features for core's build alone.
#[test]
fn core_build_alone_never_compiles_toml() {
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml");
    let out = std::process::Command::new(env!("CARGO"))
        .args([
            "tree",
            "-p",
            "uhura-core",
            "-e",
            "normal",
            "--prefix",
            "none",
        ])
        .arg("--manifest-path")
        .arg(manifest)
        .output()
        .expect("cargo tree");
    assert!(out.status.success(), "cargo tree failed");
    let tree = String::from_utf8_lossy(&out.stdout);
    assert!(
        !tree
            .lines()
            .any(|l| l.starts_with("toml ") || l.starts_with("toml_")),
        "toml is compiled into a core-only build — uhura-port's toml feature \
         must stay off on the core edge (§12.1)\n{tree}"
    );
}

#[test]
fn pure_crates_reach_no_io_capable_externals() {
    let meta = workspace_metadata();
    for krate in [
        "uhura-core",
        "uhura-check",
        "uhura-base",
        "uhura-port",
        "uhura-syntax",
        "uhura-fixture",
        "uhura-editor-model",
    ] {
        let closure = normal_dep_closure(&meta, krate);
        for denied in DENIED_EXTERNALS {
            assert!(
                !closure.contains(*denied),
                "{krate} transitively depends on {denied} — pure crates may \
                 not reach I/O-capable externals (§12.1)"
            );
        }
    }
}

#[test]
fn check_never_reaches_the_fixture_driver() {
    let meta = workspace_metadata();
    let closure = normal_dep_closure(&meta, "uhura-check");
    assert!(
        !closure.contains("uhura-fixture"),
        "uhura-check reads fixture DATA (its own parser) but must never link \
         the fixture DRIVER — example replay folds public step_u only (§6.2)"
    );
}
