//! Checked comment and authoring-metadata behavior.

use std::collections::BTreeSet;

use uhura_check::{CheckOutput, check_project_modules};
use uhura_core::{Program, RenderNode, UiNode, Value};
use uhura_syntax::{SourceIdentity, parse};

const SOURCE: &str = r#"use uhura::ui;

pub machine App {
  outcomes { commit Done }
  state {}
  observe {}
}

pub ui AppView for App(view) {
  <p>$BODY</p>
}
"#;

fn checked(body: &str) -> CheckOutput {
    let source = SOURCE.replace("$BODY", body);
    let parsed = parse(
        SourceIdentity::new(71, "example.comments@1", "app", "app.uhura"),
        &source,
    );
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    check_project_modules(&[parsed.module])
}

fn presentation_text_node(program: &Program) -> &str {
    let presentation = &program.presentations["example.comments@1::AppView"];
    let UiNode::Element { children, .. } = &presentation.nodes[0] else {
        panic!("expected paragraph root")
    };
    assert_eq!(children.len(), 1, "{children:#?}");
    let UiNode::Text { value, .. } = &children[0] else {
        panic!("expected one coalesced text node")
    };
    value
}

fn projected_text(program: &Program) -> String {
    let (instance, _) = program
        .machine_program
        .admit("example.comments@1::App", Value::Unit, "comments")
        .expect("admit comment fixture");
    let projection = program
        .project(&instance, "example.comments@1::AppView")
        .expect("project comment fixture");
    fn collect(nodes: &[RenderNode], output: &mut String) {
        for node in nodes {
            match node {
                RenderNode::Text { text, .. } => output.push_str(text),
                RenderNode::Element { children, .. } => collect(children, output),
            }
        }
    }
    let mut text = String::new();
    collect(&projection.document.nodes, &mut text);
    text
}

fn provenance_shape(output: &CheckOutput) -> BTreeSet<(String, String, String)> {
    output
        .provenance
        .as_ref()
        .expect("checked provenance")
        .occurrences
        .iter()
        .map(|occurrence| {
            (
                occurrence.node.clone(),
                occurrence.role.clone(),
                occurrence.owner.clone(),
            )
        })
        .collect()
}

#[test]
fn ordinary_markup_comments_are_inert_between_text_fragments() {
    let baseline = checked("Helloworld");
    let commented = checked("Hello<!-- ordinary -->world");
    assert!(
        baseline.diagnostics.is_empty(),
        "{:#?}",
        baseline.diagnostics
    );
    assert!(
        commented.diagnostics.is_empty(),
        "{:#?}",
        commented.diagnostics
    );

    let baseline_program = baseline.program.as_ref().expect("baseline program");
    let commented_program = commented.program.as_ref().expect("commented program");
    assert_eq!(presentation_text_node(baseline_program), "Helloworld");
    assert_eq!(presentation_text_node(commented_program), "Helloworld");
    assert_eq!(
        baseline_program.presentation_hashes,
        commented_program.presentation_hashes
    );
    assert_eq!(projected_text(baseline_program), "Helloworld");
    assert_eq!(projected_text(commented_program), "Helloworld");
    assert_eq!(provenance_shape(&baseline), provenance_shape(&commented));
    assert!(commented.authoring.targets.is_empty());
    assert!(commented.authoring.entries.is_empty());
}
