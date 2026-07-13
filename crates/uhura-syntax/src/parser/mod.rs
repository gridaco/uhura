//! The file-level parse driver. A `.uhura` file has strictly ordered
//! sections — header → optional `store {}` → markup → optional `<style>` —
//! so the driver owns all mode transitions (design §4, plan risk #1).

mod dsl;
mod examples;
mod expr;
mod markup;
mod stream;

use uhura_base::{Diagnostic, FileId, Span, codes};

use crate::ast::*;
use crate::css;
use crate::cursor::Cursor;
use crate::token::TokenKind as T;

use stream::DslStream;

/// Which grammar a source file uses (decided by filename, `.examples.uhura`
/// vs `.uhura` — the caller knows).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SourceKind {
    Module,
    Examples,
}

#[derive(Debug)]
pub enum Parsed {
    Module(Box<File>),
    Examples(ExamplesFile),
}

pub struct ParseOutput {
    pub parsed: Parsed,
    pub diagnostics: Vec<Diagnostic>,
}

/// Checker-enforced bounds (design §4).
const MAX_FILE_BYTES: usize = 256 * 1024;
const MAX_NESTING: usize = 32;
const MAX_VIEW_NODES: usize = 512;
const MAX_HANDLERS: usize = 128;

pub fn parse(file: FileId, text: &str, kind: SourceKind) -> ParseOutput {
    let mut cur = Cursor::new(file, text);

    if text.len() > MAX_FILE_BYTES {
        cur.error(
            codes::FILE_TOO_LARGE,
            format!(
                "file is {} bytes; the bound is {MAX_FILE_BYTES}",
                text.len()
            ),
            Span::new(file, 0, 0),
        );
    }

    let parsed = match kind {
        SourceKind::Examples => {
            let mut s = DslStream::new(&mut cur);
            let ex = examples::parse_examples(&mut s);
            s.finish();
            Parsed::Examples(ex)
        }
        SourceKind::Module => Parsed::Module(Box::new(parse_module(&mut cur))),
    };

    if let Parsed::Module(f) = &parsed {
        enforce_bounds(&mut cur, f);
    }

    ParseOutput {
        parsed,
        diagnostics: cur.diagnostics,
    }
}

fn parse_module(cur: &mut Cursor) -> File {
    // ── header ──────────────────────────────────────────────────────────
    let mut s = DslStream::new(cur);
    let kind = parse_def_kind(&mut s);

    let mut uses = Vec::new();
    let mut props = Vec::new();
    let mut emits = Vec::new();
    let mut params = Vec::new();
    let mut store = None;

    loop {
        match s.peek().clone() {
            T::Ident(k) if k == "use" => {
                if let Some(u) = dsl::parse_use(&mut s) {
                    uses.push(u);
                }
            }
            T::Ident(k) if k == "props" => {
                s.bump();
                props = dsl::parse_props_block(&mut s);
            }
            T::Ident(k) if k == "emits" => {
                s.bump();
                emits = dsl::parse_emits_block(&mut s);
            }
            T::Ident(k) if k == "param" => {
                if let Some(p) = dsl::parse_param(&mut s) {
                    params.push(p);
                }
            }
            T::Ident(k) if k == "store" => {
                store = Some(dsl::parse_store(&mut s));
            }
            T::Lt | T::Eof => break,
            other => {
                let desc = other.describe();
                let span = s.peek_span();
                s.cur.error(
                    codes::MISPLACED_SECTION,
                    format!(
                        "expected a header declaration (use | props | emits | param | store) \
                         or markup, found {desc}"
                    ),
                    span,
                );
                s.bump();
            }
        }
    }
    s.finish();

    // ── markup ──────────────────────────────────────────────────────────
    let (markup, stop) = markup::parse_nodes(cur);

    // ── style ───────────────────────────────────────────────────────────
    let mut style = None;
    if stop == markup::Stop::Style {
        style = parse_style_section(cur);
        // Anything after </style> other than whitespace is misplaced.
        let tail_start = cur.pos();
        let tail = cur.rest().trim();
        if !tail.is_empty() {
            cur.error(
                codes::MISPLACED_SECTION,
                "content after `</style>` — the style block ends the file",
                Span::new(cur.file, tail_start, tail_start + 1),
            );
        }
    }

    File {
        kind,
        uses,
        props,
        emits,
        params,
        store,
        markup,
        style,
    }
}

