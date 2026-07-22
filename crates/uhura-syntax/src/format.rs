//! Deterministic canonical formatting for the current Uhura core AST.
//!
//! It renders parsed structure rather than rewriting token text. Markup
//! comments are represented directly; unsupported DSL comments are refused so
//! formatting never silently erases author-visible trivia.

use std::fmt;

use super::ast::*;
use super::lexer::{TriviaKind, lex};

/// One comment that prevents structure-only canonical formatting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnsupportedComment {
    pub kind: TriviaKind,
    pub text: String,
    pub span: Span,
}

/// A deterministic formatting failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormatError {
    /// DSL comments were found, but their attachment is not yet represented
    /// in the 0.4 AST.
    UnsupportedComments { comments: Vec<UnsupportedComment> },
}

impl fmt::Display for FormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedComments { comments } => write!(
                formatter,
                "cannot canonically format {} DSL comment(s) before DSL comment attachment is available",
                comments.len()
            ),
        }
    }
}

impl std::error::Error for FormatError {}

/// Format one parsed, manifest-resolved Uhura 0.4 core module.
///
/// Output uses two-space indentation, a trailing comma for every multiline
/// list entry, explicit semicolons for statements, and exactly one final LF.
/// An empty module formats to the empty string.
pub fn format(module: &Module) -> Result<String, FormatError> {
    let comments = comments(module);
    if !comments.is_empty() {
        return Err(FormatError::UnsupportedComments { comments });
    }

    let mut formatter = Formatter::default();
    formatter.module(module);
    Ok(formatter.finish())
}

fn comments(module: &Module) -> Vec<UnsupportedComment> {
    // Re-lex the retained source rather than trusting callers to have kept the
    // lossless token cache in sync with it. This makes comment refusal robust
    // for deserialized or manually assembled modules as well as parser output.
    let mut comments: Vec<_> = lex(&module.identity, &module.source)
        .tokens
        .into_iter()
        .flat_map(|token| token.leading)
        .filter_map(|trivia| match trivia.kind {
            TriviaKind::OrdinaryComment | TriviaKind::OuterDoc | TriviaKind::InnerDoc => {
                Some(UnsupportedComment {
                    kind: trivia.kind,
                    text: trivia.text,
                    span: trivia.span,
                })
            }
            TriviaKind::Whitespace | TriviaKind::InvalidWhitespace => None,
        })
        .collect();
    for declaration in &module.declarations {
        if let DeclarationKind::Ui(ui) = &declaration.kind {
            comments.extend(ui.body.embedded_core_comments.iter().map(|trivia| {
                UnsupportedComment {
                    kind: trivia.kind,
                    text: trivia.text.clone(),
                    span: trivia.span,
                }
            }));
        }
    }
    comments.sort_by_key(|comment| (comment.span.start, comment.span.end));
    comments
}

#[derive(Default)]
struct Formatter {
    output: String,
    indent: usize,
    line_start: bool,
}

impl Formatter {
    fn finish(mut self) -> String {
        if !self.output.is_empty() && !self.output.ends_with('\n') {
            self.output.push('\n');
        }
        self.output
    }

    fn write(&mut self, value: &str) {
        if value.is_empty() {
            return;
        }
        if self.line_start {
            for _ in 0..self.indent {
                self.output.push_str("  ");
            }
            self.line_start = false;
        }
        self.output.push_str(value);
    }

    fn newline(&mut self) {
        if !self.output.ends_with('\n') {
            self.output.push('\n');
        }
        self.line_start = true;
    }

    fn blank_line(&mut self) {
        self.newline();
        if !self.output.ends_with("\n\n") {
            self.output.push('\n');
        }
        self.line_start = true;
    }

    fn module(&mut self, module: &Module) {
        for declaration in &module.uses {
            self.use_declaration(declaration);
            self.newline();
        }
        if !module.uses.is_empty() && !module.declarations.is_empty() {
            self.blank_line();
        }
        for (index, declaration) in module.declarations.iter().enumerate() {
            if index > 0 {
                self.blank_line();
            }
            self.declaration(declaration);
        }
    }

