//! Typed declaration-reference discovery for the canonical source tree.
//!
//! Resolution needs only the first segment of authored type/value paths and
//! component names. Keep that rule explicit here instead of inferring AST
//! shape from its serialized representation. Every reference-bearing container
//! enum is matched exhaustively so adding syntax cannot silently skip reference
//! discovery.

use std::collections::BTreeSet;

pub(super) fn declaration_references(
    declaration: &uhura_syntax::ast::Declaration,
) -> BTreeSet<(String, uhura_syntax::ast::Span)> {
    let mut visitor = ReferenceVisitor {
        references: BTreeSet::new(),
        scopes: vec![declaration_bound_names(declaration)],
    };
    visitor.declaration(declaration);
    visitor.references
}

pub(super) fn declaration_root_references(
    source: &uhura_syntax::ast::Module,
) -> BTreeSet<(String, uhura_syntax::ast::Span)> {
    source
        .declarations
        .iter()
        .flat_map(declaration_references)
        .collect()
}

struct ReferenceVisitor {
    references: BTreeSet<(String, uhura_syntax::ast::Span)>,
    scopes: Vec<BTreeSet<String>>,
}

impl ReferenceVisitor {
    fn declaration(&mut self, declaration: &uhura_syntax::ast::Declaration) {
        match &declaration.kind {
            uhura_syntax::ast::DeclarationKind::Machine(value) => self.machine(value),
            uhura_syntax::ast::DeclarationKind::Part(value) => self.part(value),
            uhura_syntax::ast::DeclarationKind::Ui(value) => self.ui(value),
            uhura_syntax::ast::DeclarationKind::Scenario(value) => self.scenario(value),
            uhura_syntax::ast::DeclarationKind::Example(value)
            | uhura_syntax::ast::DeclarationKind::Checkpoint(value) => {
                if let Some(arguments) = &value.arguments {
                    for argument in arguments {
                        self.expression(&argument.value);
                    }
                }
                self.evidence_reference(&value.target);
            }
            uhura_syntax::ast::DeclarationKind::Struct(value) => {
                self.typed_fields(&value.fields);
            }
            uhura_syntax::ast::DeclarationKind::Enum(value) => {
                for variant in &value.variants {
                    self.typed_fields(&variant.fields);
                }
            }
            uhura_syntax::ast::DeclarationKind::Key(value) => self.ty(&value.value),
            uhura_syntax::ast::DeclarationKind::Const(value) => self.const_declaration(value),
            uhura_syntax::ast::DeclarationKind::Function(value) => self.function(value),
        }
    }

    fn machine(&mut self, machine: &uhura_syntax::ast::MachineDeclaration) {
        for member in &machine.members {
            match &member.kind {
                uhura_syntax::ast::MachineMemberKind::Config(value) => {
                    self.typed_fields(&value.fields);
                }
                uhura_syntax::ast::MachineMemberKind::Require(value) => {
                    self.expression(&value.condition);
                }
                uhura_syntax::ast::MachineMemberKind::Const(value) => {
                    self.const_declaration(value);
                }
                uhura_syntax::ast::MachineMemberKind::Function(value) => self.function(value),
                uhura_syntax::ast::MachineMemberKind::Part(value) => {
                    self.type_path(&value.part);
                    self.expressions(&value.arguments);
                }
                uhura_syntax::ast::MachineMemberKind::Events(value)
                | uhura_syntax::ast::MachineMemberKind::Commands(value) => {
                    self.protocol_section(value);
                }
                uhura_syntax::ast::MachineMemberKind::Port(value) => self.port(value),
                uhura_syntax::ast::MachineMemberKind::Outcomes(value) => self.outcomes(value),
                uhura_syntax::ast::MachineMemberKind::State(value) => self.state(value),
                uhura_syntax::ast::MachineMemberKind::Computed(value) => self.computed(value),
                uhura_syntax::ast::MachineMemberKind::Invariant(value) => {
                    self.expressions(&value.conditions);
                }
                uhura_syntax::ast::MachineMemberKind::Observe(value) => self.observe(value),
                uhura_syntax::ast::MachineMemberKind::Handler(value) => self.handler(value),
                uhura_syntax::ast::MachineMemberKind::Update(value) => self.update(value),
                uhura_syntax::ast::MachineMemberKind::BeforeCommit(value) => {
                    self.block(&value.body)
                }
            }
        }
    }

