//! Formatter contract (design §12.5): formatting is idempotent, and the
//! formatted corpus reparses without diagnostics. Fixpoint stability implies
//! reparse-equality for everything the formatter renders.

use uhura_base::FileId;
use uhura_syntax::{Parsed, SourceKind, format_examples, format_module, parse};

fn fmt(src: &str, kind: SourceKind) -> String {
    let out = parse(FileId(0), src, kind);
    assert!(
        out.diagnostics.is_empty(),
        "input must parse clean: {:?}",
        out.diagnostics
    );
    match out.parsed {
        Parsed::Module(f) => format_module(&f),
        Parsed::Examples(e) => format_examples(&e),
    }
}

fn assert_fixpoint(src: &str, kind: SourceKind) {
    let once = fmt(src, kind);
    let twice = fmt(&once, kind);
    assert_eq!(once, twice, "formatter is not idempotent");
}

// Reuse the normative sources by including the sibling test file's constants
// via a tiny duplication-free include.
include!("common/normative_sources.rs");

#[test]
fn post_card_roundtrips() {
    assert_fixpoint(POST_CARD, SourceKind::Module);
}

#[test]
fn feed_page_roundtrips() {
    assert_fixpoint(FEED_STORE, SourceKind::Module);
}

#[test]
fn examples_roundtrip() {
    assert_fixpoint(FEED_EXAMPLES, SourceKind::Examples);
}

#[test]
fn comment_attachment_survives() {
    let src = "page\n\nstore {\n  state {\n    // the overlay\n    x: bool = false\n  }\n\n  // fires on tap\n  on tapped() {\n    // write it\n    set x = true\n  }\n}\n\n<view />\n";
    let once = fmt(src, SourceKind::Module);
    assert!(once.contains("// the overlay"), "{once}");
    assert!(once.contains("// fires on tap"), "{once}");
    assert!(once.contains("// write it"), "{once}");
    assert_fixpoint(src, SourceKind::Module);
}
