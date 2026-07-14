//! The AST for `.uhura` files (design §4) and `.examples.uhura` files
//! (design §6.1). Parsing always yields a tree; unparseable regions become
//! `…::Error` nodes so downstream passes and the formatter keep working.

use uhura_base::Span;

use crate::token::{Comment, CommentKind};

// ── source trivia and authoring metadata ───────────────────────────────────

/// One normalized, non-empty DSL documentation run. `pieces` below retains
/// the exact interleaving with ordinary comments for formatting; this table is
/// the structured attachment consumed by checking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocComment {
    pub form: DocForm,
    pub text: String,
    /// Envelope from the first doc sigil through the final doc token.
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocForm {
    Outer,
    Inner,
}

/// Ordered DSL trivia attached to one legal item/list boundary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DslTrivia {
    /// Exact lexical pieces, in source order. Empty doc runs remain here so
    /// they can act as run boundaries; the formatter omits their doc lines.
    pub pieces: Vec<Comment>,
    /// Normalized, non-empty documentation runs.
    pub docs: Vec<DocComment>,
}

impl DslTrivia {
    pub fn new(pieces: Vec<Comment>) -> Self {
        let mut docs = Vec::new();
        let mut cursor = 0;
        while cursor < pieces.len() {
            let form = match pieces[cursor].kind {
                CommentKind::Ordinary => {
                    cursor += 1;
                    continue;
                }
                CommentKind::OuterDoc => DocForm::Outer,
                CommentKind::InnerDoc => DocForm::Inner,
            };
            let start = cursor;
            let mut end = cursor;
            let mut lines = Vec::new();
            let mut last_doc = cursor;
            while end < pieces.len() {
                let piece_form = match pieces[end].kind {
                    CommentKind::Ordinary => None,
                    CommentKind::OuterDoc => Some(DocForm::Outer),
                    CommentKind::InnerDoc => Some(DocForm::Inner),
                };
                if piece_form.is_some_and(|candidate| candidate != form) {
                    break;
                }
                if piece_form == Some(form) {
                    lines.push(pieces[end].normalized_doc_line());
                    last_doc = end;
                }
                end += 1;
            }
            while lines.last().is_some_and(String::is_empty) {
                lines.pop();
            }
            if !lines.is_empty() {
                docs.push(DocComment {
                    form,
                    text: lines.join("\n"),
                    span: pieces[start].span.to(pieces[last_doc].span),
                });
            }
            cursor = end;
        }
        Self { pieces, docs }
    }

    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }

    /// Whether canonical formatting will emit at least one trivia line.
    /// Empty normalized doc runs remain lexical pieces so they can separate
    /// runs during parsing, but they must not affect canonical layout.
    pub fn has_formattable_content(&self) -> bool {
        !self.docs.is_empty()
            || self
                .pieces
                .iter()
                .any(|piece| piece.kind == CommentKind::Ordinary)
    }
}

// ── file ────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct File {
    /// Trivia before the component/page/surface header. Inner docs target the
    /// source module; outer docs target the header declaration.
    pub preamble: DslTrivia,
    pub kind: DefKind,
    pub uses: Vec<Use>,
    pub props_present: bool,
    pub props_leading: DslTrivia,
    pub props: Vec<PropDecl>,
    pub props_trailing: DslTrivia,
    pub emits_present: bool,
    pub emits_leading: DslTrivia,
    pub emits: Vec<EmitDecl>,
    pub emits_trailing: DslTrivia,
    pub params: Vec<ParamDecl>,
    pub store: Option<Store>,
    /// Legal DSL trivia immediately before markup, style, or EOF.
    pub trailing_dsl: DslTrivia,
    pub markup: MarkupList,
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
        leading: DslTrivia,
    },
    Surface {
        name: String,
        span: Span,
        leading: DslTrivia,
    },
    Port {
        name: String,
        items: Vec<PortItem>,
        span: Span,
        leading: DslTrivia,
    },
    /// `.examples.uhura` only.
    Fixture {
        name: String,
        span: Span,
        leading: DslTrivia,
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
    pub leading: DslTrivia,
}

#[derive(Debug)]
pub struct EmitDecl {
    pub name: String,
    pub params: Vec<EmitParam>,
    pub params_trailing: DslTrivia,
    pub span: Span,
    pub leading: DslTrivia,
}

#[derive(Debug)]
pub struct EmitParam {
    pub name: String,
    pub ty: TypeExpr,
    pub span: Span,
    pub leading: DslTrivia,
}

#[derive(Debug)]
pub struct ParamDecl {
    pub name: String,
    pub ty: TypeExpr,
    pub span: Span,
    pub leading: DslTrivia,
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
    pub state_present: bool,
    pub state: Vec<StateField>,
    pub handlers: Vec<Handler>,
    pub state_leading: DslTrivia,
    pub state_trailing: DslTrivia,
    pub trailing: DslTrivia,
    pub span: Span,
    pub leading: DslTrivia,
}

