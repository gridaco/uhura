//! Serialization-friendly, source-spanned syntax for Uhura 0.4.
//!
//! This tree is deliberately independent from the 0.3 AST. It represents
//! authored syntax only: resolution, typing, effects, cardinality, and
//! lowering remain checker responsibilities.

use serde::{Deserialize, Serialize};

use super::lexer::Token;

/// The resolved identity supplied by project loading before source parsing.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceIdentity {
    /// Process-local file number used by source maps.
    pub file: u32,
    /// Exact resolved package identity, for example `examples.programs@1`.
    pub package: String,
    /// Logical module locator supplied by the project manifest.
    pub module: String,
    /// Physical source path used only for provenance.
    pub path: String,
}

impl SourceIdentity {
    pub fn new(
        file: u32,
        package: impl Into<String>,
        module: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        Self {
            file,
            package: package.into(),
            module: module.into(),
            path: path.into(),
        }
    }
}

/// A half-open UTF-8 byte range in one source file.
#[derive(
    Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub struct Span {
    pub file: u32,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub const fn new(file: u32, start: u32, end: u32) -> Self {
        Self { file, start, end }
    }

    pub const fn empty(file: u32, at: u32) -> Self {
        Self::new(file, at, at)
    }

    pub fn through(self, other: Self) -> Self {
        debug_assert_eq!(self.file, other.file);
        Self::new(
            self.file,
            self.start.min(other.start),
            self.end.max(other.end),
        )
    }
}

/// A syntax value and its exact authored extent.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Node<T> {
    pub kind: T,
    pub span: Span,
}

