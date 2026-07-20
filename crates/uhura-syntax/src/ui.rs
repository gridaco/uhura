//! Character-level parser for Uhura's bounded checked UI grammar.

use unicode_ident::{is_xid_continue, is_xid_start};

use super::ast::*;
use super::lexer::{TokenKind, lex_fragment};
use super::parser::{
    ParseDiagnostic, ParseDiagnosticKind, parse_expression_fragment, parse_pattern_fragment,
};

pub(crate) struct UiParse {
    pub nodes: Vec<UiNode>,
    pub diagnostics: Vec<ParseDiagnostic>,
}

pub(crate) fn parse_ui_body(file: u32, source: &str, base: u32) -> UiParse {
    let mut parser = UiParser {
        file,
        source,
        base,
        pos: 0,
        diagnostics: Vec::new(),
    };
    let nodes = parser.nodes_until(&[]);
    UiParse {
        nodes,
        diagnostics: parser.diagnostics,
    }
}

struct UiParser<'a> {
    file: u32,
    source: &'a str,
    base: u32,
    pos: usize,
    diagnostics: Vec<ParseDiagnostic>,
}

impl UiParser<'_> {
    fn nodes_until(&mut self, stops: &[&str]) -> Vec<UiNode> {
        let mut nodes = Vec::new();
        while !self.is_eof() && !stops.iter().any(|stop| self.rest().starts_with(stop)) {
            let before = self.pos;
            if self.rest().starts_with("{#if") {
                nodes.push(self.parse_if());
            } else if self.rest().starts_with("{#match") {
                nodes.push(self.parse_match());
            } else if self.rest().starts_with("{#each") {
                nodes.push(self.parse_each());
            } else if self.rest().starts_with("{#") || self.rest().starts_with("{/") {
                let start = self.absolute();
                self.error(
                    "unexpected UI directive",
                    SourceSpan::new(self.file, start, start.saturating_add(2)),
                );
                self.bump();
            } else if self.peek() == Some('{') {
                nodes.push(self.parse_interpolation());
            } else if self.rest().starts_with("</") {
                break;
            } else if self.peek() == Some('<') {
                nodes.push(self.parse_element());
            } else if let Some(text) = self.parse_text() {
                nodes.push(text);
            }
            if before == self.pos {
                self.bump();
            }
        }
        nodes
    }

    fn parse_if(&mut self) -> UiNode {
        let start = self.absolute();
        let (header, header_base, end) = self.directive_header("{#if");
        let (condition, diagnostics) =
            parse_expression_fragment(self.file, header.trim(), header_base + leading_ws(&header));
        self.diagnostics.extend(diagnostics);
        let children = self.nodes_until(&["{/if}"]);
        if !self.eat_str("{/if}") {
            self.error(
                "missing `{/if}`",
                SourceSpan::empty(self.file, self.absolute()),
            );
        }
        Spanned::new(
            UiNodeKind::If {
                condition,
                children,
            },
            SourceSpan::new(self.file, start, self.absolute().max(end)),
        )
    }

    fn parse_match(&mut self) -> UiNode {
        let start = self.absolute();
        let (header, header_base, _) = self.directive_header("{#match");
        let trim = header.trim();
        let (subject, diagnostics) =
            parse_expression_fragment(self.file, trim, header_base + leading_ws(&header));
        self.diagnostics.extend(diagnostics);
        let mut cases = Vec::new();
        loop {
            self.skip_whitespace();
            if !self.rest().starts_with("{#case") {
                break;
            }
            let case_start = self.absolute();
            let (pattern_source, pattern_base, _) = self.directive_header("{#case");
            let trim = pattern_source.trim();
            let (pattern, diagnostics) =
                parse_pattern_fragment(self.file, trim, pattern_base + leading_ws(&pattern_source));
            self.diagnostics.extend(diagnostics);
            let children = self.nodes_until(&["{#case", "{/match}"]);
            cases.push(UiCase {
                pattern,
                children,
                span: SourceSpan::new(self.file, case_start, self.absolute()),
            });
        }
        if !self.eat_str("{/match}") {
            self.error(
                "missing `{/match}`",
                SourceSpan::empty(self.file, self.absolute()),
            );
        }
        Spanned::new(
            UiNodeKind::Match { subject, cases },
            SourceSpan::new(self.file, start, self.absolute()),
        )
    }

    fn parse_each(&mut self) -> UiNode {
        let start = self.absolute();
        let (header, header_base, _) = self.directive_header("{#each");
        let (source, pattern, key) = self.parse_each_header(&header, header_base);
        let children = self.nodes_until(&["{/each}"]);
        if !self.eat_str("{/each}") {
            self.error(
                "missing `{/each}`",
                SourceSpan::empty(self.file, self.absolute()),
            );
        }
        Spanned::new(
            UiNodeKind::Each {
                source,
                pattern,
                key,
                children,
            },
            SourceSpan::new(self.file, start, self.absolute()),
        )
    }

    fn parse_each_header(&mut self, header: &str, header_base: u32) -> (Expr, Pattern, Expr) {
        let lexical = lex_fragment(self.file, header, header_base);
        self.diagnostics
            .extend(lexical.diagnostics.into_iter().map(Into::into));
        let tokens = lexical.tokens;
        let mut depths = (0i32, 0i32, 0i32);
        let mut as_index = None;
        for (index, token) in tokens.iter().enumerate() {
            match token.kind {
                TokenKind::LParen => depths.0 += 1,
                TokenKind::RParen => depths.0 -= 1,
                TokenKind::LBracket => depths.1 += 1,
                TokenKind::RBracket => depths.1 -= 1,
                TokenKind::LBrace => depths.2 += 1,
                TokenKind::RBrace => depths.2 -= 1,
                TokenKind::Ident(ref value) if value == "as" && depths == (0, 0, 0) => {
                    as_index = Some(index);
                    break;
                }
                _ => {}
            }
        }
        let Some(as_index) = as_index else {
            let span = SourceSpan::new(
                self.file,
                header_base,
                header_base.saturating_add(header.len() as u32),
            );
            self.error("UI `each` requires `as pattern (key)`", span);
            return (error_expr(span), error_pattern(span), error_expr(span));
        };

        let as_token = &tokens[as_index];
        let source_end = (as_token.span.start - header_base) as usize;
        let source_text = &header[..source_end];
        let source_trim = source_text.trim();
        let source_base = header_base + leading_ws(source_text);
        let (source, diagnostics) = parse_expression_fragment(self.file, source_trim, source_base);
        self.diagnostics.extend(diagnostics);

        let mut depth = 0i32;
        let mut key_open_index = None;
        for (index, token) in tokens.iter().enumerate().skip(as_index + 1) {
            match token.kind {
                TokenKind::LParen if depth == 0 => {
                    key_open_index = Some(index);
                    depth += 1;
                }
                TokenKind::LParen => depth += 1,
                TokenKind::RParen => depth -= 1,
                _ => {}
            }
        }
        let Some(key_open_index) = key_open_index else {
            let span = as_token.span;
            self.error("UI `each` requires a parenthesized key", span);
            return (source, error_pattern(span), error_expr(span));
        };
        let key_close_index = matching_token(
            &tokens,
            key_open_index,
            TokenKind::LParen,
            TokenKind::RParen,
        )
        .unwrap_or(key_open_index);
        let pattern_start = tokens
            .get(as_index + 1)
            .map_or(as_token.span.end, |token| token.span.start);
        let pattern_end = tokens[key_open_index].span.start;
        let pattern_text = slice_absolute(header, header_base, pattern_start, pattern_end).trim();
        let pattern_base = pattern_start
            + leading_ws(slice_absolute(
                header,
                header_base,
                pattern_start,
                pattern_end,
            ));
        let (pattern, diagnostics) = parse_pattern_fragment(self.file, pattern_text, pattern_base);
        self.diagnostics.extend(diagnostics);

        let key_start = tokens[key_open_index].span.end;
        let key_end = tokens[key_close_index].span.start;
        let key_raw = slice_absolute(header, header_base, key_start, key_end);
        let key_text = key_raw.trim();
        let (key, diagnostics) =
            parse_expression_fragment(self.file, key_text, key_start + leading_ws(key_raw));
        self.diagnostics.extend(diagnostics);
        (source, pattern, key)
    }

    fn parse_interpolation(&mut self) -> UiNode {
        let start = self.absolute();
        let (source, source_base, _) = self.braced();
        let trim = source.trim();
        let (expression, diagnostics) =
            parse_expression_fragment(self.file, trim, source_base + leading_ws(&source));
        self.diagnostics.extend(diagnostics);
        Spanned::new(
            UiNodeKind::Interpolation(expression),
            SourceSpan::new(self.file, start, self.absolute()),
        )
    }

    fn parse_element(&mut self) -> UiNode {
        let start = self.absolute();
        self.eat('<');
        let name_start = self.absolute();
        let name_value = self.name();
        let name = Spanned::new(
            name_value.clone(),
            SourceSpan::new(self.file, name_start, self.absolute()),
        );
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
                    SourceSpan::new(self.file, start, self.absolute()),
                );
                break;
            }

            let attribute_start = self.absolute();
            let attribute_name = self.name();
            if attribute_name == "on" {
                self.skip_whitespace();
                let event_start = self.absolute();
                let event_value = self.name();
                let event = Spanned::new(
                    event_value,
                    SourceSpan::new(self.file, event_start, self.absolute()),
                );
                self.skip_whitespace();
                if !self.eat_str("->") {
                    self.error(
                        "expected `->` in UI event binding",
                        SourceSpan::empty(self.file, self.absolute()),
                    );
                }
                self.skip_whitespace();
                let expression_start = self.pos;
                self.scan_to_tag_end();
                let expression_end = self.pos;
                let raw = &self.source[expression_start..expression_end];
                let trim = raw.trim();
                let (input, diagnostics) = parse_expression_fragment(
                    self.file,
                    trim,
                    self.base + expression_start as u32 + leading_ws(raw),
                );
                self.diagnostics.extend(diagnostics);
                attributes.push(UiAttribute {
                    name: "on".into(),
                    value: UiAttributeValue::Event { event, input },
                    span: SourceSpan::new(self.file, attribute_start, self.absolute()),
                });
                continue;
            }

            self.skip_whitespace();
            if !self.eat('=') {
                self.error(
                    "UI attributes require `=`",
                    SourceSpan::new(self.file, attribute_start, self.absolute()),
                );
                attributes.push(UiAttribute {
                    name: attribute_name,
                    value: UiAttributeValue::Text(String::new()),
                    span: SourceSpan::new(self.file, attribute_start, self.absolute()),
                });
                continue;
            }
            self.skip_whitespace();
            let value = if self.peek() == Some('"') {
                UiAttributeValue::Text(self.quoted())
            } else if self.peek() == Some('{') {
                let (raw, raw_base, _) = self.braced();
                let trim = raw.trim();
                let (expression, diagnostics) =
                    parse_expression_fragment(self.file, trim, raw_base + leading_ws(&raw));
                self.diagnostics.extend(diagnostics);
                UiAttributeValue::Expression(expression)
            } else {
                self.error(
                    "UI attribute value must be quoted text or `{expression}`",
                    SourceSpan::empty(self.file, self.absolute()),
                );
                UiAttributeValue::Text(String::new())
            };
            attributes.push(UiAttribute {
                name: attribute_name,
                value,
                span: SourceSpan::new(self.file, attribute_start, self.absolute()),
            });
        }

        let children = if self_closing {
            Vec::new()
        } else {
            let children = self.nodes_until(&["</"]);
            if self.eat_str("</") {
                let close_start = self.absolute();
                let close_name = self.name();
                if close_name != name_value {
                    self.error(
                        format!("closing UI element `{close_name}` does not match `{name_value}`"),
                        SourceSpan::new(self.file, close_start, self.absolute()),
                    );
                }
                self.skip_whitespace();
                if !self.eat('>') {
                    self.error(
                        "expected `>` after closing UI element",
                        SourceSpan::empty(self.file, self.absolute()),
                    );
                }
            } else {
                self.error(
                    format!("missing closing UI element `</{name_value}>`"),
                    SourceSpan::empty(self.file, self.absolute()),
                );
            }
            children
        };
        Spanned::new(
            UiNodeKind::Element(UiElement {
                name,
                attributes,
                children,
                self_closing,
            }),
            SourceSpan::new(self.file, start, self.absolute()),
        )
    }

    fn parse_text(&mut self) -> Option<UiNode> {
        let start = self.pos;
        while !self.is_eof() && !matches!(self.peek(), Some('<' | '{')) {
            self.bump();
        }
        let raw = &self.source[start..self.pos];
        let value = collapse_ui_whitespace(raw);
        if value.trim().is_empty() {
            None
        } else {
            Some(Spanned::new(
                UiNodeKind::Text(value),
                SourceSpan::new(
                    self.file,
                    self.base + start as u32,
                    self.base + self.pos as u32,
                ),
            ))
        }
    }

    /// Consumes a `{...}` surface and returns the text inside it.
    fn braced(&mut self) -> (String, u32, u32) {
        let start = self.absolute();
        if !self.eat('{') {
            self.error(
                "expected `{`",
                SourceSpan::empty(self.file, self.absolute()),
            );
            return (String::new(), self.absolute(), self.absolute());
        }
        let content_start = self.pos;
        let mut depth = 1u32;
        let mut string = false;
        let mut escaped = false;
        while let Some(value) = self.bump() {
            if string {
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
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        let content_end = self.pos - 1;
                        return (
                            self.source[content_start..content_end].to_string(),
                            self.base + content_start as u32,
                            self.absolute(),
                        );
                    }
                }
                _ => {}
            }
        }
        self.error(
            "unterminated UI brace",
            SourceSpan::new(self.file, start, self.absolute()),
        );
        (
            self.source[content_start..].to_string(),
            self.base + content_start as u32,
            self.absolute(),
        )
    }

    fn directive_header(&mut self, prefix: &str) -> (String, u32, u32) {
        let start = self.absolute();
        if !self.eat_str(prefix) {
            self.error(
                format!("expected `{prefix}`"),
                SourceSpan::empty(self.file, start),
            );
        }
        let header_start = self.pos;
        // The opening `{` was part of `prefix`, so reconstruct a temporary
        // balanced scan with depth one.
        let mut depth = 1u32;
        let mut string = false;
        let mut escaped = false;
        while let Some(value) = self.bump() {
            if string {
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
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        let end = self.pos - 1;
                        return (
                            self.source[header_start..end].to_string(),
                            self.base + header_start as u32,
                            self.absolute(),
                        );
                    }
                }
                _ => {}
            }
        }
        self.error(
            format!("unterminated `{prefix}` directive"),
            SourceSpan::new(self.file, start, self.absolute()),
        );
        (
            self.source[header_start..].to_string(),
            self.base + header_start as u32,
            self.absolute(),
        )
    }

    fn scan_to_tag_end(&mut self) {
        let mut paren = 0i32;
        let mut bracket = 0i32;
        let mut brace = 0i32;
        let mut string = false;
        let mut escaped = false;
        while let Some(value) = self.peek() {
            if string {
                self.bump();
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
                '"' => {
                    string = true;
                    self.bump();
                }
                '(' => {
                    paren += 1;
                    self.bump();
                }
                ')' => {
                    paren -= 1;
                    self.bump();
                }
                '[' => {
                    bracket += 1;
                    self.bump();
                }
                ']' => {
                    bracket -= 1;
                    self.bump();
                }
                '{' => {
                    brace += 1;
                    self.bump();
                }
                '}' => {
                    brace -= 1;
                    self.bump();
                }
                '>' if paren == 0 && bracket == 0 && brace == 0 => break,
                '/' if paren == 0
                    && bracket == 0
                    && brace == 0
                    && self.rest().starts_with("/>") =>
                {
                    break;
                }
                _ => {
                    self.bump();
                }
            }
        }
    }

    fn quoted(&mut self) -> String {
        if !self.eat('"') {
            return String::new();
        }
        let mut value = String::new();
        let mut escaped = false;
        while let Some(next) = self.bump() {
            if escaped {
                value.push(match next {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    other => other,
                });
                escaped = false;
            } else if next == '\\' {
                escaped = true;
            } else if next == '"' {
                return value;
            } else {
                value.push(next);
            }
        }
        self.error(
            "unterminated quoted UI attribute",
            SourceSpan::empty(self.file, self.absolute()),
        );
        value
    }

    fn name(&mut self) -> String {
        let start = self.pos;
        let Some(first) = self.peek() else {
            return String::new();
        };
        if !(first == '_' || is_xid_start(first)) {
            self.error(
                "expected UI name",
                SourceSpan::empty(self.file, self.absolute()),
            );
            self.bump();
            return self.source[start..self.pos].to_string();
        }
        self.bump();
        while self
            .peek()
            .is_some_and(|value| value == '_' || value == '-' || is_xid_continue(value))
        {
            self.bump();
        }
        self.source[start..self.pos].to_string()
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    fn rest(&self) -> &str {
        &self.source[self.pos..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let value = self.peek()?;
        self.pos += value.len_utf8();
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
            self.pos += expected.len();
            true
        } else {
            false
        }
    }

    fn absolute(&self) -> u32 {
        self.base.saturating_add(self.pos as u32)
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.source.len()
    }

    fn error(&mut self, message: impl Into<String>, span: SourceSpan) {
        self.diagnostics.push(ParseDiagnostic {
            kind: ParseDiagnosticKind::InvalidUi,
            message: message.into(),
            span,
        });
    }
}

