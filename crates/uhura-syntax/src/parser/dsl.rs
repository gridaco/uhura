//! Header and store parsing (design §4.1–§4.2). These surfaces are pure
//! DSL; the file-level driver (`mod.rs`) decides when markup begins.

use uhura_base::{Span, codes};

use crate::ast::*;
use crate::token::TokenKind as T;

use super::expr::{parse_args, parse_expr, parse_type};
use super::stream::DslStream;

/// Sync set for header/store recovery: skip until one of these idents (at
/// nesting depth 0) or EOF.
fn sync_to(s: &mut DslStream, targets: &[&str]) {
    let mut depth = 0i32;
    loop {
        match s.peek() {
            T::Eof => return,
            T::LBrace => {
                depth += 1;
                s.bump();
            }
            T::RBrace => {
                if depth == 0 {
                    return;
                }
                depth -= 1;
                s.bump();
            }
            T::Ident(name) if depth == 0 && targets.iter().any(|t| t == name) => return,
            _ => {
                s.bump();
            }
        }
    }
}

// ── header declarations ─────────────────────────────────────────────────────

pub fn parse_use(s: &mut DslStream) -> Option<Use> {
    let leading = s.take_leading();
    let start = s.peek_span();
    if !s.eat_ident("use") {
        return None;
    }
    let Some((kind, _)) = s.expect_ident("after `use` (component | surface | port | fixture)")
    else {
        sync_to(s, &["use", "props", "emits", "param", "store", "example"]);
        return None;
    };
    match kind.as_str() {
        "component" => {
            let (name, nspan) = s.expect_ident("as the component name")?;
            Some(Use::Component {
                name,
                span: start.to(nspan),
                leading,
            })
        }
        "surface" => {
            let (name, nspan) = s.expect_ident("as the surface name")?;
            Some(Use::Surface {
                name,
                span: start.to(nspan),
                leading,
            })
        }
        "fixture" => {
            let (name, nspan) = s.expect_ident("as the fixture name")?;
            Some(Use::Fixture {
                name,
                span: start.to(nspan),
                leading,
            })
        }
        "port" => {
            let (name, _) = s.expect_ident("as the port name")?;
            s.expect(&T::LBrace, "to open the port import list");
            let mut items = Vec::new();
            loop {
                match s.peek().clone() {
                    T::RBrace => {
                        break;
                    }
                    T::Eof => break,
                    T::Comma => {
                        s.bump();
                    }
                    T::Ident(k) if matches!(k.as_str(), "projection" | "command" | "type") => {
                        let kspan = s.peek_span();
                        s.bump();
                        let kind = match k.as_str() {
                            "projection" => PortItemKind::Projection,
                            "command" => PortItemKind::Command,
                            _ => PortItemKind::Type,
                        };
                        if let Some((iname, ispan)) = s.expect_ident("as the imported item name") {
                            items.push(PortItem {
                                kind,
                                name: iname,
                                span: kspan.to(ispan),
                            });
                        }
                    }
                    other => {
                        let desc = other.describe();
                        let span = s.peek_span();
                        s.cur.error(
                            codes::UNEXPECTED_TOKEN,
                            format!(
                                "expected `projection`, `command`, or `type` in the port \
                                 import list, found {desc}"
                            ),
                            span,
                        );
                        s.bump();
                    }
                }
            }
            let end = s.peek_span();
            s.expect(&T::RBrace, "to close the port import list");
            Some(Use::Port {
                name,
                items,
                span: start.to(end),
                leading,
            })
        }
        other => {
            let span = s.peek_span();
            s.cur.error(
                codes::UNEXPECTED_TOKEN,
                format!("`use {other}` is not an import kind (component | surface | port)"),
                span,
            );
            sync_to(s, &["use", "props", "emits", "param", "store"]);
            None
        }
    }
}

/// `props { name: type, … }` — brace block of typed names.
pub fn parse_props_block(s: &mut DslStream) -> Vec<PropDecl> {
    parse_typed_block(s, "props")
        .into_iter()
        .map(|(name, ty, span, leading)| PropDecl {
            name,
            ty,
            span,
            leading,
        })
        .collect()
}

