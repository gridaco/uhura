//! The AST for `.uhura` files (design §4) and `.examples.uhura` files
//! (design §6.1). Parsing always yields a tree; unparseable regions become
//! `…::Error` nodes so downstream passes and the formatter keep working.

use uhura_base::Span;

use crate::token::Comment;

// ── file ────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct File {
    pub kind: DefKind,
    pub uses: Vec<Use>,
    pub props: Vec<PropDecl>,
    pub emits: Vec<EmitDecl>,
    pub params: Vec<ParamDecl>,
    pub store: Option<Store>,
    pub markup: Vec<Node>,
    pub style: Option<StyleBlock>,
}

#[derive(Debug)]
pub enum DefKind {
    Component {
        name: String,
        span: Span,
    },
    Page {
        span: Span,
    },
    Surface {
        name: String,
        modality: Option<String>,
        span: Span,
    },
    /// Header failed to parse; the rest of the file is still attempted.
    Error {
        span: Span,
    },
}

#[derive(Debug)]
pub enum Use {
    Component {
        name: String,
        span: Span,
        leading: Vec<Comment>,
    },
    Surface {
        name: String,
        span: Span,
        leading: Vec<Comment>,
    },
    Port {
        name: String,
        items: Vec<PortItem>,
        span: Span,
        leading: Vec<Comment>,
    },
    /// `.examples.uhura` only.
    Fixture {
        name: String,
        span: Span,
        leading: Vec<Comment>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortItemKind {
    Projection,
    Command,
    Type,
}

#[derive(Debug)]
pub struct PortItem {
    pub kind: PortItemKind,
    pub name: String,
    pub span: Span,
}

#[derive(Debug)]
pub struct PropDecl {
    pub name: String,
    pub ty: TypeExpr,
    pub span: Span,
    pub leading: Vec<Comment>,
}

#[derive(Debug)]
pub struct EmitDecl {
    pub name: String,
    pub params: Vec<(String, TypeExpr)>,
    pub span: Span,
    pub leading: Vec<Comment>,
}

#[derive(Debug)]
pub struct ParamDecl {
    pub name: String,
    pub ty: TypeExpr,
    pub span: Span,
    pub leading: Vec<Comment>,
}

// ── types ───────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct TypeExpr {
    pub kind: TypeKind,
    pub span: Span,
}

#[derive(Debug)]
pub enum TypeKind {
    /// `bool`, `int`, `text`, `id`, `tag`, or a port-declared type name.
    Name(String),
    /// `list[T]`
    List(Box<TypeExpr>),
    /// `map[K]V` — K is a name (`id` | `tag`, checked later).
    Map(String, Box<TypeExpr>),
    /// `T?`
    Option(Box<TypeExpr>),
    Error,
}

// ── store ───────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct Store {
    pub state: Vec<StateField>,
    pub handlers: Vec<Handler>,
    pub span: Span,
    pub leading: Vec<Comment>,
}

#[derive(Debug)]
pub struct StateField {
    pub name: String,
    pub ty: TypeExpr,
    pub init: Literal,
    pub span: Span,
    pub leading: Vec<Comment>,
}

/// State initializers are literals only (design §4.3).
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int(i64),
    Str(String),
    Bool(bool),
    None,
    /// `{}` — the empty map.
    EmptyMap,
    Error,
}

#[derive(Debug)]
pub struct Handler {
    pub event: EventRef,
    /// UI-event handlers declare typed params; outcome handlers are
    /// name-only (`ty` is `None`, types come from the contract — §4.2).
    pub params: Vec<HandlerParam>,
    pub guard: Option<Expr>,
    pub body: Vec<Stmt>,
    pub span: Span,
    pub leading: Vec<Comment>,
}

#[derive(Debug)]
pub struct HandlerParam {
    pub name: String,
    pub ty: Option<TypeExpr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutcomeKind {
    Ok,
    Err,
}

#[derive(Debug)]
pub enum EventRef {
    /// `on like-toggled(...)`
    Semantic { name: String, span: Span },
    /// `on like-post.ok(...)` / `.err(...)`
    Outcome {
        command: String,
        which: OutcomeKind,
        span: Span,
    },
}

/// The five statements — closed set (design §4.2).
#[derive(Debug)]
pub enum Stmt {
    Set {
        path: SetPath,
        value: Expr,
        span: Span,
        leading: Vec<Comment>,
    },
    Send {
        command: String,
        args: Vec<Arg>,
        bind: Option<String>,
        span: Span,
        leading: Vec<Comment>,
    },
    OpenSurface {
        name: String,
        args: Vec<Arg>,
        span: Span,
        leading: Vec<Comment>,
    },
    Dismiss {
        span: Span,
        leading: Vec<Comment>,
    },
    Navigate {
        target: NavTarget,
        span: Span,
        leading: Vec<Comment>,
    },
    Error {
        span: Span,
    },
}

#[derive(Debug)]
pub struct SetPath {
    pub field: String,
    /// `field[key]` — at most one level (micro-decision: paths are `field`
    /// or `field[key]` only).
    pub key: Option<Expr>,
    pub span: Span,
}

#[derive(Debug)]
pub enum NavTarget {
    Route { name: String, args: Vec<Arg> },
    Back,
}

/// A named argument (`name: expr`) — all argument lists are named.
#[derive(Debug)]
pub struct Arg {
    pub name: String,
    pub value: Expr,
    pub span: Span,
}

// ── expressions ─────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Neg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Concat,
    Eq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Coalesce,
}

