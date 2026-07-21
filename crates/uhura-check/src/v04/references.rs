//! Typed declaration-reference discovery for the Uhura 0.4 source tree.
//!
//! Resolution needs only the first segment of authored type/value paths and
//! component names. Keep that rule explicit here instead of inferring AST
//! shape from its serialized representation. Every reference-bearing container
//! enum is matched exhaustively so adding syntax cannot silently skip reference
//! discovery.

use std::collections::BTreeSet;

use uhura_syntax::v04;

pub(super) fn declaration_references(
    declaration: &v04::ast::Declaration,
) -> BTreeSet<(String, v04::ast::Span)> {
    let mut visitor = ReferenceVisitor {
        references: BTreeSet::new(),
        scopes: vec![declaration_bound_names(declaration)],
    };
    visitor.declaration(declaration);
    visitor.references
}

pub(super) fn declaration_root_references(
    source: &v04::ast::Module,
) -> BTreeSet<(String, v04::ast::Span)> {
    source
        .declarations
        .iter()
        .flat_map(declaration_references)
        .collect()
}

struct ReferenceVisitor {
    references: BTreeSet<(String, v04::ast::Span)>,
    scopes: Vec<BTreeSet<String>>,
}

impl ReferenceVisitor {
    fn declaration(&mut self, declaration: &v04::ast::Declaration) {
        match &declaration.kind {
            v04::ast::DeclarationKind::Machine(value) => self.machine(value),
            v04::ast::DeclarationKind::Part(value) => self.part(value),
            v04::ast::DeclarationKind::Ui(value) => self.ui(value),
            v04::ast::DeclarationKind::Scenario(value) => self.scenario(value),
            v04::ast::DeclarationKind::Example(value)
            | v04::ast::DeclarationKind::Checkpoint(value) => {
                self.evidence_reference(&value.target);
            }
            v04::ast::DeclarationKind::Struct(value) => {
                self.typed_fields(&value.fields);
            }
            v04::ast::DeclarationKind::Enum(value) => {
                for variant in &value.variants {
                    self.typed_fields(&variant.fields);
                }
            }
            v04::ast::DeclarationKind::Key(value) => self.ty(&value.value),
            v04::ast::DeclarationKind::Const(value) => self.const_declaration(value),
            v04::ast::DeclarationKind::Function(value) => self.function(value),
        }
    }

    fn machine(&mut self, machine: &v04::ast::MachineDeclaration) {
        for member in &machine.members {
            match &member.kind {
                v04::ast::MachineMemberKind::Config(value) => {
                    self.typed_fields(&value.fields);
                }
                v04::ast::MachineMemberKind::Require(value) => {
                    self.expression(&value.condition);
                }
                v04::ast::MachineMemberKind::Const(value) => {
                    self.const_declaration(value);
                }
                v04::ast::MachineMemberKind::Function(value) => self.function(value),
                v04::ast::MachineMemberKind::Part(value) => {
                    self.type_path(&value.part);
                    self.expressions(&value.arguments);
                }
                v04::ast::MachineMemberKind::Events(value)
                | v04::ast::MachineMemberKind::Commands(value) => {
                    self.protocol_section(value);
                }
                v04::ast::MachineMemberKind::Port(value) => self.port(value),
                v04::ast::MachineMemberKind::Outcomes(value) => self.outcomes(value),
                v04::ast::MachineMemberKind::State(value) => self.state(value),
                v04::ast::MachineMemberKind::Computed(value) => self.computed(value),
                v04::ast::MachineMemberKind::Invariant(value) => {
                    self.expressions(&value.conditions);
                }
                v04::ast::MachineMemberKind::Observe(value) => self.observe(value),
                v04::ast::MachineMemberKind::Handler(value) => self.handler(value),
                v04::ast::MachineMemberKind::Update(value) => self.update(value),
                v04::ast::MachineMemberKind::BeforeCommit(value) => self.block(&value.body),
            }
        }
    }

