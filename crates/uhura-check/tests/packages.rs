use uhura_check::{CapturedPackageModules, check_package_graph_with_evidence};
use uhura_core::{
    InteractionGraphEdgeKind, InteractionGraphNodeKind, RenderNode, Statement, UiNode, Value,
    semantic_node_id,
};
use uhura_syntax::{Module, SourceIdentity, parse};

fn module(file: u32, package: &str, logical: &str, source: &str) -> Module {
    let parsed = parse(
        SourceIdentity::new(file, package, logical, format!("{logical}.uhura")),
        source,
    );
    assert!(
        parsed.is_ok(),
        "parse diagnostics for {package}/{logical}:\n{:#?}",
        parsed.diagnostics
    );
    parsed.module
}

fn package(
    package: &str,
    dependencies: &[(&str, &str)],
    modules: Vec<Module>,
) -> CapturedPackageModules {
    CapturedPackageModules {
        package: package.into(),
        dependencies: dependencies
            .iter()
            .map(|(alias, package)| ((*alias).into(), (*package).into()))
            .collect(),
        modules,
    }
}

fn annotated_ui_source(label: &str) -> String {
    format!(
        r#"
use uhura::ui;

pub machine App {{
  outcomes {{ commit Done }}
  state {{}}
  observe {{}}
}}

pub ui AppView for App(view) {{
  <!-- @annotation {label} -->
  <main>Ready</main>
}}
"#
    )
}

#[test]
fn package_graph_authoring_metadata_is_scoped_to_the_root_package() {
    let graph = vec![
        package(
            "example.root@1",
            &[("vendor", "vendor.ui@1")],
            vec![module(
                1,
                "example.root@1",
                "root_view",
                &annotated_ui_source("Root note."),
            )],
        ),
        package(
            "vendor.ui@1",
            &[],
            vec![module(
                2,
                "vendor.ui@1",
                "dependency_view",
                &annotated_ui_source("Dependency note."),
            )],
        ),
    ];

    let output = check_package_graph_with_evidence("example.root@1", &graph, &[]);
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    assert_eq!(output.authoring.targets.len(), 1);
    assert_eq!(output.authoring.entries.len(), 1);
    assert_eq!(output.authoring.targets[0].owner, "example.root@1::AppView");
    assert_eq!(output.authoring.targets[0].file, "root_view.uhura");
    assert_eq!(output.authoring.entries[0].text, "Root note.");
    assert_eq!(
        output
            .provenance
            .expect("whole-graph provenance")
            .sources
            .len(),
        2,
        "dependency provenance remains complete"
    );
}

#[test]
fn root_ui_can_compose_a_public_component_from_a_locked_dependency() {
    let root = module(
        3,
        "example.root@1",
        "application",
        r#"
use uhura::ui;
use shared::card::SharedCard;

pub machine App {
  outcomes { commit Done }
  state {}
  observe {}
}

pub ui Application for App(view) {
  <main><SharedCard label="Dependency component" /></main>
}
"#,
    );
    let dependency = module(
        4,
        "vendor.shared@1",
        "card",
        r#"
use uhura::ui;

pub ui SharedCard(label: Text) {
  <section>{label}</section>
}
"#,
    );
    let graph = vec![
        package(
            "example.root@1",
            &[("shared", "vendor.shared@1")],
            vec![root],
        ),
        package("vendor.shared@1", &[], vec![dependency]),
    ];

    let output = check_package_graph_with_evidence("example.root@1", &graph, &[]);
    assert!(
        output.diagnostics.is_empty(),
        "dependency component diagnostics:\n{:#?}",
        output.diagnostics
    );
    assert!(
        output
            .provenance
            .as_ref()
            .expect("whole-graph provenance")
            .sources
            .iter()
            .any(|source| source.path == "card.uhura"),
        "dependency component source remains navigable"
    );
    let program = output.program.expect("checked dependency component graph");
    assert!(
        program
            .components
            .contains_key("vendor.shared@1::SharedCard")
    );
    let UiNode::Element { children, .. } =
        &program.presentations["example.root@1::Application"].nodes[0]
    else {
        panic!("application root must be an element")
    };
    assert!(matches!(
        children.as_slice(),
        [UiNode::Call { target, .. }] if target == "vendor.shared@1::SharedCard"
    ));

    let (instance, _) = program
        .machine_program
        .admit("example.root@1::App", Value::Unit, "packages/dependency-ui")
        .expect("root machine admission");
    let projection = program
        .project(&instance, "example.root@1::Application")
        .expect("dependency component projection");
    let [
        RenderNode::Element {
            element: root,
            children,
            ..
        },
    ] = projection.document.nodes.as_slice()
    else {
        panic!("application projection must contain one root element")
    };
    assert_eq!(root, "main");
    assert!(matches!(
        children.as_slice(),
        [RenderNode::Element {
            element,
            children,
            ..
        }] if element == "section"
            && matches!(children.as_slice(), [RenderNode::Text { text, .. }] if text == "Dependency component")
    ));
}

