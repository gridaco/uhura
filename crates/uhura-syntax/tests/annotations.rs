use uhura_syntax::ast::{DeclarationKind, Node, UiNameKind, UiNode, UiNodeKind};
use uhura_syntax::{ParseDiagnosticKind, SourceIdentity, format, parse};

fn identity(path: &str) -> SourceIdentity {
    SourceIdentity::new(44, "examples.annotations@1", "annotations", path)
}

fn parse_source(path: &str, body: &str) -> uhura_syntax::Parse {
    parse(
        identity(path),
        &format!("use uhura::ui;\n\nui AppWeb for App(view) {{{body}}}\n"),
    )
}

fn ui_nodes(parsed: &uhura_syntax::Parse) -> &[UiNode] {
    let DeclarationKind::Ui(ui) = &parsed.module.declarations[0].kind else {
        panic!("expected UI declaration")
    };
    &ui.body.nodes
}

fn root_element(parsed: &uhura_syntax::Parse) -> &uhura_syntax::ast::UiElement {
    ui_nodes(parsed)
        .iter()
        .find_map(|node| match &node.kind {
            UiNodeKind::Element(element) => Some(element),
            _ => None,
        })
        .expect("root UI element")
}

#[test]
fn annotations_attach_in_order_across_ordinary_comment_trivia() {
    let body = r#"
  <view>
    <!-- @doc The primary action. -->
    <!-- ordinary, transparent trivia -->
    <!-- @review-note
      Keep this reachable.
      It is the fallback.
    -->
    <button />
  </view>
"#;
    let parsed = parse_source("ordered.uhura", body);
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);

    let root = root_element(&parsed);
    assert_eq!(root.children.len(), 4, "comments remain source-tree trivia");
    let button = root
        .children
        .iter()
        .find_map(|node| match &node.kind {
            UiNodeKind::Element(element) if element.name.text == "button" => Some(element),
            _ => None,
        })
        .expect("button");
    assert_eq!(button.annotations.len(), 2);
    assert_eq!(button.annotations[0].kind, "doc");
    assert_eq!(button.annotations[0].text, "The primary action.");
    assert_eq!(button.annotations[1].kind, "review-note");
    assert_eq!(
        button.annotations[1].text,
        "Keep this reachable.\nIt is the fallback."
    );

    let source = &parsed.module.source;
    let start = source.find("<!-- @doc").unwrap() as u32;
    assert_eq!(button.annotations[0].span.start, start);
    assert_eq!(
        &source[button.annotations[0].span.start as usize..button.annotations[0].span.end as usize],
        "<!-- @doc The primary action. -->"
    );
}

#[test]
fn ordinary_comments_do_not_split_semantic_text_runs() {
    let parsed = parse_source(
        "text-comment.uhura",
        "\n  <p>Hello<!-- ordinary -->world</p>\n",
    );
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let paragraph = root_element(&parsed);
    assert!(
        matches!(paragraph.children.as_slice(), [
            Node { kind: UiNodeKind::Text(left), .. },
            Node { kind: UiNodeKind::Comment(_), .. },
            Node { kind: UiNodeKind::Text(right), .. },
        ] if left.raw == "Hello" && right.raw == "world"),
        "{:#?}",
        paragraph.children
    );

    let formatted = format(&parsed.module).expect("ordinary markup comment formats");
    let hello = formatted.find("Hello").expect("leading text");
    let comment = formatted.find("<!-- ordinary -->").expect("comment");
    let world = formatted.find("world").expect("trailing text");
    assert!(hello < comment && comment < world, "{formatted}");
    let reparsed = parse(identity("text-comment.formatted.uhura"), &formatted);
    assert!(
        reparsed.diagnostics.is_empty(),
        "{:#?}",
        reparsed.diagnostics
    );
    let paragraph = root_element(&reparsed);
    assert!(
        matches!(
            paragraph.children.as_slice(),
            [
                Node {
                    kind: UiNodeKind::Text(_),
                    ..
                },
                Node {
                    kind: UiNodeKind::Comment(_),
                    ..
                },
                Node {
                    kind: UiNodeKind::Text(_),
                    ..
                },
            ]
        ),
        "{:#?}",
        paragraph.children
    );
    assert_eq!(
        format(&reparsed.module).expect("canonical comment formats again"),
        formatted
    );
}

