//! The M4 gate (plan §M4): golden JSONL traces for the six CI scripts
//! (§11.4) through the REAL harness (`uhura_cli::cmd::trace::run_script` —
//! the exact code the binary runs), structural asserts over those traces,
//! replay-derived preview V goldens, a demo smoke run, and the
//! self-verifying-design check: a guard edit in the feed page fails a
//! derived example.
//!
//! Bless goldens with `UPDATE_GOLDEN=1 cargo test -p uhura-tests`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use uhura_base::render_text;
use uhura_check::manifest::load_manifest;
use uhura_check::preview::PreviewPayload;
use uhura_check::{CheckInput, SourceInput, check};
use uhura_cli::cmd::trace::run_script;
use uhura_core::eval::{eval_fragment, eval_view};
use uhura_core::ir::ProgramIr;
use uhura_syntax::SourceKind;

include!("common/corpus.rs");

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/m4")
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

fn checked_program() -> ProgramIr {
    let out = check(&corpus_input(true, &identity));
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.severity == uhura_base::Severity::Error),
        "corpus must check clean:\n{}",
        render_text(&out.diagnostics, &out.source_map)
    );
    out.lowered.expect("clean check lowers").program
}

fn read_corpus(rel: &str) -> String {
    std::fs::read_to_string(corpus_root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// Runs a script through the CLI harness; `expanded` embeds full V per
/// step (used by structural asserts, never goldened).
fn trace(program: &ProgramIr, script: &str, expanded: bool) -> Vec<serde_json::Value> {
    let fixture = read_corpus("fixtures/standard.toml");
    let script_text = read_corpus(&format!("fixtures/scripts/{script}.toml"));
    run_script(program, &fixture, &script_text, expanded)
        .unwrap_or_else(|e| panic!("{script}: {e}"))
        .iter()
        .map(|line| serde_json::from_str(line).expect("trace lines are JSON"))
        .collect()
}

// ── golden traces (the conformance artifact, §7.5) ──────────────────────────

#[test]
fn the_six_ci_scripts_trace_to_their_goldens() {
    let program = checked_program();
    for script in [
        "like-ok",
        "like-refused",
        "comment-ok",
        "paginate",
        "feed-failed",
        "feed-empty",
    ] {
        let fixture = read_corpus("fixtures/standard.toml");
        let script_text = read_corpus(&format!("fixtures/scripts/{script}.toml"));
        let lines = run_script(&program, &fixture, &script_text, false)
            .unwrap_or_else(|e| panic!("{script}: {e}"));
        let mut jsonl = lines.join("\n");
        jsonl.push('\n');
        assert_golden(&format!("trace-{script}.jsonl"), &jsonl);

        // Byte-determinism: the same fold twice.
        let again = run_script(&program, &fixture, &script_text, false).expect("reruns");
        assert_eq!(lines, again, "{script}: traces are byte-deterministic");
    }
}

// ── structural asserts (§11.4 / §13) ────────────────────────────────────────

fn commands<'a>(steps: &'a [serde_json::Value], name: &str) -> Vec<&'a serde_json::Value> {
    steps
        .iter()
        .flat_map(|s| s["c"].as_array().into_iter().flatten())
        .filter(|c| c["command"] == name)
        .collect()
}

fn step_index(steps: &[serde_json::Value], pred: impl Fn(&serde_json::Value) -> bool) -> usize {
    steps.iter().position(pred).expect("a matching step exists")
}

/// Collects every node depth-first from an expanded-V subtree.
fn walk_nodes<'a>(node: &'a serde_json::Value, out: &mut Vec<&'a serde_json::Value>) {
    out.push(node);
    for child in node["children"].as_array().into_iter().flatten() {
        walk_nodes(child, out);
    }
}

fn find_text(node: &serde_json::Value, needle: &str) -> bool {
    let mut nodes = Vec::new();
    walk_nodes(node, &mut nodes);
    nodes.iter().any(|n| {
        n["props"]["content"]["v"]
            .as_str()
            .is_some_and(|s| s.contains(needle))
    })
}

#[test]
fn like_ok_sends_exactly_one_command_and_optimism_precedes_settlement() {
    let program = checked_program();
    let steps = trace(&program, "like-ok", false);
    assert_eq!(
        commands(&steps, "like-post").len(),
        1,
        "one command per press"
    );

    let press = step_index(&steps, |s| {
        s["event"]["descriptor"]["emit"] == "like-toggled"
    });
    let outcome = step_index(&steps, |s| s["event"]["kind"] == "outcome");
    assert!(press < outcome, "the optimistic step precedes settlement");
    let writes = steps[press]["dispatch"]["writes"]
        .as_array()
        .expect("writes");
    assert!(
        writes.iter().any(|w| w["field"] == "like-overlay"),
        "the press writes the optimistic overlay"
    );
}

