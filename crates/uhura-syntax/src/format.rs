//! The one canonical formatter — zero options (design §4). Layout is fully
//! deterministic and width-independent: attributes, guards, and expressions
//! render on one line; children/bodies indent by two spaces. Comments attach
//! before their item. CSS declarations pass through verbatim (§4.5).

use crate::ast::*;
use crate::token::Comment;

const INDENT: &str = "  ";

pub fn format_module(f: &File) -> String {
    let mut out = String::new();

    // ── header ──────────────────────────────────────────────────────────
    match &f.kind {
        DefKind::Component { name, .. } => out.push_str(&format!("component {name}\n")),
        DefKind::Page { .. } => out.push_str("page\n"),
        DefKind::Surface { name, modality, .. } => match modality {
            Some(m) => out.push_str(&format!("surface {name} modality {m}\n")),
            None => out.push_str(&format!("surface {name}\n")),
        },
        DefKind::Error { .. } => {}
    }

    if !f.uses.is_empty() {
        out.push('\n');
        for u in &f.uses {
            fmt_use(u, &mut out);
        }
    }

    if !f.props.is_empty() {
        out.push('\n');
        out.push_str("props {\n");
        for p in &f.props {
            fmt_comments(&p.leading, 1, &mut out);
            out.push_str(&format!("{INDENT}{}: {}\n", p.name, type_str(&p.ty)));
        }
        out.push_str("}\n");
    }

    if !f.emits.is_empty() {
        out.push('\n');
        out.push_str("emits {\n");
        for e in &f.emits {
            fmt_comments(&e.leading, 1, &mut out);
            let params = e
                .params
                .iter()
                .map(|(n, t)| format!("{n}: {}", type_str(t)))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!("{INDENT}{}({params})\n", e.name));
        }
        out.push_str("}\n");
    }

    for p in &f.params {
        out.push('\n');
        fmt_comments(&p.leading, 0, &mut out);
        out.push_str(&format!("param {}: {}\n", p.name, type_str(&p.ty)));
    }

    if let Some(store) = &f.store {
        out.push('\n');
        fmt_comments(&store.leading, 0, &mut out);
        out.push_str("store {\n");
        if !store.state.is_empty() {
            out.push_str(&format!("{INDENT}state {{\n"));
            for sf in &store.state {
                fmt_comments(&sf.leading, 2, &mut out);
                out.push_str(&format!(
                    "{INDENT}{INDENT}{}: {} = {}\n",
                    sf.name,
                    type_str(&sf.ty),
                    literal_str(&sf.init)
                ));
            }
            out.push_str(&format!("{INDENT}}}\n"));
        }
        for h in &store.handlers {
            out.push('\n');
            fmt_handler(h, &mut out);
        }
        out.push_str("}\n");
    }

    if !f.markup.is_empty() {
        out.push('\n');
        for n in &f.markup {
            fmt_node(n, 0, &mut out);
        }
    }

    if let Some(style) = &f.style {
        out.push('\n');
        out.push_str("<style>\n");
        let trimmed = style.raw.trim_matches('\n');
        if !trimmed.is_empty() {
            out.push_str(trimmed);
            out.push('\n');
        }
        out.push_str("</style>\n");
    }

    out
}

pub fn format_examples(f: &ExamplesFile) -> String {
    let mut out = String::new();
    for u in &f.uses {
        fmt_use(u, &mut out);
    }
    for e in &f.examples {
        out.push('\n');
        fmt_comments(&e.leading, 0, &mut out);
        let default = if e.is_default { " default" } else { "" };
        out.push_str(&format!("example {}{default} {{\n", e.name));
        for c in &e.clauses {
            fmt_example_clause(c, &mut out);
        }
        out.push_str("}\n");
    }
    out
}

// ── pieces ──────────────────────────────────────────────────────────────────