    fn part(&mut self, part: &v04::ast::PartDeclaration) {
        self.parameters(&part.parameters);
        for member in &part.members {
            match &member.kind {
                v04::ast::PartMemberKind::Require(value) => {
                    self.expression(&value.condition);
                }
                v04::ast::PartMemberKind::RequiresOutcomes(value) => self.outcomes(value),
                v04::ast::PartMemberKind::Const(value) => self.const_declaration(value),
                v04::ast::PartMemberKind::Function(value) => self.function(value),
                v04::ast::PartMemberKind::Events(value)
                | v04::ast::PartMemberKind::Commands(value) => self.protocol_section(value),
                v04::ast::PartMemberKind::Port(value) => self.port(value),
                v04::ast::PartMemberKind::State(value) => self.state(value),
                v04::ast::PartMemberKind::Computed(value) => self.computed(value),
                v04::ast::PartMemberKind::Invariant(value) => {
                    self.expressions(&value.conditions);
                }
                v04::ast::PartMemberKind::Observe(value) => self.observe(value),
                v04::ast::PartMemberKind::Handler(value) => self.handler(value),
                v04::ast::PartMemberKind::Update(value) => self.update(value),
            }
        }
    }

    fn ui(&mut self, ui: &v04::ast::UiDeclaration) {
        self.type_path(&ui.machine);
        self.ui_nodes(&ui.body.nodes);
    }

    fn scenario(&mut self, scenario: &v04::ast::ScenarioDeclaration) {
        match &scenario.origin {
            v04::ast::ScenarioOrigin::Machine {
                machine,
                configuration,
            } => {
                self.type_path(machine);
                if let Some(configuration) = configuration {
                    self.expression(configuration);
                }
            }
            v04::ast::ScenarioOrigin::Snapshot(reference) => {
                self.evidence_reference(reference);
            }
        }
        for step in &scenario.steps {
            match &step.kind {
                v04::ast::EvidenceStepKind::Bind { fixture, .. } => self.expression(fixture),
                v04::ast::EvidenceStepKind::Start | v04::ast::EvidenceStepKind::Pin(_) => {}
                v04::ast::EvidenceStepKind::Send(value)
                | v04::ast::EvidenceStepKind::Deliver(value)
                | v04::ast::EvidenceStepKind::ExpectObservationWhere(value) => {
                    self.expression(value);
                }
                v04::ast::EvidenceStepKind::ExpectReaction { outcome, commands } => {
                    self.pattern_references(outcome);
                    self.expressions(commands);
                }
                v04::ast::EvidenceStepKind::ExpectObservationPattern(value)
                | v04::ast::EvidenceStepKind::ExpectInspectionPattern(value) => {
                    self.pattern_references(value);
                }
                v04::ast::EvidenceStepKind::ExpectRestore { commands } => {
                    self.expressions(commands);
                }
                v04::ast::EvidenceStepKind::ExpectSnapshot { target } => {
                    self.evidence_reference(target);
                }
            }
        }
    }

    fn evidence_reference(&mut self, reference: &v04::ast::EvidenceReference) {
        if let Some(first) = reference.path.first() {
            self.reference(first);
        }
    }

    fn const_declaration(&mut self, value: &v04::ast::ConstDeclaration) {
        self.ty(&value.ty);
        self.expression(&value.value);
    }

    fn function(&mut self, value: &v04::ast::FunctionDeclaration) {
        self.parameters(&value.parameters);
        self.ty(&value.result);
        self.push_scope(value.parameters.iter().map(|parameter| &parameter.name));
        self.block(&value.body);
        self.pop_scope();
    }

    fn update(&mut self, value: &v04::ast::UpdateDeclaration) {
        self.parameters(&value.parameters);
        if let Some(result) = &value.result {
            self.ty(result);
        }
        self.push_scope(value.parameters.iter().map(|parameter| &parameter.name));
        self.block(&value.body);
        self.pop_scope();
    }

    fn handler(&mut self, value: &v04::ast::HandlerDeclaration) {
        for pattern in &value.parameters {
            self.pattern_references(pattern);
        }
        let mut bindings = BTreeSet::new();
        for pattern in &value.parameters {
            pattern_bindings(pattern, &mut bindings);
        }
        self.scopes.push(bindings);
        self.block(&value.body);
        self.pop_scope();
    }