fn graph(root_alias: &str, middle_alias: &str) -> Vec<CapturedPackageModules> {
    let root_source = format!(
        r#"
use {root_alias}::model::Boxed;

pub machine Probe {{
  config {{ initial: Boxed }}
  events {{ Reset }}
  outcomes {{ commit Done }}
  state {{ value: Boxed = initial }}
  observe {{ value }}
  on Reset {{
    value = initial;
    Done
  }}
}}
"#
    );
    let middle_source =
        format!("use {middle_alias}::values::Unit;\npub struct Boxed {{ value: Unit }}\n");
    vec![
        package(
            "example.root@1",
            &[(root_alias, "vendor.middle@1")],
            vec![module(1, "example.root@1", "program", &root_source)],
        ),
        package(
            "vendor.middle@1",
            &[(middle_alias, "vendor.base@1")],
            vec![module(2, "vendor.middle@1", "model", &middle_source)],
        ),
        package(
            "vendor.base@1",
            &[],
            vec![module(
                3,
                "vendor.base@1",
                "values",
                "pub enum Unit { One }\n",
            )],
        ),
    ]
}

#[test]
fn transitive_external_imports_link_by_exact_package_identity() {
    let output = check_package_graph_with_evidence("example.root@1", &graph("kit", "base"), &[]);
    assert!(
        output.diagnostics.is_empty(),
        "diagnostics:\n{:#?}",
        output.diagnostics
    );
    let program = output.program.expect("linked program");
    assert!(
        program
            .machine_program
            .machines
            .contains_key("example.root@1::Probe")
    );
    assert!(
        program
            .machine_program
            .types
            .contains_key("vendor.base@1::Unit")
    );
    assert!(
        program
            .machine_program
            .types
            .contains_key("vendor.middle@1::Boxed")
    );
}

#[test]
fn dependency_aliases_are_nonsemantic() {
    let ordinary = check_package_graph_with_evidence("example.root@1", &graph("kit", "base"), &[])
        .program
        .expect("ordinary graph");
    let renamed =
        check_package_graph_with_evidence("example.root@1", &graph("toolkit", "foundation"), &[])
            .program
            .expect("renamed aliases");
    assert_eq!(
        ordinary.machine_program.program_hashes,
        renamed.machine_program.program_hashes
    );
}

#[test]
fn unused_dependency_declarations_do_not_enter_machine_program_identity() {
    let mut ordinary_graph = graph("kit", "base");
    ordinary_graph[2].modules.push(module(
        4,
        "vendor.base@1",
        "unused",
        "pub struct Unused { value: Int }\n",
    ));
    let mut changed_graph = graph("kit", "base");
    changed_graph[2].modules.push(module(
        4,
        "vendor.base@1",
        "unused",
        "pub struct Unused { value: Text }\n",
    ));

    let ordinary = check_package_graph_with_evidence("example.root@1", &ordinary_graph, &[])
        .program
        .expect("ordinary graph");
    let changed = check_package_graph_with_evidence("example.root@1", &changed_graph, &[])
        .program
        .expect("graph with changed unused declaration");
    assert_eq!(
        ordinary
            .machine_program
            .program_hashes
            .get("example.root@1::Probe"),
        changed
            .machine_program
            .program_hashes
            .get("example.root@1::Probe"),
    );
}

