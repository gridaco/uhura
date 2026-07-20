//! Source-layout-sensitive provenance for source-layout-independent semantic nodes.
//!
//! This artifact is deliberately separate from [`crate::Program`]. A source
//! move, reformat, or comment edit may change this sidecar without changing a
//! machine-program identity, checkpoint, receipt, or runtime value.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::graph::{
    InteractionGraphArtifacts, InteractionGraphEdge, InteractionGraphEdgeProvenance,
    InteractionGraphNode, InteractionGraphNodeProvenance,
};
use crate::ir::SourceRef;

pub const PROVENANCE_PROTOCOL: &str = "uhura-provenance/0";
pub const NODE_ID_PROTOCOL: &str = "uhura-node/0";
pub const SOURCE_REVISION_ID_PROTOCOL: &str = "uhura-source-revision/0";
pub const AUTHORED_INTERACTION_TOPOLOGY_PROTOCOL: &str = "uhura-authored-interaction-topology/0";

/// Physical source inventory plus semantic-node occurrences for one checked
/// project revision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    pub protocol: String,
    pub sources: Vec<ProvenanceSource>,
    pub occurrences: Vec<ProvenanceOccurrence>,
    /// Source-language ownership and dependency facts erased by lowering.
    ///
    /// This stays outside [`crate::Program`], so it can improve inspection
    /// without creating a second runtime IR or changing machine identity.
    pub topology: AuthoredInteractionTopology,
}

/// One captured UTF-8 source file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProvenanceSource {
    /// Revision-local, contiguous source-table index.
    pub source: u32,
    pub package: String,
    pub module: String,
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

/// One authored or generated occurrence of a stable semantic node.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProvenanceOccurrence {
    pub node: String,
    pub source: u32,
    pub start: u32,
    pub end: u32,
    pub role: String,
    pub owner: String,
}

/// The authored overlay merged into the runtime-derived interaction graph.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuthoredInteractionTopology {
    pub protocol: String,
    pub nodes: Vec<AuthoredInteractionNode>,
    pub edges: Vec<AuthoredInteractionEdge>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuthoredInteractionNode {
    #[serde(flatten)]
    pub node: InteractionGraphNode,
    pub sources: Vec<ProvenanceSelector>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuthoredInteractionEdge {
    #[serde(flatten)]
    pub edge: InteractionGraphEdge,
    pub sources: Vec<ProvenanceSelector>,
}

/// Selects one or more exact physical occurrences without embedding source
/// coordinates in semantic graph identity.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ProvenanceSelector {
    pub node: String,
    pub role: String,
    pub owner: String,
}

impl Default for AuthoredInteractionTopology {
    fn default() -> Self {
        Self {
            protocol: AUTHORED_INTERACTION_TOPOLOGY_PROTOCOL.into(),
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }
}

impl Provenance {
    /// Construct a canonical sidecar.
    ///
    /// Source indices are semantic only inside this revision-local artifact,
    /// so callers provide them explicitly. Occurrences are sorted by stable
    /// node identity, package-qualified physical source identity, byte range,
    /// role, and owner.
    pub fn canonical(
        sources: Vec<ProvenanceSource>,
        occurrences: Vec<ProvenanceOccurrence>,
    ) -> Result<Self, String> {
        Self::canonical_with_topology(sources, occurrences, AuthoredInteractionTopology::default())
    }

    /// Construct a canonical sidecar with source-language interaction facts.
    pub fn canonical_with_topology(
        mut sources: Vec<ProvenanceSource>,
        mut occurrences: Vec<ProvenanceOccurrence>,
        mut topology: AuthoredInteractionTopology,
    ) -> Result<Self, String> {
        sources.sort_by_key(|source| source.source);
        let source_keys = sources
            .iter()
            .map(|source| {
                (
                    source.source,
                    (
                        source.package.clone(),
                        source.module.clone(),
                        source.path.clone(),
                    ),
                )
            })
            .collect::<BTreeMap<_, _>>();
        occurrences.sort_by(|left, right| {
            (
                &left.node,
                source_keys.get(&left.source),
                left.start,
                left.end,
                &left.role,
                &left.owner,
            )
                .cmp(&(
                    &right.node,
                    source_keys.get(&right.source),
                    right.start,
                    right.end,
                    &right.role,
                    &right.owner,
                ))
        });
        occurrences.dedup();
        topology.canonicalize()?;

        let value = Self {
            protocol: PROVENANCE_PROTOCOL.into(),
            sources,
            occurrences,
            topology,
        };
        value.validate()?;
        Ok(value)
    }

