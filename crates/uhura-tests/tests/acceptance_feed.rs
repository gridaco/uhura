//! §13 acceptance — ONE executable integration test (design §12.5, M6):
//! the eight criteria over the canonical scripts. M4's gates pin most
//! invariants piecemeal (m4_gates.rs); this battery re-states them AS
//! §13 — acceptance is one `cargo test` away from the design text — and
//! adds what only lands here:
//!
//!   exact FocusRestore key-path equality (not just a prefix); flicker
//!   freedom across the whole like-settlement window; pagination
//!   failure → retry; a post-exhaustion near-end refused by the guard
//!   (the observation descriptor is markup-authored and STAYS in V —
//!   projection truth rejects the fetch, register #67); feed page state
//!   surviving the profile round trip byte-identically; the real
//!   `uhura project` run twice to byte-equal canvases; uncorrelated
//!   outcome injection as a check error; IR-bytes invariance with and
//!   without `*.examples.uhura`; and native ↔ wasm parity through
//!   `scripts/parity.mjs` against the real wasm32 binary.
//!
//! Parity needs a working `node` and both files of
//! `crates/uhura-wasm/pkg/node` (built by `scripts/build-wasm.sh`). When
//! either is missing that criterion is skipped — and libtest swallows the
//! skip notice unless `--nocapture`, so a green default run does NOT
//! prove parity ran. CI must set `UHURA_REQUIRE_PARITY=1` to turn the
//! skip into a failure (`0` and empty count as unset).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use uhura_base::render_text;
use uhura_check::fixture::load_fixture;
use uhura_check::manifest::load_manifest;
use uhura_check::{CheckInput, CheckOutput, SourceInput, check};
use uhura_cli::cmd::dev::boot_envelope;
use uhura_cli::cmd::trace::{fixture_slices_json, run_script};
use uhura_core::ir::ProgramIr;
use uhura_syntax::SourceKind;

include!("common/corpus.rs");

#[test]
fn acceptance_walkthrough() {
    // §13.1 — the corpus checks clean. Of §4.8's documented rejections,
    // ten are pinned as source-level goldens in m2_gates (REJECTIONS);
    // the value-dependent eleventh — duplicate keys — and the §13.6
    // clause that had no pin anywhere — uncorrelated outcome injection —
    // are asserted below.
    let out = check(&corpus_input(true, &identity));
    assert_clean(&out);
    let program = &out.lowered.as_ref().expect("clean check lowers").program;

    criterion_1_duplicate_keys();
    criterion_2_like(program);
    criterion_3_comments(program);
    criterion_4_pagination(program);
    criterion_5_navigation(program);
    criterion_6_projection(&out);
    criterion_7_parity(program);
    criterion_8_ir_invariance(&out);
}

// ── §13.1 — duplicate keys: the value-dependent documented rejection ────────

fn criterion_1_duplicate_keys() {
    // §4.8 lists "duplicate keys" among the checker's rejections, but the
    // collision lives in DATA — only evaluation sees it, so the check
    // catches it through derived-example replay (§6.2's self-verifying
    // design): collide two post ids in the fixture and the feed's derived
    // examples fail with the eval rejection spanned to the example.
    let mutated = check(&corpus_input(true, &|rel, text| {
        if rel == "fixtures/standard.toml" {
            text.replace("id = \"post-marco-baja\"", "id = \"post-lena-glaze\"")
        } else {
            text
        }
    }));
    assert!(
        mutated.diagnostics.iter().any(|d| {
            d.severity == uhura_base::Severity::Error
                && d.code == "UH7012"
                && d.message.contains("duplicate key `post-lena-glaze`")
        }),
        "§13.1/§4.8: colliding each-keys are a check error: {:?}",
        mutated
            .diagnostics
            .iter()
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect::<Vec<_>>()
    );
}

// ── §13.2 — like: one command, optimism first, rollback, no flicker ─────────