#[test]
fn like_refused_rolls_back_to_a_byte_identical_feed_subtree() {
    let program = checked_program();
    let steps = trace(&program, "like-refused", true);

    // Pre-like: the step where the feed page settled (last before the
    // press). Post: after the notice dismisses.
    let press = step_index(&steps, |s| {
        s["event"]["descriptor"]["emit"] == "like-toggled"
    });
    let pre_root = &steps[press - 1]["v"]["page"]["root"];
    let dismissed = step_index(&steps, |s| {
        s["event"]["descriptor"]["emit"] == "notice-dismissed"
    });
    let post_root = &steps[dismissed]["v"]["page"]["root"];
    assert!(
        find_text(
            &steps[dismissed - 1]["v"]["page"]["root"],
            "Couldn't like this post"
        ),
        "the notice explains before dismissal"
    );
    assert_eq!(
        uhura_base::to_canonical_json(pre_root),
        uhura_base::to_canonical_json(post_root),
        "after dismissing the notice, the feed subtree is byte-identical \
         to pre-like (§11.4 step 3)"
    );
}

#[test]
fn comment_ok_swaps_the_optimistic_row_atomically_and_restores_focus() {
    let program = checked_program();
    let steps = trace(&program, "comment-ok", true);

    // After submit: a dimmed pending row (the fixture author renders
    // `Posting…`), the composer cleared.
    let submit = step_index(&steps, |s| {
        s["event"]["descriptor"]["emit"] == "submit-requested"
    });
    let sheet = &steps[submit]["v"]["surfaces"][0]["root"];
    assert!(find_text(sheet, "Posting…"), "optimistic row present");

    // After the ok settles (piggybacked update applied first): exactly
    // five comments, no pending row, the echoed body authoritative.
    let outcome = step_index(&steps, |s| s["event"]["kind"] == "outcome");
    assert!(outcome > submit);
    let sheet = &steps[outcome]["v"]["surfaces"][0]["root"];
    assert!(!find_text(sheet, "Posting…"), "pending row settled away");
    assert!(
        find_text(sheet, "Saving this palette for my kitchen reno"),
        "the authoritative comment echoes the typed body"
    );
    let mut nodes = Vec::new();
    walk_nodes(sheet, &mut nodes);
    let comment_rows = nodes
        .iter()
        .filter(|n| {
            n["class"]
                .as_str()
                .is_some_and(|c| c.contains("comment-row"))
        })
        .count();
    assert_eq!(comment_rows, 5, "four authored comments plus Mira's");

    // Dismissal restores focus to the opener's triggering node.
    let dismissal = step_index(&steps, |s| {
        s["event"]["descriptor"]["emit"] == "dismiss-requested"
    });
    let intents = steps[dismissal]["i"].as_array().expect("intents");
    let focus = intents
        .iter()
        .find(|i| i["intent"] == "focus-restore")
        .expect("FocusRestore intent on topmost dismissal");
    assert!(
        focus["key-path"]
            .as_str()
            .is_some_and(|p| p.starts_with("page:1/")),
        "the key-path points into the page: {focus}"
    );
}

#[test]
fn paginate_dedupes_via_the_guard_and_appends_preserving_keys() {
    let program = checked_program();
    let steps = trace(&program, "paginate", true);
    assert_eq!(
        commands(&steps, "load-next-page").len(),
        1,
        "wiggle-scroll re-observation must not send a second command"
    );
    // The second near-end drops with the guard traced unsatisfied.
    let drops: Vec<_> = steps.iter().filter(|s| s["drop"] == "no-handler").collect();
    assert_eq!(drops.len(), 1, "the wiggle drops, traced");

    // Keys before the append are a strict prefix of keys after.
    let post_keys = |root: &serde_json::Value| -> Vec<String> {
        let mut nodes = Vec::new();
        walk_nodes(root, &mut nodes);
        nodes
            .iter()
            .filter(|n| n["class"].as_str().is_some_and(|c| c.contains("post-card")))
            .filter_map(|n| n["key"].as_str().map(ToString::to_string))
            .collect()
    };
    let ready = step_index(&steps, |s| s["event"]["kind"] == "projection");
    let before = post_keys(&steps[ready]["v"]["page"]["root"]);
    let after = post_keys(&steps[steps.len() - 1]["v"]["page"]["root"]);
    assert_eq!(before.len(), 4);
    assert_eq!(after.len(), 6);
    assert_eq!(
        &after[..4],
        &before[..],
        "the append preserves existing keys as a prefix (identity, §8.1)"
    );
}

