//! The M2 gate (plan §M2): `check` clean over the corpus + deterministic
//! IR golden; every §4.8 rejection as a diagnostics golden; the
//! examples-invariance property (IR bytes identical with and without
//! `*.examples.uhura`); lock drift as a link error.
//!
//! Bless goldens with `UPDATE_GOLDEN=1 cargo test -p uhura-tests`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use uhura_base::{Severity, render_text};
use uhura_check::manifest::load_manifest;
use uhura_check::{CheckInput, CheckOutput, LockStatus, SourceInput, check};
use uhura_syntax::SourceKind;

include!("common/corpus.rs");

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens/m2")
}

fn check_corpus(mutate: &dyn Fn(&str, String) -> String) -> CheckOutput {
    check(&corpus_input(true, mutate))
}

fn identity(_: &str, text: String) -> String {
    text
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

// ── gate 1: clean corpus + deterministic IR golden ─────────────────────────

#[test]
fn corpus_checks_clean_and_ir_matches_golden() {
    let out = check_corpus(&identity);
    let rendered = render_text(&out.diagnostics, &out.source_map);
    assert!(
        out.diagnostics.is_empty(),
        "corpus must check clean:\n{rendered}"
    );
    let lowered = out.lowered.expect("clean check lowers");
    assert_golden("instagram-ir.json", &lowered.program.to_canonical_string());

    // Determinism: a second full run produces the same bytes.
    let again = check_corpus(&identity);
    assert_eq!(
        again.lowered.expect("clean").program.to_canonical_string(),
        lowered.program.to_canonical_string()
    );
}

// ── gate 2: examples-invariance (§6.1) ─────────────────────────────────────

#[test]
fn ir_bytes_identical_with_and_without_examples_files() {
    let with = check(&corpus_input(true, &identity));
    let without = check(&corpus_input(false, &identity));
    let with_ir = with.lowered.expect("clean").program.to_canonical_string();
    let without_ir = without
        .lowered
        .expect("clean")
        .program
        .to_canonical_string();
    assert_eq!(
        with_ir, without_ir,
        "examples files are design artifacts — they may never reach the runtime bundle"
    );
}

// ── gate 3: lock drift is a link error ─────────────────────────────────────

#[test]
fn lock_drift_is_diagnosed() {
    let mut input = corpus_input(true, &identity);
    input.lock_text = Some(
        input
            .lock_text
            .expect("corpus lock exists")
            .replace("sha256:", "sha256:0"),
    );
    let out = check(&input);
    assert_eq!(out.lock_status, LockStatus::Drift);
    assert!(
        out.diagnostics.iter().any(|d| d.code == "UH2007"),
        "drift must be an error, never silent (§9.1)"
    );
}

// ── gate 4: every §4.8 rejection as a diagnostics golden ───────────────────

/// One rejection case: a corpus mutation, the code it must raise, and the
/// golden holding the full rendered diagnostics.
struct Rejection {
    name: &'static str,
    file: &'static str,
    from: &'static str,
    to: &'static str,
    expect_code: &'static str,
    expect_severity: Severity,
}

const REJECTIONS: &[Rejection] = &[
    Rejection {
        name: "unknown-element",
        file: "components/post-card.uhura",
        from: "<image class=\"avatar\" src={post.author.avatar.src} alt={post.author.avatar.alt} />",
        to: "<avatar class=\"avatar\" src={post.author.avatar.src} alt={post.author.avatar.alt} />",
        expect_code: "UH5001",
        expect_severity: Severity::Error,
    },
    Rejection {
        name: "unkeyed-each",
        file: "app/feed/page.uhura",
        from: "{#each f.posts as p (p.id)}",
        to: "{#each f.posts as p}",
        expect_code: "UH0003",
        expect_severity: Severity::Error,
    },
    Rejection {
        name: "event-on-layout",
        file: "app/feed/page.uhura",
        from: "<view role=\"list\" class=\"post-list\">",
        to: "<view role=\"list\" class=\"post-list\" on:press={emit notice-dismissed()}>",
        expect_code: "UH5002",
        expect_severity: Severity::Error,
    },
    Rejection {
        name: "undeclared-component-emit",
        file: "app/feed/page.uhura",
        from: "on:dismissed={emit notice-dismissed()}",
        to: "on:closed={emit notice-dismissed()}",
        expect_code: "UH5003",
        expect_severity: Severity::Error,
    },
    Rejection {
        name: "unbound-required-prop",
        file: "app/feed/page.uhura",
        from: "<notice-bar text={notice ?? \"\"} on:dismissed={emit notice-dismissed()} />",
        to: "<notice-bar on:dismissed={emit notice-dismissed()} />",
        expect_code: "UH5004",
        expect_severity: Severity::Error,
    },
    Rejection {
        name: "unresolved-name-did-you-mean",
        file: "app/feed/page.uhura",
        from: "on retry-reload-tapped() when !reload-pending {",
        to: "on retry-reload-tapped() when !reload-pendin {",
        expect_code: "UH3003",
        expect_severity: Severity::Error,
    },
    Rejection {
        name: "unreachable-handler",
        file: "app/feed/page.uhura",
        from: "on like-toggled(post: id, now-liked: bool) when now-liked && !(like-pending[post] ?? false) {",
        to: "on like-toggled(post: id, now-liked: bool) {",
        expect_code: "UH4005",
        expect_severity: Severity::Error,
    },
    Rejection {
        name: "unguarded-projection-read",
        file: "app/feed/page.uhura",
        from: "{#match feed-page}",
        to: "{#if feed-page.has-more}\n    <text>more soon</text>\n  {/if}\n  {#match feed-page}",
        expect_code: "UH3009",
        expect_severity: Severity::Error,
    },
    Rejection {
        name: "class-rooting-violation",
        file: "components/post-card.uhura",
        from: ".post-card { display: flex; flex-direction: column; gap: var(--space-2); background: var(--color-surface); }",
        to: "article { display: flex; flex-direction: column; gap: var(--space-2); background: var(--color-surface); }",
        expect_code: "UH6001",
        expect_severity: Severity::Error,
    },
    Rejection {
        name: "undefined-class-warning",
        file: "app/feed/page.uhura",
        from: "<scroll class=\"feed-scroll\" on:near-end={emit feed-near-end()}>",
        to: "<scroll class=\"feed-scrolll\" on:near-end={emit feed-near-end()}>",
        expect_code: "UH6002",
        expect_severity: Severity::Warning,
    },
    Rejection {
        name: "missing-availability-arm",
        file: "app/profile/[user]/page.uhura",
        from: "{:when loading}\n      <view class=\"top-bar\">\n        {#if user != viewer.id}\n          <button label=\"Back\" on:press={emit back-tapped()}>\n            <icon name=\"back\" />\n          </button>\n        {/if}\n        <text class=\"title\">Profile</text>\n      </view>\n      <view class=\"fill-center\">\n        <text class=\"muted\">Loading profile…</text>\n      </view>\n    {:when failed reason}",
        to: "{:when failed reason}",
        expect_code: "UH5015",
        expect_severity: Severity::Error,
    },
    Rejection {
        name: "controlled-promotion",
        file: "surfaces/comments-sheet.uhura",
        from: "on:change={emit composer-changed()} ",
        to: "",
        expect_code: "UH5008",
        expect_severity: Severity::Error,
    },
    Rejection {
        name: "nested-interactive",
        file: "components/post-card.uhura",
        from: "<button class=\"icon-action\" label=\"Comments\" on:press={emit comments-requested(post: post.id)}>\n        <icon name=\"comment\" />\n      </button>",
        to: "<button class=\"icon-action\" label=\"Comments\" on:press={emit comments-requested(post: post.id)}>\n        <button label=\"Inner\" on:press={emit comments-requested(post: post.id)}>\n          <icon name=\"comment\" />\n        </button>\n      </button>",
        expect_code: "UH5007",
        expect_severity: Severity::Error,
    },
    Rejection {
        name: "alt-xor-decorative",
        file: "components/post-card.uhura",
        from: "<image class=\"media\" src={m.image.src} alt={m.image.alt} />",
        to: "<image class=\"media\" src={m.image.src} alt={m.image.alt} decorative />",
        expect_code: "UH5009",
        expect_severity: Severity::Error,
    },
    Rejection {
        name: "unknown-icon",
        file: "components/bottom-nav.uhura",
        from: "<icon name=\"reels\" />",
        to: "<icon name=\"reel\" />",
        expect_code: "UH5017",
        expect_severity: Severity::Error,
    },
    Rejection {
        name: "non-exhaustive-union-match",
        file: "components/post-card.uhura",
        from: "{:when video v}",
        to: "{:when carousel v-two}",
        expect_code: "UH5016",
        expect_severity: Severity::Error,
    },
];

#[test]
fn every_rejection_raises_its_code_and_matches_golden() {
    for case in REJECTIONS {
        let out = check_corpus(&|rel, text| {
            if rel == case.file {
                assert!(
                    text.contains(case.from),
                    "{}: mutation anchor not found in {}",
                    case.name,
                    case.file
                );
                text.replace(case.from, case.to)
            } else {
                text
            }
        });
        let rendered = render_text(&out.diagnostics, &out.source_map);
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.code == case.expect_code && d.severity == case.expect_severity),
            "{}: expected {} ({:?}), got:\n{rendered}",
            case.name,
            case.expect_code,
            case.expect_severity,
        );
        if case.expect_severity == Severity::Error {
            assert!(
                out.lowered.is_none(),
                "{}: lowering must be zero-error gated",
                case.name
            );
        }
        assert_golden(&format!("reject-{}.txt", case.name), &rendered);
    }
}
