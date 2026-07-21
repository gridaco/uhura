//! The pinned `uhura.web_router@1` route-pattern and URL component codec.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::contract::TypeRef;

pub const OPAQUE_PATH_CODEC: &str = "uhura.web-router.opaque-path-component@1";
pub const QUERY_VALUE_CODEC: &str = "uhura.web-router.query-value@1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum RouteFieldKind {
    Text,
    TextKey { type_name: TypeRef },
    OptionalText,
    OptionalTextKey { type_name: TypeRef },
}

impl RouteFieldKind {
    fn is_optional(&self) -> bool {
        matches!(self, Self::OptionalText | Self::OptionalTextKey { .. })
    }

    fn decode_required(&self, text: String) -> Result<RouteFieldValue, RouteError> {
        match self {
            Self::Text => Ok(RouteFieldValue::Required(RouteAtom::Text { value: text })),
            Self::TextKey { type_name } => Ok(RouteFieldValue::Required(RouteAtom::Key {
                type_name: type_name.clone(),
                value: text,
            })),
            Self::OptionalText | Self::OptionalTextKey { .. } => Err(RouteError::new(
                RouteErrorCode::IncompatibleFieldType,
                "an optional route field cannot occupy a path segment",
            )),
        }
    }

    fn decode_optional(&self, text: Option<String>) -> Result<RouteFieldValue, RouteError> {
        match (self, text) {
            (Self::OptionalText, Some(value)) => {
                Ok(RouteFieldValue::Optional(Some(RouteAtom::Text { value })))
            }
            (Self::OptionalText, None) => Ok(RouteFieldValue::Optional(None)),
            (Self::OptionalTextKey { type_name }, Some(value)) => {
                Ok(RouteFieldValue::Optional(Some(RouteAtom::Key {
                    type_name: type_name.clone(),
                    value,
                })))
            }
            (Self::OptionalTextKey { .. }, None) => Ok(RouteFieldValue::Optional(None)),
            (Self::Text | Self::TextKey { .. }, _) => Err(RouteError::new(
                RouteErrorCode::IncompatibleFieldType,
                "a required route field cannot occupy an optional query",
            )),
        }
    }

