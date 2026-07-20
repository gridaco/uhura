//! Closed, filesystem-independent validation for Uhura 0.4 dependency locks.
//!
//! The filesystem resolver is responsible for capturing package manifests,
//! source bytes, referenced resource bytes, and canonical acquisition roots.
//! This module validates that capture against `uhura.lock` without consulting
//! the current working directory or any ambient package installation state.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use uhura_base::sha256_hex;

use crate::project_manifest::{
    DependencyAlias, LogicalModulePath, PackageId, ProjectManifest, ProjectPath,
};

pub const LOCK_PROTOCOL: &str = "uhura-lock/0";
pub const PACKAGE_ARTIFACT_PROTOCOL: &str = "uhura-package-artifact/0";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLock {
    pub root: LockedRoot,
    pub packages: BTreeMap<PackageId, LockedPackage>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockedRoot {
    pub package: PackageId,
    pub dependencies: BTreeMap<DependencyAlias, PackageId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockedPackage {
    pub package: PackageId,
    pub source: LockedPathSource,
    pub integrity: Sha256Integrity,
    pub dependencies: BTreeMap<DependencyAlias, PackageId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockedPathSource {
    pub path: ProjectPath,
}

/// A checked textual `sha256:<digest>` integrity value.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sha256Integrity(String);

impl Sha256Integrity {
    pub fn parse(value: &str) -> Result<Self, IntegrityError> {
        let Some(digest) = value.strip_prefix("sha256:") else {
            return Err(IntegrityError);
        };
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(IntegrityError);
        }
        Ok(Self(value.to_string()))
    }

    pub fn from_digest_hex(digest: &str) -> Result<Self, IntegrityError> {
        Self::parse(&format!("sha256:{digest}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn digest_hex(&self) -> &str {
        self.0
            .strip_prefix("sha256:")
            .expect("checked integrity always has the sha256 prefix")
    }
}

impl fmt::Display for Sha256Integrity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntegrityError;

impl fmt::Display for IntegrityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("expected `sha256:` followed by 64 lowercase hexadecimal digits")
    }
}

impl std::error::Error for IntegrityError {}

/// Deterministically captured inputs for one acquired non-standard package.
///
/// `source` is the package root relative to the root project. `modules` is
/// keyed by logical module path rather than physical filename. `resources`
/// is the reserved future package-resource collection and must remain empty
/// for source-only vendored packages in Uhura 0.4. The maps are ordered and
/// duplicate-free by construction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedPackage {
    pub manifest: ProjectManifest,
    pub source: ProjectPath,
    pub modules: BTreeMap<LogicalModulePath, Vec<u8>>,
    pub resolved_dependencies: BTreeMap<DependencyAlias, PackageId>,
    pub resources: BTreeMap<String, Vec<u8>>,
}

impl CapturedPackage {
    pub fn package_id(&self) -> PackageId {
        self.manifest.project.package_id()
    }

    pub fn artifact_integrity(&self) -> Result<Sha256Integrity, PackageArtifactError> {
        package_artifact_integrity(PackageArtifactInput {
            package: &self.manifest.project.package_id(),
            modules: &self.modules,
            dependencies: &self.resolved_dependencies,
            resources: &self.resources,
        })
    }
}

/// A borrowed content-only package artifact projection.
///
/// Physical paths and manifest formatting cannot be supplied through this
/// API, which prevents them from accidentally entering package integrity.
#[derive(Clone, Copy, Debug)]
pub struct PackageArtifactInput<'a> {
    pub package: &'a PackageId,
    pub modules: &'a BTreeMap<LogicalModulePath, Vec<u8>>,
    pub dependencies: &'a BTreeMap<DependencyAlias, PackageId>,
    pub resources: &'a BTreeMap<String, Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageArtifactError {
    pub module: LogicalModulePath,
    pub message: String,
}

impl fmt::Display for PackageArtifactError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "module `{}`: {}", self.module, self.message)
    }
}

