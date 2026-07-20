use uhura_base::FileId;

use super::ast::{
    DeclarationKind, EvidenceStepKind, ExprKind, MachineMemberKind, Module, SourceId,
    UiAttributeValue, UiNode, UiNodeKind,
};
use super::{SourceFile, format, parse, parse_project};

const PROGRAMS: &str = include_str!("../../../examples/programs/answers/uhura-0.3/programs.uhura");
const MACHINE: &str =
    include_str!("../../../examples/applications/a0-return-desk/answers/uhura-0.3/machine.uhura");
const WEB: &str =
    include_str!("../../../examples/applications/a0-return-desk/answers/uhura-0.3/web.uhura");
const CONFORMANCE: &str = include_str!(
    "../../../examples/applications/a0-return-desk/answers/uhura-0.3/conformance.uhura"
);

fn assert_parses(file: u32, path: &str, source: &str) {
    let parsed = parse(SourceId::new(FileId(file), path), source);
    assert!(
        parsed.diagnostics.is_empty(),
        "{path} diagnostics:\n{:#?}",
        parsed.diagnostics
    );
    assert!(parsed.module.is_some(), "{path} did not produce a module");
}

#[test]
fn parses_every_checked_in_uhura_0_3_answer() {
    assert_parses(0, "programs.uhura", PROGRAMS);
    assert_parses(1, "machine.uhura", MACHINE);
    assert_parses(2, "web.uhura", WEB);
    assert_parses(3, "conformance.uhura", CONFORMANCE);
}

#[test]
fn checked_in_answers_format_idempotently() {
    for (file, path, source) in [
        (0, "programs.uhura", PROGRAMS),
        (1, "machine.uhura", MACHINE),
        (2, "web.uhura", WEB),
        (3, "conformance.uhura", CONFORMANCE),
    ] {
        let first = parse(SourceId::new(FileId(file), path), source);
        let module = first.module.expect("module");
        let once = format(&module);
        let second = parse(SourceId::new(FileId(file), path), &once);
        assert!(
            second.diagnostics.is_empty(),
            "formatted {path} diagnostics: {:#?}",
            second.diagnostics
        );
        let reparsed = second.module.expect("formatted module");
        assert_eq!(module, reparsed, "{path} structural round trip");
        assert_eq!(once, format(&reparsed), "{path} formatter idempotence");
    }
}

#[test]
fn project_parser_keeps_source_order() {
    let parsed = parse_project([
        SourceFile::new(FileId(0), "machine.uhura", MACHINE),
        SourceFile::new(FileId(1), "web.uhura", WEB),
        SourceFile::new(FileId(2), "conformance.uhura", CONFORMANCE),
    ]);
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert_eq!(parsed.project.modules.len(), 3);
    assert_eq!(
        parsed.project.modules[0].identity.logical_name(),
        "app.return_desk.machine"
    );
    assert_eq!(
        parsed.project.modules[1].identity.logical_name(),
        "app.return_desk.web"
    );
    assert_eq!(
        parsed.project.modules[2].identity.logical_name(),
        "app.return_desk.conformance"
    );
}

#[test]
fn ui_and_evidence_are_structural_nodes() {
    let web = parse(SourceId::new(FileId(0), "web.uhura"), WEB)
        .module
        .expect("web module");
    let ui = web
        .declarations
        .iter()
        .find_map(|declaration| match &declaration.value {
            DeclarationKind::Ui(ui) => Some(ui),
            _ => None,
        })
        .expect("ui declaration");
    assert!(!ui.nodes.is_empty());
    assert!(count_ui_events(&ui.nodes) >= 10);

    let evidence = parse(SourceId::new(FileId(1), "conformance.uhura"), CONFORMANCE)
        .module
        .expect("evidence module");
    assert!(
        evidence
            .declarations
            .iter()
            .any(|declaration| matches!(declaration.value, DeclarationKind::Scenario(_)))
    );
    assert!(
        evidence
            .declarations
            .iter()
            .any(|declaration| matches!(declaration.value, DeclarationKind::Checkpoint(_)))
    );
    assert_eq!(
        evidence
            .declarations
            .iter()
            .filter(|declaration| matches!(declaration.value, DeclarationKind::Scenario(_)))
            .count(),
        19
    );
    assert_eq!(
        evidence
            .declarations
            .iter()
            .filter(|declaration| matches!(declaration.value, DeclarationKind::Example(_)))
            .count(),
        12
    );
    assert!(evidence.declarations.iter().any(|declaration| {
        matches!(
            &declaration.value,
            DeclarationKind::Scenario(scenario)
                if scenario.steps.iter().any(|step| matches!(
                    step.value,
                    EvidenceStepKind::ExpectSnapshot { .. }
                ))
        )
    }));
}

