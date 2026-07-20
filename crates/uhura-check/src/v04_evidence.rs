//! Admission checks for separately versioned evidence attached to a 0.4 core.
//!
//! Evidence deliberately keeps the implemented 0.3 tooling vocabulary.  It
//! may inspect public declarations from the resolved 0.4 package, but it is
//! not another core module and cannot contribute deployable declarations.

use std::collections::BTreeSet;

use uhura_base::Diagnostic;
use uhura_syntax::ast;

use crate::diagnostic::{codes, error};

pub(crate) fn validate(
    project: &ast::Project,
    core_package: &str,
    public_names: &BTreeSet<String>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for module in &project.modules {
        validate_features(module, &mut diagnostics);
        validate_imports(module, core_package, public_names, &mut diagnostics);
        for declaration in &module.declarations {
            if !matches!(
                declaration.value,
                ast::DeclarationKind::Scenario(_)
                    | ast::DeclarationKind::Example(_)
                    | ast::DeclarationKind::Checkpoint(_)
            ) {
                diagnostics.push(error(
                    codes::EVIDENCE,
                    "uhura-0.4/evidence-core-declaration",
                    "a `[evidence].sources` file may declare only scenarios, checkpoints, and examples; move types, values, functions, machines, and UI into a 0.4 core module",
                    declaration.span,
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

fn validate_features(module: &ast::Module, diagnostics: &mut Vec<Diagnostic>) {
    let evidence = module
        .uses
        .iter()
        .filter(|declaration| declaration.feature.value == "evidence")
        .count();
    if evidence != 1 {
        diagnostics.push(error(
            codes::EVIDENCE_NOT_ENABLED,
            "uhura-0.4/evidence-profile",
            "a `[evidence].sources` file must contain exactly one `use evidence` declaration",
            module.span,
        ));
    }
    for declaration in &module.uses {
        if declaration.feature.value != "evidence" {
            diagnostics.push(error(
                codes::EVIDENCE_NOT_ENABLED,
                "uhura-0.4/evidence-profile",
                format!(
                    "evidence tooling source cannot activate `{}`; only `use evidence` is admitted",
                    declaration.feature.value
                ),
                declaration.span,
            ));
        }
    }
}

fn validate_imports(
    module: &ast::Module,
    core_package: &str,
    public_names: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for import in &module.imports {
        if import.target == core_package {
            for name in &import.names {
                if !public_names.contains(&name.value) {
                    diagnostics.push(error(
                        codes::IMPORT,
                        "uhura-0.4/private-evidence-import",
                        format!(
                            "`{}` is not a public declaration of the resolved 0.4 package `{core_package}`",
                            name.value
                        ),
                        name.span,
                    ));
                }
            }
        } else if !is_standard_evidence_import(&import.target) {
            diagnostics.push(error(
                codes::IMPORT,
                "uhura-0.4/evidence-import-boundary",
                format!(
                    "evidence tooling source may import only `{core_package}` and compiler-provided `uhura.*@1` contracts, not `{}`",
                    import.target
                ),
                import.span,
            ));
        }
    }
}

fn is_standard_evidence_import(target: &str) -> bool {
    target
        .strip_prefix("uhura.")
        .and_then(|value| value.strip_suffix("@1"))
        .is_some_and(|name| {
            !name.is_empty()
                && name.split('.').all(|segment| {
                    !segment.is_empty()
                        && segment.bytes().all(|byte| {
                            byte.is_ascii_lowercase()
                                || byte.is_ascii_digit()
                                || byte == b'_'
                                || byte == b'-'
                        })
                })
        })
}

#[cfg(test)]
mod tests {
    use uhura_base::FileId;
    use uhura_syntax::{SourceFile, parse_project};

    use super::*;

    fn parsed(source: &str) -> ast::Project {
        let parsed = parse_project([SourceFile::new(
            FileId(7),
            "evidence/programs.uhura",
            source,
        )]);
        assert!(
            parsed.diagnostics.is_empty(),
            "parse diagnostics: {:#?}",
            parsed.diagnostics
        );
        parsed.project
    }

    #[test]
    fn admits_tooling_only_evidence_over_public_core_names() {
        let project = parsed(
            r#"language uhura 0.3
module evidence.programs@1

use evidence
import { Counter } from "example.core@1"

scenario increment for Counter {
  start
  send Increment
  expect Accepted commands []
  pin done
}

example incremented = increment::done
"#,
        );
        let diagnostics = validate(
            &project,
            "example.core@1",
            &BTreeSet::from(["Counter".to_string()]),
        );
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    }

    #[test]
    fn rejects_core_declarations_private_imports_and_foreign_packages() {
        let project = parsed(
            r#"language uhura 0.3
module evidence.invalid@1

use evidence
use ui
import { Secret } from "example.core@1"
import { Other } from "vendor.other@1"

const helper: Int = 1
"#,
        );
        let rules = validate(&project, "example.core@1", &BTreeSet::new())
            .into_iter()
            .map(|diagnostic| diagnostic.rule)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            rules,
            BTreeSet::from([
                "uhura-0.4/evidence-core-declaration",
                "uhura-0.4/evidence-import-boundary",
                "uhura-0.4/evidence-profile",
                "uhura-0.4/private-evidence-import",
            ])
        );
    }
}
