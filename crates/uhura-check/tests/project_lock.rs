use std::collections::BTreeMap;

use uhura_check::project_lock::{
    CapturedPackage, LOCK_PROTOCOL, PackageArtifactInput, Sha256Integrity, check_project_lock,
    package_artifact_integrity, parse_project_lock,
};
use uhura_check::project_manifest::{
    DependencyAlias, LogicalModulePath, PackageId, ProjectManifest, ProjectPath,
    load_project_manifest,
};

fn manifest(name: &str, module: &str, dependencies: &[(&str, &str, u64, &str)]) -> ProjectManifest {
    let dependencies = dependencies
        .iter()
        .map(|(alias, package, version, path)| {
            format!("{alias} = {{ package = {package:?}, version = {version}, path = {path:?} }}")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let text = format!(
        r#"
[project]
name = {name:?}
version = 1
language = "0.4"

[modules]
main = {module:?}

[dependencies]
{dependencies}
"#
    );
    load_project_manifest(&text).unwrap()
}

fn capture(
    manifest: ProjectManifest,
    source: &str,
    source_bytes: &[u8],
    resources: &[(&str, &[u8])],
) -> CapturedPackage {
    let module = manifest.modules.keys().next().unwrap().clone();
    let resolved_dependencies = manifest
        .dependencies
        .iter()
        .map(|(alias, dependency)| (alias.clone(), dependency.package_id()))
        .collect();
    CapturedPackage {
        manifest,
        source: ProjectPath::parse(source).unwrap(),
        modules: [(module, source_bytes.to_vec())].into_iter().collect(),
        resolved_dependencies,
        resources: resources
            .iter()
            .map(|(name, bytes)| ((*name).to_string(), bytes.to_vec()))
            .collect(),
    }
}

fn package_record(capture: &CapturedPackage) -> String {
    let dependencies = capture
        .resolved_dependencies
        .iter()
        .map(|(alias, package)| format!("{alias} = {:?}", package.to_string()))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"
[[package]]
package = {package:?}
source = {{ kind = "path", path = {source:?} }}
integrity = {integrity:?}
dependencies = {{ {dependencies} }}
"#,
        package = capture.package_id().to_string(),
        source = capture.source.as_str(),
        integrity = capture.artifact_integrity().unwrap().as_str(),
    )
}

fn valid_graph() -> (ProjectManifest, Vec<CapturedPackage>, String) {
    let root = manifest(
        "example.root",
        "root.uhura",
        &[("vendor_a", "vendor.a", 1, "vendor/a")],
    );
    let package_a = capture(
        manifest(
            "vendor.a",
            "a.uhura",
            &[("vendor_b", "vendor.b", 1, "deps/b")],
        ),
        "vendor/a",
        b"pub const A: int = 1;\n",
        &[],
    );
    let package_b = capture(
        manifest("vendor.b", "b.uhura", &[]),
        "vendor/a/deps/b",
        b"pub const B: int = 2;\n",
        &[],
    );
    let lock = format!(
        r#"protocol = "{LOCK_PROTOCOL}"

[root]
package = "example.root@1"
dependencies = {{ vendor_a = "vendor.a@1" }}
{}{}
"#,
        package_record(&package_a),
        package_record(&package_b)
    );
    (root, vec![package_a, package_b], lock)
}

fn issue_paths(
    root: &ProjectManifest,
    lock: Option<&str>,
    captures: &[CapturedPackage],
) -> Vec<String> {
    check_project_lock(root, lock, captures)
        .unwrap_err()
        .into_iter()
        .map(|issue| issue.path)
        .collect()
}

#[test]
fn admits_a_closed_transitive_graph_and_erases_table_order() {
    let (root, captures, lock) = valid_graph();
    let checked = check_project_lock(&root, Some(&lock), &captures).unwrap();
    assert_eq!(checked.root.package.to_string(), "example.root@1");
    assert_eq!(
        checked
            .packages
            .keys()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["vendor.a@1", "vendor.b@1"]
    );

    let reordered = format!(
        r#"protocol = "{LOCK_PROTOCOL}"

[root]
dependencies = {{ vendor_a = "vendor.a@1" }}
package = "example.root@1"
{}{}
"#,
        package_record(&captures[1]),
        package_record(&captures[0])
    );
    assert_eq!(
        checked,
        check_project_lock(&root, Some(&reordered), &captures).unwrap()
    );
}

#[test]
fn lock_presence_is_exactly_tied_to_root_dependencies() {
    let empty = manifest("example.empty", "main.uhura", &[]);
    let checked = check_project_lock(&empty, None, &[]).unwrap();
    assert!(checked.packages.is_empty());
    assert!(checked.root.dependencies.is_empty());

    assert_eq!(
        issue_paths(&empty, Some(""), &[]),
        ["uhura.lock".to_string()]
    );

    let (root, captures, _) = valid_graph();
    assert_eq!(
        issue_paths(&root, None, &captures),
        ["uhura.lock".to_string()]
    );

    let orphan = captures[0].clone();
    assert!(
        issue_paths(&empty, None, &[orphan])
            .iter()
            .any(|path| path.starts_with("capture.vendor.a@1"))
    );
}

#[test]
fn lock_schema_is_closed_and_all_scalar_forms_are_exact() {
    let issues = parse_project_lock(
        r#"
protocol = "uhura-lock/preview"
unexpected = true

[root]
package = "Bad@01"
dependencies = { crate = "vendor.a@01" }
extra = false

[[package]]
package = "vendor.a@1"
source = { kind = "git", path = "../vendor", url = "https://example.test" }
integrity = "sha256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
dependencies = []
extra = false
"#,
    )
    .unwrap_err();
    let paths = issues
        .into_iter()
        .map(|issue| issue.path)
        .collect::<Vec<_>>();
    for expected in [
        "protocol",
        "unexpected",
        "root.package",
        "root.dependencies.crate",
        "root.extra",
        "package[0].source.kind",
        "package[0].source.path",
        "package[0].source.url",
        "package[0].integrity",
        "package[0].dependencies",
        "package[0].extra",
    ] {
        assert!(
            paths.contains(&expected.to_string()),
            "missing issue at {expected}"
        );
    }

    assert!(PackageId::parse("valid.package@1").is_ok());
    for invalid in [
        "valid.package",
        "valid.package@0",
        "valid.package@01",
        "Valid@1",
    ] {
        assert!(PackageId::parse(invalid).is_err(), "accepted {invalid}");
    }
    assert!(Sha256Integrity::parse(&format!("sha256:{}", "0".repeat(64))).is_ok());
    assert!(Sha256Integrity::parse(&format!("sha256:{}", "A".repeat(64))).is_err());
}

#[test]
fn root_identity_bindings_and_direct_acquisition_paths_must_match_manifest() {
    let (root, captures, lock) = valid_graph();

    let wrong_root = lock.replace("example.root@1", "example.other@1");
    assert!(issue_paths(&root, Some(&wrong_root), &captures).contains(&"root.package".to_string()));

    let wrong_binding = lock.replacen("vendor.a@1", "vendor.b@1", 1);
    assert!(
        issue_paths(&root, Some(&wrong_binding), &captures)
            .contains(&"root.dependencies.vendor_a".to_string())
    );

    let wrong_source = lock.replacen("path = \"vendor/a\"", "path = \"vendor/elsewhere\"", 1);
    let paths = issue_paths(&root, Some(&wrong_source), &captures);
    assert!(paths.contains(&"root.dependencies.vendor_a".to_string()));
    assert!(paths.contains(&"package.vendor.a@1.source.path".to_string()));
}

#[test]
fn rejects_duplicate_missing_extra_and_unreferenced_records_or_captures() {
    let (root, captures, lock) = valid_graph();

    let duplicate = format!("{lock}{}", package_record(&captures[0]));
    assert!(
        parse_project_lock(&duplicate)
            .unwrap_err()
            .iter()
            .any(|issue| issue.message.contains("duplicate lock record"))
    );

    let record_b = package_record(&captures[1]);
    let missing = lock.replace(&record_b, "");
    let paths = issue_paths(&root, Some(&missing), &captures);
    assert!(
        paths
            .iter()
            .any(|path| path == "package.vendor.a@1.dependencies.vendor_b")
    );
    assert!(paths.iter().any(|path| path == "capture.vendor.b@1"));

    let package_c = capture(
        manifest("vendor.c", "c.uhura", &[]),
        "vendor/c",
        b"pub const C: int = 3;\n",
        &[],
    );
    let extra = format!("{lock}{}", package_record(&package_c));
    let mut captures_with_extra = captures.clone();
    captures_with_extra.push(package_c);
    assert!(
        check_project_lock(&root, Some(&extra), &captures_with_extra)
            .unwrap_err()
            .iter()
            .any(|issue| {
                issue.path == "package.vendor.c@1" && issue.message.contains("unreferenced")
            })
    );

    let mut capture_without_lock = captures.clone();
    capture_without_lock.push(capture(
        manifest("vendor.d", "d.uhura", &[]),
        "vendor/d",
        b"pub const D: int = 4;\n",
        &[],
    ));
    assert!(
        issue_paths(&root, Some(&lock), &capture_without_lock)
            .contains(&"capture.vendor.d@1".to_string())
    );
}

#[test]
fn every_package_dependency_map_and_relative_path_is_exact() {
    let (root, mut captures, lock) = valid_graph();
    captures[0].resolved_dependencies.clear();
    let issues = check_project_lock(&root, Some(&lock), &captures).unwrap_err();
    assert!(issues.iter().any(|issue| {
        issue.path == "capture.vendor.a@1.dependencies.vendor_b"
            && issue.message.contains("missing exact binding")
    }));
    assert!(issues.iter().any(|issue| {
        issue.path == "package.vendor.a@1.dependencies.vendor_b"
            && issue.message.contains("unexpected binding")
    }));

    let (root, mut captures, lock) = valid_graph();
    captures[1].source = ProjectPath::parse("vendor/b").unwrap();
    let paths = issue_paths(&root, Some(&lock), &captures);
    assert!(paths.contains(&"package.vendor.b@1.source.path".to_string()));

    let wrong_transitive_source = lock.replace("vendor/a/deps/b", "vendor/b");
    assert!(
        check_project_lock(&root, Some(&wrong_transitive_source), &captures)
            .unwrap_err()
            .iter()
            .any(|issue| {
                issue.path == "package.vendor.a@1.dependencies.vendor_b"
                    && issue.message.contains("root-relative lock source")
            })
    );
}

#[test]
fn vendored_dependency_resources_are_reserved() {
    let (root, mut captures, lock) = valid_graph();
    captures[0].manifest.resources.assets.manifest = Some("assets/manifest.toml".into());
    let issues = check_project_lock(&root, Some(&lock), &captures).unwrap_err();
    assert!(issues.iter().any(|issue| {
        issue.path == "capture.vendor.a@1.resources"
            && issue.message.contains("source-only in Uhura 0.4")
    }));

    let (root, mut captures, _) = valid_graph();
    captures[0]
        .resources
        .insert("future/resource".into(), b"bytes".to_vec());
    let lock = format!(
        r#"protocol = "{LOCK_PROTOCOL}"

[root]
package = "example.root@1"
dependencies = {{ vendor_a = "vendor.a@1" }}
{}{}
"#,
        package_record(&captures[0]),
        package_record(&captures[1])
    );
    let issues = check_project_lock(&root, Some(&lock), &captures).unwrap_err();
    assert!(issues.iter().any(|issue| {
        issue.path == "capture.vendor.a@1.resources"
            && issue.message.contains("reserved and must be empty")
    }));
}

#[test]
fn package_artifact_integrity_is_content_only_sorted_and_domain_separated() {
    let (_, captures, _) = valid_graph();
    let package = &captures[0];
    let integrity = package.artifact_integrity().unwrap();
    assert_eq!(integrity.as_str().len(), "sha256:".len() + 64);

    let mut relocated = package.clone();
    relocated.source = ProjectPath::parse("another/acquisition/root").unwrap();
    for path in relocated.manifest.modules.values_mut() {
        *path = ProjectPath::parse("physically/moved.uhura").unwrap();
    }
    assert_eq!(integrity, relocated.artifact_integrity().unwrap());

    let mut changed_source = package.clone();
    changed_source
        .modules
        .values_mut()
        .next()
        .unwrap()
        .push(b' ');
    assert_ne!(integrity, changed_source.artifact_integrity().unwrap());

    let mut changed_dependency = package.clone();
    changed_dependency.resolved_dependencies.insert(
        DependencyAlias::parse("vendor_b").unwrap(),
        PackageId::parse("vendor.b@2").unwrap(),
    );
    assert_ne!(integrity, changed_dependency.artifact_integrity().unwrap());

    let mut with_resource = package.clone();
    with_resource
        .resources
        .insert("icon/main".into(), b"icon-a".to_vec());
    let resource_integrity = with_resource.artifact_integrity().unwrap();
    let mut changed_resource = with_resource;
    changed_resource
        .resources
        .get_mut("icon/main")
        .unwrap()
        .push(b'!');
    assert_ne!(
        resource_integrity,
        changed_resource.artifact_integrity().unwrap()
    );

    let mut modules = BTreeMap::new();
    modules.insert(LogicalModulePath::parse("z").unwrap(), b"z".to_vec());
    modules.insert(LogicalModulePath::parse("a").unwrap(), b"a".to_vec());
    let mut reversed = BTreeMap::new();
    reversed.insert(LogicalModulePath::parse("a").unwrap(), b"a".to_vec());
    reversed.insert(LogicalModulePath::parse("z").unwrap(), b"z".to_vec());
    let dependencies = BTreeMap::new();
    let resources = BTreeMap::new();
    let package_id = PackageId::parse("fixture.order@1").unwrap();
    let ordered = package_artifact_integrity(PackageArtifactInput {
        package: &package_id,
        modules: &modules,
        dependencies: &dependencies,
        resources: &resources,
    })
    .unwrap();
    assert_eq!(
        ordered.as_str(),
        "sha256:31a96fd6546a9e6ccda2bdf29601b3263dcfc8deefc9026dd3988fca7e3ca2e4"
    );
    assert_eq!(
        ordered,
        package_artifact_integrity(PackageArtifactInput {
            package: &package_id,
            modules: &reversed,
            dependencies: &dependencies,
            resources: &resources,
        })
        .unwrap()
    );
}

#[test]
fn captured_modules_must_be_exact_and_utf8() {
    let (root, mut captures, lock) = valid_graph();
    captures[0].modules.clear();
    captures[0]
        .modules
        .insert(LogicalModulePath::parse("undeclared").unwrap(), vec![0xff]);
    let issues = check_project_lock(&root, Some(&lock), &captures).unwrap_err();
    assert!(issues.iter().any(|issue| {
        issue.path == "capture.vendor.a@1.modules.main" && issue.message.contains("no captured")
    }));
    assert!(issues.iter().any(|issue| {
        issue.path == "capture.vendor.a@1.modules.undeclared"
            && issue.message.contains("not declared")
    }));
    assert!(issues.iter().any(|issue| {
        issue.path == "capture.vendor.a@1.modules.undeclared"
            && issue.message.contains("not valid UTF-8")
    }));
}

#[test]
fn package_graph_must_be_acyclic() {
    let root = manifest(
        "example.root",
        "root.uhura",
        &[("vendor_a", "vendor.a", 1, "vendor/a")],
    );
    let package_a = capture(
        manifest(
            "vendor.a",
            "a.uhura",
            &[("vendor_b", "vendor.b", 1, "deps/b")],
        ),
        "vendor/a",
        b"a",
        &[],
    );
    let package_b = capture(
        manifest(
            "vendor.b",
            "b.uhura",
            &[("vendor_a", "vendor.a", 1, "deps/a")],
        ),
        "vendor/a/deps/b",
        b"b",
        &[],
    );
    let lock = format!(
        r#"protocol = "{LOCK_PROTOCOL}"

[root]
package = "example.root@1"
dependencies = {{ vendor_a = "vendor.a@1" }}
{}{}
"#,
        package_record(&package_a),
        package_record(&package_b)
    );
    assert!(
        check_project_lock(&root, Some(&lock), &[package_a, package_b])
            .unwrap_err()
            .iter()
            .any(|issue| issue.message.contains("package dependency cycle"))
    );
}

#[test]
fn root_and_standard_packages_are_not_lock_artifacts() {
    let (root, captures, lock) = valid_graph();
    let root_capture = capture(
        manifest("example.root", "root.uhura", &[]),
        "vendor/root-copy",
        b"root",
        &[],
    );
    let standard_capture = capture(
        manifest("uhura", "standard.uhura", &[]),
        "vendor/standard",
        b"standard",
        &[],
    );
    let extended = format!(
        "{lock}{}{}",
        package_record(&root_capture),
        package_record(&standard_capture)
    );
    let mut all = captures;
    all.push(root_capture);
    all.push(standard_capture);
    let issues = check_project_lock(&root, Some(&extended), &all).unwrap_err();
    assert!(issues.iter().any(|issue| {
        issue.path == "package.example.root@1" && issue.message.contains("root package")
    }));
    assert!(issues.iter().any(|issue| {
        issue.path == "package.uhura@1" && issue.message.contains("standard package")
    }));
}