fn criterion_2_like(program: &ProgramIr) {
    let steps = trace(program, "like-ok", true);
    assert_eq!(
        commands(&steps, "like-post").len(),
        1,
        "§13.2: exactly one typed command per press"
    );
    let press = step_index(&steps, "§13.2 like-ok press", |s| {
        s["event"]["descriptor"]["emit"] == "like-toggled"
    });
    let outcome = step_index(&steps, "§13.2 like-ok outcome", |s| {
        s["event"]["kind"] == "outcome"
    });
    assert!(
        press < outcome,
        "§13.2: the optimistic step precedes settlement"
    );

    // Pre-press: the authored resting state, pinned on the LIKE button
    // itself (found by its heart icon, not by position).
    let card = lena_card(&steps[press - 1]["v"]["page"]["root"], "§13.2 pre-press");
    assert!(
        find_text(card, "7 likes"),
        "pre-like shows the authored count"
    );
    let like = like_button(card);
    assert_eq!(
        icon_names(like),
        vec!["heart"],
        "pre-like heart is unfilled"
    );
    assert_eq!(
        like["props"]["pressed"].as_bool(),
        Some(false),
        "pre-like the like button is unpressed"
    );

    // The press itself: heart AND computed count flip before any provider
    // word arrives (§11.4 step 2).
    let card = lena_card(&steps[press]["v"]["page"]["root"], "§13.2 press");
    assert!(
        find_text(card, "8 likes"),
        "the optimistic count computes in-card"
    );
    let like = like_button(card);
    assert_eq!(
        icon_names(like),
        vec!["heart-filled"],
        "the optimistic heart fills"
    );
    assert_eq!(
        like["props"]["pressed"].as_bool(),
        Some(true),
        "the like button — that button — reports pressed"
    );

    // Settlement never flickers (§13.2): from the press to the end of the
    // script — through the piggybacked update AND the `.ok` dispatch — the
    // whole optimistic view (heart AND count AND pressed, the criterion
    // names all of it) never once regresses.
    for (i, step) in steps.iter().enumerate().skip(press) {
        let card = lena_card(
            &step["v"]["page"]["root"],
            &format!("§13.2 flicker window, step {i}"),
        );
        assert!(
            find_text(card, "8 likes") && !find_text(card, "7 likes"),
            "§13.2: step {i} flickered the count"
        );
        let like = like_button(card);
        assert_eq!(
            icon_names(like),
            vec!["heart-filled"],
            "§13.2: step {i} flickered the heart"
        );
        assert_eq!(
            like["props"]["pressed"].as_bool(),
            Some(true),
            "§13.2: step {i} flickered the pressed state"
        );
    }

    // Refusal: rollback to byte-equality with the pre-like render,
    // asserted after notice dismissal (§11.4 step 3).
    let steps = trace(program, "like-refused", true);
    let press = step_index(&steps, "§13.2 like-refused press", |s| {
        s["event"]["descriptor"]["emit"] == "like-toggled"
    });
    let dismissed = step_index(&steps, "§13.2 like-refused dismissal", |s| {
        s["event"]["descriptor"]["emit"] == "notice-dismissed"
    });
    assert!(
        find_text(
            &steps[dismissed - 1]["v"]["page"]["root"],
            "Couldn't like this post"
        ),
        "the notice explains before dismissal"
    );
    assert_eq!(
        uhura_base::to_canonical_json(&steps[press - 1]["v"]["page"]["root"]),
        uhura_base::to_canonical_json(&steps[dismissed]["v"]["page"]["root"]),
        "§13.2: post-dismissal, the feed subtree is byte-identical to pre-like"
    );
}

// ── §13.3 — comments: one keyed surface, optimism ≥ 1 tick, focus back ──────

