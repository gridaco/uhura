//! The checker's structural type model (design §4.3) and its conversion
//! from port contracts.
//!
//! Cross-port compatibility is **structural** (micro-decision): a record/
//! union/enum type is its canonical shape, so `comments`' `image-ref`
//! equals `feed`'s. Identity types stay nominal: the builtin `id` is one
//! type; a declared `kind = "id"`/`"opaque"` type is `(port, name)`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use uhura_base::Ident;
use uhura_port::{PortContract, TypeDecl, TypeExpr};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ty {
    Bool,
    Int,
    Text,
    /// The builtin `id`.
    Id,
    /// Core-minted command tag.
    Tag,
    /// An asset reference.
    Asset,
    /// A closed token set (catalog enums, port enums) — structural by
    /// value set.
    Enum(BTreeSet<Ident>),
    /// A declared nominal identity/cursor type.
    Nominal {
        port: Ident,
        name: Ident,
    },
    Record(BTreeMap<Ident, Ty>),
    Union(BTreeMap<Ident, BTreeMap<Ident, Ty>>),
    List(Box<Ty>),
    /// `map[K]V`, K ∈ {id, tag} (§4.3).
    Map(MapKey, Box<Ty>),
    Option(Box<Ty>),
    /// The type of a bare `none` literal — compatible with any option.
    NoneLit,
    /// Poison: an error was already reported; suppress cascades.
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapKey {
    Id,
    Tag,
}

impl Ty {
    pub fn is_error(&self) -> bool {
        matches!(self, Ty::Error)
    }

    /// Human name for diagnostics.
    pub fn describe(&self) -> String {
        format!("{self}")
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Bool => write!(f, "bool"),
            Ty::Int => write!(f, "int"),
            Ty::Text => write!(f, "text"),
            Ty::Id => write!(f, "id"),
            Ty::Tag => write!(f, "tag"),
            Ty::Asset => write!(f, "asset"),
            Ty::Enum(values) => {
                let names: Vec<&str> = values.iter().map(Ident::as_str).collect();
                write!(f, "one of {}", names.join(" | "))
            }
            Ty::Nominal { name, .. } => write!(f, "{name}"),
            Ty::Record(fields) => {
                let names: Vec<&str> = fields.keys().map(Ident::as_str).collect();
                write!(f, "{{ {} }}", names.join(", "))
            }
            Ty::Union(variants) => {
                let names: Vec<&str> = variants.keys().map(Ident::as_str).collect();
                write!(f, "union {}", names.join(" | "))
            }
            Ty::List(t) => write!(f, "list[{t}]"),
            Ty::Map(k, v) => {
                let k = match k {
                    MapKey::Id => "id",
                    MapKey::Tag => "tag",
                };
                write!(f, "map[{k}]{v}")
            }
            Ty::Option(t) => write!(f, "{t}?"),
            Ty::NoneLit => write!(f, "none"),
            Ty::Error => write!(f, "<error>"),
        }
    }
}

/// `expected` accepts `actual`: structural equality plus the option rules
/// (`T` flows into `T?`; a bare `none` flows into any option) and error
/// poisoning.
pub fn compatible(expected: &Ty, actual: &Ty) -> bool {
    if expected.is_error() || actual.is_error() {
        return true;
    }
    if expected == actual {
        return true;
    }
    match (expected, actual) {
        (Ty::Option(_), Ty::NoneLit) => true,
        (Ty::Option(inner), _) => compatible(inner, actual),
        _ => false,
    }
}

/// Both directions — for `==`/`!=` operands.
pub fn comparable(a: &Ty, b: &Ty) -> bool {
    compatible(a, b) || compatible(b, a)
}

/// The structural expansions of one port's declared types (contracts are
/// acyclic — validated — so expansion terminates).
pub struct PortTypes {
    pub port: Ident,
    expanded: BTreeMap<Ident, Ty>,
}

impl PortTypes {
    pub fn build(contract: &PortContract) -> PortTypes {
        let mut this = PortTypes {
            port: contract.name.clone(),
            expanded: BTreeMap::new(),
        };
        for name in contract.types.keys() {
            let ty = this.expand_named(contract, name);
            this.expanded.insert(name.clone(), ty);
        }
        this
    }

    /// The structural type of a declared name (`None` if undeclared).
    pub fn named(&self, name: &Ident) -> Option<&Ty> {
        self.expanded.get(name)
    }

    /// Converts a contract type expression to a structural type.
    pub fn from_expr(&self, contract: &PortContract, expr: &TypeExpr) -> Ty {
        match expr {
            TypeExpr::Bool => Ty::Bool,
            TypeExpr::Int => Ty::Int,
            TypeExpr::Text => Ty::Text,
            TypeExpr::Id => Ty::Id,
            TypeExpr::Asset => Ty::Asset,
            TypeExpr::Option(t) => Ty::Option(Box::new(self.from_expr(contract, t))),
            TypeExpr::List(t) => Ty::List(Box::new(self.from_expr(contract, t))),
            TypeExpr::Named(name) => match self.expanded.get(name) {
                Some(ty) => ty.clone(),
                None => self.expand_named(contract, name),
            },
        }
    }

    fn expand_named(&self, contract: &PortContract, name: &Ident) -> Ty {
        if let Some(ty) = self.expanded.get(name) {
            return ty.clone();
        }
        match contract.types.get(name) {
            None => Ty::Error,
            Some(TypeDecl::Id) | Some(TypeDecl::Opaque) => Ty::Nominal {
                port: contract.name.clone(),
                name: name.clone(),
            },
            Some(TypeDecl::Asset) => Ty::Asset,
            Some(TypeDecl::Enum { values }) => Ty::Enum(values.clone()),
            Some(TypeDecl::Record { fields }) => Ty::Record(
                fields
                    .iter()
                    .map(|(f, ty)| (f.clone(), self.from_expr(contract, ty)))
                    .collect(),
            ),
            Some(TypeDecl::Union { variants }) => Ty::Union(
                variants
                    .iter()
                    .map(|(v, fields)| {
                        (
                            v.clone(),
                            fields
                                .iter()
                                .map(|(f, ty)| (f.clone(), self.from_expr(contract, ty)))
                                .collect(),
                        )
                    })
                    .collect(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross_port_records_compare_structurally() {
        let a = Ty::Record(
            [(ident("src"), Ty::Asset), (ident("alt"), Ty::Text)]
                .into_iter()
                .collect(),
        );
        let b = Ty::Record(
            [(ident("alt"), Ty::Text), (ident("src"), Ty::Asset)]
                .into_iter()
                .collect(),
        );
        assert!(compatible(&a, &b));
    }

    #[test]
    fn option_rules() {
        assert!(compatible(&Ty::Option(Box::new(Ty::Text)), &Ty::Text));
        assert!(compatible(&Ty::Option(Box::new(Ty::Text)), &Ty::NoneLit));
        assert!(!compatible(&Ty::Text, &Ty::NoneLit));
        assert!(!compatible(&Ty::Text, &Ty::Option(Box::new(Ty::Text))));
    }

    fn ident(s: &str) -> Ident {
        Ident::new(s).unwrap()
    }
}