fn parse_typed_block(
    s: &mut DslStream,
    what: &str,
) -> Vec<(String, TypeExpr, Span, Vec<crate::token::Comment>)> {
    let mut out = Vec::new();
    s.expect(&T::LBrace, &format!("to open the `{what}` block"));
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
                let leading = s.take_leading();
                let start = s.peek_span();
                let Some((name, _)) = s.expect_ident(&format!("as a {what} name")) else {
                    sync_to(s, &[]);
                    break;
                };
                s.expect(&T::Colon, "before the type");
                let ty = parse_type(s);
                let span = start.to(ty.span);
                out.push((name, ty, span, leading));
            }
        }
    }
    out
}

/// `emits { name(field: type, …), … }`
pub fn parse_emits_block(s: &mut DslStream) -> Vec<EmitDecl> {
    let mut out = Vec::new();
    s.expect(&T::LBrace, "to open the `emits` block");
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
                let leading = s.take_leading();
                let start = s.peek_span();
                let Some((name, mut end)) = s.expect_ident("as an emit name") else {
                    sync_to(s, &[]);
                    break;
                };
                let mut params = Vec::new();
                if *s.peek() == T::LParen {
                    s.bump();
                    if *s.peek() != T::RParen {
                        loop {
                            let Some((pname, _)) = s.expect_ident("as a payload field name") else {
                                break;
                            };
                            s.expect(&T::Colon, "before the field type");
                            let ty = parse_type(s);
                            params.push((pname, ty));
                            if !s.eat(&T::Comma) {
                                break;
                            }
                        }
                    }
                    end = s.peek_span();
                    s.expect(&T::RParen, "to close the emit payload");
                }
                out.push(EmitDecl {
                    name,
                    params,
                    span: start.to(end),
                    leading,
                });
            }
        }
    }
    out
}

/// `param user: id`
pub fn parse_param(s: &mut DslStream) -> Option<ParamDecl> {
    let leading = s.take_leading();
    let start = s.peek_span();
    if !s.eat_ident("param") {
        return None;
    }
    let (name, _) = s.expect_ident("as the route parameter name")?;
    s.expect(&T::Colon, "before the parameter type");
    let ty = parse_type(s);
    let span = start.to(ty.span);
    Some(ParamDecl {
        name,
        ty,
        span,
        leading,
    })
}

// ── store ───────────────────────────────────────────────────────────────────

pub fn parse_store(s: &mut DslStream) -> Store {
    let leading = s.take_leading();
    let start = s.peek_span();
    s.eat_ident("store");
    s.expect(&T::LBrace, "to open the store block");
    let mut state = Vec::new();
    let mut handlers = Vec::new();
    loop {
        match s.peek().clone() {
            T::RBrace => {
                s.bump();
                break;
            }
            T::Eof => {
                let span = s.peek_span();
                s.cur
                    .error(codes::UNCLOSED_BLOCK, "unclosed `store { … }`", span);
                break;
            }
            T::Ident(k) if k == "state" => {
                s.bump();
                parse_state_block(s, &mut state);
            }
            T::Ident(k) if k == "on" => {
                if let Some(h) = parse_handler(s) {
                    handlers.push(h);
                }
            }
            other => {
                let desc = other.describe();
                let span = s.peek_span();
                s.cur.error(
                    codes::UNEXPECTED_TOKEN,
                    format!("expected `state` or `on` in the store, found {desc}"),
                    span,
                );
                sync_to(s, &["state", "on"]);
            }
        }
    }
    let span = start.to(s.peek_span());
    Store {
        state,
        handlers,
        span,
        leading,
    }
}