    fn protocol_section(&mut self, value: &v04::ast::ProtocolSection) {
        for variant in &value.variants {
            self.parameters(&variant.parameters);
        }
    }

    fn outcomes(&mut self, value: &v04::ast::OutcomeSection) {
        for entry in &value.entries {
            self.parameters(&entry.variant.parameters);
        }
    }

    fn port(&mut self, value: &v04::ast::PortDeclaration) {
        self.type_path(&value.contract);
        self.field_initializers(&value.fields);
    }

    fn state(&mut self, value: &v04::ast::StateSection) {
        for field in &value.fields {
            self.ty(&field.ty);
            self.expression(&field.initial);
        }
    }

    fn computed(&mut self, value: &v04::ast::ComputedDeclaration) {
        if let Some(ty) = &value.ty {
            self.ty(ty);
        }
        self.expression(&value.value);
    }

    fn observe(&mut self, value: &v04::ast::ObserveSection) {
        for field in &value.fields {
            if let Some(expression) = &field.value {
                self.expression(expression);
            }
        }
    }

    fn parameters(&mut self, parameters: &[v04::ast::Parameter]) {
        for parameter in parameters {
            self.ty(&parameter.ty);
        }
    }

    fn typed_fields(&mut self, fields: &[v04::ast::TypedField]) {
        for field in fields {
            self.ty(&field.ty);
        }
    }

    fn ty(&mut self, ty: &v04::ast::TypeExpression) {
        match &ty.kind {
            v04::ast::TypeExpressionKind::Path(path) => self.type_path(path),
            v04::ast::TypeExpressionKind::Unit => {}
            v04::ast::TypeExpressionKind::Tuple(values) => {
                for value in values {
                    self.ty(value);
                }
            }
        }
    }

    fn type_path(&mut self, path: &v04::ast::TypePath) {
        if let Some(first) = path.segments.first() {
            self.reference(&first.name);
        }
        for segment in &path.segments {
            for argument in &segment.arguments {
                self.ty(argument);
            }
        }
    }

    fn block(&mut self, block: &v04::ast::Block) {
        self.scopes.push(BTreeSet::new());
        for statement in &block.statements {
            self.statement(statement);
            if let v04::ast::StatementKind::Let { name, .. } = &statement.kind {
                self.scopes
                    .last_mut()
                    .expect("a block lexical scope is active")
                    .insert(name.text.clone());
            }
        }
        if let Some(tail) = &block.tail {
            self.expression(tail);
        }
        self.pop_scope();
    }

    fn statement(&mut self, statement: &v04::ast::Statement) {
        match &statement.kind {
            v04::ast::StatementKind::Let { ty, value, .. } => {
                if let Some(ty) = ty {
                    self.ty(ty);
                }
                self.expression(value);
            }
            v04::ast::StatementKind::Assign { value, .. } => self.expression(value),
            v04::ast::StatementKind::Emit { output, .. } => {
                self.expressions(&output.arguments);
            }
            v04::ast::StatementKind::While {
                condition,
                decreases,
                body,
            } => {
                self.expression(condition);
                self.expression(decreases);
                self.block(body);
            }
            v04::ast::StatementKind::Unreachable { .. } => {}
            v04::ast::StatementKind::Expression { expression, .. }
            | v04::ast::StatementKind::BlockExpression(expression) => {
                self.expression(expression);
            }
        }
    }

    fn expressions(&mut self, expressions: &[v04::ast::Expression]) {
        for expression in expressions {
            self.expression(expression);
        }
    }

