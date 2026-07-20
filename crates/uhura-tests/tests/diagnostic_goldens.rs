use std::panic::{AssertUnwindSafe, catch_unwind};

use uhura_base::{SourceMap, to_envelope};
use uhura_check::check_v04_module;
use uhura_syntax::v04::{SourceIdentity, parse};

struct GoldenCase {
    name: &'static str,
    source: &'static str,
    expected: &'static str,
}

const CASES: &[GoldenCase] = &[
    GoldenCase {
        name: "parse-invalid-expression",
        source: include_str!("fixtures/diagnostics/v04/parse-invalid-expression.uhura"),
        expected: include_str!("fixtures/diagnostics/v04/parse-invalid-expression.json"),
    },
    GoldenCase {
        name: "parse-fix-declaration-typo",
        source: include_str!("fixtures/diagnostics/v04/parse-fix-declaration-typo.uhura"),
        expected: include_str!("fixtures/diagnostics/v04/parse-fix-declaration-typo.json"),
    },
    GoldenCase {
        name: "name-resolution",
        source: include_str!("fixtures/diagnostics/v04/name-resolution.uhura"),
        expected: include_str!("fixtures/diagnostics/v04/name-resolution.json"),
    },
    GoldenCase {
        name: "type-mismatch",
        source: include_str!("fixtures/diagnostics/v04/type-mismatch.uhura"),
        expected: include_str!("fixtures/diagnostics/v04/type-mismatch.json"),
    },
    GoldenCase {
        name: "ui-content",
        source: include_str!("fixtures/diagnostics/v04/ui-content.uhura"),
        expected: include_str!("fixtures/diagnostics/v04/ui-content.json"),
    },
];

fn public_envelope(path: &str, source: &str) -> serde_json::Value {
    let mut source_map = SourceMap::new();
    let file = source_map.add(path, source);
    let parsed = parse(
        SourceIdentity::new(file.0, "diagnostics.fixture@1", "fixture", path),
        source,
    );
    let diagnostics = if parsed.diagnostics.is_empty() {
        check_v04_module(&parsed.module).diagnostics
    } else {
        parsed
            .diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.into_public_diagnostic())
            .collect()
    };
    to_envelope(&diagnostics, &source_map)
}

#[test]
fn v04_public_diagnostic_envelopes_match_goldens() {
    let mut mismatches = Vec::new();
    for case in CASES {
        let path = format!("diagnostics/{}.uhura", case.name);
        let actual = public_envelope(&path, case.source);
        let expected: serde_json::Value = serde_json::from_str(case.expected)
            .unwrap_or_else(|error| panic!("invalid golden for {}: {error}", case.name));
        if actual != expected {
            mismatches.push(format!(
                "{}:\n{}",
                case.name,
                serde_json::to_string_pretty(&actual).expect("serialize actual envelope")
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "public diagnostic envelopes changed:\n\n{}",
        mismatches.join("\n\n")
    );
}

#[test]
fn v04_malformed_input_smoke_never_panics() {
    const MALFORMED: &[&str] = &[
        "",
        "pub",
        "pub machine",
        "pub machine Example {",
        "const VALUE: Int = ;",
        "fn broken( -> {",
        "use uhura::ui; pub ui Broken for Missing(view) { <button>",
        "use uhura::ui; pub ui Broken for Missing(view) { {#if } {/if} }",
        "use uhura::ui; pub ui Broken for Missing(view) { <view foo={}> }",
        "machine Example { events { Run } outcomes { commit Done } on Run { @@@ } }",
    ];
    const VALID_BUT_INVALID_SEMANTICS: &[&str] = &[
        "pub const VALUE: Int = missing;",
        "pub const VALUE: Int = true;",
        r#"use uhura::ui;
pub machine Example {
  events { Run }
  outcomes { commit Done }
  state {}
  on Run { Done }
}
pub ui ExampleWeb for Example(view) {
  <button><icon name="heart"/></button>
}
"#,
    ];
    const VALID_SEED: &str = r#"pub machine Counter {
  events { Increment }
  outcomes { commit Accepted }
  state { count: Int = 0 }
  observe { count }
  on Increment {
    count = count + 1;
    Accepted
  }
}
"#;

    let mut parser_inputs = MALFORMED
        .iter()
        .map(|source| (*source).to_owned())
        .collect::<Vec<_>>();
    for offset in (0..VALID_SEED.len()).step_by(11) {
        let mut removed = VALID_SEED.to_owned();
        removed.remove(offset);
        parser_inputs.push(removed);

        let mut inserted = VALID_SEED.to_owned();
        inserted.insert(offset, '@');
        parser_inputs.push(inserted);

        parser_inputs.push(VALID_SEED[..offset].to_owned());
    }

    for (index, source) in parser_inputs.iter().enumerate() {
        let result = catch_unwind(AssertUnwindSafe(|| {
            parse(
                SourceIdentity::new(
                    0,
                    "diagnostics.smoke@1",
                    "malformed",
                    format!("malformed-{index}.uhura"),
                ),
                source,
            )
        }));
        assert!(result.is_ok(), "parser panicked for malformed case {index}");
    }

    for (index, source) in VALID_BUT_INVALID_SEMANTICS.iter().enumerate() {
        let result = catch_unwind(AssertUnwindSafe(|| {
            let parsed = parse(
                SourceIdentity::new(
                    0,
                    "diagnostics.smoke@1",
                    "semantic",
                    format!("semantic-{index}.uhura"),
                ),
                source,
            );
            assert!(
                parsed.diagnostics.is_empty(),
                "semantic smoke case must parse: {:#?}",
                parsed.diagnostics
            );
            check_v04_module(&parsed.module)
        }));
        assert!(result.is_ok(), "checker panicked for semantic case {index}");
    }
}
