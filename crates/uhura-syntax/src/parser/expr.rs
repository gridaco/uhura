//! The total, tiny, closed expression language (design §4.3) and type
//! expressions. Precedence, loosest → tightest (micro-decision #4):
//!
//! `if-then-else`  <  `||`  <  `&&`  <  comparison (non-assoc)  <  `??`
//!   <  `+ - ++`  <  unary `! -`  <  postfix `.field` `[k]` `(call)`

use uhura_base::codes;

use crate::ast::{Arg, BinaryOp, Expr, ExprKind, TypeExpr, TypeKind, UnaryOp};
use crate::token::TokenKind as T;

use super::stream::DslStream;

pub fn parse_expr(s: &mut DslStream) -> Expr {
    parse_if_expr(s)
}

fn parse_if_expr(s: &mut DslStream) -> Expr {
    let start = s.peek_span();
    if s.peek().is_ident("if") {
        s.bump();
        let cond = parse_or(s);
        s.expect(&T::Ident("then".into()), "in `if … then … else …`");
        let then = parse_if_expr(s);
        let els = if s.eat_ident("else") {
            parse_if_expr(s)
        } else {
            let span = s.peek_span();
            s.cur.error(
                codes::UNEXPECTED_TOKEN,
                "`if` expressions require an `else` branch (§4.3: both branches, same type)",
                span,
            );
            Expr {
                kind: ExprKind::Error,
                span,
            }
        };
        let span = start.to(els.span);
        return Expr {
            kind: ExprKind::If {
                cond: Box::new(cond),
                then: Box::new(then),
                els: Box::new(els),
            },
            span,
        };
    }
    parse_or(s)
}