    fn expression(&mut self, expression: &v04::ast::Expression) {
        match &expression.kind {
            v04::ast::ExpressionKind::Literal(value) => self.literal(value),
            v04::ast::ExpressionKind::Unit => {}
            v04::ast::ExpressionKind::Sequence(values)
            | v04::ast::ExpressionKind::Tuple(values) => self.expressions(values),
            v04::ast::ExpressionKind::Group(value) => self.expression(value),
            v04::ast::ExpressionKind::Name(value) => self.qualified_name(value),
            v04::ast::ExpressionKind::Record(value) => {
                self.qualified_name(&value.constructor);
                self.field_initializers(&value.fields);
                if let Some(base) = &value.base {
                    self.expression(base);
                }
            }
            v04::ast::ExpressionKind::AnonymousRecord(entries) => {
                for entry in entries {
                    self.expression(&entry.key);
                    self.expression(&entry.value);
                }
            }
            v04::ast::ExpressionKind::Block(value) => self.block(value),
            v04::ast::ExpressionKind::Call { callee, arguments } => {
                self.expression(callee);
                for argument in arguments {
                    match argument {
                        v04::ast::CallArgument::Expression(value) => self.expression(value),
                        v04::ast::CallArgument::Binder(value) => {
                            self.push_scope(std::iter::once(&value.parameter));
                            self.expression(&value.body);
                            self.pop_scope();
                        }
                    }
                }
            }
            v04::ast::ExpressionKind::Member { value, .. }
            | v04::ast::ExpressionKind::Unary { value, .. } => self.expression(value),
            v04::ast::ExpressionKind::Index { value, index } => {
                self.expression(value);
                self.expression(index);
            }
            v04::ast::ExpressionKind::Binary { left, right, .. }
            | v04::ast::ExpressionKind::Compare { left, right, .. } => {
                self.expression(left);
                self.expression(right);
            }
            v04::ast::ExpressionKind::Is { value, pattern } => {
                self.expression(value);
                self.pattern_references(pattern);
            }
            v04::ast::ExpressionKind::If(value) => {
                self.expression(&value.condition);
                self.block(&value.then_branch);
                if let Some(branch) = &value.else_branch {
                    match branch {
                        v04::ast::ElseBranch::Block(value) => self.block(value),
                        v04::ast::ElseBranch::If(value) => self.expression(value),
                    }
                }
            }
            v04::ast::ExpressionKind::Match(value) => {
                self.expression(&value.value);
                for arm in &value.arms {
                    self.pattern_references(&arm.pattern);
                    let mut bindings = BTreeSet::new();
                    pattern_bindings(&arm.pattern, &mut bindings);
                    self.scopes.push(bindings);
                    self.expression(&arm.value);
                    self.pop_scope();
                }
            }
            v04::ast::ExpressionKind::Return(value) => {
                if let Some(value) = value {
                    self.expression(value);
                }
            }
        }
    }

    fn field_initializers(&mut self, fields: &[v04::ast::FieldInitializer]) {
        for field in fields {
            if let Some(value) = &field.value {
                self.expression(value);
            } else {
                self.reference(&field.name);
            }
        }
    }

    fn literal(&self, literal: &v04::ast::Literal) {
        match literal {
            v04::ast::Literal::Bool(_)
            | v04::ast::Literal::Integer { .. }
            | v04::ast::Literal::Decimal { .. }
            | v04::ast::Literal::Text { .. } => {}
        }
    }

    fn qualified_name(&mut self, name: &v04::ast::QualifiedName) {
        if let Some(first) = name.segments.first() {
            self.reference(first);
        }
    }

    fn pattern_references(&mut self, pattern: &v04::ast::Pattern) {
        match &pattern.kind {
            v04::ast::PatternKind::Wildcard | v04::ast::PatternKind::Binder(_) => {}
            v04::ast::PatternKind::Literal(value) => self.pattern_literal(value),
            v04::ast::PatternKind::Group(value) => self.pattern_references(value),
            v04::ast::PatternKind::Tuple(values) | v04::ast::PatternKind::Alternative(values) => {
                for value in values {
                    self.pattern_references(value);
                }
            }
            v04::ast::PatternKind::Constructor(value) => self.qualified_name(value),
            v04::ast::PatternKind::TupleConstructor {
                constructor,
                arguments,
            } => {
                self.qualified_name(constructor);
                for argument in arguments {
                    self.pattern_references(argument);
                }
            }
            v04::ast::PatternKind::Record {
                constructor,
                fields,
                rest: _,
            } => {
                self.qualified_name(constructor);
                for field in fields {
                    if let Some(pattern) = &field.pattern {
                        self.pattern_references(pattern);
                    }
                }
            }
            v04::ast::PatternKind::AnonymousRecord { fields, .. } => {
                for field in fields {
                    if let Some(pattern) = &field.pattern {
                        self.pattern_references(pattern);
                    }
                }
            }
        }
    }

