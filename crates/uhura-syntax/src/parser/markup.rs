//! The markup surface (design §4.4): elements, `{#if}` / `{#each}` /
//! `{#match}` blocks, `{expr}` interpolation, `on:` event bindings. Parsed
//! char-wise off the shared cursor; every `{…}` expression region drops into
//! the DSL parser and resyncs on its closing brace.

use uhura_base::{Diagnostic, Span, codes};

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
    if starts_style_section(rest) {
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

/// `<style>` is a source-region boundary, while names such as
/// `<style-guide>` remain ordinary element/component names.
pub(super) fn starts_style_section(rest: &str) -> bool {
    let Some(mut tail) = rest.strip_prefix("<style") else {
        return false;
    };
    while let Some(ch) = tail.chars().next()
        && ch.is_whitespace()
    {
        tail = &tail[ch.len_utf8()..];
    }
    tail.starts_with('>')
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
/// XML-shaped comments live in the list's source layout and never become
/// semantic nodes.
pub fn parse_nodes(cur: &mut Cursor) -> (MarkupList, Stop) {
    let mut list = MarkupList::default();
    let mut pending = Vec::<PendingAnnotation>::new();
    loop {
        skip_markup_ws(cur);
        if let Some(stop) = peek_markup_boundary(cur) {
            diagnose_dangling_annotations(cur, &pending, cur.pos());
            return (list, stop);
        }
        if cur.rest().starts_with("<!--") {
            let comment = parse_markup_comment(cur);
            if let MarkupCommentKind::Annotation { kind } = &comment.kind {
                pending.push(PendingAnnotation {
                    comment_index: list.comments.len(),
                    annotation: MarkupAnnotation {
                        kind: kind.clone(),
                        text: comment.text.clone(),
                        span: comment.span,
                    },
                });
            }
            list.comments.push(PlacedMarkupComment {
                before: list.nodes.len(),
                comment,
            });
            continue;
        }
        let construct_start = cur.pos();
        let node = match cur.peek() {
            None => {
                diagnose_dangling_annotations(cur, &pending, cur.pos());
                return (list, Stop::Eof);
            }
            Some('<') => parse_element(cur),
            Some('{') if matches!(cur.peek2(), Some('#')) => parse_block(cur),
            Some(_) => {
                // A text run (literal text + interpolations).
                parse_text(cur)
            }
        };
        if let Some(mut node) = node {
            let recovery_target = attach_annotations(cur, &mut node, &pending);
            if recovery_target {
                mark_pending_rejected(&mut list, &pending);
            }
            pending.clear();
            push_node_coalescing_text(&mut list, node);
        } else if cur.pos() > construct_start && !pending.is_empty() {
            // A raw recovery token (currently a lone `}`) consumed source but
            // produced no semantic node. It is still the next incompatible
            // construct; metadata must not skip it and attach later.
            reject_pending_annotations(
                cur,
                &pending,
                Span::new(cur.file, construct_start, cur.pos()),
            );
            mark_pending_rejected(&mut list, &pending);
            pending.clear();
        }
    }
}

#[derive(Clone, Debug)]
struct PendingAnnotation {
    comment_index: usize,
    annotation: MarkupAnnotation,
}

fn node_span(node: &Node) -> Span {
    match node {
        Node::Element(element) => element.span,
        Node::Text { span, .. }
        | Node::If { span, .. }
        | Node::Each { span, .. }
        | Node::Match { span, .. }
        | Node::Error { span } => *span,
    }
}

/// Returns true when the incompatible target is a formatter-omitted recovery
/// node, so its carriers need a stable rejected representation.
fn attach_annotations(
    cur: &mut Cursor,
    node: &mut Node,
    annotations: &[PendingAnnotation],
) -> bool {
    if annotations.is_empty() {
        return false;
    }
    match node {
        Node::Element(element) => element
            .annotations
            .extend(annotations.iter().map(|pending| pending.annotation.clone())),
        Node::If {
            annotations: target,
            ..
        }
        | Node::Each {
            annotations: target,
            ..
        }
        | Node::Match {
            annotations: target,
            ..
        } => target.extend(annotations.iter().map(|pending| pending.annotation.clone())),
        Node::Text { .. } => {
            let target = node_span(node);
            reject_pending_annotations(cur, annotations, target);
        }
        Node::Error { .. } => {
            let target = node_span(node);
            reject_pending_annotations(cur, annotations, target);
            return true;
        }
    }
    false
}

fn reject_pending_annotations(cur: &mut Cursor, annotations: &[PendingAnnotation], target: Span) {
    for pending in annotations {
        cur.diagnostics.push(
            Diagnostic::error(
                codes::INCOMPATIBLE_METADATA_TARGET.0,
                codes::INCOMPATIBLE_METADATA_TARGET.1,
                "markup annotations target elements, component invocations, or complete structural blocks",
                pending.annotation.span,
            )
            .with_label(target, "this markup construct is not annotatable"),
        );
    }
}

fn mark_pending_rejected(list: &mut MarkupList, annotations: &[PendingAnnotation]) {
    for pending in annotations {
        if let Some(placed) = list.comments.get_mut(pending.comment_index) {
            placed.comment.kind = MarkupCommentKind::RejectedAnnotation {
                kind: pending.annotation.kind.clone(),
            };
        }
    }
}

fn diagnose_dangling_annotations(
    cur: &mut Cursor,
    annotations: &[PendingAnnotation],
    boundary: u32,
) {
    let boundary = Span::new(cur.file, boundary, boundary);
    for pending in annotations {
        cur.diagnostics.push(
            Diagnostic::error(
                codes::DANGLING_METADATA.0,
                codes::DANGLING_METADATA.1,
                "markup annotation reaches a sibling-list boundary without a target",
                pending.annotation.span,
            )
            .with_label(boundary, "the annotation cannot cross this boundary"),
        );
    }
}

/// Comments must not split semantic text. Layout comments remain anchored at
/// the resulting list boundary while adjacent text runs coalesce.
fn push_node_coalescing_text(list: &mut MarkupList, node: Node) {
    match node {
        Node::Text {
            mut runs,
            span: incoming_span,
        } => {
            let old_boundary = list.nodes.len();
            if let Some(Node::Text {
                runs: existing,
                span,
            }) = list.nodes.last_mut()
            {
                for placed in list.comments.iter_mut().rev() {
                    if placed.before != old_boundary {
                        break;
                    }
                    // A comment directly before the incoming text belongs on
                    // the text side of the boundary after semantic coalescing.
                    // Moving it before the combined text prevents an invalid
                    // annotation from drifting forward to the next element.
                    placed.before = old_boundary - 1;
                }
                if let (Some(TextRun::Literal(left)), Some(TextRun::Literal(right))) =
                    (existing.last_mut(), runs.first_mut())
                {
                    left.push_str(right);
                    runs.remove(0);
                }
                existing.append(&mut runs);
                *span = span.to(incoming_span);
            } else {
                list.nodes.push(Node::Text {
                    runs,
                    span: incoming_span,
                });
            }
        }
        other => list.nodes.push(other),
    }
}

fn parse_markup_comment(cur: &mut Cursor) -> MarkupComment {
    let start = cur.pos();
    let opened = cur.eat_str("<!--");
    debug_assert!(opened);
    let body_start = cur.pos();
    let Some(close) = cur.rest().find("-->") else {
        // Recover at a safe next-line markup boundary when possible.
        let recovery = find_comment_recovery(cur.rest()).unwrap_or(cur.rest().len());
        let body = cur.rest()[..recovery].to_string();
        cur.set_pos(body_start + recovery as u32);
        let span = cur.span_from(start);
        cur.error(
            codes::MALFORMED_MARKUP_COMMENT,
            "unterminated markup comment",
            span,
        );
        return MarkupComment {
            kind: MarkupCommentKind::Malformed { terminated: false },
            text: normalize_markup_text(&normalize_line_endings(&body)),
            span,
        };
    };
    let body = cur.rest()[..close].to_string();
    cur.set_pos(body_start + close as u32);
    cur.eat_str("-->");
    let span = cur.span_from(start);

    if body.contains("--") || body.ends_with('-') {
        cur.error(
            codes::MALFORMED_MARKUP_COMMENT,
            "markup comment bodies cannot contain `--` or end with `-`",
            span,
        );
        return MarkupComment {
            kind: MarkupCommentKind::Malformed { terminated: true },
            text: normalize_markup_text(&normalize_line_endings(&body)),
            span,
        };
    }

    classify_markup_comment(cur, &body, span)
}

fn find_comment_recovery(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let mut line = 0;
    while line < bytes.len() {
        let rel = rest[line..].find(['\n', '\r'])?;
        let mut next = line + rel + 1;
        if bytes.get(line + rel) == Some(&b'\r') && bytes.get(next) == Some(&b'\n') {
            next += 1;
        }
        let mut probe = next;
        while matches!(bytes.get(probe), Some(b' ' | b'\t')) {
            probe += 1;
        }
        if is_comment_recovery_boundary(&rest[probe..]) {
            return Some(next);
        }
        line = next;
    }
    None
}

fn is_comment_recovery_boundary(rest: &str) -> bool {
    rest.starts_with("<!--")
        || rest.starts_with("<style")
        || rest.starts_with("{#")
        || rest.starts_with("{:")
        || rest.starts_with("{/")
        || rest
            .strip_prefix("</")
            .or_else(|| rest.strip_prefix('<'))
            .and_then(|tail| tail.as_bytes().first())
            .is_some_and(u8::is_ascii_lowercase)
}

fn classify_markup_comment(cur: &mut Cursor, body: &str, span: Span) -> MarkupComment {
    let body = normalize_line_endings(body);
    let leading_trimmed = body.trim_start_matches([' ', '\t', '\n']);
    if !leading_trimmed.starts_with('@') {
        return MarkupComment {
            kind: MarkupCommentKind::Ordinary,
            text: normalize_markup_text(&body),
            span,
        };
    }

    let marker_tail = &leading_trimmed[1..];
    let separator = marker_tail
        .char_indices()
        .find(|(_, ch)| matches!(ch, ' ' | '\t' | '\n'));
    let Some((kind_end, separator_char)) = separator else {
        cur.error(
            codes::MALFORMED_MARKUP_COMMENT,
            "annotation markers require a kind, whitespace, and a non-empty payload",
            span,
        );
        return MarkupComment {
            kind: MarkupCommentKind::Malformed { terminated: true },
            text: normalize_markup_text(&body),
            span,
        };
    };
    let kind = &marker_tail[..kind_end];
    let payload_start = kind_end + separator_char.len_utf8();
    let payload = normalize_markup_text(&marker_tail[payload_start..]);
    if !valid_annotation_kind(kind) || payload.is_empty() {
        cur.error(
            codes::MALFORMED_MARKUP_COMMENT,
            "annotation kind must be 1-64 lowercase kebab bytes and its payload must be non-empty",
            span,
        );
        return MarkupComment {
            kind: MarkupCommentKind::Malformed { terminated: true },
            // Retain the leading marker as recovery text. Otherwise a format
            // pass could silently turn malformed metadata into an unrelated
            // ordinary comment and erase the author's intent.
            text: normalize_markup_text(&body),
            span,
        };
    }
    MarkupComment {
        kind: MarkupCommentKind::Annotation {
            kind: kind.to_string(),
        },
        text: payload,
        span,
    }
}

fn valid_annotation_kind(kind: &str) -> bool {
    if kind.is_empty() || kind.len() > 64 || !kind.is_ascii() {
        return false;
    }
    let bytes = kind.as_bytes();
    if !bytes[0].is_ascii_lowercase() || bytes.last() == Some(&b'-') {
        return false;
    }
    let mut previous_dash = false;
    for byte in bytes {
        if *byte == b'-' {
            if previous_dash {
                return false;
            }
            previous_dash = true;
        } else if byte.is_ascii_lowercase() || byte.is_ascii_digit() {
            previous_dash = false;
        } else {
            return false;
        }
    }
    true
}

fn normalize_line_endings(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn normalize_markup_text(text: &str) -> String {
    if !text.contains('\n') {
        return text.trim_matches([' ', '\t']).to_string();
    }
    let mut lines: Vec<&str> = text.split('\n').collect();
    while lines
        .first()
        .is_some_and(|line| line.chars().all(|ch| matches!(ch, ' ' | '\t')))
    {
        lines.remove(0);
    }
    while lines
        .last()
        .is_some_and(|line| line.chars().all(|ch| matches!(ch, ' ' | '\t')))
    {
        lines.pop();
    }
    let mut lines: Vec<String> = lines
        .into_iter()
        .map(|line| line.trim_end_matches([' ', '\t']).to_string())
        .collect();
    let common = lines
        .iter()
        .filter(|line| !line.is_empty())
        .map(|line| line.bytes().take_while(|byte| *byte == b' ').count())
        .min()
        .unwrap_or(0);
    for line in &mut lines {
        let remove = line
            .bytes()
            .take_while(|byte| *byte == b' ')
            .count()
            .min(common);
        line.drain(..remove);
    }
    lines.join("\n")
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
    // A newline plus indentation before the next sibling/comment is source
    // layout, not human-readable text. Preserve intentional same-line spaces.
    let trailing = literal
        .char_indices()
        .rev()
        .take_while(|(_, ch)| ch.is_whitespace())
        .map(|(index, _)| index)
        .last();
    if let Some(start) = trailing
        && literal[start..].contains(['\n', '\r'])
    {
        literal.truncate(start);
    }
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

/// Scans a closing tag without allowing an XML-shaped comment's `>` to end
/// the tag. Embedded carriers are diagnosed atomically and removed only from
/// the recovery name used to match the owning element.
fn scan_closing_tag(cur: &mut Cursor) -> (usize, String) {
    let source = cur.rest().to_string();
    debug_assert!(source.starts_with("</"));
    let absolute_start = cur.pos();
    let mut clean = String::new();
    let mut cursor = 2usize;

    loop {
        let next_gt = source[cursor..].find('>').map(|offset| cursor + offset);
        let next_comment = source[cursor..].find("<!--").map(|offset| cursor + offset);

        if let Some(comment_start) = next_comment
            && next_gt.is_none_or(|gt| comment_start < gt)
        {
            clean.push_str(&source[cursor..comment_start]);
            let body_start = comment_start + "<!--".len();
            if let Some(close) = source[body_start..].find("-->") {
                let body_end = body_start + close;
                let comment_end = body_end + "-->".len();
                diagnose_embedded_markup_comment(
                    cur,
                    &source[body_start..body_end],
                    Span::new(
                        cur.file,
                        absolute_start + comment_start as u32,
                        absolute_start + comment_end as u32,
                    ),
                );
                cursor = comment_end;
                continue;
            }

            let recovery_end = next_gt.map_or(source.len(), |gt| gt + 1);
            cur.error(
                codes::MALFORMED_MARKUP_COMMENT,
                "unterminated markup comment inside a closing tag",
                Span::new(
                    cur.file,
                    absolute_start + comment_start as u32,
                    absolute_start + recovery_end as u32,
                ),
            );
            return (recovery_end, clean.trim().to_string());
        }

        if let Some(gt) = next_gt {
            clean.push_str(&source[cursor..gt]);
            return (gt + 1, clean.trim().to_string());
        }

        clean.push_str(&source[cursor..]);
        return (source.len(), clean.trim().to_string());
    }
}

fn diagnose_embedded_markup_comment(cur: &mut Cursor, body: &str, span: Span) {
    if body.contains("--") || body.ends_with('-') {
        cur.error(
            codes::MALFORMED_MARKUP_COMMENT,
            "markup comment bodies cannot contain `--` or end with `-`",
            span,
        );
        return;
    }

    let comment = classify_markup_comment(cur, body, span);
    if !matches!(comment.kind, MarkupCommentKind::Malformed { .. }) {
        cur.error(
            codes::UNEXPECTED_TOKEN,
            "XML-shaped comments are only legal at markup sibling positions",
            span,
        );
    }
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
                    children: MarkupList::default(),
                    annotations: Vec::new(),
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
                if cur.rest().starts_with("<!--") {
                    let comment = parse_markup_comment(cur);
                    if !matches!(comment.kind, MarkupCommentKind::Malformed { .. }) {
                        cur.error(
                            codes::UNEXPECTED_TOKEN,
                            "XML-shaped comments are only legal at markup sibling positions",
                            comment.span,
                        );
                    }
                } else if cur.eat_str("on:") {
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

    let mut children = MarkupList::default();
    if !self_closing {
        let (kids, stop) = parse_nodes(cur);
        children = kids;
        match stop {
            Stop::CloseTag => {
                let close_start = cur.pos();
                // Speculative: consume `</ident>` only if the name matches;
                // otherwise leave it for an ancestor element to match.
                let (tag_len, close_name) = scan_closing_tag(cur);
                if close_name == name {
                    cur.set_pos(close_start + tag_len as u32);
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
        annotations: Vec::new(),
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
        annotations: Vec::new(),
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
        annotations: Vec::new(),
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
        annotations: Vec::new(),
        scrutinee,
        before_arms: stray,
        arms,
        span: cur.span_from(start),
    })
}