    fn part(&mut self, part: &uhura_syntax::ast::PartDeclaration) {
        self.parameters(&part.parameters);
        for member in &part.members {
            match &member.kind {
                uhura_syntax::ast::PartMemberKind::Require(value) => {
                    self.expression(&value.condition);
                }
                uhura_syntax::ast::PartMemberKind::RequiresOutcomes(value) => self.outcomes(value),
                uhura_syntax::ast::PartMemberKind::Const(value) => self.const_declaration(value),
                uhura_syntax::ast::PartMemberKind::Function(value) => self.function(value),
                uhura_syntax::ast::PartMemberKind::Events(value)
                | uhura_syntax::ast::PartMemberKind::Commands(value) => {
                    self.protocol_section(value)
                }
                uhura_syntax::ast::PartMemberKind::Port(value) => self.port(value),
                uhura_syntax::ast::PartMemberKind::State(value) => self.state(value),
                uhura_syntax::ast::PartMemberKind::Computed(value) => self.computed(value),
                uhura_syntax::ast::PartMemberKind::Invariant(value) => {
                    self.expressions(&value.conditions);
                }
                uhura_syntax::ast::PartMemberKind::Observe(value) => self.observe(value),
                uhura_syntax::ast::PartMemberKind::Handler(value) => self.handler(value),
                uhura_syntax::ast::PartMemberKind::Update(value) => self.update(value),
            }
        }
    }

    fn ui(&mut self, ui: &uhura_syntax::ast::UiDeclaration) {
        match &ui.binding {
            uhura_syntax::ast::UiBinding::Machine { machine, .. } => self.type_path(machine),
            uhura_syntax::ast::UiBinding::Component { parameters, emits } => {
                self.parameters(parameters);
                self.protocol_section(emits);
            }
        }
        self.ui_nodes(&ui.body.nodes);
    }

    fn scenario(&mut self, scenario: &uhura_syntax::ast::ScenarioDeclaration) {
        match &scenario.origin {
            uhura_syntax::ast::ScenarioOrigin::Machine {
                machine,
                configuration,
            } => {
                self.type_path(machine);
                if let Some(configuration) = configuration {
                    self.expression(configuration);
                }
            }
            uhura_syntax::ast::ScenarioOrigin::Snapshot(reference) => {
                self.evidence_reference(reference);
            }
        }
        for step in &scenario.steps {
            match &step.kind {
                uhura_syntax::ast::EvidenceStepKind::Bind { fixture, .. } => {
                    self.expression(fixture)
                }
                uhura_syntax::ast::EvidenceStepKind::Start
                | uhura_syntax::ast::EvidenceStepKind::Pin(_) => {}
                uhura_syntax::ast::EvidenceStepKind::Send(value)
                | uhura_syntax::ast::EvidenceStepKind::Deliver(value)
                | uhura_syntax::ast::EvidenceStepKind::ExpectObservationWhere(value) => {
                    self.expression(value);
                }
                uhura_syntax::ast::EvidenceStepKind::ExpectReaction { outcome, commands } => {
                    self.pattern_references(outcome);
                    self.expressions(commands);
                }
                uhura_syntax::ast::EvidenceStepKind::ExpectObservationPattern(value)
                | uhura_syntax::ast::EvidenceStepKind::ExpectInspectionPattern(value) => {
                    self.pattern_references(value);
                }
                uhura_syntax::ast::EvidenceStepKind::ExpectRestore { commands } => {
                    self.expressions(commands);
                }
                uhura_syntax::ast::EvidenceStepKind::ExpectSnapshot { target } => {
                    self.evidence_reference(target);
                }
            }
        }
    }

    fn evidence_reference(&mut self, reference: &uhura_syntax::ast::EvidenceReference) {
        if reference.root == uhura_syntax::ast::EvidenceReferenceRoot::Crate {
            return;
        }
        if let Some(first) = reference.path.first() {
            self.reference(first);
        }
    }

    fn const_declaration(&mut self, value: &uhura_syntax::ast::ConstDeclaration) {
        self.ty(&value.ty);
        self.expression(&value.value);
    }

    fn function(&mut self, value: &uhura_syntax::ast::FunctionDeclaration) {
        self.parameters(&value.parameters);
        self.ty(&value.result);
        self.push_scope(value.parameters.iter().map(|parameter| &parameter.name));
        self.block(&value.body);
        self.pop_scope();
    }