    fn pattern_literal(&self, literal: &v04::ast::PatternLiteral) {
        match literal {
            v04::ast::PatternLiteral::Bool(_)
            | v04::ast::PatternLiteral::Integer { .. }
            | v04::ast::PatternLiteral::Decimal { .. }
            | v04::ast::PatternLiteral::Text { .. }
            | v04::ast::PatternLiteral::Unit => {}
        }
    }

    fn ui_nodes(&mut self, nodes: &[v04::ast::UiNode]) {
        for node in nodes {
            match &node.kind {
                v04::ast::UiNodeKind::Text(_) | v04::ast::UiNodeKind::Comment(_) => {}
                v04::ast::UiNodeKind::Interpolation(value) => self.expression(value),
                v04::ast::UiNodeKind::Element(value) => {
                    match value.name.kind {
                        v04::ast::UiNameKind::Native => {}
                        v04::ast::UiNameKind::Component => self.reference_ui_name(&value.name),
                    }
                    for attribute in &value.attributes {
                        match attribute {
                            v04::ast::UiAttribute::Boolean { .. }
                            | v04::ast::UiAttribute::StaticText { .. } => {}
                            v04::ast::UiAttribute::Expression { value, .. }
                            | v04::ast::UiAttribute::Event { input: value, .. } => {
                                self.expression(value);
                            }
                        }
                    }
                    self.ui_nodes(&value.children);
                }
                v04::ast::UiNodeKind::If(value) => {
                    self.expression(&value.condition);
                    self.ui_nodes(&value.then_branch);
                    if let Some(branch) = &value.else_branch {
                        self.ui_nodes(branch);
                    }
                }
                v04::ast::UiNodeKind::Each(value) => {
                    self.expression(&value.source);
                    self.pattern_references(&value.pattern);
                    let mut bindings = BTreeSet::new();
                    pattern_bindings(&value.pattern, &mut bindings);
                    self.scopes.push(bindings);
                    self.expression(&value.key);
                    self.ui_nodes(&value.children);
                    self.pop_scope();
                }
            }
        }
    }

    fn reference(&mut self, identifier: &v04::ast::Identifier) {
        if !self.is_bound(&identifier.text) {
            self.references
                .insert((identifier.text.clone(), identifier.span));
        }
    }

    fn reference_ui_name(&mut self, name: &v04::ast::UiName) {
        if !self.is_bound(&name.text) {
            self.references.insert((name.text.clone(), name.span));
        }
    }

    fn is_bound(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(name))
    }

    fn push_scope<'a>(&mut self, names: impl IntoIterator<Item = &'a v04::ast::Identifier>) {
        self.scopes
            .push(names.into_iter().map(|name| name.text.clone()).collect());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop().expect("a lexical scope is active");
    }
}

fn pattern_bindings(pattern: &v04::ast::Pattern, names: &mut BTreeSet<String>) {
    match &pattern.kind {
        v04::ast::PatternKind::Wildcard | v04::ast::PatternKind::Literal(_) => {}
        v04::ast::PatternKind::Binder(value) => {
            names.insert(value.text.clone());
        }
        v04::ast::PatternKind::Group(value) => pattern_bindings(value, names),
        v04::ast::PatternKind::Tuple(values) | v04::ast::PatternKind::Alternative(values) => {
            for value in values {
                pattern_bindings(value, names);
            }
        }
        v04::ast::PatternKind::Constructor(_) => {}
        v04::ast::PatternKind::TupleConstructor { arguments, .. } => {
            for argument in arguments {
                pattern_bindings(argument, names);
            }
        }
        v04::ast::PatternKind::Record { fields, .. }
        | v04::ast::PatternKind::AnonymousRecord { fields, .. } => {
            for field in fields {
                if let Some(pattern) = &field.pattern {
                    pattern_bindings(pattern, names);
                } else {
                    names.insert(field.name.text.clone());
                }
            }
        }
    }
}