#[test]
fn external_same_name_reexport_is_an_identity_preserving_locator() {
    let direct_root = package(
        "example.root@1",
        &[("vendor", "vendor.base@1")],
        vec![module(
            1,
            "example.root@1",
            "program",
            r#"
use vendor::values::Item;
pub machine Probe {
  config { initial: Item }
  events { Reset }
  outcomes { commit Done }
  state { value: Item = initial }
  observe { value }
  on Reset {
    value = initial;
    Done
  }
}
"#,
        )],
    );
    let reexport_root = package(
        "example.root@1",
        &[("vendor", "vendor.facade@1")],
        vec![module(
            1,
            "example.root@1",
            "program",
            r#"
use vendor::facade::Item;
pub machine Probe {
  config { initial: Item }
  events { Reset }
  outcomes { commit Done }
  state { value: Item = initial }
  observe { value }
  on Reset {
    value = initial;
    Done
  }
}
"#,
        )],
    );
    let facade = package(
        "vendor.facade@1",
        &[("base", "vendor.base@1")],
        vec![module(
            2,
            "vendor.facade@1",
            "facade",
            "pub use base::values::Item;\n",
        )],
    );
    let base = package(
        "vendor.base@1",
        &[],
        vec![module(
            3,
            "vendor.base@1",
            "values",
            "pub struct Item { value: Int }\n",
        )],
    );

    let direct =
        check_package_graph_with_evidence("example.root@1", &[direct_root, base.clone()], &[])
            .program
            .expect("direct import graph");
    let reexported =
        check_package_graph_with_evidence("example.root@1", &[reexport_root, facade, base], &[])
            .program
            .expect("re-export graph");
    assert_eq!(
        direct
            .machine_program
            .program_hashes
            .get("example.root@1::Probe"),
        reexported
            .machine_program
            .program_hashes
            .get("example.root@1::Probe"),
    );
    assert!(
        reexported
            .machine_program
            .types
            .contains_key("vendor.base@1::Item")
    );
    assert!(
        !reexported
            .machine_program
            .types
            .contains_key("vendor.facade@1::Item")
    );
}

#[test]
fn external_reexport_cannot_replace_a_package_public_name() {
    let root = package(
        "example.root@1",
        &[("vendor", "vendor.facade@1")],
        vec![module(
            1,
            "example.root@1",
            "program",
            "pub const ROOT: Int = 0;\n",
        )],
    );
    let facade = package(
        "vendor.facade@1",
        &[("base", "vendor.base@1")],
        vec![
            module(
                2,
                "vendor.facade@1",
                "model",
                "pub struct Item { local: Int }\n",
            ),
            module(
                3,
                "vendor.facade@1",
                "facade",
                "pub use base::values::Item;\n",
            ),
        ],
    );
    let base = package(
        "vendor.base@1",
        &[],
        vec![module(
            4,
            "vendor.base@1",
            "values",
            "pub struct Item { base: Int }\n",
        )],
    );

    let output = check_package_graph_with_evidence("example.root@1", &[root, facade, base], &[]);
    assert!(output.program.is_none());
    assert!(
        output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule == "uhura-0.4/reexport-collision"),
        "diagnostics:\n{:#?}",
        output.diagnostics
    );
}

