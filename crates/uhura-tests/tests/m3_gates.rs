//! The M3 gate (plan §M3): every pinned example resolves and evaluates to
//! a deterministic V snapshot (per-preview goldens); boot projections
//! auto-bind; `eval_view` emits no commands by construction (its output
//! type has nowhere to put one — the zero-commands property of
//! `uhura project` is structural). Derived examples resolve too since M4;
//! their replay goldens live in `m4_gates`.
//!
//! Bless goldens with `UPDATE_GOLDEN=1 cargo test -p uhura-tests`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use uhura_base::render_text;
use uhura_check::manifest::load_manifest;
use uhura_check::preview::PreviewPayload;
use uhura_check::{CheckInput, SourceInput, check};
use uhura_core::eval::{eval_fragment, eval_view};
use uhura_core::state::Projections;
use uhura_core::view::{Node, PageView, Snapshot};
use uhura_syntax::SourceKind;

include!("common/corpus.rs");

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/m3")
}

fn assert_golden(name: &str, actual: &str) {
    let path = golden_dir().join(name);
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(golden_dir()).expect("golden dir");
        std::fs::write(&path, actual).expect("write golden");
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing golden {name}; bless with UPDATE_GOLDEN=1"));
    assert_eq!(
        actual, expected,
        "golden `{name}` drifted; bless intentionally with UPDATE_GOLDEN=1"
    );
}

fn identity(_: &str, text: String) -> String {
    text
}

#[test]
fn every_pinned_preview_evaluates_to_its_v_golden() {
    let out = check(&corpus_input(true, &identity));
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.severity == uhura_base::Severity::Error),
        "corpus must check clean:\n{}",
        render_text(&out.diagnostics, &out.source_map)
    );
    let program = &out.lowered.as_ref().expect("clean check lowers").program;

    let mut pinned = 0usize;
    let mut derived = 0usize;
    let mut golden_names = Vec::new();

    for preview in &out.previews {
        let subject = preview.subject.name();
        let name = format!("v-{}-{}.json", subject, preview.example);
        if preview.derived {
            // Replay-resolved — golden-pinned by the M4 gate.
            derived += 1;
            continue;
        }
        pinned += 1;
        let v_json = match &preview.payload {
            PreviewPayload::Page { u, x, .. } => {
                let snapshot = eval_view(program, u, x).unwrap_or_else(|e| panic!("{name}: {e}"));
                snapshot.to_canonical_string()
            }
            PreviewPayload::Fragment {
                surface,
                name: def_name,
                props,
                state,
                x,
            } => {
                let def = if *surface {
                    &program.surfaces[def_name]
                } else {
                    &program.components[def_name]
                };
                let node = eval_fragment(program, def, props, state, x)
                    .unwrap_or_else(|e| panic!("{name}: {e}"));
                uhura_base::to_canonical_json(&node.to_json())
            }
        };
        assert_golden(&name, &v_json);
        golden_names.push(name);
    }

    // The corpus example sets are fixed: the complete demo now includes
    // post/story detail, relationship lists, Search, Reels, and Create in
    // addition to the original feed/profile/component states.
    assert_eq!(pinned, 57, "pinned preview count");
    assert_eq!(derived, 34, "derived preview count");

    // Determinism: a fresh full run yields byte-identical V for a sample.
    let again = check(&corpus_input(true, &identity));
    let first_pinned = |previews: &[uhura_check::preview::ResolvedPreview]| -> String {
        previews
            .iter()
            .filter(|p| !p.derived)
            .find_map(|p| match &p.payload {
                PreviewPayload::Page { u, x, .. } => Some(
                    eval_view(program, u, x)
                        .expect("evaluates")
                        .to_canonical_string(),
                ),
                _ => None,
            })
            .expect("a pinned page preview exists")
    };
    assert_eq!(first_pinned(&out.previews), first_pinned(&again.previews));
}

#[test]
fn boot_projections_auto_bind_so_loading_previews_evaluate() {
    let out = check(&corpus_input(true, &identity));
    let program = &out.lowered.as_ref().expect("clean").program;
    // The feed `loading` example pins nothing — viewer comes from
    // `boot.viewer` (§6.1) and the availability match renders its loading
    // arm.
    let loading = out
        .previews
        .iter()
        .find(|p| p.subject.name().as_str() == "feed" && p.example == "loading")
        .expect("feed loading example");
    let PreviewPayload::Page { u, x, .. } = &loading.payload else {
        panic!("loading is a page preview");
    };
    assert!(
        x.snapshots
            .keys()
            .any(|(name, _)| name.as_str() == "viewer"),
        "viewer auto-bound from boot.viewer"
    );
    let snapshot: Snapshot = eval_view(program, u, x).expect("evaluates");
    let PageView { root, .. } = &snapshot.page;
    let rendered = snapshot.to_canonical_string();
    assert!(
        rendered.contains("Loading your feed…"),
        "the loading arm renders"
    );
    assert_no_dangling_keys(root, &mut BTreeMap::new());
}

/// Keys are sibling-unique everywhere (§8.1 — collisions unrepresentable).
fn assert_no_dangling_keys(node: &Node, _seen: &mut BTreeMap<String, ()>) {
    let mut seen = BTreeMap::new();
    for child in &node.children {
        assert!(
            seen.insert(child.key.clone(), ()).is_none(),
            "duplicate sibling key `{}` under `{}`",
            child.key,
            node.key
        );
        assert_no_dangling_keys(child, &mut seen);
    }
}

#[test]
fn fragments_render_prebuilt_descriptor_payloads() {
    let out = check(&corpus_input(true, &identity));
    let program = &out.lowered.as_ref().expect("clean").program;
    // post-card image-post: the like button's descriptor payload is
    // prebuilt with the real post id — never a template (§8.1).
    let preview = out
        .previews
        .iter()
        .find(|p| p.subject.name().as_str() == "post-card" && p.example == "image-post")
        .expect("post-card image-post");
    let PreviewPayload::Fragment {
        props, state, x, ..
    } = &preview.payload
    else {
        panic!("image-post is a fragment preview");
    };
    let def = &program.components[&uhura_base::Ident::new("post-card").unwrap()];
    let node = eval_fragment(program, def, props, state, x).expect("evaluates");
    let rendered = uhura_base::to_canonical_json(&node.to_json());
    assert!(
        rendered.contains(r#""payload":{"now-liked":true,"post":"post-lena-glaze"}"#),
        "prebuilt payload with the concrete post id:\n{rendered}"
    );
    let empty = Projections::default();
    let _ = empty; // fragments carry their own X; nothing global leaks in
}