impl std::error::Error for PackageArtifactError {}

/// The closed package graph admitted by the lock validator.
///
/// An empty root dependency map represents a project for which `uhura.lock`
/// was correctly absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedProjectLock {
    pub root: LockedRoot,
    pub packages: BTreeMap<PackageId, LockedPackage>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectLockIssue {
    pub path: String,
    pub message: String,
}

/// Parse the closed syntax of an `uhura.lock` file.
///
/// This checks field shapes, exact protocol and identities, path-only source
/// records, duplicate package records, and integrity spelling. Graph closure
/// and captured-content integrity require [`check_project_lock`].
pub fn parse_project_lock(text: &str) -> Result<ProjectLock, Vec<ProjectLockIssue>> {
    let table: toml::Table = match text.parse() {
        Ok(table) => table,
        Err(error) => {
            return Err(vec![ProjectLockIssue {
                path: String::new(),
                message: format!("invalid TOML: {error}"),
            }]);
        }
    };

    let mut issues = Vec::new();
    for key in table.keys() {
        if !["protocol", "root", "package"].contains(&key.as_str()) {
            issue(&mut issues, key, format!("unknown key `{key}`"));
        }
    }

    parse_protocol(&table, &mut issues);
    let root = parse_root(&table, &mut issues);
    let packages = parse_packages(&table, &mut issues);

    match (root, packages) {
        (Some(root), Some(packages)) if issues.is_empty() => Ok(ProjectLock { root, packages }),
        _ => Err(issues),
    }
}

/// Validate lock presence, the complete resolved package graph, captured
/// package inputs, and every package artifact digest.
pub fn check_project_lock(
    root_manifest: &ProjectManifest,
    lock_text: Option<&str>,
    captured_packages: &[CapturedPackage],
) -> Result<CheckedProjectLock, Vec<ProjectLockIssue>> {
    let root_id = root_manifest.project.package_id();
    let expected_root_dependencies = manifest_dependency_ids(root_manifest);

    if expected_root_dependencies.is_empty() {
        let mut issues = Vec::new();
        if lock_text.is_some() {
            issue(
                &mut issues,
                "uhura.lock",
                "lock file must be absent when `[dependencies]` is empty",
            );
        }
        for package in captured_packages {
            issue(
                &mut issues,
                format!("capture.{}", package.package_id()),
                "captured package is unreferenced because the root has no dependencies",
            );
        }
        return if issues.is_empty() {
            Ok(CheckedProjectLock {
                root: LockedRoot {
                    package: root_id,
                    dependencies: BTreeMap::new(),
                },
                packages: BTreeMap::new(),
            })
        } else {
            Err(issues)
        };
    }

    let Some(lock_text) = lock_text else {
        return Err(vec![ProjectLockIssue {
            path: "uhura.lock".to_string(),
            message: "lock file is required when `[dependencies]` is non-empty".to_string(),
        }]);
    };
    let lock = parse_project_lock(lock_text)?;
    let mut issues = Vec::new();

    if lock.root.package != root_id {
        issue(
            &mut issues,
            "root.package",
            format!(
                "expected root package `{root_id}`, found `{}`",
                lock.root.package
            ),
        );
    }
    compare_dependency_maps(
        &mut issues,
        "root.dependencies",
        &expected_root_dependencies,
        &lock.root.dependencies,
    );
    for (alias, package_id) in &expected_root_dependencies {
        if package_id.name.as_str() == "uhura" {
            issue(
                &mut issues,
                format!("root.dependencies.{alias}"),
                "the compiler-provided standard package is not an acquired dependency",
            );
        }
    }

    if lock.packages.contains_key(&root_id) {
        issue(
            &mut issues,
            format!("package.{root_id}"),
            "the root package must not have a lock package record",
        );
    }
    for package_id in lock.packages.keys() {
        if package_id.name.as_str() == "uhura" {
            issue(
                &mut issues,
                format!("package.{package_id}"),
                "the compiler-provided standard package must not have a lock record",
            );
        }
    }

    let captures = index_captures(captured_packages, &root_id, &mut issues);
    validate_root_acquisition_paths(root_manifest, &lock, &mut issues);
    validate_package_records(&lock, &captures, &mut issues);
    validate_graph(&lock, &mut issues);

    if issues.is_empty() {
        Ok(CheckedProjectLock {
            root: lock.root,
            packages: lock.packages,
        })
    } else {
        Err(issues)
    }
}