#[test]
fn same_spelled_external_declarations_remain_distinct_under_local_aliases() {
    let root = package(
        "example.root@1",
        &[
            ("left_vendor", "vendor.left@1"),
            ("right_vendor", "vendor.right@1"),
        ],
        vec![module(
            1,
            "example.root@1",
            "program",
            r#"
use left_vendor::left_model::Item as LeftItem;
use right_vendor::right_model::Item as RightItem;

pub machine Probe {
  config {
    left: LeftItem,
    right: RightItem,
  }
  events { Reset }
  outcomes { commit Done }
  state {
    selected_left: LeftItem = left,
    selected_right: RightItem = right,
  }
  observe {
    selected_left,
    selected_right,
  }
  on Reset {
    selected_left = LeftItem { left: 1 };
    selected_right = RightItem { right: "selected" };
    Done
  }
}
"#,
        )],
    );
    let left = package(
        "vendor.left@1",
        &[],
        vec![module(
            2,
            "vendor.left@1",
            "left_model",
            "pub struct Item { left: Int }\n",
        )],
    );
    let right = package(
        "vendor.right@1",
        &[],
        vec![module(
            3,
            "vendor.right@1",
            "right_model",
            "pub struct Item { right: Text }\n",
        )],
    );

    let output = check_package_graph_with_evidence("example.root@1", &[root, left, right], &[]);
    assert!(
        output.diagnostics.is_empty(),
        "diagnostics:\n{:#?}",
        output.diagnostics
    );
    let provenance = output.provenance.as_ref().expect("package provenance");
    for (package, name) in [("vendor.left@1", "Item"), ("vendor.right@1", "Item")] {
        let node = semantic_node_id(
            &format!("{package}::{name}"),
            "root",
            "struct",
            &format!("declaration/{name}"),
        );
        assert!(
            provenance
                .occurrences
                .iter()
                .any(|occurrence| occurrence.node == node && occurrence.role == "reference")
        );
    }
    let program = output.program.expect("linked program");
    assert!(
        program
            .machine_program
            .types
            .contains_key("vendor.left@1::Item")
    );
    assert!(
        program
            .machine_program
            .types
            .contains_key("vendor.right@1::Item")
    );
}

#[test]
fn undeclared_alias_and_private_external_name_are_rejected() {
    let private = package(
        "vendor.private@1",
        &[],
        vec![module(
            2,
            "vendor.private@1",
            "values",
            "const SECRET: Int = 1;\n",
        )],
    );
    let root = package(
        "example.root@1",
        &[("vendor", "vendor.private@1")],
        vec![module(
            1,
            "example.root@1",
            "program",
            "use missing::values::SECRET;\nuse vendor::values::SECRET;\npub const VALUE: Int = 1;\n",
        )],
    );
    let output = check_package_graph_with_evidence("example.root@1", &[root, private], &[]);
    assert!(output.program.is_none());
    let rules = output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.rule)
        .collect::<Vec<_>>();
    assert!(rules.contains(&"uhura-0.4/unknown-dependency-alias"));
    assert!(rules.contains(&"uhura-0.4/private-external-import"));
}

