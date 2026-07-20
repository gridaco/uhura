use std::collections::{BTreeMap, BTreeSet};

use uhura_check::{build_v04_provenance, check_v04_project_modules};
use uhura_core::{
    InteractionGraphEdgeKind, InteractionGraphNodeKind, OutcomePolicy, Provenance,
    build_interaction_graph_artifacts, merge_authored_interaction_topology, semantic_node_id,
    source_revision_id,
};
use uhura_syntax::v04::{Module, SourceIdentity, parse};

const SUPPORT: &str = r#"
pub struct Payload {
  value: Int,
}

pub enum Mode {
  Idle,
  Active,
}

pub key ItemId(Text);
pub const DEFAULT_VALUE: Int = 1;

pub fn bump(value: Int) -> Int {
  value + 1
}

pub part Counter(step: Int) {
  requires outcomes {
    commit Done,
  }

  events {
    Tick,
  }

  commands {
    Changed(value: Int),
  }

  state {
    count: Int = 0,
  }

  pub computed current: Int = count;
  invariant count >= 0;

  observe {
    count,
  }

  pub update reset() {
    count = 0;
  }

  on Tick {
    count = bump(count + step);
    emit Changed(count);
    Done
  }
}
"#;

const APPLICATION: &str = r#"
use crate::support::{Payload, Mode, ItemId, DEFAULT_VALUE, bump, Counter};

pub machine Application {
  config {
    step: Int,
  }

  require step > 0;
  const LOCAL: Int = 1;

  fn identity(value: Int) -> Int {
    value
  }

  part counter = Counter(step);

  events {
    Reset,
  }

  commands {
    Ready,
  }

  outcomes {
    commit Done,
    abort Refused,
  }

  state {
    mode: Mode = Mode::Idle,
    count: Int = DEFAULT_VALUE,
    items: Seq<Int> = [1, 2],
  }

  computed doubled: Int = bump(count);

  invariant {
    count >= 0,
    doubled >= 0,
  }

  observe {
    count,
    doubled,
    items,
  }

  on Reset {
    count = identity(0);
    emit Ready;
    Done
  }

  update clear() {
    count = 0;
  }

  before commit {}
}
"#;

const COMPOSED_TOPOLOGY: &str = r#"
pub part Counter {
  state {
    count: Int = 0,
  }

  pub computed current: Int = count;
  invariant count >= 0;

  observe {
    current,
  }

  pub update increment() {
    count = count + 1;
  }
}

pub part Controls(
  counter: Counter::Reads,
  counter_updates: Counter::Updates,
) {
  requires outcomes {
    commit Accepted,
  }

  events {
    Increment,
  }

  state {
    seen: Int = 0,
  }

  observe {
    current: counter.current,
  }

  on Increment {
    seen = counter.current;
    counter_updates.increment();
    Accepted
  }
}

pub machine Application {
  outcomes {
    commit Accepted,
    abort Refused,
  }

  part counter = Counter();
  part controls = Controls(counter.reads, counter.updates);

  events {
    Refresh,
  }

  state {
    source: Int = 1,
    mirror: Int = 0,
  }

  computed root_current: Int = source;
  invariant root_current >= 0;

  update capture(source: Int) {
    let snapshot = root_current;
    mirror = source;
  }

  on Refresh {
    mirror = root_current;
    Accepted
  }

  observe {
    current: counter.reads.current,
  }
}
"#;

