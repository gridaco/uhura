use uhura_syntax::{LexDiagnosticKind, SourceIdentity, TriviaKind, lex};

fn identity() -> SourceIdentity {
    SourceIdentity::new(3, "test@1", "test", "test.uhura")
}

#[test]
fn classifies_comments_and_decodes_json_text() {
    let source =
        "//! file\n/// outer\n//// ordinary\nconst TEXT: Text = \"A\\uD83D\\uDE80\\n\"; // tail\n";
    let output = lex(&identity(), source);
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    let kinds = output
        .tokens
        .iter()
        .flat_map(|token| token.leading.iter().map(|trivia| trivia.kind))
        .collect::<Vec<_>>();
    assert!(kinds.contains(&TriviaKind::InnerDoc));
    assert!(kinds.contains(&TriviaKind::OuterDoc));
    assert!(kinds.contains(&TriviaKind::OrdinaryComment));
    assert!(output.tokens.iter().any(|token| {
        matches!(&token.kind, uhura_syntax::TokenKind::Text(value) if value == "A🚀\n")
    }));
}

#[test]
fn rejects_non_core_lexical_spellings_deterministically() {
    for (source, expected) in [
        ("\u{feff}const X: Int = 0;", LexDiagnosticKind::InitialBom),
        (
            "const CAFÉ: Int = 0;",
            LexDiagnosticKind::NonAsciiIdentifier,
        ),
        ("const X: Int = 01;", LexDiagnosticKind::InvalidNumber),
        ("const X: Decimal = .5;", LexDiagnosticKind::InvalidNumber),
        ("const X: Text = \"\\x\";", LexDiagnosticKind::InvalidEscape),
        (
            "const X: Text = \"\\uD800\";",
            LexDiagnosticKind::InvalidSurrogatePair,
        ),
        (
            "const X: Text = \"line\n\";",
            LexDiagnosticKind::InvalidEscape,
        ),
        (
            "const X:\u{00a0}Int = 0;",
            LexDiagnosticKind::InvalidWhitespace,
        ),
    ] {
        let output = lex(&identity(), source);
        assert!(
            output
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.kind == expected),
            "expected {expected:?} for {source:?}, got {:#?}",
            output.diagnostics
        );
        let reconstructed = output
            .tokens
            .iter()
            .flat_map(|token| {
                token
                    .leading
                    .iter()
                    .map(|trivia| trivia.text.as_str())
                    .chain(std::iter::once(token.lexeme.as_str()))
            })
            .collect::<String>();
        assert_eq!(reconstructed, source);
    }
}