fn fmt_comments(comments: &[Comment], depth: usize, out: &mut String) {
    for c in comments {
        out.push_str(&INDENT.repeat(depth));
        out.push_str(&format!("//{}\n", c.text));
    }
}

fn fmt_use(u: &Use, out: &mut String) {
    match u {
        Use::Component { name, leading, .. } => {
            fmt_comments(leading, 0, out);
            out.push_str(&format!("use component {name}\n"));
        }
        Use::Surface { name, leading, .. } => {
            fmt_comments(leading, 0, out);
            out.push_str(&format!("use surface {name}\n"));
        }
        Use::Fixture { name, leading, .. } => {
            fmt_comments(leading, 0, out);
            out.push_str(&format!("use fixture {name}\n"));
        }
        Use::Port {
            name,
            items,
            leading,
            ..
        } => {
            fmt_comments(leading, 0, out);
            // ≤ 3 items inline; otherwise one per line (deterministic by
            // count, not width).
            let rendered: Vec<String> = items
                .iter()
                .map(|i| {
                    let kind = match i.kind {
                        PortItemKind::Projection => "projection",
                        PortItemKind::Command => "command",
                        PortItemKind::Type => "type",
                    };
                    format!("{kind} {}", i.name)
                })
                .collect();
            if rendered.len() <= 3 {
                out.push_str(&format!("use port {name} {{ {} }}\n", rendered.join(", ")));
            } else {
                out.push_str(&format!("use port {name} {{\n"));
                for r in rendered {
                    out.push_str(&format!("{INDENT}{r}\n"));
                }
                out.push_str("}\n");
            }
        }
    }
}

