//! The design doc's normative sources (§4.6, §4.7, §6.1) must parse with
//! zero diagnostics — the grammar is validated against the doc's own text
//! (plan risk #1 mitigation).

use uhura_base::FileId;
use uhura_syntax::ast::*;
use uhura_syntax::{Parsed, SourceKind, parse};

include!("common/normative_sources.rs");

fn assert_clean(diags: &[uhura_base::Diagnostic]) {
    assert!(
        diags.is_empty(),
        "expected zero diagnostics, got:\n{}",
        diags
            .iter()
            .map(|d| format!(
                "  [{}] {} @{}..{}",
                d.code, d.message, d.span.start, d.span.end
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn post_card_parses_clean() {
    let out = parse(FileId(0), POST_CARD, SourceKind::Module);
    assert_clean(&out.diagnostics);
    let Parsed::Module(f) = out.parsed else {
        panic!()
    };

    let DefKind::Component { name, .. } = &f.kind else {
        panic!("expected component")
    };
    assert_eq!(name, "post-card");
    assert_eq!(f.props.len(), 3);
    assert_eq!(f.emits.len(), 3);
    assert_eq!(f.uses.len(), 1);
    assert!(f.store.is_none());
    assert_eq!(f.markup.len(), 1, "component has exactly one root");

    let Node::Element(root) = &f.markup[0] else {
        panic!()
    };
    assert_eq!(root.name, "view");
    // match block with three arms sits among the children
    let match_node = root
        .children
        .iter()
        .find_map(|n| match n {
            Node::Match { arms, .. } => Some(arms),
            _ => None,
        })
        .expect("media match");
    assert_eq!(match_node.len(), 3);
    assert!(matches!(&match_node[0].pattern, MatchPattern::Variant(v) if v == "image"));
    assert_eq!(match_node[1].binding.as_deref(), Some("c"));

    let style = f.style.expect("style block");
    assert_eq!(style.rules.len(), 2);
    assert_eq!(style.rules[0].classes, vec!["post-card"]);
}

#[test]
fn feed_page_parses_clean() {
    let out = parse(FileId(0), FEED_STORE, SourceKind::Module);
    assert_clean(&out.diagnostics);
    let Parsed::Module(f) = out.parsed else {
        panic!()
    };

    assert!(matches!(f.kind, DefKind::Page { .. }));
    let store = f.store.expect("store");
    assert_eq!(store.state.len(), 4);
    assert_eq!(store.handlers.len(), 9);

    // Multi-handler + guard + outcome signatures.
    let h0 = &store.handlers[0];
    assert!(matches!(&h0.event, EventRef::Semantic { name, .. } if name == "like-toggled"));
    assert!(h0.guard.is_some());
    assert_eq!(h0.body.len(), 3);
    let h1 = &store.handlers[1];
    assert!(matches!(
        &h1.event,
        EventRef::Outcome { command, which: OutcomeKind::Ok, .. } if command == "like-post"
    ));
    // Outcome params are name-only.
    assert!(h1.params.iter().all(|p| p.ty.is_none()));

    // `send … as t` binding.
    let submit = &store.handlers[6];
    assert!(matches!(
        &submit.body[0],
        Stmt::Send { bind: Some(b), .. } if b == "t"
    ));
    // `navigate back`.
    let back = &store.handlers[8];
    assert!(matches!(
        &back.body[0],
        Stmt::Navigate {
            target: NavTarget::Back,
            ..
        }
    ));

    // Markup: forwarding event attrs on the component call.
    let Node::Element(root) = &f.markup[0] else {
        panic!()
    };
    fn find_element<'a>(nodes: &'a [Node], name: &str) -> Option<&'a Element> {
        for n in nodes {
            let kids: &[Node] = match n {
                Node::Element(e) => {
                    if e.name == name {
                        return Some(e);
                    }
                    &e.children
                }
                Node::If { then, .. } => then,
                Node::Each { body, .. } => body,
                Node::Match { arms, .. } => {
                    for a in arms {
                        if let Some(e) = find_element(&a.body, name) {
                            return Some(e);
                        }
                    }
                    &[]
                }
                _ => &[],
            };
            if let Some(e) = find_element(kids, name) {
                return Some(e);
            }
        }
        None
    }
    let card = find_element(&root.children, "post-card").expect("post-card call");
    assert_eq!(card.events.len(), 3);
    assert!(
        card.events
            .iter()
            .all(|e| matches!(e.binding, EventBinding::Forward))
    );
    assert!(card.self_closing);
}

#[test]
fn examples_file_parses_clean() {
    let out = parse(FileId(0), FEED_EXAMPLES, SourceKind::Examples);
    assert_clean(&out.diagnostics);
    let Parsed::Examples(ex) = out.parsed else {
        panic!()
    };

    assert_eq!(ex.examples.len(), 5);
    assert!(ex.examples[1].is_default);

    // Keyed projection pin.
    let comments_open = &ex.examples[3];
    assert!(comments_open.clauses.iter().any(|c| matches!(
        c,
        ExampleClause::Projection(p) if p.projection == "for-post" && p.key.is_some()
    )));

    // Timeline with all three entry kinds.
    let appended = &ex.examples[4];
    let events = appended
        .clauses
        .iter()
        .find_map(|c| match c {
            ExampleClause::Events { entries, .. } => Some(entries),
            _ => None,
        })
        .expect("events clause");
    assert_eq!(events.len(), 3);
    assert!(matches!(&events[0], ExampleEvent::Semantic { name, .. } if name == "feed-near-end"));
    assert!(matches!(&events[1], ExampleEvent::Projection(_)));
    assert!(matches!(
        &events[2],
        ExampleEvent::Outcome {
            which: OutcomeKind::Ok,
            ..
        }
    ));
}

#[test]
fn planted_errors_diagnose() {
    // Unkeyed each is a parse error (§4.4).
    let src = "component x\n<view>{#each xs as x}<text>{x}</text>{/each}</view>\n";
    let out = parse(FileId(0), src, SourceKind::Module);
    assert!(
        out.diagnostics.iter().any(|d| d.code == "UH0003"),
        "{:?}",
        out.diagnostics
    );

    // Unknown statement keyword.
    let src = "page\nstore { on x() { mutate y = 1 } }\n<view />\n";
    let out = parse(FileId(0), src, SourceKind::Module);
    assert!(out.diagnostics.iter().any(|d| d.code == "UH0001"));

    // Mismatched close tag.
    let src = "component x\n<view><text>hello</view>\n";
    let out = parse(FileId(0), src, SourceKind::Module);
    assert!(out.diagnostics.iter().any(|d| d.code == "UH0004"));
}
