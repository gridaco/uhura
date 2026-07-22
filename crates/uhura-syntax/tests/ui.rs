use uhura_syntax::ast::{DeclarationKind, UiAttribute, UiBinding, UiNameKind, UiNodeKind};
use uhura_syntax::{FormatError, ParseDiagnosticKind, SourceIdentity, TokenKind, format, parse};

const FEED: &str = include_str!("fixtures/feed-ui.uhura");

fn identity(path: &str) -> SourceIdentity {
    SourceIdentity::new(31, "examples.feed@1", "feed", path)
}

fn parse_clean(path: &str, source: &str) -> uhura_syntax::Parse {
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
    let UiBinding::Machine {
        machine,
        observation,
    } = &ui.binding
    else {
        panic!("expected a machine-bound UI declaration");
    };
    assert_eq!(machine.segments[0].name.text, "Feed");
    assert_eq!(observation.text, "view");
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
        |attribute| matches!(attribute, UiAttribute::Event { event, .. } if event.text == "Like")
    ));
}

#[test]
fn parses_and_formats_runtime_pure_ui_components() {
    let source = r#"use uhura::ui;

pub ui ProfileCard(user_id: UserId, display_name: Text) emits {
  OpenProfile(id: UserId),
  Dismiss,
} {
  <article data-user-id={user_id}>
    <Avatar image_url={avatar_url(user_id)} on LoadFailed -> AvatarFailed(user_id) />
    <button on click -> Dismiss>{display_name}</button>
  </article>
}
"#;
    let parsed = parse_clean("pure-component.uhura", source);
    assert_eq!(
        parsed
            .tokens
            .iter()
            .filter(|token| token.kind == TokenKind::UiBody)
            .count(),
        1,
    );
    let DeclarationKind::Ui(ui) = &parsed.module.declarations[0].kind else {
        panic!("expected UI declaration");
    };
    let UiBinding::Component { parameters, emits } = &ui.binding else {
        panic!("expected runtime-pure component declaration");
    };
    assert_eq!(
        parameters
            .iter()
            .map(|parameter| parameter.name.text.as_str())
            .collect::<Vec<_>>(),
        ["user_id", "display_name"],
    );
    assert_eq!(
        emits
            .variants
            .iter()
            .map(|variant| variant.name.text.as_str())
            .collect::<Vec<_>>(),
        ["OpenProfile", "Dismiss"],
    );

    let UiNodeKind::Element(article) = &ui.body.nodes[0].kind else {
        panic!("expected article root");
    };
    let avatar = article
        .children
        .iter()
        .find_map(|node| match &node.kind {
            UiNodeKind::Element(element) if element.name.text == "Avatar" => Some(element),
            _ => None,
        })
        .expect("component call");
    assert!(avatar.attributes.iter().any(
        |attribute| matches!(attribute, UiAttribute::Expression { name, .. } if name.text == "image_url")
    ));
    assert!(avatar.attributes.iter().any(
        |attribute| matches!(attribute, UiAttribute::Event { event, .. } if event.text == "LoadFailed" && event.kind == UiNameKind::Component)
    ));

    let formatted = format(&parsed.module).expect("pure component formats");
    assert!(formatted.contains(
        "pub ui ProfileCard(user_id: UserId, display_name: Text) emits {\n  OpenProfile(id: UserId),\n  Dismiss,\n} {"
    ));
    assert!(formatted.contains("image_url={avatar_url(user_id)}"));
    assert!(formatted.contains("on LoadFailed -> AvatarFailed(user_id)"));
    assert!(formatted.contains("<button on click -> Dismiss>"));
    let reparsed = parse_clean("pure-component.formatted.uhura", &formatted);
    assert_eq!(
        format(&reparsed.module).expect("formatted component remains formatable"),
        formatted,
    );
}

#[test]
fn pure_ui_components_may_omit_the_emitted_event_protocol() {
    let source = "pub ui Label(value: Text) {<span>{value}</span>}\n";
    let parsed = parse_clean("component-without-emits.uhura", source);
    let DeclarationKind::Ui(ui) = &parsed.module.declarations[0].kind else {
        panic!("expected UI declaration");
    };
    let UiBinding::Component { parameters, emits } = &ui.binding else {
        panic!("expected component declaration");
    };
    assert_eq!(parameters.len(), 1);
    assert!(emits.variants.is_empty());
    assert_eq!(
        format(&parsed.module).expect("component formats"),
        "pub ui Label(value: Text) {\n  <span>{value}</span>\n}\n",
    );
}

#[test]
fn component_and_native_attribute_name_grammars_remain_distinct() {
    let valid = r#"pub ui Row(value: Text) emits {
  Selected(value: Text),
} {
  <ItemRow item_value={value} on Selected -> Selected(value) />
  <button aria-label="Select" on press -> Selected(value) />
}
"#;
    parse_clean("attribute-domains.uhura", valid);

    let native_snake_case = parse(
        identity("native-snake-case.uhura"),
        "pub ui Row() {<button aria_label=\"Select\" />}\n",
    );
    assert!(native_snake_case.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == ParseDiagnosticKind::InvalidUi
            && diagnostic.message.contains("expected attribute name")
    }));

    let native_upper_event = parse(
        identity("native-upper-event.uhura"),
        "pub ui Row() {<button on Press -> Submit />}\n",
    );
    assert!(native_upper_event.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == ParseDiagnosticKind::InvalidUi
            && diagnostic.message.contains("expected semantic event name")
    }));

    parse_clean(
        "upper-tag-lower-event.uhura",
        "pub ui Row() {<Link routes={ROUTES} to={Location::Home} on follow -> Submit />}\n",
    );
}

