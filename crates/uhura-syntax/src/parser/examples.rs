//! `.examples.uhura` files (design §6.1): `use fixture …` imports plus
//! `example <name> [default] { clauses }` declarations. Pure DSL surface.

use uhura_base::codes;

use crate::ast::*;
use crate::token::TokenKind as T;

use super::expr::{parse_args, parse_expr};
use super::stream::DslStream;

pub fn parse_examples(s: &mut DslStream) -> ExamplesFile {
    let mut preamble = DslTrivia::default();
    let mut uses = Vec::new();
    let mut examples = Vec::new();
    let trailing;
    let mut first_item = true;
    loop {
        match s.peek().clone() {
            T::Eof => {
                trailing = s.take_leading();
                let eof = s.peek_span();
                if first_item {
                    s.accept_file_docs_at_eof(&trailing, eof);
                    preamble = trailing.clone();
                } else {
                    s.reject_boundary_docs(&trailing, eof);
                }
                break;
            }
            T::Ident(k) if k == "use" => {
                if let Some(u) = super::dsl::parse_use(s, first_item) {
                    if first_item {
                        preamble = use_leading(&u).clone();
                    }
                    uses.push(u);
                }
                first_item = false;
            }
            T::Ident(k) if k == "example" => {
                if let Some(e) = parse_example(s, first_item) {
                    if first_item {
                        preamble = e.leading.clone();
                    }
                    examples.push(e);
                }
                first_item = false;
            }
            other => {
                let desc = other.describe();
                let span = s.peek_span();
                s.cur.error(
                    codes::UNEXPECTED_TOKEN,
                    format!("expected `use fixture …` or `example …`, found {desc}"),
                    span,
                );
                s.bump();
            }
        }
    }
    ExamplesFile {
        preamble,
        uses,
        examples,
        trailing,
    }
}

fn use_leading(use_decl: &Use) -> &DslTrivia {
    match use_decl {
        Use::Component { leading, .. }
        | Use::Surface { leading, .. }
        | Use::Port { leading, .. }
        | Use::Fixture { leading, .. } => leading,
    }
}

fn parse_example(s: &mut DslStream, file_preamble: bool) -> Option<ExampleDecl> {
    let leading = s.take_leading();
    let start = s.peek_span();
    s.eat_ident("example");
    let (name, _) = s.expect_ident("as the example name")?;
    let is_default = s.eat_ident("default");
    s.expect(&T::LBrace, "to open the example body");

    let mut clauses = Vec::new();
    let mut clause_leading = Vec::new();
    let mut trailing = DslTrivia::default();
    let mut end = start;
    loop {
        match s.peek().clone() {
            T::RBrace => {
                trailing = s.take_leading();
                let boundary = s.peek_span();
                s.reject_boundary_docs(&trailing, boundary);
                end = s.bump().span;
                break;
            }
            T::Eof => {
                let span = s.peek_span();
                s.cur
                    .error(codes::UNCLOSED_BLOCK, "unclosed example body", span);
                break;
            }
            T::Ident(k) => {
                let clause_trivia = s.take_leading();
                let cstart = s.peek_span();
                match k.as_str() {
                    "from" => {
                        s.bump();
                        if let Some((from, fspan)) = s.expect_ident("as the parent example") {
                            push_clause(
                                s,
                                &mut clauses,
                                &mut clause_leading,
                                clause_trivia,
                                ExampleClause::From {
                                    name: from,
                                    span: cstart.to(fspan),
                                },
                            );
                        }
                    }
                    "note" => {
                        s.bump();
                        if let T::Str(text) = s.peek().clone() {
                            let tspan = s.peek_span();
                            s.bump();
                            push_clause(
                                s,
                                &mut clauses,
                                &mut clause_leading,
                                clause_trivia,
                                ExampleClause::Note {
                                    text,
                                    span: cstart.to(tspan),
                                },
                            );
                        } else {
                            let span = s.peek_span();
                            s.cur.error(
                                codes::UNEXPECTED_TOKEN,
                                "`note` takes a string literal",
                                span,
                            );
                        }
                    }
                    "params" | "props" | "state" => {
                        s.bump();
                        let entries = parse_assign_block(s, &k);
                        let span = cstart.to(s.peek_span());
                        let clause = match k.as_str() {
                            "params" => ExampleClause::Params { entries, span },
                            "props" => ExampleClause::Props { entries, span },
                            _ => ExampleClause::State { entries, span },
                        };
                        push_clause(s, &mut clauses, &mut clause_leading, clause_trivia, clause);
                    }
                    "projection" => {
                        s.bump();
                        if let Some(pin) = parse_projection_pin(s) {
                            push_clause(
                                s,
                                &mut clauses,
                                &mut clause_leading,
                                clause_trivia,
                                ExampleClause::Projection(pin),
                            );
                        }
                    }
                    "events" => {
                        s.bump();
                        let entries = parse_events_list(s);
                        let span = cstart.to(s.peek_span());
                        push_clause(
                            s,
                            &mut clauses,
                            &mut clause_leading,
                            clause_trivia,
                            ExampleClause::Events { entries, span },
                        );
                    }
                    other => {
                        s.cur.error(
                            codes::UNEXPECTED_TOKEN,
                            format!(
                                "unknown example clause `{other}` — clauses are from | note | \
                                 params | props | state | projection | events"
                            ),
                            cstart,
                        );
                        s.reject_docs(&clause_trivia, cstart);
                        s.bump();
                    }
                }
            }
            other => {
                let desc = other.describe();
                let span = s.peek_span();
                s.cur.error(
                    codes::UNEXPECTED_TOKEN,
                    format!("expected an example clause, found {desc}"),
                    span,
                );
                s.bump();
            }
        }
    }
    let span = start.to(end);
    if file_preamble {
        s.accept_preamble_docs(&leading, span);
    } else {
        s.accept_outer_docs(&leading, span);
    }
    Some(ExampleDecl {
        name,
        is_default,
        clauses,
        clause_leading,
        trailing,
        span,
        leading,
    })
}

