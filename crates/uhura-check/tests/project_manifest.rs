use uhura_check::project_manifest::{LANGUAGE_0_4, load_project_manifest};

fn issue_paths(text: &str) -> Vec<String> {
    load_project_manifest(text)
        .unwrap_err()
        .into_iter()
        .map(|issue| issue.path)
        .collect()
}

#[test]
fn resource_only_and_missing_manifests_are_rejected() {
    let empty = issue_paths("");
    assert!(empty.contains(&"project".to_string()));
    assert!(empty.contains(&"modules".to_string()));

    let configured = issue_paths(
        r#"
[icons]
default = "brand"

[icons.brand]
font = "assets/brand.woff2"
glyphs = "assets/brand.json"
"#,
    );
    assert!(configured.contains(&"project".to_string()));
    assert!(configured.contains(&"modules".to_string()));
}

#[test]
fn parses_the_complete_closed_v04_manifest() {
    let manifest = load_project_manifest(
        r#"
[project]
name = "examples.design-programs"
version = 2
language = "0.4"

[modules]
programs = "programs.uhura"
"shared::notice" = "src/shared/notice.uhura"

[evidence.modules]
programs = "evidence/programs.uhura"

[dependencies]
vendor_icons = { package = "vendor.icon-set", version = 1, path = "vendor/icons" }

[assets]
manifest = "fixtures/assets/manifest.toml"

[icons]
default = "brand"

[icons.brand]
font = "assets/brand.woff2"
glyphs = "assets/brand.json"
"#,
    )
    .unwrap();

    assert_eq!(manifest.project.name.as_str(), "examples.design-programs");
    assert_eq!(manifest.project.version, 2);
    assert_eq!(manifest.project.language, LANGUAGE_0_4);
    assert_eq!(
        manifest.project.package_id().to_string(),
        "examples.design-programs@2"
    );
    assert_eq!(
        manifest
            .modules
            .iter()
            .map(|(module, path)| (module.as_str(), path.as_str()))
            .collect::<Vec<_>>(),
        [
            ("programs", "programs.uhura"),
            ("shared::notice", "src/shared/notice.uhura"),
        ]
    );
    assert_eq!(
        manifest
            .evidence
            .iter()
            .map(|(module, path)| (module.as_str(), path.as_str()))
            .collect::<Vec<_>>(),
        [("programs", "evidence/programs.uhura")]
    );
    let dependency = manifest
        .dependencies
        .iter()
        .find(|(alias, _)| alias.as_str() == "vendor_icons")
        .map(|(_, dependency)| dependency)
        .unwrap();
    assert_eq!(dependency.package.as_str(), "vendor.icon-set");
    assert_eq!(dependency.version, 1);
    assert_eq!(dependency.path.as_str(), "vendor/icons");
    assert_eq!(dependency.package_id().to_string(), "vendor.icon-set@1");
    assert_eq!(
        manifest.resources.assets.manifest.as_deref(),
        Some("fixtures/assets/manifest.toml")
    );
    assert_eq!(manifest.resources.icons.default.as_str(), "brand");
}

#[test]
fn evidence_sources_are_explicit_separate_and_closed() {
    let paths = issue_paths(
        r#"
[project]
name = "example"
version = 1
language = "0.4"

[modules]
programs = "programs.uhura"

[evidence.modules]
module_conflict = "programs.uhura"
evidence = "evidence.uhura"
duplicate = "evidence.uhura"
escape = "../escape.uhura"
not_source = "notes.md"
"Bad-Module" = "bad.uhura"
"#,
    );
    for expected in [
        "evidence.modules.module_conflict",
        "evidence.modules.duplicate",
        "evidence.modules.escape",
        "evidence.modules.not_source",
        "evidence.modules.Bad-Module",
    ] {
        assert!(
            paths.contains(&expected.to_string()),
            "missing issue at {expected}"
        );
    }

    let scalar = issue_paths(
        r#"
[project]
name = "example"
version = 1
language = "0.4"

[modules]
programs = "programs.uhura"

[evidence]
modules = "evidence.uhura"
"#,
    );
    assert!(scalar.contains(&"evidence.modules".to_string()));

    let unknown = issue_paths(
        r#"
[project]
name = "example"
version = 1
language = "0.4"

[modules]
programs = "programs.uhura"

[evidence]
sources = ["evidence.uhura"]
"#,
    );
    assert!(unknown.contains(&"evidence.sources".to_string()));
    assert!(unknown.contains(&"evidence.modules".to_string()));
}

