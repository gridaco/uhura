//! Versioned `uhura.toml` project metadata.
//!
//! This module validates the closed, filesystem-independent part of the 0.4
//! project contract. File existence, UTF-8 decoding, case ambiguity, symlink
//! escape, and unmapped-source discovery require a captured project root and
//! therefore remain resolver checks.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

use crate::resource_manifest::{ResourceManifest, ResourceManifestIssue, load_resource_manifest};

pub const LANGUAGE_0_4: &str = "0.4";

/// A parsed `uhura.toml`.
///
/// Resource-only manifests remain valid for legacy 0.3 projects. The presence
/// of any 0.4 project table selects the strict 0.4 schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadedProjectManifest {
    Legacy03(ResourceManifest),
    V04(ProjectManifest),
}

impl LoadedProjectManifest {
    pub fn resources(&self) -> &ResourceManifest {
        match self {
            Self::Legacy03(resources) => resources,
            Self::V04(manifest) => &manifest.resources,
        }
    }

    pub fn as_v04(&self) -> Option<&ProjectManifest> {
        match self {
            Self::Legacy03(_) => None,
            Self::V04(manifest) => Some(manifest),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectManifest {
    pub project: ProjectConfig,
    pub modules: BTreeMap<LogicalModulePath, ProjectPath>,
    /// Explicit tooling-only sources using the separately versioned evidence
    /// language. These files are not 0.4 core modules.
    pub evidence: Vec<ProjectPath>,
    pub dependencies: BTreeMap<DependencyAlias, DependencyConfig>,
    pub resources: ResourceManifest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectConfig {
    pub name: PackageName,
    pub version: u64,
    pub language: String,
}

impl ProjectConfig {
    pub fn package_id(&self) -> PackageId {
        PackageId {
            name: self.name.clone(),
            version: self.version,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyConfig {
    pub package: PackageName,
    pub version: u64,
    pub path: ProjectPath,
}

impl DependencyConfig {
    pub fn package_id(&self) -> PackageId {
        PackageId {
            name: self.package.clone(),
            version: self.version,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageName(String);

impl PackageName {
    pub fn parse(value: &str) -> Result<Self, ProjectValueError> {
        if value.split('.').all(kebab_segment) {
            Ok(Self(value.to_string()))
        } else {
            Err(ProjectValueError::new(
                "expected dot-separated lowercase kebab-case package segments",
            ))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for PackageName {
    type Err = ProjectValueError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl fmt::Display for PackageName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageId {
    pub name: PackageName,
    pub version: u64,
}

impl PackageId {
    pub fn parse(value: &str) -> Result<Self, ProjectValueError> {
        let Some((name, version)) = value.split_once('@') else {
            return Err(ProjectValueError::new(
                "expected `<package-name>@<positive-version>`",
            ));
        };
        if name.contains('@')
            || version.is_empty()
            || version.starts_with('0')
            || !version.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(ProjectValueError::new(
                "expected `<package-name>@<positive-version>`",
            ));
        }
        let version = version.parse::<u64>().ok().filter(|value| {
            *value > 0 && *value <= u64::try_from(i64::MAX).expect("i64::MAX fits u64")
        });
        let Some(version) = version else {
            return Err(ProjectValueError::new(
                "expected `<package-name>@<positive-version>`",
            ));
        };
        Ok(Self {
            name: PackageName::parse(name)?,
            version,
        })
    }
}

impl FromStr for PackageId {
    type Err = ProjectValueError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl fmt::Display for PackageId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}@{}", self.name, self.version)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LogicalModulePath(String);

impl LogicalModulePath {
    pub fn parse(value: &str) -> Result<Self, ProjectValueError> {
        let valid = !value.is_empty()
            && value
                .split("::")
                .all(|segment| logical_segment(segment) && !matches!(segment, "crate" | "uhura"));
        if valid {
            Ok(Self(value.to_string()))
        } else {
            Err(ProjectValueError::new(
                "expected `::`-separated lowercase snake-case segments excluding `crate` and `uhura`",
            ))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for LogicalModulePath {
    type Err = ProjectValueError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl fmt::Display for LogicalModulePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DependencyAlias(String);

impl DependencyAlias {
    pub fn parse(value: &str) -> Result<Self, ProjectValueError> {
        if logical_segment(value) && !matches!(value, "crate" | "uhura") {
            Ok(Self(value.to_string()))
        } else {
            Err(ProjectValueError::new(
                "expected one lowercase snake-case segment excluding `crate` and `uhura`",
            ))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for DependencyAlias {
    type Err = ProjectValueError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl fmt::Display for DependencyAlias {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A lexically safe project-relative path.
///
/// This type proves only the manifest-level path rules. It does not imply that
/// a path exists or remains beneath the project root after symlink resolution.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectPath(String);

impl ProjectPath {
    pub fn parse(value: &str) -> Result<Self, ProjectValueError> {
        if safe_project_path(value) {
            Ok(Self(value.to_string()))
        } else {
            Err(ProjectValueError::new(
                "expected a safe project-relative path",
            ))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for ProjectPath {
    type Err = ProjectValueError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl fmt::Display for ProjectPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectManifestIssue {
    pub path: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectValueError {
    message: &'static str,
}

impl ProjectValueError {
    const fn new(message: &'static str) -> Self {
        Self { message }
    }
}

impl fmt::Display for ProjectValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message)
    }
}

impl std::error::Error for ProjectValueError {}

/// Parse either a legacy resource-only manifest or the closed 0.4 project
/// manifest.
pub fn load_project_manifest(
    text: &str,
) -> Result<LoadedProjectManifest, Vec<ProjectManifestIssue>> {
    let table: toml::Table = match text.parse() {
        Ok(table) => table,
        Err(error) => {
            return Err(vec![ProjectManifestIssue {
                path: String::new(),
                message: format!("invalid TOML: {error}"),
            }]);
        }
    };

    if !selects_v04(&table) {
        return load_resource_manifest(text)
            .map(LoadedProjectManifest::Legacy03)
            .map_err(resource_issues);
    }

    parse_v04(table)
}

fn selects_v04(table: &toml::Table) -> bool {
    ["project", "modules", "evidence", "dependencies"]
        .iter()
        .any(|key| table.contains_key(*key))
}

fn parse_v04(table: toml::Table) -> Result<LoadedProjectManifest, Vec<ProjectManifestIssue>> {
    let mut issues = Vec::new();
    for key in table.keys() {
        if ![
            "project",
            "modules",
            "evidence",
            "dependencies",
            "assets",
            "icons",
        ]
        .contains(&key.as_str())
        {
            issue(&mut issues, key, format!("unknown key `{key}`"));
        }
    }

    let project = parse_project(&table, &mut issues);
    let modules = parse_modules(&table, &mut issues);
    let evidence = parse_evidence(&table, modules.as_ref(), &mut issues);
    let dependencies = parse_dependencies(&table, &mut issues);
    let resources = parse_resources(&table, &mut issues);

    match (project, modules, resources) {
        (Some(project), Some(modules), Some(resources)) if issues.is_empty() => {
            Ok(LoadedProjectManifest::V04(ProjectManifest {
                project,
                modules,
                evidence,
                dependencies,
                resources,
            }))
        }
        _ => Err(issues),
    }
}

fn parse_project(
    table: &toml::Table,
    issues: &mut Vec<ProjectManifestIssue>,
) -> Option<ProjectConfig> {
    let Some(value) = table.get("project") else {
        issue(issues, "project", "missing required `[project]` table");
        return None;
    };
    let Some(project) = value.as_table() else {
        issue(issues, "project", "expected a `[project]` table");
        return None;
    };

    for key in project.keys() {
        if !["name", "version", "language"].contains(&key.as_str()) {
            issue(
                issues,
                format!("project.{key}"),
                format!("unknown key `{key}`"),
            );
        }
    }

    let name = required_string(project.get("name"), "project.name", issues)
        .and_then(|name| package_name(name, "project.name", issues));
    let version = positive_integer(project.get("version"), "project.version", issues);
    let language = match required_string(project.get("language"), "project.language", issues) {
        Some(LANGUAGE_0_4) => Some(LANGUAGE_0_4.to_string()),
        Some(language) => {
            issue(
                issues,
                "project.language",
                format!("expected exact language version `{LANGUAGE_0_4}`, found `{language}`"),
            );
            None
        }
        None => None,
    };

    match (name, version, language) {
        (Some(name), Some(version), Some(language)) => Some(ProjectConfig {
            name,
            version,
            language,
        }),
        _ => None,
    }
}

fn parse_modules(
    table: &toml::Table,
    issues: &mut Vec<ProjectManifestIssue>,
) -> Option<BTreeMap<LogicalModulePath, ProjectPath>> {
    let Some(value) = table.get("modules") else {
        issue(issues, "modules", "missing required `[modules]` table");
        return None;
    };
    let Some(modules) = value.as_table() else {
        issue(issues, "modules", "expected a `[modules]` table");
        return None;
    };
    if modules.is_empty() {
        issue(
            issues,
            "modules",
            "expected at least one mapped source module",
        );
    }

    let mut out = BTreeMap::new();
    let mut physical_paths = BTreeSet::new();
    for (logical, value) in modules {
        let path = format!("modules.{logical}");
        let logical = logical_module_path(logical, &path, issues);
        let physical = required_string(Some(value), &path, issues)
            .and_then(|value| project_path(value, &path, issues));
        let physical = physical.and_then(|physical| {
            if !physical.as_str().ends_with(".uhura") {
                issue(issues, &path, "expected a `.uhura` source file");
                return None;
            }
            if !physical_paths.insert(physical.clone()) {
                issue(
                    issues,
                    &path,
                    format!("source file `{physical}` is already mapped by another module"),
                );
                return None;
            }
            Some(physical)
        });

        if let (Some(logical), Some(physical)) = (logical, physical) {
            out.insert(logical, physical);
        }
    }
    Some(out)
}

fn parse_evidence(
    table: &toml::Table,
    modules: Option<&BTreeMap<LogicalModulePath, ProjectPath>>,
    issues: &mut Vec<ProjectManifestIssue>,
) -> Vec<ProjectPath> {
    let Some(value) = table.get("evidence") else {
        return Vec::new();
    };
    let Some(evidence) = value.as_table() else {
        issue(issues, "evidence", "expected an `[evidence]` table");
        return Vec::new();
    };
    for key in evidence.keys() {
        if key != "sources" {
            issue(
                issues,
                format!("evidence.{key}"),
                format!("unknown key `{key}`"),
            );
        }
    }
    let Some(value) = evidence.get("sources") else {
        issue(issues, "evidence.sources", "missing required source array");
        return Vec::new();
    };
    let Some(values) = value.as_array() else {
        issue(
            issues,
            "evidence.sources",
            "expected an array of `.uhura` source paths",
        );
        return Vec::new();
    };
    if values.is_empty() {
        issue(
            issues,
            "evidence.sources",
            "expected at least one evidence source",
        );
    }

    let module_paths = modules
        .map(|modules| modules.values().collect::<BTreeSet<_>>())
        .unwrap_or_default();
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for (index, value) in values.iter().enumerate() {
        let path = format!("evidence.sources[{index}]");
        let source = required_string(Some(value), &path, issues)
            .and_then(|value| project_path(value, &path, issues));
        let Some(source) = source else {
            continue;
        };
        if !source.as_str().ends_with(".uhura") {
            issue(issues, &path, "expected a `.uhura` source file");
            continue;
        }
        if module_paths.contains(&source) {
            issue(
                issues,
                &path,
                format!("source file `{source}` is already mapped as a 0.4 core module"),
            );
            continue;
        }
        if !seen.insert(source.clone()) {
            issue(
                issues,
                &path,
                format!("evidence source file `{source}` occurs more than once"),
            );
            continue;
        }
        out.push(source);
    }
    out.sort();
    out
}

fn parse_dependencies(
    table: &toml::Table,
    issues: &mut Vec<ProjectManifestIssue>,
) -> BTreeMap<DependencyAlias, DependencyConfig> {
    let Some(value) = table.get("dependencies") else {
        return BTreeMap::new();
    };
    let Some(dependencies) = value.as_table() else {
        issue(issues, "dependencies", "expected a `[dependencies]` table");
        return BTreeMap::new();
    };

    let mut out = BTreeMap::new();
    for (alias, value) in dependencies {
        let base = format!("dependencies.{alias}");
        let alias = dependency_alias(alias, &base, issues);
        let Some(config) = value.as_table() else {
            issue(
                issues,
                &base,
                "expected `{ package = ..., version = ..., path = ... }`",
            );
            continue;
        };
        for key in config.keys() {
            if !["package", "version", "path"].contains(&key.as_str()) {
                issue(
                    issues,
                    format!("{base}.{key}"),
                    format!("unknown key `{key}`"),
                );
            }
        }

        let package = required_string(config.get("package"), &format!("{base}.package"), issues)
            .and_then(|name| package_name(name, &format!("{base}.package"), issues));
        let version = positive_integer(config.get("version"), &format!("{base}.version"), issues);
        let path = required_string(config.get("path"), &format!("{base}.path"), issues)
            .and_then(|path| project_path(path, &format!("{base}.path"), issues));

        if let (Some(alias), Some(package), Some(version), Some(path)) =
            (alias, package, version, path)
        {
            out.insert(
                alias,
                DependencyConfig {
                    package,
                    version,
                    path,
                },
            );
        }
    }
    out
}

fn parse_resources(
    table: &toml::Table,
    issues: &mut Vec<ProjectManifestIssue>,
) -> Option<ResourceManifest> {
    let mut resource_table = toml::Table::new();
    for key in ["assets", "icons"] {
        if let Some(value) = table.get(key) {
            resource_table.insert(key.to_string(), value.clone());
        }
    }

    let text = match toml::to_string(&resource_table) {
        Ok(text) => text,
        Err(error) => {
            issue(
                issues,
                "",
                format!("could not normalize project resources: {error}"),
            );
            return None;
        }
    };
    match load_resource_manifest(&text) {
        Ok(resources) => Some(resources),
        Err(resource_errors) => {
            issues.extend(resource_issues(resource_errors));
            None
        }
    }
}

fn package_name(
    value: &str,
    path: &str,
    issues: &mut Vec<ProjectManifestIssue>,
) -> Option<PackageName> {
    if value.split('.').all(kebab_segment) {
        Some(PackageName(value.to_string()))
    } else {
        issue(
            issues,
            path,
            "expected dot-separated lowercase kebab-case package segments",
        );
        None
    }
}

fn kebab_segment(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    let mut previous_dash = false;
    for byte in &bytes[1..] {
        match byte {
            b'a'..=b'z' | b'0'..=b'9' => previous_dash = false,
            b'-' if !previous_dash => previous_dash = true,
            _ => return false,
        }
    }
    !previous_dash
}

fn logical_module_path(
    value: &str,
    path: &str,
    issues: &mut Vec<ProjectManifestIssue>,
) -> Option<LogicalModulePath> {
    let valid = !value.is_empty()
        && value
            .split("::")
            .all(|segment| logical_segment(segment) && !matches!(segment, "crate" | "uhura"));
    if valid {
        Some(LogicalModulePath(value.to_string()))
    } else {
        issue(
            issues,
            path,
            "expected `::`-separated lowercase snake-case segments excluding `crate` and `uhura`",
        );
        None
    }
}

fn dependency_alias(
    value: &str,
    path: &str,
    issues: &mut Vec<ProjectManifestIssue>,
) -> Option<DependencyAlias> {
    if logical_segment(value) && !matches!(value, "crate" | "uhura") {
        Some(DependencyAlias(value.to_string()))
    } else {
        issue(
            issues,
            path,
            "expected one lowercase snake-case segment excluding `crate` and `uhura`",
        );
        None
    }
}

fn logical_segment(segment: &str) -> bool {
    let Some(first) = segment.as_bytes().first() else {
        return false;
    };
    matches!(first, b'a'..=b'z' | b'_')
        && segment
            .as_bytes()
            .iter()
            .skip(1)
            .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'_'))
}

fn project_path(
    value: &str,
    path: &str,
    issues: &mut Vec<ProjectManifestIssue>,
) -> Option<ProjectPath> {
    if safe_project_path(value) {
        Some(ProjectPath(value.to_string()))
    } else {
        issue(issues, path, "expected a safe project-relative path");
        None
    }
}

fn safe_project_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains('\0')
        && !path.contains("://")
        && !matches!(
            path.as_bytes(),
            [drive, b':', ..] if drive.is_ascii_alphabetic()
        )
        && path
            .split('/')
            .all(|segment| !segment.is_empty() && !matches!(segment, "." | ".."))
}

fn required_string<'a>(
    value: Option<&'a toml::Value>,
    path: &str,
    issues: &mut Vec<ProjectManifestIssue>,
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

fn positive_integer(
    value: Option<&toml::Value>,
    path: &str,
    issues: &mut Vec<ProjectManifestIssue>,
) -> Option<u64> {
    match value.and_then(toml::Value::as_integer) {
        Some(value) if value > 0 => Some(value as u64),
        Some(_) => {
            issue(issues, path, "expected a positive TOML integer");
            None
        }
        None if value.is_none() => {
            issue(issues, path, "missing required positive integer");
            None
        }
        None => {
            issue(issues, path, "expected a positive TOML integer");
            None
        }
    }
}

fn resource_issues(issues: Vec<ResourceManifestIssue>) -> Vec<ProjectManifestIssue> {
    issues
        .into_iter()
        .map(|issue| ProjectManifestIssue {
            path: issue.path,
            message: issue.message,
        })
        .collect()
}

fn issue(
    issues: &mut Vec<ProjectManifestIssue>,
    path: impl Into<String>,
    message: impl Into<String>,
) {
    issues.push(ProjectManifestIssue {
        path: path.into(),
        message: message.into(),
    });
}