fn parse_state_block(s: &mut DslStream, out: &mut Vec<StateField>) {
    s.expect(&T::LBrace, "to open the state block");
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
                let leading = s.take_leading();
                let start = s.peek_span();
                let Some((name, _)) = s.expect_ident("as a state field name") else {
                    sync_to(s, &["on"]);
                    break;
                };
                s.expect(&T::Colon, "before the field type");
                let ty = parse_type(s);
                s.expect(
                    &T::Eq,
                    "before the initial value (state initializers are literals)",
                );
                let (init, end) = parse_literal(s);
                out.push(StateField {
                    name,
                    ty,
                    init,
                    span: start.to(end),
                    leading,
                });
            }
        }
    }
}

fn parse_literal(s: &mut DslStream) -> (Literal, Span) {
    let span = s.peek_span();
    let lit = match s.peek().clone() {
        T::Int(i) => {
            s.bump();
            Literal::Int(i)
        }
        T::Str(v) => {
            s.bump();
            Literal::Str(v)
        }
        T::Ident(name) => match name.as_str() {
            "true" => {
                s.bump();
                Literal::Bool(true)
            }
            "false" => {
                s.bump();
                Literal::Bool(false)
            }
            "none" => {
                s.bump();
                Literal::None
            }
            _ => {
                s.cur.error(
                    codes::UNEXPECTED_TOKEN,
                    format!("state initializers are literals only (§4.3), found `{name}`"),
                    span,
                );
                s.bump();
                Literal::Error
            }
        },
        T::LBrace => {
            s.bump();
            let end = s.peek_span();
            if s.expect(
                &T::RBrace,
                "— `{}` (the empty map) is the only brace literal here",
            )
            .is_some()
            {
                return (Literal::EmptyMap, span.to(end));
            }
            Literal::Error
        }
        T::Minus => {
            // Negative integer literal.
            s.bump();
            if let T::Int(i) = s.peek().clone() {
                let end = s.peek_span();
                s.bump();
                return (Literal::Int(-i), span.to(end));
            }
            s.cur.error(
                codes::UNEXPECTED_TOKEN,
                "expected an integer after `-`",
                span,
            );
            Literal::Error
        }
        other => {
            let desc = other.describe();
            s.cur.error(
                codes::UNEXPECTED_TOKEN,
                format!("expected a literal initializer, found {desc}"),
                span,
            );
            s.bump();
            Literal::Error
        }
    };
    (lit, span)
}

fn parse_handler(s: &mut DslStream) -> Option<Handler> {
    let leading = s.take_leading();
    let start = s.peek_span();
    s.eat_ident("on");
    let (first, fspan) = s.expect_ident("as the event name")?;

    // `on <command>.ok(…)` / `.err(…)` — outcome handlers.
    let event = if *s.peek() == T::Dot {
        s.bump();
        let (which, wspan) = s.expect_ident("(`ok` or `err`) after `.`")?;
        let kind = match which.as_str() {
            "ok" => OutcomeKind::Ok,
            "err" => OutcomeKind::Err,
            other => {
                s.cur.error(
                    codes::UNEXPECTED_TOKEN,
                    format!("outcome handlers are `.ok` or `.err`, found `.{other}`"),
                    wspan,
                );
                OutcomeKind::Err
            }
        };
        EventRef::Outcome {
            command: first,
            which: kind,
            span: fspan.to(wspan),
        }
    } else {
        EventRef::Semantic {
            name: first,
            span: fspan,
        }
    };

    // Parameter list: UI events declare `name: type`; outcome handlers are
    // name-only.
    let mut params = Vec::new();
    if *s.peek() == T::LParen {
        s.bump();
        if *s.peek() != T::RParen {
            loop {
                let pstart = s.peek_span();
                let Some((pname, pspan)) = s.expect_ident("as a handler parameter") else {
                    break;
                };
                let ty = if s.eat(&T::Colon) {
                    Some(parse_type(s))
                } else {
                    None
                };
                let span = ty.as_ref().map_or(pspan, |t| pstart.to(t.span));
                params.push(HandlerParam {
                    name: pname,
                    ty,
                    span,
                });
                if !s.eat(&T::Comma) {
                    break;
                }
            }
        }
        s.expect(&T::RParen, "to close the handler parameters");
    }

    let guard = if s.eat_ident("when") {
        Some(parse_expr(s))
    } else {
        None
    };

    s.expect(&T::LBrace, "to open the handler body");
    let mut body = Vec::new();
    loop {
        match s.peek().clone() {
            T::RBrace => {
                s.bump();
                break;
            }
            T::Eof => {
                let span = s.peek_span();
                s.cur
                    .error(codes::UNCLOSED_BLOCK, "unclosed handler body", span);
                break;
            }
            _ => match parse_stmt(s) {
                Some(st) => body.push(st),
                None => {
                    sync_to(
                        s,
                        &["set", "send", "open-surface", "dismiss", "navigate", "on"],
                    );
                    if s.peek().is_ident("on") {
                        // Missing `}` — let the store loop pick the next handler.
                        let span = s.peek_span();
                        s.cur
                            .error(codes::UNCLOSED_BLOCK, "handler body not closed", span);
                        break;
                    }
                }
            },
        }
    }
    let span = start.to(s.peek_span());
    Some(Handler {
        event,
        params,
        guard,
        body,
        span,
        leading,
    })
}

