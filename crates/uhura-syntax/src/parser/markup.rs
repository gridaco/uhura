//! The markup surface (design §4.4): elements, `{#if}` / `{#each}` /
//! `{#match}` blocks, `{expr}` interpolation, `on:` event bindings. Parsed
//! char-wise off the shared cursor; every `{…}` expression region drops into
//! the DSL parser and resyncs on its closing brace.

use uhura_base::codes;

use crate::ast::*;
use crate::cursor::Cursor;
use crate::token::TokenKind as T;

use super::expr::{parse_args, parse_expr};
use super::stream::DslStream;

/// Why `parse_nodes` stopped.
#[derive(Debug, PartialEq, Eq)]
pub enum Stop {
    /// `</name>` — left unconsumed for the caller to match.
    CloseTag,
    /// `{:…}` — an arm marker; left unconsumed.
    ArmMarker,
    /// `{/…}` — a block close; left unconsumed.
    BlockClose,
    /// `<style>` boundary (top level); left unconsumed.
    Style,
    Eof,
}

fn peek_markup_boundary(cur: &Cursor) -> Option<Stop> {
    let rest = cur.rest();
    if rest.is_empty() {
        return Some(Stop::Eof);
    }
    if rest.starts_with("</") {
        return Some(Stop::CloseTag);
    }
    if rest.starts_with("<style") {
        return Some(Stop::Style);
    }
    if rest.starts_with("{:") {
        return Some(Stop::ArmMarker);
    }
    if rest.starts_with("{/") {
        return Some(Stop::BlockClose);
    }
    None
}

fn skip_markup_ws(cur: &mut Cursor) {
    while matches!(cur.peek(), Some(c) if c.is_whitespace()) {
        cur.bump();
    }
}

/// Kebab identifier at the cursor (markup-side twin of the DSL rule).
fn markup_ident(cur: &mut Cursor) -> Option<String> {
    let start = cur.pos();
    match cur.peek() {
        Some('a'..='z') => {
            cur.bump();
        }
        _ => return None,
    }
    while let Some(c) = cur.peek() {
        match c {
            'a'..='z' | '0'..='9' => {
                cur.bump();
            }
            '-' if matches!(cur.peek2(), Some('a'..='z' | '0'..='9')) => {
                cur.bump();
            }
            _ => break,
        }
    }
    Some(cur.rest_from(start))
}

/// Parses sibling nodes until a boundary; the boundary is not consumed.
pub fn parse_nodes(cur: &mut Cursor) -> (Vec<Node>, Stop) {
    let mut nodes = Vec::new();
    loop {
        skip_markup_ws(cur);
        if let Some(stop) = peek_markup_boundary(cur) {
            return (nodes, stop);
        }
        match cur.peek() {
            None => return (nodes, Stop::Eof),
            Some('<') => {
                if let Some(el) = parse_element(cur) {
                    nodes.push(el);
                }
            }
            Some('{') if matches!(cur.peek2(), Some('#')) => {
                if let Some(block) = parse_block(cur) {
                    nodes.push(block);
                }
            }
            Some(_) => {
                // A text run (literal text + interpolations).
                if let Some(text) = parse_text(cur) {
                    nodes.push(text);
                }
            }
        }
    }
}

fn parse_text(cur: &mut Cursor) -> Option<Node> {
    let start = cur.pos();
    let mut runs: Vec<TextRun> = Vec::new();
    let mut literal = String::new();
    let mut lit_start = cur.pos();

    loop {
        match cur.peek() {
            None | Some('<') => break,
            Some('{') => {
                match cur.peek2() {
                    Some('#') | Some(':') | Some('/') => break,
                    _ => {
                        // Interpolation.
                        if !literal.trim().is_empty() {
                            runs.push(TextRun::Literal(std::mem::take(&mut literal)));
                        } else {
                            literal.clear();
                        }
                        cur.bump(); // `{`
                        let expr = parse_braced_expr(cur);
                        runs.push(TextRun::Interp(expr));
                        lit_start = cur.pos();
                    }
                }
            }
            Some('}') => {
                let at = cur.pos();
                cur.bump();
                cur.error(
                    codes::RAW_BRACE_IN_TEXT,
                    "raw `}` in markup text — human-readable content is typed data; \
                     use an interpolation or move the text into an expression string",
                    uhura_base::Span::new(cur.file, at, cur.pos()),
                );
            }
            Some(_) => {
                literal.push(cur.bump().unwrap());
            }
        }
    }
    let _ = lit_start;
    if !literal.trim().is_empty() {
        runs.push(TextRun::Literal(literal));
    }
    if runs.is_empty() {
        return None;
    }
    Some(Node::Text {
        runs,
        span: cur.span_from(start),
    })
}

