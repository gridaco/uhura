//! Tooling-only structural addresses and template-realization trace data.
//!
//! These types deliberately have no serde implementation. They describe a
//! checked template and one evaluation of it without becoming part of either
//! the `uhura-ir/0` or `uhura-view/0` wire protocols.

use uhura_base::Ident;

use crate::ir::NodeIr;

/// The definition table containing a template root.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DefinitionKind {
    Page,
    Component,
    Surface,
}

/// A program-local definition identity.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DefinitionAddress {
    pub kind: DefinitionKind,
    pub name: Ident,
}

impl DefinitionAddress {
    pub fn new(kind: DefinitionKind, name: Ident) -> Self {
        Self { kind, name }
    }
}

/// One structural step from a template node to a nested template node.
///
/// Indexes are source-order indexes within the named list. In particular,
/// they do not use runtime node keys or `ElementIr::ord`: blocks have no
/// ordinals, and runtime expansion can repeat or flatten their children.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TemplateSegment {
    ElementChild { index: usize },
    IfThen { index: usize },
    IfElse { index: usize },
    EachBody { index: usize },
    MatchArm { arm: usize, child: usize },
}

/// A comment-insensitive address of one `NodeIr` in one definition.
///
/// The definition root has an empty `path`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TemplateAddress {
    pub definition: DefinitionAddress,
    pub path: Vec<TemplateSegment>,
}

impl TemplateAddress {
    pub fn root(definition: DefinitionAddress) -> Self {
        Self {
            definition,
            path: Vec::new(),
        }
    }

    pub fn child(&self, segment: TemplateSegment) -> Self {
        let mut path = self.path.clone();
        path.push(segment);
        Self {
            definition: self.definition.clone(),
            path,
        }
    }
}

/// Visits every template operation exactly once in structural source order.
///
/// This is the shared coverage rule for tooling sidecars: an address built by
/// a lowerer can be compared with this walk without relying on serialized IR
/// ordinals. The visitor sees a parent before its descendants.
pub fn walk_template<'a>(
    definition: &DefinitionAddress,
    root: &'a NodeIr,
    mut visitor: impl FnMut(&TemplateAddress, &'a NodeIr),
) {
    fn walk<'a>(
        address: TemplateAddress,
        node: &'a NodeIr,
        visitor: &mut impl FnMut(&TemplateAddress, &'a NodeIr),
    ) {
        visitor(&address, node);
        match node {
            NodeIr::Element(element) => {
                for (index, child) in element.children.iter().enumerate() {
                    walk(
                        address.child(TemplateSegment::ElementChild { index }),
                        child,
                        visitor,
                    );
                }
            }
            NodeIr::Component(_) => {}
            NodeIr::If { then, els, .. } => {
                for (index, child) in then.iter().enumerate() {
                    walk(
                        address.child(TemplateSegment::IfThen { index }),
                        child,
                        visitor,
                    );
                }
                for (index, child) in els.iter().enumerate() {
                    walk(
                        address.child(TemplateSegment::IfElse { index }),
                        child,
                        visitor,
                    );
                }
            }
            NodeIr::Each(each) => {
                for (index, child) in each.body.iter().enumerate() {
                    walk(
                        address.child(TemplateSegment::EachBody { index }),
                        child,
                        visitor,
                    );
                }
            }
            NodeIr::Match(matched) => {
                for (arm, branch) in matched.arms.iter().enumerate() {
                    for (child, node) in branch.body.iter().enumerate() {
                        walk(
                            address.child(TemplateSegment::MatchArm { arm, child }),
                            node,
                            visitor,
                        );
                    }
                }
            }
        }
    }

    walk(
        TemplateAddress::root(definition.clone()),
        root,
        &mut visitor,
    );
}

/// The semantic root slot containing a realized node.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RenderRoot {
    Page,
    Fragment,
    /// The semantic `SurfaceView::key`, not a surface array index.
    Surface {
        key: String,
    },
}

