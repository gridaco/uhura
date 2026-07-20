use uhura_syntax::v04::ast::{
    BinaryOperator, DeclarationKind, ExpressionKind, MachineMemberKind, StatementKind,
};
use uhura_syntax::v04::{ParseDiagnosticKind, SourceIdentity, parse};

fn parse_source(source: &str) -> uhura_syntax::v04::Parse {
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
        .count();
    assert!(missing >= 2, "{:#?}", parsed.diagnostics);
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