#[test]
fn v04_requires_exact_project_metadata_and_closed_tables() {
    let paths = issue_paths(
        r#"
[project]
name = "Examples.bad_name"
version = 0
language = "0.4-preview"
description = "not part of the schema"

[modules]
programs = "programs.uhura"

[runtime]
mode = "retired"
"#,
    );
    assert!(paths.contains(&"project.name".to_string()));
    assert!(paths.contains(&"project.version".to_string()));
    assert!(paths.contains(&"project.language".to_string()));
    assert!(paths.contains(&"project.description".to_string()));
    assert!(paths.contains(&"runtime".to_string()));
}

#[test]
fn selecting_v04_requires_both_project_and_nonempty_modules() {
    let missing_modules = issue_paths(
        r#"
[project]
name = "example"
version = 1
language = "0.4"
"#,
    );
    assert!(missing_modules.contains(&"modules".to_string()));

    let missing_project = issue_paths(
        r#"
[modules]
programs = "programs.uhura"
"#,
    );
    assert!(missing_project.contains(&"project".to_string()));

    let empty_modules = issue_paths(
        r#"
[project]
name = "example"
version = 1
language = "0.4"

[modules]
"#,
    );
    assert!(empty_modules.contains(&"modules".to_string()));
}

#[test]
fn modules_are_logical_paths_mapped_one_to_one_to_safe_uhura_files() {
    let paths = issue_paths(
        r#"
[project]
name = "example"
version = 1
language = "0.4"

[modules]
"crate::owned" = "crate.uhura"
"Bad-Module" = "bad.uhura"
good = "../escape.uhura"
also_good = "src/same.uhura"
duplicate = "src/same.uhura"
not_source = "src/readme.md"
"#,
    );

    assert!(paths.contains(&"modules.crate::owned".to_string()));
    assert!(paths.contains(&"modules.Bad-Module".to_string()));
    assert!(paths.contains(&"modules.good".to_string()));
    assert!(paths.contains(&"modules.duplicate".to_string()));
    assert!(paths.contains(&"modules.not_source".to_string()));
}

#[test]
fn module_and_dependency_paths_reject_every_lexically_unsafe_shape() {
    for unsafe_path in [
        "",
        "/absolute/source.uhura",
        r"C:\source.uhura",
        "C:source.uhura",
        "https://example.test/source.uhura",
        "src//source.uhura",
        "src/./source.uhura",
        "src/../source.uhura",
    ] {
        let text = format!(
            r#"
[project]
name = "example"
version = 1
language = "0.4"

[modules]
programs = {unsafe_path:?}
"#
        );
        assert!(
            issue_paths(&text).contains(&"modules.programs".to_string()),
            "unsafe path was accepted: {unsafe_path:?}"
        );
    }
}

#[test]
fn dependencies_are_vendored_exact_and_closed() {
    let paths = issue_paths(
        r#"
[project]
name = "example"
version = 1
language = "0.4"

[modules]
programs = "programs.uhura"

[dependencies]
crate = { package = "vendor.icons", version = 1, path = "vendor/icons" }
bad = { package = "Vendor.icons", version = 0, path = "../vendor", git = "main" }
scalar = "vendor.icons"
"#,
    );

    assert!(paths.contains(&"dependencies.crate".to_string()));
    assert!(paths.contains(&"dependencies.bad.package".to_string()));
    assert!(paths.contains(&"dependencies.bad.version".to_string()));
    assert!(paths.contains(&"dependencies.bad.path".to_string()));
    assert!(paths.contains(&"dependencies.bad.git".to_string()));
    assert!(paths.contains(&"dependencies.scalar".to_string()));
}

#[test]
fn project_resources_keep_the_existing_validation_contract() {
    let issues = load_project_manifest(
        r#"
[project]
name = "example"
version = 1
language = "0.4"

[modules]
programs = "programs.uhura"

[assets]
manifest = "../assets.toml"

[icons]
default = "missing"
"#,
    )
    .unwrap_err();
    assert!(issues.iter().any(|issue| issue.path == "assets.manifest"));
    assert!(issues.iter().any(|issue| issue.path == "icons.default"));
}

#[test]
fn dependencies_without_project_tables_are_rejected() {
    let paths = issue_paths(
        r#"
[dependencies]
vendor = { package = "vendor.icons", version = 1, path = "vendor/icons" }
"#,
    );
    assert!(paths.contains(&"project".to_string()));
    assert!(paths.contains(&"modules".to_string()));
}
