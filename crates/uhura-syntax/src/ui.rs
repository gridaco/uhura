//! Character-level parser for the opt-in Web UI profile.
//!
//! Markup is intentionally a distinct lexical mode. Every brace-delimited
//! expression and pattern re-enters the exact core parser, so the profile
//! neither embeds JavaScript nor grows a second expression language.

use super::ast::*;
use super::lexer::{Keyword, Token, TokenKind, Trivia, lex_fragment};
use super::markup::normalize_markup_text;
use super::parser::{
    FragmentParse, parse_expression_fragment, parse_expression_prefix, parse_pattern_fragment,
};
use super::{ParseDiagnostic, ParseDiagnosticKind};

pub(super) struct UiParse {
    pub nodes: Vec<UiNode>,
    pub diagnostics: Vec<ParseDiagnostic>,
    pub embedded_core_comments: Vec<Trivia>,
}

pub(super) fn parse_ui_body(identity: &SourceIdentity, source: &str, base: u32) -> UiParse {
    let mut parser = UiParser {
        identity,
        source,
        base,
        position: 0,
        diagnostics: Vec::new(),
        embedded_core_comments: Vec::new(),
    };
    let (nodes, boundary) = parser.nodes_until(&[]);
    if boundary != Boundary::Eof {
        parser.error(
            "unexpected closing UI construct",
            Span::empty(identity.file, parser.absolute()),
        );
    }
    UiParse {
        nodes,
        diagnostics: parser.diagnostics,
        embedded_core_comments: parser.embedded_core_comments,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Boundary {
    None,
    CloseElement,
    Else,
    EndIf,
    EndEach,
    Eof,
}

struct UiParser<'a> {
    identity: &'a SourceIdentity,
    source: &'a str,
    base: u32,
    position: usize,
    diagnostics: Vec<ParseDiagnostic>,
    embedded_core_comments: Vec<Trivia>,
}

#[derive(Clone, Debug)]
struct PendingAnnotation {
    annotation: MarkupAnnotation,
    carrier: usize,
}

impl UiParser<'_> {
    fn nodes_until(&mut self, stops: &[Boundary]) -> (Vec<UiNode>, Boundary) {
        let mut nodes = Vec::new();
        let mut pending = Vec::<PendingAnnotation>::new();
        loop {
            let boundary = self.boundary();
            if boundary == Boundary::Eof || stops.contains(&boundary) {
                self.diagnose_dangling_annotations(&pending);
                return (nodes, boundary);
            }

            let start = self.position;
            if self.rest().starts_with("<!--") {
                let comment = self.parse_comment();
                if let UiNodeKind::Comment(comment) = &comment.kind
                    && let Some(annotation) = &comment.annotation
                {
                    pending.push(PendingAnnotation {
                        annotation: annotation.clone(),
                        carrier: nodes.len(),
                    });
                }
                nodes.push(comment);
            } else if self.starts_directive("{#if") {
                let mut node = self.parse_if();
                self.attach_annotations(&mut node, &mut pending, &mut nodes);
                nodes.push(node);
            } else if self.starts_directive("{#each") {
                let mut node = self.parse_each();
                self.attach_annotations(&mut node, &mut pending, &mut nodes);
                nodes.push(node);
            } else if self.rest().starts_with("{#")
                || self.rest().starts_with("{:")
                || (self.rest().starts_with("{/") && !self.rest().starts_with("{//"))
            {
                self.parse_unknown_directive();
                self.reject_annotations(
                    &mut pending,
                    &mut nodes,
                    Span::new(
                        self.identity.file,
                        self.base.saturating_add(start as u32),
                        self.absolute(),
                    ),
                );
            } else if self.peek() == Some('{') {
                let mut node = self.parse_interpolation();
                self.attach_annotations(&mut node, &mut pending, &mut nodes);
                nodes.push(node);
            } else if self.rest().starts_with("</") {
                self.diagnose_dangling_annotations(&pending);
                return (nodes, Boundary::CloseElement);
            } else if self.peek() == Some('<') {
                let mut node = self.parse_element();
                self.attach_annotations(&mut node, &mut pending, &mut nodes);
                nodes.push(node);
            } else if let Some(text) = self.parse_text() {
                let mut text = text;
                self.attach_annotations(&mut text, &mut pending, &mut nodes);
                nodes.push(text);
            }

            if self.position == start {
                self.bump();
            }
        }
    }

    fn attach_annotations(
        &mut self,
        node: &mut UiNode,
        pending: &mut Vec<PendingAnnotation>,
        siblings: &mut [UiNode],
    ) {
        if pending.is_empty() {
            return;
        }

        let target = match &mut node.kind {
            UiNodeKind::Element(element) => Some(&mut element.annotations),
            UiNodeKind::If(value) => Some(&mut value.annotations),
            UiNodeKind::Each(value) => Some(&mut value.annotations),
            UiNodeKind::Text(_) | UiNodeKind::Comment(_) | UiNodeKind::Interpolation(_) => None,
        };

        if let Some(target) = target {
            target.extend(pending.drain(..).map(|pending| pending.annotation));
        } else {
            self.reject_annotations(pending, siblings, node.span);
        }
    }

    fn reject_annotations(
        &mut self,
        pending: &mut Vec<PendingAnnotation>,
        siblings: &mut [UiNode],
        target: Span,
    ) {
        for pending in pending.drain(..) {
            if let Some(UiNode {
                kind: UiNodeKind::Comment(comment),
                ..
            }) = siblings.get_mut(pending.carrier)
            {
                comment.status = UiCommentStatus::RejectedAnnotation;
            }
            let annotation = pending.annotation;
            self.diagnostics.push(
                ParseDiagnostic::new(
                    ParseDiagnosticKind::IncompatibleMetadataTarget,
                    "markup annotations target native UI elements or complete `if`/`each` blocks",
                    annotation.span,
                )
                .with_label(target, "this UI construct is not annotatable"),
            );
        }
    }

    fn diagnose_dangling_annotations(&mut self, pending: &[PendingAnnotation]) {
        let boundary = Span::empty(self.identity.file, self.absolute());
        for pending in pending {
            let annotation = &pending.annotation;
            self.diagnostics.push(
                ParseDiagnostic::new(
                    ParseDiagnosticKind::DanglingMetadata,
                    "markup annotation reaches a sibling-list boundary without a target",
                    annotation.span,
                )
                .with_label(boundary, "the annotation cannot cross this boundary"),
            );
        }
    }

    fn boundary(&self) -> Boundary {
        if self.is_eof() {
            Boundary::Eof
        } else if self.rest().starts_with("</") {
            Boundary::CloseElement
        } else if self.rest().starts_with("{:else}") {
            Boundary::Else
        } else if self.rest().starts_with("{/if}") {
            Boundary::EndIf
        } else if self.rest().starts_with("{/each}") {
            Boundary::EndEach
        } else {
            // A non-boundary directive is consumed by the recovery path.
            Boundary::None
        }
    }

    fn parse_comment(&mut self) -> UiNode {
        let start = self.absolute();
        self.eat_str("<!--");
        let content_start = self.position;
        let (body, mut status) = if let Some(close) = self.rest().find("-->") {
            let body = self.source[content_start..content_start + close].to_string();
            self.position = content_start + close;
            self.eat_str("-->");
            if body.contains("--") || body.ends_with('-') {
                self.diagnostic(
                    ParseDiagnosticKind::MalformedMarkupComment,
                    "markup comment bodies cannot contain `--` or end with `-`",
                    Span::new(self.identity.file, start, self.absolute()),
                );
                (body, UiCommentStatus::Malformed { terminated: true })
            } else {
                (body, UiCommentStatus::Ordinary)
            }
        } else {
            let recovery = find_comment_recovery(self.rest()).unwrap_or(self.rest().len());
            let body = self.source[content_start..content_start + recovery].to_string();
            self.position = content_start + recovery;
            self.diagnostic(
                ParseDiagnosticKind::MalformedMarkupComment,
                "unterminated markup comment",
                Span::new(self.identity.file, start, self.absolute()),
            );
            (body, UiCommentStatus::Malformed { terminated: false })
        };
        let span = Span::new(self.identity.file, start, self.absolute());
        let annotation = if status == UiCommentStatus::Ordinary {
            match classify_annotation(&body) {
                Ok(Some((kind, text))) => {
                    status = UiCommentStatus::Annotation;
                    Some(MarkupAnnotation { kind, text, span })
                }
                Ok(None) => None,
                Err(message) => {
                    self.diagnostic(ParseDiagnosticKind::MalformedMarkupComment, message, span);
                    status = UiCommentStatus::Malformed { terminated: true };
                    None
                }
            }
        } else {
            None
        };
        Node::new(
            UiNodeKind::Comment(UiComment {
                body,
                status,
                annotation,
            }),
            span,
        )
    }

    fn parse_if(&mut self) -> UiNode {
        let start = self.absolute();
        let (header, header_base, open_span) = self.directive_header("{#if");
        let condition = self.expression(&header, header_base);
        let (then_branch, boundary) = self.nodes_until(&[Boundary::Else, Boundary::EndIf]);

        let (else_branch, else_span, boundary) = if boundary == Boundary::Else {
            let else_start = self.absolute();
            self.eat_str("{:else}");
            let else_span = Span::new(self.identity.file, else_start, self.absolute());
            let (nodes, boundary) = self.nodes_until(&[Boundary::EndIf]);
            (Some(nodes), Some(else_span), boundary)
        } else {
            (None, None, boundary)
        };

        let close_start = self.absolute();
        if boundary == Boundary::EndIf {
            self.eat_str("{/if}");
        } else {
            self.error(
                "missing `{/if}`",
                Span::empty(self.identity.file, self.absolute()),
            );
        }
        let close_span = Span::new(self.identity.file, close_start, self.absolute());
        Node::new(
            UiNodeKind::If(UiIf {
                condition,
                then_branch,
                else_branch,
                annotations: Vec::new(),
                open_span,
                else_span,
                close_span,
            }),
            Span::new(self.identity.file, start, self.absolute()),
        )
    }

    fn parse_each(&mut self) -> UiNode {
        let start = self.absolute();
        let (header, header_base, open_span) = self.directive_header("{#each");
        let (source, pattern, key) = self.each_header(&header, header_base);
        let (children, boundary) = self.nodes_until(&[Boundary::EndEach]);
        let close_start = self.absolute();
        if boundary == Boundary::EndEach {
            self.eat_str("{/each}");
        } else {
            self.error(
                "missing `{/each}`",
                Span::empty(self.identity.file, self.absolute()),
            );
        }
        let close_span = Span::new(self.identity.file, close_start, self.absolute());
        Node::new(
            UiNodeKind::Each(UiEach {
                source,
                pattern,
                key,
                children,
                annotations: Vec::new(),
                open_span,
                close_span,
            }),
            Span::new(self.identity.file, start, self.absolute()),
        )
    }

    fn each_header(&mut self, header: &str, header_base: u32) -> (Expression, Pattern, Expression) {
        let lexical = lex_fragment(self.identity.file, header, header_base);
        self.diagnostics
            .extend(lexical.diagnostics.into_iter().map(Into::into));
        let tokens = lexical.tokens;
        let as_index = top_level_as(&tokens);
        let Some(as_index) = as_index else {
            let span = Span::new(
                self.identity.file,
                header_base,
                header_base.saturating_add(header.len() as u32),
            );
            self.error("UI `each` requires `expression as pattern (key)`", span);
            return (
                self.error_expression(span),
                self.error_pattern(span),
                self.error_expression(span),
            );
        };

        let last = tokens
            .iter()
            .rposition(|token| token.kind != TokenKind::Eof)
            .unwrap_or(as_index);
        let key_pair = top_level_final_parentheses(&tokens, as_index + 1, last);
        let Some((key_open, key_close)) = key_pair else {
            let span = tokens[as_index].span;
            self.error("UI `each` requires a final parenthesized key", span);
            return (
                self.expression(
                    slice_absolute(
                        header,
                        header_base,
                        header_base,
                        tokens[as_index].span.start,
                    ),
                    header_base,
                ),
                self.error_pattern(span),
                self.error_expression(span),
            );
        };

        let source_raw = slice_absolute(
            header,
            header_base,
            header_base,
            tokens[as_index].span.start,
        );
        let source = self.expression(source_raw, header_base);

        let pattern_start = tokens[as_index].span.end;
        let pattern_raw = slice_absolute(
            header,
            header_base,
            pattern_start,
            tokens[key_open].span.start,
        );
        let pattern = self.pattern(pattern_raw, pattern_start);

        let key_start = tokens[key_open].span.end;
        let key_raw = slice_absolute(header, header_base, key_start, tokens[key_close].span.start);
        let key = self.expression(key_raw, key_start);
        (source, pattern, key)
    }

    fn parse_interpolation(&mut self) -> UiNode {
        let start = self.absolute();
        let (source, source_base, _) = self.braced();
        let expression = self.expression(&source, source_base);
        Node::new(
            UiNodeKind::Interpolation(expression),
            Span::new(self.identity.file, start, self.absolute()),
        )
    }

    fn parse_element(&mut self) -> UiNode {
        let start = self.absolute();
        self.eat('<');
        let name = self.name(true, "element name");
        let mut attributes = Vec::new();
        let mut self_closing = false;

        loop {
            self.skip_whitespace();
            if self.eat_str("/>") {
                self_closing = true;
                break;
            }
            if self.eat('>') {
                break;
            }
            if self.is_eof() {
                self.error(
                    "unterminated UI opening tag",
                    Span::new(self.identity.file, start, self.absolute()),
                );
                break;
            }

            let before = self.position;
            attributes.push(self.parse_attribute(name.kind));
            if self.position == before {
                self.bump();
            }
        }
        let open_span = Span::new(self.identity.file, start, self.absolute());

        let (children, close_span) = if self_closing {
            (Vec::new(), None)
        } else {
            let (children, boundary) = self.nodes_until(&[Boundary::CloseElement]);
            if boundary == Boundary::CloseElement && self.rest().starts_with("</") {
                let close_start = self.absolute();
                self.eat_str("</");
                let close_name = self.name(true, "closing element name");
                if close_name.text != name.text {
                    self.error(
                        format!(
                            "closing UI element `{}` does not match `{}`",
                            close_name.text, name.text
                        ),
                        close_name.span,
                    );
                }
                self.skip_whitespace();
                if !self.eat('>') {
                    self.error(
                        "expected `>` after closing UI element",
                        Span::empty(self.identity.file, self.absolute()),
                    );
                }
                (
                    children,
                    Some(Span::new(self.identity.file, close_start, self.absolute())),
                )
            } else {
                self.error(
                    format!("missing closing UI element `</{}>`", name.text),
                    Span::empty(self.identity.file, self.absolute()),
                );
                (children, None)
            }
        };

        Node::new(
            UiNodeKind::Element(UiElement {
                name,
                attributes,
                children,
                annotations: Vec::new(),
                self_closing,
                open_span,
                close_span,
            }),
            Span::new(self.identity.file, start, self.absolute()),
        )
    }

    fn parse_attribute(&mut self, element_kind: UiNameKind) -> UiAttribute {
        let start = self.absolute();
        let name = match element_kind {
            UiNameKind::Native => self.name(false, "attribute name"),
            UiNameKind::Component => self.component_prop_name("component prop name"),
        };
        if name.text == "on" {
            if !self.skip_whitespace() {
                self.error(
                    "`on` requires a semantic event name",
                    Span::empty(self.identity.file, self.absolute()),
                );
            }
            // UpperCamel tags are not necessarily user components: standard
            // extensions such as `<Link>` also use this spelling and expose
            // lowercase semantic events. Parse both identifier domains here;
            // checked element resolution decides whether the event is a
            // standard-element event or an exact component emit variant.
            let event = self.name(
                element_kind == UiNameKind::Component,
                if element_kind == UiNameKind::Component {
                    "semantic event or emitted variant name"
                } else {
                    "semantic event name"
                },
            );
            self.skip_whitespace();
            if !self.eat_str("->") {
                self.error(
                    "expected `->` in semantic UI event binding",
                    Span::empty(self.identity.file, self.absolute()),
                );
            }
            self.skip_whitespace();
            let expression_start = self.position;
            let expression_end = self.tag_content_end();
            let raw = &self.source[expression_start..expression_end];
            let parsed = parse_expression_prefix(self.identity, raw, self.absolute());
            self.take_fragment_diagnostics_and_comments(&parsed);
            if parsed.consumed <= self.absolute() {
                self.error(
                    "semantic UI event binding requires an input constructor",
                    Span::empty(self.identity.file, self.absolute()),
                );
            } else {
                self.position = parsed.consumed.saturating_sub(self.base) as usize;
            }
            return UiAttribute::Event {
                event,
                input: parsed.value,
                span: Span::new(self.identity.file, start, self.absolute()),
            };
        }

        self.skip_whitespace();
        if !self.eat('=') {
            return UiAttribute::Boolean {
                name,
                span: Span::new(self.identity.file, start, self.absolute()),
            };
        }
        self.skip_whitespace();
        if self.peek() == Some('"') {
            let value = self.quoted();
            UiAttribute::StaticText {
                name,
                value,
                span: Span::new(self.identity.file, start, self.absolute()),
            }
        } else if self.peek() == Some('{') {
            let (source, source_base, _) = self.braced();
            let value = self.expression(&source, source_base);
            UiAttribute::Expression {
                name,
                value,
                span: Span::new(self.identity.file, start, self.absolute()),
            }
        } else {
            self.error(
                "UI attribute value must be quoted text or `{expression}`",
                Span::empty(self.identity.file, self.absolute()),
            );
            self.recover_attribute_value();
            UiAttribute::StaticText {
                name,
                value: String::new(),
                span: Span::new(self.identity.file, start, self.absolute()),
            }
        }
    }

    fn parse_text(&mut self) -> Option<UiNode> {
        let start = self.position;
        while !self.is_eof() && !matches!(self.peek(), Some('<' | '{')) {
            self.bump();
        }
        let raw = &self.source[start..self.position];
        if raw.chars().all(is_layout_whitespace) {
            return None;
        }
        Some(Node::new(
            UiNodeKind::Text(UiText {
                raw: raw.to_string(),
            }),
            Span::new(
                self.identity.file,
                self.base.saturating_add(start as u32),
                self.absolute(),
            ),
        ))
    }

    fn expression(&mut self, source: &str, base: u32) -> Expression {
        let (source, offset) = trim_core(source);
        let parsed =
            parse_expression_fragment(self.identity, source, base.saturating_add(offset as u32));
        self.take_fragment_diagnostics_and_comments(&parsed);
        parsed.value
    }

    fn pattern(&mut self, source: &str, base: u32) -> Pattern {
        let (source, offset) = trim_core(source);
        let parsed =
            parse_pattern_fragment(self.identity, source, base.saturating_add(offset as u32));
        self.take_fragment_diagnostics_and_comments(&parsed);
        parsed.value
    }

    fn take_fragment_diagnostics_and_comments<T>(&mut self, parsed: &FragmentParse<T>) {
        self.diagnostics.extend(parsed.diagnostics.iter().cloned());
        self.embedded_core_comments
            .extend(parsed.comments.iter().cloned());
    }

    fn parse_unknown_directive(&mut self) {
        let start = self.absolute();
        let label = if self.rest().starts_with("{#match") {
            "the UI profile has no `match` block; use a core `match` expression in `{...}`"
        } else {
            "unknown or misplaced UI directive"
        };
        self.consume_directive();
        self.error(label, Span::new(self.identity.file, start, self.absolute()));
    }

    fn consume_directive(&mut self) {
        let (_, _, _) = self.braced();
    }

    fn braced(&mut self) -> (String, u32, Span) {
        let start = self.absolute();
        if !self.eat('{') {
            self.error(
                "expected `{`",
                Span::empty(self.identity.file, self.absolute()),
            );
            return (
                String::new(),
                self.absolute(),
                Span::empty(self.identity.file, self.absolute()),
            );
        }
        let content_start = self.position;
        let content_base = self.absolute();
        if let Some(content_end) = self.scan_balanced_brace() {
            let source = self.source[content_start..content_end].to_string();
            self.position = content_end;
            self.eat('}');
            (
                source,
                content_base,
                Span::new(self.identity.file, start, self.absolute()),
            )
        } else {
            let source = self.source[content_start..].to_string();
            self.position = self.source.len();
            self.error(
                "unterminated UI brace",
                Span::new(self.identity.file, start, self.absolute()),
            );
            (
                source,
                content_base,
                Span::new(self.identity.file, start, self.absolute()),
            )
        }
    }

    fn directive_header(&mut self, prefix: &str) -> (String, u32, Span) {
        let start = self.absolute();
        if !self.eat_str(prefix) {
            self.error(
                format!("expected `{prefix}`"),
                Span::empty(self.identity.file, self.absolute()),
            );
        }
        let content_start = self.position;
        let content_base = self.absolute();
        if let Some(content_end) = self.scan_balanced_brace() {
            let source = self.source[content_start..content_end].to_string();
            self.position = content_end;
            self.eat('}');
            (
                source,
                content_base,
                Span::new(self.identity.file, start, self.absolute()),
            )
        } else {
            let source = self.source[content_start..].to_string();
            self.position = self.source.len();
            self.error(
                format!("unterminated `{prefix}` directive"),
                Span::new(self.identity.file, start, self.absolute()),
            );
            (
                source,
                content_base,
                Span::new(self.identity.file, start, self.absolute()),
            )
        }
    }

    /// The opening brace has already been consumed (directly or as part of a
    /// directive prefix). Return the relative byte of its matching close.
    fn scan_balanced_brace(&self) -> Option<usize> {
        let mut position = self.position;
        let mut depth = 1u32;
        let mut string = false;
        let mut escaped = false;
        let mut line_comment = false;
        while position < self.source.len() {
            let value = self.source[position..].chars().next()?;
            if line_comment {
                position += value.len_utf8();
                if matches!(value, '\n' | '\r') {
                    line_comment = false;
                }
                continue;
            }
            if string {
                position += value.len_utf8();
                if escaped {
                    escaped = false;
                } else if value == '\\' {
                    escaped = true;
                } else if value == '"' {
                    string = false;
                }
                continue;
            }
            if self.source[position..].starts_with("//") {
                position += 2;
                line_comment = true;
                continue;
            }
            match value {
                '"' => string = true,
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(position);
                    }
                }
                _ => {}
            }
            position += value.len_utf8();
        }
        None
    }

    fn starts_directive(&self, prefix: &str) -> bool {
        self.rest().strip_prefix(prefix).is_some_and(|tail| {
            tail.chars()
                .next()
                .is_some_and(|value| is_layout_whitespace(value) || value == '}')
        })
    }

    fn tag_content_end(&self) -> usize {
        let mut position = self.position;
        let mut paren = 0u32;
        let mut bracket = 0u32;
        let mut brace = 0u32;
        let mut string = false;
        let mut escaped = false;
        while position < self.source.len() {
            let rest = &self.source[position..];
            let value = rest.chars().next().expect("position is in bounds");
            if string {
                position += value.len_utf8();
                if escaped {
                    escaped = false;
                } else if value == '\\' {
                    escaped = true;
                } else if value == '"' {
                    string = false;
                }
                continue;
            }
            match value {
                '"' => string = true,
                '(' => paren += 1,
                ')' => paren = paren.saturating_sub(1),
                '[' => bracket += 1,
                ']' => bracket = bracket.saturating_sub(1),
                '{' => brace += 1,
                '}' => brace = brace.saturating_sub(1),
                '>' if paren == 0 && bracket == 0 && brace == 0 => break,
                '/' if paren == 0 && bracket == 0 && brace == 0 && rest.starts_with("/>") => {
                    break;
                }
                _ => {}
            }
            position += value.len_utf8();
        }
        position
    }

    fn quoted(&mut self) -> String {
        if !self.eat('"') {
            return String::new();
        }
        let start = self.position;
        while let Some(value) = self.peek() {
            if value == '"' {
                let result = self.source[start..self.position].to_string();
                self.bump();
                return result;
            }
            self.bump();
        }
        self.error(
            "unterminated quoted UI attribute",
            Span::new(
                self.identity.file,
                self.base.saturating_add(start as u32).saturating_sub(1),
                self.absolute(),
            ),
        );
        self.source[start..].to_string()
    }

    fn name(&mut self, allow_component: bool, context: &str) -> UiName {
        let start = self.position;
        let absolute_start = self.absolute();
        let first = self.peek();
        let kind = match first {
            Some(value) if value.is_ascii_lowercase() => UiNameKind::Native,
            Some(value) if allow_component && value.is_ascii_uppercase() => UiNameKind::Component,
            _ => {
                self.error(
                    format!(
                        "expected {context} (lowercase HTML-shaped or UpperCamelCase component name)"
                    ),
                    Span::empty(self.identity.file, absolute_start),
                );
                if !self.is_eof() {
                    self.bump();
                }
                return UiName {
                    text: self.source[start..self.position].to_string(),
                    kind: UiNameKind::Native,
                    span: Span::new(self.identity.file, absolute_start, self.absolute()),
                };
            }
        };
        self.bump();
        match kind {
            UiNameKind::Native => {
                while self
                    .peek()
                    .is_some_and(|value| value.is_ascii_lowercase() || value.is_ascii_digit())
                {
                    self.bump();
                }
                while self.peek() == Some('-') {
                    self.bump();
                    if !self
                        .peek()
                        .is_some_and(|value| value.is_ascii_lowercase() || value.is_ascii_digit())
                    {
                        self.error(
                            format!("{context} cannot end a segment with `-`"),
                            Span::new(self.identity.file, absolute_start, self.absolute()),
                        );
                        break;
                    }
                    while self
                        .peek()
                        .is_some_and(|value| value.is_ascii_lowercase() || value.is_ascii_digit())
                    {
                        self.bump();
                    }
                }
            }
            UiNameKind::Component => {
                while self
                    .peek()
                    .is_some_and(|value| value.is_ascii_alphanumeric())
                {
                    self.bump();
                }
            }
        }
        UiName {
            text: self.source[start..self.position].to_string(),
            kind,
            span: Span::new(self.identity.file, absolute_start, self.absolute()),
        }
    }

    fn component_prop_name(&mut self, context: &str) -> UiName {
        let start = self.position;
        let absolute_start = self.absolute();
        if !self.peek().is_some_and(|value| value.is_ascii_lowercase()) {
            self.error(
                format!("expected {context} (lower_snake_case)"),
                Span::empty(self.identity.file, absolute_start),
            );
            if !self.is_eof() {
                self.bump();
            }
            return UiName {
                text: self.source[start..self.position].to_string(),
                kind: UiNameKind::Native,
                span: Span::new(self.identity.file, absolute_start, self.absolute()),
            };
        }

        self.bump();
        while self
            .peek()
            .is_some_and(|value| value.is_ascii_lowercase() || value.is_ascii_digit())
        {
            self.bump();
        }
        while self.peek() == Some('_') {
            self.bump();
            if !self
                .peek()
                .is_some_and(|value| value.is_ascii_lowercase() || value.is_ascii_digit())
            {
                self.error(
                    format!("{context} cannot end a segment with `_`"),
                    Span::new(self.identity.file, absolute_start, self.absolute()),
                );
                break;
            }
            while self
                .peek()
                .is_some_and(|value| value.is_ascii_lowercase() || value.is_ascii_digit())
            {
                self.bump();
            }
        }

        UiName {
            text: self.source[start..self.position].to_string(),
            kind: UiNameKind::Native,
            span: Span::new(self.identity.file, absolute_start, self.absolute()),
        }
    }

    fn recover_attribute_value(&mut self) {
        while !self.is_eof()
            && !self.peek().is_some_and(is_layout_whitespace)
            && self.peek() != Some('>')
            && !self.rest().starts_with("/>")
        {
            self.bump();
        }
    }

    fn skip_whitespace(&mut self) -> bool {
        let start = self.position;
        while self.peek().is_some_and(is_layout_whitespace) {
            self.bump();
        }
        self.position > start
    }

    fn rest(&self) -> &str {
        &self.source[self.position..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let value = self.peek()?;
        self.position += value.len_utf8();
        Some(value)
    }

    fn eat(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn eat_str(&mut self, expected: &str) -> bool {
        if self.rest().starts_with(expected) {
            self.position += expected.len();
            true
        } else {
            false
        }
    }

    fn absolute(&self) -> u32 {
        self.base.saturating_add(self.position as u32)
    }

    fn is_eof(&self) -> bool {
        self.position >= self.source.len()
    }

    fn error_expression(&self, span: Span) -> Expression {
        Node::new(
            ExpressionKind::Name(QualifiedName {
                segments: vec![Identifier::new("error", span)],
                span,
            }),
            span,
        )
    }

    fn error_pattern(&self, span: Span) -> Pattern {
        Node::new(PatternKind::Wildcard, span)
    }

    fn error(&mut self, message: impl Into<String>, span: Span) {
        self.diagnostic(ParseDiagnosticKind::InvalidUi, message, span);
    }

    fn diagnostic(&mut self, kind: ParseDiagnosticKind, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(ParseDiagnostic::new(kind, message, span));
    }
}

fn classify_annotation(body: &str) -> Result<Option<(String, String)>, &'static str> {
    let body = normalize_line_endings(body);
    let leading_trimmed = body.trim_start_matches([' ', '\t', '\n']);
    if !leading_trimmed.starts_with('@') {
        return Ok(None);
    }

    let marker_tail = &leading_trimmed[1..];
    let Some((kind_end, separator)) = marker_tail
        .char_indices()
        .find(|(_, value)| matches!(value, ' ' | '\t' | '\n'))
    else {
        return Err("annotation markers require a kind, whitespace, and a non-empty payload");
    };
    let kind = &marker_tail[..kind_end];
    let payload_start = kind_end + separator.len_utf8();
    let payload = normalize_markup_text(&marker_tail[payload_start..]);
    if !valid_annotation_kind(kind) || payload.is_empty() {
        return Err(
            "annotation kind must be 1-64 lowercase kebab bytes and its payload must be non-empty",
        );
    }
    Ok(Some((kind.to_string(), payload)))
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

fn normalize_line_endings(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

fn find_comment_recovery(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let mut line = 0;
    while line < bytes.len() {
        let relative = rest[line..].find(['\n', '\r'])?;
        let mut next = line + relative + 1;
        if bytes.get(line + relative) == Some(&b'\r') && bytes.get(next) == Some(&b'\n') {
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
        || rest.starts_with("{#")
        || rest.starts_with("{:")
        || rest.starts_with("{/")
        || rest.starts_with("</")
        || rest
            .strip_prefix('<')
            .and_then(|tail| tail.as_bytes().first())
            .is_some_and(u8::is_ascii_alphabetic)
}

fn top_level_as(tokens: &[Token]) -> Option<usize> {
    let mut paren = 0u32;
    let mut bracket = 0u32;
    let mut brace = 0u32;
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::LParen => paren += 1,
            TokenKind::RParen => paren = paren.saturating_sub(1),
            TokenKind::LBracket => bracket += 1,
            TokenKind::RBracket => bracket = bracket.saturating_sub(1),
            TokenKind::LBrace => brace += 1,
            TokenKind::RBrace => brace = brace.saturating_sub(1),
            TokenKind::Keyword(Keyword::As) if paren == 0 && bracket == 0 && brace == 0 => {
                return Some(index);
            }
            _ => {}
        }
    }
    None
}

fn top_level_final_parentheses(
    tokens: &[Token],
    start: usize,
    last: usize,
) -> Option<(usize, usize)> {
    let mut paren = 0u32;
    let mut bracket = 0u32;
    let mut brace = 0u32;
    for index in start..=last {
        match tokens[index].kind {
            TokenKind::LParen if paren == 0 && bracket == 0 && brace == 0 => {
                let close = matching_parenthesis(tokens, index)?;
                if close == last {
                    return Some((index, close));
                }
                paren += 1;
            }
            TokenKind::LParen => paren += 1,
            TokenKind::RParen => paren = paren.saturating_sub(1),
            TokenKind::LBracket => bracket += 1,
            TokenKind::RBracket => bracket = bracket.saturating_sub(1),
            TokenKind::LBrace => brace += 1,
            TokenKind::RBrace => brace = brace.saturating_sub(1),
            _ => {}
        }
    }
    None
}

fn matching_parenthesis(tokens: &[Token], open: usize) -> Option<usize> {
    let mut depth = 0u32;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        match token.kind {
            TokenKind::LParen => depth += 1,
            TokenKind::RParen => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn slice_absolute(source: &str, base: u32, start: u32, end: u32) -> &str {
    let start = start.saturating_sub(base) as usize;
    let end = end.saturating_sub(base) as usize;
    source.get(start..end).unwrap_or_default()
}

fn trim_core(source: &str) -> (&str, usize) {
    let start = source
        .char_indices()
        .find_map(|(index, value)| (!is_layout_whitespace(value)).then_some(index))
        .unwrap_or(source.len());
    let end = source
        .char_indices()
        .rev()
        .find_map(|(index, value)| {
            (!is_layout_whitespace(value)).then_some(index + value.len_utf8())
        })
        .unwrap_or(start);
    (&source[start..end], start)
}

fn is_layout_whitespace(value: char) -> bool {
    matches!(value, ' ' | '\t' | '\n' | '\r')
}