#[test]
fn composed_topology_survives_as_a_source_sidecar_and_merges_into_one_runtime_graph() {
    let modules = [module(
        51,
        "application",
        "src/application.uhura",
        COMPOSED_TOPOLOGY,
    )];
    let program = checked(&modules);
    let provenance = build_v04_provenance(&modules).expect("validated authored topology");
    provenance.validate().expect("closed provenance contract");

    let mut graph = build_interaction_graph_artifacts(&program);
    merge_authored_interaction_topology(&mut graph, &provenance)
        .expect("authored topology projects over runtime graph");

    let machine = "example.provenance@1::Application";
    assert_eq!(
        graph
            .graph
            .nodes
            .iter()
            .filter(|node| node.machine == machine && node.kind == InteractionGraphNodeKind::Part)
            .map(|node| node.label.as_str())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["controls", "counter"]),
    );
    for kind in [
        InteractionGraphNodeKind::Module,
        InteractionGraphNodeKind::State,
        InteractionGraphNodeKind::Computed,
        InteractionGraphNodeKind::Invariant,
        InteractionGraphNodeKind::Update,
        InteractionGraphNodeKind::Observation,
    ] {
        assert!(
            graph
                .graph
                .nodes
                .iter()
                .any(|node| node.machine == machine && node.kind == kind),
            "missing authored {kind:?} node",
        );
    }
    let node_id = |kind, label: &str| {
        graph
            .graph
            .nodes
            .iter()
            .find(|node| node.machine == machine && node.kind == kind && node.label == label)
            .unwrap_or_else(|| panic!("missing {kind:?} node {label}"))
            .id
            .clone()
    };
    let has_edge = |from: &str, to: &str, kind| {
        graph
            .graph
            .edges
            .iter()
            .any(|edge| edge.from == from && edge.to == to && edge.kind == kind)
    };
    let counter_increment = node_id(InteractionGraphNodeKind::Update, "counter.increment");
    let counter_count = node_id(InteractionGraphNodeKind::State, "counter.count");
    assert!(has_edge(
        &counter_increment,
        &counter_count,
        InteractionGraphEdgeKind::Reads,
    ));

    let controls_increment = node_id(InteractionGraphNodeKind::Input, "controls.Increment");
    let counter_current = node_id(InteractionGraphNodeKind::Computed, "counter.current");
    assert!(has_edge(
        &controls_increment,
        &counter_current,
        InteractionGraphEdgeKind::Reads,
    ));
    assert!(has_edge(
        &controls_increment,
        &counter_increment,
        InteractionGraphEdgeKind::Calls,
    ));
    let controls_seen = node_id(InteractionGraphNodeKind::State, "controls.seen");
    assert!(
        !has_edge(
            &controls_increment,
            &controls_seen,
            InteractionGraphEdgeKind::Reads,
        ),
        "an assignment target is not a read",
    );

    let capture = node_id(InteractionGraphNodeKind::Update, "capture");
    let root_current = node_id(InteractionGraphNodeKind::Computed, "root_current");
    let source = node_id(InteractionGraphNodeKind::State, "source");
    let mirror = node_id(InteractionGraphNodeKind::State, "mirror");
    assert!(has_edge(
        &capture,
        &root_current,
        InteractionGraphEdgeKind::Reads,
    ));
    assert!(
        !has_edge(&capture, &source, InteractionGraphEdgeKind::Reads),
        "an update parameter must shadow a state field",
    );
    assert!(
        !has_edge(&capture, &mirror, InteractionGraphEdgeKind::Reads),
        "an assignment target is not a read",
    );

    let refresh = node_id(InteractionGraphNodeKind::Input, "Refresh");
    assert!(has_edge(
        &refresh,
        &root_current,
        InteractionGraphEdgeKind::Reads,
    ));
    assert!(!has_edge(
        &refresh,
        &mirror,
        InteractionGraphEdgeKind::Reads,
    ));

    let machine_node = node_id(InteractionGraphNodeKind::Machine, machine);
    let counter_part = node_id(InteractionGraphNodeKind::Part, "counter");
    let root_invariant = node_id(InteractionGraphNodeKind::Invariant, "invariant 1");
    let counter_invariant = node_id(InteractionGraphNodeKind::Invariant, "counter.invariant 1");
    assert!(has_edge(
        &machine_node,
        &root_invariant,
        InteractionGraphEdgeKind::Owns,
    ));
    assert!(has_edge(
        &counter_part,
        &counter_invariant,
        InteractionGraphEdgeKind::Owns,
    ));
    assert!(has_edge(
        &root_invariant,
        &root_current,
        InteractionGraphEdgeKind::Reads,
    ));
    assert!(has_edge(
        &counter_invariant,
        &counter_count,
        InteractionGraphEdgeKind::Reads,
    ));

    let accepted = node_id(InteractionGraphNodeKind::Outcome, "Accepted");
    let refused = node_id(InteractionGraphNodeKind::Outcome, "Refused");
    assert_eq!(
        graph.graph.outcome_policies,
        [
            (accepted, OutcomePolicy::Commit),
            (refused, OutcomePolicy::Abort),
        ]
        .into_iter()
        .collect(),
    );
    for kind in [
        InteractionGraphEdgeKind::Composes,
        InteractionGraphEdgeKind::Reads,
        InteractionGraphEdgeKind::Calls,
        InteractionGraphEdgeKind::Observes,
    ] {
        assert!(
            graph.graph.edges.iter().any(|edge| edge.kind == kind),
            "missing authored {kind:?} edge",
        );
    }
    assert_eq!(graph.provenance.nodes.len(), graph.graph.nodes.len());
    assert_eq!(graph.provenance.edges.len(), graph.graph.edges.len());
    assert!(
        graph
            .provenance
            .nodes
            .iter()
            .all(|entry| !entry.sources.is_empty()),
    );
    assert!(
        graph
            .provenance
            .edges
            .iter()
            .all(|entry| !entry.sources.is_empty()),
    );
}