fn criterion_3_comments(program: &ProgramIr) {
    let steps = trace(program, "comment-ok", true);

    let open = step_index(&steps, "§13.3 open", |s| {
        s["event"]["descriptor"]["emit"] == "comments-requested"
    });
    let surfaces = steps[open]["v"]["surfaces"].as_array().expect("surfaces");
    assert_eq!(surfaces.len(), 1, "§13.3: open mounts ONE surface instance");
    assert!(
        surfaces[0]["key"]
            .as_str()
            .is_some_and(|k| k.starts_with("comments-sheet:")),
        "the instance is keyed"
    );
    let mut nodes = Vec::new();
    walk_nodes(&surfaces[0]["root"], &mut nodes);
    assert!(
        nodes.iter().any(|n| n["element"] == "text-field"),
        "§13.3: the sheet carries the typed input"
    );

    // The optimistic append is visible for at least one full step before
    // the authority settles it away.
    let submit = step_index(&steps, "§13.3 submit", |s| {
        s["event"]["descriptor"]["emit"] == "submit-requested"
    });
    let outcome = step_index(&steps, "§13.3 outcome", |s| {
        s["event"]["kind"] == "outcome"
    });
    assert!(submit < outcome, "the pending row exists before settlement");
    assert!(
        find_text(&steps[submit]["v"]["surfaces"][0]["root"], "Posting…"),
        "§13.3: the optimistic row renders on submit"
    );
    let settled = &steps[outcome]["v"]["surfaces"][0]["root"];
    assert!(
        !find_text(settled, "Posting…"),
        "settlement replaces it atomically"
    );
    assert!(
        find_text(settled, "Saving this palette for my kitchen reno"),
        "the authoritative comment echoes the typed body"
    );

    // Dismissal restores focus — the intent's key-path EQUALS the path of
    // the Comments button that opened the sheet, recomputed independently
    // from the view the open event was emitted against (micro-decision
    // #42: the pre-commit view).
    let dismissal = step_index(&steps, "§13.3 dismissal", |s| {
        s["event"]["descriptor"]["emit"] == "dismiss-requested"
    });
    let focus = steps[dismissal]["i"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|i| i["intent"] == "focus-restore")
        .expect("§13.3: topmost dismissal emits FocusRestore");
    let expected =
        key_path_of(&steps[open - 1]["v"]["page"]["root"], "page:1", &|n| {
            n["on"].as_array().into_iter().flatten().any(|d| {
                d["emit"] == "comments-requested" && d["payload"]["post"] == "post-lena-glaze"
            })
        })
        .expect("the trigger node exists in the pre-open view");
    assert_eq!(
        focus["key-path"].as_str().expect("key-path"),
        expected,
        "§13.3: FocusRestore points exactly at the trigger node"
    );
}

// ── §13.4 — pagination: dedupe, failure → retry, exhaustion is truth ────────

/// Test-local script (the canonical list stays closed): the first
/// `load-next-page` is unavailable, the author-visible retry sends the
/// second, page 2 appends. File order is the fixture's reply order.
const PAGINATE_RETRY: &str = r#"
[[deliver]]
after-ticks = 1
port = "feed"
projection = "feed-page"
slice = "feed.page-1"

[[reply]]
on = { command = "load-next-page", where = { cursor = "cursor-page-2" } }
after-ticks = 1
outcome = "unavailable"
reason = "network unavailable"

[[reply]]
on = { command = "load-next-page", where = { cursor = "cursor-page-2" } }
after-ticks = 2
outcome = "ok"

[[reply.updates]]
port = "feed"
projection = "feed-page"
slice = "feed.pages-1-2"

[[ui]]
at-tick = 2
emit = "feed-near-end"

[[ui]]
at-tick = 4
emit = "retry-load-tapped"
"#;

/// Test-local script: boot straight into the EXHAUSTED feed
/// (`feed.final`: 6 followed-author posts, `has-more = false`, no cursor) and press
/// near-end once. No timeline coupling to any canonical script.
const EXHAUSTED: &str = r#"
[[deliver]]
after-ticks = 1
port = "feed"
projection = "feed-page"
slice = "feed.final"

[[ui]]
at-tick = 2
emit = "feed-near-end"
"#;