    /// Validate the closed persisted contract and every byte range.
    pub fn validate(&self) -> Result<(), String> {
        if self.protocol != PROVENANCE_PROTOCOL {
            return Err(format!(
                "expected provenance protocol `{PROVENANCE_PROTOCOL}`, found `{}`",
                self.protocol
            ));
        }

        let mut source_bytes = BTreeMap::new();
        let mut source_keys = BTreeMap::new();
        let mut seen_paths = BTreeSet::new();
        for (expected, source) in self.sources.iter().enumerate() {
            if source.source as usize != expected {
                return Err(format!(
                    "provenance source indices must be contiguous from zero; expected {expected}, found {}",
                    source.source
                ));
            }
            if source.package.is_empty()
                || source.module.is_empty()
                || source.path.is_empty()
                || source.path.starts_with('/')
                || source.path.contains('\\')
                || source
                    .path
                    .split('/')
                    .any(|part| part.is_empty() || matches!(part, "." | ".."))
            {
                return Err(format!(
                    "provenance source {} has an invalid package, module, or project-relative path",
                    source.source
                ));
            }
            if !is_lower_sha256(&source.sha256) {
                return Err(format!(
                    "provenance source {} has an invalid SHA-256 digest",
                    source.source
                ));
            }
            if !seen_paths.insert((source.package.as_str(), source.path.as_str())) {
                return Err(format!(
                    "provenance source path `{}` occurs more than once in package `{}`",
                    source.path, source.package
                ));
            }
            source_bytes.insert(source.source, source.bytes);
            source_keys.insert(
                source.source,
                (
                    source.package.as_str(),
                    source.module.as_str(),
                    source.path.as_str(),
                ),
            );
        }

        let mut previous = None;
        for occurrence in &self.occurrences {
            if !is_lower_sha256(&occurrence.node) {
                return Err(format!(
                    "provenance occurrence has an invalid semantic node `{}`",
                    occurrence.node
                ));
            }
            let Some(bytes) = source_bytes.get(&occurrence.source) else {
                return Err(format!(
                    "provenance occurrence references unknown source {}",
                    occurrence.source
                ));
            };
            if occurrence.start > occurrence.end || u64::from(occurrence.end) > *bytes {
                return Err(format!(
                    "provenance occurrence {} has byte range {}..{} outside source {}",
                    occurrence.node, occurrence.start, occurrence.end, occurrence.source
                ));
            }
            if !valid_role(&occurrence.role) {
                return Err(format!(
                    "provenance occurrence {} has invalid role `{}`",
                    occurrence.node, occurrence.role
                ));
            }
            if !valid_owner(&occurrence.owner) {
                return Err(format!(
                    "provenance occurrence {} has invalid owner `{}`",
                    occurrence.node, occurrence.owner
                ));
            }

            let order = (
                occurrence.node.as_str(),
                source_keys
                    .get(&occurrence.source)
                    .copied()
                    .expect("validated source exists"),
                occurrence.start,
                occurrence.end,
                occurrence.role.as_str(),
                occurrence.owner.as_str(),
            );
            if previous.as_ref().is_some_and(|previous| previous >= &order) {
                return Err("provenance occurrences must be unique and canonically ordered".into());
            }
            previous = Some(order);
        }
        self.topology.validate(&self.occurrences)?;
        Ok(())
    }

    pub fn to_canonical_string(&self) -> String {
        uhura_base::to_canonical_json(
            &serde_json::to_value(self).expect("validated provenance serializes"),
        )
    }
}

impl AuthoredInteractionTopology {
    pub fn canonical(
        nodes: Vec<AuthoredInteractionNode>,
        edges: Vec<AuthoredInteractionEdge>,
    ) -> Result<Self, String> {
        let mut value = Self {
            protocol: AUTHORED_INTERACTION_TOPOLOGY_PROTOCOL.into(),
            nodes,
            edges,
        };
        value.canonicalize()?;
        Ok(value)
    }