fn fmt_handler(h: &Handler, out: &mut String) {
    fmt_comments(&h.leading, 1, out);
    let event = match &h.event {
        EventRef::Semantic { name, .. } => name.clone(),
        EventRef::Outcome { command, which, .. } => format!(
            "{command}.{}",
            if *which == OutcomeKind::Ok {
                "ok"
            } else {
                "err"
            }
        ),
    };
    let params = h
        .params
        .iter()
        .map(|p| match &p.ty {
            Some(t) => format!("{}: {}", p.name, type_str(t)),
            None => p.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ");
    let guard = match &h.guard {
        Some(g) => format!(" when {}", expr_str(g)),
        None => String::new(),
    };
    out.push_str(&format!("{INDENT}on {event}({params}){guard} {{\n"));
    for st in &h.body {
        fmt_stmt(st, out);
    }
    out.push_str(&format!("{INDENT}}}\n"));
}

fn fmt_stmt(st: &Stmt, out: &mut String) {
    let pad = INDENT.repeat(2);
    match st {
        Stmt::Set {
            path,
            value,
            leading,
            ..
        } => {
            fmt_comments(leading, 2, out);
            let key = match &path.key {
                Some(k) => format!("[{}]", expr_str(k)),
                None => String::new(),
            };
            out.push_str(&format!(
                "{pad}set {}{key} = {}\n",
                path.field,
                expr_str(value)
            ));
        }
        Stmt::Send {
            command,
            args,
            bind,
            leading,
            ..
        } => {
            fmt_comments(leading, 2, out);
            let bind = match bind {
                Some(b) => format!(" as {b}"),
                None => String::new(),
            };
            out.push_str(&format!("{pad}send {command}({}){bind}\n", args_str(args)));
        }
        Stmt::OpenSurface {
            name,
            args,
            leading,
            ..
        } => {
            fmt_comments(leading, 2, out);
            out.push_str(&format!("{pad}open-surface {name}({})\n", args_str(args)));
        }
        Stmt::Dismiss { leading, .. } => {
            fmt_comments(leading, 2, out);
            out.push_str(&format!("{pad}dismiss\n"));
        }
        Stmt::Navigate {
            target, leading, ..
        } => {
            fmt_comments(leading, 2, out);
            match target {
                NavTarget::Back => out.push_str(&format!("{pad}navigate back\n")),
                NavTarget::Route { name, args } => {
                    if args.is_empty() {
                        out.push_str(&format!("{pad}navigate {name}()\n"));
                    } else {
                        out.push_str(&format!("{pad}navigate {name}({})\n", args_str(args)));
                    }
                }
                NavTarget::Replace { name, args } => {
                    if args.is_empty() {
                        out.push_str(&format!("{pad}navigate replace {name}()\n"));
                    } else {
                        out.push_str(&format!(
                            "{pad}navigate replace {name}({})\n",
                            args_str(args)
                        ));
                    }
                }
            }
        }
        Stmt::Error { .. } => {}
    }
}

fn fmt_node(n: &Node, depth: usize, out: &mut String) {
    let pad = INDENT.repeat(depth);
    match n {
        Node::Element(e) => {
            let mut head = format!("<{}", e.name);
            for a in &e.attrs {
                match &a.value {
                    AttrValue::Bare => head.push_str(&format!(" {}", a.name)),
                    AttrValue::Literal(v) => head.push_str(&format!(" {}=\"{v}\"", a.name)),
                    AttrValue::Expr(x) => {
                        head.push_str(&format!(" {}={{{}}}", a.name, expr_str(x)))
                    }
                }
            }
            for ev in &e.events {
                match &ev.binding {
                    EventBinding::Forward => head.push_str(&format!(" on:{}", ev.event)),
                    EventBinding::Emit { name, args } => head.push_str(&format!(
                        " on:{}={{emit {name}({})}}",
                        ev.event,
                        args_str(args)
                    )),
                }
            }
            if e.self_closing || e.children.is_empty() {
                out.push_str(&format!("{pad}{head} />\n"));
            } else if is_inline_text_only(e) {
                // `<text …>{expr} literal</text>` stays on one line.
                let mut line = format!("{pad}{head}>");
                if let Node::Text { runs, .. } = &e.children[0] {
                    line.push_str(&text_runs_str(runs));
                }
                line.push_str(&format!("</{}>\n", e.name));
                out.push_str(&line);
            } else {
                out.push_str(&format!("{pad}{head}>\n"));
                for c in &e.children {
                    fmt_node(c, depth + 1, out);
                }
                out.push_str(&format!("{pad}</{}>\n", e.name));
            }
        }
        Node::Text { runs, .. } => {
            out.push_str(&format!("{pad}{}\n", text_runs_str(runs)));
        }
        Node::If {
            cond, then, els, ..
        } => {
            out.push_str(&format!("{pad}{{#if {}}}\n", expr_str(cond)));
            for c in then {
                fmt_node(c, depth + 1, out);
            }
            if let Some(els) = els {
                out.push_str(&format!("{pad}{{:else}}\n"));
                for c in els {
                    fmt_node(c, depth + 1, out);
                }
            }
            out.push_str(&format!("{pad}{{/if}}\n"));
        }
        Node::Each {
            item,
            seq,
            key,
            body,
            ..
        } => {
            out.push_str(&format!(
                "{pad}{{#each {} as {item} ({})}}\n",
                expr_str(seq),
                expr_str(key)
            ));
            for c in body {
                fmt_node(c, depth + 1, out);
            }
            out.push_str(&format!("{pad}{{/each}}\n"));
        }
        Node::Match {
            scrutinee, arms, ..
        } => {
            out.push_str(&format!("{pad}{{#match {}}}\n", expr_str(scrutinee)));
            for a in arms {
                match &a.pattern {
                    MatchPattern::Variant(v) => match &a.binding {
                        Some(b) => out.push_str(&format!("{pad}{INDENT}{{:when {v} {b}}}\n")),
                        None => out.push_str(&format!("{pad}{INDENT}{{:when {v}}}\n")),
                    },
                    MatchPattern::Else => out.push_str(&format!("{pad}{INDENT}{{:else}}\n")),
                }
                for c in &a.body {
                    fmt_node(c, depth + 2, out);
                }
            }
            out.push_str(&format!("{pad}{{/match}}\n"));
        }
        Node::Error { .. } => {}
    }
}

fn is_inline_text_only(e: &Element) -> bool {
    e.children.len() == 1 && matches!(&e.children[0], Node::Text { .. })
}

fn text_runs_str(runs: &[TextRun]) -> String {
    let mut out = String::new();
    for r in runs {
        match r {
            TextRun::Literal(t) => out.push_str(&normalize_text(t)),
            TextRun::Interp(x) => out.push_str(&format!("{{{}}}", expr_str(x))),
        }
    }
    out
}

/// Collapses internal whitespace runs; preserves single spaces.
fn normalize_text(t: &str) -> String {
    let has_lead = t.starts_with(char::is_whitespace);
    let has_trail = t.ends_with(char::is_whitespace);
    let core = t.split_whitespace().collect::<Vec<_>>().join(" ");
    format!(
        "{}{core}{}",
        if has_lead && !core.is_empty() {
            " "
        } else {
            ""
        },
        if has_trail && !core.is_empty() {
            " "
        } else {
            ""
        }
    )
}

fn fmt_example_clause(c: &ExampleClause, out: &mut String) {
    match c {
        ExampleClause::From { name, .. } => out.push_str(&format!("{INDENT}from {name}\n")),
        ExampleClause::Note { text, .. } => {
            out.push_str(&format!("{INDENT}note {}\n", quote(text)));
        }
        ExampleClause::Params { entries, .. } => fmt_assign_block("params", entries, out),
        ExampleClause::Props { entries, .. } => fmt_assign_block("props", entries, out),
        ExampleClause::State { entries, .. } => fmt_assign_block("state", entries, out),
        ExampleClause::Projection(p) => {
            out.push_str(&format!("{INDENT}{}\n", projection_pin_str(p)));
        }
        ExampleClause::Events { entries, .. } => {
            if entries.len() == 1 {
                out.push_str(&format!(
                    "{INDENT}events [ {} ]\n",
                    example_event_str(&entries[0])
                ));
            } else {
                out.push_str(&format!("{INDENT}events [\n"));
                for e in entries {
                    out.push_str(&format!("{INDENT}{INDENT}{}\n", example_event_str(e)));
                }
                out.push_str(&format!("{INDENT}]\n"));
            }
        }
        ExampleClause::Error { .. } => {}
    }
}

fn fmt_assign_block(kw: &str, entries: &[(String, Expr)], out: &mut String) {
    if entries.len() == 1 {
        out.push_str(&format!(
            "{INDENT}{kw} {{ {} = {} }}\n",
            entries[0].0,
            expr_str(&entries[0].1)
        ));
        return;
    }
    out.push_str(&format!("{INDENT}{kw} {{\n"));
    for (n, v) in entries {
        out.push_str(&format!("{INDENT}{INDENT}{n} = {}\n", expr_str(v)));
    }
    out.push_str(&format!("{INDENT}}}\n"));
}

fn projection_pin_str(p: &ProjectionPin) -> String {
    let key = match &p.key {
        Some(k) => format!("({})", expr_str(k)),
        None => String::new(),
    };
    format!(
        "projection {}.{}{key} = {}",
        p.port,
        p.projection,
        expr_str(&p.value)
    )
}

fn example_event_str(e: &ExampleEvent) -> String {
    match e {
        ExampleEvent::Semantic { name, args, .. } => format!("{name}({})", args_str(args)),
        ExampleEvent::Outcome {
            command,
            which,
            args,
            ..
        } => format!(
            "outcome {command}.{}({})",
            if *which == OutcomeKind::Ok {
                "ok"
            } else {
                "err"
            },
            args_str(args)
        ),
        ExampleEvent::Projection(p) => projection_pin_str(p),
    }
}

// ── leaf renderers ──────────────────────────────────────────────────────────

pub fn type_str(t: &TypeExpr) -> String {
    match &t.kind {
        TypeKind::Name(n) => n.clone(),
        TypeKind::List(inner) => format!("list[{}]", type_str(inner)),
        TypeKind::Map(k, v) => format!("map[{k}]{}", type_str(v)),
        TypeKind::Option(inner) => format!("{}?", type_str(inner)),
        TypeKind::Error => "<error>".to_string(),
    }
}

fn literal_str(l: &Literal) -> String {
    match l {
        Literal::Int(i) => i.to_string(),
        Literal::Str(s) => quote(s),
        Literal::Bool(b) => b.to_string(),
        Literal::None => "none".to_string(),
        Literal::EmptyMap => "{}".to_string(),
        Literal::Error => "<error>".to_string(),
    }
}

fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

fn args_str(args: &[Arg]) -> String {
    args.iter()
        .map(|a| format!("{}: {}", a.name, expr_str(&a.value)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Renders an expression with minimal parentheses (by precedence).
pub fn expr_str(e: &Expr) -> String {
    render_expr(e, 0)
}

/// Precedence levels, loosest → tightest (must mirror the parser).
fn level(e: &Expr) -> u8 {
    match &e.kind {
        ExprKind::If { .. } => 0,
        ExprKind::Binary { op, .. } => match op {
            BinaryOp::Or => 1,
            BinaryOp::And => 2,
            BinaryOp::Eq
            | BinaryOp::NotEq
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge => 3,
            BinaryOp::Coalesce => 4,
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Concat => 5,
        },
        ExprKind::Unary { .. } => 6,
        _ => 7,
    }
}

fn render_expr(e: &Expr, min_level: u8) -> String {
    let mine = level(e);
    let body = match &e.kind {
        ExprKind::Ident(n) => n.clone(),
        ExprKind::Int(i) => i.to_string(),
        ExprKind::Str(s) => quote(s),
        ExprKind::Bool(b) => b.to_string(),
        ExprKind::None => "none".to_string(),
        ExprKind::Field { base, name } => format!("{}.{name}", render_expr(base, 7)),
        ExprKind::Index { base, key } => {
            format!("{}[{}]", render_expr(base, 7), render_expr(key, 0))
        }
        ExprKind::Call { name, args } => format!(
            "{name}({})",
            args.iter()
                .map(|a| render_expr(a, 0))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ExprKind::Unary { op, expr } => {
            let sym = match op {
                UnaryOp::Not => "!",
                UnaryOp::Neg => "-",
            };
            format!("{sym}{}", render_expr(expr, 6))
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let sym = match op {
                BinaryOp::Add => "+",
                BinaryOp::Sub => "-",
                BinaryOp::Concat => "++",
                BinaryOp::Eq => "==",
                BinaryOp::NotEq => "!=",
                BinaryOp::Lt => "<",
                BinaryOp::Le => "<=",
                BinaryOp::Gt => ">",
                BinaryOp::Ge => ">=",
                BinaryOp::And => "&&",
                BinaryOp::Or => "||",
                BinaryOp::Coalesce => "??",
            };
            // Left-associative: rhs needs one level tighter.
            format!(
                "{} {sym} {}",
                render_expr(lhs, mine),
                render_expr(rhs, mine + 1)
            )
        }
        ExprKind::If { cond, then, els } => format!(
            "if {} then {} else {}",
            render_expr(cond, 1),
            render_expr(then, 0),
            render_expr(els, 0)
        ),
        ExprKind::Record(fields) => format!(
            "{{ {} }}",
            fields
                .iter()
                .map(|(n, v)| format!("{n}: {}", render_expr(v, 0)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ExprKind::Error => "<error>".to_string(),
    };
    if mine < min_level {
        format!("({body})")
    } else {
        body
    }
}