/// Compute the package artifact digest from the content-only projection in
/// project.md §5.
///
/// Each sorted `(name, value)` pair is an empty-tag frame. A collection is
/// the concatenation of those pair frames and is one outer frame field. This
/// makes the three variable-sized collections unambiguous while preserving
/// the specified pair ordering.
pub fn package_artifact_integrity(
    input: PackageArtifactInput<'_>,
) -> Result<Sha256Integrity, PackageArtifactError> {
    for (module, bytes) in input.modules {
        if std::str::from_utf8(bytes).is_err() {
            return Err(PackageArtifactError {
                module: module.clone(),
                message: "captured source is not valid UTF-8".to_string(),
            });
        }
    }

    let modules = pair_collection(
        input
            .modules
            .iter()
            .map(|(module, bytes)| (module.as_str().as_bytes(), bytes.as_slice())),
    );
    let dependencies = pair_collection(
        input
            .dependencies
            .iter()
            .map(|(alias, package)| (alias.as_str().as_bytes(), package.to_string().into_bytes())),
    );
    let resources = pair_collection(
        input
            .resources
            .iter()
            .map(|(name, bytes)| (name.as_bytes(), bytes.as_slice())),
    );
    let bytes = frame(
        PACKAGE_ARTIFACT_PROTOCOL,
        &[
            input.package.to_string().as_bytes(),
            &modules,
            &dependencies,
            &resources,
        ],
    );
    Ok(Sha256Integrity(format!("sha256:{}", sha256_hex(&bytes))))
}

fn parse_protocol(table: &toml::Table, issues: &mut Vec<ProjectLockIssue>) {
    match table.get("protocol") {
        Some(value) => match value.as_str() {
            Some(LOCK_PROTOCOL) => {}
            Some(found) => issue(
                issues,
                "protocol",
                format!("expected exact protocol `{LOCK_PROTOCOL}`, found `{found}`"),
            ),
            None => issue(issues, "protocol", "expected a string"),
        },
        None => issue(issues, "protocol", "missing required string"),
    }
}

fn parse_root(table: &toml::Table, issues: &mut Vec<ProjectLockIssue>) -> Option<LockedRoot> {
    let Some(value) = table.get("root") else {
        issue(issues, "root", "missing required `[root]` table");
        return None;
    };
    let Some(root) = value.as_table() else {
        issue(issues, "root", "expected a `[root]` table");
        return None;
    };
    for key in root.keys() {
        if !["package", "dependencies"].contains(&key.as_str()) {
            issue(
                issues,
                format!("root.{key}"),
                format!("unknown key `{key}`"),
            );
        }
    }
    let package = required_package_id(root.get("package"), "root.package", issues);
    let dependencies = parse_dependency_map(root.get("dependencies"), "root.dependencies", issues);
    match (package, dependencies) {
        (Some(package), Some(dependencies)) => Some(LockedRoot {
            package,
            dependencies,
        }),
        _ => None,
    }
}