    fn canonicalize(&mut self) -> Result<(), String> {
        if self.protocol != AUTHORED_INTERACTION_TOPOLOGY_PROTOCOL {
            return Err(format!(
                "expected authored interaction topology protocol `{AUTHORED_INTERACTION_TOPOLOGY_PROTOCOL}`, found `{}`",
                self.protocol
            ));
        }
        for node in &mut self.nodes {
            node.sources.sort();
            node.sources.dedup();
        }
        self.nodes.sort_by(|left, right| left.node.cmp(&right.node));
        let mut merged_nodes = Vec::<AuthoredInteractionNode>::new();
        for node in std::mem::take(&mut self.nodes) {
            if let Some(previous) = merged_nodes.last_mut()
                && previous.node.id == node.node.id
            {
                if previous.node != node.node {
                    return Err(format!(
                        "authored topology node `{}` has conflicting definitions",
                        node.node.id
                    ));
                }
                previous.sources.extend(node.sources);
                previous.sources.sort();
                previous.sources.dedup();
            } else {
                merged_nodes.push(node);
            }
        }
        self.nodes = merged_nodes;

        for edge in &mut self.edges {
            edge.sources.sort();
            edge.sources.dedup();
        }
        self.edges.sort_by(|left, right| left.edge.cmp(&right.edge));
        let mut merged_edges = Vec::<AuthoredInteractionEdge>::new();
        for edge in std::mem::take(&mut self.edges) {
            if let Some(previous) = merged_edges.last_mut()
                && previous.edge == edge.edge
            {
                previous.sources.extend(edge.sources);
                previous.sources.sort();
                previous.sources.dedup();
            } else {
                merged_edges.push(edge);
            }
        }
        self.edges = merged_edges;
        Ok(())
    }