    fn update(&mut self, value: &uhura_syntax::ast::UpdateDeclaration) {
        self.parameters(&value.parameters);
        if let Some(result) = &value.result {
            self.ty(result);
        }
        self.push_scope(value.parameters.iter().map(|parameter| &parameter.name));
        self.block(&value.body);
        self.pop_scope();
    }

    fn handler(&mut self, value: &uhura_syntax::ast::HandlerDeclaration) {
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

    fn protocol_section(&mut self, value: &uhura_syntax::ast::ProtocolSection) {
        for variant in &value.variants {
            self.parameters(&variant.parameters);
        }
    }

    fn outcomes(&mut self, value: &uhura_syntax::ast::OutcomeSection) {
        for entry in &value.entries {
            self.parameters(&entry.variant.parameters);
        }
    }

    fn port(&mut self, value: &uhura_syntax::ast::PortDeclaration) {
        self.type_path(&value.contract);
        self.field_initializers(&value.fields);
    }

    fn state(&mut self, value: &uhura_syntax::ast::StateSection) {
        for field in &value.fields {
            self.ty(&field.ty);
            self.expression(&field.initial);
        }
    }

    fn computed(&mut self, value: &uhura_syntax::ast::ComputedDeclaration) {
        if let Some(ty) = &value.ty {
            self.ty(ty);
        }
        self.expression(&value.value);
    }

    fn observe(&mut self, value: &uhura_syntax::ast::ObserveSection) {
        for field in &value.fields {
            if let Some(expression) = &field.value {
                self.expression(expression);
            }
        }
    }

    fn parameters(&mut self, parameters: &[uhura_syntax::ast::Parameter]) {
        for parameter in parameters {
            self.ty(&parameter.ty);
        }
    }

    fn typed_fields(&mut self, fields: &[uhura_syntax::ast::TypedField]) {
        for field in fields {
            self.ty(&field.ty);
        }
    }

    fn ty(&mut self, ty: &uhura_syntax::ast::TypeExpression) {
        match &ty.kind {
            uhura_syntax::ast::TypeExpressionKind::Path(path) => self.type_path(path),
            uhura_syntax::ast::TypeExpressionKind::Unit => {}
            uhura_syntax::ast::TypeExpressionKind::Tuple(values) => {
                for value in values {
                    self.ty(value);
                }
            }
        }
    }

    fn type_path(&mut self, path: &uhura_syntax::ast::TypePath) {
        if let Some(first) = path.segments.first() {
            self.reference(&first.name);
        }
        for segment in &path.segments {
            for argument in &segment.arguments {
                self.ty(argument);
            }
        }
    }

    fn block(&mut self, block: &uhura_syntax::ast::Block) {
        self.scopes.push(BTreeSet::new());
        for statement in &block.statements {
            self.statement(statement);
            if let uhura_syntax::ast::StatementKind::Let { name, .. } = &statement.kind {
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

    fn statement(&mut self, statement: &uhura_syntax::ast::Statement) {
        match &statement.kind {
            uhura_syntax::ast::StatementKind::Let { ty, value, .. } => {
                if let Some(ty) = ty {
                    self.ty(ty);
                }
                self.expression(value);
            }
            uhura_syntax::ast::StatementKind::Assign { value, .. } => self.expression(value),
            uhura_syntax::ast::StatementKind::Emit { output, .. } => {
                self.expressions(&output.arguments);
            }
            uhura_syntax::ast::StatementKind::While {
                condition,
                decreases,
                body,
            } => {
                self.expression(condition);
                self.expression(decreases);
                self.block(body);
            }
            uhura_syntax::ast::StatementKind::Unreachable { .. } => {}
            uhura_syntax::ast::StatementKind::Expression { expression, .. }
            | uhura_syntax::ast::StatementKind::BlockExpression(expression) => {
                self.expression(expression);
            }
        }
    }

    fn expressions(&mut self, expressions: &[uhura_syntax::ast::Expression]) {
        for expression in expressions {
            self.expression(expression);
        }
    }

    fn expression(&mut self, expression: &uhura_syntax::ast::Expression) {
        match &expression.kind {
            uhura_syntax::ast::ExpressionKind::Literal(value) => self.literal(value),
            uhura_syntax::ast::ExpressionKind::Unit => {}
            uhura_syntax::ast::ExpressionKind::Sequence(values)
            | uhura_syntax::ast::ExpressionKind::Tuple(values) => self.expressions(values),
            uhura_syntax::ast::ExpressionKind::Group(value) => self.expression(value),
            uhura_syntax::ast::ExpressionKind::Name(value) => self.qualified_name(value),
            uhura_syntax::ast::ExpressionKind::Record(value) => {
                self.qualified_name(&value.constructor);
                self.field_initializers(&value.fields);
                if let Some(base) = &value.base {
                    self.expression(base);
                }
            }
            uhura_syntax::ast::ExpressionKind::AnonymousRecord(entries) => {
                for entry in entries {
                    self.expression(&entry.key);
                    self.expression(&entry.value);
                }
            }
            uhura_syntax::ast::ExpressionKind::Block(value) => self.block(value),
            uhura_syntax::ast::ExpressionKind::Call { callee, arguments } => {
                self.expression(callee);
                for argument in arguments {
                    match argument {
                        uhura_syntax::ast::CallArgument::Expression(value) => {
                            self.expression(value)
                        }
                        uhura_syntax::ast::CallArgument::Binder(value) => {
                            self.push_scope(std::iter::once(&value.parameter));
                            self.expression(&value.body);
                            self.pop_scope();
                        }
                    }
                }
            }
            uhura_syntax::ast::ExpressionKind::Member { value, .. }
            | uhura_syntax::ast::ExpressionKind::Unary { value, .. } => self.expression(value),
            uhura_syntax::ast::ExpressionKind::Index { value, index } => {
                self.expression(value);
                self.expression(index);
            }
            uhura_syntax::ast::ExpressionKind::Binary { left, right, .. }
            | uhura_syntax::ast::ExpressionKind::Compare { left, right, .. } => {
                self.expression(left);
                self.expression(right);
            }
            uhura_syntax::ast::ExpressionKind::Is { value, pattern } => {
                self.expression(value);
                self.pattern_references(pattern);
            }
            uhura_syntax::ast::ExpressionKind::If(value) => {
                self.expression(&value.condition);
                self.block(&value.then_branch);
                if let Some(branch) = &value.else_branch {
                    match branch {
                        uhura_syntax::ast::ElseBranch::Block(value) => self.block(value),
                        uhura_syntax::ast::ElseBranch::If(value) => self.expression(value),
                    }
                }
            }
            uhura_syntax::ast::ExpressionKind::Match(value) => {
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
            uhura_syntax::ast::ExpressionKind::Return(value) => {
                if let Some(value) = value {
                    self.expression(value);
                }
            }
        }
    }

    fn field_initializers(&mut self, fields: &[uhura_syntax::ast::FieldInitializer]) {
        for field in fields {
            if let Some(value) = &field.value {
                self.expression(value);
            } else {
                self.reference(&field.name);
            }
        }
    }

    fn literal(&self, literal: &uhura_syntax::ast::Literal) {
        match literal {
            uhura_syntax::ast::Literal::Bool(_)
            | uhura_syntax::ast::Literal::Integer { .. }
            | uhura_syntax::ast::Literal::Decimal { .. }
            | uhura_syntax::ast::Literal::Text { .. } => {}
        }
    }

    fn qualified_name(&mut self, name: &uhura_syntax::ast::QualifiedName) {
        if let Some(first) = name.segments.first() {
            self.reference(first);
        }
    }

    fn pattern_references(&mut self, pattern: &uhura_syntax::ast::Pattern) {
        match &pattern.kind {
            uhura_syntax::ast::PatternKind::Wildcard
            | uhura_syntax::ast::PatternKind::Binder(_) => {}
            uhura_syntax::ast::PatternKind::Literal(value) => self.pattern_literal(value),
            uhura_syntax::ast::PatternKind::Group(value) => self.pattern_references(value),
            uhura_syntax::ast::PatternKind::Tuple(values)
            | uhura_syntax::ast::PatternKind::Alternative(values) => {
                for value in values {
                    self.pattern_references(value);
                }
            }
            uhura_syntax::ast::PatternKind::Constructor(value) => self.qualified_name(value),
            uhura_syntax::ast::PatternKind::TupleConstructor {
                constructor,
                arguments,
            } => {
                self.qualified_name(constructor);
                for argument in arguments {
                    self.pattern_references(argument);
                }
            }
            uhura_syntax::ast::PatternKind::Record {
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
            uhura_syntax::ast::PatternKind::AnonymousRecord { fields, .. } => {
                for field in fields {
                    if let Some(pattern) = &field.pattern {
                        self.pattern_references(pattern);
                    }
                }
            }
        }
    }

    fn pattern_literal(&self, literal: &uhura_syntax::ast::PatternLiteral) {
        match literal {
            uhura_syntax::ast::PatternLiteral::Bool(_)
            | uhura_syntax::ast::PatternLiteral::Integer { .. }
            | uhura_syntax::ast::PatternLiteral::Decimal { .. }
            | uhura_syntax::ast::PatternLiteral::Text { .. }
            | uhura_syntax::ast::PatternLiteral::Unit => {}
        }
    }

    fn ui_nodes(&mut self, nodes: &[uhura_syntax::ast::UiNode]) {
        for node in nodes {
            match &node.kind {
                uhura_syntax::ast::UiNodeKind::Text(_)
                | uhura_syntax::ast::UiNodeKind::Comment(_) => {}
                uhura_syntax::ast::UiNodeKind::Interpolation(value) => self.expression(value),
                uhura_syntax::ast::UiNodeKind::Element(value) => {
                    match value.name.kind {
                        uhura_syntax::ast::UiNameKind::Native => {}
                        uhura_syntax::ast::UiNameKind::Component => {
                            self.reference_ui_name(&value.name)
                        }
                    }
                    for attribute in &value.attributes {
                        match attribute {
                            uhura_syntax::ast::UiAttribute::Boolean { .. }
                            | uhura_syntax::ast::UiAttribute::StaticText { .. } => {}
                            uhura_syntax::ast::UiAttribute::Expression { value, .. }
                            | uhura_syntax::ast::UiAttribute::Event { input: value, .. } => {
                                self.expression(value);
                            }
                        }
                    }
                    self.ui_nodes(&value.children);
                }
                uhura_syntax::ast::UiNodeKind::If(value) => {
                    self.expression(&value.condition);
                    self.ui_nodes(&value.then_branch);
                    if let Some(branch) = &value.else_branch {
                        self.ui_nodes(branch);
                    }
                }
                uhura_syntax::ast::UiNodeKind::Each(value) => {
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

    fn reference(&mut self, identifier: &uhura_syntax::ast::Identifier) {
        if !self.is_bound(&identifier.text) {
            self.references
                .insert((identifier.text.clone(), identifier.span));
        }
    }

    fn reference_ui_name(&mut self, name: &uhura_syntax::ast::UiName) {
        if !self.is_bound(&name.text) {
            self.references.insert((name.text.clone(), name.span));
        }
    }

    fn is_bound(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(name))
    }

    fn push_scope<'a>(
        &mut self,
        names: impl IntoIterator<Item = &'a uhura_syntax::ast::Identifier>,
    ) {
        self.scopes
            .push(names.into_iter().map(|name| name.text.clone()).collect());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop().expect("a lexical scope is active");
    }
}

fn pattern_bindings(pattern: &uhura_syntax::ast::Pattern, names: &mut BTreeSet<String>) {
    match &pattern.kind {
        uhura_syntax::ast::PatternKind::Wildcard | uhura_syntax::ast::PatternKind::Literal(_) => {}
        uhura_syntax::ast::PatternKind::Binder(value) => {
            names.insert(value.text.clone());
        }
        uhura_syntax::ast::PatternKind::Group(value) => pattern_bindings(value, names),
        uhura_syntax::ast::PatternKind::Tuple(values)
        | uhura_syntax::ast::PatternKind::Alternative(values) => {
            for value in values {
                pattern_bindings(value, names);
            }
        }
        uhura_syntax::ast::PatternKind::Constructor(_) => {}
        uhura_syntax::ast::PatternKind::TupleConstructor { arguments, .. } => {
            for argument in arguments {
                pattern_bindings(argument, names);
            }
        }
        uhura_syntax::ast::PatternKind::Record { fields, .. }
        | uhura_syntax::ast::PatternKind::AnonymousRecord { fields, .. } => {
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

fn declaration_bound_names(declaration: &uhura_syntax::ast::Declaration) -> BTreeSet<String> {
    fn machine_member_names(
        members: &[uhura_syntax::ast::MachineMember],
        names: &mut BTreeSet<String>,
    ) {
        for member in members {
            match &member.kind {
                uhura_syntax::ast::MachineMemberKind::Config(value) => {
                    names.extend(value.fields.iter().map(|field| field.name.text.clone()));
                }
                uhura_syntax::ast::MachineMemberKind::Const(value) => {
                    names.insert(value.name.text.clone());
                }
                uhura_syntax::ast::MachineMemberKind::Function(value) => {
                    names.insert(value.name.text.clone());
                }
                uhura_syntax::ast::MachineMemberKind::Part(value) => {
                    names.insert(value.name.text.clone());
                }
                uhura_syntax::ast::MachineMemberKind::Port(value) => {
                    names.insert(value.name.text.clone());
                }
                uhura_syntax::ast::MachineMemberKind::State(value) => {
                    names.extend(value.fields.iter().map(|field| field.name.text.clone()));
                }
                uhura_syntax::ast::MachineMemberKind::Computed(value) => {
                    names.insert(value.name.text.clone());
                }
                uhura_syntax::ast::MachineMemberKind::Update(value) => {
                    names.insert(value.name.text.clone());
                }
                uhura_syntax::ast::MachineMemberKind::Require(_)
                | uhura_syntax::ast::MachineMemberKind::Events(_)
                | uhura_syntax::ast::MachineMemberKind::Commands(_)
                | uhura_syntax::ast::MachineMemberKind::Outcomes(_)
                | uhura_syntax::ast::MachineMemberKind::Invariant(_)
                | uhura_syntax::ast::MachineMemberKind::Observe(_)
                | uhura_syntax::ast::MachineMemberKind::Handler(_)
                | uhura_syntax::ast::MachineMemberKind::BeforeCommit(_) => {}
            }
        }
    }

    fn part_member_names(members: &[uhura_syntax::ast::PartMember], names: &mut BTreeSet<String>) {
        for member in members {
            match &member.kind {
                uhura_syntax::ast::PartMemberKind::Const(value) => {
                    names.insert(value.name.text.clone());
                }
                uhura_syntax::ast::PartMemberKind::Function(value) => {
                    names.insert(value.name.text.clone());
                }
                uhura_syntax::ast::PartMemberKind::Port(value) => {
                    names.insert(value.name.text.clone());
                }
                uhura_syntax::ast::PartMemberKind::State(value) => {
                    names.extend(value.fields.iter().map(|field| field.name.text.clone()));
                }
                uhura_syntax::ast::PartMemberKind::Computed(value) => {
                    names.insert(value.name.text.clone());
                }
                uhura_syntax::ast::PartMemberKind::Update(value) => {
                    names.insert(value.name.text.clone());
                }
                uhura_syntax::ast::PartMemberKind::Require(_)
                | uhura_syntax::ast::PartMemberKind::RequiresOutcomes(_)
                | uhura_syntax::ast::PartMemberKind::Events(_)
                | uhura_syntax::ast::PartMemberKind::Commands(_)
                | uhura_syntax::ast::PartMemberKind::Invariant(_)
                | uhura_syntax::ast::PartMemberKind::Observe(_)
                | uhura_syntax::ast::PartMemberKind::Handler(_) => {}
            }
        }
    }

    let mut names = BTreeSet::new();
    match &declaration.kind {
        uhura_syntax::ast::DeclarationKind::Function(_)
        | uhura_syntax::ast::DeclarationKind::Struct(_)
        | uhura_syntax::ast::DeclarationKind::Enum(_)
        | uhura_syntax::ast::DeclarationKind::Key(_)
        | uhura_syntax::ast::DeclarationKind::Const(_)
        | uhura_syntax::ast::DeclarationKind::Scenario(_)
        | uhura_syntax::ast::DeclarationKind::Example(_)
        | uhura_syntax::ast::DeclarationKind::Checkpoint(_) => {}
        uhura_syntax::ast::DeclarationKind::Machine(value) => {
            machine_member_names(&value.members, &mut names);
        }
        uhura_syntax::ast::DeclarationKind::Part(value) => {
            names.extend(
                value
                    .parameters
                    .iter()
                    .map(|parameter| parameter.name.text.clone()),
            );
            part_member_names(&value.members, &mut names);
        }
        uhura_syntax::ast::DeclarationKind::Ui(value) => match &value.binding {
            uhura_syntax::ast::UiBinding::Machine { observation, .. } => {
                names.insert(observation.text.clone());
            }
            uhura_syntax::ast::UiBinding::Component { parameters, .. } => {
                names.extend(
                    parameters
                        .iter()
                        .map(|parameter| parameter.name.text.clone()),
                );
            }
        },
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use uhura_syntax::{SourceIdentity, parse};

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
  <Panel value={format(view.ready)} on Press -> Start(DEFAULT) />
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
