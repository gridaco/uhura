use std::collections::{BTreeMap, BTreeSet};

use uhura_core::{ConstructorDef, TypeRef};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Ty {
    Value(TypeRef),
    Function(Vec<Ty>, Box<Ty>),
    Unknown,
    Never,
}

impl Ty {
    pub fn value(value: TypeRef) -> Self {
        Self::Value(value)
    }

    pub fn as_value(&self) -> Option<&TypeRef> {
        match self {
            Self::Value(value) => Some(value),
            _ => None,
        }
    }

    pub fn into_value(self) -> Option<TypeRef> {
        match self {
            Self::Value(value) => Some(value),
            _ => None,
        }
    }

    pub fn display(&self) -> String {
        match self {
            Self::Value(value) => display_type_ref(value),
            Self::Function(params, result) => format!(
                "({})->{}",
                params
                    .iter()
                    .map(Self::display)
                    .collect::<Vec<_>>()
                    .join(","),
                result.display()
            ),
            Self::Unknown => "_".into(),
            Self::Never => "Never".into(),
        }
    }
}

fn display_type_ref(value: &TypeRef) -> String {
    match value {
        TypeRef::Named { id } => authored_named_type(id),
        TypeRef::Option { value } => format!("Option<{}>", display_type_ref(value)),
        TypeRef::Seq { value } => format!("Seq<{}>", display_type_ref(value)),
        TypeRef::NonEmpty { value } => format!("NonEmpty<{}>", display_type_ref(value)),
        TypeRef::Set { value } => format!("Set<{}>", display_type_ref(value)),
        TypeRef::Map { key, value } => {
            format!("Map<{},{}>", display_type_ref(key), display_type_ref(value))
        }
        TypeRef::Table { key, value } => {
            format!(
                "Table<{},{}>",
                display_type_ref(key),
                display_type_ref(value)
            )
        }
        TypeRef::FiniteView { value } => format!("FiniteView<{}>", display_type_ref(value)),
        TypeRef::Tuple { values } => format!(
            "({})",
            values
                .iter()
                .map(display_type_ref)
                .collect::<Vec<_>>()
                .join(",")
        ),
        TypeRef::Record { fields } => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(name, ty)| format!("{name}:{}", display_type_ref(ty)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => value.canonical_name(),
    }
}

fn authored_named_type(id: &str) -> String {
    let segment = id.rsplit_once("::").map_or(id, |(_, segment)| segment);
    for prefix in [
        "__uhura_private_structural_",
        "__uhura_private_",
        "__uhura_part_private_",
        "__uhura_external_",
    ] {
        if let Some(encoded) = segment.strip_prefix(prefix)
            && let Some((fingerprint, authored)) = encoded.split_once('_')
            && fingerprint.len() == 24
            && fingerprint.bytes().all(|value| value.is_ascii_hexdigit())
            && !authored.is_empty()
        {
            return authored.to_string();
        }
    }
    id.to_string()
}

#[derive(Clone, Debug)]
pub(crate) enum TypeShape {
    Alias(TypeRef),
    Key(TypeRef),
    Record(Vec<(String, TypeRef)>),
    Sum(Vec<ConstructorDef>),
}

#[derive(Clone, Debug)]
pub(crate) struct TypeInfo {
    pub id: String,
    pub shape: TypeShape,
}

#[derive(Clone, Debug)]
pub(crate) struct ConstructorInfo {
    pub type_id: String,
    pub name: String,
    pub fields: Vec<(Option<String>, TypeRef)>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TypeRegistry {
    pub types: BTreeMap<String, TypeInfo>,
    pub constructors: BTreeMap<String, Vec<ConstructorInfo>>,
}

impl TypeRegistry {
    pub fn insert(&mut self, info: TypeInfo) {
        if let TypeShape::Sum(constructors) = &info.shape {
            for constructor in constructors {
                self.constructors
                    .entry(constructor.name.clone())
                    .or_default()
                    .push(ConstructorInfo {
                        type_id: info.id.clone(),
                        name: constructor.name.clone(),
                        fields: constructor.fields.clone(),
                    });
            }
        }
        self.types.insert(info.id.clone(), info);
    }

    pub fn shape(&self, ty: &TypeRef) -> Option<&TypeShape> {
        match ty {
            TypeRef::Named { id } => self.types.get(id).map(|info| &info.shape),
            _ => None,
        }
    }

    pub fn fields(&self, ty: &TypeRef) -> Option<Vec<(String, TypeRef)>> {
        match ty {
            TypeRef::Record { fields } => Some(fields.clone()),
            TypeRef::Named { id } => match &self.types.get(id)?.shape {
                TypeShape::Record(fields) => Some(fields.clone()),
                TypeShape::Alias(alias) => self.fields(alias),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn constructors_for(&self, ty: &TypeRef) -> Vec<ConstructorInfo> {
        match ty {
            TypeRef::Option { value } => vec![
                ConstructorInfo {
                    type_id: ty.canonical_name(),
                    name: "none".into(),
                    fields: Vec::new(),
                },
                ConstructorInfo {
                    type_id: ty.canonical_name(),
                    name: "some".into(),
                    fields: vec![(Some("value".into()), value.as_ref().clone())],
                },
            ],
            TypeRef::Named { id } if id.starts_with("Token<") => {
                let inner = id
                    .strip_prefix("Token<")
                    .and_then(|value| value.strip_suffix('>'))
                    .map(|id| TypeRef::Named { id: id.into() })
                    .unwrap_or(TypeRef::Text);
                vec![
                    ConstructorInfo {
                        type_id: id.clone(),
                        name: "known".into(),
                        fields: vec![(Some("value".into()), inner)],
                    },
                    ConstructorInfo {
                        type_id: id.clone(),
                        name: "unknown".into(),
                        fields: vec![(Some("value".into()), TypeRef::Text)],
                    },
                ]
            }
            TypeRef::Named { id } => match self.types.get(id).map(|info| &info.shape) {
                Some(TypeShape::Sum(constructors)) => constructors
                    .iter()
                    .map(|constructor| ConstructorInfo {
                        type_id: id.clone(),
                        name: constructor.name.clone(),
                        fields: constructor.fields.clone(),
                    })
                    .collect(),
                Some(TypeShape::Alias(alias)) => self.constructors_for(alias),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    pub fn constructor(
        &self,
        name: &str,
        expected: Option<&TypeRef>,
    ) -> Result<ConstructorInfo, Vec<ConstructorInfo>> {
        let candidates = if let Some(expected) = expected {
            self.constructors_for(expected)
                .into_iter()
                .filter(|constructor| constructor.name == name)
                .collect::<Vec<_>>()
        } else {
            self.constructors.get(name).cloned().unwrap_or_default()
        };
        if candidates.len() == 1 {
            Ok(candidates[0].clone())
        } else {
            Err(candidates)
        }
    }

    pub fn finite_constructors(&self, ty: &TypeRef) -> Option<BTreeSet<String>> {
        let values = self.constructors_for(ty);
        (!values.is_empty()).then(|| values.into_iter().map(|item| item.name).collect())
    }
}

pub(crate) fn compatible(actual: &Ty, expected: &Ty) -> bool {
    match (actual, expected) {
        (Ty::Unknown, _) | (_, Ty::Unknown) | (Ty::Never, _) => true,
        (Ty::Value(actual), Ty::Value(expected)) => value_compatible(actual, expected),
        (Ty::Function(ap, ar), Ty::Function(ep, er)) => {
            ap.len() == ep.len()
                && ap
                    .iter()
                    .zip(ep)
                    .all(|(actual, expected)| compatible(actual, expected))
                && compatible(ar, er)
        }
        _ => actual == expected,
    }
}

pub(crate) fn value_compatible(actual: &TypeRef, expected: &TypeRef) -> bool {
    if actual == expected {
        return true;
    }
    if matches!(actual, TypeRef::Named { id } if id == &expected.canonical_name())
        || matches!(expected, TypeRef::Named { id } if id == &actual.canonical_name())
    {
        return true;
    }
    matches!(
        (actual, expected),
        (TypeRef::PositiveInt, TypeRef::Nat | TypeRef::Int)
            | (TypeRef::Nat, TypeRef::Int)
            | (TypeRef::Int, TypeRef::Nat | TypeRef::PositiveInt)
            | (TypeRef::Nat, TypeRef::PositiveInt)
    )
}

pub(crate) fn join(left: &Ty, right: &Ty) -> Ty {
    if compatible(left, right) {
        if matches!(left, Ty::Never | Ty::Unknown) {
            right.clone()
        } else {
            left.clone()
        }
    } else {
        Ty::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_type_display_hides_compiler_private_identifiers_recursively() {
        let value = Ty::value(TypeRef::Option {
            value: Box::new(TypeRef::Named {
                id: "example@1::__uhura_private_0123456789abcdef01234567_User".into(),
            }),
        });
        assert_eq!(value.display(), "Option<User>");

        let part = Ty::value(TypeRef::Named {
            id: "example@1::__uhura_part_private_abcdef0123456789abcdef01_Notice".into(),
        });
        assert_eq!(part.display(), "Notice");
    }

    #[test]
    fn diagnostic_type_display_preserves_non_generated_semantic_names() {
        let value = Ty::value(TypeRef::Named {
            id: "example@1::PublicType".into(),
        });
        assert_eq!(value.display(), "example@1::PublicType");
    }
}