#[test]
fn external_public_part_retains_dependency_owned_topology_and_source_spans() {
    let root = package(
        "example.root@1",
        &[("vendor", "vendor.parts@1")],
        vec![module(
            1,
            "example.root@1",
            "main",
            r#"
use vendor::main::Counter;

pub machine App {
  outcomes { commit Done }
  state {}
  observe {}
  part counter = Counter();
}
"#,
        )],
    );
    let dependency = package(
        "vendor.parts@1",
        &[],
        vec![module(
            1,
            "vendor.parts@1",
            "main",
            r#"
pub part Counter {
  requires outcomes { commit Done }
  events { Tick }
  state { count: Int = 0 }
  pub computed current: Int = count;
  observe { count }
  on Tick {
    count = count + 1;
    Done
  }
}
"#,
        )],
    );

    let output = check_package_graph_with_evidence("example.root@1", &[root, dependency], &[]);
    assert!(
        output.diagnostics.is_empty(),
        "diagnostics:\n{:#?}",
        output.diagnostics
    );
    assert!(
        output
            .program
            .as_ref()
            .expect("linked program")
            .machine_program
            .machines
            .contains_key("example.root@1::App")
    );

    let provenance = output.provenance.expect("package-graph provenance");
    let part = provenance
        .topology
        .nodes
        .iter()
        .find(|entry| {
            entry.node.kind == InteractionGraphNodeKind::Part && entry.node.label == "counter"
        })
        .expect("composed external part topology");
    let modules = provenance
        .topology
        .nodes
        .iter()
        .filter(|entry| entry.node.kind == InteractionGraphNodeKind::Module)
        .collect::<Vec<_>>();
    assert!(
        modules
            .iter()
            .any(|entry| entry.node.label == "example.root@1::main"),
        "the consumer module remains visible in source topology"
    );
    assert!(
        modules
            .iter()
            .any(|entry| entry.node.label == "vendor.parts@1::main"),
        "the provider module remains visible in source topology"
    );
    assert_eq!(
        modules
            .iter()
            .map(|entry| entry.node.id.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        2,
        "package-qualified module identities must not collapse equal logical module names"
    );
    assert!(
        provenance.topology.edges.iter().any(|entry| {
            entry.edge.kind == InteractionGraphEdgeKind::Composes && entry.edge.to == part.node.id
        }),
        "the root machine retains its source-level composition edge"
    );
    assert!(
        provenance
            .topology
            .edges
            .iter()
            .any(|entry| entry.edge.kind == InteractionGraphEdgeKind::Reads),
        "computed dependencies from the provider part survive lowering"
    );
    assert!(
        provenance
            .topology
            .edges
            .iter()
            .any(|entry| entry.edge.kind == InteractionGraphEdgeKind::Observes),
        "committed observations from the provider part survive lowering"
    );

    let sources = provenance
        .sources
        .iter()
        .map(|source| (source.source, source.package.as_str()))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        provenance
            .sources
            .iter()
            .filter(|source| source.path == "main.uhura")
            .map(|source| source.package.as_str())
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from(["example.root@1", "vendor.parts@1"]),
        "the same relative source path is valid in distinct packages"
    );
    assert!(
        part.sources
            .iter()
            .filter(|selector| selector.role == "generated")
            .all(|selector| {
                provenance.occurrences.iter().any(|occurrence| {
                    occurrence.node == selector.node
                        && occurrence.role == selector.role
                        && occurrence.owner == selector.owner
                        && sources.get(&occurrence.source) == Some(&"vendor.parts@1")
                })
            }),
        "the generated part definition must resolve to the dependency package source"
    );
    assert!(
        provenance
            .topology
            .edges
            .iter()
            .filter(|entry| {
                matches!(
                    entry.edge.kind,
                    InteractionGraphEdgeKind::Reads | InteractionGraphEdgeKind::Observes
                )
            })
            .flat_map(|entry| &entry.sources)
            .all(|selector| {
                provenance.occurrences.iter().any(|occurrence| {
                    occurrence.node == selector.node
                        && occurrence.role == selector.role
                        && occurrence.owner == selector.owner
                        && sources.get(&occurrence.source) == Some(&"vendor.parts@1")
                })
            }),
        "provider-authored dependency edges must resolve to provider source spans"
    );
}

fn external_counter_graph(
    dependency_alias: &str,
    dependency_package: &str,
    dependency_module: &str,
) -> Vec<CapturedPackageModules> {
    let root_source = format!(
        r#"
use {dependency_alias}::{dependency_module}::Counter;

pub machine App {{
  outcomes {{ commit Done }}
  state {{}}
  observe {{}}
  part counter = Counter();
}}
"#
    );
    vec![
        package(
            "example.root@1",
            &[(dependency_alias, dependency_package)],
            vec![module(1, "example.root@1", "program", &root_source)],
        ),
        package(
            dependency_package,
            &[],
            vec![module(
                2,
                dependency_package,
                dependency_module,
                r#"
pub part Counter {
  requires outcomes { commit Done }
  events { Tick }
  state { count: Int = 0 }
  observe { count }
  on Tick {
    count = count + 1;
    Done
  }
}
"#,
            )],
        ),
    ]
}

