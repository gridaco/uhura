//! CSS handling (design §4.5): a selector tokenizer plus verbatim
//! balanced-brace declaration capture. The checker's whole CSS surface is
//! selector shape — declarations pass through untouched. Also used by
//! uhura-check on `styles/theme.css`.

use uhura_base::{FileId, Span};

use crate::ast::StyleRule;

/// Parses stylesheet text into rules. `base` is the byte offset of `text`
/// within the containing file (0 for standalone .css files) so spans line up.
pub fn parse_stylesheet(file: FileId, base: u32, text: &str) -> Vec<StyleRule> {
    let mut rules = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        // Skip whitespace and /* … */ comments.
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if text[i..].starts_with("/*") {
            i = text[i..]
                .find("*/")
                .map(|j| i + j + 2)
                .unwrap_or(bytes.len());
            continue;
        }
        // Selector runs to the next `{` (or EOF for garbage).
        let sel_start = i;
        let Some(rel_brace) = text[i..].find('{') else {
            break;
        };
        let sel_end = i + rel_brace;
        let selector_raw = text[sel_start..sel_end].trim();
        // Declaration block: balanced braces (handles @media nesting by
        // capturing the whole inner block verbatim).
        let mut depth = 0usize;
        let mut j = sel_end;
        let decl_start = sel_end + 1;
        let mut decl_end = bytes.len();
        while j < bytes.len() {
            match bytes[j] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        decl_end = j;
                        break;
                    }
                }
                _ => {}
            }
            j += 1;
        }
        let decls = text[decl_start..decl_end.min(bytes.len())].trim();
        let selector = normalize_ws(selector_raw);
        // For @-rules the class references live in the nested inner rules,
        // which are captured verbatim inside `decls`.
        let classes = if selector.starts_with('@') {
            extract_classes(decls)
        } else {
            extract_classes(&selector)
        };
        rules.push(StyleRule {
            selector,
            classes,
            decls: decls.to_string(),
            span: Span::new(
                file,
                base + sel_start as u32,
                base + decl_end.min(bytes.len()) as u32,
            ),
        });
        i = decl_end.saturating_add(1);
    }
    rules
}

fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Class names referenced anywhere in a selector (`.post-card` → `post-card`).
pub fn extract_classes(selector: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = selector.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'.' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len()
                && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'-' || bytes[end] == b'_')
            {
                end += 1;
            }
            if end > start {
                let name = &selector[start..end];
                if !out.iter().any(|c| c == name) {
                    out.push(name.to_string());
                }
            }
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_and_classes() {
        let css = "
/* tokens */
.post-card { display: flex; }
.post-card .avatar, .muted { color: var(--x); }
@media (min-width: 600px) { .post-card { gap: 8px; } }
";
        let rules = parse_stylesheet(FileId(0), 0, css);
        assert_eq!(rules.len(), 3);
        assert_eq!(rules[0].selector, ".post-card");
        assert_eq!(rules[0].classes, vec!["post-card"]);
        assert_eq!(rules[1].classes, vec!["post-card", "avatar", "muted"]);
        assert!(rules[2].selector.starts_with("@media"));
        assert_eq!(rules[2].classes, vec!["post-card"]);
        assert_eq!(rules[1].decls, "color: var(--x);");
    }
}