fn declaration_bound_names(declaration: &v04::ast::Declaration) -> BTreeSet<String> {
    fn machine_member_names(members: &[v04::ast::MachineMember], names: &mut BTreeSet<String>) {
        for member in members {
            match &member.kind {
                v04::ast::MachineMemberKind::Config(value) => {
                    names.extend(value.fields.iter().map(|field| field.name.text.clone()));
                }
                v04::ast::MachineMemberKind::Const(value) => {
                    names.insert(value.name.text.clone());
                }
                v04::ast::MachineMemberKind::Function(value) => {
                    names.insert(value.name.text.clone());
                }
                v04::ast::MachineMemberKind::Part(value) => {
                    names.insert(value.name.text.clone());
                }
                v04::ast::MachineMemberKind::Port(value) => {
                    names.insert(value.name.text.clone());
                }
                v04::ast::MachineMemberKind::State(value) => {
                    names.extend(value.fields.iter().map(|field| field.name.text.clone()));
                }
                v04::ast::MachineMemberKind::Computed(value) => {
                    names.insert(value.name.text.clone());
                }
                v04::ast::MachineMemberKind::Update(value) => {
                    names.insert(value.name.text.clone());
                }
                v04::ast::MachineMemberKind::Require(_)
                | v04::ast::MachineMemberKind::Events(_)
                | v04::ast::MachineMemberKind::Commands(_)
                | v04::ast::MachineMemberKind::Outcomes(_)
                | v04::ast::MachineMemberKind::Invariant(_)
                | v04::ast::MachineMemberKind::Observe(_)
                | v04::ast::MachineMemberKind::Handler(_)
                | v04::ast::MachineMemberKind::BeforeCommit(_) => {}
            }
        }
    }

    fn part_member_names(members: &[v04::ast::PartMember], names: &mut BTreeSet<String>) {
        for member in members {
            match &member.kind {
                v04::ast::PartMemberKind::Const(value) => {
                    names.insert(value.name.text.clone());
                }
                v04::ast::PartMemberKind::Function(value) => {
                    names.insert(value.name.text.clone());
                }
                v04::ast::PartMemberKind::Port(value) => {
                    names.insert(value.name.text.clone());
                }
                v04::ast::PartMemberKind::State(value) => {
                    names.extend(value.fields.iter().map(|field| field.name.text.clone()));
                }
                v04::ast::PartMemberKind::Computed(value) => {
                    names.insert(value.name.text.clone());
                }
                v04::ast::PartMemberKind::Update(value) => {
                    names.insert(value.name.text.clone());
                }
                v04::ast::PartMemberKind::Require(_)
                | v04::ast::PartMemberKind::RequiresOutcomes(_)
                | v04::ast::PartMemberKind::Events(_)
                | v04::ast::PartMemberKind::Commands(_)
                | v04::ast::PartMemberKind::Invariant(_)
                | v04::ast::PartMemberKind::Observe(_)
                | v04::ast::PartMemberKind::Handler(_) => {}
            }
        }
    }

    let mut names = BTreeSet::new();
    match &declaration.kind {
        v04::ast::DeclarationKind::Function(_)
        | v04::ast::DeclarationKind::Struct(_)
        | v04::ast::DeclarationKind::Enum(_)
        | v04::ast::DeclarationKind::Key(_)
        | v04::ast::DeclarationKind::Const(_)
        | v04::ast::DeclarationKind::Scenario(_)
        | v04::ast::DeclarationKind::Example(_)
        | v04::ast::DeclarationKind::Checkpoint(_) => {}
        v04::ast::DeclarationKind::Machine(value) => {
            machine_member_names(&value.members, &mut names);
        }
        v04::ast::DeclarationKind::Part(value) => {
            names.extend(
                value
                    .parameters
                    .iter()
                    .map(|parameter| parameter.name.text.clone()),
            );
            part_member_names(&value.members, &mut names);
        }
        v04::ast::DeclarationKind::Ui(value) => {
            names.insert(value.observation.text.clone());
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use uhura_syntax::v04::{SourceIdentity, parse};

    fn parsed_references(source: &str) -> Vec<(String, String)> {
        let parsed = parse(
            SourceIdentity::new(71, "references.test@1", "main", "main.uhura"),
            source,
        );
        assert!(
            parsed.diagnostics.is_empty(),
            "unexpected parser diagnostics:\n{:#?}",
            parsed.diagnostics
        );
        parsed
            .module
            .declarations
            .iter()
            .flat_map(declaration_references)
            .map(|(name, span)| {
                (
                    name,
                    source[span.start as usize..span.end as usize].to_owned(),
                )
            })
            .collect()
    }

    #[test]
    fn visits_every_declaration_family_and_ui_surface() {
        let references = parsed_references(
            r#"
use uhura::ui;

pub struct Envelope { payload: Payload }
pub enum Choice { Some { value: Payload, }, None, }
pub key Ticket(ExternalId);
pub const DEFAULT: Payload = make_payload();
pub fn convert(value: Payload) -> Result { helper(value) }

pub part Worker(settings: Config) {
  require enabled(settings);
  state { payload: Payload = initial_payload() }
  computed ready: Bool = validate(payload);
}

pub machine App {
  config { settings: Config }
  require enabled(settings);
  events { Start(payload: Payload), }
  outcomes { commit Done(result: Result), }
  state { payload: Payload = initial_payload() }
  computed ready: Bool = validate(payload);
  observe { ready }
  on Start(next) { payload = next; Done(convert(next)) }
}

pub ui Screen for App(view) {
  <Panel value={format(view.ready)} on press -> Start(DEFAULT) />
}
"#,
        );
        let names = references
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<BTreeSet<_>>();
        for expected in [
            "Payload",
            "ExternalId",
            "make_payload",
            "Result",
            "helper",
            "Config",
            "enabled",
            "initial_payload",
            "Bool",
            "validate",
            "App",
            "Panel",
            "format",
            "Start",
            "DEFAULT",
        ] {
            assert!(
                names.contains(expected),
                "missing `{expected}` in {names:#?}"
            );
        }
        for bound in ["value", "settings", "payload", "ready", "view", "next"] {
            assert!(!names.contains(bound), "bound name leaked: `{bound}`");
        }
    }

    #[test]
    fn lexical_scopes_follow_lambdas_lets_and_disjoint_blocks() {
        let source = r#"
pub fn choose(flag: Bool, items: Seq<Int>) -> Int {
  if flag {
    let value = items.map(|value| helper(value));
    value.len()
  } else {
    value()
  }
}
"#;
        let references = parsed_references(source);
        let value_references = references
            .iter()
            .filter(|(name, _)| name == "value")
            .collect::<Vec<_>>();
        assert_eq!(value_references, vec![&("value".into(), "value".into())]);
        assert!(references.iter().any(|(name, _)| name == "helper"));
        assert!(!references.iter().any(|(name, _)| name == "flag"));
        assert!(!references.iter().any(|(name, _)| name == "items"));
    }

    #[test]
    fn pattern_binders_shadow_only_their_arm_and_ui_each_body() {
        let references = parsed_references(
            r#"
use uhura::ui;

pub fn unwrap(input: Option<Item>) -> Item {
  match input {
    Some(item) => item,
    Record { field, nested: Some(other), .. } => other,
    None => fallback(),
  }
}

pub ui List for App(view) {
  {#each view.items as Row { id, .. } (id)}
    <Card value={render(id)} />
  {/each}
}
"#,
        );
        let names = references
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<BTreeSet<_>>();
        for expected in [
            "Option", "Item", "Some", "Record", "None", "fallback", "App", "Row", "Card", "render",
        ] {
            assert!(
                names.contains(expected),
                "missing `{expected}` in {names:#?}"
            );
        }
        for bound in ["input", "item", "field", "other", "view", "id"] {
            assert!(!names.contains(bound), "pattern binding leaked: `{bound}`");
        }
    }
}