#[test]
fn native_elements_and_complete_structural_blocks_are_targets() {
    let body = r#"
  <view>
    <!-- @rationale Select one complete branch. -->
    {#if view.ready}
      <p>Ready</p>
    {:else}
      <p>Waiting</p>
    {/if}
    <!-- @review-note Stable keyed iteration. -->
    {#each view.items as item (item.id)}
      <p>{item.label}</p>
    {/each}
  </view>
"#;
    let parsed = parse_source("targets.uhura", body);
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let root = root_element(&parsed);
    let conditional = root
        .children
        .iter()
        .find_map(|node| match &node.kind {
            UiNodeKind::If(value) => Some(value),
            _ => None,
        })
        .expect("if block");
    assert_eq!(conditional.annotations[0].kind, "rationale");
    let each = root
        .children
        .iter()
        .find_map(|node| match &node.kind {
            UiNodeKind::Each(value) => Some(value),
            _ => None,
        })
        .expect("each block");
    assert_eq!(each.annotations[0].kind, "review-note");
}

#[test]
fn annotations_do_not_cross_incompatible_constructs_or_boundaries() {
    let incompatible = parse_source(
        "incompatible.uhura",
        r#"
  <view>
    <!-- @annotation Not component metadata. -->
    <Card />
    <!-- @annotation Not text metadata. -->
    literal
    <button />
  </view>
"#,
    );
    let diagnostics = incompatible
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.kind == ParseDiagnosticKind::IncompatibleMetadataTarget)
        .collect::<Vec<_>>();
    assert_eq!(diagnostics.len(), 2, "{:#?}", incompatible.diagnostics);
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.labels.len() == 1)
    );
    let public = diagnostics[0].clone().into_public_diagnostic();
    assert_eq!(public.code, "UH0019");
    assert_eq!(public.rule, "syntax/incompatible-metadata-target");
    assert_eq!(public.labels.len(), 1);

    let root = root_element(&incompatible);
    let elements = root
        .children
        .iter()
        .filter_map(|node| match &node.kind {
            UiNodeKind::Element(element) => Some(element),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(elements[0].name.kind, UiNameKind::Component);
    assert!(
        elements
            .iter()
            .all(|element| element.annotations.is_empty())
    );

    let dangling = parse_source(
        "dangling.uhura",
        r#"
  <view>
    {#if view.ready}
      <!-- @annotation Cannot cross the else arm. -->
    {:else}
      <button />
    {/if}
    <!-- @annotation Cannot cross the element close. -->
  </view>
"#,
    );
    let dangling = dangling
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.kind == ParseDiagnosticKind::DanglingMetadata)
        .collect::<Vec<_>>();
    assert_eq!(dangling.len(), 2);
    let public = dangling[0].clone().into_public_diagnostic();
    assert_eq!(public.code, "UH0017");
    assert_eq!(public.labels.len(), 1);
}

#[test]
fn malformed_annotations_use_the_reserved_diagnostic_and_never_attach() {
    for (name, comment) in [
        ("uppercase", "<!-- @Bad payload -->"),
        ("empty", "<!-- @annotation -->"),
        ("double-dash", "<!-- body -- inside -->"),
        ("trailing-dash", "<!-- body --->"),
    ] {
        let parsed = parse_source(
            &format!("{name}.uhura"),
            &format!("\n  <view>{comment}<button /></view>\n"),
        );
        let malformed = parsed
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.kind == ParseDiagnosticKind::MalformedMarkupComment)
            .unwrap_or_else(|| panic!("{name}: {:#?}", parsed.diagnostics));
        assert_eq!(
            malformed.clone().into_public_diagnostic().code,
            "UH0016",
            "{name}"
        );
        let root = root_element(&parsed);
        let button = root
            .children
            .iter()
            .find_map(|node| match &node.kind {
                UiNodeKind::Element(element) if element.name.text == "button" => Some(element),
                _ => None,
            })
            .expect("button survives comment recovery");
        assert!(button.annotations.is_empty(), "{name}");
    }
}