#[test]
fn scenario_machine_origin_accepts_an_inline_configuration() {
    let source = r#"language uhura 0.3
module regression.configured_scenario@1

use evidence

scenario bounded for Counter({ minimum: 0, maximum: 2, initial: 1 }) {
  start
}
"#;
    let parsed = parse(SourceId::new(FileId(0), "configured.uhura"), source);
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let module = parsed.module.expect("configured scenario module");
    let scenario = module
        .declarations
        .iter()
        .find_map(|declaration| match &declaration.value {
            DeclarationKind::Scenario(scenario) => Some(scenario),
            _ => None,
        })
        .expect("configured scenario");
    let super::ast::ScenarioOrigin::Machine {
        machine,
        configuration: Some(configuration),
    } = &scenario.origin
    else {
        panic!("expected configured machine origin");
    };
    assert_eq!(machine.value, "Counter");
    assert!(matches!(configuration.value, ExprKind::Record(_)));
}

#[test]
fn lexer_accepts_unicode_xid_and_exact_numbers() {
    let source = r#"language uhura 0.3
module examples.δ@1
const café: Decimal = 12345678901234567890.000100
"#;
    let parsed = parse(SourceId::new(FileId(0), "unicode.uhura"), source);
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
}

#[test]
fn reserved_metadata_comments_are_diagnosed() {
    let source = "language uhura 0.3\nmodule x@1\n/// reserved\n";
    let parsed = parse(SourceId::new(FileId(0), "comment.uhura"), source);
    assert_eq!(parsed.diagnostics.len(), 1);
    assert!(parsed.diagnostics[0].message.contains("reserved"));
}

#[test]
fn a0_machine_keeps_ports_and_qualified_handlers() {
    let module = parse(SourceId::new(FileId(0), "machine.uhura"), MACHINE)
        .module
        .expect("machine module");
    let machine = module
        .declarations
        .iter()
        .find_map(|declaration| match &declaration.value {
            DeclarationKind::Machine(machine) => Some(machine),
            _ => None,
        })
        .expect("ReturnDesk");
    assert_eq!(
        machine
            .members
            .iter()
            .filter(|member| matches!(member.value, MachineMemberKind::Port(_)))
            .count(),
        3
    );
    assert!(machine.members.iter().any(|member| {
        matches!(
            &member.value,
            MachineMemberKind::Handler(handler)
                if matches!(
                    &handler.input.value,
                    super::ast::PatternKind::Constructor { path, .. }
                        if path.iter().map(|part| part.value.as_str()).eq(["router", "changed"])
                )
        )
    }));
}

#[test]
fn module_ast_is_serde_ready() {
    fn assert_wire<T: serde::Serialize + for<'de> serde::Deserialize<'de>>() {}
    assert_wire::<Module>();
}

#[test]
fn exact_envelope_and_import_identity_are_validated() {
    let invalid = r#"language uhura 0.4
module invalid@0
import { Thing } from "broken"
"#;
    let parsed = parse(SourceId::new(FileId(0), "invalid.uhura"), invalid);
    assert_eq!(parsed.diagnostics.len(), 3, "{:#?}", parsed.diagnostics);
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("version"))
    );
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("positive integer"))
    );
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("import target"))
    );
}

#[test]
fn retired_relay_language_header_is_rejected() {
    let parsed = parse(
        SourceId::new(FileId(0), "old.uhura"),
        "language relay 0.3\nmodule old@1\n",
    );
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("expected `uhura`")),
        "{:#?}",
        parsed.diagnostics
    );
}

