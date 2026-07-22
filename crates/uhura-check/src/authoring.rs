//! Checked authoring metadata projected from canonical source.
//!
//! This sidecar is compiler-owned and deliberately separate from executable
//! [`uhura_core::Program`] IR. Its targets reuse the semantic UI node IDs that
//! provenance and rendered views already expose, while metadata entry IDs are
//! stable under prose and source-coordinate edits.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use uhura_base::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthoringTargetClass {
    UiElement,
    IfBlock,
    EachBlock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthoringEntryClass {
    Annotation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthoringTarget {
    /// The target's existing semantic UI node ID.
    pub id: String,
    pub class: AuthoringTargetClass,
    pub file: String,
    pub span: Span,
    /// Package-qualified presentation owner, for example `app@1::Feed`.
    pub owner: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthoringEntry {
    /// Hash of target, metadata class, and target-local order.
    pub id: String,
    pub class: AuthoringEntryClass,
    pub kind: String,
    pub text: String,
    pub span: Span,
    pub target_id: String,
    pub order: u32,
}

impl AuthoringEntry {
    pub(crate) fn annotation(
        target_id: String,
        kind: String,
        text: String,
        span: Span,
        order: u32,
    ) -> Self {
        let class = AuthoringEntryClass::Annotation;
        let id = metadata_id(&target_id, class, order);
        Self {
            id,
            class,
            kind,
            text,
            span,
            target_id,
            order,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthoringProjection {
    pub targets: Vec<AuthoringTarget>,
    pub entries: Vec<AuthoringEntry>,
}

impl AuthoringProjection {
    pub(crate) fn append(&mut self, mut other: Self) {
        self.targets.append(&mut other.targets);
        self.entries.append(&mut other.entries);
    }

    pub(crate) fn canonicalize(&mut self) -> Result<(), String> {
        self.targets.sort_by(|left, right| {
            (
                left.file.as_bytes(),
                left.span.start,
                left.span.end,
                left.class,
                left.id.as_bytes(),
            )
                .cmp(&(
                    right.file.as_bytes(),
                    right.span.start,
                    right.span.end,
                    right.class,
                    right.id.as_bytes(),
                ))
        });

        let target_files = self
            .targets
            .iter()
            .map(|target| (target.id.as_str(), target.file.as_str()))
            .collect::<BTreeMap<_, _>>();
        self.entries.sort_by(|left, right| {
            (
                target_files.get(left.target_id.as_str()).copied(),
                left.span.start,
                left.order,
                left.id.as_bytes(),
            )
                .cmp(&(
                    target_files.get(right.target_id.as_str()).copied(),
                    right.span.start,
                    right.order,
                    right.id.as_bytes(),
                ))
        });
        self.validate()
    }

    /// Validate the closed target/entry contract before a host maps it to an
    /// editor protocol.
    pub fn validate(&self) -> Result<(), String> {
        let mut targets = BTreeMap::new();
        for target in &self.targets {
            if target.id.is_empty()
                || target.file.is_empty()
                || target.owner.is_empty()
                || target.label.is_empty()
                || target.span.is_empty()
            {
                return Err("authoring projection contains an invalid source target".into());
            }
            if targets.insert(target.id.as_str(), target).is_some() {
                return Err(format!("duplicate authoring target `{}`", target.id));
            }
        }

        let mut ids = BTreeSet::new();
        let mut next_order = BTreeMap::<&str, u32>::new();
        for entry in &self.entries {
            if entry.kind.is_empty() || entry.text.is_empty() || entry.span.is_empty() {
                return Err("authoring projection contains an invalid metadata entry".into());
            }
            if !ids.insert(entry.id.as_str()) {
                return Err(format!("duplicate authoring entry `{}`", entry.id));
            }
            let Some(target) = targets.get(entry.target_id.as_str()) else {
                return Err(format!(
                    "authoring entry `{}` references unknown target `{}`",
                    entry.id, entry.target_id
                ));
            };
            if entry.span.file != target.span.file {
                return Err(format!(
                    "authoring entry `{}` and target `{}` belong to different source files",
                    entry.id, entry.target_id
                ));
            }
            let expected = metadata_id(&entry.target_id, entry.class, entry.order);
            if entry.id != expected {
                return Err(format!(
                    "authoring entry `{}` has a non-canonical id",
                    entry.id
                ));
            }
            let expected_order = next_order.entry(entry.target_id.as_str()).or_default();
            if entry.order != *expected_order {
                return Err(format!(
                    "authoring entries for target `{}` are not contiguous from zero",
                    entry.target_id
                ));
            }
            *expected_order += 1;
        }
        Ok(())
    }
}

fn metadata_id(target_id: &str, class: AuthoringEntryClass, order: u32) -> String {
    uhura_base::hash_json(&serde_json::json!({
        "target": target_id,
        "class": class,
        "order": order,
    }))
}