#[test]
fn feed_failed_renders_the_failed_arm_then_recovers() {
    let program = checked_program();
    let steps = trace(&program, "feed-failed", true);
    let failed = step_index(&steps, |s| s["event"]["kind"] == "projection-failed");
    assert!(
        find_text(
            &steps[failed]["v"]["page"]["root"],
            "Your feed didn't load."
        ),
        "the failed arm renders"
    );
    assert_eq!(commands(&steps, "reload").len(), 1);
    let last = &steps[steps.len() - 1]["v"]["page"]["root"];
    assert!(
        !find_text(last, "Your feed didn't load."),
        "the retry recovers to the ready arm"
    );
}

#[test]
fn feed_empty_renders_and_projection_truth_blocks_pagination() {
    let program = checked_program();
    let steps = trace(&program, "feed-empty", true);
    let last = &steps[steps.len() - 1]["v"]["page"]["root"];
    assert!(find_text(
        last,
        "Posts from people you follow will appear here."
    ));
    let mut nodes = Vec::new();
    walk_nodes(last, &mut nodes);
    let near_end = nodes.iter().any(|n| {
        n["on"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|d| d["emit"] == "feed-near-end")
    });
    assert!(
        near_end,
        "the authored scroll observation remains in V (§8.1)"
    );
    assert!(commands(&steps, "load-next-page").is_empty());
    assert_eq!(steps.last().expect("steps")["drop"], "no-handler");
}

#[test]
fn the_demo_walkthrough_runs_headlessly_without_drops() {
    let program = checked_program();
    let steps = trace(&program, "demo", false);
    assert!(steps.len() >= 15, "the full walkthrough steps");
    assert!(
        steps.iter().all(|s| s.get("drop").is_none()),
        "the demo drops nothing"
    );
    // It ends back on the feed with history intents traced.
    let intents: Vec<_> = steps
        .iter()
        .flat_map(|s| s["i"].as_array().into_iter().flatten())
        .map(|i| i["intent"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(
        intents,
        vec!["focus-restore", "history-push", "history-back"],
        "dismiss, navigate, back — in walkthrough order"
    );
}

// ── replay-derived preview goldens (§6.2) ───────────────────────────────────

#[test]
fn every_derived_preview_replays_to_its_v_golden() {
    let out = check(&corpus_input(true, &identity));
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.severity == uhura_base::Severity::Error),
        "corpus must check clean:\n{}",
        render_text(&out.diagnostics, &out.source_map)
    );
    let program = &out.lowered.as_ref().expect("clean").program;

    let mut derived = 0usize;
    for preview in &out.previews {
        if !preview.derived {
            continue;
        }
        derived += 1;
        let name = format!("v-{}-{}.json", preview.subject.name(), preview.example);
        let v_json = match &preview.payload {
            PreviewPayload::Page { u, x, .. } => eval_view(program, u, x)
                .unwrap_or_else(|e| panic!("{name}: {e}"))
                .to_canonical_string(),
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
    }
    assert_eq!(derived, 34, "thirty-four derived examples replay");

    // The optimistic like really is in the derived like-pending state.
    let like_pending = out
        .previews
        .iter()
        .find(|p| p.subject.name().as_str() == "feed" && p.example == "like-pending")
        .expect("feed like-pending");
    assert_eq!(like_pending.in_flight, 1, "one command in flight");

    // comments-open mounted the sheet through the machine.
    let comments_open = out
        .previews
        .iter()
        .find(|p| p.subject.name().as_str() == "feed" && p.example == "comments-open")
        .expect("feed comments-open");
    let PreviewPayload::Page { u, .. } = &comments_open.payload else {
        panic!("page preview");
    };
    assert_eq!(
        u.surfaces.len(),
        1,
        "the sheet mounts because the machine mounted it"
    );
    assert!(
        u.surfaces[0].restore_focus.is_some(),
        "the opener's trigger node is recorded"
    );
}

// ── self-verifying design (§6.2) ────────────────────────────────────────────

#[test]
fn a_guard_change_in_the_feed_page_fails_a_derived_example() {
    let out = check(&corpus_input(true, &|rel, text| {
        if rel == "app/feed/page.uhura" {
            // Invert the like guard: the recorded `like-toggled` event no
            // longer finds a satisfied handler.
            text.replace(
                "on like-toggled(post: id, now-liked: bool) when now-liked && !(like-pending[post] ?? false) {",
                "on like-toggled(post: id, now-liked: bool) when !now-liked && !(like-pending[post] ?? false) {",
            )
        } else {
            text
        }
    }));
    let errors: Vec<String> = out
        .diagnostics
        .iter()
        .filter(|d| d.severity == uhura_base::Severity::Error)
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert!(
        errors
            .iter()
            .any(|e| e.contains("UH7012") && e.contains("like-pending")),
        "the derived example fails the check: {errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("UH7013") && e.contains("like-refused")),
        "its descendant reports blocked-by-ancestor: {errors:?}"
    );
}