#[test]
fn relay_is_an_ordinary_identifier_outside_the_retired_header() {
    let source = r#"language uhura 0.3
module valid.identifiers@1
const relay: Int = 1
const copy: Int = relay
"#;
    assert_parses(0, "relay-identifier.uhura", source);
}

#[test]
fn utf8_identifier_spans_are_byte_accurate() {
    let source = "language uhura 0.3\nmodule δ.café@1\n";
    let module = parse(SourceId::new(FileId(0), "utf8.uhura"), source)
        .module
        .expect("module");
    let name = &module.identity.path[1];
    assert_eq!(
        &source[name.span.start as usize..name.span.end as usize],
        "café"
    );
}

#[test]
fn malformed_text_escape_is_source_spanned() {
    let source = "language uhura 0.3\nmodule bad@1\nconst value: Text = \"\\q\"\n";
    let parsed = parse(SourceId::new(FileId(0), "escape.uhura"), source);
    assert_eq!(parsed.diagnostics.len(), 1, "{:#?}", parsed.diagnostics);
    let diagnostic = &parsed.diagnostics[0];
    assert!(diagnostic.message.contains("escape"));
    assert!(diagnostic.span.end > diagnostic.span.start);
}

#[test]
fn conservative_formatter_normalizes_host_line_endings() {
    let source = "language uhura 0.3\r\nmodule sample@1  \r\n";
    let module = parse(SourceId::new(FileId(0), "format.uhura"), source)
        .module
        .expect("module");
    assert_eq!(format(&module), "language uhura 0.3\nmodule sample@1\n");
}

#[test]
fn declaration_and_lambda_bindings_reject_reserved_names() {
    let source = r#"language uhura 0.3
module invalid.bindings@1
const if: Int = 1
const Int: Int = 2
const predicate: Predicate = Nat => true
"#;
    let parsed = parse(SourceId::new(FileId(0), "bindings.uhura"), source);
    assert_eq!(parsed.diagnostics.len(), 3, "{:#?}", parsed.diagnostics);
    let spellings = parsed
        .diagnostics
        .iter()
        .map(|diagnostic| &source[diagnostic.span.start as usize..diagnostic.span.end as usize])
        .collect::<Vec<_>>();
    assert_eq!(spellings, ["if", "Int", "Nat"]);
    assert!(
        parsed.diagnostics[0]
            .message
            .contains("reserved Uhura word")
    );
    assert!(parsed.diagnostics[1].message.contains("binding-reserved"));
    assert!(parsed.diagnostics[2].message.contains("binding-reserved"));
}

#[test]
fn builtin_references_and_contextual_members_remain_valid() {
    let source = r#"language uhura 0.3
module valid.references@1
const value: Option<Int> = Int.from(1)
"#;
    assert_parses(0, "references.uhura", source);
}

#[test]
fn relational_and_equality_comparisons_do_not_chain() {
    let source = r#"language uhura 0.3
module invalid.comparisons@1
const relational: Bool = 1 < 2 < 3
const equality: Bool = 1 == 1 != false
"#;
    let parsed = parse(SourceId::new(FileId(0), "comparisons.uhura"), source);
    assert_eq!(parsed.diagnostics.len(), 2, "{:#?}", parsed.diagnostics);
    let operators = parsed
        .diagnostics
        .iter()
        .map(|diagnostic| &source[diagnostic.span.start as usize..diagnostic.span.end as usize])
        .collect::<Vec<_>>();
    assert_eq!(operators, ["<", "!="]);
    assert!(
        parsed
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.message.contains("do not chain"))
    );
}

