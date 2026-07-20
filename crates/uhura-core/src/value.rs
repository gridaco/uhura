use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use num_bigint::{BigInt, Sign};
use num_integer::Integer as _;
use num_traits::{One, Signed as _, Zero};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::codec::{frame, nat};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum IntegerKind {
    Int,
    Nat,
    PositiveInt,
}

impl IntegerKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Int => "Int",
            Self::Nat => "Nat",
            Self::PositiveInt => "PositiveInt",
        }
    }

    fn admits(self, value: &BigInt) -> bool {
        match self {
            Self::Int => true,
            Self::Nat => value >= &BigInt::zero(),
            Self::PositiveInt => value >= &BigInt::one(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Decimal {
    coefficient: BigInt,
    scale: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecimalError(pub String);

impl fmt::Display for DecimalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for DecimalError {}

impl Decimal {
    pub fn new(coefficient: BigInt, scale: u32) -> Self {
        if coefficient.is_zero() {
            return Self {
                coefficient,
                scale: 0,
            };
        }
        let ten = BigInt::from(10u8);
        let mut coefficient = coefficient;
        let mut scale = scale;
        while scale > 0 {
            let (next, remainder) = coefficient.div_rem(&ten);
            if !remainder.is_zero() {
                break;
            }
            coefficient = next;
            scale -= 1;
        }
        Self { coefficient, scale }
    }

    pub fn coefficient(&self) -> &BigInt {
        &self.coefficient
    }

    pub fn scale(&self) -> u32 {
        self.scale
    }

    pub fn is_integral(&self) -> bool {
        self.scale == 0
    }

    pub fn is_ratio(&self) -> bool {
        self >= &Self::zero() && self <= &Self::one()
    }

    pub fn zero() -> Self {
        Self::new(BigInt::zero(), 0)
    }

    pub fn one() -> Self {
        Self::new(BigInt::one(), 0)
    }

    pub fn add(&self, other: &Self) -> Self {
        let scale = self.scale.max(other.scale);
        let left = scaled_coefficient(self, scale);
        let right = scaled_coefficient(other, scale);
        Self::new(left + right, scale)
    }

    pub fn subtract(&self, other: &Self) -> Self {
        let scale = self.scale.max(other.scale);
        let left = scaled_coefficient(self, scale);
        let right = scaled_coefficient(other, scale);
        Self::new(left - right, scale)
    }

    pub fn multiply(&self, other: &Self) -> Self {
        Self::new(
            &self.coefficient * &other.coefficient,
            self.scale.saturating_add(other.scale),
        )
    }

    pub fn canonical_text(&self) -> String {
        if self.scale == 0 {
            return self.coefficient.to_string();
        }
        let negative = self.coefficient.is_negative();
        let mut digits = self.coefficient.abs().to_string();
        let scale = self.scale as usize;
        if digits.len() <= scale {
            let mut padded = String::with_capacity(scale + 1);
            padded.push_str(&"0".repeat(scale + 1 - digits.len()));
            padded.push_str(&digits);
            digits = padded;
        }
        let point = digits.len() - scale;
        digits.insert(point, '.');
        if negative {
            digits.insert(0, '-');
        }
        digits
    }
}

fn scaled_coefficient(value: &Decimal, scale: u32) -> BigInt {
    let extra = scale - value.scale;
    &value.coefficient * BigInt::from(10u8).pow(extra)
}

impl FromStr for Decimal {
    type Err = DecimalError;

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        if source.is_empty() {
            return Err(DecimalError("empty decimal".into()));
        }
        let (negative, unsigned) = match source.as_bytes()[0] {
            b'-' => (true, &source[1..]),
            b'+' => {
                return Err(DecimalError(
                    "a Uhura decimal never has an explicit plus sign".into(),
                ));
            }
            _ => (false, source),
        };
        if unsigned.is_empty() {
            return Err(DecimalError("decimal has no digits".into()));
        }
        let mut split = unsigned.split('.');
        let whole = split.next().expect("one split field");
        let fraction = split.next();
        if split.next().is_some()
            || whole.is_empty()
            || !whole.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(DecimalError(format!("invalid decimal `{source}`")));
        }
        let fraction = fraction.unwrap_or("");
        if !fraction.bytes().all(|byte| byte.is_ascii_digit())
            || (source.contains('.') && fraction.is_empty())
        {
            return Err(DecimalError(format!("invalid decimal `{source}`")));
        }
        let mut digits = String::with_capacity(whole.len() + fraction.len() + 1);
        if negative {
            digits.push('-');
        }
        digits.push_str(whole);
        digits.push_str(fraction);
        let coefficient = BigInt::from_str(&digits)
            .map_err(|_| DecimalError(format!("invalid decimal `{source}`")))?;
        Ok(Self::new(coefficient, fraction.len() as u32))
    }
}

impl Ord for Decimal {
    fn cmp(&self, other: &Self) -> Ordering {
        let scale = self.scale.max(other.scale);
        scaled_coefficient(self, scale).cmp(&scaled_coefficient(other, scale))
    }
}

impl PartialOrd for Decimal {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.canonical_text())
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BoundaryNumber {
    Finite(Decimal),
    Nan,
    PositiveInfinity,
    NegativeInfinity,
}

impl BoundaryNumber {
    pub fn ratio(&self) -> Option<Decimal> {
        match self {
            Self::Finite(value) if value.is_ratio() => Some(value.clone()),
            _ => None,
        }
    }

    pub fn integer(&self) -> Option<BigInt> {
        match self {
            Self::Finite(value) if value.is_integral() => Some(value.coefficient.clone()),
            _ => None,
        }
    }
}

/// The complete immutable Uhura value model used at every semantic boundary.
///
/// Maps and sets are kept in canonical byte order by their constructors.
/// `Variant` and `Key` retain nominal type identity so structurally equal
/// payloads from different declarations never compare equal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Value {
    Unit,
    Bool(bool),
    Integer {
        kind: IntegerKind,
        value: BigInt,
    },
    Decimal(Decimal),
    Ratio(Decimal),
    Boundary(BoundaryNumber),
    Text(String),
    Key {
        type_id: String,
        value: Box<Value>,
    },
    Tuple(Vec<Value>),
    /// Record fields in declaration order.
    ///
    /// Uhura's canonical bytes and receipts make declaration order observable,
    /// so a lexical map is not an honest representation here.
    Record(Vec<(String, Value)>),
    Variant {
        type_id: String,
        constructor: String,
        fields: Vec<(Option<String>, Value)>,
    },
    Seq(Vec<Value>),
    NonEmpty(Vec<Value>),
    Set(Vec<Value>),
    Map(Vec<(Value, Value)>),
    Table {
        key_type: String,
        entries: Vec<(String, Value)>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValueError(pub String);

impl fmt::Display for ValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ValueError {}

impl Value {
    pub fn int(value: impl Into<BigInt>) -> Self {
        Self::Integer {
            kind: IntegerKind::Int,
            value: value.into(),
        }
    }

    pub fn nat(value: impl Into<BigInt>) -> Result<Self, ValueError> {
        Self::integer(IntegerKind::Nat, value)
    }

    pub fn positive(value: impl Into<BigInt>) -> Result<Self, ValueError> {
        Self::integer(IntegerKind::PositiveInt, value)
    }

    pub fn integer(kind: IntegerKind, value: impl Into<BigInt>) -> Result<Self, ValueError> {
        let value = value.into();
        if !kind.admits(&value) {
            return Err(ValueError(format!(
                "{} does not admit {value}",
                kind.name()
            )));
        }
        Ok(Self::Integer { kind, value })
    }

    pub fn ratio(value: Decimal) -> Result<Self, ValueError> {
        if !value.is_ratio() {
            return Err(ValueError(format!(
                "Ratio requires a normalized 0..1 value, got {value}"
            )));
        }
        Ok(Self::Ratio(value))
    }

    pub fn set(values: impl IntoIterator<Item = Value>) -> Self {
        let mut values = values.into_iter().collect::<Vec<_>>();
        values.sort_by_key(Value::canonical_bytes);
        values.dedup();
        Self::Set(values)
    }

    pub fn map(entries: impl IntoIterator<Item = (Value, Value)>) -> Result<Self, ValueError> {
        let mut entries = entries.into_iter().collect::<Vec<_>>();
        entries.sort_by(|left, right| left.0.canonical_bytes().cmp(&right.0.canonical_bytes()));
        if entries.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err(ValueError("Map contains an equal duplicate key".into()));
        }
        Ok(Self::Map(entries))
    }

    pub fn variant(
        type_id: impl Into<String>,
        constructor: impl Into<String>,
        fields: Vec<(Option<String>, Value)>,
    ) -> Self {
        Self::Variant {
            type_id: type_id.into(),
            constructor: constructor.into(),
            fields,
        }
    }

    pub fn record(fields: impl IntoIterator<Item = (String, Value)>) -> Result<Self, ValueError> {
        let fields = fields.into_iter().collect::<Vec<_>>();
        if fields
            .iter()
            .enumerate()
            .any(|(index, (name, _))| fields[index + 1..].iter().any(|(next, _)| next == name))
        {
            return Err(ValueError("Record contains a duplicate field name".into()));
        }
        Ok(Self::Record(fields))
    }

    pub fn canonical_bytes(&self) -> Vec<u8> {
        frame(
            "value",
            &[self.type_identity().into_bytes(), self.value_body()],
        )
    }

    pub fn type_identity(&self) -> String {
        match self {
            Self::Unit => "Unit".into(),
            Self::Bool(_) => "Bool".into(),
            Self::Integer { kind, .. } => kind.name().into(),
            Self::Decimal(_) => "Decimal".into(),
            Self::Ratio(_) => "Ratio".into(),
            Self::Boundary(_) => "BoundaryNumber".into(),
            Self::Text(_) => "Text".into(),
            Self::Key { type_id, .. } | Self::Variant { type_id, .. } => type_id.clone(),
            Self::Tuple(values) => format!(
                "({})",
                values
                    .iter()
                    .map(Self::type_identity)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Self::Record(fields) => format!(
                "{{{}}}",
                fields
                    .iter()
                    .map(|(name, value)| format!("{name}:{}", value.type_identity()))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Self::Seq(values) => format!(
                "Seq<{}>",
                values.first().map_or("_".into(), Self::type_identity)
            ),
            Self::NonEmpty(values) => format!(
                "NonEmpty<{}>",
                values.first().map_or("_".into(), Self::type_identity)
            ),
            Self::Set(values) => format!(
                "Set<{}>",
                values.first().map_or("_".into(), Self::type_identity)
            ),
            Self::Map(entries) => format!(
                "Map<{},{}>",
                entries
                    .first()
                    .map_or("_".into(), |entry| entry.0.type_identity()),
                entries
                    .first()
                    .map_or("_".into(), |entry| entry.1.type_identity())
            ),
            Self::Table { key_type, entries } => format!(
                "Table<{key_type},{}>",
                entries
                    .first()
                    .map_or("_".into(), |entry| entry.1.type_identity())
            ),
        }
    }

    fn value_body(&self) -> Vec<u8> {
        match self {
            Self::Unit => Vec::new(),
            Self::Bool(value) => vec![u8::from(*value)],
            Self::Integer { value, .. } => integer_body(value),
            Self::Decimal(value) | Self::Ratio(value) => decimal_body(value),
            Self::Boundary(value) => match value {
                BoundaryNumber::Finite(decimal) => frame("finite", &[decimal_body(decimal)]),
                BoundaryNumber::Nan => frame("nan", &[]),
                BoundaryNumber::PositiveInfinity => frame("positive-infinity", &[]),
                BoundaryNumber::NegativeInfinity => frame("negative-infinity", &[]),
            },
            Self::Text(value) => value.as_bytes().to_vec(),
            Self::Key { value, .. } => value.value_body(),
            Self::Tuple(values)
            | Self::Seq(values)
            | Self::NonEmpty(values)
            | Self::Set(values) => {
                let mut parts = Vec::with_capacity(values.len() + 1);
                parts.push(nat(values.len()));
                parts.extend(values.iter().map(Self::canonical_bytes));
                frame("items", &parts)
            }
            Self::Record(fields) => frame(
                "record",
                &fields
                    .iter()
                    .map(|(name, value)| {
                        frame(
                            "field",
                            &[name.as_bytes().to_vec(), value.canonical_bytes()],
                        )
                    })
                    .collect::<Vec<_>>(),
            ),
            Self::Variant {
                constructor,
                fields,
                ..
            } => {
                let mut parts = vec![constructor.as_bytes().to_vec()];
                parts.extend(fields.iter().map(|(name, value)| {
                    frame(
                        "variant-field",
                        &[
                            name.as_deref().unwrap_or("").as_bytes().to_vec(),
                            value.canonical_bytes(),
                        ],
                    )
                }));
                frame("variant", &parts)
            }
            Self::Map(entries) => frame(
                "map",
                &entries
                    .iter()
                    .map(|(key, value)| {
                        frame("entry", &[key.canonical_bytes(), value.canonical_bytes()])
                    })
                    .collect::<Vec<_>>(),
            ),
            Self::Table { key_type, entries } => {
                let mut parts = vec![key_type.as_bytes().to_vec()];
                parts.extend(entries.iter().map(|(key, value)| {
                    frame("slot", &[key.as_bytes().to_vec(), value.canonical_bytes()])
                }));
                frame("table", &parts)
            }
        }
    }

    pub fn to_wire_json(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            Self::Unit => json!({"$": "unit"}),
            Self::Bool(value) => json!({"$": "bool", "value": value}),
            Self::Integer { kind, value } => {
                json!({"$": kind.name(), "value": value.to_string()})
            }
            Self::Decimal(value) => json!({"$": "Decimal", "value": value.canonical_text()}),
            Self::Ratio(value) => json!({"$": "Ratio", "value": value.canonical_text()}),
            Self::Boundary(value) => match value {
                BoundaryNumber::Finite(value) => {
                    json!({"$": "BoundaryNumber", "case": "finite", "value": value.canonical_text()})
                }
                BoundaryNumber::Nan => json!({"$": "BoundaryNumber", "case": "nan"}),
                BoundaryNumber::PositiveInfinity => {
                    json!({"$": "BoundaryNumber", "case": "positive_infinity"})
                }
                BoundaryNumber::NegativeInfinity => {
                    json!({"$": "BoundaryNumber", "case": "negative_infinity"})
                }
            },
            Self::Text(value) => json!({"$": "Text", "value": value}),
            Self::Key { type_id, value } => {
                json!({"$": "key", "type": type_id, "value": value.to_wire_json()})
            }
            Self::Tuple(values) => {
                json!({"$": "tuple", "items": values.iter().map(Self::to_wire_json).collect::<Vec<_>>()})
            }
            Self::Record(fields) => json!({
                "$": "record",
                "fields": fields.iter().map(|(name, value)| {
                    json!({"name": name, "value": value.to_wire_json()})
                }).collect::<Vec<_>>(),
            }),
            Self::Variant {
                type_id,
                constructor,
                fields,
            } => json!({
                "$": "variant",
                "type": type_id,
                "case": constructor,
                "fields": fields.iter().map(|(name, value)| json!({"name": name, "value": value.to_wire_json()})).collect::<Vec<_>>(),
            }),
            Self::Seq(values) => json!({
                "$": "seq",
                "items": values.iter().map(Self::to_wire_json).collect::<Vec<_>>(),
            }),
            Self::NonEmpty(values) => json!({
                "$": "nonempty",
                "items": values.iter().map(Self::to_wire_json).collect::<Vec<_>>(),
            }),
            Self::Set(values) => json!({
                "$": "set",
                "items": values.iter().map(Self::to_wire_json).collect::<Vec<_>>(),
            }),
            Self::Map(entries) => json!({
                "$": "map",
                "entries": entries.iter().map(|(key, value)| json!([key.to_wire_json(), value.to_wire_json()])).collect::<Vec<_>>(),
            }),
            Self::Table { key_type, entries } => json!({
                "$": "table",
                "keyType": key_type,
                "entries": entries.iter().map(|(key, value)| json!([key, value.to_wire_json()])).collect::<Vec<_>>(),
            }),
        }
    }

    pub fn from_wire_json(json: &serde_json::Value) -> Result<Self, ValueError> {
        let object = json
            .as_object()
            .ok_or_else(|| ValueError("Uhura value must be a tagged object".into()))?;
        let tag = object
            .get("$")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| ValueError("Uhura value is missing `$`".into()))?;
        let string = |field: &str| {
            object
                .get(field)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| ValueError(format!("Uhura value needs `{field}` text")))
        };
        let items = |field: &str| {
            object
                .get(field)
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| ValueError(format!("Uhura value needs `{field}` list")))
        };
        match tag {
            "unit" => {
                exact_fields(object, &["$"], "unit")?;
                Ok(Self::Unit)
            }
            "bool" => {
                exact_fields(object, &["$", "value"], "Bool")?;
                Ok(Self::Bool(
                    object
                        .get("value")
                        .and_then(serde_json::Value::as_bool)
                        .ok_or_else(|| ValueError("Bool needs `value`".into()))?,
                ))
            }
            "Int" | "Nat" | "PositiveInt" => {
                exact_fields(object, &["$", "value"], tag)?;
                let kind = match tag {
                    "Int" => IntegerKind::Int,
                    "Nat" => IntegerKind::Nat,
                    _ => IntegerKind::PositiveInt,
                };
                let value = BigInt::from_str(string("value")?)
                    .map_err(|_| ValueError(format!("invalid {tag}")))?;
                Self::integer(kind, value)
            }
            "Decimal" => {
                exact_fields(object, &["$", "value"], "Decimal")?;
                Ok(Self::Decimal(
                    Decimal::from_str(string("value")?).map_err(|error| ValueError(error.0))?,
                ))
            }
            "Ratio" => {
                exact_fields(object, &["$", "value"], "Ratio")?;
                Self::ratio(
                    Decimal::from_str(string("value")?).map_err(|error| ValueError(error.0))?,
                )
            }
            "BoundaryNumber" => {
                let case = string("case")?;
                exact_fields(
                    object,
                    if case == "finite" {
                        &["$", "case", "value"]
                    } else {
                        &["$", "case"]
                    },
                    "BoundaryNumber",
                )?;
                match case {
                    "finite" => Ok(Self::Boundary(BoundaryNumber::Finite(
                        Decimal::from_str(string("value")?).map_err(|error| ValueError(error.0))?,
                    ))),
                    "nan" => Ok(Self::Boundary(BoundaryNumber::Nan)),
                    "positive_infinity" => Ok(Self::Boundary(BoundaryNumber::PositiveInfinity)),
                    "negative_infinity" => Ok(Self::Boundary(BoundaryNumber::NegativeInfinity)),
                    other => Err(ValueError(format!("unknown BoundaryNumber case `{other}`"))),
                }
            }
            "Text" => {
                exact_fields(object, &["$", "value"], "Text")?;
                Ok(Self::Text(string("value")?.into()))
            }
            "key" => {
                exact_fields(object, &["$", "type", "value"], "key")?;
                Ok(Self::Key {
                    type_id: string("type")?.into(),
                    value: Box::new(Self::from_wire_json(
                        object
                            .get("value")
                            .ok_or_else(|| ValueError("key needs value".into()))?,
                    )?),
                })
            }
            "tuple" | "seq" | "nonempty" | "set" => {
                exact_fields(object, &["$", "items"], tag)?;
                let values = items("items")?
                    .iter()
                    .map(Self::from_wire_json)
                    .collect::<Result<Vec<_>, _>>()?;
                match tag {
                    "tuple" => Ok(Self::Tuple(values)),
                    "seq" => Ok(Self::Seq(values)),
                    "nonempty" if !values.is_empty() => Ok(Self::NonEmpty(values)),
                    "nonempty" => Err(ValueError("NonEmpty cannot be empty".into())),
                    _ if values.iter().enumerate().any(|(index, value)| {
                        values[index + 1..].iter().any(|next| next == value)
                    }) =>
                    {
                        Err(ValueError(
                            "canonical Set representation contains a duplicate value".into(),
                        ))
                    }
                    _ => Ok(Self::Set(values)),
                }
            }
            "record" => {
                exact_fields(object, &["$", "fields"], "record")?;
                let fields = items("fields")?
                    .iter()
                    .map(|field| {
                        let field = field
                            .as_object()
                            .ok_or_else(|| ValueError("record field must be an object".into()))?;
                        exact_fields(field, &["name", "value"], "record field")?;
                        let name = field
                            .get("name")
                            .and_then(serde_json::Value::as_str)
                            .ok_or_else(|| ValueError("record field needs a text name".into()))?;
                        let value = field
                            .get("value")
                            .ok_or_else(|| ValueError("record field needs a value".into()))?;
                        Self::from_wire_json(value).map(|value| (name.into(), value))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Self::record(fields)
            }
            "variant" => {
                exact_fields(object, &["$", "type", "case", "fields"], "variant")?;
                let fields = items("fields")?
                    .iter()
                    .map(|field| {
                        let field = field
                            .as_object()
                            .ok_or_else(|| ValueError("variant field must be an object".into()))?;
                        exact_fields(field, &["name", "value"], "variant field")?;
                        let name = match field.get("name") {
                            Some(serde_json::Value::String(name)) => Some(name.clone()),
                            Some(serde_json::Value::Null) | None => None,
                            _ => {
                                return Err(ValueError(
                                    "variant field name must be text or null".into(),
                                ));
                            }
                        };
                        let value = Self::from_wire_json(
                            field
                                .get("value")
                                .ok_or_else(|| ValueError("variant field needs value".into()))?,
                        )?;
                        Ok((name, value))
                    })
                    .collect::<Result<Vec<_>, ValueError>>()?;
                Ok(Self::variant(string("type")?, string("case")?, fields))
            }
            "map" => {
                exact_fields(object, &["$", "entries"], "map")?;
                let entries = items("entries")?
                    .iter()
                    .map(|entry| {
                        let pair = entry
                            .as_array()
                            .ok_or_else(|| ValueError("map entry must be a pair".into()))?;
                        if pair.len() != 2 {
                            return Err(ValueError("map entry must have two values".into()));
                        }
                        Ok((
                            Self::from_wire_json(&pair[0])?,
                            Self::from_wire_json(&pair[1])?,
                        ))
                    })
                    .collect::<Result<Vec<_>, ValueError>>()?;
                if entries.iter().enumerate().any(|(index, (key, _))| {
                    entries[index + 1..].iter().any(|(next, _)| next == key)
                }) {
                    return Err(ValueError(
                        "canonical Map representation contains a duplicate key".into(),
                    ));
                }
                Ok(Self::Map(entries))
            }
            "table" => {
                exact_fields(object, &["$", "keyType", "entries"], "table")?;
                let entries = items("entries")?
                    .iter()
                    .map(|entry| {
                        let pair = entry
                            .as_array()
                            .ok_or_else(|| ValueError("table entry must be a pair".into()))?;
                        if pair.len() != 2 {
                            return Err(ValueError("table entry must have two values".into()));
                        }
                        let key = pair[0]
                            .as_str()
                            .ok_or_else(|| ValueError("table key must be text".into()))?;
                        Ok((key.into(), Self::from_wire_json(&pair[1])?))
                    })
                    .collect::<Result<Vec<_>, ValueError>>()?;
                Ok(Self::Table {
                    key_type: string("keyType")?.into(),
                    entries,
                })
            }
            other => Err(ValueError(format!("unknown Uhura value tag `{other}`"))),
        }
    }
}

fn exact_fields(
    object: &serde_json::Map<String, serde_json::Value>,
    expected: &[&str],
    context: &str,
) -> Result<(), ValueError> {
    if object.len() != expected.len()
        || object
            .keys()
            .any(|field| !expected.iter().any(|expected| field == expected))
    {
        return Err(ValueError(format!(
            "{context} must contain exactly fields {}",
            expected
                .iter()
                .map(|field| format!("`{field}`"))
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    Ok(())
}

fn integer_body(value: &BigInt) -> Vec<u8> {
    let (sign, magnitude) = value.to_bytes_be();
    let sign = match sign {
        Sign::Minus => 1,
        Sign::NoSign | Sign::Plus => 0,
    };
    frame("integer", &[vec![sign], magnitude])
}

fn decimal_body(value: &Decimal) -> Vec<u8> {
    frame(
        "decimal",
        &[integer_body(&value.coefficient), nat(value.scale as usize)],
    )
}

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.to_wire_json().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let json = serde_json::Value::deserialize(deserializer)?;
        Self::from_wire_json(&json).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_is_exact_and_canonical() {
        assert_eq!(
            Decimal::from_str("-001.2300").unwrap().canonical_text(),
            "-1.23"
        );
        assert_eq!(Decimal::from_str("0.000").unwrap().canonical_text(), "0");
        assert_eq!(
            Decimal::from_str("999999999999999999999999.1")
                .unwrap()
                .add(&Decimal::from_str("0.9").unwrap())
                .canonical_text(),
            "1000000000000000000000000"
        );
    }

    #[test]
    fn refinements_never_clamp() {
        assert!(Value::nat(-1).is_err());
        assert!(Value::positive(0).is_err());
        assert!(Value::ratio(Decimal::from_str("1.000").unwrap()).is_ok());
        assert!(Value::ratio(Decimal::from_str("1.01").unwrap()).is_err());
    }

    #[test]
    fn map_and_set_order_is_canonical() {
        let a = Value::Text("a".into());
        let b = Value::Text("b".into());
        assert_eq!(
            Value::set([b.clone(), a.clone(), b.clone()]),
            Value::set([a.clone(), b.clone()])
        );
        assert_eq!(
            Value::map([(b.clone(), Value::int(2)), (a.clone(), Value::int(1))]).unwrap(),
            Value::map([(a, Value::int(1)), (b, Value::int(2))]).unwrap(),
        );
    }

    #[test]
    fn wire_round_trip_preserves_large_values_and_nominality() {
        let value = Value::variant(
            "example@1::Outcome",
            "accepted",
            vec![(
                Some("count".into()),
                Value::positive(BigInt::from_str("999999999999999999999999").unwrap()).unwrap(),
            )],
        );
        assert_eq!(Value::from_wire_json(&value.to_wire_json()).unwrap(), value);
    }

    #[test]
    fn wire_decode_rejects_duplicates_and_unknown_fields() {
        let duplicate_set = serde_json::json!({
            "$": "set",
            "items": [
                {"$": "Text", "value": "same"},
                {"$": "Text", "value": "same"},
            ],
        });
        assert!(Value::from_wire_json(&duplicate_set).is_err());

        let duplicate_map = serde_json::json!({
            "$": "map",
            "entries": [
                [{"$": "Text", "value": "same"}, {"$": "Int", "value": "1"}],
                [{"$": "Text", "value": "same"}, {"$": "Int", "value": "2"}],
            ],
        });
        assert!(Value::from_wire_json(&duplicate_map).is_err());

        let extra_field = serde_json::json!({
            "$": "Int",
            "value": "1",
            "coerce": true,
        });
        assert!(Value::from_wire_json(&extra_field).is_err());
    }
}
