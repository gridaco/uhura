//! The tooling-neutral template realization trace stays observational while
//! covering flattening, reuse, zero anchors, and every semantic root slot.

use std::collections::BTreeMap;

use uhura_base::{Ident, Value};
use uhura_core::eval::{eval_fragment, eval_fragment_with_trace, eval_view, eval_view_with_trace};
use uhura_core::ir::{self, ProgramIr};
use uhura_core::state::{Counters, NavEntry, Projections, SurfaceState, UiState};
use uhura_core::template::{
    DefinitionAddress, DefinitionKind, EvaluationContext, EvaluationContextSegment,
    EvaluationOccurrence, RenderNodeRef, RenderRoot, TemplateAddress, TemplateSegment,
};

fn ident(value: &str) -> Ident {
    Ident::new(value).unwrap()
}

fn element(name: &str, ord: u32, children: Vec<ir::NodeIr>) -> ir::NodeIr {
    ir::NodeIr::Element(ir::ElementIr {
        element: ident(name),
        ord,
        class: None,
        props: vec![],
        events: vec![],
        text: vec![],
        children,
    })
}

fn definition(root: ir::NodeIr) -> ir::DefIr {
    ir::DefIr {
        modality: None,
        props: vec![],
        emits: vec![],
        params: vec![],
        state: BTreeMap::new(),
        events: BTreeMap::new(),
        handlers: vec![],
        root,
    }
}

fn traced_program() -> (ProgramIr, UiState) {
    let card = definition(element(
        "card",
        0,
        vec![ir::NodeIr::If {
            cond: ir::ExprIr::Bool(false),
            then: vec![element("text", 1, vec![])],
            els: vec![],
        }],
    ));

    let repeated = ir::NodeIr::Each(ir::EachIr {
        ord: 10,
        item: ident("item"),
        over: ir::OverIr::List,
        seq: ir::ExprIr::StateRef(ident("items")),
        key: ir::ExprIr::BindingRef(ident("item")),
        body: vec![ir::NodeIr::Component(ir::ComponentCallIr {
            component: ident("card"),
            ord: 11,
            props: vec![],
            emits: vec![],
        })],
    });
    let empty_each = ir::NodeIr::Each(ir::EachIr {
        ord: 12,
        item: ident("item"),
        over: ir::OverIr::List,
        seq: ir::ExprIr::StateRef(ident("empty")),
        key: ir::ExprIr::BindingRef(ident("item")),
        body: vec![element("text", 13, vec![])],
    });
    let empty_if = ir::NodeIr::If {
        cond: ir::ExprIr::Bool(false),
        then: vec![element("text", 14, vec![])],
        els: vec![],
    };
    let matched = ir::NodeIr::Match(ir::MatchIr {
        source: ir::MatchSourceIr::Union {
            value: ir::ExprIr::RecordLit(vec![ir::ArgIr {
                name: ident("ready"),
                value: ir::ExprIr::RecordLit(vec![]),
            }]),
        },
        arms: vec![
            ir::MatchArmIr {
                variant: Some(ident("ready")),
                binding: None,
                body: vec![element("text", 15, vec![]), element("text", 16, vec![])],
            },
            ir::MatchArmIr {
                variant: None,
                binding: None,
                body: vec![element("text", 17, vec![])],
            },
        ],
    });
    let page = definition(element(
        "view",
        0,
        vec![
            element("text", 1, vec![]),
            repeated,
            empty_each,
            empty_if,
            matched,
        ],
    ));
    let mut panel = definition(element("view", 0, vec![]));
    panel.modality = Some("sheet".into());

    let program = ProgramIr {
        protocol: ir::IR_PROTOCOL.into(),
        app: ident("trace-test"),
        entry: ident("home"),
        catalog: ir::CatalogPin {
            name: ident("base"),
            version: "0.1.0".into(),
            hash: "0".repeat(64),
        },
        ports: BTreeMap::new(),
        projections: BTreeMap::new(),
        element_events: BTreeMap::new(),
        element_props: BTreeMap::new(),
        routes: BTreeMap::from([(
            ident("home"),
            ir::RouteIr {
                segments: vec![],
                params: vec![],
            },
        )]),
        pages: BTreeMap::from([(ident("home"), page)]),
        components: BTreeMap::from([(ident("card"), card)]),
        surfaces: BTreeMap::from([(ident("panel"), panel)]),
    };

    let page_state = BTreeMap::from([
        (
            ident("items"),
            Value::List(vec![Value::Id("a".into()), Value::Id("b".into())]),
        ),
        (ident("empty"), Value::List(vec![])),
    ]);
    let state = UiState {
        rev: 7,
        nav: vec![NavEntry {
            serial: 1,
            route: ident("home"),
            params: BTreeMap::new(),
            state: page_state,
        }],
        surfaces: vec![
            SurfaceState {
                serial: 2,
                definition: ident("panel"),
                props: BTreeMap::new(),
                state: BTreeMap::new(),
                opener: "page:1".into(),
                restore_focus: None,
            },
            SurfaceState {
                serial: 3,
                definition: ident("panel"),
                props: BTreeMap::new(),
                state: BTreeMap::new(),
                opener: "page:1".into(),
                restore_focus: None,
            },
        ],
        pending: BTreeMap::new(),
        counters: Counters::default(),
    };
    (program, state)
}