    fn use_declaration(&mut self, declaration: &UseDeclaration) {
        self.visibility(declaration.visibility);
        self.write("use ");
        match &declaration.tree {
            ImportTree::Single { path, alias } => {
                self.import_root(&path.root);
                for segment in &path.segments {
                    self.write("::");
                    self.identifier(segment);
                }
                if let Some(alias) = alias {
                    self.write(" as ");
                    self.identifier(alias);
                }
            }
            ImportTree::Group { prefix, items } => {
                self.import_root(&prefix.root);
                for segment in &prefix.segments {
                    self.write("::");
                    self.identifier(segment);
                }
                self.write("::{");
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        self.write(", ");
                    }
                    self.identifier(&item.name);
                    if let Some(alias) = &item.alias {
                        self.write(" as ");
                        self.identifier(alias);
                    }
                }
                self.write("}");
            }
        }
        self.write(";");
    }

    fn import_root(&mut self, root: &ImportRoot) {
        match root {
            ImportRoot::Crate(_) => self.write("crate"),
            ImportRoot::Package(name) => self.identifier(name),
        }
    }

    fn declaration(&mut self, declaration: &Declaration) {
        match &declaration.kind {
            DeclarationKind::Machine(value) => self.machine(value),
            DeclarationKind::Part(value) => self.part(value),
            DeclarationKind::Ui(value) => self.ui(value),
            DeclarationKind::Scenario(value) => self.scenario(value),
            DeclarationKind::Example(value) => self.evidence_alias("example", value),
            DeclarationKind::Checkpoint(value) => self.evidence_alias("checkpoint", value),
            DeclarationKind::Struct(value) => self.struct_declaration(value),
            DeclarationKind::Enum(value) => self.enum_declaration(value),
            DeclarationKind::Key(value) => self.key_declaration(value),
            DeclarationKind::Const(value) => self.const_declaration(value, true),
            DeclarationKind::Function(value) => self.function_declaration(value, true),
        }
    }

    fn machine(&mut self, declaration: &MachineDeclaration) {
        self.visibility(declaration.visibility);
        self.write("machine ");
        self.identifier(&declaration.name);
        self.write(" ");
        self.open_multiline_body();
        for (index, member) in declaration.members.iter().enumerate() {
            if index > 0 {
                self.blank_line();
            }
            self.machine_member(member);
        }
        self.close_multiline_body();
    }

    fn part(&mut self, declaration: &PartDeclaration) {
        self.visibility(declaration.visibility);
        self.write("part ");
        self.identifier(&declaration.name);
        if !declaration.parameters.is_empty() {
            self.parameters(&declaration.parameters);
        }
        self.write(" ");
        self.open_multiline_body();
        for (index, member) in declaration.members.iter().enumerate() {
            if index > 0 {
                self.blank_line();
            }
            self.part_member(member);
        }
        self.close_multiline_body();
    }

    fn ui(&mut self, declaration: &UiDeclaration) {
        self.visibility(declaration.visibility);
        self.write("ui ");
        self.identifier(&declaration.name);
        self.write(" for ");
        self.type_path(&declaration.machine);
        self.write("(");
        self.identifier(&declaration.observation);
        self.write(") {");
        if declaration.body.nodes.is_empty() {
            self.write("}");
            return;
        }
        self.newline();
        self.indent += 1;
        self.ui_nodes(&declaration.body.nodes);
        self.indent -= 1;
        self.write("}");
    }

    fn scenario(&mut self, declaration: &ScenarioDeclaration) {
        self.write("scenario ");
        self.identifier(&declaration.name);
        match &declaration.origin {
            ScenarioOrigin::Machine {
                machine,
                configuration,
            } => {
                self.write(" for ");
                self.type_path(machine);
                if let Some(configuration) = configuration {
                    self.write("(");
                    self.expression(configuration);
                    self.write(")");
                }
            }
            ScenarioOrigin::Snapshot(reference) => {
                self.write(" from ");
                self.evidence_reference(reference);
            }
        }
        self.write(" ");
        self.open_multiline_body();
        for step in &declaration.steps {
            self.evidence_step(step);
            self.newline();
        }
        self.close_multiline_body();
    }

    fn evidence_alias(&mut self, keyword: &str, declaration: &EvidenceAliasDeclaration) {
        self.write(keyword);
        self.write(" ");
        self.identifier(&declaration.name);
        if let Some(presentation) = &declaration.presentation {
            self.write(" for ");
            self.identifier(presentation);
            self.write(" as ");
            self.write(match declaration.kind {
                Some(EvidencePresentationKind::Page) => "page",
                Some(EvidencePresentationKind::Component) => "component",
                Some(EvidencePresentationKind::Surface) => "surface",
                None => "page",
            });
            if declaration.is_default {
                self.write(" default");
            }
            if let Some(note) = &declaration.note {
                self.write(" note ");
                self.text(note);
            }
        }
        self.write(" = ");
        self.evidence_reference(&declaration.target);
        self.write(";");
    }

    fn evidence_reference(&mut self, reference: &EvidenceReference) {
        for (index, segment) in reference.path.iter().enumerate() {
            if index > 0 {
                self.write("::");
            }
            self.identifier(segment);
        }
    }

    fn evidence_step(&mut self, step: &EvidenceStep) {
        match &step.kind {
            EvidenceStepKind::Bind { port, fixture } => {
                self.write("bind ");
                self.identifier(port);
                self.write(" = ");
                self.expression(fixture);
            }
            EvidenceStepKind::Start => self.write("start"),
            EvidenceStepKind::Send(value) => {
                self.write("send ");
                self.expression(value);
            }
            EvidenceStepKind::Deliver(value) => {
                self.write("deliver ");
                self.expression(value);
            }
            EvidenceStepKind::ExpectReaction { outcome, commands } => {
                self.write("expect ");
                self.pattern(outcome);
                self.write(" commands ");
                self.command_expectation(commands);
            }
            EvidenceStepKind::ExpectObservationPattern(value) => {
                self.write("expect observation ");
                self.pattern(value);
            }
            EvidenceStepKind::ExpectInspectionPattern(value) => {
                self.write("expect inspection ");
                self.pattern(value);
            }
            EvidenceStepKind::ExpectObservationWhere(value) => {
                self.write("expect observation where ");
                self.expression(value);
            }
            EvidenceStepKind::ExpectRestore { commands } => {
                self.write("expect restore commands ");
                self.command_expectation(commands);
            }
            EvidenceStepKind::ExpectSnapshot { target } => {
                self.write("expect snapshot == ");
                self.evidence_reference(target);
            }
            EvidenceStepKind::Pin(value) => {
                self.write("pin ");
                self.identifier(value);
            }
        }
    }

    fn command_expectation(&mut self, commands: &[Expression]) {
        self.write("[");
        self.expressions(commands);
        self.write("]");
    }

    fn struct_declaration(&mut self, declaration: &StructDeclaration) {
        self.visibility(declaration.visibility);
        self.write("struct ");
        self.identifier(&declaration.name);
        self.write(" ");
        self.typed_field_body(&declaration.fields);
    }

    fn enum_declaration(&mut self, declaration: &EnumDeclaration) {
        self.visibility(declaration.visibility);
        self.write("enum ");
        self.identifier(&declaration.name);
        self.write(" ");
        self.open_multiline_body();
        for variant in &declaration.variants {
            self.identifier(&variant.name);
            if !variant.fields.is_empty() {
                self.write(" ");
                self.typed_field_body(&variant.fields);
            }
            self.write(",");
            self.newline();
        }
        self.close_multiline_body();
    }

    fn key_declaration(&mut self, declaration: &KeyDeclaration) {
        self.visibility(declaration.visibility);
        self.write("key ");
        self.identifier(&declaration.name);
        self.write("(");
        self.type_expression(&declaration.value);
        self.write(");");
    }

    fn const_declaration(&mut self, declaration: &ConstDeclaration, use_visibility: bool) {
        if use_visibility {
            self.visibility(declaration.visibility);
        }
        self.write("const ");
        self.identifier(&declaration.name);
        self.write(": ");
        self.type_expression(&declaration.ty);
        self.write(" = ");
        self.expression(&declaration.value);
        self.write(";");
    }

    fn function_declaration(&mut self, declaration: &FunctionDeclaration, use_visibility: bool) {
        if use_visibility {
            self.visibility(declaration.visibility);
        }
        self.write("fn ");
        self.identifier(&declaration.name);
        self.parameters(&declaration.parameters);
        self.write(" -> ");
        self.type_expression(&declaration.result);
        self.write(" ");
        self.block(&declaration.body);
    }

    fn machine_member(&mut self, member: &MachineMember) {
        match &member.kind {
            MachineMemberKind::Config(value) => self.typed_section("config", &value.fields),
            MachineMemberKind::Require(value) => self.require_declaration(value),
            MachineMemberKind::Const(value) => self.const_declaration(value, false),
            MachineMemberKind::Function(value) => self.function_declaration(value, false),
            MachineMemberKind::Part(value) => self.part_instance(value),
            MachineMemberKind::Events(value) => self.protocol_section("events", value),
            MachineMemberKind::Commands(value) => self.protocol_section("commands", value),
            MachineMemberKind::Port(value) => self.port_declaration(value),
            MachineMemberKind::Outcomes(value) => self.outcome_section("outcomes", value),
            MachineMemberKind::State(value) => self.state_section(value),
            MachineMemberKind::Computed(value) => self.computed_declaration(value, false),
            MachineMemberKind::Invariant(value) => self.invariant_declaration(value),
            MachineMemberKind::Observe(value) => self.observe_section(value),
            MachineMemberKind::Handler(value) => self.handler_declaration(value),
            MachineMemberKind::Update(value) => self.update_declaration(value, false),
            MachineMemberKind::BeforeCommit(value) => self.before_commit(value),
        }
    }

    fn part_member(&mut self, member: &PartMember) {
        match &member.kind {
            PartMemberKind::Require(value) => self.require_declaration(value),
            PartMemberKind::RequiresOutcomes(value) => {
                self.outcome_section("requires outcomes", value);
            }
            PartMemberKind::Const(value) => self.const_declaration(value, false),
            PartMemberKind::Function(value) => self.function_declaration(value, false),
            PartMemberKind::Events(value) => self.protocol_section("events", value),
            PartMemberKind::Commands(value) => self.protocol_section("commands", value),
            PartMemberKind::Port(value) => self.port_declaration(value),
            PartMemberKind::State(value) => self.state_section(value),
            PartMemberKind::Computed(value) => self.computed_declaration(value, true),
            PartMemberKind::Invariant(value) => self.invariant_declaration(value),
            PartMemberKind::Observe(value) => self.observe_section(value),
            PartMemberKind::Handler(value) => self.handler_declaration(value),
            PartMemberKind::Update(value) => self.update_declaration(value, true),
        }
    }

    fn typed_section(&mut self, name: &str, fields: &[TypedField]) {
        self.write(name);
        self.write(" ");
        self.typed_field_body(fields);
    }

    fn typed_field_body(&mut self, fields: &[TypedField]) {
        self.open_multiline_body();
        for field in fields {
            self.typed_field(field);
            self.write(",");
            self.newline();
        }
        self.close_multiline_body();
    }

    fn typed_field(&mut self, field: &TypedField) {
        self.identifier(&field.name);
        self.write(": ");
        self.type_expression(&field.ty);
    }

    fn require_declaration(&mut self, declaration: &RequireDeclaration) {
        self.write("require ");
        self.expression(&declaration.condition);
        self.write(";");
    }

    fn part_instance(&mut self, instance: &PartInstance) {
        self.write("part ");
        self.identifier(&instance.name);
        self.write(" = ");
        self.type_path(&instance.part);
        self.arguments(&instance.arguments);
        self.write(";");
    }

    fn protocol_section(&mut self, name: &str, section: &ProtocolSection) {
        self.write(name);
        self.write(" ");
        self.open_multiline_body();
        for variant in &section.variants {
            self.protocol_variant(variant);
            self.write(",");
            self.newline();
        }
        self.close_multiline_body();
    }

    fn protocol_variant(&mut self, variant: &ProtocolVariant) {
        self.identifier(&variant.name);
        if !variant.parameters.is_empty() {
            self.parameters(&variant.parameters);
        }
    }

    fn port_declaration(&mut self, declaration: &PortDeclaration) {
        self.write("port ");
        self.identifier(&declaration.name);
        self.write(" = ");
        self.type_path(&declaration.contract);
        self.write(" ");
        self.field_initializer_body(&declaration.fields);
        self.write(";");
    }

    fn outcome_section(&mut self, name: &str, section: &OutcomeSection) {
        self.write(name);
        self.write(" ");
        self.open_multiline_body();
        for entry in &section.entries {
            self.write(match entry.policy {
                OutcomePolicy::Commit => "commit ",
                OutcomePolicy::Abort => "abort ",
            });
            self.protocol_variant(&entry.variant);
            self.write(",");
            self.newline();
        }
        self.close_multiline_body();
    }

    fn state_section(&mut self, section: &StateSection) {
        self.write("state ");
        self.open_multiline_body();
        for field in &section.fields {
            self.identifier(&field.name);
            self.write(": ");
            self.type_expression(&field.ty);
            self.write(" = ");
            self.expression(&field.initial);
            self.write(",");
            self.newline();
        }
        self.close_multiline_body();
    }

    fn computed_declaration(&mut self, declaration: &ComputedDeclaration, use_visibility: bool) {
        if use_visibility {
            self.visibility(declaration.visibility);
        }
        self.write("computed ");
        self.identifier(&declaration.name);
        if let Some(ty) = &declaration.ty {
            self.write(": ");
            self.type_expression(ty);
        }
        self.write(" = ");
        self.expression(&declaration.value);
        self.write(";");
    }

    fn invariant_declaration(&mut self, declaration: &InvariantDeclaration) {
        if declaration.grouped {
            self.write("invariant ");
            self.open_multiline_body();
            for condition in &declaration.conditions {
                self.expression(condition);
                self.write(",");
                self.newline();
            }
            self.close_multiline_body();
        } else {
            self.write("invariant ");
            if let Some(condition) = declaration.conditions.first() {
                self.expression(condition);
            }
            self.write(";");
        }
    }

    fn observe_section(&mut self, section: &ObserveSection) {
        self.write("observe ");
        self.open_multiline_body();
        for field in &section.fields {
            self.identifier(&field.name);
            if let Some(value) = &field.value {
                self.write(": ");
                self.expression(value);
            }
            self.write(",");
            self.newline();
        }
        self.close_multiline_body();
    }

    fn handler_declaration(&mut self, declaration: &HandlerDeclaration) {
        self.write("on ");
        self.protocol_selector(&declaration.input);
        if !declaration.parameters.is_empty() {
            self.write("(");
            self.patterns(&declaration.parameters);
            self.write(")");
        }
        self.write(" ");
        self.block(&declaration.body);
    }

    fn update_declaration(&mut self, declaration: &UpdateDeclaration, use_visibility: bool) {
        if use_visibility {
            self.visibility(declaration.visibility);
        }
        self.write("update ");
        self.identifier(&declaration.name);
        self.parameters(&declaration.parameters);
        if let Some(result) = &declaration.result {
            self.write(" -> ");
            self.type_expression(result);
        }
        self.write(" ");
        self.block(&declaration.body);
    }

    fn before_commit(&mut self, declaration: &BeforeCommitDeclaration) {
        self.write("before commit ");
        self.block(&declaration.body);
    }

    fn ui_nodes(&mut self, nodes: &[UiNode]) {
        for node in nodes {
            self.ui_node(node);
            self.newline();
        }
    }

    fn ui_node(&mut self, node: &UiNode) {
        match &node.kind {
            UiNodeKind::Text(text) => self.write(normalize_ui_text(&text.raw).trim()),
            UiNodeKind::Comment(comment) => self.ui_comment(comment),
            UiNodeKind::Interpolation(expression) => {
                self.write("{");
                self.expression(expression);
                self.write("}");
            }
            UiNodeKind::Element(element) => self.ui_element(element),
            UiNodeKind::If(value) => self.ui_if(value),
            UiNodeKind::Each(value) => self.ui_each(value),
        }
    }

    fn ui_comment(&mut self, comment: &UiComment) {
        match comment.status {
            UiCommentStatus::Malformed { terminated } => {
                // Preserve the lexical failure exactly. Canonical padding or an
                // invented close could turn recovery text into valid metadata.
                self.write("<!--");
                self.write(&comment.body);
                if terminated {
                    self.write("-->");
                }
                return;
            }
            UiCommentStatus::RejectedAnnotation => {
                // Keep the visible kind and prose, but use `:` (outside the
                // annotation-kind grammar) as a stable malformed carrier. A
                // discarded recovery target can therefore never retarget the
                // annotation on the next parse.
                let annotation = comment
                    .annotation
                    .as_ref()
                    .expect("rejected annotation retains its parsed payload");
                self.write("<!--@");
                self.write(&annotation.kind);
                self.write(":");
                if annotation.text.contains('\n') {
                    self.newline();
                    for line in annotation.text.split('\n') {
                        self.write(line);
                        self.newline();
                    }
                } else {
                    self.write(" ");
                    self.write(&annotation.text);
                }
                self.write("-->");
                return;
            }
            UiCommentStatus::Ordinary | UiCommentStatus::Annotation => {}
        }

        let (marker, text) = match &comment.annotation {
            Some(annotation) => (Some(annotation.kind.as_str()), annotation.text.clone()),
            None => (None, normalize_markup_text(&comment.body)),
        };

        if !text.contains('\n') {
            self.write("<!--");
            match marker {
                Some(kind) => {
                    self.write(" @");
                    self.write(kind);
                    self.write(" ");
                    self.write(&text);
                    self.write(" ");
                }
                None if text.is_empty() => self.write(" "),
                None => {
                    self.write(" ");
                    self.write(&text);
                    self.write(" ");
                }
            }
            self.write("-->");
            return;
        }

        self.write("<!--");
        if let Some(kind) = marker {
            self.write(" @");
            self.write(kind);
        }
        self.newline();
        for line in text.split('\n') {
            self.write(line);
            self.newline();
        }
        self.write("-->");
    }

    fn ui_element(&mut self, element: &UiElement) {
        self.write("<");
        self.write(&element.name.text);
        if element.attributes.len() <= 1 {
            for attribute in &element.attributes {
                self.write(" ");
                self.ui_attribute(attribute);
            }
        } else {
            self.newline();
            self.indent += 1;
            for attribute in &element.attributes {
                self.ui_attribute(attribute);
                self.newline();
            }
            self.indent -= 1;
        }

        if element.self_closing {
            self.write("/>");
            return;
        }
        self.write(">");

        if element.children.is_empty() {
            self.write("</");
            self.write(&element.name.text);
            self.write(">");
        } else if element.children.iter().all(ui_node_is_inline) {
            for (index, child) in element.children.iter().enumerate() {
                self.ui_inline_node(child, index == 0, index + 1 == element.children.len());
            }
            self.write("</");
            self.write(&element.name.text);
            self.write(">");
        } else {
            self.newline();
            self.indent += 1;
            self.ui_nodes(&element.children);
            self.indent -= 1;
            self.write("</");
            self.write(&element.name.text);
            self.write(">");
        }
    }

    fn ui_inline_node(&mut self, node: &UiNode, first: bool, last: bool) {
        match &node.kind {
            UiNodeKind::Text(text) => {
                let mut value = normalize_ui_text(&text.raw);
                if first && value.starts_with(' ') {
                    value.remove(0);
                }
                if last && value.ends_with(' ') {
                    value.pop();
                }
                self.write(&value);
            }
            UiNodeKind::Interpolation(expression) => {
                self.write("{");
                self.expression(expression);
                self.write("}");
            }
            _ => unreachable!("inline UI children are text or interpolation"),
        }
    }

    fn ui_attribute(&mut self, attribute: &UiAttribute) {
        match attribute {
            UiAttribute::Boolean { name, .. } => self.write(&name.text),
            UiAttribute::StaticText { name, value, .. } => {
                self.write(&name.text);
                self.write("=\"");
                self.write(value);
                self.write("\"");
            }
            UiAttribute::Expression { name, value, .. } => {
                self.write(&name.text);
                self.write("={");
                self.expression(value);
                self.write("}");
            }
            UiAttribute::Event { event, input, .. } => {
                self.write("on ");
                self.write(&event.text);
                self.write(" -> ");
                self.expression(input);
            }
        }
    }

    fn ui_if(&mut self, value: &UiIf) {
        self.write("{#if ");
        self.expression(&value.condition);
        self.write("}");
        if !value.then_branch.is_empty() {
            self.newline();
            self.indent += 1;
            self.ui_nodes(&value.then_branch);
            self.indent -= 1;
        } else {
            self.newline();
        }
        if let Some(else_branch) = &value.else_branch {
            self.write("{:else}");
            if !else_branch.is_empty() {
                self.newline();
                self.indent += 1;
                self.ui_nodes(else_branch);
                self.indent -= 1;
            } else {
                self.newline();
            }
        }
        self.write("{/if}");
    }

    fn ui_each(&mut self, value: &UiEach) {
        self.write("{#each ");
        self.expression(&value.source);
        self.write(" as ");
        self.pattern(&value.pattern);
        self.write(" (");
        self.expression(&value.key);
        self.write(")}");
        if !value.children.is_empty() {
            self.newline();
            self.indent += 1;
            self.ui_nodes(&value.children);
            self.indent -= 1;
        } else {
            self.newline();
        }
        self.write("{/each}");
    }

    fn type_expression(&mut self, ty: &TypeExpression) {
        match &ty.kind {
            TypeExpressionKind::Path(path) => self.type_path(path),
            TypeExpressionKind::Unit => self.write("()"),
            TypeExpressionKind::Tuple(values) => {
                self.write("(");
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        self.write(", ");
                    }
                    self.type_expression(value);
                }
                self.write(")");
            }
        }
    }

    fn type_path(&mut self, path: &TypePath) {
        for (index, segment) in path.segments.iter().enumerate() {
            if index > 0 {
                self.write("::");
            }
            self.identifier(&segment.name);
            if !segment.arguments.is_empty() {
                self.write("<");
                for (argument_index, argument) in segment.arguments.iter().enumerate() {
                    if argument_index > 0 {
                        self.write(", ");
                    }
                    self.type_expression(argument);
                }
                self.write(">");
            }
        }
    }

    fn parameters(&mut self, parameters: &[Parameter]) {
        self.write("(");
        for (index, parameter) in parameters.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            self.identifier(&parameter.name);
            self.write(": ");
            self.type_expression(&parameter.ty);
        }
        self.write(")");
    }

    fn arguments(&mut self, arguments: &[Expression]) {
        self.write("(");
        for (index, argument) in arguments.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            self.expression(argument);
        }
        self.write(")");
    }

    fn block(&mut self, block: &Block) {
        self.write("{");
        if block.statements.is_empty() && block.tail.is_none() {
            self.write("}");
            return;
        }
        self.newline();
        self.indent += 1;
        for statement in &block.statements {
            self.statement(statement);
            self.newline();
        }
        if let Some(tail) = &block.tail {
            self.expression(tail);
            self.newline();
        }
        self.indent -= 1;
        self.write("}");
    }

    fn statement(&mut self, statement: &Statement) {
        match &statement.kind {
            StatementKind::Let {
                name, ty, value, ..
            } => {
                self.write("let ");
                self.identifier(name);
                if let Some(ty) = ty {
                    self.write(": ");
                    self.type_expression(ty);
                }
                self.write(" = ");
                self.expression(value);
                self.write(";");
            }
            StatementKind::Assign { target, value, .. } => {
                self.identifier(target);
                self.write(" = ");
                self.expression(value);
                self.write(";");
            }
            StatementKind::Emit { output, .. } => {
                self.write("emit ");
                self.output_constructor(output);
                self.write(";");
            }
            StatementKind::While {
                condition,
                decreases,
                body,
            } => {
                self.write("while ");
                self.expression(condition);
                self.write(" decreases(");
                self.expression(decreases);
                self.write(") ");
                self.block(body);
            }
            StatementKind::Unreachable { .. } => self.write("unreachable;"),
            StatementKind::Expression { expression, .. } => {
                self.expression(expression);
                self.write(";");
            }
            StatementKind::BlockExpression(expression) => self.expression(expression),
        }
    }

    fn output_constructor(&mut self, output: &OutputConstructor) {
        self.protocol_selector(&output.selector);
        if !output.arguments.is_empty() {
            self.arguments(&output.arguments);
        }
    }

    fn protocol_selector(&mut self, selector: &ProtocolSelector) {
        if let Some(owner) = &selector.owner {
            self.identifier(owner);
            self.write(".");
        }
        self.identifier(&selector.variant);
    }

    fn expression(&mut self, expression: &Expression) {
        self.expression_at(expression, Precedence::Return);
    }

    fn expression_at(&mut self, expression: &Expression, minimum: Precedence) {
        let precedence = expression_precedence(&expression.kind);
        let parenthesized = precedence < minimum;
        if parenthesized {
            self.write("(");
        }
        match &expression.kind {
            ExpressionKind::Literal(value) => self.literal(value),
            ExpressionKind::Unit => self.write("()"),
            ExpressionKind::Sequence(values) => {
                self.write("[");
                self.expressions(values);
                self.write("]");
            }
            ExpressionKind::Tuple(values) => {
                self.write("(");
                self.expressions(values);
                self.write(")");
            }
            ExpressionKind::Group(value) => {
                self.write("(");
                self.expression(value);
                self.write(")");
            }
            ExpressionKind::Name(value) => self.qualified_name(value),
            ExpressionKind::Record(value) => self.record_expression(value),
            ExpressionKind::AnonymousRecord(value) => self.anonymous_record(value),
            ExpressionKind::Block(value) => self.block(value),
            ExpressionKind::Call { callee, arguments } => {
                self.expression_at(callee, Precedence::Postfix);
                self.write("(");
                for (index, argument) in arguments.iter().enumerate() {
                    if index > 0 {
                        self.write(", ");
                    }
                    match argument {
                        CallArgument::Expression(value) => self.expression(value),
                        CallArgument::Binder(value) => self.binder(value),
                    }
                }
                self.write(")");
            }
            ExpressionKind::Member { value, member } => {
                self.expression_at(value, Precedence::Postfix);
                self.write(".");
                self.identifier(member);
            }
            ExpressionKind::Index { value, index } => {
                self.expression_at(value, Precedence::Postfix);
                self.write("[");
                self.expression(index);
                self.write("]");
            }
            ExpressionKind::Unary { operator, value } => {
                self.write(match operator {
                    UnaryOperator::Not => "!",
                    UnaryOperator::Negate => "-",
                });
                self.expression_at(value, Precedence::Unary);
            }
            ExpressionKind::Binary {
                operator,
                left,
                right,
            } => {
                let own = binary_precedence(*operator);
                self.expression_at(left, own);
                self.write(match operator {
                    BinaryOperator::Multiply => " * ",
                    BinaryOperator::Add => " + ",
                    BinaryOperator::Subtract => " - ",
                    BinaryOperator::And => " && ",
                    BinaryOperator::Or => " || ",
                });
                self.expression_at(right, own.next());
            }
            ExpressionKind::Compare {
                operator,
                left,
                right,
            } => {
                self.expression_at(left, Precedence::Additive);
                self.write(match operator {
                    ComparisonOperator::Equal => " == ",
                    ComparisonOperator::NotEqual => " != ",
                    ComparisonOperator::Less => " < ",
                    ComparisonOperator::LessEqual => " <= ",
                    ComparisonOperator::Greater => " > ",
                    ComparisonOperator::GreaterEqual => " >= ",
                });
                self.expression_at(right, Precedence::Additive);
            }
            ExpressionKind::Is { value, pattern } => {
                self.expression_at(value, Precedence::Additive);
                self.write(" is ");
                self.pattern(pattern);
            }
            ExpressionKind::If(value) => self.if_expression(value),
            ExpressionKind::Match(value) => self.match_expression(value),
            ExpressionKind::Return(value) => {
                self.write("return");
                if let Some(value) = value {
                    self.write(" ");
                    self.expression(value);
                }
            }
        }
        if parenthesized {
            self.write(")");
        }
    }

    fn expressions(&mut self, values: &[Expression]) {
        for (index, value) in values.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            self.expression(value);
        }
    }

    fn literal(&mut self, literal: &Literal) {
        match literal {
            Literal::Bool(value) => self.write(if *value { "true" } else { "false" }),
            Literal::Integer { raw } | Literal::Decimal { raw } => self.write(raw),
            Literal::Text { value, .. } => self.text(value),
        }
    }

    fn text(&mut self, value: &str) {
        self.write("\"");
        for character in value.chars() {
            match character {
                '"' => self.write("\\\""),
                '\\' => self.write("\\\\"),
                '\u{0008}' => self.write("\\b"),
                '\u{000c}' => self.write("\\f"),
                '\n' => self.write("\\n"),
                '\r' => self.write("\\r"),
                '\t' => self.write("\\t"),
                character if character <= '\u{001f}' => {
                    self.write(&format!("\\u{:04x}", character as u32));
                }
                character => self.write(&character.to_string()),
            }
        }
        self.write("\"");
    }

    fn record_expression(&mut self, record: &RecordExpression) {
        self.qualified_name(&record.constructor);
        self.write(" ");
        self.open_record_body();
        for field in &record.fields {
            self.field_initializer(field);
            self.write(",");
            self.newline();
        }
        if let Some(base) = &record.base {
            self.write("..");
            self.expression(base);
            self.write(",");
            self.newline();
        }
        self.close_record_body(record.fields.is_empty() && record.base.is_none());
    }

    fn anonymous_record(&mut self, entries: &[AnonymousRecordEntry]) {
        self.open_record_body();
        for entry in entries {
            self.expression(&entry.key);
            self.write(": ");
            self.expression(&entry.value);
            self.write(",");
            self.newline();
        }
        self.close_record_body(entries.is_empty());
    }

    fn field_initializer_body(&mut self, fields: &[FieldInitializer]) {
        self.open_record_body();
        for field in fields {
            self.field_initializer(field);
            self.write(",");
            self.newline();
        }
        self.close_record_body(fields.is_empty());
    }

    fn field_initializer(&mut self, field: &FieldInitializer) {
        self.identifier(&field.name);
        if let Some(value) = &field.value {
            self.write(": ");
            self.expression(value);
        }
    }

    fn binder(&mut self, binder: &BinderExpression) {
        self.write("|");
        self.identifier(&binder.parameter);
        self.write("| ");
        self.expression(&binder.body);
    }

    fn if_expression(&mut self, expression: &IfExpression) {
        self.write("if ");
        self.expression(&expression.condition);
        self.write(" ");
        self.block(&expression.then_branch);
        if let Some(branch) = &expression.else_branch {
            self.write(" else ");
            match branch {
                ElseBranch::Block(block) => self.block(block),
                ElseBranch::If(expression) => self.expression(expression),
            }
        }
    }

    fn match_expression(&mut self, expression: &MatchExpression) {
        self.write("match ");
        self.expression(&expression.value);
        self.write(" ");
        self.open_multiline_body();
        for arm in &expression.arms {
            self.pattern(&arm.pattern);
            self.write(" => ");
            self.expression(&arm.value);
            self.write(",");
            self.newline();
        }
        self.close_multiline_body();
    }

    fn patterns(&mut self, patterns: &[Pattern]) {
        for (index, pattern) in patterns.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            self.pattern(pattern);
        }
    }

    fn pattern(&mut self, pattern: &Pattern) {
        match &pattern.kind {
            PatternKind::Alternative(values) => {
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        self.write(" | ");
                    }
                    self.atomic_pattern(value);
                }
            }
            _ => self.atomic_pattern(pattern),
        }
    }

    fn atomic_pattern(&mut self, pattern: &Pattern) {
        match &pattern.kind {
            PatternKind::Wildcard => self.write("_"),
            PatternKind::Binder(value) => self.identifier(value),
            PatternKind::Literal(value) => self.pattern_literal(value),
            PatternKind::Group(value) => {
                self.write("(");
                self.pattern(value);
                self.write(")");
            }
            PatternKind::Tuple(values) => {
                self.write("(");
                self.patterns(values);
                self.write(")");
            }
            PatternKind::Constructor(value) => self.qualified_name(value),
            PatternKind::TupleConstructor {
                constructor,
                arguments,
            } => {
                self.qualified_name(constructor);
                self.write("(");
                self.patterns(arguments);
                self.write(")");
            }
            PatternKind::Record {
                constructor,
                fields,
                rest,
            } => {
                self.qualified_name(constructor);
                self.write(" ");
                self.open_record_body();
                for field in fields {
                    self.identifier(&field.name);
                    if let Some(value) = &field.pattern {
                        self.write(": ");
                        self.pattern(value);
                    }
                    self.write(",");
                    self.newline();
                }
                if *rest {
                    self.write("..,");
                    self.newline();
                }
                self.close_record_body(fields.is_empty() && !rest);
            }
            PatternKind::AnonymousRecord { fields, rest } => {
                self.open_record_body();
                for field in fields {
                    self.identifier(&field.name);
                    if let Some(value) = &field.pattern {
                        self.write(": ");
                        self.pattern(value);
                    }
                    self.write(",");
                    self.newline();
                }
                if *rest {
                    self.write("..,");
                    self.newline();
                }
                self.close_record_body(fields.is_empty() && !rest);
            }
            PatternKind::Alternative(_) => {
                self.write("(");
                self.pattern(pattern);
                self.write(")");
            }
        }
    }

    fn pattern_literal(&mut self, literal: &PatternLiteral) {
        match literal {
            PatternLiteral::Bool(value) => self.write(if *value { "true" } else { "false" }),
            PatternLiteral::Integer { raw, negative }
            | PatternLiteral::Decimal { raw, negative } => {
                if *negative {
                    self.write("-");
                }
                self.write(raw);
            }
            PatternLiteral::Text { value, .. } => self.text(value),
            PatternLiteral::Unit => self.write("()"),
        }
    }

    fn qualified_name(&mut self, name: &QualifiedName) {
        for (index, segment) in name.segments.iter().enumerate() {
            if index > 0 {
                self.write("::");
            }
            self.identifier(segment);
        }
    }

    fn identifier(&mut self, identifier: &Identifier) {
        self.write(&identifier.text);
    }

    fn visibility(&mut self, visibility: Visibility) {
        if visibility == Visibility::Public {
            self.write("pub ");
        }
    }

    fn open_multiline_body(&mut self) {
        self.write("{");
        self.newline();
        self.indent += 1;
    }

    fn close_multiline_body(&mut self) {
        if !self.line_start {
            self.newline();
        }
        self.indent -= 1;
        self.write("}");
    }

    fn open_record_body(&mut self) {
        self.write("{");
        self.newline();
        self.indent += 1;
    }

    fn close_record_body(&mut self, empty: bool) {
        self.indent -= 1;
        if empty {
            // Remove the newline introduced by `open_record_body` so empty
            // record/port forms retain their unambiguous compact spelling.
            if self.output.ends_with('\n') {
                self.output.pop();
            }
            self.line_start = false;
        }
        self.write("}");
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Precedence {
    Return,
    Or,
    And,
    Comparison,
    Additive,
    Multiplicative,
    Unary,
    Postfix,
    Primary,
}

impl Precedence {
    fn next(self) -> Self {
        match self {
            Self::Return => Self::Or,
            Self::Or => Self::And,
            Self::And => Self::Comparison,
            Self::Comparison => Self::Additive,
            Self::Additive => Self::Multiplicative,
            Self::Multiplicative => Self::Unary,
            Self::Unary => Self::Postfix,
            Self::Postfix | Self::Primary => Self::Primary,
        }
    }
}

fn expression_precedence(expression: &ExpressionKind) -> Precedence {
    match expression {
        ExpressionKind::Return(_) => Precedence::Return,
        ExpressionKind::Binary {
            operator: BinaryOperator::Or,
            ..
        } => Precedence::Or,
        ExpressionKind::Binary {
            operator: BinaryOperator::And,
            ..
        } => Precedence::And,
        ExpressionKind::Compare { .. } | ExpressionKind::Is { .. } => Precedence::Comparison,
        ExpressionKind::Binary {
            operator: BinaryOperator::Add | BinaryOperator::Subtract,
            ..
        } => Precedence::Additive,
        ExpressionKind::Binary {
            operator: BinaryOperator::Multiply,
            ..
        } => Precedence::Multiplicative,
        ExpressionKind::Unary { .. } => Precedence::Unary,
        ExpressionKind::Call { .. }
        | ExpressionKind::Member { .. }
        | ExpressionKind::Index { .. } => Precedence::Postfix,
        ExpressionKind::Literal(_)
        | ExpressionKind::Unit
        | ExpressionKind::Sequence(_)
        | ExpressionKind::Tuple(_)
        | ExpressionKind::Group(_)
        | ExpressionKind::Name(_)
        | ExpressionKind::Record(_)
        | ExpressionKind::AnonymousRecord(_)
        | ExpressionKind::Block(_)
        | ExpressionKind::If(_)
        | ExpressionKind::Match(_) => Precedence::Primary,
    }
}

fn binary_precedence(operator: BinaryOperator) -> Precedence {
    match operator {
        BinaryOperator::Multiply => Precedence::Multiplicative,
        BinaryOperator::Add | BinaryOperator::Subtract => Precedence::Additive,
        BinaryOperator::And => Precedence::And,
        BinaryOperator::Or => Precedence::Or,
    }
}

fn ui_node_is_inline(node: &UiNode) -> bool {
    matches!(
        node.kind,
        UiNodeKind::Text(_) | UiNodeKind::Interpolation(_)
    )
}

fn normalize_ui_text(value: &str) -> String {
    let mut output = String::new();
    let mut whitespace = false;
    for value in value.chars() {
        if matches!(value, ' ' | '\t' | '\n' | '\r') {
            whitespace = true;
        } else {
            if whitespace {
                output.push(' ');
            }
            output.push(value);
            whitespace = false;
        }
    }
    if whitespace {
        output.push(' ');
    }
    output
}

fn normalize_markup_text(value: &str) -> String {
    let value = value.replace("\r\n", "\n").replace('\r', "\n");
    if !value.contains('\n') {
        return value.trim_matches([' ', '\t']).to_string();
    }

    let mut lines = value.split('\n').collect::<Vec<_>>();
    while lines
        .first()
        .is_some_and(|line| line.chars().all(|value| matches!(value, ' ' | '\t')))
    {
        lines.remove(0);
    }
    while lines
        .last()
        .is_some_and(|line| line.chars().all(|value| matches!(value, ' ' | '\t')))
    {
        lines.pop();
    }
    let mut lines = lines
        .into_iter()
        .map(|line| line.trim_end_matches([' ', '\t']).to_string())
        .collect::<Vec<_>>();
    let common = lines
        .iter()
        .filter(|line| !line.is_empty())
        .map(|line| line.bytes().take_while(|value| *value == b' ').count())
        .min()
        .unwrap_or(0);
    for line in &mut lines {
        let remove = line
            .bytes()
            .take_while(|value| *value == b' ')
            .count()
            .min(common);
        line.drain(..remove);
    }
    lines.join("\n")
}
