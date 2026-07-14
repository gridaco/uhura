use uhura_base::FileId;
use uhura_syntax::ast::{DocForm, MarkupCommentKind, Node, TextRun};
use uhura_syntax::{
    CommentKind, Cursor, Parsed, SourceKind, format_examples, format_module, parse,
};

fn module(src: &str) -> (Box<uhura_syntax::ast::File>, Vec<uhura_base::Diagnostic>) {
    let output = parse(FileId(0), src, SourceKind::Module);
    let Parsed::Module(file) = output.parsed else {
        panic!("expected module")
    };
    (file, output.diagnostics)
}

fn formatted_module(src: &str) -> String {
    let (file, diagnostics) = module(src);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    format_module(&file)
}

#[test]
fn dsl_comment_sigils_classify_exactly() {
    let mut cursor = Cursor::new(
        FileId(0),
        "// ordinary\n// @todo ordinary\n//// divider\n///// divider\n/// outer\n//! inner\npage",
    );
    let token = cursor.dsl_token();
    let kinds: Vec<_> = token.leading.iter().map(|comment| comment.kind).collect();
    assert_eq!(
        kinds,
        vec![
            CommentKind::Ordinary,
            CommentKind::Ordinary,
            CommentKind::Ordinary,
            CommentKind::Ordinary,
            CommentKind::OuterDoc,
            CommentKind::InnerDoc,
        ]
    );
    assert_eq!(token.leading[2].text, "// divider");
    assert_eq!(token.leading[4].text, " outer");
    assert_eq!(token.leading[5].text, " inner");
}

#[test]
fn declaration_docs_normalize_and_attach_at_closed_targets() {
    let src = "//! Module docs.   \n\
               /// Page docs.\n\
               page\n\n\
               props {\n\
                 /// First line.   \n\
                 // retained between doc lines\n\
                 /// Second line.\n\
                 title: text\n\
               }\n\n\
               <view />\n";
    let (file, diagnostics) = module(src);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    assert_eq!(file.preamble.docs.len(), 2);
    assert_eq!(file.preamble.docs[0].form, DocForm::Inner);
    assert_eq!(file.preamble.docs[0].text, "Module docs.");
    assert_eq!(file.preamble.docs[1].form, DocForm::Outer);
    assert_eq!(
        file.props[0].leading.docs[0].text,
        "First line.\nSecond line."
    );

    let formatted = format_module(&file);
    assert!(formatted.starts_with("//! Module docs.\n/// Page docs.\npage\n"));
    assert!(formatted.contains(
        "  /// First line.\n  // retained between doc lines\n  /// Second line.\n  title: text"
    ));
    assert_eq!(formatted_module(&formatted), formatted);
}

#[test]
fn metadata_and_target_spans_are_half_open_and_precise() {
    let source = "page\n/// First.\n// transparent\n/// Second.\nstore {}\n<view>\n  <!-- @doc Local. -->\n  <button />\n</view>\n";
    let (file, diagnostics) = module(source);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let store = file.store.as_ref().expect("store");
    let doc = &store.leading.docs[0];
    let doc_start = source.find("/// First.").unwrap() as u32;
    let doc_end = (source.find("/// Second.").unwrap() + "/// Second.".len()) as u32;
    assert_eq!((doc.span.start, doc.span.end), (doc_start, doc_end));
    assert_eq!(doc.text, "First.\nSecond.");
    let store_start = source.find("store {}").unwrap() as u32;
    assert_eq!(
        (store.span.start, store.span.end),
        (store_start, store_start + "store {}".len() as u32)
    );

    let Node::Element(root) = &file.markup[0] else {
        panic!()
    };
    let Node::Element(button) = &root.children[0] else {
        panic!()
    };
    let annotation = &button.annotations[0];
    let annotation_start = source.find("<!-- @doc Local. -->").unwrap() as u32;
    assert_eq!(annotation.span.start, annotation_start);
    assert_eq!(
        annotation.span.end,
        annotation_start + "<!-- @doc Local. -->".len() as u32
    );
}

#[test]
fn crlf_and_bare_cr_doc_lines_normalize_to_lf() {
    let source = "/// One.\r\n/// Two.\rpage\n<view />\n";
    let (file, diagnostics) = module(source);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    assert_eq!(file.preamble.docs[0].text, "One.\nTwo.");
    assert_eq!(
        format_module(&file),
        "/// One.\n/// Two.\npage\n\n<view />\n"
    );
}

