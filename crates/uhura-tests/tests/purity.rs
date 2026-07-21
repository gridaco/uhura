use std::collections::{BTreeMap, BTreeSet};

use cargo_metadata::{DependencyKind, Metadata, MetadataCommand};

fn workspace_metadata() -> Metadata {
    MetadataCommand::new()
        .manifest_path(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml"))
        .exec()
        .expect("Uhura workspace metadata")
}

fn normal_workspace_dependencies(metadata: &Metadata, root: &str) -> BTreeSet<String> {
    let resolve = metadata.resolve.as_ref().expect("workspace resolve graph");
    let packages = metadata
        .packages
        .iter()
        .map(|package| (package.id.clone(), package))
        .collect::<BTreeMap<_, _>>();
    let nodes = resolve
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node))
        .collect::<BTreeMap<_, _>>();
    let workspace = metadata
        .workspace_members
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let root = metadata
        .packages
        .iter()
        .find(|package| package.name == root)
        .unwrap_or_else(|| panic!("workspace package `{root}`"))
        .id
        .clone();

    let mut dependencies = BTreeSet::new();
    let mut visited = BTreeSet::from([root.clone()]);
    let mut pending = vec![root];
    while let Some(package) = pending.pop() {
        let node = nodes.get(&package).expect("package resolve node");
        for dependency in &node.deps {
            let normal = dependency.dep_kinds.is_empty()
                || dependency
                    .dep_kinds
                    .iter()
                    .any(|kind| kind.kind == DependencyKind::Normal);
            if !normal || !workspace.contains(&dependency.pkg) {
                continue;
            }
            dependencies.insert(packages[&dependency.pkg].name.clone());
            if visited.insert(dependency.pkg.clone()) {
                pending.push(dependency.pkg.clone());
            }
        }
    }
    dependencies
}

#[test]
fn core_reaches_only_the_foundation_and_port_seam() {
    let dependencies = normal_workspace_dependencies(&workspace_metadata(), "uhura-core");
    assert_eq!(
        dependencies,
        BTreeSet::from(["uhura-base".to_string(), "uhura-port".to_string()]),
        "the pure engine core must not acquire parser, checker, host, Wasm, or acceptance dependencies"
    );
}