fn occurrence<'a>(
    occurrences: &'a [EvaluationOccurrence],
    template: &TemplateAddress,
    context: &EvaluationContext,
) -> &'a EvaluationOccurrence {
    let matches = occurrences
        .iter()
        .filter(|occurrence| &occurrence.template == template && &occurrence.context == context)
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1, "expected one occurrence of {template:?}");
    matches[0]
}

fn node(root: RenderRoot, path: &[usize]) -> RenderNodeRef {
    RenderNodeRef {
        root,
        path: path.to_vec(),
    }
}

#[test]
fn traced_and_untraced_views_are_identical_and_trace_all_root_slots() {
    let (program, state) = traced_program();
    let projections = Projections::default();

    let plain = eval_view(&program, &state, &projections).unwrap();
    let (traced, trace) = eval_view_with_trace(&program, &state, &projections).unwrap();
    assert_eq!(traced, plain);
    assert_eq!(traced.to_canonical_string(), plain.to_canonical_string());
    assert_eq!(
        trace,
        eval_view_with_trace(&program, &state, &projections)
            .unwrap()
            .1,
        "the trace itself is deterministic"
    );

    let page = DefinitionAddress::new(DefinitionKind::Page, ident("home"));
    let page_root = TemplateAddress::root(page.clone());
    assert_eq!(
        occurrence(
            &trace.occurrences,
            &page_root,
            &EvaluationContext::default()
        )
        .anchors,
        vec![node(RenderRoot::Page, &[])]
    );

    let panel = TemplateAddress::root(DefinitionAddress::new(
        DefinitionKind::Surface,
        ident("panel"),
    ));
    let panel_occurrences = trace
        .occurrences
        .iter()
        .filter(|occurrence| occurrence.template == panel)
        .collect::<Vec<_>>();
    assert_eq!(panel_occurrences.len(), 2);
    assert_eq!(
        panel_occurrences[0].anchors,
        vec![node(
            RenderRoot::Surface {
                key: "panel:2".into()
            },
            &[]
        )]
    );
    assert_eq!(
        panel_occurrences[1].anchors,
        vec![node(
            RenderRoot::Surface {
                key: "panel:3".into()
            },
            &[]
        )]
    );
}