fn parse_packages(
    table: &toml::Table,
    issues: &mut Vec<ProjectLockIssue>,
) -> Option<BTreeMap<PackageId, LockedPackage>> {
    let Some(value) = table.get("package") else {
        issue(issues, "package", "missing required `[[package]]` records");
        return None;
    };
    let Some(records) = value.as_array() else {
        issue(issues, "package", "expected `[[package]]` records");
        return None;
    };
    let mut packages = BTreeMap::new();
    for (index, value) in records.iter().enumerate() {
        let base = format!("package[{index}]");
        let Some(record) = value.as_table() else {
            issue(issues, &base, "expected a `[[package]]` table");
            continue;
        };
        for key in record.keys() {
            if !["package", "source", "integrity", "dependencies"].contains(&key.as_str()) {
                issue(
                    issues,
                    format!("{base}.{key}"),
                    format!("unknown key `{key}`"),
                );
            }
        }
        let package =
            required_package_id(record.get("package"), &format!("{base}.package"), issues);
        let source = parse_source(record.get("source"), &format!("{base}.source"), issues);
        let integrity = parse_integrity(
            record.get("integrity"),
            &format!("{base}.integrity"),
            issues,
        );
        let dependencies = parse_dependency_map(
            record.get("dependencies"),
            &format!("{base}.dependencies"),
            issues,
        );

        if let (Some(package), Some(source), Some(integrity), Some(dependencies)) =
            (package, source, integrity, dependencies)
        {
            let locked = LockedPackage {
                package: package.clone(),
                source,
                integrity,
                dependencies,
            };
            if packages.insert(package.clone(), locked).is_some() {
                issue(
                    issues,
                    format!("{base}.package"),
                    format!("duplicate lock record for package `{package}`"),
                );
            }
        }
    }
    Some(packages)
}

fn parse_source(
    value: Option<&toml::Value>,
    path: &str,
    issues: &mut Vec<ProjectLockIssue>,
) -> Option<LockedPathSource> {
    let Some(value) = value else {
        issue(issues, path, "missing required path source table");
        return None;
    };
    let Some(source) = value.as_table() else {
        issue(issues, path, "expected `{ kind = \"path\", path = ... }`");
        return None;
    };
    for key in source.keys() {
        if !["kind", "path"].contains(&key.as_str()) {
            issue(
                issues,
                format!("{path}.{key}"),
                format!("unknown key `{key}`"),
            );
        }
    }
    match source.get("kind").and_then(toml::Value::as_str) {
        Some("path") => {}
        Some(found) => issue(
            issues,
            format!("{path}.kind"),
            format!("expected exact source kind `path`, found `{found}`"),
        ),
        None if source.contains_key("kind") => {
            issue(issues, format!("{path}.kind"), "expected a string")
        }
        None => issue(issues, format!("{path}.kind"), "missing required string"),
    }
    let source_path = required_string(source.get("path"), &format!("{path}.path"), issues)
        .and_then(|value| match ProjectPath::parse(value) {
            Ok(path) => Some(path),
            Err(error) => {
                issue(issues, format!("{path}.path"), error.to_string());
                None
            }
        });
    source_path.map(|path| LockedPathSource { path })
}

fn parse_integrity(
    value: Option<&toml::Value>,
    path: &str,
    issues: &mut Vec<ProjectLockIssue>,
) -> Option<Sha256Integrity> {
    required_string(value, path, issues).and_then(|value| match Sha256Integrity::parse(value) {
        Ok(integrity) => Some(integrity),
        Err(error) => {
            issue(issues, path, error.to_string());
            None
        }
    })
}

fn parse_dependency_map(
    value: Option<&toml::Value>,
    path: &str,
    issues: &mut Vec<ProjectLockIssue>,
) -> Option<BTreeMap<DependencyAlias, PackageId>> {
    let Some(value) = value else {
        issue(issues, path, "missing required dependency map");
        return None;
    };
    let Some(table) = value.as_table() else {
        issue(issues, path, "expected a dependency map");
        return None;
    };
    let mut dependencies = BTreeMap::new();
    for (alias, value) in table {
        let item_path = format!("{path}.{alias}");
        let alias = match DependencyAlias::parse(alias) {
            Ok(alias) => Some(alias),
            Err(error) => {
                issue(issues, &item_path, error.to_string());
                None
            }
        };
        let package = required_package_id(Some(value), &item_path, issues);
        if let (Some(alias), Some(package)) = (alias, package) {
            dependencies.insert(alias, package);
        }
    }
    Some(dependencies)
}

