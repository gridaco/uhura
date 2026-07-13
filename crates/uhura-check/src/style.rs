//! The whole CSS surface (§4.5): class-rooted selectors in per-file
//! `<style>` blocks (error, with the subject's root class recommended),
//! markup-referenced classes defined nowhere (warning), and the compiled
//! stylesheet — `theme.css` then `<style>` blocks in path order (plan
//! micro-decision #7). Declarations pass through verbatim.

use std::collections::BTreeSet;

use uhura_base::{Diagnostic, Span, codes};
use uhura_syntax::ast;
use uhura_syntax::css;

/// Checks one definition's `<style>` block; returns the classes it defines.
pub fn check_style_block(
    subject_name: &str,
    style: &ast::StyleBlock,
    diags: &mut Vec<Diagnostic>,
) -> BTreeSet<String> {
    let mut defined = BTreeSet::new();
    for rule in &style.rules {
        defined.extend(rule.classes.iter().cloned());
        // @-rules keep their inner selectors verbatim; the shallow check
        // covers plain rules only (§4.5 — everything inside declarations
        // is passed through).
        if rule.selector.starts_with('@') {
            continue;
        }
        for selector in rule.selector.split(',') {
            let selector = selector.trim();
            if selector.is_empty() {
                continue;
            }
            if !selector.starts_with('.') {
                diags.push(
                    Diagnostic::error(
                        codes::CLASS_ROOTING.0,
                        codes::CLASS_ROOTING.1,
                        format!(
                            "`{selector}` is not class-rooted — scoped CSS is deferred, so \
                             rooting is the isolation contract (§4.5)"
                        ),
                        rule.span,
                    )
                    .with_note(format!(
                        "root it under the subject's class: `.{subject_name} …`"
                    )),
                );
            }
        }
    }
    defined
}

/// Cross-app class existence: every class referenced in markup must appear
/// in some style source (theme.css or any `<style>` block) — else warning.
pub fn check_class_existence(
    class_refs: &[(String, Span)],
    defined: &BTreeSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    let mut reported: BTreeSet<&str> = BTreeSet::new();
    for (class, span) in class_refs {
        if !defined.contains(class) && reported.insert(class) {
            diags.push(Diagnostic::warning(
                codes::UNDEFINED_CLASS.0,
                codes::UNDEFINED_CLASS.1,
                format!("class `{class}` is styled nowhere (theme.css or any `<style>` block)"),
                *span,
            ));
        }
    }
}

/// Classes defined by `theme.css` (selector-level scan; declarations are
/// opaque). The file id is only used for spans, which nothing reports on
/// theme.css in this pass.
pub fn theme_classes(theme_file: uhura_base::FileId, theme_css: &str) -> BTreeSet<String> {
    css::parse_stylesheet(theme_file, 0, theme_css)
        .into_iter()
        .flat_map(|r| r.classes)
        .collect()
}

/// The compiled stylesheet: `theme.css` then `<style>` blocks in
/// path-lexicographic order, each under a provenance banner. Ships beside
/// the IR, never inside it (micro-decision #8).
pub fn compile_stylesheet(
    theme_css: Option<&str>,
    blocks: &[(String, String)], // (rel_path, raw css) — pre-sorted by path
) -> String {
    let mut out = String::new();
    if let Some(theme) = theme_css {
        out.push_str("/* styles/theme.css */\n");
        out.push_str(theme.trim_end());
        out.push('\n');
    }
    for (path, raw) in blocks {
        out.push_str(&format!("\n/* {path} <style> */\n"));
        out.push_str(raw.trim());
        out.push('\n');
    }
    out
}