#[derive(Debug)]
pub struct StateField {
    pub name: String,
    pub ty: TypeExpr,
    pub init: Literal,
    pub span: Span,
    pub leading: DslTrivia,
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
    pub params_trailing: DslTrivia,
    pub guard: Option<Expr>,
    pub body: Vec<Stmt>,
    pub body_trailing: DslTrivia,
    pub span: Span,
    pub leading: DslTrivia,
}

#[derive(Debug)]
pub struct HandlerParam {
    pub name: String,
    pub ty: Option<TypeExpr>,
    pub span: Span,
    pub leading: DslTrivia,
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
        leading: DslTrivia,
    },
    Send {
        command: String,
        args: Vec<Arg>,
        bind: Option<String>,
        span: Span,
        leading: DslTrivia,
    },
    OpenSurface {
        name: String,
        args: Vec<Arg>,
        span: Span,
        leading: DslTrivia,
    },
    Dismiss {
        span: Span,
        leading: DslTrivia,
    },
    Navigate {
        target: NavTarget,
        span: Span,
        leading: DslTrivia,
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
    Replace { name: String, args: Vec<Arg> },
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
        annotations: Vec<MarkupAnnotation>,
        cond: Expr,
        then: MarkupList,
        els: Option<MarkupList>,
        span: Span,
    },
    Each {
        annotations: Vec<MarkupAnnotation>,
        item: String,
        seq: Expr,
        key: Expr,
        body: MarkupList,
        span: Span,
    },
    Match {
        annotations: Vec<MarkupAnnotation>,
        scrutinee: Expr,
        /// Source layout before the first arm. It may contain ordinary
        /// comments but no valid semantic nodes.
        before_arms: MarkupList,
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
    pub body: MarkupList,
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
    pub children: MarkupList,
    pub annotations: Vec<MarkupAnnotation>,
    pub self_closing: bool,
    pub span: Span,
}

/// An attached, normalized markup annotation. It is duplicated in the
/// sibling list's source layout so formatting remains independent from the
/// checked metadata projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkupAnnotation {
    pub kind: String,
    pub text: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkupCommentKind {
    Ordinary,
    Annotation {
        kind: String,
    },
    /// Kept only for recovery/layout. It never becomes metadata. Retaining
    /// termination state lets formatting preserve the lexical error instead
    /// of silently manufacturing a valid comment or annotation.
    Malformed {
        terminated: bool,
    },
    /// A well-formed annotation encountered a recovery node that canonical
    /// formatting cannot reproduce. The formatter emits a stable malformed
    /// carrier which retains the visible kind and prose without allowing the
    /// annotation to attach to a later valid target.
    RejectedAnnotation {
        kind: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkupComment {
    pub kind: MarkupCommentKind,
    /// Normalized body/payload. For malformed recovery this is best-effort
    /// body text.
    pub text: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacedMarkupComment {
    /// Semantic node index before which this comment formats. `nodes.len()`
    /// is trailing sibling-list trivia.
    pub before: usize,
    pub comment: MarkupComment,
}

/// A markup sibling list with source layout separate from semantic nodes.
/// Ordinary comments and annotation carriers therefore never affect node,
/// child, or root cardinality.
#[derive(Debug, Default)]
pub struct MarkupList {
    pub nodes: Vec<Node>,
    pub comments: Vec<PlacedMarkupComment>,
}

impl MarkupList {
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn first(&self) -> Option<&Node> {
        self.nodes.first()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Node> {
        self.nodes.iter()
    }
}

impl std::ops::Index<usize> for MarkupList {
    type Output = Node;

    fn index(&self, index: usize) -> &Self::Output {
        &self.nodes[index]
    }
}

impl<'a> IntoIterator for &'a MarkupList {
    type Item = &'a Node;
    type IntoIter = std::slice::Iter<'a, Node>;

    fn into_iter(self) -> Self::IntoIter {
        self.nodes.iter()
    }
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
    pub preamble: DslTrivia,
    pub uses: Vec<Use>,
    pub examples: Vec<ExampleDecl>,
    pub trailing: DslTrivia,
}

#[derive(Debug)]
pub struct ExampleDecl {
    pub name: String,
    pub is_default: bool,
    pub clauses: Vec<ExampleClause>,
    /// Parallel to `clauses`; keeps ordinary comments and rejected docs at
    /// the legal clause boundary without making them semantic clauses.
    pub clause_leading: Vec<DslTrivia>,
    pub trailing: DslTrivia,
    pub span: Span,
    pub leading: DslTrivia,
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