#[test]
fn blocks_and_reused_components_have_exact_cardinality_and_shared_anchors() {
    let (program, state) = traced_program();
    let trace = eval_view_with_trace(&program, &state, &Projections::default())
        .unwrap()
        .1;

    let page = DefinitionAddress::new(DefinitionKind::Page, ident("home"));
    let root = TemplateAddress::root(page);
    let repeated = root.child(TemplateSegment::ElementChild { index: 1 });
    let call = repeated.child(TemplateSegment::EachBody { index: 0 });
    assert_eq!(
        occurrence(&trace.occurrences, &repeated, &EvaluationContext::default()).anchors,
        vec![node(RenderRoot::Page, &[1]), node(RenderRoot::Page, &[2])]
    );

    let context_a = EvaluationContext {
        segments: vec![EvaluationContextSegment::EachItem {
            each: repeated.clone(),
            key: "a".into(),
        }],
    };
    let context_b = EvaluationContext {
        segments: vec![EvaluationContextSegment::EachItem {
            each: repeated.clone(),
            key: "b".into(),
        }],
    };
    assert_eq!(
        occurrence(&trace.occurrences, &call, &context_a).anchors,
        vec![node(RenderRoot::Page, &[1])]
    );
    assert_eq!(
        occurrence(&trace.occurrences, &call, &context_b).anchors,
        vec![node(RenderRoot::Page, &[2])]
    );

    let component_root = TemplateAddress::root(DefinitionAddress::new(
        DefinitionKind::Component,
        ident("card"),
    ));
    let component_context_a =
        context_a.child(EvaluationContextSegment::ComponentCall { call: call.clone() });
    let component_context_b =
        context_b.child(EvaluationContextSegment::ComponentCall { call: call.clone() });
    assert_eq!(
        occurrence(&trace.occurrences, &component_root, &component_context_a).anchors,
        vec![node(RenderRoot::Page, &[1])]
    );
    assert_eq!(
        occurrence(&trace.occurrences, &component_root, &component_context_b).anchors,
        vec![node(RenderRoot::Page, &[2])]
    );

    // The nested false if is evaluated once in each component instance even
    // though neither occurrence has a semantic node to anchor.
    let component_if = component_root.child(TemplateSegment::ElementChild { index: 0 });
    assert!(
        occurrence(&trace.occurrences, &component_if, &component_context_a)
            .anchors
            .is_empty()
    );
    assert!(
        occurrence(&trace.occurrences, &component_if, &component_context_b)
            .anchors
            .is_empty()
    );

    let empty_each = root.child(TemplateSegment::ElementChild { index: 2 });
    let empty_if = root.child(TemplateSegment::ElementChild { index: 3 });
    assert!(
        occurrence(
            &trace.occurrences,
            &empty_each,
            &EvaluationContext::default()
        )
        .anchors
        .is_empty()
    );
    assert!(
        occurrence(&trace.occurrences, &empty_if, &EvaluationContext::default())
            .anchors
            .is_empty()
    );

    let matched = root.child(TemplateSegment::ElementChild { index: 4 });
    assert_eq!(
        occurrence(&trace.occurrences, &matched, &EvaluationContext::default()).anchors,
        vec![node(RenderRoot::Page, &[3]), node(RenderRoot::Page, &[4])]
    );
    let inactive_match_child = matched.child(TemplateSegment::MatchArm { arm: 1, child: 0 });
    assert!(
        trace
            .occurrences
            .iter()
            .all(|occurrence| occurrence.template != inactive_match_child),
        "inactive descendants do not acquire synthetic occurrences"
    );
}

#[test]
fn fragment_trace_uses_the_explicit_definition_and_preserves_errors() {
    let (program, _) = traced_program();
    let card = program.components.get(&ident("card")).unwrap();
    let definition_address = DefinitionAddress::new(DefinitionKind::Component, ident("card"));
    let plain = eval_fragment(
        &program,
        card,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &Projections::default(),
    )
    .unwrap();
    let (traced, trace) = eval_fragment_with_trace(
        &program,
        &definition_address,
        card,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &Projections::default(),
    )
    .unwrap();
    assert_eq!(traced, plain);
    assert_eq!(trace.occurrences[0].template.definition, definition_address);
    assert_eq!(
        trace.occurrences[0].anchors,
        vec![node(RenderRoot::Fragment, &[])]
    );

    let invalid = definition(ir::NodeIr::If {
        cond: ir::ExprIr::Bool(false),
        then: vec![element("view", 0, vec![])],
        els: vec![],
    });
    let plain_error = eval_fragment(
        &program,
        &invalid,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &Projections::default(),
    )
    .unwrap_err();
    let traced_error = eval_fragment_with_trace(
        &program,
        &DefinitionAddress::new(DefinitionKind::Surface, ident("invalid")),
        &invalid,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &Projections::default(),
    )
    .unwrap_err();
    assert_eq!(traced_error, plain_error);
}
