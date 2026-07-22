//! Static lowering for non-terminal Uhura updates.
//!
//! The machine kernel models one reaction transaction and terminal `Outcome`
//! transitions. Uhura 0.4 updates that return `Unit` or an ordinary closed
//! value are therefore source-level checked helpers: this pass alpha-renames
//! and inlines them before the checker-neutral bridge. Lexical `return` is
//! handled by continuation-passing source rewriting, so it returns to the
//! update caller rather than terminating the enclosing reaction.

use std::collections::{BTreeMap, BTreeSet};

use uhura_base::Diagnostic;
use uhura_core::{INLINE_UPDATE_JOIN_LOCAL_PREFIX, INLINE_UPDATE_LOOP_EXIT_LOCAL_PREFIX};

use crate::checker_ir as ast;
use crate::diagnostic::{codes, error};

#[derive(Clone, Debug)]
struct InlineUpdate {
    declaration: uhura_syntax::ast::UpdateDeclaration,
    result: uhura_syntax::ast::TypeExpression,
    span: uhura_syntax::ast::Span,
}

#[derive(Clone, Debug)]
enum EffectContinuation {
    Tail,
    Return,
    BindThen {
        name: uhura_syntax::ast::Identifier,
        ty: Option<uhura_syntax::ast::TypeExpression>,
        remainder: uhura_syntax::ast::Block,
        normal: Box<EffectContinuation>,
        span: uhura_syntax::ast::Span,
    },
    DiscardThen {
        remainder: uhura_syntax::ast::Block,
        normal: Box<EffectContinuation>,
        span: uhura_syntax::ast::Span,
    },
    CaptureUpdateResult {
        name: String,
        ty: uhura_syntax::ast::TypeExpression,
        span: uhura_syntax::ast::Span,
    },
    CaptureUpdateLoopExit {
        name: String,
        ty: uhura_syntax::ast::TypeExpression,
        span: uhura_syntax::ast::Span,
    },
    Unary {
        operator: uhura_syntax::ast::UnaryOperator,
        span: uhura_syntax::ast::Span,
        next: Box<EffectContinuation>,
    },
    BinaryLeft {
        operator: uhura_syntax::ast::BinaryOperator,
        right: uhura_syntax::ast::Expression,
        span: uhura_syntax::ast::Span,
        next: Box<EffectContinuation>,
    },
    BinaryRight {
        operator: uhura_syntax::ast::BinaryOperator,
        left: uhura_syntax::ast::Expression,
        span: uhura_syntax::ast::Span,
        next: Box<EffectContinuation>,
    },
    ShortCircuitLeft {
        operator: uhura_syntax::ast::BinaryOperator,
        right: uhura_syntax::ast::Expression,
        span: uhura_syntax::ast::Span,
        next: Box<EffectContinuation>,
    },
    CompareLeft {
        operator: uhura_syntax::ast::ComparisonOperator,
        right: uhura_syntax::ast::Expression,
        span: uhura_syntax::ast::Span,
        next: Box<EffectContinuation>,
    },
    CompareRight {
        operator: uhura_syntax::ast::ComparisonOperator,
        left: uhura_syntax::ast::Expression,
        span: uhura_syntax::ast::Span,
        next: Box<EffectContinuation>,
    },
    Member {
        member: uhura_syntax::ast::Identifier,
        span: uhura_syntax::ast::Span,
        next: Box<EffectContinuation>,
    },
    IndexValue {
        index: uhura_syntax::ast::Expression,
        span: uhura_syntax::ast::Span,
        next: Box<EffectContinuation>,
    },
    IndexIndex {
        value: uhura_syntax::ast::Expression,
        span: uhura_syntax::ast::Span,
        next: Box<EffectContinuation>,
    },
    Is {
        pattern: uhura_syntax::ast::Pattern,
        span: uhura_syntax::ast::Span,
        next: Box<EffectContinuation>,
    },
    Sequence {
        tuple: bool,
        completed: Vec<uhura_syntax::ast::Expression>,
        remaining: Vec<uhura_syntax::ast::Expression>,
        span: uhura_syntax::ast::Span,
        next: Box<EffectContinuation>,
    },
    IfCondition {
        expression: uhura_syntax::ast::IfExpression,
        span: uhura_syntax::ast::Span,
        next: Box<EffectContinuation>,
    },
    MatchSubject {
        expression: uhura_syntax::ast::MatchExpression,
        span: uhura_syntax::ast::Span,
        next: Box<EffectContinuation>,
    },
    CallCallee {
        arguments: Vec<uhura_syntax::ast::CallArgument>,
        span: uhura_syntax::ast::Span,
        next: Box<EffectContinuation>,
    },
    CallArgument {
        callee: uhura_syntax::ast::Expression,
        arguments: Vec<uhura_syntax::ast::CallArgument>,
        index: usize,
        span: uhura_syntax::ast::Span,
        next: Box<EffectContinuation>,
    },
    InlineCallArgument {
        target: String,
        arguments: Vec<uhura_syntax::ast::Expression>,
        index: usize,
        span: uhura_syntax::ast::Span,
        next: Box<EffectContinuation>,
    },
    RecordField {
        record: uhura_syntax::ast::RecordExpression,
        index: usize,
        span: uhura_syntax::ast::Span,
        next: Box<EffectContinuation>,
    },
    RecordBase {
        record: uhura_syntax::ast::RecordExpression,
        span: uhura_syntax::ast::Span,
        next: Box<EffectContinuation>,
    },
    AssignValue {
        target: uhura_syntax::ast::Identifier,
        semicolon: uhura_syntax::ast::Span,
        remainder: uhura_syntax::ast::Block,
        normal: Box<EffectContinuation>,
        statement_span: uhura_syntax::ast::Span,
        block_span: uhura_syntax::ast::Span,
    },
    EmitArgument {
        output: uhura_syntax::ast::OutputConstructor,
        index: usize,
        semicolon: uhura_syntax::ast::Span,
        remainder: uhura_syntax::ast::Block,
        normal: Box<EffectContinuation>,
        statement_span: uhura_syntax::ast::Span,
        block_span: uhura_syntax::ast::Span,
    },
    WhileCondition {
        decreases: uhura_syntax::ast::Expression,
        body: uhura_syntax::ast::Block,
        remainder: uhura_syntax::ast::Block,
        normal: Box<EffectContinuation>,
        statement_span: uhura_syntax::ast::Span,
        block_span: uhura_syntax::ast::Span,
    },
    WhileMeasure {
        condition: uhura_syntax::ast::Expression,
        body: uhura_syntax::ast::Block,
        remainder: uhura_syntax::ast::Block,
        normal: Box<EffectContinuation>,
        statement_span: uhura_syntax::ast::Span,
        block_span: uhura_syntax::ast::Span,
    },
}

struct Engine<'a> {
    updates: BTreeMap<String, InlineUpdate>,
    diagnostics: &'a mut Vec<Diagnostic>,
    stack: Vec<String>,
    temporary: u64,
}

pub(super) fn lower_project(
    modules: &mut [uhura_syntax::ast::Module],
    diagnostics: &mut Vec<Diagnostic>,
) {
    for module in modules {
        for declaration in &mut module.declarations {
            let uhura_syntax::ast::DeclarationKind::Machine(machine) = &mut declaration.kind else {
                continue;
            };
            lower_machine(machine, diagnostics);
        }
    }
}

fn lower_machine(
    machine: &mut uhura_syntax::ast::MachineDeclaration,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut updates = BTreeMap::new();
    for member in &machine.members {
        let uhura_syntax::ast::MachineMemberKind::Update(update) = &member.kind else {
            continue;
        };
        if update_returns_outcome(update) {
            continue;
        }
        let result = update
            .result
            .clone()
            .unwrap_or_else(|| unit_type(update.name.span));
        if updates
            .insert(
                update.name.text.clone(),
                InlineUpdate {
                    declaration: update.clone(),
                    result,
                    span: member.span,
                },
            )
            .is_some()
        {
            diagnostic(
                diagnostics,
                codes::DUPLICATE,
                "uhura-0.4/duplicate-update",
                format!("update `{}` is declared more than once", update.name.text),
                update.name.span,
            );
        }
    }
    if updates.is_empty() {
        return;
    }

    reject_cycles(&updates, diagnostics, machine.name.span);
    machine.members.retain(|member| {
        !matches!(
            &member.kind,
            uhura_syntax::ast::MachineMemberKind::Update(update)
                if updates.contains_key(&update.name.text)
        )
    });

    let mut engine = Engine {
        updates,
        diagnostics,
        stack: Vec::new(),
        temporary: 0,
    };
    for member in &mut machine.members {
        let body = match &mut member.kind {
            uhura_syntax::ast::MachineMemberKind::Handler(value) => Some(&mut value.body),
            uhura_syntax::ast::MachineMemberKind::Update(value) => Some(&mut value.body),
            uhura_syntax::ast::MachineMemberKind::BeforeCommit(value) => Some(&mut value.body),
            _ => None,
        };
        if let Some(body) = body {
            // Handler/before-commit placement is nonsemantic. Generated
            // update-control ordinals are scoped to one lowered body.
            engine.temporary = 0;
            *body = engine.lower_block(
                body.clone(),
                EffectContinuation::Tail,
                EffectContinuation::Return,
            );
        }
    }
}