fn parse_def_kind(s: &mut DslStream) -> DefKind {
    let start = s.peek_span();
    let T::Ident(kw) = s.peek().clone() else {
        let span = s.peek_span();
        s.cur.error(
            codes::MISPLACED_SECTION,
            "a .uhura file starts with `component <name>`, `page`, or `surface <name>`",
            span,
        );
        return DefKind::Error { span: start };
    };
    match kw.as_str() {
        "component" => {
            s.bump();
            match s.expect_ident("as the component name") {
                Some((name, nspan)) => DefKind::Component {
                    name,
                    span: start.to(nspan),
                },
                None => DefKind::Error { span: start },
            }
        }
        "page" => {
            s.bump();
            DefKind::Page { span: start }
        }
        "surface" => {
            s.bump();
            let Some((name, mut end)) = s.expect_ident("as the surface name") else {
                return DefKind::Error { span: start };
            };
            let modality = if s.eat_ident("modality") {
                match s.expect_ident("as the modality (`sheet`)") {
                    Some((m, mspan)) => {
                        end = mspan;
                        Some(m)
                    }
                    None => None,
                }
            } else {
                None
            };
            DefKind::Surface {
                name,
                modality,
                span: start.to(end),
            }
        }
        other => {
            s.cur.error(
                codes::MISPLACED_SECTION,
                format!(
                    "`{other}` is not a definition kind — a .uhura file starts with \
                     `component <name>`, `page`, or `surface <name>`"
                ),
                start,
            );
            DefKind::Error { span: start }
        }
    }
}

fn parse_style_section(cur: &mut Cursor) -> Option<StyleBlock> {
    let start = cur.pos();
    if !cur.eat_str("<style") {
        return None;
    }
    // Allow only optional whitespace before `>`.
    while matches!(cur.peek(), Some(c) if c.is_whitespace()) {
        cur.bump();
    }
    if !cur.eat('>') {
        cur.error(
            codes::INVALID_STYLE_BLOCK,
            "`<style>` takes no attributes",
            cur.span_from(start),
        );
        while let Some(c) = cur.bump() {
            if c == '>' {
                break;
            }
        }
    }
    let inner_start = cur.pos();
    let rest = cur.rest();
    let (inner_len, closed) = match rest.find("</style>") {
        Some(i) => (i, true),
        None => (rest.len(), false),
    };
    let raw = rest[..inner_len].to_string();
    cur.set_pos(inner_start + inner_len as u32);
    if closed {
        cur.eat_str("</style>");
    } else {
        cur.error(
            codes::INVALID_STYLE_BLOCK,
            "`<style>` is never closed",
            cur.span_from(start),
        );
    }
    let rules = css::parse_stylesheet(cur.file, inner_start, &raw);
    Some(StyleBlock {
        rules,
        raw,
        span: cur.span_from(start),
    })
}

// ── bounds (design §4) ──────────────────────────────────────────────────────

fn enforce_bounds(cur: &mut Cursor, f: &File) {
    if let Some(store) = &f.store
        && store.handlers.len() > MAX_HANDLERS
    {
        cur.error(
            codes::TOO_MANY_HANDLERS,
            format!(
                "{} handlers; the bound is {MAX_HANDLERS}",
                store.handlers.len()
            ),
            store.span,
        );
    }
    let mut count = 0usize;
    let mut too_deep: Option<Span> = None;
    for n in &f.markup {
        walk(n, 1, &mut count, &mut too_deep);
    }
    if count > MAX_VIEW_NODES {
        let span = f
            .markup
            .first()
            .map_or(Span::new(cur.file, 0, 0), node_span);
        cur.error(
            codes::TOO_MANY_NODES,
            format!("{count} view nodes; the bound is {MAX_VIEW_NODES}"),
            span,
        );
    }
    if let Some(span) = too_deep {
        cur.error(
            codes::NESTING_TOO_DEEP,
            format!("markup nesting exceeds {MAX_NESTING}"),
            span,
        );
    }
}

fn node_span(n: &Node) -> Span {
    match n {
        Node::Element(e) => e.span,
        Node::Text { span, .. }
        | Node::If { span, .. }
        | Node::Each { span, .. }
        | Node::Match { span, .. }
        | Node::Error { span } => *span,
    }
}

fn walk(n: &Node, depth: usize, count: &mut usize, too_deep: &mut Option<Span>) {
    *count += 1;
    if depth > MAX_NESTING && too_deep.is_none() {
        *too_deep = Some(node_span(n));
    }
    let kids: Vec<&Node> = match n {
        Node::Element(e) => e.children.iter().collect(),
        Node::If { then, els, .. } => then.iter().chain(els.iter().flatten()).collect(),
        Node::Each { body, .. } => body.iter().collect(),
        Node::Match { arms, .. } => arms.iter().flat_map(|a| a.body.iter()).collect(),
        _ => Vec::new(),
    };
    for k in kids {
        walk(k, depth + 1, count, too_deep);
    }
}
