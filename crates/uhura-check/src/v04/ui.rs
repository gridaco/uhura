//! Structural adaptation and lexical activation for the Uhura 0.4 Web UI profile.
//!
//! The presentation checker and kernel IR are shared with the production
//! checker. This module only translates the source-shaped 0.4 UI tree and
//! enforces the profile's deliberately exact activation spelling.

use uhura_syntax::{ast, v04};

use super::{Adapter, ExprEnv};
use crate::diagnostic::{codes, error};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProfileUse {
    Exact,
    Aliased,
    Public,
    Other,
}

fn classify_profile_use(declaration: &v04::ast::UseDeclaration) -> ProfileUse {
    let v04::ast::ImportTree::Single { path, alias } = &declaration.tree else {
        return ProfileUse::Other;
    };
    let v04::ast::ImportRoot::Package(root) = &path.root else {
        return ProfileUse::Other;
    };
    if root.text != "uhura" || path.segments.len() != 1 || path.segments[0].text != "ui" {
        return ProfileUse::Other;
    }
    if declaration.visibility == v04::ast::Visibility::Public {
        ProfileUse::Public
    } else if alias.is_some() {
        ProfileUse::Aliased
    } else {
        ProfileUse::Exact
    }
}

/// Consume the standard profile import before ordinary package resolution.
///
/// `uhura::ui` is a standard root export rather than a package declaration.
/// Its use is inert and lexical, so it must not become an ordinary name
/// binding in the flattened checker module.
pub(super) fn handle_standard_profile_use(
    declaration: &v04::ast::UseDeclaration,
    diagnostics: &mut Vec<uhura_base::Diagnostic>,
) -> bool {
    match classify_profile_use(declaration) {
        ProfileUse::Exact => true,
        ProfileUse::Aliased => {
            diagnostics.push(error(
                codes::UI_NOT_ENABLED,
                "uhura-0.4/aliased-ui-profile",
                "the UI profile must be activated by exact unaliased `use uhura::ui;`; an alias does not activate UI",
                super::span(declaration.span),
            ));
            true
        }
        ProfileUse::Public => {
            diagnostics.push(error(
                codes::UI_NOT_ENABLED,
                "uhura-0.4/reexported-ui-profile",
                "the UI profile is lexical and cannot be re-exported; use private `use uhura::ui;` in every module containing UI",
                super::span(declaration.span),
            ));
            true
        }
        ProfileUse::Other => false,
    }
}

/// Validate activation independently in every authored logical module, then
/// enable the shared checker profile on the one synthetic lowering module.
///
/// The synthetic use is an implementation detail. The authored direct-use
/// checks above are the authority; flattening must never make activation
/// transitive between source modules.
pub(super) fn profile_activation(
    sources: &[v04::ast::Module],
    diagnostics: &mut Vec<uhura_base::Diagnostic>,
) -> Vec<ast::UseDecl> {
    let mut contains_any_ui = false;
    let mut representative = None;

    for source in sources {
        let ui_declarations = source
            .declarations
            .iter()
            .filter(|declaration| matches!(declaration.kind, v04::ast::DeclarationKind::Ui(_)))
            .collect::<Vec<_>>();
        contains_any_ui |= !ui_declarations.is_empty();

        let exact_uses = source
            .uses
            .iter()
            .filter(|declaration| classify_profile_use(declaration) == ProfileUse::Exact)
            .collect::<Vec<_>>();
        if representative.is_none() {
            representative = exact_uses.first().map(|declaration| declaration.span);
        }

        if !ui_declarations.is_empty() && exact_uses.is_empty() {
            for declaration in ui_declarations {
                diagnostics.push(error(
                    codes::UI_NOT_ENABLED,
                    "uhura-0.4/ui-without-direct-profile-use",
                    "this logical module contains a UI declaration but does not directly contain exact private `use uhura::ui;`",
                    super::span(declaration.span),
                ));
            }
        }
        for duplicate in exact_uses.iter().skip(1) {
            diagnostics.push(error(
                codes::DUPLICATE,
                "uhura-0.4/duplicate-ui-profile-use",
                "the UI profile is activated more than once in this logical module",
                super::span(duplicate.span),
            ));
        }
    }

    if !contains_any_ui {
        return Vec::new();
    }

    let span = representative
        .map(super::span)
        .unwrap_or_else(|| ast::SourceSpan::empty(0, 0));
    vec![ast::UseDecl {
        feature: ast::Spanned::new("ui".into(), span),
        span,
    }]
}