fn criterion_4_pagination(program: &ProgramIr) {
    // One near-end episode = one command; the wiggle re-observation drops
    // with the guard traced (the guard IS the dedupe).
    let steps = trace(program, "paginate", true);
    assert_eq!(
        commands(&steps, "load-next-page").len(),
        1,
        "§13.4: one per episode"
    );
    let drops: Vec<_> = steps.iter().filter(|s| s["drop"] == "no-handler").collect();
    assert_eq!(
        drops.len(),
        1,
        "§13.4: the duplicate is guard-rejected and traced"
    );

    // Append preserves existing keys in order.
    let ready = step_index(&steps, "§13.4 paginate page-1", |s| {
        s["event"]["kind"] == "projection"
    });
    let before = post_keys(&steps[ready]["v"]["page"]["root"]);
    let after = post_keys(&steps[steps.len() - 1]["v"]["page"]["root"]);
    assert_eq!((before.len(), after.len()), (4, 6));
    assert_eq!(
        &after[..4],
        &before[..],
        "§13.4: existing keys are a strict prefix"
    );

    // Failure → retry: refusal renders the retry affordance; the retry is
    // a NEW episode (second command), and the append still lands. The
    // prefix is asserted against THIS trace's own page-1 keys.
    let steps = trace_text(program, "PAGINATE_RETRY", PAGINATE_RETRY, true);
    assert_eq!(
        commands(&steps, "load-next-page").len(),
        2,
        "§13.4: the failure and the retry are one command each"
    );
    let ready = step_index(&steps, "§13.4 retry page-1", |s| {
        s["event"]["kind"] == "projection"
    });
    let retry_before = post_keys(&steps[ready]["v"]["page"]["root"]);
    assert_eq!(retry_before.len(), 4);
    let refused = step_index(&steps, "§13.4 retry refusal", |s| {
        s["event"]["kind"] == "outcome"
    });
    assert!(
        find_text(&steps[refused]["v"]["page"]["root"], "Couldn't load more."),
        "the refusal renders the retry affordance"
    );
    let last = &steps[steps.len() - 1]["v"]["page"]["root"];
    assert!(
        !find_text(last, "Couldn't load more."),
        "the retry recovers"
    );
    assert_eq!(post_keys(last).len(), 6, "page 2 appends after the retry");
    assert_eq!(
        &post_keys(last)[..4],
        &retry_before[..],
        "keys still a prefix"
    );

    // Exhausted derives from projection truth: with `has-more = false`
    // the end cap renders and a near-end press fires into an unsatisfied
    // guard — zero commands. The observation descriptor is markup-authored
    // and deliberately still in V (register #67); it is the MACHINE that
    // refuses.
    let steps = trace_text(program, "EXHAUSTED", EXHAUSTED, true);
    assert!(
        commands(&steps, "load-next-page").is_empty(),
        "§13.4: exhaustion refuses to fetch"
    );
    let last = steps.last().expect("steps");
    assert_eq!(
        last["event"]["descriptor"]["emit"], "feed-near-end",
        "the post-exhaustion press is the final step"
    );
    assert_eq!(
        last["drop"], "no-handler",
        "…and drops with the guard traced"
    );
    let root = &last["v"]["page"]["root"];
    assert!(
        find_text(root, "You're all caught up."),
        "the end cap renders from `!has-more`"
    );
    assert!(
        has_near_end_descriptor(root),
        "register #67: the markup-authored observation descriptor stays in V"
    );

    // The empty feed renders its empty state. Its scroll still owns the
    // authored observation descriptor, but projection truth guard-drops the
    // observation without emitting a pagination command.
    let steps = trace(program, "feed-empty", true);
    let root = &steps[steps.len() - 1]["v"]["page"]["root"];
    assert!(
        find_text(root, "Posts from people you follow will appear here."),
        "§13.4: the empty state actually renders"
    );
    assert!(
        has_near_end_descriptor(root),
        "§13.4/§8.1: the markup-authored scroll observation stays in V"
    );
    assert!(
        commands(&steps, "load-next-page").is_empty(),
        "§13.4: projection truth prevents empty-feed pagination"
    );
    assert_eq!(
        last["drop"], "no-handler",
        "the empty-feed observation is rejected by the guard"
    );
}

// ── §13.5 — navigation: the round trip returns the same feed ────────────────