fn required_package_id(
    value: Option<&toml::Value>,
    path: &str,
    issues: &mut Vec<ProjectLockIssue>,
) -> Option<PackageId> {
    required_string(value, path, issues).and_then(|value| match PackageId::parse(value) {
        Ok(package) => Some(package),
        Err(error) => {
            issue(issues, path, error.to_string());
            None
        }
    })
}

fn required_string<'a>(
    value: Option<&'a toml::Value>,
    path: &str,
    issues: &mut Vec<ProjectLockIssue>,
) -> Option<&'a str> {
    match value {
        Some(value) => match value.as_str() {
            Some(value) => Some(value),
            None => {
                issue(issues, path, "expected a string");
                None
            }
        },
        None => {
            issue(issues, path, "missing required string");
            None
        }
    }
}

fn manifest_dependency_ids(manifest: &ProjectManifest) -> BTreeMap<DependencyAlias, PackageId> {
    manifest
        .dependencies
        .iter()
        .map(|(alias, dependency)| (alias.clone(), dependency.package_id()))
        .collect()
}

fn compare_dependency_maps(
    issues: &mut Vec<ProjectLockIssue>,
    path: &str,
    expected: &BTreeMap<DependencyAlias, PackageId>,
    actual: &BTreeMap<DependencyAlias, PackageId>,
) {
    for (alias, expected_package) in expected {
        match actual.get(alias) {
            Some(actual_package) if actual_package == expected_package => {}
            Some(actual_package) => issue(
                issues,
                format!("{path}.{alias}"),
                format!("expected `{expected_package}`, found `{actual_package}`"),
            ),
            None => issue(
                issues,
                format!("{path}.{alias}"),
                format!("missing exact binding to `{expected_package}`"),
            ),
        }
    }
    for (alias, actual_package) in actual {
        if !expected.contains_key(alias) {
            issue(
                issues,
                format!("{path}.{alias}"),
                format!("unexpected binding to `{actual_package}`"),
            );
        }
    }
}

fn index_captures<'a>(
    captures: &'a [CapturedPackage],
    root_id: &PackageId,
    issues: &mut Vec<ProjectLockIssue>,
) -> BTreeMap<PackageId, &'a CapturedPackage> {
    let mut indexed = BTreeMap::new();
    for capture in captures {
        let package_id = capture.package_id();
        if package_id == *root_id {
            issue(
                issues,
                format!("capture.{package_id}"),
                "the root package must not be supplied as a dependency capture",
            );
        }
        if package_id.name.as_str() == "uhura" {
            issue(
                issues,
                format!("capture.{package_id}"),
                "the compiler-provided standard package must not be captured",
            );
        }
        if indexed.insert(package_id.clone(), capture).is_some() {
            issue(
                issues,
                format!("capture.{package_id}"),
                "duplicate captured package",
            );
        }
    }
    indexed
}

fn validate_root_acquisition_paths(
    root_manifest: &ProjectManifest,
    lock: &ProjectLock,
    issues: &mut Vec<ProjectLockIssue>,
) {
    for (alias, dependency) in &root_manifest.dependencies {
        let package_id = dependency.package_id();
        if let Some(record) = lock.packages.get(&package_id)
            && record.source.path != dependency.path
        {
            issue(
                issues,
                format!("root.dependencies.{alias}"),
                format!(
                    "manifest path `{}` resolves to `{package_id}`, but its lock source is `{}`",
                    dependency.path, record.source.path
                ),
            );
        }
    }
}