/// `{` already consumed; parses `expr }` and resyncs.
fn parse_braced_expr(cur: &mut Cursor) -> Expr {
    let mut s = DslStream::new(cur);
    let expr = parse_expr(&mut s);
    s.expect(&T::RBrace, "to close the interpolation");
    s.finish();
    expr
}

fn parse_element(cur: &mut Cursor) -> Option<Node> {
    let start = cur.pos();
    cur.bump(); // `<`
    let Some(name) = markup_ident(cur) else {
        cur.error(
            codes::UNEXPECTED_TOKEN,
            "expected an element or component name after `<`",
            cur.span_from(start),
        );
        // Skip to `>` to avoid loops.
        while let Some(c) = cur.bump() {
            if c == '>' {
                break;
            }
        }
        return Some(Node::Error {
            span: cur.span_from(start),
        });
    };

    let mut attrs = Vec::new();
    let mut events = Vec::new();
    let mut self_closing = false;

    loop {
        skip_markup_ws(cur);
        match cur.peek() {
            None => {
                cur.error(
                    codes::UNCLOSED_TAG,
                    format!("`<{name}` is never closed"),
                    cur.span_from(start),
                );
                return Some(Node::Element(Element {
                    name,
                    attrs,
                    events,
                    children: Vec::new(),
                    self_closing: true,
                    span: cur.span_from(start),
                }));
            }
            Some('>') => {
                cur.bump();
                break;
            }
            Some('/') => {
                cur.bump();
                if cur.eat('>') {
                    self_closing = true;
                    break;
                }
                cur.error(
                    codes::UNEXPECTED_TOKEN,
                    "expected `/>`",
                    cur.span_from(cur.pos() - 1),
                );
            }
            Some(_) => {
                let astart = cur.pos();
                if cur.eat_str("on:") {
                    // Event attribute.
                    let Some(event) = markup_ident(cur) else {
                        cur.error(
                            codes::UNEXPECTED_TOKEN,
                            "expected an event name after `on:`",
                            cur.span_from(astart),
                        );
                        cur.bump();
                        continue;
                    };
                    let binding = if cur.eat('=') {
                        if cur.eat('{') {
                            parse_event_binding(cur)
                        } else {
                            cur.error(
                                codes::UNEXPECTED_TOKEN,
                                "event bindings are `on:event={emit …}` or bare `on:event`",
                                cur.span_from(astart),
                            );
                            EventBinding::Forward
                        }
                    } else {
                        EventBinding::Forward
                    };
                    events.push(EventAttr {
                        event,
                        binding,
                        span: cur.span_from(astart),
                    });
                } else if let Some(aname) = markup_ident(cur) {
                    let value = if cur.eat('=') {
                        match cur.peek() {
                            Some('"') => {
                                cur.bump();
                                let vstart = cur.pos();
                                while let Some(c) = cur.peek() {
                                    if c == '"' || c == '\n' {
                                        break;
                                    }
                                    cur.bump();
                                }
                                let text = cur.rest_from(vstart);
                                if !cur.eat('"') {
                                    cur.error(
                                        codes::UNTERMINATED_STRING,
                                        "unterminated attribute string",
                                        cur.span_from(astart),
                                    );
                                }
                                AttrValue::Literal(text)
                            }
                            Some('{') => {
                                cur.bump();
                                AttrValue::Expr(parse_braced_expr(cur))
                            }
                            _ => {
                                cur.error(
                                    codes::UNEXPECTED_TOKEN,
                                    "attribute values are `\"literal\"` or `{expr}`",
                                    cur.span_from(astart),
                                );
                                AttrValue::Bare
                            }
                        }
                    } else {
                        AttrValue::Bare
                    };
                    attrs.push(Attr {
                        name: aname,
                        value,
                        span: cur.span_from(astart),
                    });
                } else {
                    cur.error(
                        codes::UNEXPECTED_TOKEN,
                        "expected an attribute, `on:` binding, `>` or `/>`",
                        cur.span_from(astart),
                    );
                    cur.bump();
                }
            }
        }
    }

    let mut children = Vec::new();
    if !self_closing {
        let (kids, stop) = parse_nodes(cur);
        children = kids;
        match stop {
            Stop::CloseTag => {
                let close_start = cur.pos();
                // Speculative: consume `</ident>` only if the name matches;
                // otherwise leave it for an ancestor element to match.
                let rest = cur.rest();
                let tag_len = rest[2..].find('>').map(|i| i + 2).unwrap_or(rest.len());
                let close_name = rest[2..tag_len].trim().to_string();
                if close_name == name {
                    cur.set_pos(close_start + tag_len as u32);
                    cur.eat('>');
                } else {
                    cur.error(
                        codes::UNCLOSED_TAG,
                        format!(
                            "`<{name}>` is closed by `</{close_name}>` — closing `<{name}>` here"
                        ),
                        cur.span_from(close_start),
                    );
                    // Do not consume; an ancestor may match it.
                }
            }
            Stop::Eof | Stop::Style => {
                cur.error(
                    codes::UNCLOSED_TAG,
                    format!("`<{name}>` is never closed"),
                    cur.span_from(start),
                );
            }
            Stop::ArmMarker | Stop::BlockClose => {
                cur.error(
                    codes::UNCLOSED_TAG,
                    format!("`<{name}>` is not closed before the enclosing block ends"),
                    cur.span_from(start),
                );
            }
        }
    }

    Some(Node::Element(Element {
        name,
        attrs,
        events,
        children,
        self_closing,
        span: cur.span_from(start),
    }))
}