/// One node in the final semantic view tree.
///
/// `path` contains semantic child indexes. The root node has an empty path;
/// renderer-created mechanics never enter this address space.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RenderNodeRef {
    pub root: RenderRoot,
    pub path: Vec<usize>,
}

/// A dynamic nesting step which can cause one template operation to be
/// evaluated more than once in a render root.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EvaluationContextSegment {
    /// Evaluation entered the named component through this call operation.
    ComponentCall { call: TemplateAddress },
    /// Evaluation entered one keyed iteration of this each operation.
    EachItem { each: TemplateAddress, key: String },
}

/// Deterministic dynamic identity within one render root.
///
/// A root definition starts with an empty context. Component-call addresses
/// and keyed each items make reused components, repeated descendants, and
/// repeated zero-anchor blocks distinct without depending on encounter
/// counters.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EvaluationContext {
    pub segments: Vec<EvaluationContextSegment>,
}

impl EvaluationContext {
    pub fn child(&self, segment: EvaluationContextSegment) -> Self {
        let mut segments = self.segments.clone();
        segments.push(segment);
        Self { segments }
    }
}

/// One dynamic evaluation of one structural template operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvaluationOccurrence {
    pub template: TemplateAddress,
    pub root: RenderRoot,
    pub context: EvaluationContext,
    /// Top-level semantic nodes realized by this operation. Evaluated empty
    /// blocks retain an occurrence with an empty anchor list.
    pub anchors: Vec<RenderNodeRef>,
}

/// Tooling-neutral realization information for one evaluated preview.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EvaluationTrace {
    pub occurrences: Vec<EvaluationOccurrence>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{EachIr, ElementIr, ExprIr, MatchArmIr, MatchIr, MatchSourceIr, OverIr};

    fn ident(value: &str) -> Ident {
        Ident::new(value).unwrap()
    }

    fn element(children: Vec<NodeIr>) -> NodeIr {
        NodeIr::Element(ElementIr {
            element: ident("view"),
            ord: 0,
            class: None,
            props: vec![],
            events: vec![],
            text: vec![],
            children,
        })
    }

    #[test]
    fn walker_covers_every_structural_list_with_typed_segments() {
        let root = element(vec![
            NodeIr::If {
                cond: ExprIr::Bool(true),
                then: vec![element(vec![])],
                els: vec![element(vec![])],
            },
            NodeIr::Each(EachIr {
                ord: 4,
                item: ident("item"),
                over: OverIr::List,
                seq: ExprIr::RecordLit(vec![]),
                key: ExprIr::Int(0),
                body: vec![element(vec![])],
            }),
            NodeIr::Match(MatchIr {
                source: MatchSourceIr::Union {
                    value: ExprIr::RecordLit(vec![]),
                },
                arms: vec![MatchArmIr {
                    variant: None,
                    binding: None,
                    body: vec![element(vec![])],
                }],
            }),
        ]);
        let definition = DefinitionAddress::new(DefinitionKind::Page, ident("home"));
        let mut addresses = Vec::new();
        walk_template(&definition, &root, |address, _| {
            addresses.push(address.clone());
        });

        assert_eq!(addresses.len(), 8);
        assert_eq!(addresses[0], TemplateAddress::root(definition.clone()));
        assert_eq!(
            addresses[2].path,
            vec![
                TemplateSegment::ElementChild { index: 0 },
                TemplateSegment::IfThen { index: 0 },
            ]
        );
        assert_eq!(
            addresses[3].path,
            vec![
                TemplateSegment::ElementChild { index: 0 },
                TemplateSegment::IfElse { index: 0 },
            ]
        );
        assert_eq!(
            addresses[5].path,
            vec![
                TemplateSegment::ElementChild { index: 1 },
                TemplateSegment::EachBody { index: 0 },
            ]
        );
        assert_eq!(
            addresses[7].path,
            vec![
                TemplateSegment::ElementChild { index: 2 },
                TemplateSegment::MatchArm { arm: 0, child: 0 },
            ]
        );
    }
}