#[test]
fn external_part_public_id_is_machine_identity_material_but_its_locator_is_not() {
    let ordinary = check_package_graph_with_evidence(
        "example.root@1",
        &external_counter_graph("vendor", "vendor.parts@1", "parts"),
        &[],
    )
    .program
    .expect("ordinary external Part graph");
    let relocated = check_package_graph_with_evidence(
        "example.root@1",
        &external_counter_graph("toolkit", "vendor.parts@1", "renamed::parts"),
        &[],
    )
    .program
    .expect("relocated external Part graph");
    let different_provider = check_package_graph_with_evidence(
        "example.root@1",
        &external_counter_graph("vendor", "vendor.other@1", "parts"),
        &[],
    )
    .program
    .expect("different external Part provider");

    assert_eq!(
        ordinary
            .machine_program
            .program_hashes
            .get("example.root@1::App"),
        relocated
            .machine_program
            .program_hashes
            .get("example.root@1::App"),
        "dependency aliases and logical module locators are nonsemantic"
    );
    assert_ne!(
        ordinary
            .machine_program
            .program_hashes
            .get("example.root@1::App"),
        different_provider
            .machine_program
            .program_hashes
            .get("example.root@1::App"),
        "the composed Part PublicId is semantic even when its body is identical"
    );
    assert_eq!(
        ordinary
            .machine_program
            .composed_part_declarations
            .get("example.root@1::App")
            .expect("composed Part identity closure"),
        &std::collections::BTreeSet::from(["vendor.parts@1::Counter".into()])
    );
}

#[test]
fn external_part_reexport_preserves_the_provider_public_id() {
    let direct = check_package_graph_with_evidence(
        "example.root@1",
        &external_counter_graph("vendor", "vendor.parts@1", "parts"),
        &[],
    )
    .program
    .expect("direct external Part graph");

    let root = package(
        "example.root@1",
        &[("vendor", "vendor.facade@1")],
        vec![module(
            1,
            "example.root@1",
            "program",
            r#"
use vendor::facade::Counter;
pub machine App {
  outcomes { commit Done }
  state {}
  observe {}
  part counter = Counter();
}
"#,
        )],
    );
    let facade = package(
        "vendor.facade@1",
        &[("base", "vendor.parts@1")],
        vec![module(
            2,
            "vendor.facade@1",
            "facade",
            "pub use base::parts::Counter;\n",
        )],
    );
    let provider = external_counter_graph("vendor", "vendor.parts@1", "parts")
        .pop()
        .expect("provider package");
    let reexported =
        check_package_graph_with_evidence("example.root@1", &[root, facade, provider], &[])
            .program
            .expect("re-exported external Part graph");

    assert_eq!(
        direct
            .machine_program
            .program_hashes
            .get("example.root@1::App"),
        reexported
            .machine_program
            .program_hashes
            .get("example.root@1::App")
    );
    assert_eq!(
        reexported
            .machine_program
            .composed_part_declarations
            .get("example.root@1::App")
            .expect("composed Part identity closure"),
        &std::collections::BTreeSet::from(["vendor.parts@1::Counter".into()])
    );
}

#[test]
fn external_part_body_links_public_transitive_package_dependencies() {
    let root = package(
        "example.root@1",
        &[("vendor", "vendor.parts@1")],
        vec![module(
            1,
            "example.root@1",
            "program",
            r#"
use vendor::parts::Counter;
pub machine App {
  outcomes { commit Done }
  state {}
  observe {}
  part counter = Counter();
}
"#,
        )],
    );
    let parts = package(
        "vendor.parts@1",
        &[("base", "vendor.base@1")],
        vec![module(
            2,
            "vendor.parts@1",
            "parts",
            r#"
use base::math::increment;
pub part Counter {
  requires outcomes { commit Done }
  events { Tick }
  state { count: Int = 0 }
  observe { count }
  on Tick {
    count = increment(count);
    Done
  }
}
"#,
        )],
    );
    let base = package(
        "vendor.base@1",
        &[],
        vec![module(
            3,
            "vendor.base@1",
            "math",
            "pub fn increment(value: Int) -> Int { value + 1 }\n",
        )],
    );

    let output = check_package_graph_with_evidence("example.root@1", &[root, parts, base], &[]);
    assert!(
        output.diagnostics.is_empty(),
        "diagnostics:\n{:#?}",
        output.diagnostics
    );
    assert!(
        output
            .program
            .expect("linked program")
            .machine_program
            .functions
            .contains_key("vendor.base@1::increment")
    );
}