/// Inside `on:x={` — parses `emit name(args)` up to the closing `}`.
fn parse_event_binding(cur: &mut Cursor) -> EventBinding {
    let mut s = DslStream::new(cur);
    let binding = if s.eat_ident("emit") {
        match s.expect_ident("as the emitted event name") {
            Some((name, _)) => {
                let args = if *s.peek() == T::LParen {
                    parse_args(&mut s)
                } else {
                    Vec::new()
                };
                EventBinding::Emit { name, args }
            }
            None => EventBinding::Forward,
        }
    } else {
        let span = s.peek_span();
        s.cur.error(
            codes::UNEXPECTED_TOKEN,
            "event bindings start with `emit` (`on:press={emit like-toggled(…)}`)",
            span,
        );
        EventBinding::Forward
    };
    s.expect(&T::RBrace, "to close the event binding");
    s.finish();
    binding
}

/// `{#…` blocks. The `{` and `#` are unconsumed on entry.
fn parse_block(cur: &mut Cursor) -> Option<Node> {
    let start = cur.pos();
    cur.bump(); // `{`
    cur.bump(); // `#`
    let Some(kw) = markup_ident(cur) else {
        cur.error(
            codes::UNEXPECTED_TOKEN,
            "expected `if`, `each`, or `match` after `{#`",
            cur.span_from(start),
        );
        return Some(Node::Error {
            span: cur.span_from(start),
        });
    };
    match kw.as_str() {
        "if" => parse_if_block(cur, start),
        "each" => parse_each_block(cur, start),
        "match" => parse_match_block(cur, start),
        other => {
            cur.error(
                codes::UNEXPECTED_TOKEN,
                format!("unknown block `{{#{other}}}` — blocks are if | each | match"),
                cur.span_from(start),
            );
            skip_past_brace(cur);
            Some(Node::Error {
                span: cur.span_from(start),
            })
        }
    }
}

fn skip_past_brace(cur: &mut Cursor) {
    while let Some(c) = cur.bump() {
        if c == '}' {
            break;
        }
    }
}

/// Consumes a `{:marker}` or `{/marker}` head and returns the marker word.
fn consume_marker(cur: &mut Cursor) -> Option<String> {
    cur.bump(); // `{`
    cur.bump(); // `:` or `/`
    // Anything up to `}` beyond the word is handled by specific callers
    // (e.g. `{:when variant binding}`); default: expect `}` right away.
    markup_ident(cur)
}

fn parse_if_block(cur: &mut Cursor, start: u32) -> Option<Node> {
    let cond = {
        let mut s = DslStream::new(cur);
        let e = parse_expr(&mut s);
        s.expect(&T::RBrace, "to close the `{#if …}` head");
        s.finish();
        e
    };
    let (then, stop) = parse_nodes(cur);
    let mut els = None;
    match stop {
        Stop::ArmMarker => {
            let m_start = cur.pos();
            let word = consume_marker(cur);
            if word.as_deref() == Some("else") {
                if !cur.eat('}') {
                    cur.error(
                        codes::UNEXPECTED_TOKEN,
                        "expected `}` after `{:else`",
                        cur.span_from(m_start),
                    );
                    skip_past_brace(cur);
                }
                let (e, stop2) = parse_nodes(cur);
                els = Some(e);
                expect_block_close(cur, stop2, "if");
            } else {
                cur.error(
                    codes::UNEXPECTED_TOKEN,
                    "only `{:else}` is valid inside `{#if}`",
                    cur.span_from(m_start),
                );
                skip_past_brace(cur);
                let (_, stop2) = parse_nodes(cur);
                expect_block_close(cur, stop2, "if");
            }
        }
        other => expect_block_close(cur, other, "if"),
    }
    Some(Node::If {
        cond,
        then,
        els,
        span: cur.span_from(start),
    })
}