#[test]
fn formatter_canonicalizes_annotations_and_is_idempotent() {
    let source = r#"
  <view>
    <!--   @annotation   One line.   -->
    <button />
    <!-- @review-note
        First line.{TRAILING_SPACES}
        Second line.
    -->
    {#if view.ready}<p>Ready</p>{/if}
  </view>
"#
    .replace("{TRAILING_SPACES}", "  ");
    let parsed = parse_source("format.uhura", &source);
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let formatted = format(&parsed.module).expect("valid markup annotations format");
    assert!(formatted.contains("<!-- @annotation One line. -->"));
    assert!(
        formatted.contains("<!-- @review-note\n    First line.\n    Second line.\n    -->"),
        "{formatted}"
    );

    let reparsed = parse(identity("format.formatted.uhura"), &formatted);
    assert!(
        reparsed.diagnostics.is_empty(),
        "{:#?}",
        reparsed.diagnostics
    );
    assert_eq!(
        format(&reparsed.module).expect("canonical annotations format again"),
        formatted
    );
}

#[test]
fn unterminated_comment_recovers_before_the_next_sibling_and_declaration() {
    let source = r#"use uhura::ui;

ui AppWeb for App(view) {
  <view>
    <!-- @annotation missing close
    <button />
  </view>
}

const AFTER: Bool = true;
"#;
    let parsed = parse(identity("unterminated.uhura"), source);
    assert_eq!(
        parsed.module.declarations.len(),
        2,
        "{:#?}",
        parsed.diagnostics
    );
    assert_eq!(
        parsed
            .diagnostics
            .iter()
            .filter(|diagnostic| { diagnostic.kind == ParseDiagnosticKind::MalformedMarkupComment })
            .count(),
        1,
        "{:#?}",
        parsed.diagnostics
    );
    let root = root_element(&parsed);
    assert!(root.children.iter().any(
        |node| matches!(&node.kind, UiNodeKind::Element(element) if element.name.text == "button")
    ));
}

#[test]
fn malformed_and_rejected_comment_recovery_is_stable_through_formatting() {
    for (name, body) in [
        (
            "trailing-dash",
            "\n  <view><!-- body ---><button /></view>\n",
        ),
        (
            "unterminated",
            "\n  <view><!-- @doc missing close\n    <button />\n  </view>\n",
        ),
    ] {
        let parsed = parse_source(&format!("{name}.uhura"), body);
        assert_eq!(
            parsed
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.kind == ParseDiagnosticKind::MalformedMarkupComment)
                .count(),
            1,
            "{name}: {:#?}",
            parsed.diagnostics
        );
        let formatted = format(&parsed.module).expect("recovered markup formats stably");
        let reparsed = parse(identity(&format!("{name}.formatted.uhura")), &formatted);
        assert_eq!(
            reparsed
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.kind == ParseDiagnosticKind::MalformedMarkupComment)
                .count(),
            1,
            "{name}: malformed state was lost after formatting\n{formatted}\n{:#?}",
            reparsed.diagnostics
        );
        let root = root_element(&reparsed);
        let button = root
            .children
            .iter()
            .find_map(|node| match &node.kind {
                UiNodeKind::Element(element) if element.name.text == "button" => Some(element),
                _ => None,
            })
            .expect("button survives recovered comment formatting");
        assert!(button.annotations.is_empty(), "{name}");
        assert_eq!(
            format(&reparsed.module).expect("recovered markup formats again"),
            formatted,
            "{name}"
        );
    }

    let rejected = parse_source(
        "rejected.uhura",
        r#"
  <view>
    <!-- @doc Must not retarget. -->
    {#unknown}
    <button />
  </view>
"#,
    );
    assert!(
        rejected
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == ParseDiagnosticKind::IncompatibleMetadataTarget),
        "{:#?}",
        rejected.diagnostics
    );
    let formatted = format(&rejected.module).expect("rejected annotation formats inertly");
    let reparsed = parse(identity("rejected.formatted.uhura"), &formatted);
    assert!(
        reparsed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == ParseDiagnosticKind::MalformedMarkupComment),
        "rejected carrier became valid\n{formatted}\n{:#?}",
        reparsed.diagnostics
    );
    let root = root_element(&reparsed);
    let button = root
        .children
        .iter()
        .find_map(|node| match &node.kind {
            UiNodeKind::Element(element) if element.name.text == "button" => Some(element),
            _ => None,
        })
        .expect("button remains after rejected annotation recovery");
    assert!(button.annotations.is_empty());
    assert_eq!(
        format(&reparsed.module).expect("malformed carrier formats again"),
        formatted
    );
}