#[test]
fn empty_doc_runs_are_noops_but_remain_run_boundaries() {
    let src = "page\nprops {\n  ///   \n  // ordinary survives\n  title: text\n}\n<view />\n";
    let (file, diagnostics) = module(src);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    assert!(file.props[0].leading.docs.is_empty());
    let formatted = format_module(&file);
    assert!(!formatted.contains("///"));
    assert!(formatted.contains("// ordinary survives"));
}

#[test]
fn metadata_placement_diagnostics_have_rfc_precedence() {
    let cases = [
        ("page\n//! too late\nstore {}\n<view />\n", "UH0018"),
        (
            "page\n/// cannot document an import\nuse component x\n<view />\n",
            "UH0019",
        ),
        ("page\n/// no target\n<view />\n", "UH0017"),
        (
            "page\nprops { title: // token interior\n text }\n<view />\n",
            "UH0001",
        ),
    ];
    for (source, expected) in cases {
        let (_, diagnostics) = module(source);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == expected),
            "expected {expected}: {diagnostics:#?}"
        );
    }
}

#[test]
fn parameter_trivia_forces_canonical_multiline_signatures() {
    let src = "page\nemits { changed(/// value docs\nvalue: text, // next param\nnext: bool) }\n\
               store { on changed(/// handler value\nvalue: text) {} }\n<view />\n";
    let formatted = formatted_module(src);
    assert!(formatted.contains(
        "  changed(\n    /// value docs\n    value: text,\n    // next param\n    next: bool\n  )"
    ));
    assert!(formatted.contains("  on changed(\n    /// handler value\n    value: text\n  ) {"));
    assert_eq!(formatted_module(&formatted), formatted);
}

#[test]
fn trailing_dsl_trivia_and_empty_grouping_bodies_survive() {
    let src = "page\n// before props\nprops { // empty props\n}\n\
               emits { // empty emits\n}\n\
               store { // before state\nstate { // empty state\n}\n// end store\n}\n\
               // before markup\n<view />\n";
    let formatted = formatted_module(src);
    for expected in [
        "// before props",
        "// empty props",
        "// empty emits",
        "// before state",
        "// empty state",
        "// end store",
        "// before markup",
    ] {
        assert!(
            formatted.contains(expected),
            "missing {expected}:\n{formatted}"
        );
    }
    assert_eq!(formatted_module(&formatted), formatted);
}

#[test]
fn annotations_attach_in_order_without_becoming_nodes() {
    let src = "page\n<view>\n\
                 <!-- @doc The primary action. -->\n\
                 <!-- ordinary, transparent trivia -->\n\
                 <!-- @review-note\n  Keep this reachable.  \n  It is the fallback.\n-->\n\
                 <button />\n\
               </view>\n";
    let (file, diagnostics) = module(src);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    assert_eq!(file.markup.len(), 1);
    let Node::Element(root) = &file.markup[0] else {
        panic!("expected root")
    };
    assert_eq!(root.children.len(), 1);
    assert_eq!(root.children.comments.len(), 3);
    let Node::Element(button) = &root.children[0] else {
        panic!("expected button")
    };
    assert_eq!(button.annotations.len(), 2);
    assert_eq!(button.annotations[0].kind, "doc");
    assert_eq!(button.annotations[0].text, "The primary action.");
    assert_eq!(button.annotations[1].kind, "review-note");
    assert_eq!(
        button.annotations[1].text,
        "Keep this reachable.\nIt is the fallback."
    );

    let formatted = format_module(&file);
    assert!(formatted.contains("<!-- @doc The primary action. -->"));
    assert!(formatted.contains("<!-- @review-note\n"));
    assert_eq!(formatted_module(&formatted), formatted);
}

#[test]
fn markup_comments_are_text_and_structure_inert() {
    let with_comment = formatted_module("page\n<text>Hello<!-- ordinary -->world</text>\n");
    let (file, diagnostics) = module(&with_comment);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let Node::Element(text) = &file.markup[0] else {
        panic!()
    };
    assert_eq!(text.children.len(), 1);
    let Node::Text { runs, .. } = &text.children[0] else {
        panic!()
    };
    assert_eq!(runs.len(), 1);
    assert!(
        matches!(&runs[0], TextRun::Literal(value) if value.split_whitespace().collect::<String>() == "Helloworld"),
        "{runs:#?}"
    );
    assert_eq!(text.children.comments.len(), 1);
    assert_eq!(formatted_module(&with_comment), with_comment);
}