    fn text<'a>(&self, value: &'a RouteFieldValue) -> Result<Option<&'a str>, RouteError> {
        match (self, value) {
            (Self::Text, RouteFieldValue::Required(RouteAtom::Text { value })) => Ok(Some(value)),
            (
                Self::TextKey {
                    type_name: expected,
                },
                RouteFieldValue::Required(RouteAtom::Key { type_name, value }),
            ) if expected == type_name => Ok(Some(value)),
            (Self::OptionalText, RouteFieldValue::Optional(None)) => Ok(None),
            (Self::OptionalText, RouteFieldValue::Optional(Some(RouteAtom::Text { value }))) => {
                Ok(Some(value))
            }
            (Self::OptionalTextKey { .. }, RouteFieldValue::Optional(None)) => Ok(None),
            (
                Self::OptionalTextKey {
                    type_name: expected,
                },
                RouteFieldValue::Optional(Some(RouteAtom::Key { type_name, value })),
            ) if expected == type_name => Ok(Some(value)),
            _ => Err(RouteError::new(
                RouteErrorCode::ValueShapeMismatch,
                "route field value does not match its declared text/key/option type",
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RouteFieldDecl {
    pub name: String,
    pub kind: RouteFieldKind,
}

impl RouteFieldDecl {
    pub fn new(name: impl Into<String>, kind: RouteFieldKind) -> Self {
        Self {
            name: name.into(),
            kind,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RouteConstructorDecl {
    pub name: String,
    #[serde(default)]
    pub fields: Vec<RouteFieldDecl>,
}

impl RouteConstructorDecl {
    pub fn new(name: impl Into<String>, fields: Vec<RouteFieldDecl>) -> Self {
        Self {
            name: name.into(),
            fields,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RoutePatternDecl {
    pub constructor: String,
    pub pattern: String,
}

impl RoutePatternDecl {
    pub fn new(constructor: impl Into<String>, pattern: impl Into<String>) -> Self {
        Self {
            constructor: constructor.into(),
            pattern: pattern.into(),
        }
    }
}

/// Checked immutable `Routes<Location>` configuration.
///
/// Custom deserialization recompiles the table; a serialized table cannot
/// bypass the same totality, field-use, and ambiguity checks as source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteTable {
    location_type: TypeRef,
    constructors: Vec<RouteConstructorDecl>,
    patterns: Vec<RoutePatternDecl>,
    checked_paths: Vec<CheckedRoutePath>,
}

/// The checked path-only meaning of one route pattern.
///
/// Query fields do not participate in pathname ownership. Hosts consume this
/// shape instead of reparsing the diagnostic source spelling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedRoutePath {
    constructor: String,
    parts: Vec<RoutePathPart>,
}

impl CheckedRoutePath {
    pub fn constructor(&self) -> &str {
        &self.constructor
    }

    pub fn parts(&self) -> &[RoutePathPart] {
        &self.parts
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoutePathPart {
    Literal(String),
    Field(String),
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct RouteTableRepr {
    location_type: TypeRef,
    constructors: Vec<RouteConstructorDecl>,
    patterns: Vec<RoutePatternDecl>,
}

impl Serialize for RouteTable {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        RouteTableRepr {
            location_type: self.location_type.clone(),
            constructors: self.constructors.clone(),
            patterns: self.patterns.clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RouteTable {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let repr = RouteTableRepr::deserialize(deserializer)?;
        Self::compile(repr.location_type, repr.constructors, repr.patterns)
            .map_err(serde::de::Error::custom)
    }
}

impl RouteTable {
    pub fn compile(
        location_type: TypeRef,
        constructors: Vec<RouteConstructorDecl>,
        patterns: Vec<RoutePatternDecl>,
    ) -> Result<Self, RouteError> {
        if constructors.is_empty() {
            return Err(RouteError::new(
                RouteErrorCode::EmptyLocation,
                "Routes<Location> needs at least one Location constructor",
            ));
        }

        let mut constructors_by_name = BTreeMap::new();
        for constructor in &constructors {
            validate_symbol(&constructor.name).map_err(|message| {
                RouteError::for_route(
                    RouteErrorCode::InvalidDeclaration,
                    &constructor.name,
                    message,
                )
            })?;
            if constructors_by_name
                .insert(constructor.name.as_str(), constructor)
                .is_some()
            {
                return Err(RouteError::for_route(
                    RouteErrorCode::DuplicateConstructor,
                    &constructor.name,
                    format!("Location constructor `{}` is duplicated", constructor.name),
                ));
            }
            let mut fields = BTreeSet::new();
            for field in &constructor.fields {
                validate_symbol(&field.name).map_err(|message| {
                    RouteError::for_route(
                        RouteErrorCode::InvalidDeclaration,
                        &constructor.name,
                        message,
                    )
                })?;
                if !fields.insert(&field.name) {
                    return Err(RouteError::for_route(
                        RouteErrorCode::DuplicateField,
                        &constructor.name,
                        format!(
                            "Location constructor `{}` repeats field `{}`",
                            constructor.name, field.name
                        ),
                    ));
                }
            }
        }

        let mut patterns_by_name = BTreeMap::new();
        for pattern in &patterns {
            if !constructors_by_name.contains_key(pattern.constructor.as_str()) {
                return Err(RouteError::for_route(
                    RouteErrorCode::UnknownConstructor,
                    &pattern.constructor,
                    format!(
                        "route pattern names unknown Location constructor `{}`",
                        pattern.constructor
                    ),
                ));
            }
            if patterns_by_name
                .insert(pattern.constructor.as_str(), pattern)
                .is_some()
            {
                return Err(RouteError::for_route(
                    RouteErrorCode::DuplicatePattern,
                    &pattern.constructor,
                    format!(
                        "Location constructor `{}` has more than one route pattern",
                        pattern.constructor
                    ),
                ));
            }
        }

        let mut canonical_patterns = Vec::with_capacity(constructors.len());
        let mut parsed = Vec::with_capacity(constructors.len());
        for constructor in &constructors {
            let pattern = patterns_by_name
                .get(constructor.name.as_str())
                .ok_or_else(|| {
                    RouteError::for_route(
                        RouteErrorCode::MissingPattern,
                        &constructor.name,
                        format!(
                            "Location constructor `{}` has no route pattern",
                            constructor.name
                        ),
                    )
                })?;
            let parsed_pattern = parse_pattern(constructor, &pattern.pattern)?;
            canonical_patterns.push((*pattern).clone());
            parsed.push((constructor.name.as_str(), parsed_pattern));
        }

        for left in 0..parsed.len() {
            for right in (left + 1)..parsed.len() {
                if path_patterns_overlap(&parsed[left].1, &parsed[right].1) {
                    return Err(RouteError::for_route(
                        RouteErrorCode::AmbiguousPattern,
                        parsed[left].0,
                        format!(
                            "route patterns for `{}` and `{}` can match the same path",
                            parsed[left].0, parsed[right].0
                        ),
                    ));
                }
            }
        }

        let checked_paths = parsed
            .into_iter()
            .map(|(constructor, parsed)| CheckedRoutePath {
                constructor: constructor.to_string(),
                parts: parsed.path,
            })
            .collect();
        Ok(Self {
            location_type,
            constructors,
            patterns: canonical_patterns,
            checked_paths,
        })
    }

    pub fn location_type(&self) -> &TypeRef {
        &self.location_type
    }

    pub fn constructors(&self) -> &[RouteConstructorDecl] {
        &self.constructors
    }

    pub fn patterns(&self) -> &[RoutePatternDecl] {
        &self.patterns
    }

    pub fn checked_paths(&self) -> &[CheckedRoutePath] {
        &self.checked_paths
    }

    /// Encodes one typed Location value to a canonical origin-form URL.
    pub fn encode(&self, location: &RouteLocation) -> Result<String, RouteError> {
        let (constructor, parsed) = self.resolved_route(&location.constructor)?;
        let declared_fields: BTreeSet<&str> = constructor
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect();
        for name in location.fields.keys() {
            if !declared_fields.contains(name.as_str()) {
                return Err(RouteError::for_route(
                    RouteErrorCode::UnknownField,
                    &constructor.name,
                    format!("Location value contains undeclared field `{name}`"),
                ));
            }
        }
        for field in &constructor.fields {
            if !location.fields.contains_key(&field.name) {
                return Err(RouteError::for_route(
                    RouteErrorCode::MissingField,
                    &constructor.name,
                    format!("Location value is missing field `{}`", field.name),
                ));
            }
        }

        let fields_by_name: BTreeMap<&str, &RouteFieldDecl> = constructor
            .fields
            .iter()
            .map(|field| (field.name.as_str(), field))
            .collect();
        let mut output = String::new();
        if parsed.path.is_empty() {
            output.push('/');
        } else {
            for part in &parsed.path {
                output.push('/');
                match part {
                    RoutePathPart::Literal(literal) => output.push_str(literal),
                    RoutePathPart::Field(name) => {
                        let declaration = fields_by_name[name.as_str()];
                        let value = &location.fields[name];
                        let Some(text) = declaration.kind.text(value)? else {
                            return Err(RouteError::for_route(
                                RouteErrorCode::ValueShapeMismatch,
                                &constructor.name,
                                format!("path field `{name}` cannot be absent"),
                            ));
                        };
                        output.push_str(&encode_opaque_path_component(text));
                    }
                }
            }
        }

        let mut separator = '?';
        for query in &parsed.query {
            let declaration = fields_by_name[query.field.as_str()];
            let value = &location.fields[&query.field];
            let Some(text) = declaration.kind.text(value)? else {
                continue;
            };
            output.push(separator);
            separator = '&';
            output.push_str(&encode_query_value(&query.key));
            output.push('=');
            output.push_str(&encode_query_value(text));
        }
        Ok(output)
    }

    /// Decodes a host-delivered origin-form URL into the closed Location sum.
    pub fn decode(&self, url: &str) -> Result<RouteLocation, RouteError> {
        if !url.starts_with('/') || url.contains('#') {
            return Err(RouteError::new(
                RouteErrorCode::MalformedUrl,
                "router ingress requires an origin-form path/query with no fragment",
            ));
        }
        let (path, query) = match url.split_once('?') {
            Some((path, query)) => (path, Some(query)),
            None => (url, None),
        };
        let path_segments = split_path(path)?;
        for segment in &path_segments {
            validate_percent_syntax(segment)?;
        }
        let query_pairs = parse_query(query)?;

        let mut matched: Option<(&RouteConstructorDecl, ParsedPattern)> = None;
        for constructor in &self.constructors {
            let (_, parsed) = self.resolved_route(&constructor.name)?;
            if path_matches(&parsed.path, &path_segments) {
                if matched.is_some() {
                    return Err(RouteError::new(
                        RouteErrorCode::AmbiguousPattern,
                        "more than one checked route matched host ingress",
                    ));
                }
                matched = Some((constructor, parsed));
            }
        }
        let Some((constructor, parsed)) = matched else {
            return Err(RouteError::new(
                RouteErrorCode::UnknownRoute,
                format!("`{path}` is not a declared route"),
            ));
        };

        let declarations: BTreeMap<&str, &RouteFieldDecl> = constructor
            .fields
            .iter()
            .map(|field| (field.name.as_str(), field))
            .collect();
        let mut fields = BTreeMap::new();
        for (part, segment) in parsed.path.iter().zip(path_segments.iter()) {
            if let RoutePathPart::Field(name) = part {
                let text = decode_opaque_path_component(segment).map_err(|mut error| {
                    error.route = Some(constructor.name.clone());
                    error
                })?;
                let value = declarations[name.as_str()].kind.decode_required(text)?;
                fields.insert(name.clone(), value);
            }
        }
        for query_part in &parsed.query {
            let values: Vec<&str> = query_pairs
                .iter()
                .filter(|(key, _)| key == &query_part.key)
                .map(|(_, value)| value.as_str())
                .collect();
            if values.len() > 1 {
                return Err(RouteError::for_route(
                    RouteErrorCode::DuplicateQueryKey,
                    &constructor.name,
                    format!(
                        "declared query key `{}` occurs more than once",
                        query_part.key
                    ),
                ));
            }
            let value = values.first().copied().map(ToString::to_string);
            fields.insert(
                query_part.field.clone(),
                declarations[query_part.field.as_str()]
                    .kind
                    .decode_optional(value)?,
            );
        }
        for declaration in &constructor.fields {
            if !fields.contains_key(&declaration.name) {
                return Err(RouteError::for_route(
                    RouteErrorCode::MissingField,
                    &constructor.name,
                    format!("route did not decode required field `{}`", declaration.name),
                ));
            }
        }

        Ok(RouteLocation {
            constructor: constructor.name.clone(),
            fields,
        })
    }

    fn resolved_route(
        &self,
        constructor_name: &str,
    ) -> Result<(&RouteConstructorDecl, ParsedPattern), RouteError> {
        let constructor = self
            .constructors
            .iter()
            .find(|constructor| constructor.name == constructor_name)
            .ok_or_else(|| {
                RouteError::for_route(
                    RouteErrorCode::UnknownConstructor,
                    constructor_name,
                    format!("unknown Location constructor `{constructor_name}`"),
                )
            })?;
        let pattern = self
            .patterns
            .iter()
            .find(|pattern| pattern.constructor == constructor_name)
            .expect("RouteTable construction proves one pattern per constructor");
        Ok((constructor, parse_pattern(constructor, &pattern.pattern)?))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum RouteAtom {
    Text { value: String },
    Key { type_name: TypeRef, value: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "presence", content = "value", rename_all = "kebab-case")]
pub enum RouteFieldValue {
    Required(RouteAtom),
    Optional(Option<RouteAtom>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RouteLocation {
    pub constructor: String,
    #[serde(default)]
    pub fields: BTreeMap<String, RouteFieldValue>,
}

impl RouteLocation {
    pub fn new(constructor: impl Into<String>, fields: BTreeMap<String, RouteFieldValue>) -> Self {
        Self {
            constructor: constructor.into(),
            fields,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ParsedPattern {
    path: Vec<RoutePathPart>,
    query: Vec<QueryPart>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct QueryPart {
    key: String,
    field: String,
}

fn parse_pattern(
    constructor: &RouteConstructorDecl,
    pattern: &str,
) -> Result<ParsedPattern, RouteError> {
    if !pattern.starts_with('/') || pattern.contains('#') {
        return Err(RouteError::for_route(
            RouteErrorCode::InvalidPattern,
            &constructor.name,
            "a route pattern must be an absolute path with no fragment",
        ));
    }
    let (path, raw_query) = match pattern.split_once('?') {
        Some((path, query)) if !query.is_empty() => (path, Some(query)),
        Some(_) => {
            return Err(RouteError::for_route(
                RouteErrorCode::InvalidPattern,
                &constructor.name,
                "a route pattern cannot end with an empty query",
            ));
        }
        None => (pattern, None),
    };

    let field_map: BTreeMap<&str, &RouteFieldDecl> = constructor
        .fields
        .iter()
        .map(|field| (field.name.as_str(), field))
        .collect();
    let mut used_fields = BTreeSet::new();
    let mut path_parts = Vec::new();
    for segment in split_pattern_path(path).map_err(|message| {
        RouteError::for_route(RouteErrorCode::InvalidPattern, &constructor.name, message)
    })? {
        if let Some(field) = placeholder(segment, false) {
            validate_symbol(field).map_err(|message| {
                RouteError::for_route(RouteErrorCode::InvalidPattern, &constructor.name, message)
            })?;
            let declaration = field_map.get(field).ok_or_else(|| {
                RouteError::for_route(
                    RouteErrorCode::UnknownField,
                    &constructor.name,
                    format!("path placeholder references unknown field `{field}`"),
                )
            })?;
            if declaration.kind.is_optional() {
                return Err(RouteError::for_route(
                    RouteErrorCode::IncompatibleFieldType,
                    &constructor.name,
                    format!("optional field `{field}` cannot occupy a path segment"),
                ));
            }
            if !used_fields.insert(field) {
                return Err(RouteError::for_route(
                    RouteErrorCode::DuplicateFieldUse,
                    &constructor.name,
                    format!("field `{field}` occurs more than once in its route"),
                ));
            }
            path_parts.push(RoutePathPart::Field(field.to_string()));
        } else {
            if segment.contains('{') || segment.contains('}') {
                return Err(RouteError::for_route(
                    RouteErrorCode::InvalidPattern,
                    &constructor.name,
                    format!("`{segment}` is not a complete path placeholder"),
                ));
            }
            let decoded = decode_query_value(segment).map_err(|_| {
                RouteError::for_route(
                    RouteErrorCode::InvalidPattern,
                    &constructor.name,
                    format!("literal path component `{segment}` is not canonical"),
                )
            })?;
            if decoded.is_empty() || decoded == "." || decoded == ".." || decoded.starts_with('~') {
                return Err(RouteError::for_route(
                    RouteErrorCode::InvalidPattern,
                    &constructor.name,
                    format!("literal path component `{segment}` is reserved"),
                ));
            }
            path_parts.push(RoutePathPart::Literal(segment.to_string()));
        }
    }

    let mut query_parts = Vec::new();
    let mut query_keys = BTreeSet::new();
    if let Some(raw_query) = raw_query {
        for pair in raw_query.split('&') {
            if pair.is_empty() {
                return Err(RouteError::for_route(
                    RouteErrorCode::InvalidPattern,
                    &constructor.name,
                    "a route pattern contains an empty query pair",
                ));
            }
            let (raw_key, raw_value) = pair.split_once('=').ok_or_else(|| {
                RouteError::for_route(
                    RouteErrorCode::InvalidPattern,
                    &constructor.name,
                    format!("query pattern `{pair}` needs `key={{field?}}`"),
                )
            })?;
            if raw_key.is_empty() || raw_value.contains('=') {
                return Err(RouteError::for_route(
                    RouteErrorCode::InvalidPattern,
                    &constructor.name,
                    format!("query pattern `{pair}` is malformed"),
                ));
            }
            let key = decode_query_value(raw_key).map_err(|_| {
                RouteError::for_route(
                    RouteErrorCode::InvalidPattern,
                    &constructor.name,
                    format!("query key `{raw_key}` is not canonical"),
                )
            })?;
            if key.is_empty() || !query_keys.insert(key.clone()) {
                return Err(RouteError::for_route(
                    RouteErrorCode::DuplicateQueryKey,
                    &constructor.name,
                    format!("query key `{key}` is empty or duplicated"),
                ));
            }
            let Some(field) = placeholder(raw_value, true) else {
                return Err(RouteError::for_route(
                    RouteErrorCode::InvalidPattern,
                    &constructor.name,
                    format!("query value `{raw_value}` must be `{{field?}}`"),
                ));
            };
            validate_symbol(field).map_err(|message| {
                RouteError::for_route(RouteErrorCode::InvalidPattern, &constructor.name, message)
            })?;
            let declaration = field_map.get(field).ok_or_else(|| {
                RouteError::for_route(
                    RouteErrorCode::UnknownField,
                    &constructor.name,
                    format!("query placeholder references unknown field `{field}`"),
                )
            })?;
            if !declaration.kind.is_optional() {
                return Err(RouteError::for_route(
                    RouteErrorCode::IncompatibleFieldType,
                    &constructor.name,
                    format!("query field `{field}` must have an Option text/key type"),
                ));
            }
            if !used_fields.insert(field) {
                return Err(RouteError::for_route(
                    RouteErrorCode::DuplicateFieldUse,
                    &constructor.name,
                    format!("field `{field}` occurs more than once in its route"),
                ));
            }
            query_parts.push(QueryPart {
                key,
                field: field.to_string(),
            });
        }
    }

    for field in &constructor.fields {
        if !used_fields.contains(field.name.as_str()) {
            return Err(RouteError::for_route(
                RouteErrorCode::MissingField,
                &constructor.name,
                format!("route pattern does not map field `{}`", field.name),
            ));
        }
    }
    Ok(ParsedPattern {
        path: path_parts,
        query: query_parts,
    })
}

fn placeholder(value: &str, optional: bool) -> Option<&str> {
    let inner = value.strip_prefix('{')?.strip_suffix('}')?;
    if optional {
        inner.strip_suffix('?')
    } else if inner.ends_with('?') {
        None
    } else {
        Some(inner)
    }
}

fn split_pattern_path(path: &str) -> Result<Vec<&str>, String> {
    let rest = path
        .strip_prefix('/')
        .ok_or_else(|| "route pattern must start with `/`".to_string())?;
    if rest.is_empty() {
        return Ok(Vec::new());
    }
    let segments: Vec<&str> = rest.split('/').collect();
    if segments.iter().any(|segment| segment.is_empty()) {
        return Err("route patterns cannot contain empty path segments".to_string());
    }
    Ok(segments)
}

fn path_patterns_overlap(left: &ParsedPattern, right: &ParsedPattern) -> bool {
    left.path.len() == right.path.len()
        && left
            .path
            .iter()
            .zip(right.path.iter())
            .all(|(left, right)| match (left, right) {
                (RoutePathPart::Literal(left), RoutePathPart::Literal(right)) => left == right,
                _ => true,
            })
}

fn path_matches(pattern: &[RoutePathPart], segments: &[&str]) -> bool {
    pattern.len() == segments.len()
        && pattern
            .iter()
            .zip(segments.iter())
            .all(|(part, segment)| match part {
                RoutePathPart::Literal(literal) => literal == segment,
                RoutePathPart::Field(_) => true,
            })
}

fn split_path(path: &str) -> Result<Vec<&str>, RouteError> {
    let rest = path.strip_prefix('/').ok_or_else(|| {
        RouteError::new(
            RouteErrorCode::MalformedUrl,
            "router path must begin with `/`",
        )
    })?;
    if rest.is_empty() {
        return Ok(Vec::new());
    }
    let segments: Vec<&str> = rest.split('/').collect();
    if segments.iter().any(|segment| segment.is_empty()) {
        return Err(RouteError::new(
            RouteErrorCode::MalformedUrl,
            "router path contains an empty segment",
        ));
    }
    Ok(segments)
}

fn parse_query(query: Option<&str>) -> Result<Vec<(String, String)>, RouteError> {
    let Some(query) = query else {
        return Ok(Vec::new());
    };
    if query.is_empty() {
        return Ok(Vec::new());
    }
    query
        .split('&')
        .map(|pair| {
            if pair.is_empty() {
                return Err(RouteError::new(
                    RouteErrorCode::MalformedUrl,
                    "router query contains an empty pair",
                ));
            }
            let (key, value) = pair.split_once('=').ok_or_else(|| {
                RouteError::new(
                    RouteErrorCode::MalformedUrl,
                    "router query pairs must contain `=`",
                )
            })?;
            if value.contains('=') {
                return Err(RouteError::new(
                    RouteErrorCode::NonCanonicalComponent,
                    "literal `=` in a query value must be percent encoded",
                ));
            }
            Ok((decode_query_value(key)?, decode_query_value(value)?))
        })
        .collect()
}

/// Encodes a dynamic path value with the pinned opaque component codec.
pub fn encode_opaque_path_component(value: &str) -> String {
    let encoded = encode_url_component(value);
    if encoded.is_empty() || encoded == "." || encoded == ".." || encoded.starts_with('~') {
        format!("~{}", encode_base64url(value.as_bytes()))
    } else {
        encoded
    }
}

/// Decodes and canonicality-checks one opaque dynamic path component.
pub fn decode_opaque_path_component(component: &str) -> Result<String, RouteError> {
    let decoded = if let Some(encoded) = component.strip_prefix('~') {
        let bytes = decode_base64url(encoded)?;
        String::from_utf8(bytes).map_err(|_| {
            RouteError::new(
                RouteErrorCode::MalformedText,
                "opaque path component is not valid UTF-8",
            )
        })?
    } else {
        decode_percent_bytes(component)?
    };
    if encode_opaque_path_component(&decoded) != component {
        return Err(RouteError::new(
            RouteErrorCode::NonCanonicalComponent,
            format!("`{component}` is not a canonical opaque path component"),
        ));
    }
    Ok(decoded)
}

/// Canonical URL-component encoding used by route query values.
pub fn encode_query_value(value: &str) -> String {
    encode_url_component(value)
}

/// Decodes a query component and rejects every non-canonical spelling.
pub fn decode_query_value(component: &str) -> Result<String, RouteError> {
    let decoded = decode_percent_bytes(component)?;
    if encode_query_value(&decoded) != component {
        return Err(RouteError::new(
            RouteErrorCode::NonCanonicalComponent,
            format!("`{component}` is not a canonical query component"),
        ));
    }
    Ok(decoded)
}

fn encode_url_component(value: &str) -> String {
    let mut output = String::new();
    for byte in value.as_bytes() {
        if safe_component_byte(*byte) {
            output.push(char::from(*byte));
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            output.push('%');
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    output
}

fn safe_component_byte(byte: u8) -> bool {
    matches!(
        byte,
        b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')'
    )
}

fn decode_percent_bytes(component: &str) -> Result<String, RouteError> {
    let bytes = component.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(RouteError::new(
                    RouteErrorCode::MalformedPercentEscape,
                    "truncated percent escape",
                ));
            }
            let high = uppercase_hex(bytes[index + 1]).ok_or_else(|| {
                RouteError::new(
                    RouteErrorCode::MalformedPercentEscape,
                    "percent escapes require uppercase hexadecimal",
                )
            })?;
            let low = uppercase_hex(bytes[index + 2]).ok_or_else(|| {
                RouteError::new(
                    RouteErrorCode::MalformedPercentEscape,
                    "percent escapes require uppercase hexadecimal",
                )
            })?;
            output.push((high << 4) | low);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).map_err(|_| {
        RouteError::new(
            RouteErrorCode::MalformedText,
            "URL component is not valid UTF-8",
        )
    })
}

fn validate_percent_syntax(component: &str) -> Result<(), RouteError> {
    let bytes = component.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || uppercase_hex(bytes[index + 1]).is_none()
                || uppercase_hex(bytes[index + 2]).is_none()
            {
                return Err(RouteError::new(
                    RouteErrorCode::MalformedPercentEscape,
                    "path contains a malformed or lowercase percent escape",
                ));
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    Ok(())
}

fn uppercase_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn encode_base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut chunks = bytes.chunks_exact(3);
    for chunk in &mut chunks {
        let bits = (u32::from(chunk[0]) << 16) | (u32::from(chunk[1]) << 8) | u32::from(chunk[2]);
        output.push(char::from(ALPHABET[((bits >> 18) & 63) as usize]));
        output.push(char::from(ALPHABET[((bits >> 12) & 63) as usize]));
        output.push(char::from(ALPHABET[((bits >> 6) & 63) as usize]));
        output.push(char::from(ALPHABET[(bits & 63) as usize]));
    }
    match chunks.remainder() {
        [first] => {
            let bits = u32::from(*first) << 16;
            output.push(char::from(ALPHABET[((bits >> 18) & 63) as usize]));
            output.push(char::from(ALPHABET[((bits >> 12) & 63) as usize]));
        }
        [first, second] => {
            let bits = (u32::from(*first) << 16) | (u32::from(*second) << 8);
            output.push(char::from(ALPHABET[((bits >> 18) & 63) as usize]));
            output.push(char::from(ALPHABET[((bits >> 12) & 63) as usize]));
            output.push(char::from(ALPHABET[((bits >> 6) & 63) as usize]));
        }
        [] => {}
        _ => unreachable!("chunks_exact remainder is shorter than three"),
    }
    output
}

fn decode_base64url(encoded: &str) -> Result<Vec<u8>, RouteError> {
    if encoded.len() % 4 == 1 {
        return Err(RouteError::new(
            RouteErrorCode::MalformedOpaqueEscape,
            "unpadded base64url has an impossible length",
        ));
    }
    let mut sextets = Vec::with_capacity(encoded.len());
    for byte in encoded.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => {
                return Err(RouteError::new(
                    RouteErrorCode::MalformedOpaqueEscape,
                    "opaque escape must be unpadded base64url",
                ));
            }
        };
        sextets.push(value);
    }

    let mut output = Vec::with_capacity(encoded.len() * 3 / 4);
    let mut chunks = sextets.chunks_exact(4);
    for chunk in &mut chunks {
        output.push((chunk[0] << 2) | (chunk[1] >> 4));
        output.push((chunk[1] << 4) | (chunk[2] >> 2));
        output.push((chunk[2] << 6) | chunk[3]);
    }
    match chunks.remainder() {
        [first, second] => {
            if second & 0x0f != 0 {
                return Err(RouteError::new(
                    RouteErrorCode::MalformedOpaqueEscape,
                    "opaque escape has non-zero unused bits",
                ));
            }
            output.push((first << 2) | (second >> 4));
        }
        [first, second, third] => {
            if third & 0x03 != 0 {
                return Err(RouteError::new(
                    RouteErrorCode::MalformedOpaqueEscape,
                    "opaque escape has non-zero unused bits",
                ));
            }
            output.push((first << 2) | (second >> 4));
            output.push((second << 4) | (third >> 2));
        }
        [] => {}
        _ => {
            return Err(RouteError::new(
                RouteErrorCode::MalformedOpaqueEscape,
                "opaque escape has an impossible remainder",
            ));
        }
    }
    Ok(output)
}

fn validate_symbol(value: &str) -> Result<(), String> {
    let mut characters = value.chars();
    let Some(first) = characters.next() else {
        return Err("route names and fields cannot be empty".to_string());
    };
    if first != '_' && !first.is_alphabetic() {
        return Err(format!("`{value}` is not an Uhura identifier"));
    }
    if !characters.all(|character| character == '_' || character.is_alphanumeric()) {
        return Err(format!("`{value}` is not an Uhura identifier"));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RouteErrorCode {
    EmptyLocation,
    InvalidDeclaration,
    DuplicateConstructor,
    DuplicateField,
    UnknownConstructor,
    MissingPattern,
    DuplicatePattern,
    InvalidPattern,
    UnknownField,
    MissingField,
    DuplicateFieldUse,
    IncompatibleFieldType,
    DuplicateQueryKey,
    AmbiguousPattern,
    ValueShapeMismatch,
    MalformedUrl,
    UnknownRoute,
    MalformedPercentEscape,
    MalformedOpaqueEscape,
    MalformedText,
    NonCanonicalComponent,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RouteError {
    pub code: RouteErrorCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    pub message: String,
}

impl RouteError {
    fn new(code: RouteErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            route: None,
            message: message.into(),
        }
    }

    fn for_route(
        code: RouteErrorCode,
        route: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            route: Some(route.into()),
            message: message.into(),
        }
    }
}

impl fmt::Display for RouteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(route) = &self.route {
            write!(formatter, "{:?} at `{route}`: {}", self.code, self.message)
        } else {
            write!(formatter, "{:?}: {}", self.code, self.message)
        }
    }
}

impl std::error::Error for RouteError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn ty(value: &str) -> TypeRef {
        TypeRef::new(value).unwrap()
    }

    fn return_routes() -> RouteTable {
        RouteTable::compile(
            ty("app.return_desk.machine@1::Location"),
            vec![
                RouteConstructorDecl::new(
                    "flow",
                    vec![
                        RouteFieldDecl::new(
                            "order",
                            RouteFieldKind::TextKey {
                                type_name: ty("app.return_desk.machine@1::OrderId"),
                            },
                        ),
                        RouteFieldDecl::new("step", RouteFieldKind::OptionalText),
                    ],
                ),
                RouteConstructorDecl::new(
                    "order",
                    vec![RouteFieldDecl::new(
                        "order",
                        RouteFieldKind::TextKey {
                            type_name: ty("app.return_desk.machine@1::OrderId"),
                        },
                    )],
                ),
                RouteConstructorDecl::new(
                    "receipt",
                    vec![RouteFieldDecl::new(
                        "return_id",
                        RouteFieldKind::TextKey {
                            type_name: ty("app.return_desk.machine@1::ReturnId"),
                        },
                    )],
                ),
            ],
            vec![
                RoutePatternDecl::new("flow", "/orders/{order}/return?step={step?}"),
                RoutePatternDecl::new("order", "/orders/{order}"),
                RoutePatternDecl::new("receipt", "/returns/{return_id}"),
            ],
        )
        .unwrap()
    }

    fn key(type_name: &str, value: &str) -> RouteFieldValue {
        RouteFieldValue::Required(RouteAtom::Key {
            type_name: ty(type_name),
            value: value.to_string(),
        })
    }

    #[test]
    fn frozen_opaque_path_vectors_encode_and_decode() {
        let vectors = [
            ("order-100", "order-100"),
            ("return-900", "return-900"),
            ("return/with/slash", "return%2Fwith%2Fslash"),
            ("", "~"),
            (".", "~Lg"),
            ("..", "~Li4"),
            ("~reserved-prefix", "~fnJlc2VydmVkLXByZWZpeA"),
        ];
        for (text, component) in vectors {
            assert_eq!(encode_opaque_path_component(text), component);
            assert_eq!(decode_opaque_path_component(component).unwrap(), text);
        }
    }

    #[test]
    fn opaque_decoder_rejects_noncanonical_or_malformed_spellings() {
        for invalid in [
            "",
            ".",
            "..",
            "%2f",
            "%6Frder",
            "%6F",
            "%7Ereserved-prefix",
            "~Lg==",
            "~Lh",
            "%",
            "%FF",
        ] {
            assert!(
                decode_opaque_path_component(invalid).is_err(),
                "{invalid} should be rejected"
            );
        }
    }

    #[test]
    fn path_and_query_codecs_round_trip_unicode_and_reserved_text() {
        for text in [
            "plain",
            "space value",
            "slash/percent%",
            "~",
            "한글",
            "emoji-🛰️",
            "\0",
            ".",
            "..",
            "",
        ] {
            let path = encode_opaque_path_component(text);
            assert_eq!(decode_opaque_path_component(&path).unwrap(), text);
            let query = encode_query_value(text);
            assert_eq!(decode_query_value(&query).unwrap(), text);
        }

        for noncanonical in ["raw space", "+", "%41", "%2f", "한글"] {
            assert!(decode_query_value(noncanonical).is_err(), "{noncanonical}");
        }
    }

    #[test]
    fn a0_routes_round_trip_typed_nominal_and_optional_fields() {
        let routes = return_routes();
        let order_key_type = "app.return_desk.machine@1::OrderId";
        let location = RouteLocation::new(
            "flow",
            BTreeMap::from([
                ("order".to_string(), key(order_key_type, "order-100")),
                (
                    "step".to_string(),
                    RouteFieldValue::Optional(Some(RouteAtom::Text {
                        value: "review/confirm".to_string(),
                    })),
                ),
            ]),
        );
        let encoded = routes.encode(&location).unwrap();
        assert_eq!(encoded, "/orders/order-100/return?step=review%2Fconfirm");
        assert_eq!(routes.decode(&encoded).unwrap(), location);

        let no_step = RouteLocation::new(
            "flow",
            BTreeMap::from([
                ("order".to_string(), key(order_key_type, ".")),
                ("step".to_string(), RouteFieldValue::Optional(None)),
            ]),
        );
        assert_eq!(routes.encode(&no_step).unwrap(), "/orders/~Lg/return");
        assert_eq!(routes.decode("/orders/~Lg/return").unwrap(), no_step);
    }

    #[test]
    fn query_ingress_rejects_duplicate_declared_keys_and_malformed_values() {
        let routes = return_routes();
        let duplicate = routes
            .decode("/orders/order-100/return?step=items&step=review")
            .unwrap_err();
        assert_eq!(duplicate.code, RouteErrorCode::DuplicateQueryKey);

        let malformed = routes
            .decode("/orders/order-100/return?step=%2f")
            .unwrap_err();
        assert_eq!(malformed.code, RouteErrorCode::MalformedPercentEscape);
    }

    #[test]
    fn unknown_query_keys_do_not_invent_location_fields() {
        let routes = return_routes();
        let decoded = routes
            .decode("/orders/order-100/return?utm=agent&step=items")
            .unwrap();
        assert_eq!(
            decoded.fields["step"],
            RouteFieldValue::Optional(Some(RouteAtom::Text {
                value: "items".to_string(),
            }))
        );
        assert_eq!(decoded.fields.len(), 2);
    }

    #[test]
    fn route_compilation_requires_exact_nonambiguous_constructor_coverage() {
        let constructors = vec![
            RouteConstructorDecl::new(
                "by_id",
                vec![RouteFieldDecl::new("id", RouteFieldKind::Text)],
            ),
            RouteConstructorDecl::new("new", Vec::new()),
        ];
        let missing = RouteTable::compile(
            ty("Location"),
            constructors.clone(),
            vec![RoutePatternDecl::new("by_id", "/items/{id}")],
        )
        .unwrap_err();
        assert_eq!(missing.code, RouteErrorCode::MissingPattern);

        let ambiguous = RouteTable::compile(
            ty("Location"),
            constructors,
            vec![
                RoutePatternDecl::new("by_id", "/items/{id}"),
                RoutePatternDecl::new("new", "/items/new"),
            ],
        )
        .unwrap_err();
        assert_eq!(ambiguous.code, RouteErrorCode::AmbiguousPattern);
    }

    #[test]
    fn route_compilation_rejects_illegal_field_placements() {
        let optional_in_path = RouteTable::compile(
            ty("Location"),
            vec![RouteConstructorDecl::new(
                "page",
                vec![RouteFieldDecl::new("section", RouteFieldKind::OptionalText)],
            )],
            vec![RoutePatternDecl::new("page", "/{section}")],
        )
        .unwrap_err();
        assert_eq!(optional_in_path.code, RouteErrorCode::IncompatibleFieldType);

        let required_in_query = RouteTable::compile(
            ty("Location"),
            vec![RouteConstructorDecl::new(
                "page",
                vec![RouteFieldDecl::new("section", RouteFieldKind::Text)],
            )],
            vec![RoutePatternDecl::new("page", "/page?section={section?}")],
        )
        .unwrap_err();
        assert_eq!(
            required_in_query.code,
            RouteErrorCode::IncompatibleFieldType
        );
    }

    #[test]
    fn serialized_route_tables_are_revalidated() {
        let routes = return_routes();
        let json = serde_json::to_value(&routes).unwrap();
        assert_eq!(serde_json::from_value::<RouteTable>(json).unwrap(), routes);

        let mut invalid = serde_json::to_value(routes).unwrap();
        invalid["patterns"][0]["pattern"] =
            serde_json::Value::String("/orders/{missing}".to_string());
        assert!(serde_json::from_value::<RouteTable>(invalid).is_err());
    }
}
