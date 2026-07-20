use uhura_syntax::v04::ast::{DeclarationKind, UiAttribute, UiNameKind, UiNodeKind};
use uhura_syntax::v04::{
    FormatError, ParseDiagnosticKind, SourceIdentity, TokenKind, format, parse,
};

const FEED: &str = include_str!("fixtures/v04-feed-ui.uhura");

fn identity(path: &str) -> SourceIdentity {
    SourceIdentity::new(31, "examples.feed@1", "feed", path)
}

fn parse_clean(path: &str, source: &str) -> uhura_syntax::v04::Parse {
    let parsed = parse(identity(path), source);
    assert!(
        parsed.diagnostics.is_empty(),
        "unexpected diagnostics for {path}:\n{:#?}",
        parsed.diagnostics
    );
    parsed
}

#[test]
fn parses_the_complete_ui_profile_losslessly_with_exact_spans() {
    let parsed = parse_clean("feed.uhura", FEED);
    assert_eq!(parsed.source_from_tokens(), FEED);
    assert_eq!(
        parsed
            .tokens
            .iter()
            .filter(|token| token.kind == TokenKind::UiBody)
            .count(),
        1
    );

    let declaration = &parsed.module.declarations[0];
    let DeclarationKind::Ui(ui) = &declaration.kind else {
        panic!("expected contextual UI declaration");
    };
    assert_eq!(ui.name.text, "FeedWeb");
    assert_eq!(ui.machine.segments[0].name.text, "Feed");
    assert_eq!(ui.observation.text, "view");
    let body_start = FEED.find("Feed(view) {").unwrap() + "Feed(view) {".len();
    assert_eq!(
        &FEED[ui.body.span.start as usize..ui.body.span.end as usize],
        &FEED[body_start..FEED.rfind('}').unwrap()]
    );

    let UiNodeKind::Element(main) = &ui.body.nodes[0].kind else {
        panic!("expected root element");
    };
    assert_eq!(main.name.text, "main");
    assert_eq!(main.name.kind, UiNameKind::Native);
    let root_source =
        &FEED[ui.body.nodes[0].span.start as usize..ui.body.nodes[0].span.end as usize];
    assert!(root_source.starts_with("<main"));
    assert!(root_source.ends_with("</main>"));
    assert!(matches!(main.attributes[0], UiAttribute::StaticText { .. }));

    let each = main
        .children
        .iter()
        .find_map(|node| match &node.kind {
            UiNodeKind::Each(value) => Some(value),
            _ => None,
        })
        .expect("keyed each");
    let component = each
        .children
        .iter()
        .find_map(|node| match &node.kind {
            UiNodeKind::Element(value) => Some(value),
            _ => None,
        })
        .expect("component element");
    assert_eq!(component.name.kind, UiNameKind::Component);
    assert!(component
        .attributes
        .iter()
        .any(|attribute| matches!(attribute, UiAttribute::Boolean { name, .. } if name.text == "featured")));
    assert!(component.attributes.iter().any(
        |attribute| matches!(attribute, UiAttribute::Event { event, .. } if event.text == "like")
    ));
}

#[test]
fn canonical_ui_format_is_parseable_and_idempotent() {
    let parsed = parse_clean("feed.uhura", FEED);
    let formatted = format(&parsed.module).expect("comment-free core expressions format");
    assert!(formatted.contains("pub ui FeedWeb for Feed(view) {"));
    assert!(formatted.contains("{#if view.loading}"));
    assert!(formatted.contains("{:else}"));
    assert!(formatted.contains("{#each view.posts as Post {"));
    assert!(formatted.contains("on like -> ToggleLike(id)"));
    assert!(formatted.contains("featured"));
    assert!(formatted.contains("<!--one stable keyed card-->"));
    assert!(formatted.contains(">Refresh</button>"));
    assert!(formatted.contains("<p>안녕하세요 {view.viewer_name}</p>"));

    let reparsed = parse_clean("feed.formatted.uhura", &formatted);
    let reformatted = format(&reparsed.module).expect("formatted UI must format again");
    assert_eq!(reformatted, formatted);
}