impl Engine<'_> {
    fn lower_block(
        &mut self,
        block: uhura_syntax::ast::Block,
        normal: EffectContinuation,
        returned: EffectContinuation,
    ) -> uhura_syntax::ast::Block {
        self.lower_block_at(&block, 0, normal, returned)
    }

    fn lower_block_at(
        &mut self,
        block: &uhura_syntax::ast::Block,
        index: usize,
        normal: EffectContinuation,
        returned: EffectContinuation,
    ) -> uhura_syntax::ast::Block {
        let Some(statement) = block.statements.get(index) else {
            if block.tail.is_none() && matches!(normal, EffectContinuation::Tail) {
                return uhura_syntax::ast::Block {
                    statements: Vec::new(),
                    tail: None,
                    span: block.span,
                };
            }
            let value = block.tail.as_deref().cloned().unwrap_or_else(|| {
                uhura_syntax::ast::Node::new(uhura_syntax::ast::ExpressionKind::Unit, block.span)
            });
            return self.lower_expression(value, normal, returned);
        };
        let remainder = uhura_syntax::ast::Block {
            statements: block.statements[index + 1..].to_vec(),
            tail: block.tail.clone(),
            span: block.span,
        };
        match &statement.kind {
            uhura_syntax::ast::StatementKind::Let {
                name, ty, value, ..
            } => self.lower_expression(
                value.clone(),
                EffectContinuation::BindThen {
                    name: name.clone(),
                    ty: ty.clone(),
                    remainder,
                    normal: Box::new(normal),
                    span: statement.span,
                },
                returned,
            ),
            uhura_syntax::ast::StatementKind::Expression { expression, .. }
            | uhura_syntax::ast::StatementKind::BlockExpression(expression) => self
                .lower_expression(
                    expression.clone(),
                    EffectContinuation::DiscardThen {
                        remainder,
                        normal: Box::new(normal),
                        span: statement.span,
                    },
                    returned,
                ),
            uhura_syntax::ast::StatementKind::While {
                condition,
                decreases,
                body,
            } => {
                self.reject_nested_calls(condition, "a loop condition");
                self.reject_nested_calls(decreases, "a loop measure");
                self.lower_expression(
                    condition.clone(),
                    EffectContinuation::WhileCondition {
                        decreases: decreases.clone(),
                        body: body.clone(),
                        remainder,
                        normal: Box::new(normal),
                        statement_span: statement.span,
                        block_span: block.span,
                    },
                    returned,
                )
            }
            uhura_syntax::ast::StatementKind::Assign {
                target,
                value,
                semicolon,
            } => {
                self.reject_nested_calls(value, "a state assignment");
                self.lower_expression(
                    value.clone(),
                    EffectContinuation::AssignValue {
                        target: target.clone(),
                        semicolon: *semicolon,
                        remainder,
                        normal: Box::new(normal),
                        statement_span: statement.span,
                        block_span: block.span,
                    },
                    returned,
                )
            }
            uhura_syntax::ast::StatementKind::Emit { output, semicolon } => {
                for value in &output.arguments {
                    self.reject_nested_calls(value, "a command payload");
                }
                self.lower_emit_arguments(
                    output.clone(),
                    0,
                    *semicolon,
                    remainder,
                    normal,
                    returned,
                    statement.span,
                    block.span,
                )
            }
            uhura_syntax::ast::StatementKind::Unreachable { .. } => uhura_syntax::ast::Block {
                statements: vec![statement.clone()],
                tail: None,
                span: block.span,
            },
        }
    }

    fn lower_expression(
        &mut self,
        expression: uhura_syntax::ast::Expression,
        normal: EffectContinuation,
        returned: EffectContinuation,
    ) -> uhura_syntax::ast::Block {
        if let Some((target, arguments)) = direct_inline_call(&expression, &self.updates) {
            if matches!(normal, EffectContinuation::DiscardThen { .. })
                && !is_unit_type(&self.updates[&target].result)
            {
                diagnostic(
                    self.diagnostics,
                    codes::EFFECT,
                    "uhura-0.4/discarded-effectful-update",
                    format!("value-returning update `{target}` cannot be discarded as a statement"),
                    expression.span,
                );
            }
            for argument in &arguments {
                self.reject_nested_calls(argument, "an update argument");
            }
            return self.lower_inline_call_arguments(
                target,
                arguments,
                0,
                normal,
                returned,
                expression.span,
            );
        }

        let structural_expression = matches!(
            &expression.kind,
            uhura_syntax::ast::ExpressionKind::Group(_)
                | uhura_syntax::ast::ExpressionKind::Block(_)
                | uhura_syntax::ast::ExpressionKind::If(_)
                | uhura_syntax::ast::ExpressionKind::Match(_)
        );
        let needs_structural_effect_lowering = structural_expression
            && (first_inline_call(&expression, &self.updates).is_some()
                || matches!(&normal, EffectContinuation::CaptureUpdateResult { .. }));
        if !expression_has_return(&expression) && !needs_structural_effect_lowering {
            self.reject_nested_calls(&expression, "a nested expression");
            return self.apply_continuation(expression, normal, returned);
        }

        match expression.kind {
            uhura_syntax::ast::ExpressionKind::Return(value) => {
                let value = value.map_or_else(
                    || {
                        uhura_syntax::ast::Node::new(
                            uhura_syntax::ast::ExpressionKind::Unit,
                            expression.span,
                        )
                    },
                    |value| *value,
                );
                self.lower_expression(value, returned.clone(), returned)
            }
            uhura_syntax::ast::ExpressionKind::Group(value) => {
                self.lower_expression(*value, normal, returned)
            }
            uhura_syntax::ast::ExpressionKind::Block(block) => {
                self.lower_block(block, normal, returned)
            }
            uhura_syntax::ast::ExpressionKind::Unary { operator, value } => {
                self.reject_nested_calls(&value, "a unary operand");
                self.lower_expression(
                    *value,
                    EffectContinuation::Unary {
                        operator,
                        span: expression.span,
                        next: Box::new(normal),
                    },
                    returned,
                )
            }
            uhura_syntax::ast::ExpressionKind::Binary {
                operator,
                left,
                right,
            } => {
                self.reject_nested_calls(&left, "a binary operand");
                self.reject_nested_calls(&right, "a binary operand");
                let continuation = if matches!(
                    operator,
                    uhura_syntax::ast::BinaryOperator::And | uhura_syntax::ast::BinaryOperator::Or
                ) && expression_has_return(&right)
                {
                    EffectContinuation::ShortCircuitLeft {
                        operator,
                        right: *right,
                        span: expression.span,
                        next: Box::new(normal),
                    }
                } else {
                    EffectContinuation::BinaryLeft {
                        operator,
                        right: *right,
                        span: expression.span,
                        next: Box::new(normal),
                    }
                };
                self.lower_expression(*left, continuation, returned)
            }
            uhura_syntax::ast::ExpressionKind::Compare {
                operator,
                left,
                right,
            } => {
                self.reject_nested_calls(&left, "a comparison operand");
                self.reject_nested_calls(&right, "a comparison operand");
                self.lower_expression(
                    *left,
                    EffectContinuation::CompareLeft {
                        operator,
                        right: *right,
                        span: expression.span,
                        next: Box::new(normal),
                    },
                    returned,
                )
            }
            uhura_syntax::ast::ExpressionKind::Member { value, member } => {
                self.reject_nested_calls(&value, "a member receiver");
                self.lower_expression(
                    *value,
                    EffectContinuation::Member {
                        member,
                        span: expression.span,
                        next: Box::new(normal),
                    },
                    returned,
                )
            }
            uhura_syntax::ast::ExpressionKind::Index { value, index } => {
                self.reject_nested_calls(&value, "an index receiver");
                self.reject_nested_calls(&index, "an index operand");
                self.lower_expression(
                    *value,
                    EffectContinuation::IndexValue {
                        index: *index,
                        span: expression.span,
                        next: Box::new(normal),
                    },
                    returned,
                )
            }
            uhura_syntax::ast::ExpressionKind::Is { value, pattern } => {
                self.reject_nested_calls(&value, "a pattern-test operand");
                self.lower_expression(
                    *value,
                    EffectContinuation::Is {
                        pattern,
                        span: expression.span,
                        next: Box::new(normal),
                    },
                    returned,
                )
            }
            uhura_syntax::ast::ExpressionKind::Sequence(values) => {
                self.lower_sequence(values, false, normal, returned, expression.span)
            }
            uhura_syntax::ast::ExpressionKind::Tuple(values) => {
                self.lower_sequence(values, true, normal, returned, expression.span)
            }
            uhura_syntax::ast::ExpressionKind::Record(record) => {
                for value in record
                    .fields
                    .iter()
                    .filter_map(|field| field.value.as_ref())
                {
                    self.reject_nested_calls(value, "a record field");
                }
                if let Some(base) = &record.base {
                    self.reject_nested_calls(base, "a record-update base");
                }
                self.lower_record(record, 0, normal, returned, expression.span)
            }
            uhura_syntax::ast::ExpressionKind::AnonymousRecord(entries) => self.apply_continuation(
                uhura_syntax::ast::Node::new(
                    uhura_syntax::ast::ExpressionKind::AnonymousRecord(entries),
                    expression.span,
                ),
                normal,
                returned,
            ),
            uhura_syntax::ast::ExpressionKind::Call { callee, arguments } => {
                let binder_returns = arguments.iter().any(|argument| {
                    matches!(argument, uhura_syntax::ast::CallArgument::Binder(value) if expression_has_return(&value.body))
                });
                if binder_returns {
                    diagnostic(
                        self.diagnostics,
                        codes::EFFECT,
                        "uhura-0.4/return-in-binder",
                        "a non-escaping collection binder cannot use lexical `return`",
                        expression.span,
                    );
                    return self.apply_continuation(
                        uhura_syntax::ast::Node::new(
                            uhura_syntax::ast::ExpressionKind::Call { callee, arguments },
                            expression.span,
                        ),
                        normal,
                        returned,
                    );
                }
                self.reject_nested_calls(&callee, "a call target");
                for argument in &arguments {
                    if let uhura_syntax::ast::CallArgument::Expression(value) = argument {
                        self.reject_nested_calls(value, "a call argument");
                    }
                }
                if expression_has_return(&callee) {
                    self.lower_expression(
                        *callee,
                        EffectContinuation::CallCallee {
                            arguments,
                            span: expression.span,
                            next: Box::new(normal),
                        },
                        returned,
                    )
                } else {
                    self.lower_call_arguments(
                        *callee,
                        arguments,
                        0,
                        normal,
                        returned,
                        expression.span,
                    )
                }
            }
            uhura_syntax::ast::ExpressionKind::If(value) => {
                if expression_has_return(&value.condition) {
                    let condition = *value.condition.clone();
                    return self.lower_expression(
                        condition,
                        EffectContinuation::IfCondition {
                            expression: value,
                            span: expression.span,
                            next: Box::new(normal),
                        },
                        returned,
                    );
                }
                self.reject_nested_calls(&value.condition, "an `if` condition");
                let then_branch =
                    self.lower_block(value.then_branch, normal.clone(), returned.clone());
                let else_branch = if let Some(branch) = value.else_branch {
                    match branch {
                        uhura_syntax::ast::ElseBranch::Block(block) => {
                            self.lower_block(block, normal, returned.clone())
                        }
                        uhura_syntax::ast::ElseBranch::If(value) => {
                            self.lower_expression(*value, normal, returned.clone())
                        }
                    }
                } else {
                    self.apply_continuation(
                        uhura_syntax::ast::Node::new(
                            uhura_syntax::ast::ExpressionKind::Unit,
                            expression.span,
                        ),
                        normal,
                        returned.clone(),
                    )
                };
                block_with_tail(uhura_syntax::ast::Node::new(
                    uhura_syntax::ast::ExpressionKind::If(uhura_syntax::ast::IfExpression {
                        condition: value.condition,
                        then_branch,
                        else_branch: Some(uhura_syntax::ast::ElseBranch::Block(else_branch)),
                    }),
                    expression.span,
                ))
            }
            uhura_syntax::ast::ExpressionKind::Match(value) => {
                if expression_has_return(&value.value) {
                    let subject = *value.value.clone();
                    return self.lower_expression(
                        subject,
                        EffectContinuation::MatchSubject {
                            expression: value,
                            span: expression.span,
                            next: Box::new(normal),
                        },
                        returned,
                    );
                }
                self.reject_nested_calls(&value.value, "a match subject");
                let arms = value
                    .arms
                    .into_iter()
                    .map(|arm| {
                        let span = arm.value.span;
                        let body =
                            self.lower_expression(arm.value, normal.clone(), returned.clone());
                        uhura_syntax::ast::MatchArm {
                            pattern: arm.pattern,
                            value: uhura_syntax::ast::Node::new(
                                uhura_syntax::ast::ExpressionKind::Block(body),
                                span,
                            ),
                            span: arm.span,
                        }
                    })
                    .collect();
                block_with_tail(uhura_syntax::ast::Node::new(
                    uhura_syntax::ast::ExpressionKind::Match(uhura_syntax::ast::MatchExpression {
                        value: value.value,
                        arms,
                    }),
                    expression.span,
                ))
            }
            uhura_syntax::ast::ExpressionKind::Literal(_)
            | uhura_syntax::ast::ExpressionKind::Unit
            | uhura_syntax::ast::ExpressionKind::Name(_) => {
                unreachable!("return-free leaves were handled before CPS decomposition")
            }
        }
    }

    fn lower_call_arguments(
        &mut self,
        callee: uhura_syntax::ast::Expression,
        arguments: Vec<uhura_syntax::ast::CallArgument>,
        start: usize,
        next: EffectContinuation,
        returned: EffectContinuation,
        span: uhura_syntax::ast::Span,
    ) -> uhura_syntax::ast::Block {
        if let Some((index, value)) =
            arguments
                .iter()
                .enumerate()
                .skip(start)
                .find_map(|(index, argument)| match argument {
                    uhura_syntax::ast::CallArgument::Expression(value)
                        if expression_has_return(value) =>
                    {
                        Some((index, value.clone()))
                    }
                    _ => None,
                })
        {
            return self.lower_expression(
                value,
                EffectContinuation::CallArgument {
                    callee,
                    arguments,
                    index,
                    span,
                    next: Box::new(next),
                },
                returned,
            );
        }
        self.lower_expression(
            uhura_syntax::ast::Node::new(
                uhura_syntax::ast::ExpressionKind::Call {
                    callee: Box::new(callee),
                    arguments,
                },
                span,
            ),
            next,
            returned,
        )
    }

    fn lower_sequence(
        &mut self,
        mut values: Vec<uhura_syntax::ast::Expression>,
        tuple: bool,
        next: EffectContinuation,
        returned: EffectContinuation,
        span: uhura_syntax::ast::Span,
    ) -> uhura_syntax::ast::Block {
        for value in &values {
            self.reject_nested_calls(value, "a collection element");
        }
        let first = values.remove(0);
        self.lower_expression(
            first,
            EffectContinuation::Sequence {
                tuple,
                completed: Vec::new(),
                remaining: values,
                span,
                next: Box::new(next),
            },
            returned,
        )
    }

    fn lower_inline_call_arguments(
        &mut self,
        target: String,
        arguments: Vec<uhura_syntax::ast::Expression>,
        start: usize,
        next: EffectContinuation,
        returned: EffectContinuation,
        span: uhura_syntax::ast::Span,
    ) -> uhura_syntax::ast::Block {
        if let Some((index, value)) = arguments
            .iter()
            .enumerate()
            .skip(start)
            .find(|(_, value)| expression_has_return(value))
            .map(|(index, value)| (index, value.clone()))
        {
            return self.lower_expression(
                value,
                EffectContinuation::InlineCallArgument {
                    target,
                    arguments,
                    index,
                    span,
                    next: Box::new(next),
                },
                returned,
            );
        }
        self.inline_call(&target, arguments, next, returned, span)
    }

    fn lower_record(
        &mut self,
        record: uhura_syntax::ast::RecordExpression,
        start: usize,
        next: EffectContinuation,
        returned: EffectContinuation,
        span: uhura_syntax::ast::Span,
    ) -> uhura_syntax::ast::Block {
        if let Some((index, value)) =
            record
                .fields
                .iter()
                .enumerate()
                .skip(start)
                .find_map(|(index, field)| {
                    field
                        .value
                        .as_ref()
                        .filter(|value| expression_has_return(value))
                        .cloned()
                        .map(|value| (index, value))
                })
        {
            return self.lower_expression(
                value,
                EffectContinuation::RecordField {
                    record,
                    index,
                    span,
                    next: Box::new(next),
                },
                returned,
            );
        }
        if let Some(base) = record.base.as_deref().cloned()
            && expression_has_return(&base)
        {
            return self.lower_expression(
                base,
                EffectContinuation::RecordBase {
                    record,
                    span,
                    next: Box::new(next),
                },
                returned,
            );
        }
        self.lower_expression(
            uhura_syntax::ast::Node::new(uhura_syntax::ast::ExpressionKind::Record(record), span),
            next,
            returned,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_emit_arguments(
        &mut self,
        output: uhura_syntax::ast::OutputConstructor,
        start: usize,
        semicolon: uhura_syntax::ast::Span,
        remainder: uhura_syntax::ast::Block,
        normal: EffectContinuation,
        returned: EffectContinuation,
        statement_span: uhura_syntax::ast::Span,
        block_span: uhura_syntax::ast::Span,
    ) -> uhura_syntax::ast::Block {
        if let Some((index, value)) = output
            .arguments
            .iter()
            .enumerate()
            .skip(start)
            .find(|(_, value)| expression_has_return(value))
            .map(|(index, value)| (index, value.clone()))
        {
            return self.lower_expression(
                value,
                EffectContinuation::EmitArgument {
                    output,
                    index,
                    semicolon,
                    remainder,
                    normal: Box::new(normal),
                    statement_span,
                    block_span,
                },
                returned,
            );
        }
        let rest = self.lower_block(remainder, normal, returned);
        prepend_statement(
            uhura_syntax::ast::Node::new(
                uhura_syntax::ast::StatementKind::Emit { output, semicolon },
                statement_span,
            ),
            rest,
            block_span,
        )
    }

    fn inline_call(
        &mut self,
        target: &str,
        arguments: Vec<uhura_syntax::ast::Expression>,
        next: EffectContinuation,
        returned: EffectContinuation,
        call_span: uhura_syntax::ast::Span,
    ) -> uhura_syntax::ast::Block {
        let Some(update) = self.updates.get(target).cloned() else {
            return self.apply_continuation(
                call_expression(target, arguments, call_span),
                next,
                returned,
            );
        };
        if self.stack.iter().any(|name| name == target) {
            diagnostic(
                self.diagnostics,
                codes::DEPENDENCY_CYCLE,
                "uhura-0.4/update-cycle",
                format!("update `{target}` participates in a call cycle"),
                call_span,
            );
            return block_with_tail(uhura_syntax::ast::Node::new(
                uhura_syntax::ast::ExpressionKind::Unit,
                call_span,
            ));
        }
        if arguments.len() != update.declaration.parameters.len() {
            diagnostic(
                self.diagnostics,
                codes::ARITY,
                "uhura-0.4/update-argument-arity",
                format!(
                    "update `{target}` expects {} arguments, got {}",
                    update.declaration.parameters.len(),
                    arguments.len()
                ),
                call_span,
            );
        }

        let prefix = format!(
            "__uhura_update_{}_{}_",
            target.replace('.', "_"),
            self.temporary
        );
        self.temporary += 1;
        let mut renames = BTreeMap::new();
        let mut prefix_statements = Vec::new();
        for (index, parameter) in update.declaration.parameters.iter().enumerate() {
            let name = format!("{prefix}arg_{}", parameter.name.text);
            renames.insert(parameter.name.text.clone(), name.clone());
            let argument = arguments.get(index).cloned().unwrap_or_else(|| {
                uhura_syntax::ast::Node::new(uhura_syntax::ast::ExpressionKind::Unit, call_span)
            });
            self.reject_nested_calls(&argument, "an update argument");
            prefix_statements.push(uhura_syntax::ast::Node::new(
                uhura_syntax::ast::StatementKind::Let {
                    name: uhura_syntax::ast::Identifier::new(name, parameter.name.span),
                    ty: Some(parameter.ty.clone()),
                    value: argument,
                    semicolon: parameter.name.span,
                },
                parameter.span,
            ));
        }
        let mut body = update.declaration.body;
        alpha_rename_block(&mut body, &mut renames, &prefix);

        self.stack.push(target.to_owned());
        let result_name = format!("{INLINE_UPDATE_JOIN_LOCAL_PREFIX}{}", self.temporary);
        self.temporary += 1;
        let result_continuation = EffectContinuation::CaptureUpdateResult {
            name: result_name.clone(),
            ty: update.result,
            span: update.span,
        };
        let lowered = self.lower_block(body, result_continuation.clone(), result_continuation);
        self.stack.pop();
        let continuation =
            self.apply_continuation(name_expression(result_name, call_span), next, returned);
        let lowered = append_blocks(lowered, continuation, call_span);
        prepend_statements(prefix_statements, lowered, call_span)
    }

    fn apply_continuation(
        &mut self,
        value: uhura_syntax::ast::Expression,
        continuation: EffectContinuation,
        returned: EffectContinuation,
    ) -> uhura_syntax::ast::Block {
        match continuation {
            EffectContinuation::Tail => block_with_tail(value),
            EffectContinuation::Return => block_with_tail(uhura_syntax::ast::Node::new(
                uhura_syntax::ast::ExpressionKind::Return(Some(Box::new(value.clone()))),
                value.span,
            )),
            EffectContinuation::BindThen {
                name,
                ty,
                remainder,
                normal,
                span,
            } => {
                let rest = self.lower_block(remainder, *normal, returned);
                prepend_statement(
                    uhura_syntax::ast::Node::new(
                        uhura_syntax::ast::StatementKind::Let {
                            name,
                            ty,
                            value,
                            semicolon: span,
                        },
                        span,
                    ),
                    rest,
                    span,
                )
            }
            EffectContinuation::DiscardThen {
                remainder,
                normal,
                span,
            } => {
                let rest = self.lower_block(remainder, *normal, returned);
                if expression_is_reaction_statement(&value) {
                    prepend_statement(
                        uhura_syntax::ast::Node::new(
                            uhura_syntax::ast::StatementKind::BlockExpression(value),
                            span,
                        ),
                        rest,
                        span,
                    )
                } else {
                    let name = format!("__uhura_update_discard_{}", self.temporary);
                    self.temporary += 1;
                    prepend_statement(
                        uhura_syntax::ast::Node::new(
                            uhura_syntax::ast::StatementKind::Let {
                                name: uhura_syntax::ast::Identifier::new(name, span),
                                ty: Some(unit_type(span)),
                                value,
                                semicolon: span,
                            },
                            span,
                        ),
                        rest,
                        span,
                    )
                }
            }
            EffectContinuation::CaptureUpdateResult { name, ty, span } => {
                let identifier = uhura_syntax::ast::Identifier::new(name.clone(), span);
                let checked = uhura_syntax::ast::Node::new(
                    uhura_syntax::ast::StatementKind::Let {
                        name: identifier,
                        ty: Some(ty),
                        value,
                        semicolon: span,
                    },
                    span,
                );
                uhura_syntax::ast::Block {
                    statements: vec![checked],
                    tail: None,
                    span,
                }
            }
            EffectContinuation::CaptureUpdateLoopExit { name, ty, span } => {
                let checked = uhura_syntax::ast::Node::new(
                    uhura_syntax::ast::StatementKind::Let {
                        name: uhura_syntax::ast::Identifier::new(name, span),
                        ty: Some(option_type(ty, span)),
                        value: call_expression("Some", vec![value], span),
                        semicolon: span,
                    },
                    span,
                );
                uhura_syntax::ast::Block {
                    statements: vec![checked],
                    tail: None,
                    span,
                }
            }
            EffectContinuation::Unary {
                operator,
                span,
                next,
            } => self.lower_expression(
                uhura_syntax::ast::Node::new(
                    uhura_syntax::ast::ExpressionKind::Unary {
                        operator,
                        value: Box::new(value),
                    },
                    span,
                ),
                *next,
                returned,
            ),
            EffectContinuation::BinaryLeft {
                operator,
                right,
                span,
                next,
            } => self.lower_expression(
                right,
                EffectContinuation::BinaryRight {
                    operator,
                    left: value,
                    span,
                    next,
                },
                returned,
            ),
            EffectContinuation::BinaryRight {
                operator,
                left,
                span,
                next,
            } => self.lower_expression(
                uhura_syntax::ast::Node::new(
                    uhura_syntax::ast::ExpressionKind::Binary {
                        operator,
                        left: Box::new(left),
                        right: Box::new(value),
                    },
                    span,
                ),
                *next,
                returned,
            ),
            EffectContinuation::ShortCircuitLeft {
                operator,
                right,
                span,
                next,
            } => {
                let right_branch = self.lower_expression(right, (*next).clone(), returned.clone());
                let short_value = uhura_syntax::ast::Node::new(
                    uhura_syntax::ast::ExpressionKind::Literal(uhura_syntax::ast::Literal::Bool(
                        matches!(operator, uhura_syntax::ast::BinaryOperator::Or),
                    )),
                    span,
                );
                let short_branch = self.apply_continuation(short_value, *next, returned);
                let (then_branch, else_branch) = match operator {
                    uhura_syntax::ast::BinaryOperator::And => (right_branch, short_branch),
                    uhura_syntax::ast::BinaryOperator::Or => (short_branch, right_branch),
                    _ => unreachable!("only boolean short-circuit operators use this frame"),
                };
                block_with_tail(uhura_syntax::ast::Node::new(
                    uhura_syntax::ast::ExpressionKind::If(uhura_syntax::ast::IfExpression {
                        condition: Box::new(value),
                        then_branch,
                        else_branch: Some(uhura_syntax::ast::ElseBranch::Block(else_branch)),
                    }),
                    span,
                ))
            }
            EffectContinuation::CompareLeft {
                operator,
                right,
                span,
                next,
            } => self.lower_expression(
                right,
                EffectContinuation::CompareRight {
                    operator,
                    left: value,
                    span,
                    next,
                },
                returned,
            ),
            EffectContinuation::CompareRight {
                operator,
                left,
                span,
                next,
            } => self.lower_expression(
                uhura_syntax::ast::Node::new(
                    uhura_syntax::ast::ExpressionKind::Compare {
                        operator,
                        left: Box::new(left),
                        right: Box::new(value),
                    },
                    span,
                ),
                *next,
                returned,
            ),
            EffectContinuation::Member { member, span, next } => self.lower_expression(
                uhura_syntax::ast::Node::new(
                    uhura_syntax::ast::ExpressionKind::Member {
                        value: Box::new(value),
                        member,
                    },
                    span,
                ),
                *next,
                returned,
            ),
            EffectContinuation::IndexValue { index, span, next } => self.lower_expression(
                index,
                EffectContinuation::IndexIndex { value, span, next },
                returned,
            ),
            EffectContinuation::IndexIndex {
                value: receiver,
                span,
                next,
            } => self.lower_expression(
                uhura_syntax::ast::Node::new(
                    uhura_syntax::ast::ExpressionKind::Index {
                        value: Box::new(receiver),
                        index: Box::new(value),
                    },
                    span,
                ),
                *next,
                returned,
            ),
            EffectContinuation::Is {
                pattern,
                span,
                next,
            } => self.lower_expression(
                uhura_syntax::ast::Node::new(
                    uhura_syntax::ast::ExpressionKind::Is {
                        value: Box::new(value),
                        pattern,
                    },
                    span,
                ),
                *next,
                returned,
            ),
            EffectContinuation::Sequence {
                tuple,
                mut completed,
                mut remaining,
                span,
                next,
            } => {
                completed.push(value);
                if remaining.is_empty() {
                    self.lower_expression(
                        uhura_syntax::ast::Node::new(
                            if tuple {
                                uhura_syntax::ast::ExpressionKind::Tuple(completed)
                            } else {
                                uhura_syntax::ast::ExpressionKind::Sequence(completed)
                            },
                            span,
                        ),
                        *next,
                        returned,
                    )
                } else {
                    let first = remaining.remove(0);
                    self.lower_expression(
                        first,
                        EffectContinuation::Sequence {
                            tuple,
                            completed,
                            remaining,
                            span,
                            next,
                        },
                        returned,
                    )
                }
            }
            EffectContinuation::IfCondition {
                mut expression,
                span,
                next,
            } => {
                expression.condition = Box::new(value);
                self.lower_expression(
                    uhura_syntax::ast::Node::new(
                        uhura_syntax::ast::ExpressionKind::If(expression),
                        span,
                    ),
                    *next,
                    returned,
                )
            }
            EffectContinuation::MatchSubject {
                mut expression,
                span,
                next,
            } => {
                expression.value = Box::new(value);
                self.lower_expression(
                    uhura_syntax::ast::Node::new(
                        uhura_syntax::ast::ExpressionKind::Match(expression),
                        span,
                    ),
                    *next,
                    returned,
                )
            }
            EffectContinuation::CallCallee {
                arguments,
                span,
                next,
            } => self.lower_call_arguments(value, arguments, 0, *next, returned, span),
            EffectContinuation::CallArgument {
                callee,
                mut arguments,
                index,
                span,
                next,
            } => {
                arguments[index] = uhura_syntax::ast::CallArgument::Expression(value);
                self.lower_call_arguments(callee, arguments, index + 1, *next, returned, span)
            }
            EffectContinuation::InlineCallArgument {
                target,
                mut arguments,
                index,
                span,
                next,
            } => {
                arguments[index] = value;
                self.lower_inline_call_arguments(
                    target,
                    arguments,
                    index + 1,
                    *next,
                    returned,
                    span,
                )
            }
            EffectContinuation::RecordField {
                mut record,
                index,
                span,
                next,
            } => {
                record.fields[index].value = Some(value);
                self.lower_record(record, index + 1, *next, returned, span)
            }
            EffectContinuation::RecordBase {
                mut record,
                span,
                next,
            } => {
                record.base = Some(Box::new(value));
                let field_count = record.fields.len();
                self.lower_record(record, field_count, *next, returned, span)
            }
            EffectContinuation::AssignValue {
                target,
                semicolon,
                remainder,
                normal,
                statement_span,
                block_span,
            } => {
                let rest = self.lower_block(remainder, *normal, returned);
                prepend_statement(
                    uhura_syntax::ast::Node::new(
                        uhura_syntax::ast::StatementKind::Assign {
                            target,
                            value,
                            semicolon,
                        },
                        statement_span,
                    ),
                    rest,
                    block_span,
                )
            }
            EffectContinuation::EmitArgument {
                mut output,
                index,
                semicolon,
                remainder,
                normal,
                statement_span,
                block_span,
            } => {
                output.arguments[index] = value;
                self.lower_emit_arguments(
                    output,
                    index + 1,
                    semicolon,
                    remainder,
                    *normal,
                    returned,
                    statement_span,
                    block_span,
                )
            }
            EffectContinuation::WhileCondition {
                decreases,
                body,
                remainder,
                normal,
                statement_span,
                block_span,
            } => self.lower_expression(
                decreases,
                EffectContinuation::WhileMeasure {
                    condition: value,
                    body,
                    remainder,
                    normal,
                    statement_span,
                    block_span,
                },
                returned,
            ),
            EffectContinuation::WhileMeasure {
                condition,
                body,
                remainder,
                normal,
                statement_span,
                block_span,
            } => {
                let loop_result = match &returned {
                    EffectContinuation::CaptureUpdateResult { ty, span, .. }
                    | EffectContinuation::CaptureUpdateLoopExit { ty, span, .. } => {
                        Some((ty.clone(), *span))
                    }
                    _ => None,
                };
                if block_has_return(&body)
                    && let Some((result_ty, result_span)) = loop_result
                {
                    let ordinal = self.temporary;
                    self.temporary += 1;
                    let loop_exit_name = format!("{INLINE_UPDATE_LOOP_EXIT_LOCAL_PREFIX}{ordinal}");
                    let loop_value_name = format!("__uhura_update_loop_value_{ordinal}");
                    let lowered_body = self.lower_block(
                        body,
                        EffectContinuation::Tail,
                        EffectContinuation::CaptureUpdateLoopExit {
                            name: loop_exit_name.clone(),
                            ty: result_ty.clone(),
                            span: result_span,
                        },
                    );
                    let rest = self.lower_block(remainder, *normal, returned.clone());
                    let selected = self.apply_continuation(
                        name_expression(loop_value_name.clone(), result_span),
                        returned,
                        EffectContinuation::Return,
                    );
                    let select = match_expression(
                        name_expression(loop_exit_name.clone(), result_span),
                        vec![
                            (
                                some_pattern(loop_value_name, result_span),
                                block_expression(selected, result_span),
                            ),
                            (
                                none_pattern(result_span),
                                block_expression(rest, result_span),
                            ),
                        ],
                        result_span,
                    );
                    let after = uhura_syntax::ast::Block {
                        statements: vec![uhura_syntax::ast::Node::new(
                            uhura_syntax::ast::StatementKind::BlockExpression(select),
                            result_span,
                        )],
                        tail: None,
                        span: block_span,
                    };
                    let with_loop = prepend_statement(
                        uhura_syntax::ast::Node::new(
                            uhura_syntax::ast::StatementKind::While {
                                condition,
                                decreases: value,
                                body: lowered_body,
                            },
                            statement_span,
                        ),
                        after,
                        block_span,
                    );
                    return prepend_statement(
                        uhura_syntax::ast::Node::new(
                            uhura_syntax::ast::StatementKind::Let {
                                name: uhura_syntax::ast::Identifier::new(
                                    loop_exit_name,
                                    result_span,
                                ),
                                ty: Some(option_type(result_ty, result_span)),
                                value: name_expression("None".into(), result_span),
                                semicolon: result_span,
                            },
                            result_span,
                        ),
                        with_loop,
                        block_span,
                    );
                }
                let body =
                    self.lower_block(body, EffectContinuation::Tail, EffectContinuation::Return);
                let rest = self.lower_block(remainder, *normal, returned);
                prepend_statement(
                    uhura_syntax::ast::Node::new(
                        uhura_syntax::ast::StatementKind::While {
                            condition,
                            decreases: value,
                            body,
                        },
                        statement_span,
                    ),
                    rest,
                    block_span,
                )
            }
        }
    }

    fn reject_nested_calls(&mut self, expression: &uhura_syntax::ast::Expression, context: &str) {
        if let Some((name, span)) = first_inline_call(expression, &self.updates) {
            diagnostic(
                self.diagnostics,
                codes::EFFECT,
                "uhura-0.4/nested-effectful-update",
                format!(
                    "effectful update `{name}` cannot be used inside {context}; bind the complete call in a sequential position"
                ),
                span,
            );
        }
    }
}

fn update_returns_outcome(update: &uhura_syntax::ast::UpdateDeclaration) -> bool {
    update
        .result
        .as_ref()
        .is_some_and(|result| type_mentions(result, "Outcome"))
}

fn type_mentions(ty: &uhura_syntax::ast::TypeExpression, name: &str) -> bool {
    match &ty.kind {
        uhura_syntax::ast::TypeExpressionKind::Path(path) => path.segments.iter().any(|segment| {
            segment.name.text == name
                || segment
                    .arguments
                    .iter()
                    .any(|argument| type_mentions(argument, name))
        }),
        uhura_syntax::ast::TypeExpressionKind::Tuple(values) => {
            values.iter().any(|value| type_mentions(value, name))
        }
        uhura_syntax::ast::TypeExpressionKind::Unit => false,
    }
}

fn is_unit_type(ty: &uhura_syntax::ast::TypeExpression) -> bool {
    matches!(ty.kind, uhura_syntax::ast::TypeExpressionKind::Unit)
}

fn unit_type(span: uhura_syntax::ast::Span) -> uhura_syntax::ast::TypeExpression {
    uhura_syntax::ast::Node::new(uhura_syntax::ast::TypeExpressionKind::Unit, span)
}

fn direct_inline_call(
    expression: &uhura_syntax::ast::Expression,
    updates: &BTreeMap<String, InlineUpdate>,
) -> Option<(String, Vec<uhura_syntax::ast::Expression>)> {
    let expression = match &expression.kind {
        uhura_syntax::ast::ExpressionKind::Group(value) => value.as_ref(),
        _ => expression,
    };
    let uhura_syntax::ast::ExpressionKind::Call { callee, arguments } = &expression.kind else {
        return None;
    };
    let uhura_syntax::ast::ExpressionKind::Name(name) = &callee.kind else {
        return None;
    };
    if name.segments.len() != 1 || !updates.contains_key(&name.segments[0].text) {
        return None;
    }
    let values = arguments
        .iter()
        .filter_map(|argument| match argument {
            uhura_syntax::ast::CallArgument::Expression(value) => Some(value.clone()),
            uhura_syntax::ast::CallArgument::Binder(_) => None,
        })
        .collect::<Vec<_>>();
    Some((name.segments[0].text.clone(), values))
}

fn first_inline_call(
    expression: &uhura_syntax::ast::Expression,
    updates: &BTreeMap<String, InlineUpdate>,
) -> Option<(String, uhura_syntax::ast::Span)> {
    if let Some((name, _)) = direct_inline_call(expression, updates) {
        return Some((name, expression.span));
    }
    match &expression.kind {
        uhura_syntax::ast::ExpressionKind::Literal(_)
        | uhura_syntax::ast::ExpressionKind::Unit
        | uhura_syntax::ast::ExpressionKind::Name(_)
        | uhura_syntax::ast::ExpressionKind::Return(None) => None,
        uhura_syntax::ast::ExpressionKind::Sequence(values)
        | uhura_syntax::ast::ExpressionKind::Tuple(values) => values
            .iter()
            .find_map(|value| first_inline_call(value, updates)),
        uhura_syntax::ast::ExpressionKind::Group(value)
        | uhura_syntax::ast::ExpressionKind::Unary { value, .. }
        | uhura_syntax::ast::ExpressionKind::Member { value, .. }
        | uhura_syntax::ast::ExpressionKind::Is { value, .. } => first_inline_call(value, updates),
        uhura_syntax::ast::ExpressionKind::Record(value) => value
            .fields
            .iter()
            .filter_map(|field| field.value.as_ref())
            .find_map(|value| first_inline_call(value, updates))
            .or_else(|| {
                value
                    .base
                    .as_deref()
                    .and_then(|value| first_inline_call(value, updates))
            }),
        uhura_syntax::ast::ExpressionKind::AnonymousRecord(entries) => {
            entries.iter().find_map(|entry| {
                first_inline_call(&entry.key, updates)
                    .or_else(|| first_inline_call(&entry.value, updates))
            })
        }
        uhura_syntax::ast::ExpressionKind::Block(block) => {
            first_inline_call_in_block(block, updates)
        }
        uhura_syntax::ast::ExpressionKind::Call { callee, arguments } => {
            first_inline_call(callee, updates).or_else(|| {
                arguments.iter().find_map(|argument| match argument {
                    uhura_syntax::ast::CallArgument::Expression(value) => {
                        first_inline_call(value, updates)
                    }
                    uhura_syntax::ast::CallArgument::Binder(value) => {
                        first_inline_call(&value.body, updates)
                    }
                })
            })
        }
        uhura_syntax::ast::ExpressionKind::Index { value, index }
        | uhura_syntax::ast::ExpressionKind::Binary {
            left: value,
            right: index,
            ..
        }
        | uhura_syntax::ast::ExpressionKind::Compare {
            left: value,
            right: index,
            ..
        } => first_inline_call(value, updates).or_else(|| first_inline_call(index, updates)),
        uhura_syntax::ast::ExpressionKind::If(value) => {
            first_inline_call(&value.condition, updates)
                .or_else(|| first_inline_call_in_block(&value.then_branch, updates))
                .or_else(|| {
                    value.else_branch.as_ref().and_then(|branch| match branch {
                        uhura_syntax::ast::ElseBranch::Block(block) => {
                            first_inline_call_in_block(block, updates)
                        }
                        uhura_syntax::ast::ElseBranch::If(value) => {
                            first_inline_call(value, updates)
                        }
                    })
                })
        }
        uhura_syntax::ast::ExpressionKind::Match(value) => first_inline_call(&value.value, updates)
            .or_else(|| {
                value
                    .arms
                    .iter()
                    .find_map(|arm| first_inline_call(&arm.value, updates))
            }),
        uhura_syntax::ast::ExpressionKind::Return(Some(value)) => first_inline_call(value, updates),
    }
}

fn first_inline_call_in_block(
    block: &uhura_syntax::ast::Block,
    updates: &BTreeMap<String, InlineUpdate>,
) -> Option<(String, uhura_syntax::ast::Span)> {
    for statement in &block.statements {
        let found = match &statement.kind {
            uhura_syntax::ast::StatementKind::Let { value, .. }
            | uhura_syntax::ast::StatementKind::Assign { value, .. } => {
                first_inline_call(value, updates)
            }
            uhura_syntax::ast::StatementKind::Emit { output, .. } => output
                .arguments
                .iter()
                .find_map(|value| first_inline_call(value, updates)),
            uhura_syntax::ast::StatementKind::While {
                condition,
                decreases,
                body,
            } => first_inline_call(condition, updates)
                .or_else(|| first_inline_call(decreases, updates))
                .or_else(|| first_inline_call_in_block(body, updates)),
            uhura_syntax::ast::StatementKind::Expression { expression, .. }
            | uhura_syntax::ast::StatementKind::BlockExpression(expression) => {
                first_inline_call(expression, updates)
            }
            uhura_syntax::ast::StatementKind::Unreachable { .. } => None,
        };
        if found.is_some() {
            return found;
        }
    }
    block
        .tail
        .as_deref()
        .and_then(|value| first_inline_call(value, updates))
}

fn expression_has_return(expression: &uhura_syntax::ast::Expression) -> bool {
    match &expression.kind {
        uhura_syntax::ast::ExpressionKind::Return(_) => true,
        uhura_syntax::ast::ExpressionKind::Sequence(values)
        | uhura_syntax::ast::ExpressionKind::Tuple(values) => {
            values.iter().any(expression_has_return)
        }
        uhura_syntax::ast::ExpressionKind::Group(value)
        | uhura_syntax::ast::ExpressionKind::Unary { value, .. }
        | uhura_syntax::ast::ExpressionKind::Member { value, .. }
        | uhura_syntax::ast::ExpressionKind::Is { value, .. } => expression_has_return(value),
        uhura_syntax::ast::ExpressionKind::Record(value) => {
            value
                .fields
                .iter()
                .filter_map(|field| field.value.as_ref())
                .any(expression_has_return)
                || value.base.as_deref().is_some_and(expression_has_return)
        }
        uhura_syntax::ast::ExpressionKind::AnonymousRecord(entries) => entries
            .iter()
            .any(|entry| expression_has_return(&entry.key) || expression_has_return(&entry.value)),
        uhura_syntax::ast::ExpressionKind::Block(block) => block_has_return(block),
        uhura_syntax::ast::ExpressionKind::Call { callee, arguments } => {
            expression_has_return(callee)
                || arguments.iter().any(|argument| match argument {
                    uhura_syntax::ast::CallArgument::Expression(value) => {
                        expression_has_return(value)
                    }
                    uhura_syntax::ast::CallArgument::Binder(value) => {
                        expression_has_return(&value.body)
                    }
                })
        }
        uhura_syntax::ast::ExpressionKind::Index { value, index }
        | uhura_syntax::ast::ExpressionKind::Binary {
            left: value,
            right: index,
            ..
        }
        | uhura_syntax::ast::ExpressionKind::Compare {
            left: value,
            right: index,
            ..
        } => expression_has_return(value) || expression_has_return(index),
        uhura_syntax::ast::ExpressionKind::If(value) => {
            expression_has_return(&value.condition)
                || block_has_return(&value.then_branch)
                || value
                    .else_branch
                    .as_ref()
                    .is_some_and(|branch| match branch {
                        uhura_syntax::ast::ElseBranch::Block(block) => block_has_return(block),
                        uhura_syntax::ast::ElseBranch::If(value) => expression_has_return(value),
                    })
        }
        uhura_syntax::ast::ExpressionKind::Match(value) => {
            expression_has_return(&value.value)
                || value
                    .arms
                    .iter()
                    .any(|arm| expression_has_return(&arm.value))
        }
        uhura_syntax::ast::ExpressionKind::Literal(_)
        | uhura_syntax::ast::ExpressionKind::Unit
        | uhura_syntax::ast::ExpressionKind::Name(_) => false,
    }
}

fn expression_has_reaction_control(expression: &uhura_syntax::ast::Expression) -> bool {
    match &expression.kind {
        uhura_syntax::ast::ExpressionKind::Return(_) => true,
        uhura_syntax::ast::ExpressionKind::Sequence(values)
        | uhura_syntax::ast::ExpressionKind::Tuple(values) => {
            values.iter().any(expression_has_reaction_control)
        }
        uhura_syntax::ast::ExpressionKind::Group(value)
        | uhura_syntax::ast::ExpressionKind::Unary { value, .. }
        | uhura_syntax::ast::ExpressionKind::Member { value, .. }
        | uhura_syntax::ast::ExpressionKind::Is { value, .. } => {
            expression_has_reaction_control(value)
        }
        uhura_syntax::ast::ExpressionKind::Record(value) => {
            value
                .fields
                .iter()
                .filter_map(|field| field.value.as_ref())
                .any(expression_has_reaction_control)
                || value
                    .base
                    .as_deref()
                    .is_some_and(expression_has_reaction_control)
        }
        uhura_syntax::ast::ExpressionKind::AnonymousRecord(entries) => {
            entries.iter().any(|entry| {
                expression_has_reaction_control(&entry.key)
                    || expression_has_reaction_control(&entry.value)
            })
        }
        uhura_syntax::ast::ExpressionKind::Block(block) => block_has_reaction_control(block),
        uhura_syntax::ast::ExpressionKind::Call { callee, arguments } => {
            expression_has_reaction_control(callee)
                || arguments.iter().any(|argument| match argument {
                    uhura_syntax::ast::CallArgument::Expression(value) => {
                        expression_has_reaction_control(value)
                    }
                    uhura_syntax::ast::CallArgument::Binder(value) => {
                        expression_has_reaction_control(&value.body)
                    }
                })
        }
        uhura_syntax::ast::ExpressionKind::Index { value, index }
        | uhura_syntax::ast::ExpressionKind::Binary {
            left: value,
            right: index,
            ..
        }
        | uhura_syntax::ast::ExpressionKind::Compare {
            left: value,
            right: index,
            ..
        } => expression_has_reaction_control(value) || expression_has_reaction_control(index),
        uhura_syntax::ast::ExpressionKind::If(value) => {
            expression_has_reaction_control(&value.condition)
                || block_has_reaction_control(&value.then_branch)
                || value
                    .else_branch
                    .as_ref()
                    .is_some_and(|branch| match branch {
                        uhura_syntax::ast::ElseBranch::Block(block) => {
                            block_has_reaction_control(block)
                        }
                        uhura_syntax::ast::ElseBranch::If(value) => {
                            expression_has_reaction_control(value)
                        }
                    })
        }
        uhura_syntax::ast::ExpressionKind::Match(value) => {
            expression_has_reaction_control(&value.value)
                || value
                    .arms
                    .iter()
                    .any(|arm| expression_has_reaction_control(&arm.value))
        }
        uhura_syntax::ast::ExpressionKind::Literal(_)
        | uhura_syntax::ast::ExpressionKind::Unit
        | uhura_syntax::ast::ExpressionKind::Name(_) => false,
    }
}

fn expression_is_reaction_statement(expression: &uhura_syntax::ast::Expression) -> bool {
    match &expression.kind {
        uhura_syntax::ast::ExpressionKind::Group(value) => expression_is_reaction_statement(value),
        uhura_syntax::ast::ExpressionKind::Block(_)
        | uhura_syntax::ast::ExpressionKind::If(_)
        | uhura_syntax::ast::ExpressionKind::Match(_)
        | uhura_syntax::ast::ExpressionKind::Return(_) => true,
        _ => expression_has_reaction_control(expression),
    }
}

fn block_has_reaction_control(block: &uhura_syntax::ast::Block) -> bool {
    block
        .statements
        .iter()
        .any(|statement| match &statement.kind {
            uhura_syntax::ast::StatementKind::Assign { .. }
            | uhura_syntax::ast::StatementKind::Emit { .. }
            | uhura_syntax::ast::StatementKind::While { .. }
            | uhura_syntax::ast::StatementKind::Unreachable { .. } => true,
            uhura_syntax::ast::StatementKind::Let { value, .. } => {
                expression_has_reaction_control(value)
            }
            uhura_syntax::ast::StatementKind::Expression { expression, .. }
            | uhura_syntax::ast::StatementKind::BlockExpression(expression) => {
                expression_has_reaction_control(expression)
            }
        })
        || block
            .tail
            .as_deref()
            .is_some_and(expression_has_reaction_control)
}

fn block_has_return(block: &uhura_syntax::ast::Block) -> bool {
    block
        .statements
        .iter()
        .any(|statement| match &statement.kind {
            uhura_syntax::ast::StatementKind::Let { value, .. }
            | uhura_syntax::ast::StatementKind::Assign { value, .. } => {
                expression_has_return(value)
            }
            uhura_syntax::ast::StatementKind::Emit { output, .. } => {
                output.arguments.iter().any(expression_has_return)
            }
            uhura_syntax::ast::StatementKind::While {
                condition,
                decreases,
                body,
            } => {
                expression_has_return(condition)
                    || expression_has_return(decreases)
                    || block_has_return(body)
            }
            uhura_syntax::ast::StatementKind::Expression { expression, .. }
            | uhura_syntax::ast::StatementKind::BlockExpression(expression) => {
                expression_has_return(expression)
            }
            uhura_syntax::ast::StatementKind::Unreachable { .. } => false,
        })
        || block.tail.as_deref().is_some_and(expression_has_return)
}

fn reject_cycles(
    updates: &BTreeMap<String, InlineUpdate>,
    diagnostics: &mut Vec<Diagnostic>,
    span: uhura_syntax::ast::Span,
) {
    let mut graph = BTreeMap::<String, BTreeSet<String>>::new();
    for (name, update) in updates {
        collect_inline_calls_in_block(&update.declaration.body, updates, &mut |target| {
            graph.entry(name.clone()).or_default().insert(target);
        });
    }
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for name in graph.keys() {
        if graph_has_cycle(name, &graph, &mut visiting, &mut visited) {
            diagnostic(
                diagnostics,
                codes::DEPENDENCY_CYCLE,
                "uhura-0.4/update-cycle",
                "non-terminal updates form a recursive call cycle",
                span,
            );
            break;
        }
    }
}

fn collect_inline_calls_in_block(
    block: &uhura_syntax::ast::Block,
    updates: &BTreeMap<String, InlineUpdate>,
    visitor: &mut impl FnMut(String),
) {
    for statement in &block.statements {
        match &statement.kind {
            uhura_syntax::ast::StatementKind::Let { value, .. }
            | uhura_syntax::ast::StatementKind::Assign { value, .. } => {
                collect_inline_calls(value, updates, visitor);
            }
            uhura_syntax::ast::StatementKind::Emit { output, .. } => {
                for value in &output.arguments {
                    collect_inline_calls(value, updates, visitor);
                }
            }
            uhura_syntax::ast::StatementKind::While {
                condition,
                decreases,
                body,
            } => {
                collect_inline_calls(condition, updates, visitor);
                collect_inline_calls(decreases, updates, visitor);
                collect_inline_calls_in_block(body, updates, visitor);
            }
            uhura_syntax::ast::StatementKind::Expression { expression, .. }
            | uhura_syntax::ast::StatementKind::BlockExpression(expression) => {
                collect_inline_calls(expression, updates, visitor);
            }
            uhura_syntax::ast::StatementKind::Unreachable { .. } => {}
        }
    }
    if let Some(tail) = &block.tail {
        collect_inline_calls(tail, updates, visitor);
    }
}

fn collect_inline_calls(
    expression: &uhura_syntax::ast::Expression,
    updates: &BTreeMap<String, InlineUpdate>,
    visitor: &mut impl FnMut(String),
) {
    if let Some((target, _)) = direct_inline_call(expression, updates) {
        visitor(target);
    }
    match &expression.kind {
        uhura_syntax::ast::ExpressionKind::Sequence(values)
        | uhura_syntax::ast::ExpressionKind::Tuple(values) => {
            for value in values {
                collect_inline_calls(value, updates, visitor);
            }
        }
        uhura_syntax::ast::ExpressionKind::Group(value)
        | uhura_syntax::ast::ExpressionKind::Unary { value, .. }
        | uhura_syntax::ast::ExpressionKind::Member { value, .. }
        | uhura_syntax::ast::ExpressionKind::Is { value, .. }
        | uhura_syntax::ast::ExpressionKind::Return(Some(value)) => {
            collect_inline_calls(value, updates, visitor);
        }
        uhura_syntax::ast::ExpressionKind::Record(value) => {
            for value in value.fields.iter().filter_map(|field| field.value.as_ref()) {
                collect_inline_calls(value, updates, visitor);
            }
            if let Some(base) = &value.base {
                collect_inline_calls(base, updates, visitor);
            }
        }
        uhura_syntax::ast::ExpressionKind::AnonymousRecord(entries) => {
            for entry in entries {
                collect_inline_calls(&entry.key, updates, visitor);
                collect_inline_calls(&entry.value, updates, visitor);
            }
        }
        uhura_syntax::ast::ExpressionKind::Block(block) => {
            collect_inline_calls_in_block(block, updates, visitor);
        }
        uhura_syntax::ast::ExpressionKind::Call { callee, arguments } => {
            collect_inline_calls(callee, updates, visitor);
            for argument in arguments {
                match argument {
                    uhura_syntax::ast::CallArgument::Expression(value) => {
                        collect_inline_calls(value, updates, visitor);
                    }
                    uhura_syntax::ast::CallArgument::Binder(value) => {
                        collect_inline_calls(&value.body, updates, visitor);
                    }
                }
            }
        }
        uhura_syntax::ast::ExpressionKind::Index { value, index }
        | uhura_syntax::ast::ExpressionKind::Binary {
            left: value,
            right: index,
            ..
        }
        | uhura_syntax::ast::ExpressionKind::Compare {
            left: value,
            right: index,
            ..
        } => {
            collect_inline_calls(value, updates, visitor);
            collect_inline_calls(index, updates, visitor);
        }
        uhura_syntax::ast::ExpressionKind::If(value) => {
            collect_inline_calls(&value.condition, updates, visitor);
            collect_inline_calls_in_block(&value.then_branch, updates, visitor);
            if let Some(branch) = &value.else_branch {
                match branch {
                    uhura_syntax::ast::ElseBranch::Block(block) => {
                        collect_inline_calls_in_block(block, updates, visitor);
                    }
                    uhura_syntax::ast::ElseBranch::If(value) => {
                        collect_inline_calls(value, updates, visitor);
                    }
                }
            }
        }
        uhura_syntax::ast::ExpressionKind::Match(value) => {
            collect_inline_calls(&value.value, updates, visitor);
            for arm in &value.arms {
                collect_inline_calls(&arm.value, updates, visitor);
            }
        }
        uhura_syntax::ast::ExpressionKind::Literal(_)
        | uhura_syntax::ast::ExpressionKind::Unit
        | uhura_syntax::ast::ExpressionKind::Name(_)
        | uhura_syntax::ast::ExpressionKind::Return(None) => {}
    }
}

fn graph_has_cycle(
    node: &str,
    graph: &BTreeMap<String, BTreeSet<String>>,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
) -> bool {
    if visited.contains(node) {
        return false;
    }
    if !visiting.insert(node.to_owned()) {
        return true;
    }
    if graph.get(node).is_some_and(|edges| {
        edges
            .iter()
            .any(|edge| graph_has_cycle(edge, graph, visiting, visited))
    }) {
        return true;
    }
    visiting.remove(node);
    visited.insert(node.to_owned());
    false
}

fn alpha_rename_block(
    block: &mut uhura_syntax::ast::Block,
    renames: &mut BTreeMap<String, String>,
    prefix: &str,
) {
    for statement in &mut block.statements {
        match &mut statement.kind {
            uhura_syntax::ast::StatementKind::Let { name, value, .. } => {
                alpha_rename_expression(value, renames, prefix);
                let original = name.text.clone();
                name.text = format!("{prefix}local_{original}");
                renames.insert(original, name.text.clone());
            }
            uhura_syntax::ast::StatementKind::Assign { target, value, .. } => {
                rename_identifier(target, renames);
                alpha_rename_expression(value, renames, prefix);
            }
            uhura_syntax::ast::StatementKind::Emit { output, .. } => {
                for value in &mut output.arguments {
                    alpha_rename_expression(value, renames, prefix);
                }
            }
            uhura_syntax::ast::StatementKind::While {
                condition,
                decreases,
                body,
            } => {
                alpha_rename_expression(condition, renames, prefix);
                alpha_rename_expression(decreases, renames, prefix);
                let mut child = renames.clone();
                alpha_rename_block(body, &mut child, prefix);
            }
            uhura_syntax::ast::StatementKind::Expression { expression, .. }
            | uhura_syntax::ast::StatementKind::BlockExpression(expression) => {
                alpha_rename_expression(expression, renames, prefix);
            }
            uhura_syntax::ast::StatementKind::Unreachable { .. } => {}
        }
    }
    if let Some(tail) = &mut block.tail {
        alpha_rename_expression(tail, renames, prefix);
    }
}

fn alpha_rename_expression(
    expression: &mut uhura_syntax::ast::Expression,
    renames: &BTreeMap<String, String>,
    prefix: &str,
) {
    match &mut expression.kind {
        uhura_syntax::ast::ExpressionKind::Name(name) => {
            if name.segments.len() == 1 {
                rename_identifier(&mut name.segments[0], renames);
            }
        }
        uhura_syntax::ast::ExpressionKind::Sequence(values)
        | uhura_syntax::ast::ExpressionKind::Tuple(values) => {
            for value in values {
                alpha_rename_expression(value, renames, prefix);
            }
        }
        uhura_syntax::ast::ExpressionKind::Group(value)
        | uhura_syntax::ast::ExpressionKind::Unary { value, .. }
        | uhura_syntax::ast::ExpressionKind::Member { value, .. }
        | uhura_syntax::ast::ExpressionKind::Is { value, .. }
        | uhura_syntax::ast::ExpressionKind::Return(Some(value)) => {
            alpha_rename_expression(value, renames, prefix);
        }
        uhura_syntax::ast::ExpressionKind::Record(value) => {
            for field in &mut value.fields {
                if let Some(value) = &mut field.value {
                    alpha_rename_expression(value, renames, prefix);
                } else if let Some(name) = renames.get(&field.name.text) {
                    field.value = Some(name_expression(name.clone(), field.name.span));
                }
            }
            if let Some(base) = &mut value.base {
                alpha_rename_expression(base, renames, prefix);
            }
        }
        uhura_syntax::ast::ExpressionKind::AnonymousRecord(entries) => {
            for entry in entries {
                alpha_rename_expression(&mut entry.key, renames, prefix);
                alpha_rename_expression(&mut entry.value, renames, prefix);
            }
        }
        uhura_syntax::ast::ExpressionKind::Block(block) => {
            let mut child = renames.clone();
            alpha_rename_block(block, &mut child, prefix);
        }
        uhura_syntax::ast::ExpressionKind::Call { callee, arguments } => {
            alpha_rename_expression(callee, renames, prefix);
            for argument in arguments {
                match argument {
                    uhura_syntax::ast::CallArgument::Expression(value) => {
                        alpha_rename_expression(value, renames, prefix);
                    }
                    uhura_syntax::ast::CallArgument::Binder(value) => {
                        let mut child = renames.clone();
                        let original = value.parameter.text.clone();
                        value.parameter.text = format!("{prefix}bind_{original}");
                        child.insert(original, value.parameter.text.clone());
                        alpha_rename_expression(&mut value.body, &child, prefix);
                    }
                }
            }
        }
        uhura_syntax::ast::ExpressionKind::Index { value, index }
        | uhura_syntax::ast::ExpressionKind::Binary {
            left: value,
            right: index,
            ..
        }
        | uhura_syntax::ast::ExpressionKind::Compare {
            left: value,
            right: index,
            ..
        } => {
            alpha_rename_expression(value, renames, prefix);
            alpha_rename_expression(index, renames, prefix);
        }
        uhura_syntax::ast::ExpressionKind::If(value) => {
            alpha_rename_expression(&mut value.condition, renames, prefix);
            let mut then_scope = renames.clone();
            alpha_rename_block(&mut value.then_branch, &mut then_scope, prefix);
            if let Some(branch) = &mut value.else_branch {
                match branch {
                    uhura_syntax::ast::ElseBranch::Block(block) => {
                        let mut child = renames.clone();
                        alpha_rename_block(block, &mut child, prefix);
                    }
                    uhura_syntax::ast::ElseBranch::If(value) => {
                        alpha_rename_expression(value, renames, prefix);
                    }
                }
            }
        }
        uhura_syntax::ast::ExpressionKind::Match(value) => {
            alpha_rename_expression(&mut value.value, renames, prefix);
            for arm in &mut value.arms {
                let mut child = renames.clone();
                alpha_rename_pattern(&mut arm.pattern, &mut child, prefix);
                alpha_rename_expression(&mut arm.value, &child, prefix);
            }
        }
        uhura_syntax::ast::ExpressionKind::Literal(_)
        | uhura_syntax::ast::ExpressionKind::Unit
        | uhura_syntax::ast::ExpressionKind::Return(None) => {}
    }
}

fn alpha_rename_pattern(
    pattern: &mut uhura_syntax::ast::Pattern,
    renames: &mut BTreeMap<String, String>,
    prefix: &str,
) {
    match &mut pattern.kind {
        uhura_syntax::ast::PatternKind::Binder(value) => {
            let original = value.text.clone();
            value.text = format!("{prefix}bind_{original}");
            renames.insert(original, value.text.clone());
        }
        uhura_syntax::ast::PatternKind::Group(value) => {
            alpha_rename_pattern(value, renames, prefix)
        }
        uhura_syntax::ast::PatternKind::Tuple(values)
        | uhura_syntax::ast::PatternKind::Alternative(values) => {
            for value in values {
                alpha_rename_pattern(value, renames, prefix);
            }
        }
        uhura_syntax::ast::PatternKind::TupleConstructor { arguments, .. } => {
            for argument in arguments {
                alpha_rename_pattern(argument, renames, prefix);
            }
        }
        uhura_syntax::ast::PatternKind::Record { fields, .. }
        | uhura_syntax::ast::PatternKind::AnonymousRecord { fields, .. } => {
            for field in fields {
                if let Some(pattern) = &mut field.pattern {
                    alpha_rename_pattern(pattern, renames, prefix);
                } else {
                    let original = field.name.text.clone();
                    let lowered = format!("{prefix}bind_{original}");
                    field.pattern = Some(uhura_syntax::ast::Node::new(
                        uhura_syntax::ast::PatternKind::Binder(uhura_syntax::ast::Identifier::new(
                            lowered.clone(),
                            field.name.span,
                        )),
                        field.span,
                    ));
                    renames.insert(original, lowered);
                }
            }
        }
        uhura_syntax::ast::PatternKind::Wildcard
        | uhura_syntax::ast::PatternKind::Literal(_)
        | uhura_syntax::ast::PatternKind::Constructor(_) => {}
    }
}

fn rename_identifier(
    identifier: &mut uhura_syntax::ast::Identifier,
    renames: &BTreeMap<String, String>,
) {
    if let Some(name) = renames.get(&identifier.text) {
        identifier.text.clone_from(name);
    }
}

fn prepend_statement(
    statement: uhura_syntax::ast::Statement,
    mut block: uhura_syntax::ast::Block,
    span: uhura_syntax::ast::Span,
) -> uhura_syntax::ast::Block {
    block.statements.insert(0, statement);
    block.span = span;
    block
}

fn prepend_statements(
    mut statements: Vec<uhura_syntax::ast::Statement>,
    mut block: uhura_syntax::ast::Block,
    span: uhura_syntax::ast::Span,
) -> uhura_syntax::ast::Block {
    statements.append(&mut block.statements);
    block.statements = statements;
    block.span = span;
    block
}

fn append_blocks(
    mut prefix: uhura_syntax::ast::Block,
    mut suffix: uhura_syntax::ast::Block,
    span: uhura_syntax::ast::Span,
) -> uhura_syntax::ast::Block {
    if let Some(tail) = prefix.tail.take() {
        let tail_span = tail.span;
        prefix.statements.push(uhura_syntax::ast::Node::new(
            uhura_syntax::ast::StatementKind::BlockExpression(*tail),
            tail_span,
        ));
    }
    prefix.statements.append(&mut suffix.statements);
    prefix.tail = suffix.tail;
    prefix.span = span;
    prefix
}

fn block_with_tail(value: uhura_syntax::ast::Expression) -> uhura_syntax::ast::Block {
    uhura_syntax::ast::Block {
        statements: Vec::new(),
        span: value.span,
        tail: Some(Box::new(value)),
    }
}

fn block_expression(
    block: uhura_syntax::ast::Block,
    span: uhura_syntax::ast::Span,
) -> uhura_syntax::ast::Expression {
    uhura_syntax::ast::Node::new(uhura_syntax::ast::ExpressionKind::Block(block), span)
}

fn option_type(
    value: uhura_syntax::ast::TypeExpression,
    span: uhura_syntax::ast::Span,
) -> uhura_syntax::ast::TypeExpression {
    uhura_syntax::ast::Node::new(
        uhura_syntax::ast::TypeExpressionKind::Path(uhura_syntax::ast::TypePath {
            segments: vec![uhura_syntax::ast::TypePathSegment {
                name: uhura_syntax::ast::Identifier::new("Option", span),
                arguments: vec![value],
                span,
            }],
            span,
        }),
        span,
    )
}

fn some_pattern(name: String, span: uhura_syntax::ast::Span) -> uhura_syntax::ast::Pattern {
    uhura_syntax::ast::Node::new(
        uhura_syntax::ast::PatternKind::TupleConstructor {
            constructor: uhura_syntax::ast::QualifiedName {
                segments: vec![uhura_syntax::ast::Identifier::new("Some", span)],
                span,
            },
            arguments: vec![uhura_syntax::ast::Node::new(
                uhura_syntax::ast::PatternKind::Binder(uhura_syntax::ast::Identifier::new(
                    name, span,
                )),
                span,
            )],
        },
        span,
    )
}

fn none_pattern(span: uhura_syntax::ast::Span) -> uhura_syntax::ast::Pattern {
    uhura_syntax::ast::Node::new(
        uhura_syntax::ast::PatternKind::Constructor(uhura_syntax::ast::QualifiedName {
            segments: vec![uhura_syntax::ast::Identifier::new("None", span)],
            span,
        }),
        span,
    )
}

fn match_expression(
    value: uhura_syntax::ast::Expression,
    arms: Vec<(uhura_syntax::ast::Pattern, uhura_syntax::ast::Expression)>,
    span: uhura_syntax::ast::Span,
) -> uhura_syntax::ast::Expression {
    uhura_syntax::ast::Node::new(
        uhura_syntax::ast::ExpressionKind::Match(uhura_syntax::ast::MatchExpression {
            value: Box::new(value),
            arms: arms
                .into_iter()
                .map(|(pattern, value)| uhura_syntax::ast::MatchArm {
                    pattern,
                    value,
                    span,
                })
                .collect(),
        }),
        span,
    )
}

fn name_expression(name: String, span: uhura_syntax::ast::Span) -> uhura_syntax::ast::Expression {
    uhura_syntax::ast::Node::new(
        uhura_syntax::ast::ExpressionKind::Name(uhura_syntax::ast::QualifiedName {
            segments: vec![uhura_syntax::ast::Identifier::new(name, span)],
            span,
        }),
        span,
    )
}

fn call_expression(
    name: &str,
    arguments: Vec<uhura_syntax::ast::Expression>,
    span: uhura_syntax::ast::Span,
) -> uhura_syntax::ast::Expression {
    uhura_syntax::ast::Node::new(
        uhura_syntax::ast::ExpressionKind::Call {
            callee: Box::new(name_expression(name.to_owned(), span)),
            arguments: arguments
                .into_iter()
                .map(uhura_syntax::ast::CallArgument::Expression)
                .collect(),
        },
        span,
    )
}

fn diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    code: &'static str,
    rule: &'static str,
    message: impl Into<String>,
    span: uhura_syntax::ast::Span,
) {
    diagnostics.push(error(
        code,
        rule,
        message,
        ast::SourceSpan::new(span.file, span.start, span.end),
    ));
}