    fn validate(&self, occurrences: &[ProvenanceOccurrence]) -> Result<(), String> {
        if self.protocol != AUTHORED_INTERACTION_TOPOLOGY_PROTOCOL {
            return Err(format!(
                "expected authored interaction topology protocol `{AUTHORED_INTERACTION_TOPOLOGY_PROTOCOL}`, found `{}`",
                self.protocol
            ));
        }
        let mut node_ids = BTreeSet::new();
        let mut previous_node = None;
        for entry in &self.nodes {
            if entry.node.id.is_empty()
                || entry.node.machine.is_empty()
                || entry.node.label.is_empty()
                || entry.sources.is_empty()
            {
                return Err("authored topology nodes require identity, machine, label, and source selectors".into());
            }
            if previous_node
                .as_ref()
                .is_some_and(|previous| previous >= &entry.node)
            {
                return Err(
                    "authored topology nodes must be unique and canonically ordered".into(),
                );
            }
            previous_node = Some(entry.node.clone());
            if !node_ids.insert(entry.node.id.as_str()) {
                return Err(format!(
                    "authored topology node `{}` occurs more than once",
                    entry.node.id
                ));
            }
            validate_selectors(&entry.sources, occurrences)?;
        }

        let mut previous_edge = None;
        for entry in &self.edges {
            if !node_ids.contains(entry.edge.from.as_str())
                || !node_ids.contains(entry.edge.to.as_str())
                || entry.sources.is_empty()
            {
                return Err(format!(
                    "authored topology edge `{} -> {}` must reference declared nodes and source selectors",
                    entry.edge.from, entry.edge.to
                ));
            }
            if previous_edge
                .as_ref()
                .is_some_and(|previous| previous >= &entry.edge)
            {
                return Err(
                    "authored topology edges must be unique and canonically ordered".into(),
                );
            }
            previous_edge = Some(entry.edge.clone());
            validate_selectors(&entry.sources, occurrences)?;
        }
        Ok(())
    }
}

fn validate_selectors(
    selectors: &[ProvenanceSelector],
    occurrences: &[ProvenanceOccurrence],
) -> Result<(), String> {
    let mut previous = None;
    for selector in selectors {
        if !is_lower_sha256(&selector.node)
            || !valid_role(&selector.role)
            || !valid_owner(&selector.owner)
        {
            return Err("authored topology contains an invalid provenance selector".into());
        }
        if previous
            .as_ref()
            .is_some_and(|previous| *previous >= selector)
        {
            return Err(
                "authored topology selectors must be unique and canonically ordered".into(),
            );
        }
        previous = Some(selector);
        if !occurrences.iter().any(|occurrence| {
            occurrence.node == selector.node
                && occurrence.role == selector.role
                && occurrence.owner == selector.owner
        }) {
            return Err(format!(
                "authored topology selector `{}` ({}/{}) has no provenance occurrence",
                selector.node, selector.role, selector.owner
            ));
        }
    }
    Ok(())
}

/// Merge checker-owned authored topology into the graph derived from the one
/// runtime IR, retaining a complete physical source projection.
pub fn merge_authored_interaction_topology(
    artifacts: &mut InteractionGraphArtifacts,
    provenance: &Provenance,
) -> Result<(), String> {
    provenance.validate()?;

    let mut nodes = artifacts
        .graph
        .nodes
        .iter()
        .cloned()
        .map(|node| (node.id.clone(), node))
        .collect::<BTreeMap<_, _>>();
    let mut node_sources = artifacts
        .provenance
        .nodes
        .iter()
        .cloned()
        .map(|entry| (entry.node.clone(), entry.sources))
        .collect::<BTreeMap<_, _>>();

    for entry in &provenance.topology.nodes {
        match nodes.get(&entry.node.id) {
            Some(existing) if existing != &entry.node => {
                return Err(format!(
                    "authored topology conflicts with runtime graph node `{}`",
                    entry.node.id
                ));
            }
            Some(_) => {}
            None => {
                nodes.insert(entry.node.id.clone(), entry.node.clone());
            }
        }
        let resolved = resolve_selectors(provenance, &entry.sources)?;
        let sources = node_sources.entry(entry.node.id.clone()).or_default();
        for source in resolved {
            push_source(sources, source);
        }
    }

    let mut edges = artifacts
        .graph
        .edges
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut edge_sources = artifacts
        .provenance
        .edges
        .iter()
        .cloned()
        .map(|entry| (entry.edge, entry.sources))
        .collect::<BTreeMap<_, _>>();
    for entry in &provenance.topology.edges {
        if !nodes.contains_key(&entry.edge.from) || !nodes.contains_key(&entry.edge.to) {
            return Err(format!(
                "authored topology edge `{} -> {}` is not closed",
                entry.edge.from, entry.edge.to
            ));
        }
        edges.insert(entry.edge.clone());
        let resolved = resolve_selectors(provenance, &entry.sources)?;
        let sources = edge_sources.entry(entry.edge.clone()).or_default();
        for source in resolved {
            push_source(sources, source);
        }
    }

    artifacts.graph.nodes = nodes.into_values().collect();
    artifacts.graph.edges = edges.into_iter().collect();
    artifacts.provenance.nodes = node_sources
        .into_iter()
        .map(|(node, sources)| InteractionGraphNodeProvenance { node, sources })
        .collect();
    artifacts.provenance.edges = edge_sources
        .into_iter()
        .map(|(edge, sources)| InteractionGraphEdgeProvenance { edge, sources })
        .collect();
    retain_physical_graph_sources(&mut artifacts.provenance, provenance)?;
    Ok(())
}

fn retain_physical_graph_sources(
    graph: &mut crate::graph::InteractionGraphProvenance,
    provenance: &Provenance,
) -> Result<(), String> {
    let source_bytes =
        provenance
            .sources
            .iter()
            .fold(BTreeMap::<&str, Vec<u64>>::new(), |mut paths, source| {
                paths
                    .entry(source.path.as_str())
                    .or_default()
                    .push(source.bytes);
                paths
            });
    let retain = |sources: &mut Vec<SourceRef>| {
        sources.retain(|source| {
            source.start <= source.end
                && source_bytes
                    .get(source.path.as_str())
                    .is_some_and(|limits| {
                        limits.iter().any(|bytes| u64::from(source.end) <= *bytes)
                    })
        });
    };
    for entry in &mut graph.nodes {
        retain(&mut entry.sources);
        if entry.sources.is_empty() {
            return Err(format!(
                "interaction graph node `{}` has no physical semantic-provenance source",
                entry.node
            ));
        }
    }
    for entry in &mut graph.edges {
        retain(&mut entry.sources);
        if entry.sources.is_empty() {
            return Err(format!(
                "interaction graph edge `{} -> {}` has no physical semantic-provenance source",
                entry.edge.from, entry.edge.to
            ));
        }
    }
    Ok(())
}

fn resolve_selectors(
    provenance: &Provenance,
    selectors: &[ProvenanceSelector],
) -> Result<Vec<SourceRef>, String> {
    let paths = provenance
        .sources
        .iter()
        .map(|source| (source.source, source.path.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut sources = Vec::new();
    for selector in selectors {
        for occurrence in provenance.occurrences.iter().filter(|occurrence| {
            occurrence.node == selector.node
                && occurrence.role == selector.role
                && occurrence.owner == selector.owner
        }) {
            let path = paths.get(&occurrence.source).ok_or_else(|| {
                format!(
                    "authored topology selector references unknown source {}",
                    occurrence.source
                )
            })?;
            push_source(
                &mut sources,
                SourceRef {
                    id: selector.node.clone(),
                    path: (*path).to_string(),
                    start: occurrence.start,
                    end: occurrence.end,
                },
            );
        }
    }
    if sources.is_empty() {
        return Err("authored topology selector resolved no physical source".into());
    }
    Ok(sources)
}

fn push_source(sources: &mut Vec<SourceRef>, source: SourceRef) {
    if !sources.contains(&source) {
        sources.push(source);
        sources.sort_by(|left, right| {
            (&left.path, left.start, left.end, &left.id).cmp(&(
                &right.path,
                right.start,
                right.end,
                &right.id,
            ))
        });
    }
}

/// Stable semantic node identity. None of the inputs may be a physical source
/// locator or byte coordinate.
#[must_use]
pub fn semantic_node_id(
    public_owner: &str,
    composition_owner: &str,
    kind: &str,
    semantic_path: &str,
) -> String {
    let mut bytes = NODE_ID_PROTOCOL.as_bytes().to_vec();
    for field in [public_owner, composition_owner, kind, semantic_path] {
        bytes.extend_from_slice(&(field.len() as u64).to_be_bytes());
        bytes.extend_from_slice(field.as_bytes());
    }
    uhura_base::sha256_hex(&bytes)
}

/// Physical identity of one coherently captured project input set.
///
/// Paths and raw bytes intentionally participate. Input ordering does not.
pub fn source_revision_id<'a>(
    case_insensitive: bool,
    files: impl IntoIterator<Item = (&'a str, &'a [u8])>,
) -> Result<String, String> {
    let mut files = files.into_iter().collect::<Vec<_>>();
    files.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    for window in files.windows(2) {
        if window[0].0 == window[1].0 {
            return Err(format!(
                "source revision contains duplicate path `{}`",
                window[0].0
            ));
        }
    }

    let mut bytes = SOURCE_REVISION_ID_PROTOCOL.as_bytes().to_vec();
    append_frame_field(
        &mut bytes,
        if case_insensitive {
            b"case-insensitive"
        } else {
            b"case-sensitive"
        },
    );
    for (path, contents) in files {
        if path.is_empty()
            || path.starts_with('/')
            || path.contains('\\')
            || path
                .split('/')
                .any(|part| part.is_empty() || matches!(part, "." | ".."))
        {
            return Err(format!(
                "source revision path `{path}` is not project-relative"
            ));
        }
        let mut pair = b"source".to_vec();
        append_frame_field(&mut pair, path.as_bytes());
        append_frame_field(&mut pair, contents);
        append_frame_field(&mut bytes, &pair);
    }
    Ok(uhura_base::sha256_hex(&bytes))
}

fn append_frame_field(target: &mut Vec<u8>, field: &[u8]) {
    target.extend_from_slice(&(field.len() as u64).to_be_bytes());
    target.extend_from_slice(field);
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_role(role: &str) -> bool {
    matches!(role, "definition" | "reference" | "generated")
        || role.split_once('/').is_some_and(|(profile, versioned)| {
            lower_kebab(profile)
                && versioned.split_once(':').is_some_and(|(version, name)| {
                    !version.is_empty()
                        && version.bytes().all(|byte| byte.is_ascii_digit())
                        && lower_kebab(name)
                })
        })
}

fn valid_owner(owner: &str) -> bool {
    owner == "root" || owner.split('.').all(lower_snake)
}

fn lower_snake(value: &str) -> bool {
    let Some(first) = value.as_bytes().first() else {
        return false;
    };
    first.is_ascii_lowercase()
        && value
            .bytes()
            .skip(1)
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn lower_kebab(value: &str) -> bool {
    let Some(first) = value.as_bytes().first() else {
        return false;
    };
    first.is_ascii_lowercase()
        && value
            .bytes()
            .skip(1)
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !value.ends_with('-')
        && !value.contains("--")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(path: &str, bytes: u64) -> ProvenanceSource {
        ProvenanceSource {
            source: 0,
            package: "examples.programs@1".into(),
            module: "programs".into(),
            path: path.into(),
            sha256: "a".repeat(64),
            bytes,
        }
    }

    #[test]
    fn node_identity_excludes_physical_source_coordinates() {
        let first = semantic_node_id(
            "examples.programs@1::Counter",
            "root",
            "state",
            "state/count/initial",
        );
        let second = semantic_node_id(
            "examples.programs@1::Counter",
            "root",
            "state",
            "state/count/initial",
        );
        assert_eq!(first, second);
        assert!(is_lower_sha256(&first));
    }

    #[test]
    fn canonical_sidecar_sorts_by_node_then_source_path_and_range() {
        let a = semantic_node_id("example@1::M", "root", "state", "state/a");
        let b = semantic_node_id("example@1::M", "root", "state", "state/b");
        let sidecar = Provenance::canonical(
            vec![source("machine.uhura", 40)],
            vec![
                ProvenanceOccurrence {
                    node: b.clone(),
                    source: 0,
                    start: 20,
                    end: 21,
                    role: "definition".into(),
                    owner: "root".into(),
                },
                ProvenanceOccurrence {
                    node: a.clone(),
                    source: 0,
                    start: 10,
                    end: 11,
                    role: "reference".into(),
                    owner: "root".into(),
                },
            ],
        )
        .unwrap();
        let mut expected = vec![a, b];
        expected.sort();
        assert_eq!(
            sidecar
                .occurrences
                .iter()
                .map(|occurrence| occurrence.node.as_str())
                .collect::<Vec<_>>(),
            expected
        );
        sidecar.validate().unwrap();
    }

    #[test]
    fn source_paths_are_unique_within_but_not_across_packages() {
        let root = source("main.uhura", 40);
        let dependency = ProvenanceSource {
            source: 1,
            package: "vendor.parts@1".into(),
            module: "main".into(),
            path: "main.uhura".into(),
            sha256: "b".repeat(64),
            bytes: 20,
        };
        Provenance::canonical(vec![root.clone(), dependency], Vec::new())
            .expect("different packages may use the same relative path");

        let duplicate = ProvenanceSource {
            source: 1,
            module: "other".into(),
            ..root.clone()
        };
        let failure = Provenance::canonical(vec![root, duplicate], Vec::new())
            .expect_err("one package cannot assign the same path twice");
        assert!(failure.contains("more than once in package"));
    }

    #[test]
    fn closed_validation_rejects_unknown_sources_ranges_and_roles() {
        let node = semantic_node_id("example@1::M", "root", "state", "state/a");
        for occurrence in [
            ProvenanceOccurrence {
                node: node.clone(),
                source: 1,
                start: 0,
                end: 1,
                role: "definition".into(),
                owner: "root".into(),
            },
            ProvenanceOccurrence {
                node: node.clone(),
                source: 0,
                start: 2,
                end: 50,
                role: "definition".into(),
                owner: "root".into(),
            },
            ProvenanceOccurrence {
                node: node.clone(),
                source: 0,
                start: 0,
                end: 1,
                role: "invented".into(),
                owner: "root".into(),
            },
        ] {
            assert!(Provenance::canonical(vec![source("m.uhura", 4)], vec![occurrence]).is_err());
        }
    }

    #[test]
    fn source_revision_is_order_independent_but_path_content_and_case_mode_sensitive() {
        let first = source_revision_id(
            false,
            [
                ("machine.uhura", b"machine".as_slice()),
                ("uhura.toml", b"manifest".as_slice()),
            ],
        )
        .unwrap();
        let reordered = source_revision_id(
            false,
            [
                ("uhura.toml", b"manifest".as_slice()),
                ("machine.uhura", b"machine".as_slice()),
            ],
        )
        .unwrap();
        assert_eq!(first, reordered);
        assert_ne!(
            first,
            source_revision_id(
                false,
                [
                    ("moved.uhura", b"machine".as_slice()),
                    ("uhura.toml", b"manifest".as_slice()),
                ],
            )
            .unwrap()
        );
        assert_ne!(
            first,
            source_revision_id(
                false,
                [
                    ("machine.uhura", b"changed".as_slice()),
                    ("uhura.toml", b"manifest".as_slice()),
                ],
            )
            .unwrap()
        );
        assert_ne!(
            first,
            source_revision_id(
                true,
                [
                    ("machine.uhura", b"machine".as_slice()),
                    ("uhura.toml", b"manifest".as_slice()),
                ],
            )
            .unwrap()
        );
    }
}