#[test]
fn ordinary_markup_comments_survive_empty_and_trailing_lists() {
    let src = "page\n<view><!-- --></view>\n<!-- trailing -->\n";
    let formatted = formatted_module(src);
    assert!(formatted.contains("<view>\n  <!-- -->\n</view>"));
    assert!(formatted.contains("<!-- trailing -->"));
    assert_eq!(formatted_module(&formatted), formatted);
}

#[test]
fn malformed_markup_comments_use_uh0016_and_recover() {
    let cases = [
        "<!-- @Bad payload -->",
        "<!-- @annotation -->",
        "<!-- body -- inside -->",
        "<!-- body --->",
    ];
    for comment in cases {
        let source = format!("page\n<view>{comment}<button /></view>\n");
        let (file, diagnostics) = module(&source);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "UH0016"),
            "{source}: {diagnostics:#?}"
        );
        let Node::Element(root) = &file.markup[0] else {
            panic!()
        };
        assert_eq!(root.children.len(), 1, "recovery must retain the button");
    }

    let source = "page\n<view><!-- unterminated\n<button /></view>\n";
    let (file, diagnostics) = module(source);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "UH0016")
    );
    let Node::Element(root) = &file.markup[0] else {
        panic!()
    };
    assert_eq!(root.children.len(), 1);

    // An interpolation line is payload, not a safe sibling boundary. Recovery
    // continues to the following element without manufacturing an extra node.
    let source = "page\n<view><!-- unterminated\n{value}\n<button /></view>\n";
    let (file, diagnostics) = module(source);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "UH0016")
    );
    let Node::Element(root) = &file.markup[0] else {
        panic!()
    };
    assert_eq!(root.children.len(), 1);
    assert!(matches!(&root.children[0], Node::Element(element) if element.name == "button"));
}

#[test]
fn formatting_retains_a_malformed_annotation_marker() {
    let source = "page\n<view><!-- @Bad payload --><button /></view>\n";
    let (file, diagnostics) = module(source);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "UH0016")
    );

    let formatted = format_module(&file);
    assert!(formatted.contains("@Bad payload"), "{formatted}");
    let (_, reparsed) = module(&formatted);
    assert!(
        reparsed
            .iter()
            .any(|diagnostic| diagnostic.code == "UH0016"),
        "{reparsed:#?}"
    );
}

#[test]
fn xml_shaped_bytes_inside_style_stay_css_input() {
    let source =
        "page\n<view />\n<style>\n  view { content: \"<!-- @doc not metadata -->\"; }\n</style>\n";
    let (file, diagnostics) = module(source);
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| !matches!(diagnostic.code, "UH0016" | "UH0017" | "UH0019")),
        "{diagnostics:#?}"
    );
    assert!(file.markup.comments.is_empty());
}

#[test]
fn annotations_do_not_cross_text_or_scope_boundaries() {
    let (_, incompatible) =
        module("page\n<view><!-- @annotation note -->literal<button /></view>\n");
    assert!(
        incompatible
            .iter()
            .any(|diagnostic| diagnostic.code == "UH0019")
    );

    let (_, dangling) = module("page\n<view><button /><!-- @annotation no target --></view>\n");
    assert!(
        dangling
            .iter()
            .any(|diagnostic| diagnostic.code == "UH0017")
    );
}

#[test]
fn structural_blocks_are_annotation_targets_and_arm_comments_stay_scoped() {
    let source = "page\n<view>\n\
                    <!-- @rationale Choose one complete branch. -->\n\
                    {#if ready}\n\
                      <!-- then-arm trailing -->\n\
                    {:else}\n\
                      <button />\n\
                    {/if}\n\
                    {#match status}\n\
                      <!-- before first arm -->\n\
                      {:when ready}\n\
                        <text>Ready</text>\n\
                    {/match}\n\
                  </view>\n";
    let (file, diagnostics) = module(source);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let Node::Element(root) = &file.markup[0] else {
        panic!()
    };
    let Node::If {
        annotations, then, ..
    } = &root.children[0]
    else {
        panic!()
    };
    assert_eq!(annotations.len(), 1);
    assert_eq!(then.comments.len(), 1);
    let Node::Match { before_arms, .. } = &root.children[1] else {
        panic!()
    };
    assert_eq!(before_arms.comments.len(), 1);

    let formatted = format_module(&file);
    assert!(formatted.contains("<!-- then-arm trailing -->\n  {:else}"));
    assert!(formatted.contains("<!-- before first arm -->\n    {:when ready}"));
    assert_eq!(formatted_module(&formatted), formatted);
}