impl Adapter<'_> {
    pub(super) fn ui_declaration(&mut self, declaration: &v04::ast::UiDeclaration) -> ast::UiDecl {
        ast::UiDecl {
            name: self.name(&declaration.name),
            machine: self.ui_machine_name(&declaration.machine),
            binding: self.name(&declaration.observation),
            nodes: self.ui_nodes(&declaration.body.nodes),
        }
    }

    fn ui_machine_name(&mut self, path: &v04::ast::TypePath) -> ast::Name {
        let Some(segment) = path.segments.last() else {
            self.unsupported(path.span, "a UI binding requires one machine name");
            return ast::Spanned::new("Never".into(), self.span(path.span));
        };
        if path.segments.len() != 1 || !segment.arguments.is_empty() {
            self.unsupported(
                path.span,
                "a UI binding names one local or imported machine without type arguments",
            );
        }
        self.resolved_name(&segment.name)
    }

    fn ui_nodes(&mut self, nodes: &[v04::ast::UiNode]) -> Vec<ast::UiNode> {
        nodes.iter().flat_map(|node| self.ui_node(node)).collect()
    }

    fn ui_node(&mut self, node: &v04::ast::UiNode) -> Vec<ast::UiNode> {
        let span = self.span(node.span);
        let value = match &node.kind {
            v04::ast::UiNodeKind::Text(value) if value.raw.chars().all(char::is_whitespace) => {
                return Vec::new();
            }
            v04::ast::UiNodeKind::Text(value) => ast::UiNodeKind::Text(value.raw.clone()),
            // Markup comments are retained by the lossless syntax tree but are
            // nonsemantic presentation trivia, just like core comments.
            v04::ast::UiNodeKind::Comment(_) => return Vec::new(),
            v04::ast::UiNodeKind::Interpolation(value) => {
                ast::UiNodeKind::Interpolation(self.expr(value, &ExprEnv::default()))
            }
            v04::ast::UiNodeKind::Element(value) => {
                ast::UiNodeKind::Element(self.ui_element(value))
            }
            v04::ast::UiNodeKind::If(value) => return self.ui_if(value, node.span),
            v04::ast::UiNodeKind::Each(value) => ast::UiNodeKind::Each {
                source: self.expr(&value.source, &ExprEnv::default()),
                pattern: self.pattern(&value.pattern),
                key: self.expr(&value.key, &ExprEnv::default()),
                children: self.ui_nodes(&value.children),
            },
        };
        vec![ast::Spanned::new(value, span)]
    }

    fn ui_if(&mut self, value: &v04::ast::UiIf, source_span: v04::ast::Span) -> Vec<ast::UiNode> {
        let condition = self.expr(&value.condition, &ExprEnv::default());
        let Some(else_branch) = &value.else_branch else {
            return vec![ast::Spanned::new(
                ast::UiNodeKind::If {
                    condition,
                    children: self.ui_nodes(&value.then_branch),
                },
                self.span(source_span),
            )];
        };

        // The source-neutral kernel has a one-branch conditional. UI
        // expressions are pure projections, so two mutually exclusive
        // conditionals preserve source semantics. Keeping the positive
        // condition intact also preserves pattern refinements such as
        // `value is Some(item)` inside the then branch.
        let else_span = value.else_span.expect("an else branch has an else span");
        let then_source = self.span(value.open_span.through(else_span));
        let else_source = self.span(else_span.through(value.close_span));
        let negated = ast::Spanned::new(
            ast::ExprKind::Unary {
                op: ast::Spanned::new(ast::UnaryOp::Not, self.span(value.condition.span)),
                operand: Box::new(condition.clone()),
            },
            self.span(value.condition.span),
        );
        vec![
            ast::Spanned::new(
                ast::UiNodeKind::If {
                    condition,
                    children: self.ui_nodes(&value.then_branch),
                },
                then_source,
            ),
            ast::Spanned::new(
                ast::UiNodeKind::If {
                    condition: negated,
                    children: self.ui_nodes(else_branch),
                },
                else_source,
            ),
        ]
    }

    fn ui_element(&mut self, value: &v04::ast::UiElement) -> ast::UiElement {
        let name = match value.name.kind {
            v04::ast::UiNameKind::Native => value.name.text.clone(),
            v04::ast::UiNameKind::Component => self.resolved_text(&value.name.text).to_owned(),
        };
        ast::UiElement {
            // The checker-neutral tree uses resolved spelling: lowercase
            // native names remain native, while component-shaped names must
            // resolve to a checked presentation or standard UI element.
            name: ast::Spanned::new(name, self.span(value.name.span)),
            attributes: value
                .attributes
                .iter()
                .map(|attribute| self.ui_attribute(attribute))
                .collect(),
            children: self.ui_nodes(&value.children),
            self_closing: value.self_closing,
        }
    }

    fn ui_attribute(&mut self, value: &v04::ast::UiAttribute) -> ast::UiAttribute {
        match value {
            v04::ast::UiAttribute::Boolean { name, span } => ast::UiAttribute {
                name: name.text.clone(),
                value: ast::UiAttributeValue::Expression(ast::Spanned::new(
                    ast::ExprKind::Bool(true),
                    self.span(*span),
                )),
                span: self.span(*span),
            },
            v04::ast::UiAttribute::StaticText { name, value, span } => ast::UiAttribute {
                name: name.text.clone(),
                value: ast::UiAttributeValue::Text(value.clone()),
                span: self.span(*span),
            },
            v04::ast::UiAttribute::Expression { name, value, span } => ast::UiAttribute {
                name: name.text.clone(),
                value: ast::UiAttributeValue::Expression(self.expr(value, &ExprEnv::default())),
                span: self.span(*span),
            },
            v04::ast::UiAttribute::Event { event, input, span } => ast::UiAttribute {
                name: "on".into(),
                value: ast::UiAttributeValue::Event {
                    event: ast::Spanned::new(event.text.clone(), self.span(event.span)),
                    input: self.expr(input, &ExprEnv::default()),
                },
                span: self.span(*span),
            },
        }
    }
}