fn validate_package_records(
    lock: &ProjectLock,
    captures: &BTreeMap<PackageId, &CapturedPackage>,
    issues: &mut Vec<ProjectLockIssue>,
) {
    for (package_id, record) in &lock.packages {
        let Some(capture) = captures.get(package_id) else {
            issue(
                issues,
                format!("package.{package_id}"),
                "lock record has no captured package artifact",
            );
            continue;
        };

        if record.source.path != capture.source {
            issue(
                issues,
                format!("package.{package_id}.source.path"),
                format!(
                    "expected captured acquisition path `{}`, found `{}`",
                    capture.source, record.source.path
                ),
            );
        }

        let manifest_modules = capture
            .manifest
            .modules
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        let captured_modules = capture.modules.keys().cloned().collect::<BTreeSet<_>>();
        for module in manifest_modules.difference(&captured_modules) {
            issue(
                issues,
                format!("capture.{package_id}.modules.{module}"),
                "manifest module has no captured UTF-8 source",
            );
        }
        for module in captured_modules.difference(&manifest_modules) {
            issue(
                issues,
                format!("capture.{package_id}.modules.{module}"),
                "captured source is not declared by the package manifest",
            );
        }

        let expected_dependencies = manifest_dependency_ids(&capture.manifest);
        for (alias, dependency) in &expected_dependencies {
            if dependency.name.as_str() == "uhura" {
                issue(
                    issues,
                    format!("capture.{package_id}.dependencies.{alias}"),
                    "the compiler-provided standard package is not an acquired dependency",
                );
            }
        }
        compare_dependency_maps(
            issues,
            &format!("capture.{package_id}.dependencies"),
            &expected_dependencies,
            &capture.resolved_dependencies,
        );
        compare_dependency_maps(
            issues,
            &format!("package.{package_id}.dependencies"),
            &capture.resolved_dependencies,
            &record.dependencies,
        );

        if capture.manifest.resources.assets.manifest.is_some()
            || !capture.manifest.resources.icons.families.is_empty()
        {
            issue(
                issues,
                format!("capture.{package_id}.resources"),
                "vendored dependency packages are source-only in Uhura 0.4; `[assets]` and project-local `[icons]` are not admitted",
            );
        }
        if !capture.resources.is_empty() {
            issue(
                issues,
                format!("capture.{package_id}.resources"),
                "vendored dependency resource artifact inputs are reserved and must be empty in Uhura 0.4",
            );
        }

        for (alias, dependency) in &capture.manifest.dependencies {
            let target = dependency.package_id();
            let expected_path = joined_path(&capture.source, &dependency.path);
            if let (Some(expected_path), Some(target_record)) =
                (expected_path, lock.packages.get(&target))
                && target_record.source.path != expected_path
            {
                issue(
                    issues,
                    format!("package.{package_id}.dependencies.{alias}"),
                    format!(
                        "manifest path `{}` resolves to `{target}`, but its root-relative lock source is `{}` instead of `{expected_path}`",
                        dependency.path, target_record.source.path
                    ),
                );
            }
        }

        match capture.artifact_integrity() {
            Ok(actual) if actual == record.integrity => {}
            Ok(actual) => issue(
                issues,
                format!("package.{package_id}.integrity"),
                format!("captured artifact has integrity `{actual}`"),
            ),
            Err(error) => issue(
                issues,
                format!("capture.{package_id}.modules.{}", error.module),
                error.message,
            ),
        }
    }

    for package_id in captures.keys() {
        if !lock.packages.contains_key(package_id) {
            issue(
                issues,
                format!("capture.{package_id}"),
                "captured package has no lock record",
            );
        }
    }
}

fn joined_path(parent: &ProjectPath, child: &ProjectPath) -> Option<ProjectPath> {
    ProjectPath::parse(&format!("{parent}/{child}")).ok()
}