#[test]
fn xml_comments_outside_sibling_positions_use_uh0001() {
    let (_, diagnostics) = module("page\n<view <!-- ordinary --> />\n");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "UH0001")
    );
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "UH0016")
    );

    let (_, expression) = module("page\n<view hidden={true <!-- ordinary -->} />\n");
    assert!(
        expression
            .iter()
            .any(|diagnostic| diagnostic.code == "UH0001")
    );

    let (_, malformed) = module("page\n<view hidden={true <!-- @Bad note -->} />\n");
    assert!(
        malformed
            .iter()
            .any(|diagnostic| diagnostic.code == "UH0016")
    );
}

#[test]
fn examples_file_docs_example_docs_and_notes_stay_distinct() {
    let source = "//! Example source docs.\n/// The default example.\n\
                  example base default {\n  // clause trivia\n  note \"runtime note\"\n}\n";
    let output = parse(FileId(0), source, SourceKind::Examples);
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    let Parsed::Examples(file) = output.parsed else {
        panic!()
    };
    assert_eq!(file.examples[0].leading.docs.len(), 2);
    assert_eq!(file.examples[0].clauses.len(), 1);
    let formatted = format_examples(&file);
    assert!(formatted.contains("//! Example source docs."));
    assert!(formatted.contains("/// The default example."));
    assert!(formatted.contains("// clause trivia"));
    assert!(formatted.contains("note \"runtime note\""));
}

#[test]
fn list_layout_retains_comment_classification() {
    let (file, diagnostics) = module("page\n<!-- ordinary -->\n<view />\n");
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    assert!(matches!(
        file.markup.comments[0].comment.kind,
        MarkupCommentKind::Ordinary
    ));
}

#[test]
fn empty_doc_runs_do_not_change_canonical_layout() {
    let cases = [
        (
            "parameter",
            "page\nemits { changed(///   \nvalue: text) }\n<view />\n",
            "page\n\nemits {\n  changed(value: text)\n}\n\n<view />\n",
        ),
        ("module eof", "page\n///   \n", "page\n"),
    ];

    for (name, source, expected) in cases {
        let (file, diagnostics) = module(source);
        assert!(diagnostics.is_empty(), "{name}: {diagnostics:#?}");
        let formatted = format_module(&file);
        assert_eq!(formatted, expected, "{name}");
        assert_eq!(formatted_module(&formatted), formatted, "{name}");
    }
}

#[test]
fn malformed_markup_recovery_never_formats_into_valid_metadata() {
    let cases = [
        (
            "xml trailing dash",
            "page\n<view><!-- body ---><button /></view>\n",
        ),
        (
            "unterminated ordinary",
            "page\n<view><!-- unterminated\n<button /></view>\n",
        ),
        (
            "unterminated annotation",
            "page\n<view><!-- @doc missing close\n<button /></view>\n",
        ),
    ];

    for (name, source) in cases {
        let (file, diagnostics) = module(source);
        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "UH0016")
                .count(),
            1,
            "{name}: {diagnostics:#?}"
        );
        let formatted = format_module(&file);
        let (reparsed, diagnostics) = module(&formatted);
        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "UH0016")
                .count(),
            1,
            "{name}: malformed state was lost after formatting\n{formatted}\n{diagnostics:#?}"
        );
        let Node::Element(root) = &reparsed.markup[0] else {
            panic!("{name}: expected root element")
        };
        let button = root.children.iter().find_map(|node| match node {
            Node::Element(element) if element.name == "button" => Some(element),
            _ => None,
        });
        if let Some(button) = button {
            assert!(
                button.annotations.is_empty(),
                "{name}: malformed recovery promoted into metadata"
            );
        }
        assert_eq!(format_module(&reparsed), formatted, "{name}");
    }
}

