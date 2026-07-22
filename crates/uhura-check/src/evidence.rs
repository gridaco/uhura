//! Project-role validation for headerless evidence modules.
//!
//! Evidence shares the 0.4 lexer, parser, expression language, imports, and
//! source identity with core modules. The manifest role is the capability
//! boundary: evidence may prove and name reachable states, but it cannot add
//! deployable declarations.

use crate::checker_ir::SourceSpan;
use uhura_base::Diagnostic;

use crate::diagnostic::{codes, error};

pub(crate) fn validate(
    core_modules: &[uhura_syntax::ast::Module],
    evidence_modules: &[uhura_syntax::ast::Module],
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for module in core_modules {
        for declaration in &module.declarations {
            if is_evidence(&declaration.kind) {
                diagnostics.push(error(
                    codes::EVIDENCE,
                    "uhura-0.4/evidence-outside-manifest-role",
                    "`scenario`, `example`, and `checkpoint` declarations belong in a module mapped by `[evidence.modules]`",
                    span(declaration.span),
                ));
            }
        }
    }

    for module in evidence_modules {
        validate_imports(module, &mut diagnostics);
        for declaration in &module.declarations {
            if !is_evidence(&declaration.kind) {
                diagnostics.push(error(
                    codes::EVIDENCE,
                    "uhura-0.4/core-declaration-in-evidence",
                    "an evidence module may declare only scenarios, checkpoints, and examples; move deployable declarations into `[modules]`",
                    span(declaration.span),
                ));
            }
        }
    }

    diagnostics.sort_by_key(|diagnostic| {
        (
            diagnostic.span.file.0,
            diagnostic.span.start,
            diagnostic.span.end,
            diagnostic.code,
            diagnostic.rule,
        )
    });
    diagnostics
}

fn is_evidence(declaration: &uhura_syntax::ast::DeclarationKind) -> bool {
    matches!(
        declaration,
        uhura_syntax::ast::DeclarationKind::Scenario(_)
            | uhura_syntax::ast::DeclarationKind::Example(_)
            | uhura_syntax::ast::DeclarationKind::Checkpoint(_)
    )
}

fn validate_imports(module: &uhura_syntax::ast::Module, diagnostics: &mut Vec<Diagnostic>) {
    for declaration in &module.uses {
        if declaration.visibility == uhura_syntax::ast::Visibility::Public {
            diagnostics.push(error(
                codes::IMPORT,
                "uhura-0.4/evidence-reexport",
                "evidence modules cannot re-export declarations",
                span(declaration.span),
            ));
        }
        let root = match &declaration.tree {
            uhura_syntax::ast::ImportTree::Single { path, .. } => &path.root,
            uhura_syntax::ast::ImportTree::Group { prefix, .. } => &prefix.root,
        };
        if matches!(
            root,
            uhura_syntax::ast::ImportRoot::Package(alias) if alias.text != "uhura"
        ) {
            diagnostics.push(error(
                codes::IMPORT,
                "uhura-0.4/evidence-import-boundary",
                "evidence may import only public declarations from `crate` and compiler-provided `uhura` contracts",
                span(declaration.span),
            ));
        }
    }
}

fn span(value: uhura_syntax::ast::Span) -> SourceSpan {
    SourceSpan::new(value.file, value.start, value.end)
}

#[cfg(test)]
mod tests {
    use uhura_syntax::{SourceIdentity, parse};

    use super::*;

    fn parsed(module: &str, source: &str) -> uhura_syntax::ast::Module {
        let parsed = parse(
            SourceIdentity::new(7, "example.core@1", module, format!("{module}.uhura")),
            source,
        );
        assert!(
            parsed.diagnostics.is_empty(),
            "parse diagnostics: {:#?}",
            parsed.diagnostics
        );
        parsed.module
    }

    #[test]
    fn evidence_is_a_manifest_role_of_the_current_frontend() {
        let core = parsed(
            "core",
            r#"
pub machine Counter {
  events { Increment, }
  outcomes { commit Accepted, }
  state { count: Int = 0 }
  observe { count }
  on Increment { count = count + 1; Accepted }
}
"#,
        );
        let evidence = parsed(
            "evidence",
            r#"
use crate::core::Counter;

scenario increment for Counter {
  start
  send Increment
  expect Accepted commands []
  pin done
}

example incremented = increment::done;
"#,
        );
        assert!(validate(&[core], &[evidence]).is_empty());
    }

    #[test]
    fn rejects_role_crossing_and_foreign_evidence_imports() {
        let core = parsed(
            "core",
            r#"
scenario misplaced for Missing {
  start
}
"#,
        );
        let evidence = parsed(
            "evidence",
            r#"
use vendor::module::Thing;

pub struct Helper { value: Int }
"#,
        );
        let rules = validate(&[core], &[evidence])
            .into_iter()
            .map(|diagnostic| diagnostic.rule)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            rules,
            std::collections::BTreeSet::from([
                "uhura-0.4/core-declaration-in-evidence",
                "uhura-0.4/evidence-import-boundary",
                "uhura-0.4/evidence-outside-manifest-role",
            ])
        );
    }
}
