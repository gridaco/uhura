//! Static lowering for non-terminal Uhura 0.4 updates.
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
use uhura_syntax::v04;

use crate::checker_ir as ast;
use crate::diagnostic::{codes, error};

#[derive(Clone, Debug)]
struct InlineUpdate {
    declaration: v04::ast::UpdateDeclaration,
    result: v04::ast::TypeExpression,
    span: v04::ast::Span,
}

#[derive(Clone, Debug)]
enum EffectContinuation {
    Tail,
    Return,
    BindThen {
        name: v04::ast::Identifier,
        ty: Option<v04::ast::TypeExpression>,
        remainder: v04::ast::Block,
        normal: Box<EffectContinuation>,
        span: v04::ast::Span,
    },
    DiscardThen {
        remainder: v04::ast::Block,
        normal: Box<EffectContinuation>,
        span: v04::ast::Span,
    },
    CaptureUpdateResult {
        name: String,
        ty: v04::ast::TypeExpression,
        span: v04::ast::Span,
    },
    CaptureUpdateLoopExit {
        name: String,
        ty: v04::ast::TypeExpression,
        span: v04::ast::Span,
    },
    Unary {
        operator: v04::ast::UnaryOperator,
        span: v04::ast::Span,
        next: Box<EffectContinuation>,
    },
    BinaryLeft {
        operator: v04::ast::BinaryOperator,
        right: v04::ast::Expression,
        span: v04::ast::Span,
        next: Box<EffectContinuation>,
    },
    BinaryRight {
        operator: v04::ast::BinaryOperator,
        left: v04::ast::Expression,
        span: v04::ast::Span,
        next: Box<EffectContinuation>,
    },
    ShortCircuitLeft {
        operator: v04::ast::BinaryOperator,
        right: v04::ast::Expression,
        span: v04::ast::Span,
        next: Box<EffectContinuation>,
    },
    CompareLeft {
        operator: v04::ast::ComparisonOperator,
        right: v04::ast::Expression,
        span: v04::ast::Span,
        next: Box<EffectContinuation>,
    },
    CompareRight {
        operator: v04::ast::ComparisonOperator,
        left: v04::ast::Expression,
        span: v04::ast::Span,
        next: Box<EffectContinuation>,
    },
    Member {
        member: v04::ast::Identifier,
        span: v04::ast::Span,
        next: Box<EffectContinuation>,
    },
    IndexValue {
        index: v04::ast::Expression,
        span: v04::ast::Span,
        next: Box<EffectContinuation>,
    },
    IndexIndex {
        value: v04::ast::Expression,
        span: v04::ast::Span,
        next: Box<EffectContinuation>,
    },
    Is {
        pattern: v04::ast::Pattern,
        span: v04::ast::Span,
        next: Box<EffectContinuation>,
    },
    Sequence {
        tuple: bool,
        completed: Vec<v04::ast::Expression>,
        remaining: Vec<v04::ast::Expression>,
        span: v04::ast::Span,
        next: Box<EffectContinuation>,
    },
    IfCondition {
        expression: v04::ast::IfExpression,
        span: v04::ast::Span,
        next: Box<EffectContinuation>,
    },
    MatchSubject {
        expression: v04::ast::MatchExpression,
        span: v04::ast::Span,
        next: Box<EffectContinuation>,
    },
    CallCallee {
        arguments: Vec<v04::ast::CallArgument>,
        span: v04::ast::Span,
        next: Box<EffectContinuation>,
    },
    CallArgument {
        callee: v04::ast::Expression,
        arguments: Vec<v04::ast::CallArgument>,
        index: usize,
        span: v04::ast::Span,
        next: Box<EffectContinuation>,
    },
    InlineCallArgument {
        target: String,
        arguments: Vec<v04::ast::Expression>,
        index: usize,
        span: v04::ast::Span,
        next: Box<EffectContinuation>,
    },
    RecordField {
        record: v04::ast::RecordExpression,
        index: usize,
        span: v04::ast::Span,
        next: Box<EffectContinuation>,
    },
    RecordBase {
        record: v04::ast::RecordExpression,
        span: v04::ast::Span,
        next: Box<EffectContinuation>,
    },
    AssignValue {
        target: v04::ast::Identifier,
        semicolon: v04::ast::Span,
        remainder: v04::ast::Block,
        normal: Box<EffectContinuation>,
        statement_span: v04::ast::Span,
        block_span: v04::ast::Span,
    },
    EmitArgument {
        output: v04::ast::OutputConstructor,
        index: usize,
        semicolon: v04::ast::Span,
        remainder: v04::ast::Block,
        normal: Box<EffectContinuation>,
        statement_span: v04::ast::Span,
        block_span: v04::ast::Span,
    },
    WhileCondition {
        decreases: v04::ast::Expression,
        body: v04::ast::Block,
        remainder: v04::ast::Block,
        normal: Box<EffectContinuation>,
        statement_span: v04::ast::Span,
        block_span: v04::ast::Span,
    },
    WhileMeasure {
        condition: v04::ast::Expression,
        body: v04::ast::Block,
        remainder: v04::ast::Block,
        normal: Box<EffectContinuation>,
        statement_span: v04::ast::Span,
        block_span: v04::ast::Span,
    },
}

struct Engine<'a> {
    updates: BTreeMap<String, InlineUpdate>,
    diagnostics: &'a mut Vec<Diagnostic>,
    stack: Vec<String>,
    temporary: u64,
}

pub(super) fn lower_project(modules: &mut [v04::ast::Module], diagnostics: &mut Vec<Diagnostic>) {
    for module in modules {
        for declaration in &mut module.declarations {
            let v04::ast::DeclarationKind::Machine(machine) = &mut declaration.kind else {
                continue;
            };
            lower_machine(machine, diagnostics);
        }
    }
}

