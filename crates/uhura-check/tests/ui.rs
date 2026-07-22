//! Web UI profile conformance.

use std::collections::BTreeSet;

use uhura_check::{AuthoringEntryClass, AuthoringTargetClass, check_project_modules};
use uhura_core::{RenderNode, UiNode, Value, semantic_node_id};
use uhura_syntax::{Module, SourceIdentity, parse};

const MACHINE: &str = r#"
pub machine Counter {
  events {
    Increment,
    Changed(value: Text),
  }

  outcomes {
    commit Accepted,
  }

  state {
    count: Int = 0,
    query: Text = "",
    items: Seq<Int> = [1, 2],
    selected: Option<Text> = Some("chosen"),
  }

  observe {
    count,
    query,
    items,
    selected,
  }

  on Increment {
    count = count + 1;
    Accepted
  }

  on Changed(value) {
    query = value;
    Accepted
  }
}
"#;

const UI: &str = r#"
use uhura::ui;
use crate::counter::Counter;

pub ui CounterWeb for Counter(view) {
  <main>
    {#if view.query == ""}
      <p>Empty</p>
    {:else}
      <output>{view.query}</output>
    {/if}

    {#each view.items as item (item)}
      <p>{item}</p>
    {/each}

    {#if view.selected is Some(value)}
      <p>{value}</p>
    {:else}
      <p>Nothing selected</p>
    {/if}

    <input value={view.query} on input -> Changed(event.value) />
    <button on press -> Increment>Increment</button>
  </main>
}
"#;

fn module(file: u32, logical: &str, source: &str) -> Module {
    let parsed = parse(
        SourceIdentity::new(file, "example.ui@1", logical, format!("{logical}.uhura")),
        source,
    );
    assert!(
        parsed.diagnostics.is_empty(),
        "parse diagnostics for {logical}:\n{:#?}",
        parsed.diagnostics
    );
    parsed.module
}

fn checked(machine: &str, ui: &str) -> uhura_check::CheckOutput {
    check_project_modules(&[module(1, "counter", machine), module(2, "web", ui)])
}

fn rendered_text(nodes: &[RenderNode], output: &mut String) {
    for node in nodes {
        match node {
            RenderNode::Text { text, .. } => output.push_str(text),
            RenderNode::Element { children, .. } => rendered_text(children, output),
        }
    }
}

fn event_binding(nodes: &[RenderNode], element: &str, event: &str) -> Option<String> {
    for node in nodes {
        if let RenderNode::Element {
            element: candidate,
            events,
            children,
            ..
        } = node
        {
            if candidate == element
                && let Some(binding) = events
                    .iter()
                    .find_map(|value| (value.event == event).then(|| value.binding.clone()))
            {
                return Some(binding);
            }
            if let Some(binding) = event_binding(children, element, event) {
                return Some(binding);
            }
        }
    }
    None
}

#[test]
fn checked_annotations_join_stable_ui_targets_without_entering_program_ir() {
    const ANNOTATED_UI: &str = r#"
use uhura::ui;
use crate::counter::Counter;

pub ui CounterWeb for Counter(view) {
  <!-- @annotation The outer frame. -->
  <main>
    <!-- @rationale Empty query selects the placeholder. -->
    {#if view.query == ""}
      <p>Empty</p>
    {/if}

    <!-- @review-note Keep keyed rows stable. -->
    <!-- @todo Confirm the final row treatment. -->
    {#each view.items as item (item)}
      <p>{item}</p>
    {/each}
  </main>
}
"#;

    let baseline = checked(MACHINE, ANNOTATED_UI);
    assert!(
        baseline.diagnostics.is_empty(),
        "{:#?}",
        baseline.diagnostics
    );
    baseline
        .authoring
        .validate()
        .expect("valid authoring sidecar");
    assert_eq!(baseline.authoring.targets.len(), 3);
    assert_eq!(baseline.authoring.entries.len(), 4);

    let owner = "example.ui@1::CounterWeb";
    let main_id = semantic_node_id(owner, "root", "ui_element", "tree/0/element/main");
    let if_id = semantic_node_id(owner, "root", "ui_if", "tree/0/children/0/if");
    let each_id = semantic_node_id(owner, "root", "ui_each", "tree/0/children/1/each");
    let target = |id: &str| {
        baseline
            .authoring
            .targets
            .iter()
            .find(|target| target.id == id)
            .expect("semantic UI target")
    };
    assert_eq!(target(&main_id).class, AuthoringTargetClass::UiElement);
    assert_eq!(target(&if_id).class, AuthoringTargetClass::IfBlock);
    assert_eq!(target(&each_id).class, AuthoringTargetClass::EachBlock);
    assert_eq!(target(&main_id).file, "web.uhura");
    assert_eq!(target(&main_id).owner, owner);

    let each_entries = baseline
        .authoring
        .entries
        .iter()
        .filter(|entry| entry.target_id == each_id)
        .collect::<Vec<_>>();
    assert_eq!(each_entries.len(), 2);
    assert_eq!(each_entries[0].class, AuthoringEntryClass::Annotation);
    assert_eq!(each_entries[0].kind, "review-note");
    assert_eq!(each_entries[0].order, 0);
    assert_eq!(each_entries[1].kind, "todo");
    assert_eq!(each_entries[1].order, 1);

    let revised_text = ANNOTATED_UI.replace(
        "Keep keyed rows stable.",
        "Explain the keyed rows with completely different prose.",
    );
    let revised = checked(MACHINE, &revised_text);
    assert!(revised.diagnostics.is_empty(), "{:#?}", revised.diagnostics);
    let baseline_program = baseline.program.as_ref().expect("baseline program");
    let revised_program = revised.program.as_ref().expect("revised program");
    assert_eq!(
        baseline_program.machine_program.program_hashes,
        revised_program.machine_program.program_hashes
    );
    assert_eq!(
        baseline_program.presentation_hashes,
        revised_program.presentation_hashes
    );
    assert_eq!(
        baseline
            .authoring
            .targets
            .iter()
            .map(|target| (&target.id, target.class))
            .collect::<Vec<_>>(),
        revised
            .authoring
            .targets
            .iter()
            .map(|target| (&target.id, target.class))
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        baseline
            .authoring
            .entries
            .iter()
            .map(|entry| (&entry.id, &entry.target_id, entry.order))
            .collect::<Vec<_>>(),
        revised
            .authoring
            .entries
            .iter()
            .map(|entry| (&entry.id, &entry.target_id, entry.order))
            .collect::<Vec<_>>(),
    );
    assert!(
        revised
            .authoring
            .entries
            .iter()
            .any(|entry| entry.text.contains("completely different prose"))
    );
}

#[test]
fn private_ui_identity_ignores_comments_and_annotations_while_authoring_uses_lowered_ids() {
    fn private_ui(prefix: &str, contents: &str) -> String {
        format!(
            r#"
use uhura::ui;
use crate::counter::Counter;

ui CounterWeb for Counter(view) {{
{prefix}  <main>{contents}</main>
}}
"#
        )
    }

    let sources = [
        private_ui("", "Helloworld"),
        private_ui("  <!-- An ordinary source-only note. -->\n", "Helloworld"),
        private_ui("", "Hello<!-- inline ordinary note -->world"),
        private_ui(
            "  <!-- @annotation First annotation prose. -->\n",
            "Helloworld",
        ),
        private_ui(
            "  <!-- @annotation Completely different prose. -->\n  <!-- @todo A second annotation. -->\n",
            "Helloworld",
        ),
    ];
    let outputs = sources
        .iter()
        .map(|source| checked(MACHINE, source))
        .collect::<Vec<_>>();
    for output in &outputs {
        assert!(
            output.diagnostics.is_empty(),
            "private UI diagnostics:\n{:#?}",
            output.diagnostics
        );
    }

    let presentation_ids = outputs
        .iter()
        .map(|output| {
            output
                .program
                .as_ref()
                .expect("private UI program")
                .presentations
                .keys()
                .find(|id| id.ends_with("_CounterWeb"))
                .expect("lowered private presentation")
                .clone()
        })
        .collect::<Vec<_>>();
    assert!(
        presentation_ids[0].starts_with("example.ui@1::__uhura_private_"),
        "private UI must retain its lowered semantic identity: {}",
        presentation_ids[0]
    );
    assert!(
        presentation_ids
            .iter()
            .all(|candidate| candidate == &presentation_ids[0]),
        "comment and annotation edits must not perturb private presentation IDs: {presentation_ids:#?}"
    );

    let baseline_program = outputs[0].program.as_ref().expect("baseline program");
    for output in &outputs[1..] {
        let program = output.program.as_ref().expect("revised program");
        assert_eq!(
            program.machine_program.program_hashes, baseline_program.machine_program.program_hashes,
            "source-only UI metadata must not perturb machine program IDs"
        );
        assert_eq!(
            program.presentation_hashes, baseline_program.presentation_hashes,
            "source-only UI metadata must not perturb presentation identity"
        );
    }

    assert!(outputs[0].authoring.entries.is_empty());
    assert!(outputs[1].authoring.entries.is_empty());
    assert!(outputs[2].authoring.entries.is_empty());
    assert_eq!(outputs[3].authoring.entries.len(), 1);
    assert_eq!(outputs[4].authoring.entries.len(), 2);

    let private_owner = &presentation_ids[0];
    let target_id = semantic_node_id(private_owner, "root", "ui_element", "tree/0/element/main");
    for output in &outputs[3..] {
        output
            .authoring
            .validate()
            .expect("private UI authoring sidecar");
        let target = output
            .authoring
            .targets
            .iter()
            .find(|target| target.id == target_id)
            .expect("annotation target uses the lowered private semantic ID");
        assert_eq!(&target.owner, private_owner);
        assert!(
            output
                .authoring
                .entries
                .iter()
                .all(|entry| entry.target_id == target_id)
        );

        let program = output.program.as_ref().expect("private UI program");
        let UiNode::Element { source, .. } = &program.presentations[private_owner].nodes[0] else {
            panic!("private presentation root must be the annotated main element")
        };
        assert_eq!(source.id, target_id);
        assert!(
            output
                .provenance
                .as_ref()
                .expect("private UI provenance")
                .occurrences
                .iter()
                .any(|occurrence| occurrence.node == target_id),
            "private annotation target must have authored provenance"
        );
    }
}

#[test]
fn direct_ui_profile_lowers_into_the_same_executable_program() {
    let output = checked(MACHINE, UI);
    assert!(
        output.diagnostics.is_empty(),
        "UI diagnostics:\n{:#?}",
        output.diagnostics
    );
    let program = output.program.expect("checked 0.4 program");
    program
        .validate_protocol()
        .expect("0.4 program plus presentation validates");
    let machine = "example.ui@1::Counter";
    let presentation_id = "example.ui@1::CounterWeb";
    let presentation = &program.presentations[presentation_id];
    assert_eq!(presentation.id, presentation_id);
    assert_eq!(presentation.machine, machine);
    assert_eq!(presentation.binding, "view");
    assert!(program.presentation_hashes.contains_key(presentation_id));
    assert!(
        presentation
            .nodes
            .iter()
            .any(|node| matches!(node, UiNode::Element { name, .. } if name == "main"))
    );

    let ui_start = UI.find("pub ui CounterWeb").expect("UI declaration") as u32;
    assert_eq!(presentation.source.start, ui_start);
    assert!(presentation.source.end > presentation.source.start);

    let (instance, _) = program
        .machine_program
        .admit(machine, Value::Unit, "ui")
        .expect("machine admission");
    let initial = program
        .project(&instance, presentation_id)
        .expect("initial projection from the checked Program");
    let mut initial_text = String::new();
    rendered_text(&initial.document.nodes, &mut initial_text);
    assert!(initial_text.contains("Empty"));
    assert!(initial_text.contains('1'));
    assert!(initial_text.contains('2'));
    assert!(initial_text.contains("chosen"));
    assert!(!initial_text.contains("Nothing selected"));

    let binding =
        event_binding(&initial.document.nodes, "input", "input").expect("checked input event edge");
    let input = program
        .resolve_ui_input(
            &instance,
            &initial,
            &binding,
            Value::Record(vec![("value".into(), Value::Text("hello".into()))]),
        )
        .expect("event payload constructs one semantic input");
    let Value::Variant {
        constructor,
        fields,
        ..
    } = &input
    else {
        panic!("expected local input constructor, found {input:?}");
    };
    assert_eq!(constructor, "Changed");
    assert_eq!(fields[0].1, Value::Text("hello".into()));

    let reacted = program
        .machine_program
        .react(&instance, input)
        .expect("checked UI input reacts");
    let changed = program
        .project(&reacted.instance, presentation_id)
        .expect("changed projection");
    let mut changed_text = String::new();
    rendered_text(&changed.document.nodes, &mut changed_text);
    assert!(changed_text.contains("hello"));
    assert!(!changed_text.contains("Empty"));
}

#[test]
fn if_else_is_lowered_without_a_kernel_conditional_gap() {
    let output = checked(MACHINE, UI);
    let program = output.program.expect("checked UI");
    let presentation = &program.presentations["example.ui@1::CounterWeb"];

    fn count_conditionals(nodes: &[UiNode]) -> usize {
        nodes
            .iter()
            .map(|node| match node {
                UiNode::If { children, .. } => 1 + count_conditionals(children),
                UiNode::Element { children, .. } => count_conditionals(children),
                _ => 0,
            })
            .sum()
    }

    // Each authored if/else becomes two mutually exclusive pure kernel
    // conditionals. The second source-level if also proves that a positive
    // `is Some(value)` refinement remains available in its then branch.
    assert_eq!(count_conditionals(&presentation.nodes), 4);
}

#[test]
fn profile_activation_is_lexical_exact_and_not_transitive() {
    let cases = [
        (
            "missing",
            UI.replacen("use uhura::ui;\n", "", 1),
            "uhura-0.4/ui-without-direct-profile-use",
        ),
        (
            "aliased",
            UI.replacen("use uhura::ui;", "use uhura::ui as web;", 1),
            "uhura-0.4/aliased-ui-profile",
        ),
        (
            "public",
            UI.replacen("use uhura::ui;", "pub use uhura::ui;", 1),
            "uhura-0.4/reexported-ui-profile",
        ),
        (
            "duplicate",
            UI.replacen("use uhura::ui;", "use uhura::ui;\nuse uhura::ui;", 1),
            "uhura-0.4/duplicate-ui-profile-use",
        ),
    ];

    for (name, ui, rule) in cases {
        let output = checked(MACHINE, &ui);
        assert!(output.program.is_none(), "{name} must not admit a program");
        assert!(
            output.diagnostics.iter().any(|value| value.rule == rule),
            "{name}: {:#?}",
            output.diagnostics
        );
    }

    let activating_but_ui_free = module(3, "profile", "use uhura::ui;\n");
    let ui_without_use = module(4, "web", &UI.replacen("use uhura::ui;\n", "", 1));
    let output = check_project_modules(&[
        module(1, "counter", MACHINE),
        activating_but_ui_free,
        ui_without_use,
    ]);
    assert!(output.program.is_none());
    assert!(
        output
            .diagnostics
            .iter()
            .any(|value| value.rule == "uhura-0.4/ui-without-direct-profile-use")
    );

    let inert = check_project_modules(&[
        module(1, "counter", MACHINE),
        module(5, "profile", "use uhura::ui;\n"),
    ]);
    assert!(
        inert.diagnostics.is_empty(),
        "an unused exact profile use is inert: {:#?}",
        inert.diagnostics
    );
    assert!(inert.program.is_some());
}

#[test]
fn ui_event_payload_is_scoped_and_checked() {
    let wrong_field = UI.replace("event.value", "event.text");
    let output = checked(MACHINE, &wrong_field);
    assert!(output.program.is_none());
    assert!(
        output
            .diagnostics
            .iter()
            .any(|value| value.rule == "uhura/unknown-member")
    );

    let payloadless = UI.replace("on press -> Increment", "on press -> Changed(event.value)");
    let output = checked(MACHINE, &payloadless);
    assert!(output.program.is_none());
    assert!(
        output
            .diagnostics
            .iter()
            .any(|value| value.rule == "uhura/unknown-member")
    );

    let escaped = UI.replace("<main>", "<main><output>{event.value}</output>");
    let output = checked(MACHINE, &escaped);
    assert!(output.program.is_none());
    assert!(
        output
            .diagnostics
            .iter()
            .any(|value| value.rule == "uhura/unknown-name")
    );
}

#[test]
fn every_ui_module_requires_its_own_direct_activation() {
    let secondary = r#"
use crate::counter::Counter;

pub ui CompactCounterWeb for Counter(view) {
  <button on press -> Increment>{view.count}</button>
}
"#;
    let output = check_project_modules(&[
        module(1, "counter", MACHINE),
        module(2, "web", UI),
        module(3, "compact", secondary),
    ]);
    assert!(output.program.is_none());
    let diagnostic = output
        .diagnostics
        .iter()
        .find(|value| value.rule == "uhura-0.4/ui-without-direct-profile-use")
        .expect("module-local activation diagnostic");
    assert_eq!(diagnostic.span.file.0, 3);
}

#[test]
fn presentation_names_are_rejected_until_typed_ui_calls_exist() {
    let unknown_component = UI.replace("<main>", "<main><UnknownCard />");
    let output = checked(MACHINE, &unknown_component);
    assert!(output.program.is_none());
    assert!(
        output
            .diagnostics
            .iter()
            .any(|value| value.rule == "uhura/unknown-ui-element")
    );

    let card = r#"
use uhura::ui;
use crate::counter::Counter;

pub ui CounterCard for Counter(view) {
  <p>{view.count}</p>
}
"#;
    let screen = r#"
use uhura::ui;
use crate::counter::Counter;
use crate::card::CounterCard as Card;

pub ui CardScreen for Counter(view) {
  <main><Card count={view.count} /></main>
}
"#;
    let output = check_project_modules(&[
        module(1, "counter", MACHINE),
        module(2, "card", card),
        module(3, "screen", screen),
    ]);
    assert!(output.program.is_none());
    let diagnostic = output
        .diagnostics
        .iter()
        .find(|value| value.rule == "uhura/ui-presentation-invocation-unavailable")
        .unwrap_or_else(|| {
            panic!(
                "missing unavailable presentation-call diagnostic: {:#?}",
                output.diagnostics
            )
        });
    assert_eq!(diagnostic.span.file.0, 3);
    assert!(diagnostic.message.contains("presentation invocation"));
    assert!(
        !output
            .diagnostics
            .iter()
            .any(|value| value.rule == "uhura/unknown-ui-element"),
        "a resolved presentation should receive the precise boundary diagnostic"
    );
}

#[test]
fn ui_catalog_is_closed_and_scroll_position_requires_a_ratio() {
    let valid = UI.replace(
        "<main>",
        r#"<main><scroll direction="horizontal" position={0.5}><p>Midway</p></scroll>"#,
    );
    let output = checked(MACHINE, &valid);
    assert!(
        output.diagnostics.is_empty(),
        "normalized scroll position diagnostics: {:#?}",
        output.diagnostics
    );
    assert!(output.program.is_some());

    for (name, ui, rule) in [
        (
            "quoted position",
            valid.replace("position={0.5}", "position=\"0.5\""),
            "uhura/ui-attribute-type",
        ),
        (
            "out-of-range position",
            valid.replace("position={0.5}", "position={1.1}"),
            "uhura/number-refinement",
        ),
        (
            "unknown scroll attribute",
            valid.replace("position={0.5}", "offset={0.5}"),
            "uhura/invalid-ui-attribute",
        ),
        (
            "invalid direction token",
            valid.replace("direction=\"horizontal\"", "direction=\"diagonal\""),
            "uhura/ui-attribute-value",
        ),
        (
            "void icon children",
            valid.replace(
                "<p>Midway</p>",
                r#"<icon name="heart"><text>invalid</text></icon>"#,
            ),
            "uhura/ui-void-children",
        ),
        (
            "unnamed icon button",
            valid.replace("<p>Midway</p>", r#"<button><icon name="heart"/></button>"#),
            "uhura/ui-accessible-name",
        ),
        (
            "nested interactive control",
            valid.replace(
                "<p>Midway</p>",
                r#"<button label="Outer"><button label="Inner">Inner</button></button>"#,
            ),
            "uhura/ui-nested-interactive",
        ),
        (
            "semantic list child without item boundary",
            valid.replace(
                "<p>Midway</p>",
                r#"<view role="list"><region label="Open">Profile</region></view>"#,
            ),
            "uhura/ui-list-item-boundary",
        ),
        (
            "inert interactive region",
            valid.replace("<p>Midway</p>", r#"<region label="Open">Profile</region>"#),
            "uhura/ui-missing-event",
        ),
    ] {
        let output = checked(MACHINE, &ui);
        assert!(output.program.is_none(), "{name} must not admit a program");
        assert!(
            output.diagnostics.iter().any(|value| value.rule == rule),
            "{name}: {:#?}",
            output.diagnostics
        );
    }
}

#[test]
fn presentation_identity_is_separate_from_machine_behavior_and_source_layout() {
    let baseline = checked(MACHINE, UI).program.expect("baseline UI");

    let changed_ui = UI.replace("<p>Empty</p>", "<p>No query</p>");
    let changed = checked(MACHINE, &changed_ui)
        .program
        .expect("changed presentation");
    assert_eq!(
        baseline.machine_program.program_hashes,
        changed.machine_program.program_hashes
    );
    assert_ne!(baseline.presentation_hashes, changed.presentation_hashes);

    let moved_ui = UI.replace("crate::counter", "crate::domain");
    let moved = check_project_modules(&[
        module(91, "domain", MACHINE),
        module(92, "screen", &moved_ui),
    ])
    .program
    .expect("source-layout move");
    assert_eq!(
        baseline.presentations.keys().collect::<Vec<_>>(),
        moved.presentations.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        baseline.machine_program.program_hashes,
        moved.machine_program.program_hashes
    );
    assert_eq!(baseline.presentation_hashes, moved.presentation_hashes);
}

#[test]
fn unrelated_earlier_declaration_does_not_shift_presentation_or_ui_target_ids() {
    fn source_ids(nodes: &[UiNode], output: &mut BTreeSet<String>) {
        for node in nodes {
            match node {
                UiNode::Text { source, .. } | UiNode::Interpolation { source, .. } => {
                    output.insert(source.id.clone());
                }
                UiNode::Element {
                    attributes,
                    children,
                    source,
                    ..
                } => {
                    output.insert(source.id.clone());
                    output.extend(
                        attributes
                            .iter()
                            .map(|attribute| attribute.source.id.clone()),
                    );
                    source_ids(children, output);
                }
                UiNode::If {
                    children, source, ..
                }
                | UiNode::Each {
                    children, source, ..
                } => {
                    output.insert(source.id.clone());
                    source_ids(children, output);
                }
                UiNode::Match { cases, source, .. } => {
                    output.insert(source.id.clone());
                    for case in cases {
                        output.insert(case.source.id.clone());
                        source_ids(&case.children, output);
                    }
                }
            }
        }
    }

    let earlier = UI.replace(
        "pub ui CounterWeb",
        "pub const ALPHABETICALLY_FIRST: Int = 7;\n\npub ui CounterWeb",
    );
    let baseline = checked(MACHINE, UI);
    let shifted = checked(MACHINE, &earlier);
    assert!(
        baseline.diagnostics.is_empty() && shifted.diagnostics.is_empty(),
        "baseline: {:#?}\nshifted: {:#?}",
        baseline.diagnostics,
        shifted.diagnostics,
    );
    let baseline_provenance = baseline.provenance.expect("baseline provenance");
    let shifted_provenance = shifted.provenance.expect("shifted provenance");
    let baseline = baseline.program.expect("baseline program");
    let shifted = shifted
        .program
        .expect("program with an earlier declaration");
    let presentation_id = "example.ui@1::CounterWeb";
    assert_eq!(
        baseline.presentation_hashes[presentation_id],
        shifted.presentation_hashes[presentation_id],
    );

    let mut baseline_sources = BTreeSet::new();
    let mut shifted_sources = BTreeSet::new();
    source_ids(
        &baseline.presentations[presentation_id].nodes,
        &mut baseline_sources,
    );
    source_ids(
        &shifted.presentations[presentation_id].nodes,
        &mut shifted_sources,
    );
    assert_eq!(baseline_sources, shifted_sources);
    assert!(baseline_sources.contains(&semantic_node_id(
        presentation_id,
        "root",
        "ui_element",
        "tree/0/element/main",
    )));
    for source in &baseline_sources {
        assert!(
            baseline_provenance
                .occurrences
                .iter()
                .any(|occurrence| occurrence.node == *source),
            "baseline UI target `{source}` must join provenance",
        );
        assert!(
            shifted_provenance
                .occurrences
                .iter()
                .any(|occurrence| occurrence.node == *source),
            "shifted UI target `{source}` must join provenance",
        );
    }

    let machine_id = "example.ui@1::Counter";
    let (baseline_instance, _) = baseline
        .machine_program
        .admit(machine_id, Value::Unit, "ui/stable-baseline")
        .expect("baseline admission");
    let (shifted_instance, _) = shifted
        .machine_program
        .admit(machine_id, Value::Unit, "ui/stable-shifted")
        .expect("shifted admission");
    let baseline_projection = baseline
        .project(&baseline_instance, presentation_id)
        .expect("baseline projection");
    let shifted_projection = shifted
        .project(&shifted_instance, presentation_id)
        .expect("shifted projection");
    assert_eq!(
        baseline_projection
            .sources
            .nodes
            .iter()
            .map(|(key, source)| (key, &source.id))
            .collect::<Vec<_>>(),
        shifted_projection
            .sources
            .nodes
            .iter()
            .map(|(key, source)| (key, &source.id))
            .collect::<Vec<_>>(),
    );
}

#[test]
fn part_observation_paths_and_inputs_keep_their_source_hierarchy() {
    let part = r#"
pub part Counter {
  requires outcomes {
    commit Done,
  }

  events {
    Tick,
    Changed(value: Text),
  }

  state {
    count: Int = 0,
    query: Text = "",
  }

  observe {
    count,
    query,
  }

  on Tick {
    count = count + 1;
    Done
  }

  on Changed(value) {
    query = value;
    Done
  }
}
"#;
    let application = r#"
use crate::parts::Counter;

pub machine Composed {
  outcomes {
    commit Done,
  }

  state {}
  observe {}

  part counter = Counter();
}
"#;
    let ui = r#"
use uhura::ui;
use crate::application::Composed;

pub ui ComposedWeb for Composed(view) {
  <main>
    <button on press -> counter.Tick>{view.counter.count}</button>
    <input
      value={view.counter.query}
      on input -> counter.Changed(event.value)
    />
    <output>{view.counter.query}</output>
  </main>
}
"#;
    let output = check_project_modules(&[
        module(1, "parts", part),
        module(2, "application", application),
        module(3, "web", ui),
    ]);
    assert!(
        output.diagnostics.is_empty(),
        "part UI diagnostics: {:#?}",
        output.diagnostics
    );
    let program = output.program.expect("part-backed UI");
    let machine = "example.ui@1::Composed";
    let presentation = "example.ui@1::ComposedWeb";
    let (instance, _) = program
        .machine_program
        .admit(machine, Value::Unit, "ui/part")
        .expect("part machine admission");
    let initial = program
        .project(&instance, presentation)
        .expect("part observation projection");
    let mut text = String::new();
    rendered_text(&initial.document.nodes, &mut text);
    assert!(text.contains('0'));

    let binding =
        event_binding(&initial.document.nodes, "button", "press").expect("part input edge");
    let input = program
        .resolve_ui_input(&instance, &initial, &binding, Value::Unit)
        .expect("qualified part input");
    let Value::Variant { constructor, .. } = &input else {
        panic!("expected part input, found {input:?}");
    };
    assert_eq!(constructor, "counter.Tick");
    let reacted = program
        .machine_program
        .react(&instance, input)
        .expect("part input reacts");
    let changed = program
        .project(&reacted.instance, presentation)
        .expect("changed part observation");
    let mut text = String::new();
    rendered_text(&changed.document.nodes, &mut text);
    assert!(text.contains('1'));

    let binding = event_binding(&changed.document.nodes, "input", "input")
        .expect("payload-bearing part input edge");
    let input = program
        .resolve_ui_input(
            &reacted.instance,
            &changed,
            &binding,
            Value::Record(vec![("value".into(), Value::Text("hello".into()))]),
        )
        .expect("qualified payload-bearing part input");
    let Value::Variant { constructor, .. } = &input else {
        panic!("expected part input, found {input:?}");
    };
    assert_eq!(constructor, "counter.Changed");
    let reacted = program
        .machine_program
        .react(&reacted.instance, input)
        .expect("payload-bearing part input reacts");
    let changed = program
        .project(&reacted.instance, presentation)
        .expect("changed part text observation");
    let mut text = String::new();
    rendered_text(&changed.document.nodes, &mut text);
    assert!(text.contains("hello"));
}