fn parse_or(s: &mut DslStream) -> Expr {
    let mut lhs = parse_and(s);
    while *s.peek() == T::OrOr {
        s.bump();
        let rhs = parse_and(s);
        let span = lhs.span.to(rhs.span);
        lhs = Expr {
            kind: ExprKind::Binary {
                op: BinaryOp::Or,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
            span,
        };
    }
    lhs
}

fn parse_and(s: &mut DslStream) -> Expr {
    let mut lhs = parse_cmp(s);
    while *s.peek() == T::AndAnd {
        s.bump();
        let rhs = parse_cmp(s);
        let span = lhs.span.to(rhs.span);
        lhs = Expr {
            kind: ExprKind::Binary {
                op: BinaryOp::And,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
            span,
        };
    }
    lhs
}

fn cmp_op(t: &T) -> Option<BinaryOp> {
    match t {
        T::EqEq => Some(BinaryOp::Eq),
        T::NotEq => Some(BinaryOp::NotEq),
        T::Lt => Some(BinaryOp::Lt),
        T::Le => Some(BinaryOp::Le),
        T::Gt => Some(BinaryOp::Gt),
        T::Ge => Some(BinaryOp::Ge),
        _ => None,
    }
}

fn parse_cmp(s: &mut DslStream) -> Expr {
    let lhs = parse_coalesce(s);
    let Some(op) = cmp_op(s.peek()) else {
        return lhs;
    };
    s.bump();
    let rhs = parse_coalesce(s);
    let span = lhs.span.to(rhs.span);
    let out = Expr {
        kind: ExprKind::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
        span,
    };
    // Comparison is non-associative: a second comparison operator here is
    // a hard parse error (design §4.3).
    if cmp_op(s.peek()).is_some() {
        let span = s.peek_span();
        s.cur.error(
            codes::UNEXPECTED_TOKEN,
            "comparison operators do not chain — parenthesize explicitly",
            span,
        );
        s.bump();
        let _ = parse_coalesce(s);
    }
    out
}

fn parse_coalesce(s: &mut DslStream) -> Expr {
    let mut lhs = parse_additive(s);
    while *s.peek() == T::Coalesce {
        s.bump();
        let rhs = parse_additive(s);
        let span = lhs.span.to(rhs.span);
        lhs = Expr {
            kind: ExprKind::Binary {
                op: BinaryOp::Coalesce,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
            span,
        };
    }
    lhs
}

fn parse_additive(s: &mut DslStream) -> Expr {
    let mut lhs = parse_unary(s);
    loop {
        let op = match s.peek() {
            T::Plus => BinaryOp::Add,
            T::Minus => BinaryOp::Sub,
            T::PlusPlus => BinaryOp::Concat,
            _ => break,
        };
        s.bump();
        let rhs = parse_unary(s);
        let span = lhs.span.to(rhs.span);
        lhs = Expr {
            kind: ExprKind::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
            span,
        };
    }
    lhs
}

fn parse_unary(s: &mut DslStream) -> Expr {
    let start = s.peek_span();
    let op = match s.peek() {
        T::Bang => Some(UnaryOp::Not),
        T::Minus => Some(UnaryOp::Neg),
        _ => None,
    };
    if let Some(op) = op {
        s.bump();
        let expr = parse_unary(s);
        let span = start.to(expr.span);
        return Expr {
            kind: ExprKind::Unary {
                op,
                expr: Box::new(expr),
            },
            span,
        };
    }
    parse_postfix(s)
}

fn parse_postfix(s: &mut DslStream) -> Expr {
    let mut expr = parse_primary(s);
    loop {
        match s.peek() {
            T::Dot => {
                s.bump();
                if let Some((name, nspan)) = s.expect_ident("after `.`") {
                    let span = expr.span.to(nspan);
                    expr = Expr {
                        kind: ExprKind::Field {
                            base: Box::new(expr),
                            name,
                        },
                        span,
                    };
                } else {
                    break;
                }
            }
            T::LBracket => {
                s.bump();
                let key = parse_expr(s);
                let end = s.peek_span();
                s.expect(&T::RBracket, "to close the index");
                let span = expr.span.to(end);
                expr = Expr {
                    kind: ExprKind::Index {
                        base: Box::new(expr),
                        key: Box::new(key),
                    },
                    span,
                };
            }
            _ => break,
        }
    }
    expr
}

fn parse_primary(s: &mut DslStream) -> Expr {
    let t = s.peek_token();
    let span = t.span;
    match s.peek().clone() {
        T::Int(i) => {
            s.bump();
            Expr {
                kind: ExprKind::Int(i),
                span,
            }
        }
        T::Str(v) => {
            s.bump();
            Expr {
                kind: ExprKind::Str(v),
                span,
            }
        }
        T::Ident(name) => {
            match name.as_str() {
                "true" => {
                    s.bump();
                    return Expr {
                        kind: ExprKind::Bool(true),
                        span,
                    };
                }
                "false" => {
                    s.bump();
                    return Expr {
                        kind: ExprKind::Bool(false),
                        span,
                    };
                }
                "none" => {
                    s.bump();
                    return Expr {
                        kind: ExprKind::None,
                        span,
                    };
                }
                _ => {}
            }
            s.bump();
            // Call form: `name(expr, …)` — builtins and keyed projections.
            if *s.peek() == T::LParen {
                s.bump();
                let mut args = Vec::new();
                if *s.peek() != T::RParen {
                    loop {
                        args.push(parse_expr(s));
                        if !s.eat(&T::Comma) {
                            break;
                        }
                    }
                }
                let end = s.peek_span();
                s.expect(&T::RParen, "to close the call");
                return Expr {
                    kind: ExprKind::Call { name, args },
                    span: span.to(end),
                };
            }
            Expr {
                kind: ExprKind::Ident(name),
                span,
            }
        }
        T::LParen => {
            s.bump();
            let inner = parse_expr(s);
            s.expect(&T::RParen, "to close the group");
            inner
        }
        T::LBrace => {
            // Record literal `{ field: expr, … }` (set-rhs and example pins).
            s.bump();
            let mut fields = Vec::new();
            if *s.peek() != T::RBrace {
                loop {
                    let Some((name, _)) = s.expect_ident("as a record field name") else {
                        break;
                    };
                    s.expect(&T::Colon, "after the field name");
                    fields.push((name, parse_expr(s)));
                    if !s.eat(&T::Comma) {
                        break;
                    }
                }
            }
            let end = s.peek_span();
            s.expect(&T::RBrace, "to close the record literal");
            Expr {
                kind: ExprKind::Record(fields),
                span: span.to(end),
            }
        }
        other => {
            let desc = other.describe();
            s.cur.error(
                codes::UNEXPECTED_TOKEN,
                format!("expected an expression, found {desc}"),
                span,
            );
            s.bump();
            Expr {
                kind: ExprKind::Error,
                span,
            }
        }
    }
}

/// Named argument list: `(name: expr, …)` — the opening paren is expected
/// by the caller's context description.
pub fn parse_args(s: &mut DslStream) -> Vec<Arg> {
    let mut args = Vec::new();
    if s.expect(&T::LParen, "to open the argument list").is_none() {
        return args;
    }
    if *s.peek() != T::RParen {
        loop {
            let start = s.peek_span();
            let Some((name, _)) = s.expect_ident("as an argument name") else {
                break;
            };
            s.expect(
                &T::Colon,
                "after the argument name (all arguments are named)",
            );
            let value = parse_expr(s);
            let span = start.to(value.span);
            args.push(Arg { name, value, span });
            if !s.eat(&T::Comma) {
                break;
            }
        }
    }
    s.expect(&T::RParen, "to close the argument list");
    args
}

/// Type expressions: `name`, `list[T]`, `map[K]V`, suffix `?`.
pub fn parse_type(s: &mut DslStream) -> TypeExpr {
    let start = s.peek_span();
    let base = match s.peek().clone() {
        T::Ident(name) => {
            s.bump();
            match name.as_str() {
                "list" if *s.peek() == T::LBracket => {
                    s.bump();
                    let inner = parse_type(s);
                    let end = s.peek_span();
                    s.expect(&T::RBracket, "to close `list[…]`");
                    TypeExpr {
                        kind: TypeKind::List(Box::new(inner)),
                        span: start.to(end),
                    }
                }
                "map" if *s.peek() == T::LBracket => {
                    s.bump();
                    let key = match s.expect_ident("as the map key type") {
                        Some((k, _)) => k,
                        None => "id".to_string(),
                    };
                    s.expect(&T::RBracket, "to close the map key");
                    let value = parse_type(s);
                    let span = start.to(value.span);
                    TypeExpr {
                        kind: TypeKind::Map(key, Box::new(value)),
                        span,
                    }
                }
                _ => TypeExpr {
                    kind: TypeKind::Name(name),
                    span: start,
                },
            }
        }
        other => {
            let desc = other.describe();
            s.cur.error(
                codes::UNEXPECTED_TOKEN,
                format!("expected a type, found {desc}"),
                start,
            );
            TypeExpr {
                kind: TypeKind::Error,
                span: start,
            }
        }
    };
    if *s.peek() == T::Question {
        let end = s.peek_span();
        s.bump();
        let span = base.span.to(end);
        return TypeExpr {
            kind: TypeKind::Option(Box::new(base)),
            span,
        };
    }
    base
}