fn push_clause(
    s: &mut DslStream,
    clauses: &mut Vec<ExampleClause>,
    leading: &mut Vec<DslTrivia>,
    trivia: DslTrivia,
    clause: ExampleClause,
) {
    let span = clause_span(&clause);
    s.reject_docs(&trivia, span);
    leading.push(trivia);
    clauses.push(clause);
}

fn clause_span(clause: &ExampleClause) -> uhura_base::Span {
    match clause {
        ExampleClause::From { span, .. }
        | ExampleClause::Note { span, .. }
        | ExampleClause::Params { span, .. }
        | ExampleClause::Props { span, .. }
        | ExampleClause::State { span, .. }
        | ExampleClause::Events { span, .. }
        | ExampleClause::Error { span } => *span,
        ExampleClause::Projection(pin) => pin.span,
    }
}

/// `{ name = expr, … }` for params / props / state clauses.
fn parse_assign_block(s: &mut DslStream, what: &str) -> Vec<(String, Expr)> {
    let mut out = Vec::new();
    if s.expect(&T::LBrace, &format!("to open the `{what}` clause"))
        .is_none()
    {
        return out;
    }
    loop {
        match s.peek() {
            T::RBrace => {
                s.bump();
                break;
            }
            T::Eof => break,
            T::Comma => {
                s.bump();
            }
            _ => {
                let Some((name, _)) = s.expect_ident(&format!("as a {what} entry name")) else {
                    break;
                };
                s.expect(&T::Eq, "before the value");
                out.push((name, parse_expr(s)));
            }
        }
    }
    out
}

/// `feed.feed-page = expr` or `comments.for-post("post-1") = expr`
/// (the leading `projection` keyword is already consumed).
fn parse_projection_pin(s: &mut DslStream) -> Option<ProjectionPin> {
    let start = s.peek_span();
    let (port, _) = s.expect_ident("as the port name")?;
    s.expect(&T::Dot, "between port and projection");
    let (projection, _) = s.expect_ident("as the projection name")?;
    let key = if *s.peek() == T::LParen {
        s.bump();
        let k = parse_expr(s);
        s.expect(&T::RParen, "to close the projection key");
        Some(k)
    } else {
        None
    };
    s.expect(&T::Eq, "before the pinned value");
    let value = parse_expr(s);
    let span = start.to(value.span);
    Some(ProjectionPin {
        port,
        projection,
        key,
        value,
        span,
    })
}

/// `[ entry … ]` — the derivation timeline (design §6.2).
fn parse_events_list(s: &mut DslStream) -> Vec<ExampleEvent> {
    let mut out = Vec::new();
    if s.expect(&T::LBracket, "to open the events timeline")
        .is_none()
    {
        return out;
    }
    loop {
        match s.peek().clone() {
            T::RBracket => {
                s.bump();
                break;
            }
            T::Eof => {
                let span = s.peek_span();
                s.cur
                    .error(codes::UNCLOSED_BLOCK, "unclosed events timeline", span);
                break;
            }
            T::Comma => {
                s.bump();
            }
            T::Ident(k) if k == "outcome" => {
                let start = s.peek_span();
                s.bump();
                let Some((command, _)) = s.expect_ident("as the command name") else {
                    continue;
                };
                s.expect(&T::Dot, "before `ok` or `err`");
                let which = match s.expect_ident("(`ok` or `err`)") {
                    Some((w, wspan)) => match w.as_str() {
                        "ok" => OutcomeKind::Ok,
                        "err" => OutcomeKind::Err,
                        other => {
                            s.cur.error(
                                codes::UNEXPECTED_TOKEN,
                                format!("expected `ok` or `err`, found `{other}`"),
                                wspan,
                            );
                            OutcomeKind::Err
                        }
                    },
                    None => OutcomeKind::Err,
                };
                let args = if *s.peek() == T::LParen {
                    parse_args(s)
                } else {
                    Vec::new()
                };
                let span = start.to(s.peek_span());
                out.push(ExampleEvent::Outcome {
                    command,
                    which,
                    args,
                    span,
                });
            }
            T::Ident(k) if k == "projection" => {
                s.bump();
                if let Some(pin) = parse_projection_pin(s) {
                    out.push(ExampleEvent::Projection(pin));
                }
            }
            T::Ident(_) => {
                let start = s.peek_span();
                let (name, _) = s.expect_ident("as the event name").unwrap();
                let args = if *s.peek() == T::LParen {
                    parse_args(s)
                } else {
                    Vec::new()
                };
                let span = start.to(s.peek_span());
                out.push(ExampleEvent::Semantic { name, args, span });
            }
            other => {
                let desc = other.describe();
                let span = s.peek_span();
                s.cur.error(
                    codes::UNEXPECTED_TOKEN,
                    format!("expected a timeline entry, found {desc}"),
                    span,
                );
                s.bump();
            }
        }
    }
    out
}