fn validate_graph(lock: &ProjectLock, issues: &mut Vec<ProjectLockIssue>) {
    for (alias, package_id) in &lock.root.dependencies {
        validate_dependency_target(
            issues,
            &format!("root.dependencies.{alias}"),
            package_id,
            &lock.root.package,
        );
        if !lock.packages.contains_key(package_id) {
            issue(
                issues,
                format!("root.dependencies.{alias}"),
                format!("missing package record for `{package_id}`"),
            );
        }
    }
    for (owner, record) in &lock.packages {
        for (alias, package_id) in &record.dependencies {
            validate_dependency_target(
                issues,
                &format!("package.{owner}.dependencies.{alias}"),
                package_id,
                &lock.root.package,
            );
            if !lock.packages.contains_key(package_id) {
                issue(
                    issues,
                    format!("package.{owner}.dependencies.{alias}"),
                    format!("missing package record for `{package_id}`"),
                );
            }
        }
    }

    let mut reachable = BTreeSet::new();
    let mut stack = lock.root.dependencies.values().cloned().collect::<Vec<_>>();
    while let Some(package_id) = stack.pop() {
        if !reachable.insert(package_id.clone()) {
            continue;
        }
        if let Some(record) = lock.packages.get(&package_id) {
            stack.extend(record.dependencies.values().cloned());
        }
    }
    for package_id in lock.packages.keys() {
        if !reachable.contains(package_id) {
            issue(
                issues,
                format!("package.{package_id}"),
                "unreferenced package record is outside the root dependency closure",
            );
        }
    }

    let mut marks = BTreeMap::new();
    let mut path = Vec::new();
    for package_id in lock.packages.keys() {
        if find_cycle(package_id, lock, &mut marks, &mut path, issues) {
            break;
        }
    }
}

fn validate_dependency_target(
    issues: &mut Vec<ProjectLockIssue>,
    path: &str,
    package_id: &PackageId,
    root_id: &PackageId,
) {
    if package_id == root_id {
        issue(
            issues,
            path,
            "dependency graph must not point back to the root package",
        );
    }
    if package_id.name.as_str() == "uhura" {
        issue(
            issues,
            path,
            "the compiler-provided standard package is outside `uhura.lock`",
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VisitMark {
    Visiting,
    Complete,
}

fn find_cycle(
    package_id: &PackageId,
    lock: &ProjectLock,
    marks: &mut BTreeMap<PackageId, VisitMark>,
    path: &mut Vec<PackageId>,
    issues: &mut Vec<ProjectLockIssue>,
) -> bool {
    match marks.get(package_id) {
        Some(VisitMark::Complete) => return false,
        Some(VisitMark::Visiting) => {
            let start = path.iter().position(|item| item == package_id).unwrap_or(0);
            let mut cycle = path[start..]
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            cycle.push(package_id.to_string());
            issue(
                issues,
                format!("package.{package_id}.dependencies"),
                format!("package dependency cycle: {}", cycle.join(" -> ")),
            );
            return true;
        }
        None => {}
    }
    marks.insert(package_id.clone(), VisitMark::Visiting);
    path.push(package_id.clone());
    if let Some(record) = lock.packages.get(package_id) {
        for dependency in record.dependencies.values() {
            if lock.packages.contains_key(dependency)
                && find_cycle(dependency, lock, marks, path, issues)
            {
                return true;
            }
        }
    }
    path.pop();
    marks.insert(package_id.clone(), VisitMark::Complete);
    false
}

fn pair_collection<'a, K, V, I>(pairs: I) -> Vec<u8>
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<[u8]> + 'a,
    V: AsRef<[u8]> + 'a,
{
    let mut bytes = Vec::new();
    for (key, value) in pairs {
        bytes.extend_from_slice(&frame("", &[key.as_ref(), value.as_ref()]));
    }
    bytes
}

fn frame(tag: &str, fields: &[&[u8]]) -> Vec<u8> {
    let mut bytes = tag.as_bytes().to_vec();
    for field in fields {
        let length = u64::try_from(field.len()).expect("an allocated field length fits u64");
        bytes.extend_from_slice(&length.to_be_bytes());
        bytes.extend_from_slice(field);
    }
    bytes
}

fn issue(issues: &mut Vec<ProjectLockIssue>, path: impl Into<String>, message: impl Into<String>) {
    issues.push(ProjectLockIssue {
        path: path.into(),
        message: message.into(),
    });
}