impl<T> Node<T> {
    pub const fn new(kind: T, span: Span) -> Self {
        Self { kind, span }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Node<U> {
        Node::new(f(self.kind), self.span)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Identifier {
    pub text: String,
    pub span: Span,
}

impl Identifier {
    pub fn new(text: impl Into<String>, span: Span) -> Self {
        Self {
            text: text.into(),
            span,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    #[default]
    Private,
    Public,
}

/// One parsed logical module. `source` and `tokens` make the tree lossless.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub identity: SourceIdentity,
    pub span: Span,
    pub uses: Vec<UseDeclaration>,
    pub declarations: Vec<Declaration>,
    pub source: String,
    pub tokens: Vec<Token>,
}

pub type Declaration = Node<DeclarationKind>;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum DeclarationKind {
    Machine(MachineDeclaration),
    Part(PartDeclaration),
    Ui(UiDeclaration),
    Struct(StructDeclaration),
    Enum(EnumDeclaration),
    Key(KeyDeclaration),
    Const(ConstDeclaration),
    Function(FunctionDeclaration),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UseDeclaration {
    pub visibility: Visibility,
    pub tree: ImportTree,
    pub span: Span,
    pub semicolon: Span,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ImportTree {
    Single {
        path: ImportPath,
        alias: Option<Identifier>,
    },
    Group {
        prefix: ImportPrefix,
        items: Vec<ImportItem>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ImportPath {
    pub root: ImportRoot,
    pub segments: Vec<Identifier>,
    pub span: Span,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ImportPrefix {
    pub root: ImportRoot,
    pub segments: Vec<Identifier>,
    pub span: Span,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ImportRoot {
    Crate(Span),
    Package(Identifier),
}

impl ImportRoot {
    pub fn span(&self) -> Span {
        match self {
            Self::Crate(span) => *span,
            Self::Package(name) => name.span,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ImportItem {
    pub name: Identifier,
    pub alias: Option<Identifier>,
    pub span: Span,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct StructDeclaration {
    pub visibility: Visibility,
    pub name: Identifier,
    pub fields: Vec<TypedField>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct EnumDeclaration {
    pub visibility: Visibility,
    pub name: Identifier,
    pub variants: Vec<EnumVariant>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct EnumVariant {
    pub name: Identifier,
    pub fields: Vec<TypedField>,
    pub span: Span,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct KeyDeclaration {
    pub visibility: Visibility,
    pub name: Identifier,
    pub value: TypeExpression,
    pub semicolon: Span,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ConstDeclaration {
    pub visibility: Visibility,
    pub name: Identifier,
    pub ty: TypeExpression,
    pub value: Expression,
    pub semicolon: Span,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FunctionDeclaration {
    pub visibility: Visibility,
    pub name: Identifier,
    pub parameters: Vec<Parameter>,
    pub result: TypeExpression,
    pub body: Block,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub name: Identifier,
    pub ty: TypeExpression,
    pub span: Span,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TypedField {
    pub name: Identifier,
    pub ty: TypeExpression,
    pub span: Span,
}

pub type TypeExpression = Node<TypeExpressionKind>;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum TypeExpressionKind {
    Path(TypePath),
    Unit,
    Tuple(Vec<TypeExpression>),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TypePath {
    pub segments: Vec<TypePathSegment>,
    pub span: Span,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TypePathSegment {
    pub name: Identifier,
    pub arguments: Vec<TypeExpression>,
    pub span: Span,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MachineDeclaration {
    pub visibility: Visibility,
    pub name: Identifier,
    pub members: Vec<MachineMember>,
}

pub type MachineMember = Node<MachineMemberKind>;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum MachineMemberKind {
    Config(ConfigSection),
    Require(RequireDeclaration),
    Const(ConstDeclaration),
    Function(FunctionDeclaration),
    Part(PartInstance),
    Events(ProtocolSection),
    Commands(ProtocolSection),
    Port(PortDeclaration),
    Outcomes(OutcomeSection),
    State(StateSection),
    Computed(ComputedDeclaration),
    Invariant(InvariantDeclaration),
    Observe(ObserveSection),
    Handler(HandlerDeclaration),
    Update(UpdateDeclaration),
    BeforeCommit(BeforeCommitDeclaration),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PartDeclaration {
    pub visibility: Visibility,
    pub name: Identifier,
    pub parameters: Vec<Parameter>,
    pub members: Vec<PartMember>,
}

/// A pure Web presentation bound to one machine observation.
///
/// Profile activation and binding validity are checker concerns. The syntax
/// frontend recognizes this contextual declaration independently so tooling
/// can parse and format a module before resolution.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UiDeclaration {
    pub visibility: Visibility,
    pub name: Identifier,
    pub machine: TypePath,
    pub observation: Identifier,
    pub body: UiBody,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UiBody {
    pub nodes: Vec<UiNode>,
    /// Core comments authored inside brace-delimited UI expressions. They are
    /// retained separately until RFC 0003 attachment is represented in the
    /// core AST, allowing the formatter to refuse rather than erase them.
    pub embedded_core_comments: Vec<super::lexer::Trivia>,
    /// The exact source extent between the declaration's outer braces.
    pub span: Span,
}

pub type UiNode = Node<UiNodeKind>;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum UiNodeKind {
    Text(UiText),
    Comment(UiComment),
    Interpolation(Expression),
    Element(UiElement),
    If(UiIf),
    Each(UiEach),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UiText {
    /// Exact authored text. The formatter applies the profile's whitespace
    /// normalization only when producing canonical source.
    pub raw: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UiComment {
    /// Exact comment body between `<!--` and `-->`.
    pub body: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UiNameKind {
    Native,
    Component,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UiName {
    pub text: String,
    pub kind: UiNameKind,
    pub span: Span,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UiElement {
    pub name: UiName,
    pub attributes: Vec<UiAttribute>,
    pub children: Vec<UiNode>,
    pub self_closing: bool,
    pub open_span: Span,
    pub close_span: Option<Span>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum UiAttribute {
    /// An authored bare attribute, semantically the Boolean value `true`.
    Boolean { name: UiName, span: Span },
    StaticText {
        name: UiName,
        value: String,
        span: Span,
    },
    Expression {
        name: UiName,
        value: Expression,
        span: Span,
    },
    Event {
        event: UiName,
        input: Expression,
        span: Span,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UiIf {
    pub condition: Expression,
    pub then_branch: Vec<UiNode>,
    pub else_branch: Option<Vec<UiNode>>,
    pub open_span: Span,
    pub else_span: Option<Span>,
    pub close_span: Span,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UiEach {
    pub source: Expression,
    pub pattern: Pattern,
    pub key: Expression,
    pub children: Vec<UiNode>,
    pub open_span: Span,
    pub close_span: Span,
}

pub type PartMember = Node<PartMemberKind>;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum PartMemberKind {
    Require(RequireDeclaration),
    RequiresOutcomes(OutcomeSection),
    Const(ConstDeclaration),
    Function(FunctionDeclaration),
    Events(ProtocolSection),
    Commands(ProtocolSection),
    Port(PortDeclaration),
    State(StateSection),
    Computed(ComputedDeclaration),
    Invariant(InvariantDeclaration),
    Observe(ObserveSection),
    Handler(HandlerDeclaration),
    Update(UpdateDeclaration),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ConfigSection {
    pub fields: Vec<TypedField>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RequireDeclaration {
    pub condition: Expression,
    pub semicolon: Span,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PartInstance {
    pub name: Identifier,
    pub part: TypePath,
    pub arguments: Vec<Expression>,
    pub semicolon: Span,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ProtocolSection {
    pub variants: Vec<ProtocolVariant>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ProtocolVariant {
    pub name: Identifier,
    pub parameters: Vec<Parameter>,
    pub span: Span,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PortDeclaration {
    pub name: Identifier,
    pub contract: TypePath,
    pub fields: Vec<FieldInitializer>,
    pub semicolon: Span,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OutcomeSection {
    pub entries: Vec<OutcomeEntry>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutcomePolicy {
    Commit,
    Abort,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OutcomeEntry {
    pub policy: OutcomePolicy,
    pub variant: ProtocolVariant,
    pub span: Span,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct StateSection {
    pub fields: Vec<StateField>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct StateField {
    pub name: Identifier,
    pub ty: TypeExpression,
    pub initial: Expression,
    pub span: Span,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ComputedDeclaration {
    pub visibility: Visibility,
    pub name: Identifier,
    pub ty: Option<TypeExpression>,
    pub value: Expression,
    pub semicolon: Span,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct InvariantDeclaration {
    pub conditions: Vec<Expression>,
    pub grouped: bool,
    pub semicolon: Option<Span>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ObserveSection {
    pub fields: Vec<ObserveField>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ObserveField {
    pub name: Identifier,
    pub value: Option<Expression>,
    pub span: Span,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HandlerDeclaration {
    pub input: ProtocolSelector,
    pub parameters: Vec<Pattern>,
    pub body: Block,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UpdateDeclaration {
    pub visibility: Visibility,
    pub name: Identifier,
    pub parameters: Vec<Parameter>,
    pub result: Option<TypeExpression>,
    pub body: Block,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct BeforeCommitDeclaration {
    pub body: Block,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ProtocolSelector {
    pub owner: Option<Identifier>,
    pub variant: Identifier,
    pub span: Span,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub statements: Vec<Statement>,
    pub tail: Option<Box<Expression>>,
    pub span: Span,
}

pub type Statement = Node<StatementKind>;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum StatementKind {
    Let {
        name: Identifier,
        ty: Option<TypeExpression>,
        value: Expression,
        semicolon: Span,
    },
    Assign {
        target: Identifier,
        value: Expression,
        semicolon: Span,
    },
    Emit {
        output: OutputConstructor,
        semicolon: Span,
    },
    While {
        condition: Expression,
        decreases: Expression,
        body: Block,
    },
    Unreachable {
        semicolon: Span,
    },
    Expression {
        expression: Expression,
        semicolon: Span,
    },
    /// The grammar's sole unterminated non-final statement exception.
    BlockExpression(Expression),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OutputConstructor {
    pub selector: ProtocolSelector,
    pub arguments: Vec<Expression>,
    pub span: Span,
}

pub type Expression = Node<ExpressionKind>;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ExpressionKind {
    Literal(Literal),
    Unit,
    Sequence(Vec<Expression>),
    Tuple(Vec<Expression>),
    Group(Box<Expression>),
    Name(QualifiedName),
    Record(RecordExpression),
    Block(Block),
    Call {
        callee: Box<Expression>,
        arguments: Vec<CallArgument>,
    },
    Member {
        value: Box<Expression>,
        member: Identifier,
    },
    Index {
        value: Box<Expression>,
        index: Box<Expression>,
    },
    Unary {
        operator: UnaryOperator,
        value: Box<Expression>,
    },
    Binary {
        operator: BinaryOperator,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Compare {
        operator: ComparisonOperator,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Is {
        value: Box<Expression>,
        pattern: Pattern,
    },
    If(IfExpression),
    Match(MatchExpression),
    Return(Option<Box<Expression>>),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Literal {
    Bool(bool),
    Integer { raw: String },
    Decimal { raw: String },
    Text { raw: String, value: String },
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOperator {
    Not,
    Negate,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOperator {
    Multiply,
    Add,
    Subtract,
    And,
    Or,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComparisonOperator {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct QualifiedName {
    pub segments: Vec<Identifier>,
    pub span: Span,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RecordExpression {
    pub constructor: QualifiedName,
    pub fields: Vec<FieldInitializer>,
    pub base: Option<Box<Expression>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FieldInitializer {
    pub name: Identifier,
    pub value: Option<Expression>,
    pub span: Span,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum CallArgument {
    Expression(Expression),
    Binder(BinderExpression),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct BinderExpression {
    pub parameter: Identifier,
    pub body: Expression,
    pub span: Span,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct IfExpression {
    pub condition: Box<Expression>,
    pub then_branch: Block,
    pub else_branch: Option<ElseBranch>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ElseBranch {
    Block(Block),
    If(Box<Expression>),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MatchExpression {
    pub value: Box<Expression>,
    pub arms: Vec<MatchArm>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub value: Expression,
    pub span: Span,
}

pub type Pattern = Node<PatternKind>;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum PatternKind {
    Wildcard,
    Binder(Identifier),
    Literal(PatternLiteral),
    Group(Box<Pattern>),
    Tuple(Vec<Pattern>),
    Constructor(QualifiedName),
    TupleConstructor {
        constructor: QualifiedName,
        arguments: Vec<Pattern>,
    },
    Record {
        constructor: QualifiedName,
        fields: Vec<FieldPattern>,
        rest: bool,
    },
    Alternative(Vec<Pattern>),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum PatternLiteral {
    Bool(bool),
    Integer { raw: String, negative: bool },
    Decimal { raw: String, negative: bool },
    Text { raw: String, value: String },
    Unit,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FieldPattern {
    pub name: Identifier,
    pub pattern: Option<Pattern>,
    pub span: Span,
}