#[test]
fn external_part_fault_sites_join_dependency_provenance_per_composition_owner() {
    let root = package(
        "example.root@1",
        &[("vendor", "vendor.parts@1")],
        vec![module(
            1,
            "example.root@1",
            "program",
            r#"
use vendor::parts::Guard;
pub machine App {
  outcomes { commit Done }
  state {}
  observe {}
  part right = Guard();
  part left = Guard();
}
"#,
        )],
    );
    let dependency = package(
        "vendor.parts@1",
        &[],
        vec![module(
            1,
            "vendor.parts@1",
            "parts",
            r#"
pub part Guard {
  requires outcomes { commit Done }
  events { Crash }
  state { valid: Bool = true }
  invariant valid;
  on Crash { unreachable; }
}
"#,
        )],
    );

    let output = check_package_graph_with_evidence("example.root@1", &[root, dependency], &[]);
    assert!(
        output.diagnostics.is_empty(),
        "diagnostics:\n{:#?}",
        output.diagnostics
    );
    let program = output.program.expect("linked fault-site program");
    let provenance = output.provenance.expect("linked fault-site provenance");
    let machine_id = "example.root@1::App";
    let machine = &program.machine_program.machines[machine_id];
    for (index, owner) in ["left", "right"].into_iter().enumerate() {
        let invariant = semantic_node_id(machine_id, owner, "invariant", "invariant/0");
        let unreachable = semantic_node_id(
            machine_id,
            owner,
            "unreachable",
            "handler/Crash/statement/0",
        );
        assert_eq!(machine.invariants[index].1.id, invariant);
        let Statement::Unreachable { source } =
            &machine.handlers[&format!("{owner}.Crash")].body[0]
        else {
            panic!("expected direct authored unreachable for `{owner}`");
        };
        assert_eq!(source.id, unreachable);
        for node in [invariant, unreachable] {
            assert!(provenance.occurrences.iter().any(|occurrence| {
                occurrence.node == node
                    && occurrence.owner == owner
                    && occurrence.role == "generated"
            }));
        }
    }
}

#[test]
fn private_external_part_and_colliding_part_imports_are_rejected() {
    let root = package(
        "example.root@1",
        &[
            ("private_vendor", "vendor.private@1"),
            ("left", "vendor.left@1"),
            ("right", "vendor.right@1"),
        ],
        vec![module(
            1,
            "example.root@1",
            "program",
            r#"
use private_vendor::parts::Hidden;
use left::parts::Counter as Shared;
use right::parts::Counter as Shared;
pub machine App {
  outcomes { commit Done }
  state {}
  observe {}
  part hidden = Hidden();
  part shared = Shared();
}
"#,
        )],
    );
    let private = package(
        "vendor.private@1",
        &[],
        vec![module(
            2,
            "vendor.private@1",
            "parts",
            "part Hidden { state {} observe {} }\n",
        )],
    );
    let public_part =
        "pub part Counter { requires outcomes { commit Done } state {} observe {} }\n";
    let left = package(
        "vendor.left@1",
        &[],
        vec![module(3, "vendor.left@1", "parts", public_part)],
    );
    let right = package(
        "vendor.right@1",
        &[],
        vec![module(4, "vendor.right@1", "parts", public_part)],
    );

    let output =
        check_package_graph_with_evidence("example.root@1", &[root, private, left, right], &[]);
    assert!(output.program.is_none());
    let rules = output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.rule)
        .collect::<Vec<_>>();
    assert!(rules.contains(&"uhura-0.4/private-external-import"));
    assert!(rules.contains(&"uhura-0.4/import-collision"));
}