#[test]
fn ui_and_for_remain_contextual_outside_the_declaration_shape() {
    let source = r#"fn ui(value: Bool) -> Bool {
  value
}

fn for_value(value: Bool) -> Bool {
  value
}
"#;
    let parsed = parse_clean("contextual.uhura", source);
    assert_eq!(parsed.module.declarations.len(), 2);
    assert!(
        !parsed
            .tokens
            .iter()
            .any(|token| token.kind == TokenKind::UiBody)
    );
}

#[test]
fn ui_lexical_mode_owns_markup_braces_and_resumes_at_the_next_declaration() {
    let source = r#"use uhura::ui;

ui AppWeb for App(view) {
  <p title="}"><!-- } remains comment text -->{view.label}</p>
}

const AFTER: Bool = true;
"#;
    let parsed = parse_clean("mode-boundary.uhura", source);
    assert_eq!(parsed.source_from_tokens(), source);
    assert_eq!(parsed.module.declarations.len(), 2);
    assert_eq!(
        parsed
            .tokens
            .iter()
            .filter(|token| token.kind == TokenKind::UiBody)
            .count(),
        1
    );
}

#[test]
fn global_nul_refusal_survives_ui_body_isolation() {
    let source = "use uhura::ui;\nui AppWeb for App(view) {<p>\0</p>}\n";
    let parsed = parse(identity("nul.uhura"), source);
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == ParseDiagnosticKind::Lexical
                && diagnostic.message.contains("U+0000"))
    );
    assert_eq!(parsed.source_from_tokens(), source);
}

#[test]
fn reports_invalid_or_non_selected_ui_forms_without_losing_the_module() {
    let cases = [
        (
            "unkeyed",
            "use uhura::ui;\nui AppWeb for App(view) {{#each view.items as item}<p>{item}</p>{/each}}\n",
            "parenthesized key",
        ),
        (
            "match-block",
            "use uhura::ui;\nui AppWeb for App(view) {{#match view.state}{/match}}\n",
            "has no `match` block",
        ),
        (
            "mismatched-tag",
            "use uhura::ui;\nui AppWeb for App(view) {<main></section>}\n",
            "does not match",
        ),
        (
            "missing-arrow",
            "use uhura::ui;\nui AppWeb for App(view) {<button on press Submit />}\n",
            "expected `->`",
        ),
        (
            "unquoted-attribute",
            "use uhura::ui;\nui AppWeb for App(view) {<p class=notice>Hi</p>}\n",
            "quoted text",
        ),
        (
            "missing-if-close",
            "use uhura::ui;\nui AppWeb for App(view) {{#if view.ready}<p>Ready</p>}\n",
            "missing `{/if}`",
        ),
    ];

    for (name, source, expected) in cases {
        let parsed = parse(identity(&format!("{name}.uhura")), source);
        assert!(
            parsed.diagnostics.iter().any(|diagnostic| {
                diagnostic.kind == ParseDiagnosticKind::InvalidUi
                    && diagnostic.message.contains(expected)
            }),
            "{name}: {:#?}",
            parsed.diagnostics
        );
        assert_eq!(
            parsed.source_from_tokens(),
            source,
            "{name} must stay lossless"
        );
        assert_eq!(parsed.module.declarations.len(), 1, "{name} must recover");
    }
}

#[test]
fn formatter_preserves_markup_comments_and_refuses_embedded_core_comments() {
    let markup = parse_clean(
        "markup-comment.uhura",
        "use uhura::ui;\nui AppWeb for App(view) {<!-- note --><p>Ready</p>}\n",
    );
    let formatted = format(&markup.module).expect("markup comments are represented");
    assert!(formatted.contains("<!--note-->"));

    let core_comment = parse_clean(
        "core-comment.uhura",
        "use uhura::ui;\nui AppWeb for App(view) {<p>{// why }\nview.label}</p>}\n",
    );
    let FormatError::UnsupportedComments { comments } =
        format(&core_comment.module).expect_err("embedded core comments cannot be erased");
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].text, "// why }");
}