#[derive(Debug)]
pub enum ExprKind {
    /// Name reference: state field, prop, param, binding, or projection.
    Ident(String),
    Int(i64),
    Str(String),
    Bool(bool),
    None,
    /// `base.field`
    Field {
        base: Box<Expr>,
        name: String,
    },
    /// `base[key]` — option-returning (§4.3).
    Index {
        base: Box<Expr>,
        key: Box<Expr>,
    },
    /// `name(args…)` — builtins (`to-text`, `count`) and keyed projection
    /// reads (`for-post(post)`); resolution decides which.
    Call {
        name: String,
        args: Vec<Expr>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// `if c then a else b`
    If {
        cond: Box<Expr>,
        then: Box<Expr>,
        els: Box<Expr>,
    },
    /// `{ field: expr, … }` — legal on `set` rhs and in example pins.
    Record(Vec<(String, Expr)>),
    Error,
}

// ── markup ──────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum Node {
    Element(Element),
    /// Text content — only meaningful inside `<text>` (checked later).
    Text {
        runs: Vec<TextRun>,
        span: Span,
    },
    If {
        cond: Expr,
        then: Vec<Node>,
        els: Option<Vec<Node>>,
        span: Span,
    },
    Each {
        item: String,
        seq: Expr,
        key: Expr,
        body: Vec<Node>,
        span: Span,
    },
    Match {
        scrutinee: Expr,
        arms: Vec<MatchArm>,
        span: Span,
    },
    Error {
        span: Span,
    },
}

#[derive(Debug)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    /// `{:when carousel c}` — the optional value binding.
    pub binding: Option<String>,
    pub body: Vec<Node>,
    pub span: Span,
}

#[derive(Debug)]
pub enum MatchPattern {
    /// A union variant, or an availability arm (`loading` / `failed` /
    /// `ready`) — resolution decides which (design §9.2).
    Variant(String),
    Else,
}

#[derive(Debug)]
pub struct Element {
    /// Catalog element or imported component name.
    pub name: String,
    pub attrs: Vec<Attr>,
    pub events: Vec<EventAttr>,
    pub children: Vec<Node>,
    pub self_closing: bool,
    pub span: Span,
}

#[derive(Debug)]
pub enum TextRun {
    Literal(String),
    Interp(Expr),
}

#[derive(Debug)]
pub struct Attr {
    pub name: String,
    pub value: AttrValue,
    pub span: Span,
}

#[derive(Debug)]
pub enum AttrValue {
    /// `attr="literal"`
    Literal(String),
    /// `attr={expr}`
    Expr(Expr),
    /// bare `attr` — boolean true.
    Bare,
}

#[derive(Debug)]
pub struct EventAttr {
    /// The event name after `on:` — a catalog event on elements, an emit
    /// name on components.
    pub event: String,
    pub binding: EventBinding,
    pub span: Span,
}

#[derive(Debug)]
pub enum EventBinding {
    /// `on:press={emit like-toggled(post: p.id)}`
    Emit { name: String, args: Vec<Arg> },
    /// Bare `on:like-toggled` — forwards same name + payload to the
    /// enclosing machine scope (component emits only, §4.4).
    Forward,
}

// ── style ───────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct StyleBlock {
    pub rules: Vec<StyleRule>,
    /// The whole `<style>…</style>` inner text, verbatim.
    pub raw: String,
    pub span: Span,
}

#[derive(Debug)]
pub struct StyleRule {
    /// Selector text, verbatim (normalized whitespace).
    pub selector: String,
    /// Class names referenced by the selector, for rooting/existence checks.
    pub classes: Vec<String>,
    /// Declaration block, verbatim, without the outer braces.
    pub decls: String,
    pub span: Span,
}

// ── examples files (design §6.1) ────────────────────────────────────────────

#[derive(Debug)]
pub struct ExamplesFile {
    pub uses: Vec<Use>,
    pub examples: Vec<ExampleDecl>,
}

#[derive(Debug)]
pub struct ExampleDecl {
    pub name: String,
    pub is_default: bool,
    pub clauses: Vec<ExampleClause>,
    pub span: Span,
    pub leading: Vec<Comment>,
}

#[derive(Debug)]
pub enum ExampleClause {
    From {
        name: String,
        span: Span,
    },
    Note {
        text: String,
        span: Span,
    },
    /// `params { user = "…" }` (pages with dynamic segments).
    Params {
        entries: Vec<(String, Expr)>,
        span: Span,
    },
    /// `props { post = fixture.posts.x, … }` (components/surfaces).
    Props {
        entries: Vec<(String, Expr)>,
        span: Span,
    },
    /// `state { field = expr }` — literal state pin.
    State {
        entries: Vec<(String, Expr)>,
        span: Span,
    },
    /// `projection feed.feed-page = fixture.feed.page-1`
    /// `projection comments.for-post("post-1") = fixture.comments.x`
    Projection(ProjectionPin),
    /// `events [ … ]` — the derivation timeline.
    Events {
        entries: Vec<ExampleEvent>,
        span: Span,
    },
    Error {
        span: Span,
    },
}

#[derive(Debug)]
pub struct ProjectionPin {
    pub port: String,
    pub projection: String,
    pub key: Option<Expr>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug)]
pub enum ExampleEvent {
    /// `like-toggled(post: "post-1", now-liked: true)`
    Semantic {
        name: String,
        args: Vec<Arg>,
        span: Span,
    },
    /// `outcome like-post.err(refusal: rate-limited)`
    Outcome {
        command: String,
        which: OutcomeKind,
        args: Vec<Arg>,
        span: Span,
    },
    /// `projection feed.feed-page = fixture.feed.pages-1-2`
    Projection(ProjectionPin),
}