fn criterion_5_navigation(program: &ProgramIr) {
    let steps = trace(program, "demo", true);
    let nav = step_index(&steps, "§13.5 navigate", |s| {
        s["event"]["descriptor"]["emit"] == "author-tapped"
    });
    let back = step_index(&steps, "§13.5 back", |s| {
        s["event"]["descriptor"]["emit"] == "back-tapped"
    });
    assert!(nav < back);
    assert_eq!(
        uhura_base::to_canonical_json(&steps[nav - 1]["v"]["page"]["root"]),
        uhura_base::to_canonical_json(&steps[back]["v"]["page"]["root"]),
        "§13.5: feed → profile → back retains the feed page byte-identically"
    );

    // History intents are emitted and traced, in walkthrough order:
    // dismiss, navigate, back. (§13.5's "executed as no-ops" half is the
    // SHELL's contract — §7.4, shell/main.js — outside this headless
    // battery; no automated test pins it.)
    let intents: Vec<String> = steps
        .iter()
        .flat_map(|s| s["i"].as_array().into_iter().flatten())
        .map(|i| i["intent"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(
        intents,
        vec!["focus-restore", "history-push", "history-back"],
        "§13.5: the traced intent sequence"
    );
}

// ── §13.6 — projection: eval-only, deterministic, injection rejected ────────

fn criterion_6_projection(out: &CheckOutput) {
    // `uhura project` executes zero transitions and zero I/O at projection
    // time: its render path is eval-only by construction — no driver is
    // even constructible from its inputs. The executable witnesses are
    // determinism and coverage: the REAL command, twice, to the byte.
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("acceptance-project");
    let mut canvases = Vec::new();
    for run in ["a", "b"] {
        let dir = tmp.join(run);
        let args = uhura_cli::CommonArgs {
            root: corpus_root(),
            format_json: false,
            deny_warnings: false,
            emit_ir: false,
        };
        let code = uhura_cli::cmd::project::run(&args, dir.to_str());
        assert_eq!(
            format!("{code:?}"),
            format!("{:?}", std::process::ExitCode::SUCCESS),
            "§13.6: `uhura project` succeeds"
        );
        canvases.push(std::fs::read(dir.join("canvas.html")).expect("canvas.html"));
    }
    assert_eq!(
        canvases[0], canvases[1],
        "§13.6: projection is byte-deterministic"
    );

    // Every resolved preview — pinned and replay-derived — is on the
    // board, matched by the exact caption markup render_frame emits (a
    // bare-name contains() is satisfiable by CSS rules, provenance badges,
    // and same-named examples of OTHER subjects).
    let canvas = String::from_utf8(canvases.pop().expect("one canvas")).expect("utf8");
    assert!(!out.previews.is_empty());
    for preview in &out.previews {
        let caption = format!(
            "<span class=\"caption-title\">{} / {}</span>",
            preview.subject.name(),
            preview.example
        );
        assert!(
            canvas.contains(&caption),
            "§13.6: preview `{}/{}` is framed (missing caption `{caption}`)",
            preview.subject.name(),
            preview.example
        );
    }

    // Uncorrelated outcome injection is a check error (§13.6; the
    // reachability half — a guard edit fails the derived example — is
    // pinned in m4_gates).
    let mutated = check(&corpus_input(true, &|rel, text| {
        if rel == "app/feed/page.examples.uhura" {
            format!(
                "{text}\nexample uncorrelated {{\n  from first-page\n  \
                 events [ outcome like-post.ok() ]\n}}\n"
            )
        } else {
            text
        }
    }));
    assert!(
        mutated.diagnostics.iter().any(|d| {
            d.severity == uhura_base::Severity::Error
                && d.code == "UH7012"
                && d.message.contains("no unsettled")
        }),
        "§13.6: an outcome with no unsettled command is a check error: {:?}",
        mutated
            .diagnostics
            .iter()
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect::<Vec<_>>()
    );
}

// ── §13.7 — native ↔ wasm parity (scripts/parity.mjs) ───────────────────────

const PARITY_SCRIPTS: [&str; 7] = [
    "like-ok",
    "like-refused",
    "comment-ok",
    "paginate",
    "feed-failed",
    "feed-empty",
    "demo",
];

fn criterion_7_parity(program: &ProgramIr) {
    // Gate on BOTH package files (a half-built pkg would fail seven
    // scripts confusingly) and on node actually RUNNING (a broken shim
    // spawns fine and exits nonzero — spawn success alone proves nothing).
    let pkg = Path::new(env!("CARGO_MANIFEST_DIR")).join("../uhura-wasm/pkg/node");
    let pkg_ready = pkg.join("uhura_wasm.js").exists() && pkg.join("uhura_wasm_bg.wasm").exists();
    let node_ok = std::process::Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let required = !matches!(
        std::env::var("UHURA_REQUIRE_PARITY").ok().as_deref(),
        None | Some("") | Some("0")
    );
    if !pkg_ready || !node_ok {
        let missing = if node_ok {
            "crates/uhura-wasm/pkg/node (uhura_wasm.js + uhura_wasm_bg.wasm)"
        } else {
            "a working `node`"
        };
        assert!(
            !required,
            "§13.7: UHURA_REQUIRE_PARITY is set but {missing} is missing \
             (build with scripts/build-wasm.sh)"
        );
        eprintln!(
            "§13.7 parity SKIPPED: {missing} is missing — build with \
             scripts/build-wasm.sh; set UHURA_REQUIRE_PARITY=1 to hard-fail \
             (this notice is only visible under --nocapture)"
        );
        return;
    }

    let parity_mjs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/parity.mjs");
    let fixture_text = read_corpus("fixtures/standard.toml");
    let fixture = load_fixture(&fixture_text).expect("fixture loads");
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("acceptance-parity");

    for script in PARITY_SCRIPTS {
        // The prepared artifact directory parity.mjs documents — every
        // file from the same producers `uhura dev` serves from.
        let dir = tmp.join(script);
        std::fs::create_dir_all(&dir).expect("parity dir");
        let script_text = read_corpus(&format!("fixtures/scripts/{script}.toml"));
        let script_json = uhura_fixture::toml_to_json(&script_text).expect("script parses");
        let native = run_script(program, &fixture_text, &script_text, false)
            .unwrap_or_else(|e| panic!("{script}: {e}"));
        let write = |name: &str, bytes: &[u8]| {
            std::fs::write(dir.join(name), bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        };
        write("ir.json", program.to_canonical_string().as_bytes());
        write("fixture.json", fixture_slices_json(&fixture).as_bytes());
        write(
            "script.json",
            uhura_base::to_canonical_json(&script_json).as_bytes(),
        );
        write(
            "boot.json",
            boot_envelope(program, &fixture)
                .expect("boot envelope")
                .as_bytes(),
        );
        write(
            "native.jsonl",
            format!("{}\n", native.join("\n")).as_bytes(),
        );

        let out = std::process::Command::new("node")
            .arg(&parity_mjs)
            .arg(&dir)
            .output()
            .expect("node runs");
        assert!(
            out.status.success(),
            "§13.7: {script} diverged:\n{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("byte-identical"),
            "§13.7: {script}: unexpected parity report: {stdout}"
        );
    }
}

// ── §13.8 — the IR is blind to examples ─────────────────────────────────────

fn criterion_8_ir_invariance(with_examples: &CheckOutput) {
    let without = check(&corpus_input(false, &identity));
    assert_clean(&without);
    assert_eq!(
        with_examples
            .lowered
            .as_ref()
            .expect("clean")
            .program
            .to_canonical_string(),
        without
            .lowered
            .as_ref()
            .expect("clean")
            .program
            .to_canonical_string(),
        "§13.8: IR bytes are identical with and without *.examples.uhura"
    );
}

// ── helpers ─────────────────────────────────────────────────────────────────

fn identity(_: &str, text: String) -> String {
    text
}

fn assert_clean(out: &CheckOutput) {
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.severity == uhura_base::Severity::Error),
        "§13.1: the corpus must check clean:\n{}",
        render_text(&out.diagnostics, &out.source_map)
    );
}

fn read_corpus(rel: &str) -> String {
    std::fs::read_to_string(corpus_root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn trace(program: &ProgramIr, script: &str, expanded: bool) -> Vec<serde_json::Value> {
    trace_text(
        program,
        script,
        &read_corpus(&format!("fixtures/scripts/{script}.toml")),
        expanded,
    )
}

fn trace_text(
    program: &ProgramIr,
    label: &str,
    script_text: &str,
    expanded: bool,
) -> Vec<serde_json::Value> {
    let fixture = read_corpus("fixtures/standard.toml");
    run_script(program, &fixture, script_text, expanded)
        .unwrap_or_else(|e| panic!("{label}: {e}"))
        .iter()
        .map(|line| serde_json::from_str(line).expect("trace lines are JSON"))
        .collect()
}

fn commands<'a>(steps: &'a [serde_json::Value], name: &str) -> Vec<&'a serde_json::Value> {
    steps
        .iter()
        .flat_map(|s| s["c"].as_array().into_iter().flatten())
        .filter(|c| c["command"] == name)
        .collect()
}

fn step_index(
    steps: &[serde_json::Value],
    label: &str,
    pred: impl Fn(&serde_json::Value) -> bool,
) -> usize {
    steps
        .iter()
        .position(pred)
        .unwrap_or_else(|| panic!("{label}: no step matches"))
}

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

fn has_near_end_descriptor(root: &serde_json::Value) -> bool {
    let mut nodes = Vec::new();
    walk_nodes(root, &mut nodes);
    nodes.iter().any(|n| {
        n["on"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|d| d["emit"] == "feed-near-end")
    })
}

/// Lena's post-card subtree (each-keys are `<ordinal>.<key>` — §8.1).
fn lena_card<'a>(root: &'a serde_json::Value, label: &str) -> &'a serde_json::Value {
    let mut nodes = Vec::new();
    walk_nodes(root, &mut nodes);
    nodes
        .into_iter()
        .find(|n| {
            n["key"]
                .as_str()
                .is_some_and(|k| k.ends_with(".post-lena-glaze"))
        })
        .unwrap_or_else(|| panic!("{label}: Lena's card is in the feed"))
}