#[test]
fn unit_and_one_element_tuple_expressions_are_rejected() {
    let source = r#"language uhura 0.3
module invalid.tuples@1
const unit: Unit = ()
const singleton: Tuple = (1,)
"#;
    let parsed = parse(SourceId::new(FileId(0), "tuples.uhura"), source);
    assert_eq!(parsed.diagnostics.len(), 2, "{:#?}", parsed.diagnostics);
    let spans = parsed
        .diagnostics
        .iter()
        .map(|diagnostic| &source[diagnostic.span.start as usize..diagnostic.span.end as usize])
        .collect::<Vec<_>>();
    assert_eq!(spans, ["()", ",)"]);
    assert!(parsed.diagnostics[0].message.contains("unit tuple"));
    assert!(parsed.diagnostics[1].message.contains("at least two"));
}

#[test]
fn standalone_ellipsis_pattern_is_rejected_at_the_marker() {
    let source = r#"language uhura 0.3
module invalid.ellipsis@1
const invalid: Bool = value is ...
"#;
    let parsed = parse(SourceId::new(FileId(0), "ellipsis.uhura"), source);
    assert_eq!(parsed.diagnostics.len(), 1, "{:#?}", parsed.diagnostics);
    let diagnostic = &parsed.diagnostics[0];
    assert_eq!(
        &source[diagnostic.span.start as usize..diagnostic.span.end as usize],
        "..."
    );
    assert!(diagnostic.message.contains("record pattern"));
}

#[test]
fn record_update_accepts_only_one_with_clause() {
    let source = r#"language uhura 0.3
module invalid.updates@1
const invalid: Item = item with { value: 1 } with { value: 2 }
"#;
    let parsed = parse(SourceId::new(FileId(0), "updates.uhura"), source);
    assert_eq!(parsed.diagnostics.len(), 1, "{:#?}", parsed.diagnostics);
    let diagnostic = &parsed.diagnostics[0];
    assert_eq!(
        &source[diagnostic.span.start as usize..diagnostic.span.end as usize],
        "with"
    );
    assert!(diagnostic.message.contains("only one"));
}

#[test]
fn braces_literal_lookahead_accepts_expression_keys() {
    let source = r#"language uhura 0.3
module valid.map_keys@1
const values: Map<Key, Text> = {
  -1: "negative",
  1.5: "decimal",
  (1 + 1): "grouped",
  Key(3): "constructed",
  namespace.answer: "qualified",
}
"#;
    let module = parse(SourceId::new(FileId(0), "map-keys.uhura"), source);
    assert!(module.diagnostics.is_empty(), "{:#?}", module.diagnostics);
    let declaration = module
        .module
        .expect("module")
        .declarations
        .into_iter()
        .next()
        .expect("constant");
    let DeclarationKind::Const(constant) = declaration.value else {
        panic!("expected constant");
    };
    let ExprKind::Record(entries) = constant.value.value else {
        panic!("expected braces literal");
    };
    assert_eq!(entries.len(), 5);
}

#[test]
fn machine_member_sections_cannot_move_backwards() {
    let source = r#"language uhura 0.3
module invalid.machine_order@1
machine Wrong {
  input = ping
  command = Never
  outcome = accepted commit
  state {}
  observe {}
  config {}
  on ping { finish accepted }
}
"#;
    let parsed = parse(SourceId::new(FileId(0), "machine-order.uhura"), source);
    assert_eq!(parsed.diagnostics.len(), 1, "{:#?}", parsed.diagnostics);
    let diagnostic = &parsed.diagnostics[0];
    assert_eq!(
        &source[diagnostic.span.start as usize..diagnostic.span.end as usize],
        "config {}"
    );
    assert!(diagnostic.message.contains("out of source order"));
}

fn count_ui_events(nodes: &[UiNode]) -> usize {
    nodes
        .iter()
        .map(|node| match &node.value {
            UiNodeKind::Element(element) => {
                element
                    .attributes
                    .iter()
                    .filter(|attribute| matches!(attribute.value, UiAttributeValue::Event { .. }))
                    .count()
                    + count_ui_events(&element.children)
            }
            UiNodeKind::If { children, .. } | UiNodeKind::Each { children, .. } => {
                count_ui_events(children)
            }
            UiNodeKind::Match { cases, .. } => cases
                .iter()
                .map(|case| count_ui_events(&case.children))
                .sum(),
            UiNodeKind::Text(_) | UiNodeKind::Interpolation(_) => 0,
        })
        .sum()
}