fn lower_machine(machine: &mut v04::ast::MachineDeclaration, diagnostics: &mut Vec<Diagnostic>) {
    let mut updates = BTreeMap::new();
    for member in &machine.members {
        let v04::ast::MachineMemberKind::Update(update) = &member.kind else {
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
            v04::ast::MachineMemberKind::Update(update)
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
            v04::ast::MachineMemberKind::Handler(value) => Some(&mut value.body),
            v04::ast::MachineMemberKind::Update(value) => Some(&mut value.body),
            v04::ast::MachineMemberKind::BeforeCommit(value) => Some(&mut value.body),
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
        block: v04::ast::Block,
        normal: EffectContinuation,
        returned: EffectContinuation,
    ) -> v04::ast::Block {
        self.lower_block_at(&block, 0, normal, returned)
    }

    fn lower_block_at(
        &mut self,
        block: &v04::ast::Block,
        index: usize,
        normal: EffectContinuation,
        returned: EffectContinuation,
    ) -> v04::ast::Block {
        let Some(statement) = block.statements.get(index) else {
            if block.tail.is_none() && matches!(normal, EffectContinuation::Tail) {
                return v04::ast::Block {
                    statements: Vec::new(),
                    tail: None,
                    span: block.span,
                };
            }
            let value =
                block.tail.as_deref().cloned().unwrap_or_else(|| {
                    v04::ast::Node::new(v04::ast::ExpressionKind::Unit, block.span)
                });
            return self.lower_expression(value, normal, returned);
        };
        let remainder = v04::ast::Block {
            statements: block.statements[index + 1..].to_vec(),
            tail: block.tail.clone(),
            span: block.span,
        };
        match &statement.kind {
            v04::ast::StatementKind::Let {
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
            v04::ast::StatementKind::Expression { expression, .. }
            | v04::ast::StatementKind::BlockExpression(expression) => self.lower_expression(
                expression.clone(),
                EffectContinuation::DiscardThen {
                    remainder,
                    normal: Box::new(normal),
                    span: statement.span,
                },
                returned,
            ),
            v04::ast::StatementKind::While {
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
            v04::ast::StatementKind::Assign {
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
            v04::ast::StatementKind::Emit { output, semicolon } => {
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
            v04::ast::StatementKind::Unreachable { .. } => v04::ast::Block {
                statements: vec![statement.clone()],
                tail: None,
                span: block.span,
            },
        }
    }

    fn lower_expression(
        &mut self,
        expression: v04::ast::Expression,
        normal: EffectContinuation,
        returned: EffectContinuation,
    ) -> v04::ast::Block {
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
            v04::ast::ExpressionKind::Group(_)
                | v04::ast::ExpressionKind::Block(_)
                | v04::ast::ExpressionKind::If(_)
                | v04::ast::ExpressionKind::Match(_)
        );
        let needs_structural_effect_lowering = structural_expression
            && (first_inline_call(&expression, &self.updates).is_some()
                || matches!(&normal, EffectContinuation::CaptureUpdateResult { .. }));
        if !expression_has_return(&expression) && !needs_structural_effect_lowering {
            self.reject_nested_calls(&expression, "a nested expression");
            return self.apply_continuation(expression, normal, returned);
        }

        match expression.kind {
            v04::ast::ExpressionKind::Return(value) => {
                let value = value.map_or_else(
                    || v04::ast::Node::new(v04::ast::ExpressionKind::Unit, expression.span),
                    |value| *value,
                );
                self.lower_expression(value, returned.clone(), returned)
            }
            v04::ast::ExpressionKind::Group(value) => {
                self.lower_expression(*value, normal, returned)
            }
            v04::ast::ExpressionKind::Block(block) => self.lower_block(block, normal, returned),
            v04::ast::ExpressionKind::Unary { operator, value } => {
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
            v04::ast::ExpressionKind::Binary {
                operator,
                left,
                right,
            } => {
                self.reject_nested_calls(&left, "a binary operand");
                self.reject_nested_calls(&right, "a binary operand");
                let continuation = if matches!(
                    operator,
                    v04::ast::BinaryOperator::And | v04::ast::BinaryOperator::Or
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
            v04::ast::ExpressionKind::Compare {
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
            v04::ast::ExpressionKind::Member { value, member } => {
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
            v04::ast::ExpressionKind::Index { value, index } => {
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
            v04::ast::ExpressionKind::Is { value, pattern } => {
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
            v04::ast::ExpressionKind::Sequence(values) => {
                self.lower_sequence(values, false, normal, returned, expression.span)
            }
            v04::ast::ExpressionKind::Tuple(values) => {
                self.lower_sequence(values, true, normal, returned, expression.span)
            }
            v04::ast::ExpressionKind::Record(record) => {
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
            v04::ast::ExpressionKind::AnonymousRecord(entries) => self.apply_continuation(
                v04::ast::Node::new(
                    v04::ast::ExpressionKind::AnonymousRecord(entries),
                    expression.span,
                ),
                normal,
                returned,
            ),
            v04::ast::ExpressionKind::Call { callee, arguments } => {
                let binder_returns = arguments.iter().any(|argument| {
                    matches!(argument, v04::ast::CallArgument::Binder(value) if expression_has_return(&value.body))
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
                        v04::ast::Node::new(
                            v04::ast::ExpressionKind::Call { callee, arguments },
                            expression.span,
                        ),
                        normal,
                        returned,
                    );
                }
                self.reject_nested_calls(&callee, "a call target");
                for argument in &arguments {
                    if let v04::ast::CallArgument::Expression(value) = argument {
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
            v04::ast::ExpressionKind::If(value) => {
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
                        v04::ast::ElseBranch::Block(block) => {
                            self.lower_block(block, normal, returned.clone())
                        }
                        v04::ast::ElseBranch::If(value) => {
                            self.lower_expression(*value, normal, returned.clone())
                        }
                    }
                } else {
                    self.apply_continuation(
                        v04::ast::Node::new(v04::ast::ExpressionKind::Unit, expression.span),
                        normal,
                        returned.clone(),
                    )
                };
                block_with_tail(v04::ast::Node::new(
                    v04::ast::ExpressionKind::If(v04::ast::IfExpression {
                        condition: value.condition,
                        then_branch,
                        else_branch: Some(v04::ast::ElseBranch::Block(else_branch)),
                    }),
                    expression.span,
                ))
            }
            v04::ast::ExpressionKind::Match(value) => {
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
                        v04::ast::MatchArm {
                            pattern: arm.pattern,
                            value: v04::ast::Node::new(v04::ast::ExpressionKind::Block(body), span),
                            span: arm.span,
                        }
                    })
                    .collect();
                block_with_tail(v04::ast::Node::new(
                    v04::ast::ExpressionKind::Match(v04::ast::MatchExpression {
                        value: value.value,
                        arms,
                    }),
                    expression.span,
                ))
            }
            v04::ast::ExpressionKind::Literal(_)
            | v04::ast::ExpressionKind::Unit
            | v04::ast::ExpressionKind::Name(_) => {
                unreachable!("return-free leaves were handled before CPS decomposition")
            }
        }
    }

    fn lower_call_arguments(
        &mut self,
        callee: v04::ast::Expression,
        arguments: Vec<v04::ast::CallArgument>,
        start: usize,
        next: EffectContinuation,
        returned: EffectContinuation,
        span: v04::ast::Span,
    ) -> v04::ast::Block {
        if let Some((index, value)) =
            arguments
                .iter()
                .enumerate()
                .skip(start)
                .find_map(|(index, argument)| match argument {
                    v04::ast::CallArgument::Expression(value) if expression_has_return(value) => {
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
            v04::ast::Node::new(
                v04::ast::ExpressionKind::Call {
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
        mut values: Vec<v04::ast::Expression>,
        tuple: bool,
        next: EffectContinuation,
        returned: EffectContinuation,
        span: v04::ast::Span,
    ) -> v04::ast::Block {
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
        arguments: Vec<v04::ast::Expression>,
        start: usize,
        next: EffectContinuation,
        returned: EffectContinuation,
        span: v04::ast::Span,
    ) -> v04::ast::Block {
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
        record: v04::ast::RecordExpression,
        start: usize,
        next: EffectContinuation,
        returned: EffectContinuation,
        span: v04::ast::Span,
    ) -> v04::ast::Block {
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
            v04::ast::Node::new(v04::ast::ExpressionKind::Record(record), span),
            next,
            returned,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_emit_arguments(
        &mut self,
        output: v04::ast::OutputConstructor,
        start: usize,
        semicolon: v04::ast::Span,
        remainder: v04::ast::Block,
        normal: EffectContinuation,
        returned: EffectContinuation,
        statement_span: v04::ast::Span,
        block_span: v04::ast::Span,
    ) -> v04::ast::Block {
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
            v04::ast::Node::new(
                v04::ast::StatementKind::Emit { output, semicolon },
                statement_span,
            ),
            rest,
            block_span,
        )
    }

    fn inline_call(
        &mut self,
        target: &str,
        arguments: Vec<v04::ast::Expression>,
        next: EffectContinuation,
        returned: EffectContinuation,
        call_span: v04::ast::Span,
    ) -> v04::ast::Block {
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
            return block_with_tail(v04::ast::Node::new(
                v04::ast::ExpressionKind::Unit,
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
            let argument = arguments
                .get(index)
                .cloned()
                .unwrap_or_else(|| v04::ast::Node::new(v04::ast::ExpressionKind::Unit, call_span));
            self.reject_nested_calls(&argument, "an update argument");
            prefix_statements.push(v04::ast::Node::new(
                v04::ast::StatementKind::Let {
                    name: v04::ast::Identifier::new(name, parameter.name.span),
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
        value: v04::ast::Expression,
        continuation: EffectContinuation,
        returned: EffectContinuation,
    ) -> v04::ast::Block {
        match continuation {
            EffectContinuation::Tail => block_with_tail(value),
            EffectContinuation::Return => block_with_tail(v04::ast::Node::new(
                v04::ast::ExpressionKind::Return(Some(Box::new(value.clone()))),
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
                    v04::ast::Node::new(
                        v04::ast::StatementKind::Let {
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
                        v04::ast::Node::new(v04::ast::StatementKind::BlockExpression(value), span),
                        rest,
                        span,
                    )
                } else {
                    let name = format!("__uhura_update_discard_{}", self.temporary);
                    self.temporary += 1;
                    prepend_statement(
                        v04::ast::Node::new(
                            v04::ast::StatementKind::Let {
                                name: v04::ast::Identifier::new(name, span),
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
                let identifier = v04::ast::Identifier::new(name.clone(), span);
                let checked = v04::ast::Node::new(
                    v04::ast::StatementKind::Let {
                        name: identifier,
                        ty: Some(ty),
                        value,
                        semicolon: span,
                    },
                    span,
                );
                v04::ast::Block {
                    statements: vec![checked],
                    tail: None,
                    span,
                }
            }
            EffectContinuation::CaptureUpdateLoopExit { name, ty, span } => {
                let checked = v04::ast::Node::new(
                    v04::ast::StatementKind::Let {
                        name: v04::ast::Identifier::new(name, span),
                        ty: Some(option_type(ty, span)),
                        value: call_expression("Some", vec![value], span),
                        semicolon: span,
                    },
                    span,
                );
                v04::ast::Block {
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
                v04::ast::Node::new(
                    v04::ast::ExpressionKind::Unary {
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
                v04::ast::Node::new(
                    v04::ast::ExpressionKind::Binary {
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
                let short_value = v04::ast::Node::new(
                    v04::ast::ExpressionKind::Literal(v04::ast::Literal::Bool(matches!(
                        operator,
                        v04::ast::BinaryOperator::Or
                    ))),
                    span,
                );
                let short_branch = self.apply_continuation(short_value, *next, returned);
                let (then_branch, else_branch) = match operator {
                    v04::ast::BinaryOperator::And => (right_branch, short_branch),
                    v04::ast::BinaryOperator::Or => (short_branch, right_branch),
                    _ => unreachable!("only boolean short-circuit operators use this frame"),
                };
                block_with_tail(v04::ast::Node::new(
                    v04::ast::ExpressionKind::If(v04::ast::IfExpression {
                        condition: Box::new(value),
                        then_branch,
                        else_branch: Some(v04::ast::ElseBranch::Block(else_branch)),
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
                v04::ast::Node::new(
                    v04::ast::ExpressionKind::Compare {
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
                v04::ast::Node::new(
                    v04::ast::ExpressionKind::Member {
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
                v04::ast::Node::new(
                    v04::ast::ExpressionKind::Index {
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
                v04::ast::Node::new(
                    v04::ast::ExpressionKind::Is {
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
                        v04::ast::Node::new(
                            if tuple {
                                v04::ast::ExpressionKind::Tuple(completed)
                            } else {
                                v04::ast::ExpressionKind::Sequence(completed)
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
                    v04::ast::Node::new(v04::ast::ExpressionKind::If(expression), span),
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
                    v04::ast::Node::new(v04::ast::ExpressionKind::Match(expression), span),
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
                arguments[index] = v04::ast::CallArgument::Expression(value);
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
                    v04::ast::Node::new(
                        v04::ast::StatementKind::Assign {
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
                    let after = v04::ast::Block {
                        statements: vec![v04::ast::Node::new(
                            v04::ast::StatementKind::BlockExpression(select),
                            result_span,
                        )],
                        tail: None,
                        span: block_span,
                    };
                    let with_loop = prepend_statement(
                        v04::ast::Node::new(
                            v04::ast::StatementKind::While {
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
                        v04::ast::Node::new(
                            v04::ast::StatementKind::Let {
                                name: v04::ast::Identifier::new(loop_exit_name, result_span),
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
                    v04::ast::Node::new(
                        v04::ast::StatementKind::While {
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

    fn reject_nested_calls(&mut self, expression: &v04::ast::Expression, context: &str) {
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

fn update_returns_outcome(update: &v04::ast::UpdateDeclaration) -> bool {
    update
        .result
        .as_ref()
        .is_some_and(|result| type_mentions(result, "Outcome"))
}

fn type_mentions(ty: &v04::ast::TypeExpression, name: &str) -> bool {
    match &ty.kind {
        v04::ast::TypeExpressionKind::Path(path) => path.segments.iter().any(|segment| {
            segment.name.text == name
                || segment
                    .arguments
                    .iter()
                    .any(|argument| type_mentions(argument, name))
        }),
        v04::ast::TypeExpressionKind::Tuple(values) => {
            values.iter().any(|value| type_mentions(value, name))
        }
        v04::ast::TypeExpressionKind::Unit => false,
    }
}

fn is_unit_type(ty: &v04::ast::TypeExpression) -> bool {
    matches!(ty.kind, v04::ast::TypeExpressionKind::Unit)
}

fn unit_type(span: v04::ast::Span) -> v04::ast::TypeExpression {
    v04::ast::Node::new(v04::ast::TypeExpressionKind::Unit, span)
}

fn direct_inline_call(
    expression: &v04::ast::Expression,
    updates: &BTreeMap<String, InlineUpdate>,
) -> Option<(String, Vec<v04::ast::Expression>)> {
    let expression = match &expression.kind {
        v04::ast::ExpressionKind::Group(value) => value.as_ref(),
        _ => expression,
    };
    let v04::ast::ExpressionKind::Call { callee, arguments } = &expression.kind else {
        return None;
    };
    let v04::ast::ExpressionKind::Name(name) = &callee.kind else {
        return None;
    };
    if name.segments.len() != 1 || !updates.contains_key(&name.segments[0].text) {
        return None;
    }
    let values = arguments
        .iter()
        .filter_map(|argument| match argument {
            v04::ast::CallArgument::Expression(value) => Some(value.clone()),
            v04::ast::CallArgument::Binder(_) => None,
        })
        .collect::<Vec<_>>();
    Some((name.segments[0].text.clone(), values))
}

fn first_inline_call(
    expression: &v04::ast::Expression,
    updates: &BTreeMap<String, InlineUpdate>,
) -> Option<(String, v04::ast::Span)> {
    if let Some((name, _)) = direct_inline_call(expression, updates) {
        return Some((name, expression.span));
    }
    match &expression.kind {
        v04::ast::ExpressionKind::Literal(_)
        | v04::ast::ExpressionKind::Unit
        | v04::ast::ExpressionKind::Name(_)
        | v04::ast::ExpressionKind::Return(None) => None,
        v04::ast::ExpressionKind::Sequence(values) | v04::ast::ExpressionKind::Tuple(values) => {
            values
                .iter()
                .find_map(|value| first_inline_call(value, updates))
        }
        v04::ast::ExpressionKind::Group(value)
        | v04::ast::ExpressionKind::Unary { value, .. }
        | v04::ast::ExpressionKind::Member { value, .. }
        | v04::ast::ExpressionKind::Is { value, .. } => first_inline_call(value, updates),
        v04::ast::ExpressionKind::Record(value) => value
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
        v04::ast::ExpressionKind::AnonymousRecord(entries) => entries.iter().find_map(|entry| {
            first_inline_call(&entry.key, updates)
                .or_else(|| first_inline_call(&entry.value, updates))
        }),
        v04::ast::ExpressionKind::Block(block) => first_inline_call_in_block(block, updates),
        v04::ast::ExpressionKind::Call { callee, arguments } => first_inline_call(callee, updates)
            .or_else(|| {
                arguments.iter().find_map(|argument| match argument {
                    v04::ast::CallArgument::Expression(value) => first_inline_call(value, updates),
                    v04::ast::CallArgument::Binder(value) => {
                        first_inline_call(&value.body, updates)
                    }
                })
            }),
        v04::ast::ExpressionKind::Index { value, index }
        | v04::ast::ExpressionKind::Binary {
            left: value,
            right: index,
            ..
        }
        | v04::ast::ExpressionKind::Compare {
            left: value,
            right: index,
            ..
        } => first_inline_call(value, updates).or_else(|| first_inline_call(index, updates)),
        v04::ast::ExpressionKind::If(value) => first_inline_call(&value.condition, updates)
            .or_else(|| first_inline_call_in_block(&value.then_branch, updates))
            .or_else(|| {
                value.else_branch.as_ref().and_then(|branch| match branch {
                    v04::ast::ElseBranch::Block(block) => {
                        first_inline_call_in_block(block, updates)
                    }
                    v04::ast::ElseBranch::If(value) => first_inline_call(value, updates),
                })
            }),
        v04::ast::ExpressionKind::Match(value) => {
            first_inline_call(&value.value, updates).or_else(|| {
                value
                    .arms
                    .iter()
                    .find_map(|arm| first_inline_call(&arm.value, updates))
            })
        }
        v04::ast::ExpressionKind::Return(Some(value)) => first_inline_call(value, updates),
    }
}

fn first_inline_call_in_block(
    block: &v04::ast::Block,
    updates: &BTreeMap<String, InlineUpdate>,
) -> Option<(String, v04::ast::Span)> {
    for statement in &block.statements {
        let found = match &statement.kind {
            v04::ast::StatementKind::Let { value, .. }
            | v04::ast::StatementKind::Assign { value, .. } => first_inline_call(value, updates),
            v04::ast::StatementKind::Emit { output, .. } => output
                .arguments
                .iter()
                .find_map(|value| first_inline_call(value, updates)),
            v04::ast::StatementKind::While {
                condition,
                decreases,
                body,
            } => first_inline_call(condition, updates)
                .or_else(|| first_inline_call(decreases, updates))
                .or_else(|| first_inline_call_in_block(body, updates)),
            v04::ast::StatementKind::Expression { expression, .. }
            | v04::ast::StatementKind::BlockExpression(expression) => {
                first_inline_call(expression, updates)
            }
            v04::ast::StatementKind::Unreachable { .. } => None,
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

fn expression_has_return(expression: &v04::ast::Expression) -> bool {
    match &expression.kind {
        v04::ast::ExpressionKind::Return(_) => true,
        v04::ast::ExpressionKind::Sequence(values) | v04::ast::ExpressionKind::Tuple(values) => {
            values.iter().any(expression_has_return)
        }
        v04::ast::ExpressionKind::Group(value)
        | v04::ast::ExpressionKind::Unary { value, .. }
        | v04::ast::ExpressionKind::Member { value, .. }
        | v04::ast::ExpressionKind::Is { value, .. } => expression_has_return(value),
        v04::ast::ExpressionKind::Record(value) => {
            value
                .fields
                .iter()
                .filter_map(|field| field.value.as_ref())
                .any(expression_has_return)
                || value.base.as_deref().is_some_and(expression_has_return)
        }
        v04::ast::ExpressionKind::AnonymousRecord(entries) => entries
            .iter()
            .any(|entry| expression_has_return(&entry.key) || expression_has_return(&entry.value)),
        v04::ast::ExpressionKind::Block(block) => block_has_return(block),
        v04::ast::ExpressionKind::Call { callee, arguments } => {
            expression_has_return(callee)
                || arguments.iter().any(|argument| match argument {
                    v04::ast::CallArgument::Expression(value) => expression_has_return(value),
                    v04::ast::CallArgument::Binder(value) => expression_has_return(&value.body),
                })
        }
        v04::ast::ExpressionKind::Index { value, index }
        | v04::ast::ExpressionKind::Binary {
            left: value,
            right: index,
            ..
        }
        | v04::ast::ExpressionKind::Compare {
            left: value,
            right: index,
            ..
        } => expression_has_return(value) || expression_has_return(index),
        v04::ast::ExpressionKind::If(value) => {
            expression_has_return(&value.condition)
                || block_has_return(&value.then_branch)
                || value
                    .else_branch
                    .as_ref()
                    .is_some_and(|branch| match branch {
                        v04::ast::ElseBranch::Block(block) => block_has_return(block),
                        v04::ast::ElseBranch::If(value) => expression_has_return(value),
                    })
        }
        v04::ast::ExpressionKind::Match(value) => {
            expression_has_return(&value.value)
                || value
                    .arms
                    .iter()
                    .any(|arm| expression_has_return(&arm.value))
        }
        v04::ast::ExpressionKind::Literal(_)
        | v04::ast::ExpressionKind::Unit
        | v04::ast::ExpressionKind::Name(_) => false,
    }
}

fn expression_has_reaction_control(expression: &v04::ast::Expression) -> bool {
    match &expression.kind {
        v04::ast::ExpressionKind::Return(_) => true,
        v04::ast::ExpressionKind::Sequence(values) | v04::ast::ExpressionKind::Tuple(values) => {
            values.iter().any(expression_has_reaction_control)
        }
        v04::ast::ExpressionKind::Group(value)
        | v04::ast::ExpressionKind::Unary { value, .. }
        | v04::ast::ExpressionKind::Member { value, .. }
        | v04::ast::ExpressionKind::Is { value, .. } => expression_has_reaction_control(value),
        v04::ast::ExpressionKind::Record(value) => {
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
        v04::ast::ExpressionKind::AnonymousRecord(entries) => entries.iter().any(|entry| {
            expression_has_reaction_control(&entry.key)
                || expression_has_reaction_control(&entry.value)
        }),
        v04::ast::ExpressionKind::Block(block) => block_has_reaction_control(block),
        v04::ast::ExpressionKind::Call { callee, arguments } => {
            expression_has_reaction_control(callee)
                || arguments.iter().any(|argument| match argument {
                    v04::ast::CallArgument::Expression(value) => {
                        expression_has_reaction_control(value)
                    }
                    v04::ast::CallArgument::Binder(value) => {
                        expression_has_reaction_control(&value.body)
                    }
                })
        }
        v04::ast::ExpressionKind::Index { value, index }
        | v04::ast::ExpressionKind::Binary {
            left: value,
            right: index,
            ..
        }
        | v04::ast::ExpressionKind::Compare {
            left: value,
            right: index,
            ..
        } => expression_has_reaction_control(value) || expression_has_reaction_control(index),
        v04::ast::ExpressionKind::If(value) => {
            expression_has_reaction_control(&value.condition)
                || block_has_reaction_control(&value.then_branch)
                || value
                    .else_branch
                    .as_ref()
                    .is_some_and(|branch| match branch {
                        v04::ast::ElseBranch::Block(block) => block_has_reaction_control(block),
                        v04::ast::ElseBranch::If(value) => expression_has_reaction_control(value),
                    })
        }
        v04::ast::ExpressionKind::Match(value) => {
            expression_has_reaction_control(&value.value)
                || value
                    .arms
                    .iter()
                    .any(|arm| expression_has_reaction_control(&arm.value))
        }
        v04::ast::ExpressionKind::Literal(_)
        | v04::ast::ExpressionKind::Unit
        | v04::ast::ExpressionKind::Name(_) => false,
    }
}

fn expression_is_reaction_statement(expression: &v04::ast::Expression) -> bool {
    match &expression.kind {
        v04::ast::ExpressionKind::Group(value) => expression_is_reaction_statement(value),
        v04::ast::ExpressionKind::Block(_)
        | v04::ast::ExpressionKind::If(_)
        | v04::ast::ExpressionKind::Match(_)
        | v04::ast::ExpressionKind::Return(_) => true,
        _ => expression_has_reaction_control(expression),
    }
}

fn block_has_reaction_control(block: &v04::ast::Block) -> bool {
    block
        .statements
        .iter()
        .any(|statement| match &statement.kind {
            v04::ast::StatementKind::Assign { .. }
            | v04::ast::StatementKind::Emit { .. }
            | v04::ast::StatementKind::While { .. }
            | v04::ast::StatementKind::Unreachable { .. } => true,
            v04::ast::StatementKind::Let { value, .. } => expression_has_reaction_control(value),
            v04::ast::StatementKind::Expression { expression, .. }
            | v04::ast::StatementKind::BlockExpression(expression) => {
                expression_has_reaction_control(expression)
            }
        })
        || block
            .tail
            .as_deref()
            .is_some_and(expression_has_reaction_control)
}

fn block_has_return(block: &v04::ast::Block) -> bool {
    block
        .statements
        .iter()
        .any(|statement| match &statement.kind {
            v04::ast::StatementKind::Let { value, .. }
            | v04::ast::StatementKind::Assign { value, .. } => expression_has_return(value),
            v04::ast::StatementKind::Emit { output, .. } => {
                output.arguments.iter().any(expression_has_return)
            }
            v04::ast::StatementKind::While {
                condition,
                decreases,
                body,
            } => {
                expression_has_return(condition)
                    || expression_has_return(decreases)
                    || block_has_return(body)
            }
            v04::ast::StatementKind::Expression { expression, .. }
            | v04::ast::StatementKind::BlockExpression(expression) => {
                expression_has_return(expression)
            }
            v04::ast::StatementKind::Unreachable { .. } => false,
        })
        || block.tail.as_deref().is_some_and(expression_has_return)
}

fn reject_cycles(
    updates: &BTreeMap<String, InlineUpdate>,
    diagnostics: &mut Vec<Diagnostic>,
    span: v04::ast::Span,
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
    block: &v04::ast::Block,
    updates: &BTreeMap<String, InlineUpdate>,
    visitor: &mut impl FnMut(String),
) {
    for statement in &block.statements {
        match &statement.kind {
            v04::ast::StatementKind::Let { value, .. }
            | v04::ast::StatementKind::Assign { value, .. } => {
                collect_inline_calls(value, updates, visitor);
            }
            v04::ast::StatementKind::Emit { output, .. } => {
                for value in &output.arguments {
                    collect_inline_calls(value, updates, visitor);
                }
            }
            v04::ast::StatementKind::While {
                condition,
                decreases,
                body,
            } => {
                collect_inline_calls(condition, updates, visitor);
                collect_inline_calls(decreases, updates, visitor);
                collect_inline_calls_in_block(body, updates, visitor);
            }
            v04::ast::StatementKind::Expression { expression, .. }
            | v04::ast::StatementKind::BlockExpression(expression) => {
                collect_inline_calls(expression, updates, visitor);
            }
            v04::ast::StatementKind::Unreachable { .. } => {}
        }
    }
    if let Some(tail) = &block.tail {
        collect_inline_calls(tail, updates, visitor);
    }
}

fn collect_inline_calls(
    expression: &v04::ast::Expression,
    updates: &BTreeMap<String, InlineUpdate>,
    visitor: &mut impl FnMut(String),
) {
    if let Some((target, _)) = direct_inline_call(expression, updates) {
        visitor(target);
    }
    match &expression.kind {
        v04::ast::ExpressionKind::Sequence(values) | v04::ast::ExpressionKind::Tuple(values) => {
            for value in values {
                collect_inline_calls(value, updates, visitor);
            }
        }
        v04::ast::ExpressionKind::Group(value)
        | v04::ast::ExpressionKind::Unary { value, .. }
        | v04::ast::ExpressionKind::Member { value, .. }
        | v04::ast::ExpressionKind::Is { value, .. }
        | v04::ast::ExpressionKind::Return(Some(value)) => {
            collect_inline_calls(value, updates, visitor);
        }
        v04::ast::ExpressionKind::Record(value) => {
            for value in value.fields.iter().filter_map(|field| field.value.as_ref()) {
                collect_inline_calls(value, updates, visitor);
            }
            if let Some(base) = &value.base {
                collect_inline_calls(base, updates, visitor);
            }
        }
        v04::ast::ExpressionKind::AnonymousRecord(entries) => {
            for entry in entries {
                collect_inline_calls(&entry.key, updates, visitor);
                collect_inline_calls(&entry.value, updates, visitor);
            }
        }
        v04::ast::ExpressionKind::Block(block) => {
            collect_inline_calls_in_block(block, updates, visitor);
        }
        v04::ast::ExpressionKind::Call { callee, arguments } => {
            collect_inline_calls(callee, updates, visitor);
            for argument in arguments {
                match argument {
                    v04::ast::CallArgument::Expression(value) => {
                        collect_inline_calls(value, updates, visitor);
                    }
                    v04::ast::CallArgument::Binder(value) => {
                        collect_inline_calls(&value.body, updates, visitor);
                    }
                }
            }
        }
        v04::ast::ExpressionKind::Index { value, index }
        | v04::ast::ExpressionKind::Binary {
            left: value,
            right: index,
            ..
        }
        | v04::ast::ExpressionKind::Compare {
            left: value,
            right: index,
            ..
        } => {
            collect_inline_calls(value, updates, visitor);
            collect_inline_calls(index, updates, visitor);
        }
        v04::ast::ExpressionKind::If(value) => {
            collect_inline_calls(&value.condition, updates, visitor);
            collect_inline_calls_in_block(&value.then_branch, updates, visitor);
            if let Some(branch) = &value.else_branch {
                match branch {
                    v04::ast::ElseBranch::Block(block) => {
                        collect_inline_calls_in_block(block, updates, visitor);
                    }
                    v04::ast::ElseBranch::If(value) => {
                        collect_inline_calls(value, updates, visitor);
                    }
                }
            }
        }
        v04::ast::ExpressionKind::Match(value) => {
            collect_inline_calls(&value.value, updates, visitor);
            for arm in &value.arms {
                collect_inline_calls(&arm.value, updates, visitor);
            }
        }
        v04::ast::ExpressionKind::Literal(_)
        | v04::ast::ExpressionKind::Unit
        | v04::ast::ExpressionKind::Name(_)
        | v04::ast::ExpressionKind::Return(None) => {}
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
    block: &mut v04::ast::Block,
    renames: &mut BTreeMap<String, String>,
    prefix: &str,
) {
    for statement in &mut block.statements {
        match &mut statement.kind {
            v04::ast::StatementKind::Let { name, value, .. } => {
                alpha_rename_expression(value, renames, prefix);
                let original = name.text.clone();
                name.text = format!("{prefix}local_{original}");
                renames.insert(original, name.text.clone());
            }
            v04::ast::StatementKind::Assign { target, value, .. } => {
                rename_identifier(target, renames);
                alpha_rename_expression(value, renames, prefix);
            }
            v04::ast::StatementKind::Emit { output, .. } => {
                for value in &mut output.arguments {
                    alpha_rename_expression(value, renames, prefix);
                }
            }
            v04::ast::StatementKind::While {
                condition,
                decreases,
                body,
            } => {
                alpha_rename_expression(condition, renames, prefix);
                alpha_rename_expression(decreases, renames, prefix);
                let mut child = renames.clone();
                alpha_rename_block(body, &mut child, prefix);
            }
            v04::ast::StatementKind::Expression { expression, .. }
            | v04::ast::StatementKind::BlockExpression(expression) => {
                alpha_rename_expression(expression, renames, prefix);
            }
            v04::ast::StatementKind::Unreachable { .. } => {}
        }
    }
    if let Some(tail) = &mut block.tail {
        alpha_rename_expression(tail, renames, prefix);
    }
}

fn alpha_rename_expression(
    expression: &mut v04::ast::Expression,
    renames: &BTreeMap<String, String>,
    prefix: &str,
) {
    match &mut expression.kind {
        v04::ast::ExpressionKind::Name(name) => {
            if name.segments.len() == 1 {
                rename_identifier(&mut name.segments[0], renames);
            }
        }
        v04::ast::ExpressionKind::Sequence(values) | v04::ast::ExpressionKind::Tuple(values) => {
            for value in values {
                alpha_rename_expression(value, renames, prefix);
            }
        }
        v04::ast::ExpressionKind::Group(value)
        | v04::ast::ExpressionKind::Unary { value, .. }
        | v04::ast::ExpressionKind::Member { value, .. }
        | v04::ast::ExpressionKind::Is { value, .. }
        | v04::ast::ExpressionKind::Return(Some(value)) => {
            alpha_rename_expression(value, renames, prefix);
        }
        v04::ast::ExpressionKind::Record(value) => {
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
        v04::ast::ExpressionKind::AnonymousRecord(entries) => {
            for entry in entries {
                alpha_rename_expression(&mut entry.key, renames, prefix);
                alpha_rename_expression(&mut entry.value, renames, prefix);
            }
        }
        v04::ast::ExpressionKind::Block(block) => {
            let mut child = renames.clone();
            alpha_rename_block(block, &mut child, prefix);
        }
        v04::ast::ExpressionKind::Call { callee, arguments } => {
            alpha_rename_expression(callee, renames, prefix);
            for argument in arguments {
                match argument {
                    v04::ast::CallArgument::Expression(value) => {
                        alpha_rename_expression(value, renames, prefix);
                    }
                    v04::ast::CallArgument::Binder(value) => {
                        let mut child = renames.clone();
                        let original = value.parameter.text.clone();
                        value.parameter.text = format!("{prefix}bind_{original}");
                        child.insert(original, value.parameter.text.clone());
                        alpha_rename_expression(&mut value.body, &child, prefix);
                    }
                }
            }
        }
        v04::ast::ExpressionKind::Index { value, index }
        | v04::ast::ExpressionKind::Binary {
            left: value,
            right: index,
            ..
        }
        | v04::ast::ExpressionKind::Compare {
            left: value,
            right: index,
            ..
        } => {
            alpha_rename_expression(value, renames, prefix);
            alpha_rename_expression(index, renames, prefix);
        }
        v04::ast::ExpressionKind::If(value) => {
            alpha_rename_expression(&mut value.condition, renames, prefix);
            let mut then_scope = renames.clone();
            alpha_rename_block(&mut value.then_branch, &mut then_scope, prefix);
            if let Some(branch) = &mut value.else_branch {
                match branch {
                    v04::ast::ElseBranch::Block(block) => {
                        let mut child = renames.clone();
                        alpha_rename_block(block, &mut child, prefix);
                    }
                    v04::ast::ElseBranch::If(value) => {
                        alpha_rename_expression(value, renames, prefix);
                    }
                }
            }
        }
        v04::ast::ExpressionKind::Match(value) => {
            alpha_rename_expression(&mut value.value, renames, prefix);
            for arm in &mut value.arms {
                let mut child = renames.clone();
                alpha_rename_pattern(&mut arm.pattern, &mut child, prefix);
                alpha_rename_expression(&mut arm.value, &child, prefix);
            }
        }
        v04::ast::ExpressionKind::Literal(_)
        | v04::ast::ExpressionKind::Unit
        | v04::ast::ExpressionKind::Return(None) => {}
    }
}

fn alpha_rename_pattern(
    pattern: &mut v04::ast::Pattern,
    renames: &mut BTreeMap<String, String>,
    prefix: &str,
) {
    match &mut pattern.kind {
        v04::ast::PatternKind::Binder(value) => {
            let original = value.text.clone();
            value.text = format!("{prefix}bind_{original}");
            renames.insert(original, value.text.clone());
        }
        v04::ast::PatternKind::Group(value) => alpha_rename_pattern(value, renames, prefix),
        v04::ast::PatternKind::Tuple(values) | v04::ast::PatternKind::Alternative(values) => {
            for value in values {
                alpha_rename_pattern(value, renames, prefix);
            }
        }
        v04::ast::PatternKind::TupleConstructor { arguments, .. } => {
            for argument in arguments {
                alpha_rename_pattern(argument, renames, prefix);
            }
        }
        v04::ast::PatternKind::Record { fields, .. }
        | v04::ast::PatternKind::AnonymousRecord { fields, .. } => {
            for field in fields {
                if let Some(pattern) = &mut field.pattern {
                    alpha_rename_pattern(pattern, renames, prefix);
                } else {
                    let original = field.name.text.clone();
                    let lowered = format!("{prefix}bind_{original}");
                    field.pattern = Some(v04::ast::Node::new(
                        v04::ast::PatternKind::Binder(v04::ast::Identifier::new(
                            lowered.clone(),
                            field.name.span,
                        )),
                        field.span,
                    ));
                    renames.insert(original, lowered);
                }
            }
        }
        v04::ast::PatternKind::Wildcard
        | v04::ast::PatternKind::Literal(_)
        | v04::ast::PatternKind::Constructor(_) => {}
    }
}

fn rename_identifier(identifier: &mut v04::ast::Identifier, renames: &BTreeMap<String, String>) {
    if let Some(name) = renames.get(&identifier.text) {
        identifier.text.clone_from(name);
    }
}

fn prepend_statement(
    statement: v04::ast::Statement,
    mut block: v04::ast::Block,
    span: v04::ast::Span,
) -> v04::ast::Block {
    block.statements.insert(0, statement);
    block.span = span;
    block
}

fn prepend_statements(
    mut statements: Vec<v04::ast::Statement>,
    mut block: v04::ast::Block,
    span: v04::ast::Span,
) -> v04::ast::Block {
    statements.append(&mut block.statements);
    block.statements = statements;
    block.span = span;
    block
}

fn append_blocks(
    mut prefix: v04::ast::Block,
    mut suffix: v04::ast::Block,
    span: v04::ast::Span,
) -> v04::ast::Block {
    if let Some(tail) = prefix.tail.take() {
        let tail_span = tail.span;
        prefix.statements.push(v04::ast::Node::new(
            v04::ast::StatementKind::BlockExpression(*tail),
            tail_span,
        ));
    }
    prefix.statements.append(&mut suffix.statements);
    prefix.tail = suffix.tail;
    prefix.span = span;
    prefix
}

fn block_with_tail(value: v04::ast::Expression) -> v04::ast::Block {
    v04::ast::Block {
        statements: Vec::new(),
        span: value.span,
        tail: Some(Box::new(value)),
    }
}

fn block_expression(block: v04::ast::Block, span: v04::ast::Span) -> v04::ast::Expression {
    v04::ast::Node::new(v04::ast::ExpressionKind::Block(block), span)
}

fn option_type(value: v04::ast::TypeExpression, span: v04::ast::Span) -> v04::ast::TypeExpression {
    v04::ast::Node::new(
        v04::ast::TypeExpressionKind::Path(v04::ast::TypePath {
            segments: vec![v04::ast::TypePathSegment {
                name: v04::ast::Identifier::new("Option", span),
                arguments: vec![value],
                span,
            }],
            span,
        }),
        span,
    )
}

fn some_pattern(name: String, span: v04::ast::Span) -> v04::ast::Pattern {
    v04::ast::Node::new(
        v04::ast::PatternKind::TupleConstructor {
            constructor: v04::ast::QualifiedName {
                segments: vec![v04::ast::Identifier::new("Some", span)],
                span,
            },
            arguments: vec![v04::ast::Node::new(
                v04::ast::PatternKind::Binder(v04::ast::Identifier::new(name, span)),
                span,
            )],
        },
        span,
    )
}

fn none_pattern(span: v04::ast::Span) -> v04::ast::Pattern {
    v04::ast::Node::new(
        v04::ast::PatternKind::Constructor(v04::ast::QualifiedName {
            segments: vec![v04::ast::Identifier::new("None", span)],
            span,
        }),
        span,
    )
}

fn match_expression(
    value: v04::ast::Expression,
    arms: Vec<(v04::ast::Pattern, v04::ast::Expression)>,
    span: v04::ast::Span,
) -> v04::ast::Expression {
    v04::ast::Node::new(
        v04::ast::ExpressionKind::Match(v04::ast::MatchExpression {
            value: Box::new(value),
            arms: arms
                .into_iter()
                .map(|(pattern, value)| v04::ast::MatchArm {
                    pattern,
                    value,
                    span,
                })
                .collect(),
        }),
        span,
    )
}

fn name_expression(name: String, span: v04::ast::Span) -> v04::ast::Expression {
    v04::ast::Node::new(
        v04::ast::ExpressionKind::Name(v04::ast::QualifiedName {
            segments: vec![v04::ast::Identifier::new(name, span)],
            span,
        }),
        span,
    )
}

fn call_expression(
    name: &str,
    arguments: Vec<v04::ast::Expression>,
    span: v04::ast::Span,
) -> v04::ast::Expression {
    v04::ast::Node::new(
        v04::ast::ExpressionKind::Call {
            callee: Box::new(name_expression(name.to_owned(), span)),
            arguments: arguments
                .into_iter()
                .map(v04::ast::CallArgument::Expression)
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
    span: v04::ast::Span,
) {
    diagnostics.push(error(
        code,
        rule,
        message,
        ast::SourceSpan::new(span.file, span.start, span.end),
    ));
}
