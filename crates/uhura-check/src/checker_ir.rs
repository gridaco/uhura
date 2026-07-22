//! Private source-spanned IR consumed by the checker kernel.
//!
//! This is not an authored language or a public syntax API. The current Uhura
//! frontend resolves and lowers its AST into this stable substrate so the
//! checker kernel can remain independent of source spelling. The tree stops
//! before name and type checking, and every semantically observable order
//! remains source ordered.

use std::fmt;

use serde::{Deserialize, Serialize};

use uhura_base::{FileId, Span};

/// A serialization-friendly checker source identity.
///
/// `FileId` is process-local and deliberately tiny. The lowering substrate
/// also carries the logical path for diagnostics and deterministic identity.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceId {
    pub file: u32,
    pub path: String,
}

/// A UTF-8 byte range retained through frontend lowering.
#[derive(
    Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub struct SourceSpan {
    pub file: u32,
    pub start: u32,
    pub end: u32,
}

impl SourceSpan {
    pub const fn new(file: u32, start: u32, end: u32) -> Self {
        Self { file, start, end }
    }

    pub const fn empty(file: u32, at: u32) -> Self {
        Self::new(file, at, at)
    }

    pub fn to(self, other: Self) -> Self {
        debug_assert_eq!(self.file, other.file);
        Self::new(
            self.file,
            self.start.min(other.start),
            self.end.max(other.end),
        )
    }

    pub fn as_base(self) -> Span {
        Span::new(FileId(self.file), self.start, self.end)
    }
}

impl From<Span> for SourceSpan {
    fn from(value: Span) -> Self {
        Self::new(value.file.0, value.start, value.end)
    }
}

/// A checker-IR value plus its exact authored source extent.
///
/// Structural equality intentionally ignores locations while retaining spans
/// for diagnostics, editor selection, and lowering.
#[derive(Serialize, Deserialize, Clone)]
pub struct Spanned<T> {
    pub value: T,
    pub span: SourceSpan,
}

impl<T> Spanned<T> {
    pub const fn new(value: T, span: SourceSpan) -> Self {
        Self { value, span }
    }
}

impl<T: fmt::Debug> fmt::Debug for Spanned<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Spanned")
            .field("value", &self.value)
            .field("span", &self.span)
            .finish()
    }
}

impl<T: PartialEq> PartialEq for Spanned<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T: Eq> Eq for Spanned<T> {}

pub type Name = Spanned<String>;
pub type TypeExpr = Spanned<TypeExprKind>;
pub type Pattern = Spanned<PatternKind>;
pub type Expr = Spanned<ExprKind>;
pub type Statement = Spanned<StatementKind>;
pub type Declaration = Spanned<DeclarationKind>;
pub type MachineMember = Spanned<MachineMemberKind>;
pub type UiNode = Spanned<UiNodeKind>;
pub type EvidenceStep = Spanned<EvidenceStepKind>;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Module {
    pub source_id: SourceId,
    pub span: SourceSpan,
    pub language: LanguageHeader,
    pub identity: ModuleIdentity,
    pub uses: Vec<UseDecl>,
    pub imports: Vec<ImportDecl>,
    pub declarations: Vec<Declaration>,
}

impl PartialEq for Module {
    fn eq(&self, other: &Self) -> bool {
        self.language == other.language
            && self.identity == other.identity
            && self.uses == other.uses
            && self.imports == other.imports
            && self.declarations == other.declarations
    }
}