#[test]
fn same_spelled_external_parts_compose_distinctly_under_local_aliases() {
    let root = package(
        "example.root@1",
        &[
            ("left_vendor", "vendor.left@1"),
            ("right_vendor", "vendor.right@1"),
        ],
        vec![module(
            1,
            "example.root@1",
            "program",
            r#"
use left_vendor::parts::Counter as LeftCounter;
use right_vendor::parts::Counter as RightCounter;
pub machine App {
  outcomes { commit Done }
  state {}
  observe {}
  part left = LeftCounter();
  part right = RightCounter();
}
"#,
        )],
    );
    let part = r#"
pub part Counter {
  requires outcomes { commit Done }
  events { Tick }
  state { count: Int = 0 }
  observe { count }
  on Tick {
    count = count + 1;
    Done
  }
}
"#;
    let left = package(
        "vendor.left@1",
        &[],
        vec![module(2, "vendor.left@1", "parts", part)],
    );
    let right = package(
        "vendor.right@1",
        &[],
        vec![module(3, "vendor.right@1", "parts", part)],
    );

    let output = check_package_graph_with_evidence("example.root@1", &[root, left, right], &[]);
    assert!(
        output.diagnostics.is_empty(),
        "diagnostics:\n{:#?}",
        output.diagnostics
    );
    assert_eq!(
        output
            .program
            .expect("two distinct external Parts")
            .machine_program
            .composed_part_declarations["example.root@1::App"],
        std::collections::BTreeSet::from([
            "vendor.left@1::Counter".into(),
            "vendor.right@1::Counter".into(),
        ])
    );
}

#[test]
fn external_part_body_closes_over_provider_private_helpers() {
    let root = package(
        "example.root@1",
        &[("vendor", "vendor.parts@1")],
        vec![module(
            1,
            "example.root@1",
            "program",
            r#"
use vendor::parts::Counter;
pub machine App {
  outcomes { commit Done }
  state {}
  observe {}
  part counter = Counter();
}
"#,
        )],
    );
    let dependency = package(
        "vendor.parts@1",
        &[],
        vec![module(
            2,
            "vendor.parts@1",
            "parts",
            r#"
fn increment(value: Int) -> Int {
  value + 1
}

pub part Counter {
  requires outcomes { commit Done }
  events { Tick }
  state { count: Int = 0 }
  observe { count }
  on Tick {
    count = increment(count);
    Done
  }
}
"#,
        )],
    );

    let output = check_package_graph_with_evidence("example.root@1", &[root, dependency], &[]);
    assert!(
        output.diagnostics.is_empty(),
        "diagnostics:\n{:#?}",
        output.diagnostics
    );
    let program = output.program.expect("linked private helper closure");
    assert!(
        program
            .machine_program
            .functions
            .keys()
            .any(|name| name.contains("__uhura_part_private_") && name.ends_with("_increment"))
    );
}

#[test]
fn external_part_carries_its_lexical_standard_imports_into_composition() {
    let root = package(
        "example.root@1",
        &[("vendor", "vendor.parts@1")],
        vec![module(
            1,
            "example.root@1",
            "program",
            r#"
use vendor::parts::Logger;
pub machine App {
  outcomes { commit Done }
  state {}
  observe {}
  part logger = Logger();
}
"#,
        )],
    );
    let dependency = package(
        "vendor.parts@1",
        &[],
        vec![module(
            2,
            "vendor.parts@1",
            "parts",
            r#"
use uhura::ports::SinkPort;
pub part Logger {
  port sink = SinkPort<Text> {};
  state {}
  observe {}
}
"#,
        )],
    );

    let output = check_package_graph_with_evidence("example.root@1", &[root, dependency], &[]);
    assert!(
        output.diagnostics.is_empty(),
        "diagnostics:\n{:#?}",
        output.diagnostics
    );
    assert_eq!(
        output
            .program
            .expect("linked standard contract")
            .machine_program
            .machines["example.root@1::App"]
            .ports[0]
            .name,
        "logger.sink"
    );
}
