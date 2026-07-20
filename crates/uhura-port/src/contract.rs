//! Serializable Uhura contract declarations and instance admission.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use uhura_base::hash_json;

use super::canonical::CanonicalJson;

/// A checked canonical Uhura type spelling.
///
/// `uhura-port` intentionally does not depend on the Uhura checker. Keeping
/// the spelling opaque lets the checker own the full type algebra while this
/// crate can still serialize, hash, instantiate, and compare contracts.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TypeRef(String);

impl TypeRef {
    pub fn new(value: impl Into<String>) -> Result<Self, ContractModelError> {
        let value = value.into();
        if value.is_empty()
            || value.trim() != value
            || value
                .chars()
                .any(|character| character.is_control() || character.is_whitespace())
        {
            return Err(ContractModelError::new(
                "type",
                format!("`{value}` is not a canonical Uhura type spelling"),
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn substitute(&self, arguments: &BTreeMap<&str, &TypeRef>) -> Self {
        let mut output = String::with_capacity(self.0.len());
        let mut token = String::new();
        let flush = |token: &mut String, output: &mut String| {
            if token.is_empty() {
                return;
            }
            if let Some(argument) = arguments.get(token.as_str()) {
                output.push_str(argument.as_str());
            } else {
                output.push_str(token);
            }
            token.clear();
        };

        for character in self.0.chars() {
            if character == '_' || character.is_alphanumeric() {
                token.push(character);
            } else {
                flush(&mut token, &mut output);
                output.push(character);
            }
        }
        flush(&mut token, &mut output);
        Self(output)
    }
}

impl fmt::Display for TypeRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for TypeRef {
    type Error = ContractModelError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<TypeRef> for String {
    fn from(value: TypeRef) -> Self {
        value.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ContractIdentity {
    pub module: String,
    pub major: u32,
    pub name: String,
}

impl ContractIdentity {
    pub fn new(
        module: impl Into<String>,
        major: u32,
        name: impl Into<String>,
    ) -> Result<Self, ContractModelError> {
        let identity = Self {
            module: module.into(),
            major,
            name: name.into(),
        };
        identity.validate()?;
        Ok(identity)
    }

    pub fn validate(&self) -> Result<(), ContractModelError> {
        if self.major == 0 {
            return Err(ContractModelError::new(
                "identity.major",
                "an Uhura module major must be positive",
            ));
        }
        if self.module.is_empty() || !self.module.split('.').all(valid_symbol) {
            return Err(ContractModelError::new(
                "identity.module",
                format!("`{}` is not a logical Uhura module identity", self.module),
            ));
        }
        validate_symbol("identity.name", &self.name)
    }

    pub fn canonical_name(&self) -> String {
        format!("{}@{}::{}", self.module, self.major, self.name)
    }
}

impl fmt::Display for ContractIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.canonical_name())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct FieldDecl {
    pub name: String,
    pub ty: TypeRef,
}

impl FieldDecl {
    pub fn new(name: impl Into<String>, ty: TypeRef) -> Self {
        Self {
            name: name.into(),
            ty,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ConstructorDecl {
    pub name: String,
    #[serde(default)]
    pub fields: Vec<FieldDecl>,
}

impl ConstructorDecl {
    pub fn new(name: impl Into<String>, fields: Vec<FieldDecl>) -> Self {
        Self {
            name: name.into(),
            fields,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SumDecl {
    /// An empty constructor list is Uhura's `Never`.
    #[serde(default)]
    pub constructors: Vec<ConstructorDecl>,
}

impl SumDecl {
    pub fn never() -> Self {
        Self::default()
    }

    pub fn constructors(constructors: Vec<ConstructorDecl>) -> Self {
        Self { constructors }
    }

    pub fn is_never(&self) -> bool {
        self.constructors.is_empty()
    }

    pub fn constructor(&self, name: &str) -> Option<&ConstructorDecl> {
        self.constructors
            .iter()
            .find(|constructor| constructor.name == name)
    }

    fn substitute(&self, arguments: &BTreeMap<&str, &TypeRef>) -> Self {
        Self {
            constructors: self
                .constructors
                .iter()
                .map(|constructor| ConstructorDecl {
                    name: constructor.name.clone(),
                    fields: constructor
                        .fields
                        .iter()
                        .map(|field| FieldDecl {
                            name: field.name.clone(),
                            ty: field.ty.substitute(arguments),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    fn validate(&self, path: &str) -> Result<(), ContractModelError> {
        let mut constructor_names = BTreeSet::new();
        for constructor in &self.constructors {
            validate_symbol(&format!("{path}.{}", constructor.name), &constructor.name)?;
            if !constructor_names.insert(&constructor.name) {
                return Err(ContractModelError::new(
                    path,
                    format!("duplicate constructor `{}`", constructor.name),
                ));
            }

            let mut field_names = BTreeSet::new();
            for field in &constructor.fields {
                validate_symbol(
                    &format!("{path}.{}.{}", constructor.name, field.name),
                    &field.name,
                )?;
                if !field_names.insert(&field.name) {
                    return Err(ContractModelError::new(
                        format!("{path}.{}", constructor.name),
                        format!("duplicate field `{}`", field.name),
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PureHelperDecl {
    pub name: String,
    pub signature: String,
    pub semantic_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct CodecDecl {
    pub name: String,
    pub target: TypeRef,
    pub semantic_id: String,
    /// The immutable port configuration contributes to this resolved codec.
    #[serde(default)]
    pub configuration_scoped: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct UiAttributeDecl {
    pub name: String,
    pub ty: TypeRef,
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<CanonicalJson>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct CheckedUiDecl {
    pub name: String,
    #[serde(default)]
    pub attributes: Vec<UiAttributeDecl>,
    #[serde(default)]
    pub events: SumDecl,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PortContract {
    pub identity: ContractIdentity,
    #[serde(default)]
    pub type_parameters: Vec<String>,
    pub configuration: TypeRef,
    #[serde(default)]
    pub receive: SumDecl,
    #[serde(default)]
    pub send: SumDecl,
    #[serde(default)]
    pub pure_helpers: Vec<PureHelperDecl>,
    #[serde(default)]
    pub codecs: Vec<CodecDecl>,
    #[serde(default)]
    pub checked_ui: Vec<CheckedUiDecl>,
}

impl PortContract {
    pub fn validate(&self) -> Result<(), ContractModelError> {
        self.identity.validate()?;
        let mut parameters = BTreeSet::new();
        for parameter in &self.type_parameters {
            validate_symbol("type-parameters", parameter)?;
            if !parameters.insert(parameter) {
                return Err(ContractModelError::new(
                    "type-parameters",
                    format!("duplicate type parameter `{parameter}`"),
                ));
            }
        }
        self.receive.validate("receive")?;
        self.send.validate("send")?;
        validate_named_semantics("pure-helpers", &self.pure_helpers, |helper| {
            (&helper.name, &helper.semantic_id)
        })?;
        validate_named_semantics("codecs", &self.codecs, |codec| {
            (&codec.name, &codec.semantic_id)
        })?;

        let mut ui_names = BTreeSet::new();
        for element in &self.checked_ui {
            validate_symbol("checked-ui", &element.name)?;
            if !ui_names.insert(&element.name) {
                return Err(ContractModelError::new(
                    "checked-ui",
                    format!("duplicate checked element `{}`", element.name),
                ));
            }
            let mut attributes = BTreeSet::new();
            for attribute in &element.attributes {
                validate_symbol(
                    &format!("checked-ui.{}.attributes", element.name),
                    &attribute.name,
                )?;
                if !attributes.insert(&attribute.name) {
                    return Err(ContractModelError::new(
                        format!("checked-ui.{}.attributes", element.name),
                        format!("duplicate attribute `{}`", attribute.name),
                    ));
                }
                if attribute.required && attribute.default.is_some() {
                    return Err(ContractModelError::new(
                        format!("checked-ui.{}.attributes.{}", element.name, attribute.name),
                        "a required attribute cannot have a default",
                    ));
                }
            }
            element
                .events
                .validate(&format!("checked-ui.{}.events", element.name))?;
        }
        Ok(())
    }

    /// Stable declaration content identity used by host and fixture binding.
    ///
    /// The explicit frame prevents this JSON bridge from colliding with the
    /// previous Uhura port format. Field/constructor order remains semantic.
    pub fn content_hash(&self) -> String {
        let declaration =
            serde_json::to_value(self).expect("Uhura contract declarations are serializable");
        hash_json(&serde_json::json!({
            "frame": "uhura-port-contract/1",
            "declaration": declaration,
        }))
    }

    pub fn instantiate(
        &self,
        type_arguments: Vec<TypeRef>,
        configuration: CanonicalJson,
    ) -> Result<ContractInstance, ContractModelError> {
        self.validate()?;
        if type_arguments.len() != self.type_parameters.len() {
            return Err(ContractModelError::new(
                "type-arguments",
                format!(
                    "{} expects {} type argument(s), received {}",
                    self.identity,
                    self.type_parameters.len(),
                    type_arguments.len()
                ),
            ));
        }

        let substitution: BTreeMap<&str, &TypeRef> = self
            .type_parameters
            .iter()
            .map(String::as_str)
            .zip(type_arguments.iter())
            .collect();
        let resolved_type_arguments: Vec<TypeArgument> = self
            .type_parameters
            .iter()
            .cloned()
            .zip(type_arguments.iter().cloned())
            .map(|(parameter, argument)| TypeArgument {
                parameter,
                argument,
            })
            .collect();
        let configuration_hash = configuration.hash();
        let codecs = self
            .codecs
            .iter()
            .map(|codec| ResolvedCodec {
                name: codec.name.clone(),
                target: codec.target.substitute(&substitution),
                semantic_id: codec.semantic_id.clone(),
                configuration_hash: codec
                    .configuration_scoped
                    .then(|| configuration_hash.clone()),
            })
            .collect();

        Ok(ContractInstance {
            identity: self.identity.clone(),
            content_hash: self.content_hash(),
            type_arguments: resolved_type_arguments,
            configuration_type: self.configuration.substitute(&substitution),
            configuration,
            receive: self.receive.substitute(&substitution),
            send: self.send.substitute(&substitution),
            codecs,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TypeArgument {
    pub parameter: String,
    pub argument: TypeRef,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ResolvedCodec {
    pub name: String,
    pub target: TypeRef,
    pub semantic_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration_hash: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ContractInstance {
    pub identity: ContractIdentity,
    pub content_hash: String,
    #[serde(default)]
    pub type_arguments: Vec<TypeArgument>,
    pub configuration_type: TypeRef,
    pub configuration: CanonicalJson,
    #[serde(default)]
    pub receive: SumDecl,
    #[serde(default)]
    pub send: SumDecl,
    #[serde(default)]
    pub codecs: Vec<ResolvedCodec>,
}

impl ContractInstance {
    pub fn instance_hash(&self) -> String {
        let value = serde_json::to_value(self).expect("Uhura contract instances are serializable");
        hash_json(&serde_json::json!({
            "frame": "uhura-port-instance/1",
            "instance": value,
        }))
    }

    pub fn compatibility(&self) -> ContractCompatibility {
        ContractCompatibility {
            identity: self.identity.clone(),
            content_hash: self.content_hash.clone(),
            type_arguments: self.type_arguments.clone(),
            configuration_hash: self.configuration.hash(),
            codecs: self.codecs.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ContractCompatibility {
    pub identity: ContractIdentity,
    pub content_hash: String,
    #[serde(default)]
    pub type_arguments: Vec<TypeArgument>,
    pub configuration_hash: String,
    #[serde(default)]
    pub codecs: Vec<ResolvedCodec>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PortDeclaration {
    pub name: String,
    pub contract: ContractInstance,
}

impl PortDeclaration {
    pub fn new(
        name: impl Into<String>,
        contract: ContractInstance,
    ) -> Result<Self, ContractModelError> {
        let declaration = Self {
            name: name.into(),
            contract,
        };
        validate_symbol("port.name", &declaration.name)?;
        Ok(declaration)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PortBinding {
    pub port: String,
    pub adapter: String,
    pub contract: ContractCompatibility,
}

impl PortBinding {
    pub fn for_instance(
        port: impl Into<String>,
        adapter: impl Into<String>,
        instance: &ContractInstance,
    ) -> Result<Self, ContractModelError> {
        let binding = Self {
            port: port.into(),
            adapter: adapter.into(),
            contract: instance.compatibility(),
        };
        validate_symbol("binding.port", &binding.port)?;
        if binding.adapter.trim().is_empty() || binding.adapter.trim() != binding.adapter {
            return Err(ContractModelError::new(
                "binding.adapter",
                "an adapter identity must be non-empty and have no edge whitespace",
            ));
        }
        Ok(binding)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AdmissionIssueCode {
    DuplicatePortDeclaration,
    PortUnbound,
    DuplicatePortBinding,
    UnexpectedPortBinding,
    AdapterIdentityInvalid,
    ContractIdentityMismatch,
    ContractContentMismatch,
    TypeArgumentsMismatch,
    ConfigurationMismatch,
    CanonicalCodecMismatch,
}

impl AdmissionIssueCode {
    pub fn diagnostic_code(self) -> &'static str {
        match self {
            Self::PortUnbound => "R3005",
            Self::ContractIdentityMismatch
            | Self::ContractContentMismatch
            | Self::TypeArgumentsMismatch
            | Self::ConfigurationMismatch
            | Self::CanonicalCodecMismatch
            | Self::AdapterIdentityInvalid => "R3015",
            Self::DuplicatePortDeclaration
            | Self::DuplicatePortBinding
            | Self::UnexpectedPortBinding => "R1002",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct AdmissionIssue {
    pub code: AdmissionIssueCode,
    pub diagnostic: String,
    pub port: String,
    pub message: String,
}

impl AdmissionIssue {
    fn new(code: AdmissionIssueCode, port: &str, message: impl Into<String>) -> Self {
        Self {
            code,
            diagnostic: code.diagnostic_code().to_string(),
            port: port.to_string(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct AdmittedBinding {
    pub port: String,
    pub adapter: String,
    pub contract_instance_hash: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct AdmittedPortSet {
    #[serde(default)]
    pub bindings: Vec<AdmittedBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct AdmissionReport {
    pub accepted: bool,
    #[serde(default)]
    pub bindings: Vec<AdmittedBinding>,
    #[serde(default)]
    pub issues: Vec<AdmissionIssue>,
}

impl AdmissionReport {
    pub fn into_result(self) -> Result<AdmittedPortSet, Vec<AdmissionIssue>> {
        if self.accepted {
            Ok(AdmittedPortSet {
                bindings: self.bindings,
            })
        } else {
            Err(self.issues)
        }
    }
}

/// Verifies the complete port set before genesis.
///
/// The operation never returns a partial admitted set: if one declaration or
/// binding is wrong, `bindings` is empty and every deterministic issue found
/// by this pass is reported.
pub fn admit_bindings(
    declarations: &[PortDeclaration],
    bindings: &[PortBinding],
) -> AdmissionReport {
    let mut issues = Vec::new();
    let mut declarations_by_name: BTreeMap<&str, Vec<&PortDeclaration>> = BTreeMap::new();
    let mut bindings_by_name: BTreeMap<&str, Vec<&PortBinding>> = BTreeMap::new();

    for declaration in declarations {
        declarations_by_name
            .entry(&declaration.name)
            .or_default()
            .push(declaration);
    }
    for binding in bindings {
        bindings_by_name
            .entry(&binding.port)
            .or_default()
            .push(binding);
    }

    for (name, entries) in &declarations_by_name {
        if entries.len() != 1 {
            issues.push(AdmissionIssue::new(
                AdmissionIssueCode::DuplicatePortDeclaration,
                name,
                format!("port `{name}` is declared {} times", entries.len()),
            ));
        }
    }

    let mut admitted = Vec::new();
    for (name, declaration_entries) in &declarations_by_name {
        if declaration_entries.len() != 1 {
            continue;
        }
        let declaration = declaration_entries[0];
        let Some(binding_entries) = bindings_by_name.get(name) else {
            issues.push(AdmissionIssue::new(
                AdmissionIssueCode::PortUnbound,
                name,
                format!("required port `{name}` has no binding"),
            ));
            continue;
        };
        if binding_entries.len() != 1 {
            issues.push(AdmissionIssue::new(
                AdmissionIssueCode::DuplicatePortBinding,
                name,
                format!("port `{name}` has {} bindings", binding_entries.len()),
            ));
            continue;
        }

        let binding = binding_entries[0];
        let expected = declaration.contract.compatibility();
        let actual = &binding.contract;
        let before = issues.len();
        if binding.adapter.trim().is_empty() || binding.adapter.trim() != binding.adapter {
            issues.push(AdmissionIssue::new(
                AdmissionIssueCode::AdapterIdentityInvalid,
                name,
                "adapter identity is empty or has edge whitespace",
            ));
        }
        compare_compatibility(name, &expected, actual, &mut issues);
        if issues.len() == before {
            admitted.push(AdmittedBinding {
                port: (*name).to_string(),
                adapter: binding.adapter.clone(),
                contract_instance_hash: declaration.contract.instance_hash(),
            });
        }
    }

    for name in bindings_by_name.keys() {
        if !declarations_by_name.contains_key(name) {
            issues.push(AdmissionIssue::new(
                AdmissionIssueCode::UnexpectedPortBinding,
                name,
                format!("binding targets undeclared port `{name}`"),
            ));
        }
    }

    if issues.is_empty() {
        AdmissionReport {
            accepted: true,
            bindings: admitted,
            issues,
        }
    } else {
        AdmissionReport {
            accepted: false,
            bindings: Vec::new(),
            issues,
        }
    }
}

fn compare_compatibility(
    port: &str,
    expected: &ContractCompatibility,
    actual: &ContractCompatibility,
    issues: &mut Vec<AdmissionIssue>,
) {
    if expected.identity != actual.identity {
        issues.push(AdmissionIssue::new(
            AdmissionIssueCode::ContractIdentityMismatch,
            port,
            format!(
                "adapter declares `{}`, expected `{}`",
                actual.identity, expected.identity
            ),
        ));
    }
    if expected.content_hash != actual.content_hash {
        issues.push(AdmissionIssue::new(
            AdmissionIssueCode::ContractContentMismatch,
            port,
            "adapter contract content hash does not match",
        ));
    }
    if expected.type_arguments != actual.type_arguments {
        issues.push(AdmissionIssue::new(
            AdmissionIssueCode::TypeArgumentsMismatch,
            port,
            "adapter contract type arguments do not match",
        ));
    }
    if expected.configuration_hash != actual.configuration_hash {
        issues.push(AdmissionIssue::new(
            AdmissionIssueCode::ConfigurationMismatch,
            port,
            "adapter contract configuration does not match",
        ));
    }
    if expected.codecs != actual.codecs {
        issues.push(AdmissionIssue::new(
            AdmissionIssueCode::CanonicalCodecMismatch,
            port,
            "adapter canonical codecs do not match",
        ));
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractModelError {
    pub path: String,
    pub message: String,
}

impl ContractModelError {
    pub fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for ContractModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for ContractModelError {}

fn validate_named_semantics<T>(
    path: &str,
    values: &[T],
    fields: impl Fn(&T) -> (&String, &String),
) -> Result<(), ContractModelError> {
    let mut names = BTreeSet::new();
    for value in values {
        let (name, semantic_id) = fields(value);
        validate_symbol(path, name)?;
        if !names.insert(name) {
            return Err(ContractModelError::new(
                path,
                format!("duplicate declaration `{name}`"),
            ));
        }
        if semantic_id.trim().is_empty() || semantic_id.trim() != semantic_id {
            return Err(ContractModelError::new(
                path,
                format!("`{name}` has an invalid semantic identity"),
            ));
        }
    }
    Ok(())
}

fn validate_symbol(path: &str, value: &str) -> Result<(), ContractModelError> {
    if valid_symbol(value) {
        Ok(())
    } else {
        Err(ContractModelError::new(
            path,
            format!("`{value}` is not an Uhura identifier"),
        ))
    }
}

fn valid_symbol(value: &str) -> bool {
    let mut characters = value.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first == '_' || first.is_alphabetic())
        && characters.all(|character| character == '_' || character.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ty(value: &str) -> TypeRef {
        TypeRef::new(value).unwrap()
    }

    fn test_contract() -> PortContract {
        PortContract {
            identity: ContractIdentity::new("example.service", 1, "Request").unwrap(),
            type_parameters: vec!["Payload".to_string()],
            configuration: ty("Unit"),
            receive: SumDecl::constructors(vec![ConstructorDecl::new(
                "settled",
                vec![FieldDecl::new("value", ty("Payload"))],
            )]),
            send: SumDecl::constructors(vec![ConstructorDecl::new(
                "request",
                vec![FieldDecl::new("value", ty("Payload"))],
            )]),
            pure_helpers: Vec::new(),
            codecs: vec![CodecDecl {
                name: "payload".to_string(),
                target: ty("Payload"),
                semantic_id: "uhura.canonical-value@1".to_string(),
                configuration_scoped: false,
            }],
            checked_ui: Vec::new(),
        }
    }

    #[test]
    fn instantiation_substitutes_the_resolved_interface() {
        let instance = test_contract()
            .instantiate(vec![ty("app@1::Message")], CanonicalJson::unit())
            .unwrap();
        assert_eq!(
            instance.receive.constructors[0].fields[0].ty.as_str(),
            "app@1::Message"
        );
        assert_eq!(instance.codecs[0].target.as_str(), "app@1::Message");
        assert_eq!(
            instance.type_arguments,
            vec![TypeArgument {
                parameter: "Payload".to_string(),
                argument: ty("app@1::Message"),
            }]
        );
    }

    #[test]
    fn declaration_hash_is_stable_and_order_sensitive() {
        let first = test_contract();
        let mut second = first.clone();
        assert_eq!(first.content_hash(), second.content_hash());
        second.send.constructors[0]
            .fields
            .push(FieldDecl::new("id", ty("Int")));
        assert_ne!(first.content_hash(), second.content_hash());
    }

    #[test]
    fn declaration_instance_binding_and_report_are_serializable() {
        let contract = test_contract();
        let contract_json = serde_json::to_value(&contract).unwrap();
        assert_eq!(
            serde_json::from_value::<PortContract>(contract_json).unwrap(),
            contract
        );

        let instance = contract
            .instantiate(vec![ty("Text")], CanonicalJson::unit())
            .unwrap();
        let declaration = PortDeclaration::new("service", instance.clone()).unwrap();
        let binding = PortBinding::for_instance("service", "fixture.service", &instance).unwrap();
        let report = admit_bindings(&[declaration], &[binding]);
        let report_json = serde_json::to_value(&report).unwrap();
        assert_eq!(
            serde_json::from_value::<AdmissionReport>(report_json).unwrap(),
            report
        );

        assert!(serde_json::from_str::<TypeRef>(r#"" Option<Text> ""#).is_err());
    }

    #[test]
    fn admission_is_all_or_nothing_and_checks_codecs() {
        let instance = test_contract()
            .instantiate(vec![ty("Text")], CanonicalJson::unit())
            .unwrap();
        let declaration = PortDeclaration::new("service", instance.clone()).unwrap();
        let compatible =
            PortBinding::for_instance("service", "fixture.service", &instance).unwrap();
        let report = admit_bindings(
            std::slice::from_ref(&declaration),
            std::slice::from_ref(&compatible),
        );
        assert!(report.accepted);
        assert_eq!(report.bindings.len(), 1);

        let mut incompatible = compatible;
        incompatible.contract.codecs[0].semantic_id = "host-json".to_string();
        let report = admit_bindings(&[declaration], &[incompatible]);
        assert!(!report.accepted);
        assert!(report.bindings.is_empty());
        assert_eq!(
            report.issues[0].code,
            AdmissionIssueCode::CanonicalCodecMismatch
        );
        assert_eq!(report.issues[0].diagnostic, "R3015");
    }

    #[test]
    fn admission_checks_instantiated_type_arguments_and_configuration() {
        let instance = test_contract()
            .instantiate(vec![ty("Text")], CanonicalJson::unit())
            .unwrap();
        let declaration = PortDeclaration::new("service", instance.clone()).unwrap();

        let mut wrong_type =
            PortBinding::for_instance("service", "fixture.service", &instance).unwrap();
        wrong_type.contract.type_arguments[0].argument = ty("Int");
        let type_report = admit_bindings(std::slice::from_ref(&declaration), &[wrong_type]);
        assert!(!type_report.accepted);
        assert!(type_report.bindings.is_empty());
        assert_eq!(
            type_report.issues[0].code,
            AdmissionIssueCode::TypeArgumentsMismatch
        );
        assert_eq!(type_report.issues[0].diagnostic, "R3015");

        let mut wrong_configuration =
            PortBinding::for_instance("service", "fixture.service", &instance).unwrap();
        wrong_configuration.contract.configuration_hash = "0".repeat(64);
        let configuration_report =
            admit_bindings(std::slice::from_ref(&declaration), &[wrong_configuration]);
        assert!(!configuration_report.accepted);
        assert!(configuration_report.bindings.is_empty());
        assert_eq!(
            configuration_report.issues[0].code,
            AdmissionIssueCode::ConfigurationMismatch
        );
        assert_eq!(configuration_report.issues[0].diagnostic, "R3015");
    }

    #[test]
    fn admission_requires_exactly_one_binding_per_port() {
        let instance = test_contract()
            .instantiate(vec![ty("Text")], CanonicalJson::unit())
            .unwrap();
        let declaration = PortDeclaration::new("service", instance.clone()).unwrap();
        let binding = PortBinding::for_instance("service", "fixture.service", &instance).unwrap();

        let missing = admit_bindings(std::slice::from_ref(&declaration), &[]);
        assert_eq!(missing.issues[0].code, AdmissionIssueCode::PortUnbound);

        let duplicate = admit_bindings(
            std::slice::from_ref(&declaration),
            &[binding.clone(), binding],
        );
        assert_eq!(
            duplicate.issues[0].code,
            AdmissionIssueCode::DuplicatePortBinding
        );
    }
}