fn expect_block_close(cur: &mut Cursor, stop: Stop, kw: &str) {
    match stop {
        Stop::BlockClose => {
            let start = cur.pos();
            let word = consume_marker(cur);
            if word.as_deref() != Some(kw) {
                cur.error(
                    codes::UNCLOSED_BLOCK,
                    format!(
                        "expected `{{/{kw}}}`, found `{{/{}}}`",
                        word.unwrap_or_default()
                    ),
                    cur.span_from(start),
                );
            }
            if !cur.eat('}') {
                skip_past_brace(cur);
            }
        }
        _ => {
            let span = cur.span_from(cur.pos());
            cur.error(
                codes::UNCLOSED_BLOCK,
                format!("`{{#{kw}}}` is never closed"),
                span,
            );
        }
    }
}

fn parse_each_block(cur: &mut Cursor, start: u32) -> Option<Node> {
    let mut s = DslStream::new(cur);
    let seq = parse_expr(&mut s);
    s.expect(&T::Ident("as".into()), "in `{#each list as item (key)}`");
    let item = s
        .expect_ident("as the item binding")
        .map(|(n, _)| n)
        .unwrap_or_else(|| "item".to_string());
    // The parenthesized key is grammar, not lint (design §4.4).
    let key = if *s.peek() == T::LParen {
        s.bump();
        let k = parse_expr(&mut s);
        s.expect(&T::RParen, "to close the key");
        k
    } else {
        let span = s.peek_span();
        s.cur.error(
            codes::UNKEYED_EACH,
            "`{#each}` requires a key: `{#each list as item (item.id)}`",
            span,
        );
        Expr {
            kind: ExprKind::Error,
            span,
        }
    };
    s.expect(&T::RBrace, "to close the `{#each …}` head");
    s.finish();

    let (body, stop) = parse_nodes(cur);
    expect_block_close(cur, stop, "each");
    Some(Node::Each {
        item,
        seq,
        key,
        body,
        span: cur.span_from(start),
    })
}

fn parse_match_block(cur: &mut Cursor, start: u32) -> Option<Node> {
    let scrutinee = {
        let mut s = DslStream::new(cur);
        let e = parse_expr(&mut s);
        s.expect(&T::RBrace, "to close the `{#match …}` head");
        s.finish();
        e
    };

    let mut arms = Vec::new();
    // Nodes directly inside `{#match}` before the first arm are ignored
    // whitespace or an error.
    let (stray, mut stop) = parse_nodes(cur);
    if !stray.is_empty() {
        let span = cur.span_from(start);
        cur.error(
            codes::UNEXPECTED_TOKEN,
            "content inside `{#match}` must live under a `{:when …}` arm",
            span,
        );
    }
    loop {
        match stop {
            Stop::ArmMarker => {
                let a_start = cur.pos();
                let word = consume_marker(cur);
                match word.as_deref() {
                    Some("when") => {
                        skip_markup_ws(cur);
                        let Some(variant) = markup_ident(cur) else {
                            cur.error(
                                codes::UNEXPECTED_TOKEN,
                                "expected a variant name after `{:when`",
                                cur.span_from(a_start),
                            );
                            skip_past_brace(cur);
                            let (_, s2) = parse_nodes(cur);
                            stop = s2;
                            continue;
                        };
                        skip_markup_ws(cur);
                        let binding = markup_ident(cur);
                        skip_markup_ws(cur);
                        if !cur.eat('}') {
                            cur.error(
                                codes::UNEXPECTED_TOKEN,
                                "expected `}` to close the `{:when …}` arm head",
                                cur.span_from(a_start),
                            );
                            skip_past_brace(cur);
                        }
                        let (body, s2) = parse_nodes(cur);
                        arms.push(MatchArm {
                            pattern: MatchPattern::Variant(variant),
                            binding,
                            body,
                            span: cur.span_from(a_start),
                        });
                        stop = s2;
                    }
                    Some("else") => {
                        if !cur.eat('}') {
                            skip_past_brace(cur);
                        }
                        let (body, s2) = parse_nodes(cur);
                        arms.push(MatchArm {
                            pattern: MatchPattern::Else,
                            binding: None,
                            body,
                            span: cur.span_from(a_start),
                        });
                        stop = s2;
                    }
                    other => {
                        cur.error(
                            codes::UNEXPECTED_TOKEN,
                            format!(
                                "unknown arm `{{:{}}}` — arms are `{{:when …}}` or `{{:else}}`",
                                other.unwrap_or_default()
                            ),
                            cur.span_from(a_start),
                        );
                        skip_past_brace(cur);
                        let (_, s2) = parse_nodes(cur);
                        stop = s2;
                    }
                }
            }
            other => {
                expect_block_close(cur, other, "match");
                break;
            }
        }
    }
    Some(Node::Match {
        scrutinee,
        arms,
        span: cur.span_from(start),
    })
}