impl Eq for Module {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LanguageHeader {
    pub name: Name,
    /// Internal checker-kernel version. Authored source is headerless.
    pub version: String,
    pub span: SourceSpan,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ModuleIdentity {
    pub path: Vec<Name>,
    pub major: String,
    pub span: SourceSpan,
}

impl ModuleIdentity {
    pub fn logical_name(&self) -> String {
        self.path
            .iter()
            .map(|part| part.value.as_str())
            .collect::<Vec<_>>()
            .join(".")
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UseDecl {
    pub feature: Name,
    pub span: SourceSpan,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ImportDecl {
    pub names: Vec<Name>,
    /// The quoted logical identity, retained exactly as decoded text.
    pub target: String,
    /// Parsed exact logical identity.  Kept alongside the decoded spelling so
    /// resolvers never need to reinterpret a string.
    pub identity: ModuleIdentity,
    pub target_span: SourceSpan,
    pub span: SourceSpan,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum DeclarationKind {
    Const(ConstDecl),
    Key(KeyDecl),
    Type(TypeDecl),
    Function(FunctionDecl),
    Machine(MachineDecl),
    Ui(UiDecl),
    Scenario(ScenarioDecl),
    Example(EvidenceAliasDecl),
    Checkpoint(EvidenceAliasDecl),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ConstDecl {
    pub name: Name,
    pub ty: TypeExpr,
    pub value: Expr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct KeyDecl {
    pub name: Name,
    pub over: TypeExpr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TypeDecl {
    pub name: Name,
    pub parameters: Vec<Name>,
    pub body: TypeBody,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum TypeBody {
    Alias(TypeExpr),
    Sum(ClosedSum),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum TypeExprKind {
    Named {
        path: Vec<Name>,
        arguments: Vec<TypeExpr>,
    },
    Record(Vec<TypeField>),
    Tuple(Vec<TypeExpr>),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TypeField {
    pub name: Name,
    pub ty: TypeExpr,
    pub span: SourceSpan,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ClosedSum {
    pub variants: Vec<Variant>,
    pub leading_pipe: bool,
    pub span: SourceSpan,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Variant {
    pub name: Name,
    pub payload: VariantPayload,
    pub span: SourceSpan,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum VariantPayload {
    Unit,
    Positional(Vec<TypeExpr>),
    Named(Vec<TypeField>),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FunctionDecl {
    pub name: Name,
    pub parameters: Vec<Parameter>,
    pub result: TypeExpr,
    pub body: Expr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub name: Name,
    pub ty: TypeExpr,
    pub span: SourceSpan,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MachineDecl {
    pub name: Name,
    /// Kept in source order.  Ordering and cardinality are checker concerns.
    pub members: Vec<MachineMember>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum MachineMemberKind {
    Const(ConstDecl),
    Key(KeyDecl),
    Type(TypeDecl),
    Port(PortDecl),
    Config(FieldBlock),
    Require(Expr),
    Input(SumDomain),
    Command(SumDomain),
    Outcome(OutcomeDecl),
    State(StateDecl),
    Function(FunctionDecl),
    Derive(DeriveDecl),
    Invariant(InvariantDecl),
    Observe(ObserveDecl),
    Transition(TransitionDecl),
    Handler(HandlerDecl),
    BeforeCommit(Block),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PortDecl {
    pub name: Name,
    pub contract: TypeExpr,
    pub configuration: Vec<Expr>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FieldBlock {
    pub fields: Vec<TypeField>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SumDomain {
    Never(Name),
    Sum(ClosedSum),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OutcomeDecl {
    pub variants: Vec<OutcomeVariant>,
    pub leading_pipe: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutcomePolicy {
    Commit,
    Abort,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OutcomeVariant {
    pub variant: Variant,
    pub policy: Spanned<OutcomePolicy>,
    pub span: SourceSpan,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct StateDecl {
    pub fields: Vec<InitializedField>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct InitializedField {
    pub name: Name,
    pub ty: TypeExpr,
    pub value: Expr,
    pub span: SourceSpan,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DeriveDecl {
    pub name: Name,
    pub ty: Option<TypeExpr>,
    pub value: Expr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct InvariantDecl {
    pub expressions: Vec<Expr>,
    pub braced: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ObserveDecl {
    pub fields: Vec<ObserveField>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ObserveField {
    pub name: Name,
    pub ty: Option<TypeExpr>,
    pub value: Expr,
    pub span: SourceSpan,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TransitionDecl {
    pub name: Name,
    pub parameters: Vec<Parameter>,
    pub body: Block,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HandlerDecl {
    pub input: Pattern,
    pub body: HandlerBody,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum HandlerBody {
    Block(Block),
    Delegate(Expr),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub statements: Vec<Statement>,
    pub span: SourceSpan,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum StatementKind {
    Let {
        name: Name,
        ty: Option<TypeExpr>,
        value: Expr,
    },
    Set {
        target: Name,
        value: Expr,
    },
    Emit(Expr),
    While {
        condition: Expr,
        decreases: Expr,
        body: Block,
    },
    Expr(Expr),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ExprKind {
    Integer(String),
    Decimal(String),
    Text(String),
    Bool(bool),
    Name(Name),
    Tuple(Vec<Expr>),
    Sequence(Vec<Expr>),
    Record(Vec<RecordEntry>),
    Block(Block),
    Unary {
        op: Spanned<UnaryOp>,
        operand: Box<Expr>,
    },
    Binary {
        left: Box<Expr>,
        op: Spanned<BinaryOp>,
        right: Box<Expr>,
    },
    Is {
        value: Box<Expr>,
        pattern: Pattern,
    },
    Call {
        callee: Box<Expr>,
        arguments: Vec<Expr>,
    },
    Member {
        receiver: Box<Expr>,
        member: Name,
    },
    Index {
        receiver: Box<Expr>,
        index: Box<Expr>,
    },
    Update {
        base: Box<Expr>,
        fields: Vec<RecordEntry>,
    },
    Lambda {
        parameters: Vec<Pattern>,
        body: Box<Expr>,
    },
    If {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Option<Box<Expr>>,
    },
    Match {
        subject: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    Collect(Vec<CollectClause>),
    SetComprehension {
        binding: Pattern,
        source: Box<Expr>,
        filters: Vec<Expr>,
        value: Box<Expr>,
    },
    Finish(Box<Expr>),
    Unreachable,
    Error,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Negate,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Or,
    And,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Add,
    Subtract,
    Multiply,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RecordEntry {
    pub key: Expr,
    pub value: Expr,
    pub span: SourceSpan,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expr,
    pub span: SourceSpan,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CollectClause {
    pub condition: Expr,
    pub value: Expr,
    pub span: SourceSpan,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum PatternKind {
    Wildcard,
    Rest,
    Integer(String),
    Decimal(String),
    Text(String),
    Bool(bool),
    Name(Name),
    Constructor {
        path: Vec<Name>,
        arguments: Vec<Pattern>,
    },
    Tuple(Vec<Pattern>),
    Record {
        fields: Vec<RecordPatternField>,
        open: bool,
    },
    Alternative(Vec<Pattern>),
    Error,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RecordPatternField {
    pub name: Name,
    pub pattern: Pattern,
    pub span: SourceSpan,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UiDecl {
    pub name: Name,
    pub binding: UiBinding,
    pub nodes: Vec<UiNode>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum UiBinding {
    Machine {
        machine: Name,
        observation: Name,
    },
    Component {
        parameters: Vec<Parameter>,
        emits: SumDomain,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum UiNodeKind {
    Text(String),
    Interpolation(Expr),
    Element(UiElement),
    If {
        condition: Expr,
        children: Vec<UiNode>,
    },
    Match {
        subject: Expr,
        cases: Vec<UiCase>,
    },
    Each {
        source: Expr,
        pattern: Pattern,
        key: Expr,
        children: Vec<UiNode>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UiElement {
    pub name: Name,
    pub attributes: Vec<UiAttribute>,
    pub children: Vec<UiNode>,
    pub self_closing: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UiAttribute {
    pub name: String,
    pub value: UiAttributeValue,
    pub span: SourceSpan,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum UiAttributeValue {
    Text(String),
    Expression(Expr),
    Event { event: Name, input: Expr },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UiCase {
    pub pattern: Pattern,
    pub children: Vec<UiNode>,
    pub span: SourceSpan,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ScenarioDecl {
    pub name: Name,
    pub origin: ScenarioOrigin,
    pub steps: Vec<EvidenceStep>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ScenarioOrigin {
    Machine {
        machine: Name,
        configuration: Option<Expr>,
    },
    Snapshot(EvidenceRef),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct EvidenceRef {
    pub path: Vec<Name>,
    pub span: SourceSpan,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct EvidenceAliasDecl {
    pub name: Name,
    pub presentation: Option<Name>,
    pub arguments: Option<Vec<EvidenceArgument>>,
    pub kind: Option<EvidencePresentationKind>,
    pub is_default: bool,
    pub note: Option<String>,
    pub target: EvidenceRef,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct EvidenceArgument {
    pub name: Name,
    pub value: Expr,
    pub span: SourceSpan,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EvidencePresentationKind {
    Page,
    Component,
    Surface,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum EvidenceStepKind {
    Bind {
        port: Name,
        fixture: Expr,
    },
    Start,
    Send(Expr),
    Deliver(Expr),
    ExpectReaction {
        outcome: Pattern,
        commands: Vec<Expr>,
    },
    ExpectObservationPattern(Pattern),
    ExpectInspectionPattern(Pattern),
    ExpectObservationWhere(Expr),
    ExpectRestore {
        commands: Vec<Expr>,
    },
    ExpectSnapshot {
        target: EvidenceRef,
    },
    Pin(Name),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Project {
    pub modules: Vec<Module>,
}