/// The card's LIKE button, identified by its heart icon (never by
/// position): the pressed/heart asserts must name that button, not any
/// pressed button in the subtree.
fn like_button(card: &serde_json::Value) -> &serde_json::Value {
    let mut nodes = Vec::new();
    walk_nodes(card, &mut nodes);
    nodes
        .into_iter()
        .find(|n| n["element"] == "button" && icon_names(n).iter().any(|i| i.starts_with("heart")))
        .expect("the like button carries the heart icon")
}

fn icon_names(node: &serde_json::Value) -> Vec<String> {
    let mut nodes = Vec::new();
    walk_nodes(node, &mut nodes);
    nodes
        .iter()
        .filter(|n| n["element"] == "icon")
        .filter_map(|n| n["props"]["name"].as_str().map(ToString::to_string))
        .collect()
}

fn post_keys(root: &serde_json::Value) -> Vec<String> {
    let mut nodes = Vec::new();
    walk_nodes(root, &mut nodes);
    nodes
        .iter()
        .filter(|n| n["class"].as_str().is_some_and(|c| c.contains("post-card")))
        .filter_map(|n| n["key"].as_str().map(ToString::to_string))
        .collect()
}

/// Recomputes a node's key-path exactly the way the core does
/// (`find_descriptor`, step.rs): scope, then every node key root → target,
/// first match depth-first.
fn key_path_of(
    node: &serde_json::Value,
    prefix: &str,
    pred: &dyn Fn(&serde_json::Value) -> bool,
) -> Option<String> {
    let path = format!("{prefix}/{}", node["key"].as_str()?);
    if pred(node) {
        return Some(path);
    }
    node["children"]
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|child| key_path_of(child, &path, pred))
}