fn leading_ws(value: &str) -> u32 {
    value.len().saturating_sub(value.trim_start().len()) as u32
}

fn collapse_ui_whitespace(value: &str) -> String {
    let mut output = String::new();
    let mut whitespace = false;
    for value in value.chars() {
        if value.is_whitespace() {
            whitespace = true;
        } else {
            if whitespace {
                output.push(' ');
            }
            output.push(value);
            whitespace = false;
        }
    }
    if whitespace {
        output.push(' ');
    }
    output
}

fn matching_token(
    tokens: &[super::lexer::Token],
    open_index: usize,
    open: TokenKind,
    close: TokenKind,
) -> Option<usize> {
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate().skip(open_index) {
        if std::mem::discriminant(&token.kind) == std::mem::discriminant(&open) {
            depth += 1;
        } else if std::mem::discriminant(&token.kind) == std::mem::discriminant(&close) {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

fn slice_absolute(source: &str, base: u32, start: u32, end: u32) -> &str {
    let start = start.saturating_sub(base) as usize;
    let end = end.saturating_sub(base) as usize;
    source.get(start..end).unwrap_or_default()
}

fn error_expr(span: SourceSpan) -> Expr {
    Spanned::new(ExprKind::Error, span)
}

fn error_pattern(span: SourceSpan) -> Pattern {
    Spanned::new(PatternKind::Error, span)
}