const WEB: &str = r#"
use uhura::ui;
use crate::app::Application;

pub ui ApplicationWeb for Application(view) {
  <main>
    {#if view.count == 0}
      <p>Empty</p>
    {:else}
      <output>{view.doubled}</output>
    {/if}

    {#each view.items as item (item)}
      <p>{item}</p>
    {/each}

    <button disabled={false} on press -> Reset>Reset</button>
  </main>
}
"#;

fn module(file: u32, logical: &str, path: &str, source: &str) -> Module {
    let parsed = parse(
        SourceIdentity::new(file, "example.provenance@1", logical, path),
        source,
    );
    assert!(
        parsed.is_ok(),
        "parse diagnostics for {logical}:\n{:#?}",
        parsed.diagnostics
    );
    parsed.module
}

fn checked(modules: &[Module]) -> uhura_core::Program {
    let output = check_v04_project_modules(modules);
    assert!(
        output.diagnostics.is_empty(),
        "check diagnostics:\n{:#?}",
        output.diagnostics
    );
    output.program.expect("checked 0.4 program")
}

fn node_set(provenance: &Provenance) -> BTreeSet<&str> {
    provenance
        .occurrences
        .iter()
        .map(|occurrence| occurrence.node.as_str())
        .collect()
}

fn has_occurrence(provenance: &Provenance, node: &str, role: &str, owner: &str) -> bool {
    provenance.occurrences.iter().any(|occurrence| {
        occurrence.node == node && occurrence.role == role && occurrence.owner == owner
    })
}

#[test]
fn covers_public_declarations_machine_members_ui_imports_and_direct_parts() {
    let modules = [
        module(41, "support", "src/support.uhura", SUPPORT),
        module(42, "app", "src/app.uhura", APPLICATION),
        module(43, "web", "src/web.uhura", WEB),
    ];
    let program = checked(&modules);
    assert!(
        program
            .machines
            .contains_key("example.provenance@1::Application")
    );
    let provenance = build_v04_provenance(&modules).expect("validated provenance");
    provenance.validate().expect("closed provenance contract");

    let package = "example.provenance@1";
    for (name, kind) in [
        ("Payload", "struct"),
        ("Mode", "enum"),
        ("ItemId", "key"),
        ("DEFAULT_VALUE", "const"),
        ("bump", "function"),
        ("Counter", "part"),
        ("Application", "machine"),
        ("ApplicationWeb", "ui"),
    ] {
        let owner = format!("{package}::{name}");
        let node = semantic_node_id(&owner, "root", kind, &format!("declaration/{name}"));
        assert!(
            has_occurrence(&provenance, &node, "definition", "root"),
            "missing public {kind} declaration `{name}`"
        );
    }

    let machine = format!("{package}::Application");
    for (kind, path) in [
        ("config", "config/step"),
        ("event", "events/Reset"),
        ("command", "commands/Ready"),
        ("outcome", "outcomes/commit/Done"),
        ("state", "state/count"),
        ("computed", "computed/doubled"),
        ("observation", "observe/doubled"),
        ("handler", "handler/Reset/0"),
        ("update", "update/clear"),
        ("invariant", "invariant/0"),
        ("before_commit", "before_commit/0"),
    ] {
        let node = semantic_node_id(&machine, "root", kind, path);
        assert!(
            has_occurrence(&provenance, &node, "definition", "root"),
            "missing machine node {kind}:{path}"
        );
    }

    let generated_part_state = semantic_node_id(&machine, "counter", "state", "state/count");
    assert!(has_occurrence(
        &provenance,
        &generated_part_state,
        "generated",
        "counter"
    ));
    let generated_part_handler = semantic_node_id(&machine, "counter", "handler", "handler/Tick/0");
    assert!(has_occurrence(
        &provenance,
        &generated_part_handler,
        "generated",
        "counter"
    ));

    let ui = format!("{package}::ApplicationWeb");
    let element = semantic_node_id(&ui, "root", "ui_element", "tree/0/element/main");
    assert!(has_occurrence(&provenance, &element, "definition", "root"));
    let binding = semantic_node_id(
        &ui,
        "root",
        "ui_event_binding",
        "tree/0/children/3/event/press/0",
    );
    assert!(has_occurrence(&provenance, &binding, "definition", "root"));

    let reset = semantic_node_id(&machine, "root", "event", "events/Reset");
    assert!(
        provenance
            .occurrences
            .iter()
            .filter(|occurrence| occurrence.node == reset && occurrence.role == "reference")
            .count()
            >= 1,
        "UI input must reference the machine event node"
    );
    let counter_declaration = semantic_node_id(
        &format!("{package}::Counter"),
        "root",
        "part",
        "declaration/Counter",
    );
    assert!(
        provenance
            .occurrences
            .iter()
            .any(|occurrence| occurrence.node == counter_declaration
                && occurrence.role == "reference"),
        "the import and the part instance must reference the public part declaration"
    );

    assert_eq!(provenance.sources.len(), 3);
    for source in &provenance.sources {
        assert_eq!(source.sha256.len(), 64);
        assert_eq!(
            source.bytes,
            modules
                .iter()
                .find(|module| module.identity.path == source.path)
                .expect("captured source")
                .source
                .len() as u64
        );
    }
}

#[test]
fn merged_graph_sources_resolve_to_exact_machine_part_and_ui_files() {
    let modules = [
        module(41, "support", "src/support.uhura", SUPPORT),
        module(42, "app", "src/app.uhura", APPLICATION),
        module(43, "web", "src/web.uhura", WEB),
    ];
    let output = check_v04_project_modules(&modules);
    assert!(
        output.diagnostics.is_empty(),
        "check diagnostics:\n{:#?}",
        output.diagnostics
    );
    let program = output.program.expect("checked 0.4 program");
    let provenance = output.provenance.expect("checked 0.4 provenance");
    let mut graph = build_interaction_graph_artifacts(&program);
    merge_authored_interaction_topology(&mut graph, &provenance)
        .expect("authored topology projects over runtime graph");

    let inventory = provenance
        .sources
        .iter()
        .map(|source| (source.path.as_str(), source.bytes))
        .collect::<BTreeMap<_, _>>();
    let mut graph_paths = BTreeSet::new();
    assert_eq!(graph.provenance.nodes.len(), graph.graph.nodes.len());
    assert_eq!(graph.provenance.edges.len(), graph.graph.edges.len());
    for entry in graph
        .provenance
        .nodes
        .iter()
        .flat_map(|entry| entry.sources.iter())
        .chain(
            graph
                .provenance
                .edges
                .iter()
                .flat_map(|entry| entry.sources.iter()),
        )
    {
        let bytes = inventory.get(entry.path.as_str()).unwrap_or_else(|| {
            panic!(
                "graph source `{}` is absent from semantic provenance",
                entry.path
            )
        });
        assert!(entry.start <= entry.end);
        assert!(u64::from(entry.end) <= *bytes);
        graph_paths.insert(entry.path.as_str());
    }
    assert_eq!(
        graph_paths,
        BTreeSet::from(["src/app.uhura", "src/support.uhura", "src/web.uhura"])
    );
}

#[test]
fn grouped_invariant_fault_sites_join_each_condition_span() {
    let source = r#"
pub machine Counter {
  outcomes { commit Done }
  state { count: Int = 0 }
  invariant {
    count >= 0,
    count <= 10,
  }
  observe { count }
}
"#;
    let modules = [module(61, "counter", "src/counter.uhura", source)];
    let output = check_v04_project_modules(&modules);
    assert!(
        output.diagnostics.is_empty(),
        "grouped invariant diagnostics: {:#?}",
        output.diagnostics
    );
    let program = output.program.expect("checked grouped invariant");
    let provenance = output.provenance.expect("checked semantic provenance");
    let machine_id = "example.provenance@1::Counter";
    let machine = &program.machines[machine_id];
    assert_eq!(machine.invariants.len(), 2);

    for (ordinal, text) in ["count >= 0", "count <= 10"].into_iter().enumerate() {
        let start = source.find(text).expect("condition text") as u32;
        let end = start + text.len() as u32;
        let node = semantic_node_id(
            machine_id,
            "root",
            "invariant_condition",
            &format!("invariant/0/condition/{ordinal}"),
        );
        assert_eq!(machine.invariants[ordinal].1.id, node);
        assert_eq!(
            (
                machine.invariants[ordinal].1.start,
                machine.invariants[ordinal].1.end
            ),
            (start, end),
        );
        assert!(
            provenance.occurrences.iter().any(|occurrence| {
                occurrence.node == node
                    && occurrence.start == start
                    && occurrence.end == end
                    && occurrence.role == "definition"
            }),
            "condition {ordinal} must join its exact authored expression span",
        );
    }
}

const STABLE_BASE: &str = r#"use uhura::ui;

pub machine Counter {
  events { Increment }
  outcomes { commit Done }
  state { count: Int = 0 }
  observe { count }
  on Increment {
    count = count + 1;
    Done
  }
}

pub ui CounterWeb for Counter(view) {
  <button on press -> Increment>{view.count}</button>
}
"#;

const STABLE_EDITED: &str = r#"// This source-only comment and layout are deliberately nonsemantic.
use uhura::ui;


pub machine Counter {
  events {
    Increment,
  }

  outcomes {
    commit Done,
  }

  state {
    count: Int = 0,
  }

  observe {
    count,
  }

  on Increment {
    count = count + 1;
    Done
  }
}

pub ui CounterWeb for Counter(view) {
    <button on press -> Increment>{view.count}</button>
}
"#;

#[test]
fn file_module_comment_and_format_moves_preserve_nodes_but_change_provenance_revision() {
    let before_modules = [module(1, "counter", "counter.uhura", STABLE_BASE)];
    let after_modules = [module(
        99,
        "application::counter",
        "src/application/counter.uhura",
        STABLE_EDITED,
    )];
    let before_program = checked(&before_modules);
    let after_program = checked(&after_modules);
    assert_eq!(before_program.program_hashes, after_program.program_hashes);

    let before = build_v04_provenance(&before_modules).expect("before provenance");
    let after = build_v04_provenance(&after_modules).expect("after provenance");
    assert_eq!(node_set(&before), node_set(&after));
    assert_ne!(before, after);

    let before_revision =
        source_revision_id(false, [("counter.uhura", STABLE_BASE.as_bytes())]).unwrap();
    let after_revision = source_revision_id(
        false,
        [("src/application/counter.uhura", STABLE_EDITED.as_bytes())],
    )
    .unwrap();
    assert_ne!(before_revision, after_revision);
}

#[test]
fn rejects_out_of_bounds_and_non_utf8_boundary_occurrence_spans() {
    let mut outside = module(1, "counter", "counter.uhura", STABLE_BASE);
    let source_len = outside.source.len() as u32;
    let declaration = outside
        .declarations
        .first_mut()
        .expect("machine declaration");
    let uhura_syntax::v04::ast::DeclarationKind::Machine(machine) = &mut declaration.kind else {
        panic!("expected machine declaration")
    };
    machine.name.span.end = source_len + 1;
    let error = build_v04_provenance(&[outside]).expect_err("range must be rejected");
    assert!(error.message().contains("exceeds"));

    let unicode_source = STABLE_BASE.replacen("use uhura::ui;", "// é\nuse uhura::ui;", 1);
    let mut split = module(2, "counter", "unicode.uhura", &unicode_source);
    let declaration = split.declarations.first_mut().expect("machine declaration");
    let uhura_syntax::v04::ast::DeclarationKind::Machine(machine) = &mut declaration.kind else {
        panic!("expected machine declaration")
    };
    let codepoint = unicode_source.find('é').expect("unicode marker") as u32;
    machine.name.span.start = codepoint + 1;
    machine.name.span.end = codepoint + 2;
    let error = build_v04_provenance(&[split]).expect_err("UTF-8 split must be rejected");
    assert!(error.message().contains("UTF-8"));
}
