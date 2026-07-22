use uhura_syntax::ast::{
    BinaryOperator, DeclarationKind, ExpressionKind, MachineMemberKind, StatementKind,
};
use uhura_syntax::{ParseDiagnosticKind, ParseFix, SourceIdentity, Span, parse};

fn parse_source(source: &str) -> uhura_syntax::Parse {
    parse(
        SourceIdentity::new(11, "test@1", "precedence", "precedence.uhura"),
        source,
    )
}

#[test]
fn applies_the_frozen_operator_precedence() {
    let parsed = parse_source("const VALUE: Bool = a || b && c == d + e * f;");
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let DeclarationKind::Const(value) = &parsed.module.declarations[0].kind else {
        panic!("expected const");
    };
    let ExpressionKind::Binary {
        operator: BinaryOperator::Or,
        right,
        ..
    } = &value.value.kind
    else {
        panic!("expected top-level logical or: {:#?}", value.value);
    };
    assert!(matches!(
        right.kind,
        ExpressionKind::Binary {
            operator: BinaryOperator::And,
            ..
        }
    ));
}

#[test]
fn rejects_comparison_chains() {
    let parsed = parse_source("const VALUE: Bool = a < b < c;");
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == ParseDiagnosticKind::ComparisonChain)
    );
}

#[test]
fn preserves_block_tail_and_every_authored_semicolon() {
    let source = r#"machine Example {
  events { Run, }
  outcomes { commit Accepted, }
  state { count: Int = 0, }
  on Run {
    let next = count + 1;
    count = next;
    if next > 2 {
      count = 2;
    }
    Accepted
  }
}
"#;
    let parsed = parse_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let DeclarationKind::Machine(machine) = &parsed.module.declarations[0].kind else {
        panic!("expected machine");
    };
    let handler = machine
        .members
        .iter()
        .find_map(|member| match &member.kind {
            MachineMemberKind::Handler(handler) => Some(handler),
            _ => None,
        })
        .expect("handler");
    assert_eq!(handler.body.statements.len(), 3);
    assert!(handler.body.tail.is_some());
    assert!(matches!(
        handler.body.statements[0].kind,
        StatementKind::Let { .. }
    ));
    assert!(matches!(
        handler.body.statements[1].kind,
        StatementKind::Assign { .. }
    ));
    assert!(matches!(
        handler.body.statements[2].kind,
        StatementKind::BlockExpression(_)
    ));
}

#[test]
fn diagnoses_missing_semicolons_and_recovers_to_later_statements() {
    let source = r#"machine Example {
  events { Run, }
  outcomes { commit Accepted, }
  state { count: Int = 0, }
  on Run {
    let next = count + 1
    count = next
    Accepted
  }
}
"#;
    let parsed = parse_source(source);
    let missing = parsed
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.kind == ParseDiagnosticKind::MissingToken)
        .collect::<Vec<_>>();
    assert_eq!(missing.len(), 2, "{:#?}", parsed.diagnostics);
    let assignment = source.find("count = next").unwrap() as u32;
    let accepted = source.find("Accepted\n").unwrap() as u32;
    assert_eq!(
        missing[0],
        &uhura_syntax::ParseDiagnostic {
            kind: ParseDiagnosticKind::MissingToken,
            message: "expected `;` in let statement".into(),
            span: Span::new(11, assignment, assignment + "count".len() as u32),
            labels: vec![],
            fix: Some(ParseFix {
                title: "Insert `;`".into(),
                span: Span::empty(11, assignment),
                insert: ";".into(),
            }),
        }
    );
    assert_eq!(
        missing[1],
        &uhura_syntax::ParseDiagnostic {
            kind: ParseDiagnosticKind::MissingToken,
            message: "expected `;` in state assignment".into(),
            span: Span::new(11, accepted, accepted + "Accepted".len() as u32),
            labels: vec![],
            fix: Some(ParseFix {
                title: "Insert `;`".into(),
                span: Span::empty(11, accepted),
                insert: ";".into(),
            }),
        }
    );
}

#[test]
fn declaration_typo_has_one_stable_kind_and_safe_replacement() {
    let source = "pub mashine Counter {}\n";
    let parsed = parse_source(source);
    assert_eq!(
        parsed.diagnostics,
        vec![uhura_syntax::ParseDiagnostic {
            kind: ParseDiagnosticKind::InvalidDeclaration,
            message: "unknown module declaration `mashine`; expected `machine`, `part`, `ui`, `scenario`, `example`, `checkpoint`, `struct`, `enum`, `key`, `const`, or `fn`".into(),
            span: Span::new(11, 4, 11),
            labels: vec![],
            fix: Some(ParseFix {
                title: "Replace `mashine` with `machine`".into(),
                span: Span::new(11, 4, 11),
                insert: "machine".into(),
            }),
        }]
    );
}

#[test]
fn missing_expression_preserves_the_enclosing_delimiter_without_a_cascade() {
    let source = "pub const INITIAL: Int = ;\n";
    let parsed = parse_source(source);
    let semicolon = source.find(';').unwrap() as u32;
    assert_eq!(
        parsed.diagnostics,
        vec![uhura_syntax::ParseDiagnostic {
            kind: ParseDiagnosticKind::InvalidExpression,
            message: "expected expression, found `;`".into(),
            span: Span::new(11, semicolon, semicolon + 1),
            labels: vec![],
            fix: None,
        }]
    );
    let diagnostic = parsed
        .diagnostics
        .into_iter()
        .next()
        .unwrap()
        .into_public_diagnostic();
    assert_eq!(diagnostic.code, "R1001");
    assert_eq!(diagnostic.rule, "uhura-0.4/parse/invalid-expression");
    assert_eq!(diagnostic.span.start, semicolon);
    assert_eq!(diagnostic.span.end, semicolon + 1);
}

#[test]
fn admits_keywords_only_as_contextual_member_and_explicit_record_labels() {
    let source = r#"struct Entry {
  value: Text,
}

fn project(entry: Entry) -> Text {
  let copied = Entry {
    key: entry.key,
    match: entry.match,
  };
  match copied {
    Entry { key: id, match: value } => value,
  }
}
"#;
    let parsed = parse_source(source);
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);

    let shorthand = parse_source("const VALUE: Entry = Entry { key };");
    assert!(shorthand.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == ParseDiagnosticKind::InvalidName
            && diagnostic.message.contains("keyword shorthand")
    }));
}

#[test]
fn evidence_patterns_admit_nominal_key_constructors_without_relaxing_core_patterns() {
    let evidence = parse_source(
        r#"scenario pending for Counter {
  start
  expect inspection {
    request: {
      id: RequestId(1),
      ..
    },
    ..
  }
}
"#,
    );
    assert!(
        evidence.diagnostics.is_empty(),
        "{:#?}",
        evidence.diagnostics
    );

    let core = parse_source(
        r#"fn is_first(value: RequestId) -> Bool {
  match value {
    RequestId(1) => true,
    _ => false,
  }
}
"#,
    );
    assert!(core.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == ParseDiagnosticKind::InvalidPattern
            && diagnostic.message.contains("only the prelude")
    }));
}