/// The five statements (design §4.2).
fn parse_stmt(s: &mut DslStream) -> Option<Stmt> {
    let leading = s.take_leading();
    let start = s.peek_span();
    let T::Ident(kw) = s.peek().clone() else {
        let desc = s.peek().describe();
        s.cur.error(
            codes::UNEXPECTED_TOKEN,
            format!("expected a statement (set | send | open-surface | dismiss | navigate), found {desc}"),
            start,
        );
        return None;
    };
    match kw.as_str() {
        "set" => {
            s.bump();
            let (field, fspan) = s.expect_ident("as the state field")?;
            let key = if s.eat(&T::LBracket) {
                let k = parse_expr(s);
                s.expect(&T::RBracket, "to close the map key");
                Some(k)
            } else {
                None
            };
            let path_span = fspan;
            s.expect(&T::Eq, "in `set <path> = <expr>`");
            let value = parse_expr(s);
            let span = start.to(value.span);
            Some(Stmt::Set {
                path: SetPath {
                    field,
                    key,
                    span: path_span,
                },
                value,
                span,
                leading,
            })
        }
        "send" => {
            s.bump();
            let (command, _) = s.expect_ident("as the command name")?;
            let args = parse_args(s);
            let bind = if s.eat_ident("as") {
                s.expect_ident("as the tag binding name").map(|(n, _)| n)
            } else {
                None
            };
            let span = start.to(s.peek_span());
            Some(Stmt::Send {
                command,
                args,
                bind,
                span,
                leading,
            })
        }
        "open-surface" => {
            s.bump();
            let (name, _) = s.expect_ident("as the surface name")?;
            let args = parse_args(s);
            let span = start.to(s.peek_span());
            Some(Stmt::OpenSurface {
                name,
                args,
                span,
                leading,
            })
        }
        "dismiss" => {
            s.bump();
            Some(Stmt::Dismiss {
                span: start,
                leading,
            })
        }
        "navigate" => {
            s.bump();
            let (mut target_name, tspan) =
                s.expect_ident("as a route name, `replace`, or `back`")?;
            if target_name == "back" {
                return Some(Stmt::Navigate {
                    target: NavTarget::Back,
                    span: start.to(tspan),
                    leading,
                });
            }
            let replace = target_name == "replace";
            if replace {
                (target_name, _) = s.expect_ident("as the route name after `replace`")?;
            }
            let args = if *s.peek() == T::LParen {
                parse_args(s)
            } else {
                Vec::new()
            };
            let span = start.to(s.peek_span());
            Some(Stmt::Navigate {
                target: if replace {
                    NavTarget::Replace {
                        name: target_name,
                        args,
                    }
                } else {
                    NavTarget::Route {
                        name: target_name,
                        args,
                    }
                },
                span,
                leading,
            })
        }
        other => {
            s.cur.error(
                codes::UNEXPECTED_TOKEN,
                format!(
                    "`{other}` is not a statement — the closed set is \
                     set | send | open-surface | dismiss | navigate (§4.2)"
                ),
                start,
            );
            None
        }
    }
}