#[test]
fn incompatible_annotations_stay_on_the_incompatible_side_of_recovery() {
    let source = "page\n<view>A<!-- @doc bad -->B<!-- @doc good --><button /></view>\n";
    let (file, diagnostics) = module(source);
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "UH0019")
            .count(),
        1,
        "{diagnostics:#?}"
    );
    let formatted = format_module(&file);
    let bad = formatted.find("<!-- @doc bad -->").unwrap();
    let text = formatted.find("AB").unwrap();
    let good = formatted.find("<!-- @doc good -->").unwrap();
    let button = formatted.find("<button />").unwrap();
    assert!(bad < text && text < good && good < button, "{formatted}");

    let (reparsed, diagnostics) = module(&formatted);
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "UH0019")
            .count(),
        1,
        "{diagnostics:#?}"
    );
    let Node::Element(root) = &reparsed.markup[0] else {
        panic!()
    };
    let Node::Element(button) = &root.children[1] else {
        panic!("expected button after the coalesced text")
    };
    assert_eq!(button.annotations.len(), 1);
    assert_eq!(button.annotations[0].text, "good");
    assert_eq!(format_module(&reparsed), formatted);

    let recovery_cases = [
        (
            "omitted error node",
            "page\n<view><!-- @doc bad --><!oops><button /></view>\n",
            "UH0001",
        ),
        (
            "raw close brace",
            "page\n<view><!-- @doc bad -->}<button /></view>\n",
            "UH0006",
        ),
    ];
    for (name, source, recovery_code) in recovery_cases {
        let (file, diagnostics) = module(source);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == recovery_code),
            "{name}: {diagnostics:#?}"
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "UH0019"),
            "{name}: pending annotation skipped recovery: {diagnostics:#?}"
        );
        let formatted = format_module(&file);
        let (reparsed, diagnostics) = module(&formatted);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "UH0016"),
            "{name}: rejected carrier became valid\n{formatted}\n{diagnostics:#?}"
        );
        let Node::Element(root) = &reparsed.markup[0] else {
            panic!("{name}: expected root")
        };
        let Node::Element(button) = &root.children[0] else {
            panic!("{name}: expected button")
        };
        assert!(button.annotations.is_empty(), "{name}");
        assert_eq!(format_module(&reparsed), formatted, "{name}");
    }
}

#[test]
fn style_boundary_is_exact_and_closing_tag_comments_are_atomic() {
    let source = "page\n<!-- @doc note --><style-guide />\n";
    let (file, diagnostics) = module(source);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    assert!(file.style.is_none());
    let Node::Element(element) = &file.markup[0] else {
        panic!()
    };
    assert_eq!(element.name, "style-guide");
    assert_eq!(element.annotations.len(), 1);
    assert_eq!(
        formatted_module(&format_module(&file)),
        format_module(&file)
    );

    let (style, diagnostics) = module("page\n<style>\n</style>\n");
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    assert!(style.style.is_some());

    let closing_cases = [
        (
            "ordinary",
            "page\n<view></view <!-- ordinary -->>\n",
            "UH0001",
        ),
        (
            "malformed marker",
            "page\n<view></view <!-- @Bad note -->>\n",
            "UH0016",
        ),
        (
            "invalid xml body",
            "page\n<view></view <!-- body --->>\n",
            "UH0016",
        ),
    ];
    for (name, source, expected) in closing_cases {
        let (_, diagnostics) = module(source);
        assert_eq!(diagnostics.len(), 1, "{name}: {diagnostics:#?}");
        assert_eq!(diagnostics[0].code, expected, "{name}: {diagnostics:#?}");
    }
}

#[test]
fn embedded_dsl_comment_carriers_receive_one_lexical_diagnostic() {
    let cases = [
        (
            "ordinary after expression",
            "page\n<view hidden={true <!-- ordinary -->} />\n",
            "UH0001",
        ),
        (
            "malformed after expression",
            "page\n<view hidden={true <!-- @Bad note -->} />\n",
            "UH0016",
        ),
        (
            "ordinary before expression",
            "page\n<view hidden={<!-- ordinary --> true} />\n",
            "UH0001",
        ),
        (
            "structural head",
            "page\n<view>{#if <!-- ordinary --> true}<button />{/if}</view>\n",
            "UH0001",
        ),
    ];

    for (name, source, expected) in cases {
        let (_, diagnostics) = module(source);
        assert_eq!(diagnostics.len(), 1, "{name}: {diagnostics:#?}");
        assert_eq!(diagnostics[0].code, expected, "{name}: {diagnostics:#?}");
    }
}