#[test]
fn canonical_ui_format_is_parseable_and_idempotent() {
    let parsed = parse_clean("feed.uhura", FEED);
    let formatted = format(&parsed.module).expect("comment-free core expressions format");
    assert!(formatted.contains("pub ui FeedWeb for Feed(view) {"));
    assert!(formatted.contains("{#if view.loading}"));
    assert!(formatted.contains("{:else}"));
    assert!(formatted.contains("{#each view.posts as Post {"));
    assert!(formatted.contains("on Like -> ToggleLike(id)"));
    assert!(formatted.contains("featured"));
    assert!(formatted.contains("<!-- one stable keyed card -->"));
    assert!(formatted.contains(">Refresh</button>"));
    assert!(formatted.contains("<p>안녕하세요 {view.viewer_name}</p>"));

    let reparsed = parse_clean("feed.formatted.uhura", &formatted);
    let reformatted = format(&reparsed.module).expect("formatted UI must format again");
    assert_eq!(reformatted, formatted);
}

#[test]
fn literal_right_brace_in_element_text_survives_parse_and_format() {
    let source = r#"use uhura::ui;

ui AppWeb for App(view) {
  <p>Use } to close a block.</p>
}
"#;
    let parsed = parse_clean("literal-right-brace.uhura", source);
    let DeclarationKind::Ui(ui) = &parsed.module.declarations[0].kind else {
        panic!("expected UI declaration");
    };
    let UiNodeKind::Element(paragraph) = &ui.body.nodes[0].kind else {
        panic!("expected paragraph");
    };
    assert!(matches!(
        &paragraph.children[0].kind,
        UiNodeKind::Text(text) if text.raw == "Use } to close a block."
    ));

    let formatted = format(&parsed.module).expect("literal text formats");
    assert!(formatted.contains("<p>Use } to close a block.</p>"));
    let reparsed = parse_clean("literal-right-brace.formatted.uhura", &formatted);
    assert_eq!(
        format(&reparsed.module).expect("formatted literal text remains formatable"),
        formatted,
    );
}

#[test]
fn root_right_brace_requires_an_interpolation_escape() {
    let ambiguous = r#"use uhura::ui;

ui AppWeb for App(view) {
  literal } text
  <p>After the brace</p>
}
"#;
    let rejected = parse(identity("root-right-brace.uhura"), ambiguous);
    assert_eq!(rejected.source_from_tokens(), ambiguous);
    assert!(
        rejected.diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == ParseDiagnosticKind::InvalidUi
                && diagnostic.message.contains("render a literal right brace")
                && diagnostic.message.contains("as `{\"}\"}`")
        }),
        "{:#?}",
        rejected.diagnostics,
    );

    let escaped = r#"use uhura::ui;

ui AppWeb for App(view) {
  literal {"}"} text
  <p>After the brace</p>
}
"#;
    let parsed = parse_clean("escaped-root-right-brace.uhura", escaped);
    let formatted = format(&parsed.module).expect("escaped root text formats");
    assert!(formatted.contains("{\"}\"}"));
    let reparsed = parse_clean("escaped-root-right-brace.formatted.uhura", &formatted);
    assert_eq!(
        format(&reparsed.module).expect("escaped root text remains formatable"),
        formatted,
    );
}

#[test]
fn event_comparisons_do_not_close_the_surrounding_ui_tag() {
    let source = r#"use uhura::ui;

ui AppWeb for App(view) {
  <button on press -> Submit(view.count > 0) />
}

const AFTER: Text = "}";
"#;
    let parsed = parse_clean("event-comparison.uhura", source);
    assert_eq!(parsed.module.declarations.len(), 2);
    let DeclarationKind::Ui(ui) = &parsed.module.declarations[0].kind else {
        panic!("expected UI declaration");
    };
    let UiNodeKind::Element(button) = &ui.body.nodes[0].kind else {
        panic!("expected button");
    };
    assert!(matches!(
        &button.attributes[0],
        UiAttribute::Event { event, .. } if event.text == "press"
    ));
}

#[test]
fn declaration_typos_after_ui_keep_the_declaration_fix() {
    let source = r#"use uhura::ui;

ui AppWeb for App(view) {
  <p>Hello</p>
}

machin Other<T> {};
"#;
    let parsed = parse(identity("declaration-typo-after-ui.uhura"), source);
    let diagnostic = parsed
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.kind == ParseDiagnosticKind::InvalidDeclaration
                && diagnostic
                    .message
                    .contains("unknown module declaration `machin`")
        })
        .expect("declaration typo remains the primary diagnostic");
    assert_eq!(
        diagnostic.fix.as_ref().map(|fix| fix.insert.as_str()),
        Some("machine"),
    );
    assert!(
        parsed
            .diagnostics
            .iter()
            .all(|diagnostic| !diagnostic.message.contains("literal right brace")),
        "{:#?}",
        parsed.diagnostics,
    );
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
    assert!(formatted.contains("<!-- note -->"));

    let core_comment = parse_clean(
        "core-comment.uhura",
        "use uhura::ui;\nui AppWeb for App(view) {<p>{// why }\nview.label}</p>}\n",
    );
    let FormatError::UnsupportedComments { comments } =
        format(&core_comment.module).expect_err("embedded core comments cannot be erased");
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].text, "// why }");
}
